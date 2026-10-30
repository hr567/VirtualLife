//! Whole-life public-command probe. Never edits state or skips a rule blocker.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};
use virtual_life_core::engine::Runtime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Standard,
    University,
    Lost,
}
impl Route {
    fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::University => "university",
            Self::Lost => "lost",
        }
    }
    fn target_stage(self) -> Option<i64> {
        match self {
            Self::Standard => None,
            Self::University => Some(4),
            Self::Lost => Some(5),
        }
    }
}
/// The public question text is matched to the shipped, unshuffled original bank.
/// The bank stores its correct answer in slot zero; no private game state is read.
fn route_exam_choice(
    snapshot: &Value,
    route: Route,
    questions: &Value,
) -> Result<Option<usize>, String> {
    if route == Route::Standard || snapshot["life"]["stage"] != 3 {
        return Ok(None);
    }
    let pending = &snapshot["pending"];
    let text = pending["text"].as_str().unwrap_or("");
    let options = pending["options"]
        .as_array()
        .ok_or("Exam choice without options")?;
    if text.starts_with("高中毕业考试：") {
        let index = options
            .iter()
            .find(|option| option["text"] == "参加考试")
            .and_then(|option| option["index"].as_u64())
            .ok_or("Graduation invitation has no public participate option")?;
        return Ok(Some(index as usize));
    }
    if !text.starts_with("第 ") || !text.contains("/10 题 · ") {
        return Ok(None);
    }
    let question = text
        .split_once('\n')
        .map(|(_, question)| question)
        .ok_or("Missing public graduation question")?;
    let record = questions
        .as_array()
        .and_then(|records| records.iter().find(|record| record["question"] == question))
        .ok_or_else(|| format!("Public question absent from original bank: {question}"))?;
    let correct = record["options"][0]
        .as_str()
        .ok_or("Original question missing its first answer")?;
    let selected = options
        .iter()
        .find(|option| {
            option["disabled"] != true
                && (option["text"].as_str() == Some(correct)) == (route == Route::University)
        })
        .ok_or("No eligible answer for requested graduation route")?;
    Ok(Some(
        selected["index"]
            .as_u64()
            .ok_or("Answer without public index")? as usize,
    ))
}

fn action(kind: &str, name: &str, index: usize) -> Value {
    json!({"type":format!("{kind}Action"),"action":name,"index":index})
}
fn tick(kind: &str) -> Value {
    json!({"type":format!("{kind}Tick"),"elapsedMs":71})
}
fn command(
    s: &Value,
    seed: usize,
    iteration: usize,
    route: Route,
    questions: &Value,
) -> Result<Value, String> {
    if s["phase"] == "playing" {
        return Ok(json!({"type":"rollDice"}));
    }
    let p = &s["pending"];
    let g = &p["state"];
    let phase = g["phase"].as_str().unwrap_or("");
    Ok(match p["kind"].as_str().unwrap_or("") {
        "dialogue" => json!({"type":"advanceEvent"}),
        "allocation" => json!({"type":"allocateStats","allocations":p["defaults"]}),
        "choice" => {
            if let Some(index) = route_exam_choice(s, route, questions)? {
                return Ok(json!({"type":"chooseOption","index":index}));
            }
            let eligible: Vec<&Value> = p["options"]
                .as_array()
                .ok_or("Choice without options")?
                .iter()
                .filter(|o| o["disabled"] != true)
                .collect();
            let selected = eligible
                .iter()
                .find(|o| o["text"].as_str().is_some_and(|s| s.contains("进入社会")))
                .copied()
                .or_else(|| {
                    eligible
                        .iter()
                        .find(|o| o["text"].as_str().is_some_and(|s| s.contains("离开")))
                        .copied()
                })
                .or_else(|| eligible.first().copied())
                .ok_or("No enabled choice")?;
            json!({"type":"chooseOption","index":selected["index"]})
        }
        "relationship" => action("relationship", "close", 0),
        "minigame" => match (p["game"].as_str().unwrap_or(""), phase) {
            ("tracking", "memorize") => action("minigame", "startShuffle", 0),
            ("tracking", "choose") => action("minigame", "choose", (seed + iteration) % 5),
            ("tracking", "roundResult") => action("minigame", "nextRound", 0),
            ("spiritSlots", "spinning") => action("minigame", "stop", 0),
            (_, "ready" | "help") => action("minigame", "start", 0),
            ("gas", "playing") => action("minigame", "throw", 0),
            _ => tick("minigame"),
        },
        "competition" => match phase {
            "menu" => action("competition", "choose", seed % 3),
            "intro" | "quizIntro" | "quizResult" => action("competition", "continue", 0),
            "quizQuestion" => action("competition", "answer", (iteration + seed) % 4),
            _ => tick("competition"),
        },
        "battle" => {
            if p["game"] == "ordinary" {
                if g["help"] == true || phase == "intro" {
                    action("battle", "continue", 0)
                } else if phase == "reward" {
                    action("battle", "finishRewardAnimation", 0)
                } else {
                    tick("battle")
                }
            } else {
                match phase {
                    "intro" | "shop" | "opening" => action("battle", "continue", 0),
                    "playing" if g["combatState"] == 0 && g["cardsMoving"] == false => {
                        let cards = g["cards"].as_array().ok_or("Final battle without cards")?;
                        // Original cards form a same-kind combination: selecting another
                        // kind clears the previous selection, so selecting all kinds
                        // would be a driver loop rather than a game defect.
                        let kind = cards
                            .iter()
                            .find(|c| c["selected"] == true)
                            .or_else(|| cards.first())
                            .map(|c| &c["kind"]);
                        if let Some((i, _)) = cards
                            .iter()
                            .enumerate()
                            .find(|(_, c)| Some(&c["kind"]) == kind && c["selected"] != true)
                        {
                            action("battle", "selectCard", i)
                        } else if !cards.is_empty() {
                            action("battle", "attack", 0)
                        } else {
                            tick("battle")
                        }
                    }
                    _ => tick("battle"),
                }
            }
        }
        "ending" => match phase {
            "summary" | "movie" => action("ending", "escape", 0),
            "credits" if g["credits"]["clockMs"].as_u64().unwrap_or(0) > 3000 => {
                action("ending", "escape", 0)
            }
            "endImage" => action("ending", "continue", 0),
            "reincarnation" => action("ending", "choose", 0),
            _ => tick("ending"),
        },
        "blocked" => return Err(format!("Original-rule blocker: {}", p["text"])),
        kind => return Err(format!("No probe driver for {kind}: {p}")),
    })
}
fn send(
    runtime: &mut Runtime,
    snapshot: &mut Value,
    mut command: Value,
) -> Result<Vec<Value>, String> {
    command["seq"] = json!(snapshot["lastSeq"].as_u64().unwrap() + 1);
    let response: Value = serde_json::from_str(&runtime.dispatch(&command.to_string())).unwrap();
    *snapshot = response["snapshot"].clone();
    if response["ok"] != true {
        return Err(format!("Command {command}: {}", response["error"]));
    }
    Ok(response["events"].as_array().cloned().unwrap_or_default())
}
/// Save only a publicly reached, idle map. The payload is the ordinary new save;
/// no state field is changed to make a scene suitable for a screenshot.
fn capture_map(
    runtime: &Runtime,
    snapshot: &Value,
    directory: &Path,
    captured: &mut BTreeSet<String>,
) -> Result<(), String> {
    if snapshot["phase"] != "playing" || !snapshot["pending"].is_null() {
        return Ok(());
    }
    let Some(id) = snapshot["mapId"].as_str() else {
        return Ok(());
    };
    if captured.contains(id) {
        return Ok(());
    }
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!("Unsafe public map identifier: {id}"));
    }
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{id}.state.json"));
    fs::write(&path, runtime.serialize())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    captured.insert(id.to_owned());
    eprintln!("captured public map {id}: {}", path.display());
    Ok(())
}
fn semantic_token(value: &str) -> String {
    let mut token = String::new();
    for character in value.chars() {
        if character.is_ascii_uppercase() {
            if !token.is_empty() {
                token.push('-');
            }
            token.push(character.to_ascii_lowercase());
        } else if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            token.push(character);
        }
    }
    token
}
/// One representative per public activity phase, not per animation frame or
/// questionnaire question. No hidden host state is inspected by the driver.
fn activity_id(snapshot: &Value) -> Option<String> {
    let native = &snapshot["nativeView"];
    if let Some(kind) = native["kind"].as_str() {
        let phase = match kind {
            "statShop" => match native["purchases"].as_u64().unwrap_or(0) {
                0 => "browse".to_owned(),
                count => format!("purchased-{count}"),
            },
            "itemShop" => if native["offers"]
                .as_array()
                .is_some_and(|offers| offers.iter().any(|offer| offer["purchased"] == true))
            {
                "purchased"
            } else {
                "browse"
            }
            .to_owned(),
            "career" => "select".to_owned(),
            "schoolExam" => if native["started"] == true {
                "question"
            } else {
                "invitation"
            }
            .to_owned(),
            "allocation" => "preview".to_owned(),
            "equipmentWorkshop" => native["outcome"]
                .as_str()
                .map(|outcome| format!("outcome-{}", semantic_token(outcome)))
                .unwrap_or_else(|| native["mode"].as_str().unwrap_or("menu").to_owned()),
            "housing" => if !native["result"].is_null() {
                "result"
            } else if native["housing"]["investmentStage"] == 0 {
                "purchase"
            } else {
                "investment"
            }
            .to_owned(),
            "fortune" => native["phase"].as_str().unwrap_or("question").to_owned(),
            _ => return None,
        };
        return Some(format!(
            "native-{}-{}",
            semantic_token(kind),
            semantic_token(&phase)
        ));
    }
    let pending = &snapshot["pending"];
    let kind = pending["kind"].as_str()?;
    if kind == "choice" {
        if native.is_null()
            && pending["options"].as_array().is_some_and(|options| {
                options.len() >= 2 && options.iter().all(|option| option["nodeId"].is_i64())
            })
        {
            return Some("event-n1-choice".into());
        }
        let text = pending["text"].as_str().unwrap_or("");
        if text.starts_with("高中毕业考试：") {
            return Some("school-exam-invitation".into());
        }
        if text.starts_with("第 ") && text.contains("/10 题 · ") {
            return Some("school-exam-question".into());
        }
    }
    let phase = pending["state"]["phase"]
        .as_str()
        .or_else(|| pending["state"]["page"].as_str())
        .or_else(|| pending["state"]["tab"].as_str());
    match kind {
        "minigame" | "battle" | "ending" | "competition" | "relationship" | "status"
        | "inventory" => {
            let mut id = semantic_token(kind);
            if let Some(game) = pending["game"].as_str() {
                id.push('-');
                id.push_str(&semantic_token(game));
            }
            if let Some(phase) = phase {
                id.push('-');
                id.push_str(&semantic_token(phase));
            }
            Some(id)
        }
        "choice" | "dialogue" | "allocation" => Some(format!("event-{kind}")),
        _ => None,
    }
}
fn capture_activity(
    runtime: &Runtime,
    snapshot: &Value,
    directory: &Path,
    captured: &mut BTreeSet<String>,
) -> Result<(), String> {
    let Some(id) = activity_id(snapshot) else {
        return Ok(());
    };
    if captured.contains(&id) {
        return Ok(());
    }
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{id}.state.json"));
    fs::write(&path, runtime.serialize())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    captured.insert(id.clone());
    eprintln!("captured public activity {id}: {}", path.display());
    Ok(())
}
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |name: &str| -> Value {
        serde_json::from_str(
            &fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap(),
        )
        .unwrap()
    };
    let mut catalog = read("maps");
    for name in ["events", "rules", "questionnaires"] {
        catalog[name] = read(name);
    }
    let args: Vec<String> = env::args().collect();
    let count = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let limit = args
        .get(2)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(300_000);
    let first_seed = args
        .get(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let resume = args.iter().skip(4).any(|s| s == "resume");
    let snapshots = args.iter().skip(4).any(|s| s == "snapshots");
    let route = if args.iter().skip(4).any(|s| s == "university") {
        Route::University
    } else if args.iter().skip(4).any(|s| s == "lost") {
        Route::Lost
    } else {
        Route::Standard
    };
    let directory = if route == Route::Standard {
        root.join("output/life-probes")
    } else {
        root.join("output/life-probes").join(route.name())
    };
    fs::create_dir_all(&directory).unwrap();
    let mut reports = Vec::new();
    let mut all_completed = true;
    let mut captured_maps = BTreeSet::new();
    let mut captured_activities = BTreeSet::new();
    for seed in first_seed..first_seed + count {
        let mut runtime = Runtime::new(&catalog.to_string()).unwrap();
        let mut s:Value=serde_json::from_str::<Value>(&runtime.dispatch(&json!({"seq":1,"type":"newGame","profile":{"name":"流程回归","sex":if seed%2==1{"male"}else{"female"},"birthMonth":1,"birthDay":1,"bloodType":"A"},"seed":seed}).to_string())).unwrap()["snapshot"].clone();
        let mut stages = BTreeSet::new();
        let mut map_ids = BTreeSet::new();
        let mut activities = BTreeSet::new();
        let mut events: BTreeMap<String, usize> = BTreeMap::new();
        let mut error = None;
        let mut completed = false;
        let mut restored = 0usize;
        let mut steps = 0usize;
        let mut previous_steps = 0u64;
        if resume {
            let save =
                fs::read_to_string(directory.join(format!("seed-{seed}.state.json"))).unwrap();
            let restored_state: Value = serde_json::from_str(&runtime.restore(&save)).unwrap();
            assert_eq!(restored_state["ok"], true, "{}", restored_state["error"]);
            s = restored_state["snapshot"].clone();
            fs::write(
                directory.join(format!("seed-{seed}.resumed.state.json")),
                save,
            )
            .unwrap();
            let old: Value = serde_json::from_str(
                &fs::read_to_string(directory.join(format!("seed-{seed}.json"))).unwrap(),
            )
            .unwrap();
            map_ids = serde_json::from_value(old["mapIds"].clone()).unwrap_or_default();
            stages = serde_json::from_value(old["stages"].clone()).unwrap();
            activities = serde_json::from_value(old["activities"].clone()).unwrap();
            events = serde_json::from_value(old["events"].clone()).unwrap();
            restored = old["saveRoundTrips"].as_u64().unwrap() as usize;
            previous_steps = old["driverSteps"].as_u64().unwrap();
        }
        for i in 0..limit {
            steps = i + 1;
            stages.insert(s["life"]["stage"].as_i64().unwrap_or(-1));
            if let Some(id) = s["mapId"].as_str() {
                map_ids.insert(id.to_owned());
            }
            if snapshots {
                if let Err(reason) =
                    capture_map(&runtime, &s, &directory.join("maps"), &mut captured_maps).and_then(
                        |_| {
                            capture_activity(
                                &runtime,
                                &s,
                                &directory.join("activities"),
                                &mut captured_activities,
                            )
                        },
                    )
                {
                    error = Some(reason);
                    break;
                }
            }
            if s["phase"] == "completed"
                || (events.contains_key("lifeCompleted") && events.contains_key("reincarnated"))
            {
                completed = true;
                break;
            }
            activities.insert(format!(
                "{}:{}",
                s["pending"]["kind"].as_str().unwrap_or("map"),
                s["pending"]["game"].as_str().unwrap_or("")
            ));
            let input = match command(
                &s,
                seed,
                i,
                route,
                &catalog["questionnaires"]["highTest"]["records"],
            ) {
                Ok(v) => v,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            };
            let timed = input["type"].as_str().unwrap().ends_with("Tick");
            let mut replies = match send(&mut runtime, &mut s, input) {
                Ok(v) => v,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            };
            if !timed {
                let kind = s["pending"]["kind"].as_str().unwrap_or("");
                if matches!(
                    kind,
                    "minigame" | "competition" | "battle" | "ending" | "relationship"
                ) {
                    let input = tick(kind);
                    match send(&mut runtime, &mut s, input) {
                        Ok(v) => replies.extend(v),
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    }
                }
            }
            for event in replies {
                if let Some(kind) = event["type"].as_str() {
                    *events.entry(kind.into()).or_default() += 1;
                }
            }
            if !timed || i % 1000 == 0 {
                let save = runtime.serialize();
                let result: Value = serde_json::from_str(&runtime.restore(&save)).unwrap();
                if result["ok"] != true {
                    error = Some(format!("Save restore failed: {}", result["error"]));
                    break;
                }
                assert_eq!(result["snapshot"], s, "Save round trip seed {seed}:{i}");
                restored += 1;
            }
            if i % 20000 == 0 {
                eprintln!(
                    "seed {seed}: step {i}, {} turns, stage {}, {}",
                    s["turn"], s["life"]["stage"], s["pending"]["game"]
                );
                fs::write(
                    directory.join(format!("seed-{seed}.progress.state.json")),
                    runtime.serialize(),
                )
                .unwrap();
            }
        }
        if !completed && error.is_none() {
            error = Some("Probe driver reached its step limit".to_string());
        }
        let route_achieved = route
            .target_stage()
            .is_none_or(|stage| stages.contains(&stage));
        if completed && !route_achieved {
            error = Some("Requested graduation route was not entered".into());
        }
        let report = json!({"route":route.name(),"routeAchieved":route_achieved,"mapIds":map_ids,"capturedMapIds":if snapshots{Some(&captured_maps)}else{None},"capturedActivityIds":if snapshots{Some(&captured_activities)}else{None},"seed":seed,"completed":completed,"resumedFromSave":resume,"commands":s["lastSeq"],"driverSteps":previous_steps+steps as u64,"turns":s["turn"],"life":s["life"],"stages":stages,"activities":activities,"events":events,"saveRoundTrips":restored,"error":error});
        fs::write(
            directory.join(format!("seed-{seed}.json")),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        if !completed {
            fs::write(
                directory.join(format!("seed-{seed}.state.json")),
                runtime.serialize(),
            )
            .unwrap();
        }
        eprintln!("{report}");
        all_completed &= completed && route_achieved;
        reports.push(report);
    }
    fs::write(
        directory.join("summary.json"),
        serde_json::to_string_pretty(&reports).unwrap(),
    )
    .unwrap();
    if !all_completed {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;
    #[test]
    fn public_activity_capture_ids_distinguish_semantic_phases_without_frame_or_question_numbers() {
        let ready = json!({"pending":{"kind":"minigame","game":"hammer","state":{"phase":"ready","frame":7}}});
        assert_eq!(
            activity_id(&ready).as_deref(),
            Some("minigame-hammer-ready")
        );
        let movie = json!({"pending":{"kind":"ending","state":{"phase":"movie","clockMs":71}}});
        assert_eq!(activity_id(&movie).as_deref(), Some("ending-movie"));
        let first = json!({"nativeView":{"kind":"fortune","phase":"question","questionIndex":0}});
        let later = json!({"nativeView":{"kind":"fortune","phase":"question","questionIndex":10}});
        assert_eq!(activity_id(&first), activity_id(&later));
        assert_eq!(
            activity_id(&first).as_deref(),
            Some("native-fortune-question")
        );
        let exam = json!({"pending":{"kind":"choice","text":"第 5/10 题 · 40 分\n原版题文"}});
        assert_eq!(activity_id(&exam).as_deref(), Some("school-exam-question"));
        let n1 = json!({"pending":{"kind":"choice","text":"请选择","options":[{"index":0,"nodeId":80},{"index":1,"nodeId":81}]}});
        assert_eq!(activity_id(&n1).as_deref(), Some("event-n1-choice"));
        let spirit = json!({"pending":{"kind":"choice","options":[{"index":0,"text":"继续"}]}});
        assert_eq!(activity_id(&spirit).as_deref(), Some("event-choice"));
        assert!(activity_id(&json!({"phase":"playing","pending":null})).is_none());
    }
    #[test]
    fn graduation_routes_select_only_visible_options_from_the_original_question_bank() {
        let bank = json!([{"question":"原版问题","options":["正确答案","错误答案甲","错误答案乙","错误答案丙"]}]);
        let prompt = json!({"life":{"stage":3},"pending":{"kind":"choice","text":"高中毕业考试：共十题，每题十分。是否参加？","options":[{"index":0,"text":"参加考试"},{"index":1,"text":"放弃考试，进入社会"}]}});
        assert_eq!(
            route_exam_choice(&prompt, Route::University, &bank).unwrap(),
            Some(0)
        );
        assert_eq!(
            route_exam_choice(&prompt, Route::Lost, &bank).unwrap(),
            Some(0)
        );
        assert_eq!(
            route_exam_choice(&prompt, Route::Standard, &bank).unwrap(),
            None
        );
        let question = json!({"life":{"stage":3},"pending":{"kind":"choice","text":"第 1/10 题 · 0 分\n原版问题","options":[{"index":0,"text":"错误答案乙"},{"index":1,"text":"错误答案甲"},{"index":2,"text":"正确答案"},{"index":3,"text":"错误答案丙"}]}});
        assert_eq!(
            route_exam_choice(&question, Route::University, &bank).unwrap(),
            Some(2)
        );
        assert_eq!(
            route_exam_choice(&question, Route::Lost, &bank).unwrap(),
            Some(0)
        );
    }
}
