//! Re-encode checked-in domain evidence for real browser file-import tests.
//! Requires test-support and is never linked into the production WASM client.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};
use virtual_life_core::{GameCommand, GameSession, Profile};

const SAVED_AT: &str = "2026-10-29T00:00:00.000Z";
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn read(path: &Path) -> Result<Value, String> {
    serde_json::from_str(
        &fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?,
    )
    .map_err(|error| format!("{}: {error}", path.display()))
}
fn write_scenario(
    directory: &Path,
    id: &str,
    session: &mut GameSession,
    payload: String,
    source: &str,
    source_hash: Option<String>,
) -> Result<Value, String> {
    let verified = session.restore(&payload);
    if !verified.ok {
        return Err(format!("{id}: fresh restore failed: {:?}", verified.error));
    }
    if session.save() != payload {
        return Err(format!("{id}: fresh save is not canonically stable"));
    }
    let envelope = json!({"format":"virtual-life-bevy-save","version":1,"savedAt":SAVED_AT,"contentId":session.content_identity(),"payload":payload,"checksum":sha256(payload.as_bytes())});
    let file = format!("{id}.json");
    fs::write(
        directory.join(&file),
        serde_json::to_vec_pretty(&envelope).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let snapshot = session.snapshot();
    let pending = snapshot.pending.as_ref().unwrap_or(&Value::Null);
    let summary = json!({"id":id,"file":file,"source":source,"sourceSha256":source_hash,"contentId":session.content_identity(),"phase":snapshot.phase,"mapId":snapshot.map_id,"pendingKind":pending["kind"],"game":pending["game"],"nativeKind":snapshot.native_view.as_ref().map(|view|&view["kind"]),"statePhase":pending["state"]["phase"],"lastSeq":snapshot.last_seq,"turn":snapshot.turn});
    eprintln!(
        "exported {id}: phase={} kind={} game={}",
        snapshot.phase, pending["kind"], pending["game"]
    );
    Ok(summary)
}
/// Optional maps and activities captured by complete_life's public commands. They already use
/// the production save format; unlike original evidence, these are never decoded
/// through the test-only raw-state conversion helper.
fn export_public_captures(
    root: &Path,
    directory: &Path,
    content: &Value,
    category: &str,
) -> Result<Vec<Value>, String> {
    let mut summaries = Vec::new();
    let mut exported = BTreeSet::new();
    for route in ["", "university", "lost"] {
        let map_directory = root.join("output/life-probes").join(route).join(category);
        if !map_directory.exists() {
            continue;
        }
        let mut paths = fs::read_dir(&map_directory)
            .map_err(|error| format!("{}: {error}", map_directory.display()))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            let Some(id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".state.json"))
            else {
                continue;
            };
            let payload = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let mut session = GameSession::from_content(content.clone())?;
            let restored = session.restore(&payload);
            if !restored.ok {
                return Err(format!(
                    "{}: invalid public save: {:?}",
                    path.display(),
                    restored.error
                ));
            }
            let snapshot = session.snapshot();
            if category == "maps" {
                if snapshot.phase != "playing"
                    || snapshot.pending.is_some()
                    || snapshot.map_id.as_deref() != Some(id)
                {
                    return Err(format!(
                        "{}: expected idle playing map {id}",
                        path.display()
                    ));
                }
            } else if snapshot.pending.is_none() {
                return Err(format!(
                    "{}: expected a pending public activity",
                    path.display()
                ));
            }
            if session.save() != payload {
                return Err(format!(
                    "{}: public save is not canonically stable",
                    path.display()
                ));
            }
            // Validate every source, but export one deterministic state per id.
            if !exported.insert(id.to_owned()) {
                continue;
            }
            let source = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let source_hash = sha256(payload.as_bytes());
            summaries.push(write_scenario(
                directory,
                &format!(
                    "{}-{id}",
                    if category == "maps" {
                        "map"
                    } else {
                        "activity"
                    }
                ),
                &mut session,
                payload,
                &source,
                Some(source_hash),
            )?);
        }
    }
    Ok(summaries)
}
fn run() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("output/bevy-scenarios"));
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let mut content = read(&root.join("assets/catalog/maps.json"))?;
    for name in ["events", "rules", "questionnaires"] {
        content[name] = read(&root.join(format!("assets/catalog/{name}.json")))?;
    }
    let mut paths = fs::read_dir(root.join("tests/fixtures"))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("browser-") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut summaries = Vec::new();
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("Invalid fixture filename")?;
        let id = name
            .strip_prefix("browser-")
            .ok_or("Invalid evidence filename")?;
        let evidence = read(&path)?;
        let state = evidence
            .get("state")
            .ok_or_else(|| format!("{} has no domain state", path.display()))?
            .clone();
        let mut session = GameSession::from_content(content.clone())?;
        let payload = session
            .save_test_state(state)
            .map_err(|error| format!("{id}: {error}"))?;
        let source = format!("tests/fixtures/{name}.json");
        let source_hash = sha256(&fs::read(&path).map_err(|error| error.to_string())?);
        summaries.push(write_scenario(
            &directory,
            id,
            &mut session,
            payload,
            &source,
            Some(source_hash),
        )?);
    }
    // Public-command seeds provide basic map entry independent of converted evidence.
    for seed in [1, 4] {
        let mut session = GameSession::from_content(content.clone())?;
        let response = session.send(GameCommand::NewGame {
            seq: 0,
            profile: Profile {
                name: format!("原版回归{seed}"),
                sex: if seed == 1 { "male" } else { "female" }.into(),
                birth_month: 1,
                birth_day: 1,
                blood_type: "A".into(),
            },
            seed,
        });
        if !response.ok {
            return Err(format!("seed {seed}: {:?}", response.error));
        }
        let payload = session.save();
        summaries.push(write_scenario(
            &directory,
            &format!("newgame-seed-{seed}"),
            &mut session,
            payload,
            "public commands: NewGame",
            None,
        )?);
        let response = session.send(GameCommand::OpenStatus { seq: 0 });
        if !response.ok {
            return Err(format!("seed {seed} open status: {:?}", response.error));
        }
        let response = session.send(GameCommand::StatusAction {
            seq: 0,
            action: "close".into(),
            page: None,
            delta: None,
            index: None,
        });
        if !response.ok {
            return Err(format!("seed {seed} close status: {:?}", response.error));
        }
        if session.snapshot().pending.is_some() || session.snapshot().phase != "playing" {
            return Err(format!(
                "seed {seed}: public commands did not return to the map"
            ));
        }
        let payload = session.save();
        summaries.push(write_scenario(
            &directory,
            &format!("map-seed-{seed}"),
            &mut session,
            payload,
            "public commands: NewGame, OpenStatus, StatusAction(close)",
            None,
        )?);
    }
    summaries.extend(export_public_captures(&root, &directory, &content, "maps")?);
    summaries.extend(export_public_captures(
        &root,
        &directory,
        &content,
        "activities",
    )?);
    let index = json!({"format":"virtual-life-bevy-browser-scenarios","version":1,"savedAt":SAVED_AT,"scenarios":summaries});
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&index).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "Validated and exported {} fresh-save browser scenarios to {}",
        index["scenarios"].as_array().unwrap().len(),
        directory.display()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Scenario export failed: {error}");
        std::process::exit(1);
    }
}
