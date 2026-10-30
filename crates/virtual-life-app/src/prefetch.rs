//! Ordered compressed-resource predictions. Decoding and GPU uploads stay on demand.
use crate::scene::Screen;
use serde_json::Value;
use std::collections::HashSet;

struct Planner<'a> {
    manifest: &'a Value,
    maps: &'a Value,
    rules: &'a Value,
    seen: HashSet<String>,
    ordered: Vec<String>,
}
impl Planner<'_> {
    fn add(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        let normalized = path.replace('\\', "/").to_lowercase();
        let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
        for candidate in [
            normalized.to_owned(),
            format!("install/{normalized}"),
            format!("cd/data/{normalized}"),
            format!("cd/{normalized}"),
        ] {
            if self.manifest["resources"].get(&candidate).is_some() {
                if self.seen.insert(candidate.clone()) {
                    self.ordered.push(candidate)
                }
                return;
            }
        }
    }
    fn collect(&mut self, value: &Value) {
        match value {
            Value::String(value) if value.contains('/') || value.contains('\\') => self.add(value),
            Value::Array(values) => {
                for value in values {
                    self.collect(value)
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    self.collect(value)
                }
            }
            _ => {}
        }
    }
    fn group(&mut self, prefix: &str) {
        let manifest = self.manifest;
        if let Some(resources) = manifest["resources"].as_object() {
            for key in resources.keys() {
                if key.starts_with(prefix) {
                    self.add(key)
                }
            }
        }
    }
    fn map(&mut self, id: &str) {
        let maps = self.maps;
        if let Some(map) = maps["maps"]
            .as_array()
            .and_then(|maps| maps.iter().find(|map| map["id"] == id))
        {
            self.map_art(map)
        }
    }
    fn map_art(&mut self, map: &Value) {
        if let Some(tiles) = map["backgroundTiles"].as_array() {
            for tile in tiles {
                self.add(tile["resourceId"].as_str().unwrap_or(""));
            }
        }
        let source = map["sourceResource"].as_str().unwrap_or("");
        let directory = source.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        if let Some(sections) = map["legacySections"].as_object() {
            for (name, fields) in sections {
                if ["Spirit_", "Floater_"].iter().any(|prefix| {
                    name.strip_prefix(*prefix).is_some_and(|index| {
                        !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit())
                    })
                }) {
                    if let Some(file) = fields["File"].as_str() {
                        self.add(&format!(
                            "{directory}/{}",
                            file.replace('\\', "/").to_lowercase()
                        ));
                    }
                }
            }
        }
    }
    fn character(&mut self, index: usize) {
        let rules = self.rules;
        let characters = &rules["characters"];
        if let Some(directory) = characters["directories"][index].as_str() {
            let model = directory
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("");
            if let Some(suffixes) = characters["animationSuffixes"].as_array() {
                for suffix in suffixes.iter().filter_map(Value::as_str) {
                    let suffix = if suffix
                        .get(..3)
                        .is_some_and(|p| p.eq_ignore_ascii_case("m00"))
                    {
                        format!("{model}{}", &suffix[3..])
                    } else {
                        suffix.to_owned()
                    };
                    self.add(&format!("{directory}{suffix}"));
                }
            }
        }
        if let Some(portrait) = characters["portraits"][index].as_str() {
            self.add(portrait)
        }
    }
}
fn number(v: &Value, key: &str, default: i64) -> i64 {
    v[key].as_i64().unwrap_or(default)
}
fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn stage<'a>(rules: &'a Value, id: i64) -> Option<&'a Value> {
    rules["stages"]
        .as_array()?
        .iter()
        .find(|value| value["id"] == id)
}

/// Returns only resolvable manifest resource IDs, in likely-use order with no duplicates.
/// The browser service controls foreground priority, two background requests and pause conditions.
pub fn plan(
    manifest: &Value,
    maps: &Value,
    rules: &Value,
    snapshot: &Value,
    screen: Screen,
) -> Vec<String> {
    let mut p = Planner {
        manifest,
        maps,
        rules,
        seen: HashSet::new(),
        ordered: Vec::new(),
    };
    p.add("install/bmp/pm0301.bmp");
    if matches!(screen, Screen::Menu | Screen::Profile) {
        p.map("park");
    }
    let pending = &snapshot["pending"];
    let state = &pending["state"];
    p.collect(&snapshot["scene"]);
    p.collect(state);
    match string(pending, "kind") {
        "minigame" => {
            let directory = match string(pending, "game") {
                "tracking" => "gbs",
                "hammer" => "hammer",
                "fist" => "fistgame",
                "gas" => "gas",
                "fire" => "fire",
                "firebee" => "firebee",
                "spiritSlots" => "spirit",
                _ => "",
            };
            if !directory.is_empty() {
                p.group(&format!("install/{directory}/"));
            }
            p.group("install/public/bonus/");
            p.group("install/sound/gb/");
        }
        "battle" if string(pending, "game") == "ordinary" => {
            let r = &rules["battle"]["rendering"];
            let input = &state["input"];
            let sex = number(input, "sex", 0).max(0) as usize;
            let age = number(input, "stage", 0);
            let career = number(input, "career", -1);
            let portrait = if age < 2 {
                25
            } else if age < 4 {
                26
            } else if age == 4 {
                27
            } else if career < 0 {
                28
            } else {
                career
            } as usize;
            let background = number(state, "background", 0).max(0) as usize;
            let enemy = number(state, "enemy", 0).max(0) as usize;
            let player = number(state, "playerClass", 0).max(0) as usize;
            for value in [
                &r["backgrounds"][background],
                &r["backgroundOverlays"][background],
                &r["versusDoors"],
                &r["versusEnemyPortraits"][enemy],
                &r["versusPlayerPortraits"][sex][portrait],
                &r["actors"][sex][player],
                &r["enemyActors"][enemy],
            ] {
                p.collect(value)
            }
            for key in [
                "panels",
                "cards",
                "selectedCards",
                "timeDigits",
                "healthBar",
                "hpDigits",
                "attackButton",
                "words",
                "bonus",
                "sounds",
            ] {
                p.collect(&r[key]);
            }
        }
        "battle" if string(pending, "game") == "final" => {
            p.collect(&rules["finalBattle"]["openingVideo"]);
            p.collect(&rules["finalBattle"]["openingAudio"]);
            p.collect(&rules["finalBattle"]);
        }
        "ending" => {
            p.collect(&rules["endingSummary"]["music"]);
            let epilogue = &rules["epilogue"];
            for key in ["movie", "outro", "lossOutro", "music", "credits", "choice"] {
                p.collect(&epilogue[key]);
            }
            p.collect(
                &epilogue["narration"][usize::from(state["victory"].as_bool().unwrap_or(false))],
            );
        }
        "competition" => p.collect(&rules["competition"]),
        "relationship" => {
            p.group("install/pm0l/");
            p.add("install/public/button/exita.flc");
            p.add("sound/music/n2love.wav");
            p.collect(&rules["relationships"]);
        }
        "inventory" => {
            p.collect(&rules["inventory"]["rendering"]);
            if let Some(entries) = state["entries"].as_array() {
                for entry in entries {
                    let id = number(entry, "id", -1);
                    if id >= 0 {
                        p.collect(
                            &rules["inventory"][string(state, "tab")][id as usize]["animation"],
                        );
                    }
                }
            }
        }
        "status" => match string(state, "page") {
            "house" => {
                let housing = &pending["housing"];
                if let Some(styles) = rules["statusRendering"]["house"]["styles"].as_array() {
                    if let Some(style) = styles
                        .iter()
                        .find(|style| style["style"] == housing["style"])
                    {
                        p.collect(style)
                    }
                }
                p.collect(&rules["statusRendering"]["house"]["sound"]);
            }
            "investment" | "skills" | "careerHistory" => {
                p.collect(&rules["statusRendering"][string(state, "page")])
            }
            _ => {}
        },
        _ => {}
    }
    p.map(snapshot["mapId"].as_str().unwrap_or("park"));
    let life = &snapshot["life"];
    let current_stage = number(life, "stage", 2);
    let current = stage(rules, current_stage);
    if let Some(current) = current {
        p.add(string(current, "musicPath"));
    }
    let female = if string(&snapshot["player"], "sex") == "female" {
        31
    } else {
        0
    };
    let profession = number(life, "profession", -1);
    let character =
        (female + current_stage + if profession == -1 { 0 } else { profession }).max(0) as usize;
    p.character(character);
    if snapshot["player"].is_null() {
        p.character((31 + current_stage).max(0) as usize);
    }
    if !snapshot["spirits"].is_null()
        && current_stage != 2
        && !life["traveling"].as_bool().unwrap_or(false)
    {
        if let Some(age) = [1, 1, 1, 2, 2, 4, 3].get(current_stage.max(0) as usize) {
            for spirit in ["xr", "dl", "sa", "fd", "yb"] {
                for direction in ["3a", "3b", "3c", "3d"] {
                    p.add(&format!("spirit/{age}{spirit}{direction}.flc"));
                }
            }
        }
    }
    for index in 1..=5 {
        p.add(&format!("install/bmp/pm020{index}.bmp"));
    }
    p.group("install/bmp/pm06a");
    p.group("install/ani/pm06a");
    for index in 1..=8 {
        p.add(&format!("install/bmp/zhan0{index}.bmp"));
    }
    for path in [
        "install/bmp/frame.bmp",
        "install/bmp/talk.bmp",
        "install/ani/3.flc",
    ] {
        p.add(path)
    }
    p.collect(&rules["inventory"]["rendering"]);
    p.collect(&rules["optionsRendering"]);
    let status = &rules["statusRendering"];
    let portrait = number(
        &snapshot["statusOverview"],
        "portraitIndex",
        character as i64,
    )
    .max(0) as usize;
    for key in [
        "background",
        "hpLabel",
        "hpFrame",
        "hpFill",
        "back",
        "buttons",
        "disabledButtons",
    ] {
        p.collect(&status[key]);
    }
    p.collect(&status["portraits"][portrait]);
    let variant = status["hpGdi"]["portraitVariants"][portrait]
        .as_u64()
        .unwrap_or(0) as usize;
    p.collect(&status["hpGdi"]["variants"][variant]);
    if let Some(current) = current {
        let turns = number(current, "stageTurns", 0);
        if !life.is_null() && turns > 0 && turns - number(life, "stageTurn", 0) <= 5 {
            let next: &[i64] = match current_stage {
                0 => &[1],
                1 => &[2],
                2 => &[3],
                3 => &[4, 5, 6],
                4 => &[6],
                _ => &[],
            };
            for &id in next {
                if let Some(next) = stage(rules, id) {
                    let source = format!("cd/data/{}.mmi", string(next, "mapPath"));
                    if let Some(next_map) = maps["maps"]
                        .as_array()
                        .and_then(|maps| maps.iter().find(|map| map["sourceResource"] == source))
                    {
                        p.map_art(next_map)
                    }
                    p.add(string(next, "musicPath"));
                    p.character((female + id).max(0) as usize);
                }
            }
        }
    }
    // These six compressed game groups are small; large battles and films stay conditional.
    for directory in [
        "gbs",
        "hammer",
        "fistgame",
        "gas",
        "fire",
        "firebee",
        "public/bonus",
    ] {
        p.group(&format!("install/{directory}/"));
    }
    p.group("install/sound/gb/");
    p.ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn catalog() -> (Value, Value, Value) {
        let mut manifest = json!({"resources":{}});
        for resource in [
            "install/bmp/pm0301.bmp",
            "cd/data/map/park.bmp",
            "cd/data/map/school.bmp",
            "cd/data/map/study.bmp",
            "cd/data/map/adult.bmp",
            "cd/data/map/lost.bmp",
            "cd/data/map/cloud.flc",
            "install/scene/current.bmp",
            "install/sound/current.wav",
            "install/sound/next.wav",
            "install/unused/large-ending.webm",
            "install/fire/help.bmp",
        ] {
            manifest["resources"][resource] = json!({"sha256":resource});
        }
        let maps = json!({"maps":[{"id":"park","sourceResource":"cd/data/map/park.mmi","backgroundTiles":[{"resourceId":"cd/data/map/park.bmp"}],"legacySections":{"Spirit_0":{"File":"CLOUD.FLC"}}},{"id":"school","sourceResource":"cd/data/map/school.mmi","backgroundTiles":[{"resourceId":"cd/data/map/school.bmp"}]},{"id":"study","sourceResource":"cd/data/map/study.mmi","backgroundTiles":[{"resourceId":"cd/data/map/study.bmp"}]},{"id":"lost","sourceResource":"cd/data/map/lost.mmi","backgroundTiles":[{"resourceId":"cd/data/map/lost.bmp"}]},{"id":"adult","sourceResource":"cd/data/map/adult.mmi","backgroundTiles":[{"resourceId":"cd/data/map/adult.bmp"}]}]});
        let rules = json!({"stages":[{"id":3,"mapPath":"map/school","musicPath":"sound/current.wav","stageTurns":100},{"id":4,"mapPath":"map/study","musicPath":"sound/next.wav"},{"id":5,"mapPath":"map/lost"},{"id":6,"mapPath":"map/adult"}]});
        (manifest, maps, rules)
    }
    #[test]
    fn scene_then_current_map_then_graduation_targets_are_ordered_and_deduplicated() {
        let (manifest, maps, rules) = catalog();
        let snapshot = json!({"scene":{"backgroundResource":"scene\\current.bmp","duplicate":"install/scene/current.bmp","text":"not a downloadable/path"},"mapId":"school","player":{"sex":"male"},"life":{"stage":3,"stageTurn":95}});
        let resources = plan(&manifest, &maps, &rules, &snapshot, Screen::Game);
        assert_eq!(
            &resources[..3],
            [
                "install/bmp/pm0301.bmp",
                "install/scene/current.bmp",
                "cd/data/map/school.bmp"
            ]
        );
        for next in ["study", "lost", "adult"] {
            assert!(resources.contains(&format!("cd/data/map/{next}.bmp")));
        }
        assert!(resources.contains(&"install/sound/current.wav".into()));
        assert!(resources.contains(&"install/sound/next.wav".into()));
        assert_eq!(
            resources
                .iter()
                .filter(|v| *v == "install/scene/current.bmp")
                .count(),
            1
        );
        assert!(!resources.iter().any(|v| v.contains("large-ending")));
    }
    #[test]
    fn menu_predicts_initial_map_decoration_while_distant_graduation_stays_unloaded() {
        let (manifest, maps, rules) = catalog();
        let snapshot =
            json!({"mapId":"school","player":{"sex":"female"},"life":{"stage":3,"stageTurn":20}});
        let resources = plan(&manifest, &maps, &rules, &snapshot, Screen::Menu);
        assert_eq!(resources[1], "cd/data/map/park.bmp");
        assert!(resources.contains(&"cd/data/map/cloud.flc".into()));
        assert!(!resources.contains(&"cd/data/map/study.bmp".into()));
    }
    #[test]
    fn large_ending_movie_is_predicted_only_after_entering_that_flow() {
        let (mut manifest, maps, mut rules) = catalog();
        manifest["resources"]["install/movie/ending.webm"] = json!({});
        rules["epilogue"] = json!({"movie":{"resource":"movie/ending.webm"}});
        let inactive = plan(&manifest, &maps, &rules, &json!({}), Screen::Menu);
        assert!(!inactive.contains(&"install/movie/ending.webm".into()));
        let active = plan(
            &manifest,
            &maps,
            &rules,
            &json!({"pending":{"kind":"ending","state":{"victory":true}}}),
            Screen::Game,
        );
        assert!(active.contains(&"install/movie/ending.webm".into()));
    }
}
