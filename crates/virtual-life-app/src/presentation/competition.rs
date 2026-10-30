//! Original student competition scenes, driven by the rules' explicit frame lists.
use super::*;

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let state = &snapshot["pending"]["state"];
    let phase = s(state, "/phase");
    let rules = &c.rules["competition"];
    let rendering = &rules["rendering"];
    let black = phase == "rankPrize" || phase == "running" && b(state, "/running/clearedFrame");
    if phase == "arrival" {
        super::map::compose(f, c, snapshot, ui);
        // Arrival overlays the currently displayed map. Its old controls cannot remain active.
        f.controls.clear();
    } else if !black {
        let background = if phase == "intro" {
            rules["rendering"]["introBackgrounds"][n(state, "/sex").clamp(0, 1) as usize]
                [(n(state, "/introPage") - 1).clamp(0, 3) as usize]
                .as_str()
                .unwrap_or("")
        } else {
            s(rendering, "/background/resource")
        };
        let index = sprite(f, "competition-background", background, 0, 0, 0);
        f.sprites[index].transparent = Transparency::Opaque;
    }
    f.tick = Some(json!({"type":"competitionTick"}));
    if phase == "menu" {
        menu(f, rules, state, snapshot, ui);
    } else if [
        "intro",
        "beauty",
        "running",
        "quizIntro",
        "quizQuestion",
        "quizFeedback",
        "quizResult",
    ]
    .contains(&phase)
        && !black
    {
        bitmap(f, "competition-talk", &rendering["talkBackground"], 1);
        if phase != "running" {
            let host = &rendering["hostAnimation"];
            if let Some(index) = animation(f, "competition-host", host, 2) {
                f.sprites[index].playback = Some(Playback {
                    interval_ms: n(host, "/interval").max(1) as u32,
                    max_advances: 1,
                    ..Default::default()
                });
            }
        }
        textbox(
            f,
            "competition-commentary",
            s(snapshot, "/pending/text"),
            &rendering["talkText"],
        );
    }
    if phase == "moneyPrize" {
        let mut opening = rules["openingAnimation"].clone();
        opening["frame"] = json!((n(&opening, "/frameCount") - 1).max(0));
        animation(f, "competition-opening", &opening, 19);
    }
    animation(f, "competition-main", &state["animation"], 20);
    match phase {
        "beauty" => beauty(f, rules, state),
        "running" => running(f, rules, state),
        "quizQuestion" | "quizFeedback" | "quizResult" => quiz(f, rules, state, ui),
        _ => {}
    }
    if ["intro", "quizIntro", "quizResult"].contains(&phase) {
        control(
            f,
            "competition-continue",
            "继续比赛",
            Rect::new(0, 0, 640, 480),
            game("competitionAction", "continue", None),
            &["Enter"],
        );
    }
    audio(f, rules, state);
}

fn bitmap(f: &mut SceneFrame, id: &str, meta: &Value, z: i32) {
    if let Some(index) = image(f, id, meta, z) {
        f.sprites[index].transparent = Transparency::Index(1);
    }
}
fn positioned_bitmap(f: &mut SceneFrame, id: &str, resource: &str, x: i32, y: i32, z: i32) {
    let index = sprite(f, id, resource, x, y, z);
    f.sprites[index].transparent = Transparency::Index(1);
}
fn pair(value: &Value) -> (i32, i32) {
    (n(value, "/0"), n(value, "/1"))
}
fn color(value: &str) -> [u8; 4] {
    if value.len() == 7 && value.starts_with('#') {
        let mut rgba = [0, 0, 0, 255];
        for (i, start) in [1, 3, 5].into_iter().enumerate() {
            rgba[i] = u8::from_str_radix(&value[start..start + 2], 16).unwrap_or(0);
        }
        rgba
    } else {
        [10, 10, 10, 255]
    }
}
fn textbox(f: &mut SceneFrame, id: &str, value: &str, area: &Value) {
    text(
        f,
        id,
        value,
        Rect::new(
            n(area, "/x"),
            n(area, "/y"),
            n(area, "/width").max(0) as u32,
            n(area, "/height").max(0) as u32,
        ),
        color(s(area, "/color")),
    );
    let item = f.texts.last_mut().unwrap();
    item.line_height = n(area, "/fontSize").max(1) as u32;
    item.font = TextFont::Gdi {
        size: item.line_height,
        bold: true,
    };
    item.clip = Some(item.bounds);
}
fn label(
    f: &mut SceneFrame,
    id: &str,
    value: impl Into<String>,
    x: i32,
    y: i32,
    size: u32,
    white: bool,
) {
    text(
        f,
        id,
        value,
        Rect::new(x, y, 150, size),
        if white { [255; 4] } else { [10, 10, 10, 255] },
    );
    let item = f.texts.last_mut().unwrap();
    item.line_height = size;
    item.font = TextFont::Gdi { size, bold: true };
}

fn menu(
    f: &mut SceneFrame,
    rules: &Value,
    state: &Value,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let r = &rules["rendering"];
    bitmap(f, "competition-menu-title", &r["menuTitle"], 10);
    bitmap(f, "competition-menu-host", &r["menuHost"], 11);
    let mut commentary = s(snapshot, "/pending/text");
    for (index, meta) in a(r, "/menuButtons").iter().enumerate() {
        let id = format!("competition-menu-{index}");
        let width = n(meta, "/width").max(0) as u32;
        let height = n(meta, "/height").max(0) as u32;
        let frame = if ui.pressed.as_deref() == Some(&id) {
            2
        } else if ui.hover.as_deref() == Some(&id) {
            1
        } else {
            0
        };
        let k = sprite(
            f,
            &id,
            s(meta, "/resource"),
            n(meta, "/x"),
            n(meta, "/y"),
            12,
        );
        f.sprites[k].source = Some(Rect::new(0, frame * height as i32, width, height));
        f.sprites[k].width = Some(width);
        f.sprites[k].height = Some(height);
        control(
            f,
            &id,
            ["造型大赛", "飞人大赛", "智力大赛"][index.min(2)],
            Rect::new(n(meta, "/x"), n(meta, "/y"), width, height),
            game("competitionAction", "choose", Some(index)),
            &[],
        );
        if ui.hover.as_deref() == Some(&id) {
            commentary = rules["menu"][(n(state, "/stage") - 2).clamp(0, 2) as usize][index + 1]
                .as_str()
                .unwrap_or(commentary);
        }
    }
    textbox(f, "competition-commentary", commentary, &r["menuText"]);
}

fn portrait_panels(f: &mut SceneFrame, rules: &Value, state: &Value, running: bool) {
    let r = &rules["rendering"];
    let resources = if running {
        &rules["running"]["rendering"]
    } else {
        r
    };
    let sex = n(state, "/sex").clamp(0, 1) as usize;
    let stage = (n(state, "/stage") - 2).clamp(0, 2) as usize;
    let contest = if running {
        &state["running"]
    } else {
        &state["beauty"]
    };
    for index in 0..4 {
        let active = if running {
            n(contest, "/displayedContestant") == index as i32
        } else {
            n(contest, "/contestant") == index as i32
                && ["walkIn", "pose", "walkOut", "afterWalk"].contains(&s(contest, "/phase"))
        };
        let variants = if index == 0 {
            &resources[if running {
                "playerPortraits"
            } else {
                "beautyPlayerPortraits"
            }][sex][stage]
        } else {
            &resources[if running {
                "otherPortraits"
            } else {
                "beautyOtherPortraits"
            }][index - 1]
        };
        let resource = variants[usize::from(!active)].as_str().unwrap_or("");
        let (x, y) = pair(&r["portraitPositions"][index]);
        positioned_bitmap(
            f,
            &format!("competition-portrait-{index}"),
            resource,
            x,
            y,
            21,
        );
        let (x, y) = pair(&r["scorePanelPositions"][index]);
        positioned_bitmap(
            f,
            &format!("competition-panel-{index}"),
            s(r, "/scorePanelResource"),
            x,
            y,
            22,
        );
        let (x, y) = pair(&r["scoreNamePositions"][index]);
        let (sx, sy) = pair(&r["scoreValuePositions"][index]);
        label(
            f,
            &format!("competition-name-label-{index}"),
            resources["scoreLabels"][0].as_str().unwrap_or(""),
            x - 40,
            y,
            12,
            false,
        );
        label(
            f,
            &format!("competition-score-label-{index}"),
            resources["scoreLabels"][1].as_str().unwrap_or(""),
            sx - 40,
            sy,
            12,
            false,
        );
        let name = if index == 0 {
            s(state, "/name")
        } else {
            rules[if running {
                "runningOpponents"
            } else {
                "beautyOpponents"
            }][sex][index - 1]
                .as_str()
                .unwrap_or("")
        };
        label(
            f,
            &format!("competition-name-{index}"),
            name,
            x,
            y,
            12,
            false,
        );
        let scored = if running {
            (index as i32) < n(contest, "/displayedContestant")
        } else {
            (index as i32) < n(contest, "/contestant")
                || ["scores", "podium", "finished"].contains(&s(contest, "/phase"))
                    && index as i32 == n(contest, "/contestant")
        };
        if scored {
            let score = if running {
                let value = n(contest, &format!("/displayedTimes/{index}"));
                format!("{:02}'{}", value / 10, value % 10)
            } else {
                n(contest, &format!("/totals/{index}")).to_string()
            };
            label(
                f,
                &format!("competition-score-{index}"),
                score,
                sx,
                sy,
                12,
                false,
            );
        }
    }
}
fn podium(f: &mut SceneFrame, rules: &Value, state: &Value, running: bool) {
    let contest = if running {
        &state["running"]
    } else {
        &state["beauty"]
    };
    if s(contest, "/phase") != "podium" {
        return;
    }
    let sex = n(state, "/sex").clamp(0, 1) as usize;
    let first = (if running { 4 } else { 3 } - n(contest, "/podiumPlace")).clamp(0, 3) as usize;
    for place in first..3 {
        let winner = n(contest, &format!("/winners/{place}")).clamp(0, 3) as usize;
        let name = if winner == 0 {
            s(state, "/name")
        } else {
            rules[if running {
                "runningOpponents"
            } else {
                "beautyOpponents"
            }][sex][winner - 1]
                .as_str()
                .unwrap_or("")
        };
        let (x, y) = pair(&rules["rendering"]["podiumNamePositions"][place]);
        label(
            f,
            &format!("competition-podium-name-{place}"),
            name,
            x,
            y,
            16,
            true,
        );
    }
}
fn beauty(f: &mut SceneFrame, rules: &Value, state: &Value) {
    portrait_panels(f, rules, state, false);
    let contest = &state["beauty"];
    for (index, layer) in a(contest, "/layers").iter().enumerate() {
        animation(
            f,
            &format!("competition-beauty-{index}"),
            layer,
            30 + index as i32,
        );
    }
    if s(contest, "/phase") == "scores" {
        let contestant = n(contest, "/contestant").clamp(0, 3) as usize;
        for judge in 0..3 {
            let (x, y) = pair(&rules["rendering"]["judgeScorePositions"][judge]);
            label(
                f,
                &format!("competition-judge-score-{judge}"),
                contest["scores"][contestant][judge].to_string(),
                x,
                y,
                16,
                false,
            );
        }
    }
    podium(f, rules, state, false);
}
fn running(f: &mut SceneFrame, rules: &Value, state: &Value) {
    let contest = &state["running"];
    let cleared = b(contest, "/clearedFrame");
    let panels = b(contest, "/showScorePanels");
    if panels && !cleared {
        portrait_panels(f, rules, state, true);
    }
    for (index, layer) in a(contest, "/layers").iter().enumerate() {
        let early = if cleared {
            0
        } else if panels {
            2
        } else {
            1
        };
        let z = if index < early {
            index as i32 + 2
        } else {
            index as i32 + 30
        };
        animation(f, &format!("competition-running-{index}"), layer, z);
    }
    podium(f, rules, state, true);
    if n(contest, "/contestant") == 0 && ["preparing", "running"].contains(&s(contest, "/phase")) {
        for (index, (step, key, label)) in [
            ("left", "ArrowLeft", "左步加速"),
            ("right", "ArrowRight", "右步加速"),
        ]
        .into_iter()
        .enumerate()
        {
            let action = game("competitionAction", step, None);
            control(
                f,
                &format!("competition-run-{step}"),
                label,
                Rect::new(index as i32 * 320, 0, 320, 480),
                action.clone(),
                &[key],
            );
            f.controls.last_mut().unwrap().on_press = Some(action);
        }
    }
}

fn original_width(value: &str) -> u32 {
    value
        .chars()
        .map(|ch| if ch.is_ascii() { 8 } else { 16 })
        .sum()
}
fn wrapped_height(value: &str, width: u32) -> u32 {
    let mut lines = 1;
    let mut x = 0;
    for ch in value.chars() {
        if ch == '\n' {
            lines += 1;
            x = 0;
            continue;
        }
        let cw = if ch.is_ascii() { 8 } else { 16 };
        if x + cw > width && x > 0 {
            lines += 1;
            x = 0;
        }
        x += cw;
    }
    lines * 16
}
fn choose_answer(index: usize) -> Action {
    Action::Ui(UiAction::SetField {
        field: "_competitionAnswer".into(),
        value: json!(index),
    })
}
fn quiz(f: &mut SceneFrame, rules: &Value, state: &Value, ui: &PresentationState) {
    let r = &rules["rendering"];
    let phase = s(state, "/phase");
    bitmap(f, "competition-scorebar", &r["quizScoreBackground"], 25);
    positioned_bitmap(
        f,
        "competition-scoreicon",
        r["quizScoreResources"][0].as_str().unwrap_or(""),
        n(r, "/quizScorePosition/x"),
        n(r, "/quizScorePosition/baseY")
            + n(state, "/exam/score") / 10 * n(r, "/quizScorePosition/stepY"),
        26,
    );
    if phase == "quizFeedback" {
        animation(
            f,
            "competition-feedback",
            &state["feedback"][usize::from(b(state, "/lastCorrect"))],
            27,
        );
    }
    if !["quizQuestion", "quizFeedback"].contains(&phase) {
        return;
    }
    let question = if phase == "quizFeedback" && !state["answeredQuestion"].is_null() {
        &state["answeredQuestion"]
    } else {
        &state["exam"]["current"]
    };
    let area = &r["quizText"];
    let x = n(area, "/x");
    let top = n(area, "/y");
    let width = n(area, "/width").max(1) as u32;
    let mut y = top;
    let prompt = s(question, "/question");
    let prompt_height = wrapped_height(prompt, width);
    text(
        f,
        "competition-question",
        prompt,
        Rect::new(x, y, width, prompt_height),
        [255; 4],
    );
    y += prompt_height as i32 + 16;
    let selected = ui.profile["_competitionAnswer"]
        .as_u64()
        .unwrap_or(0)
        .min(3) as usize;
    for (index, answer) in a(question, "/options").iter().enumerate() {
        let answer = answer.as_str().unwrap_or("");
        let value = format!("   {}:{answer}", (b'A' + index as u8) as char);
        let height = wrapped_height(&value, width);
        let id = format!("competition-answer-{index}");
        if index == selected {
            f.fills.push(SceneFill {
                id: "competition-answer-selection".into(),
                bounds: Rect::new(x + 26, y, (original_width(answer) + 38).min(274), 16),
                color: [170, 0, 0, 255],
                z: 800,
                ..Default::default()
            });
            if phase == "quizQuestion" {
                let mut selection = r["quizSelection"].clone();
                selection["x"] = json!(x);
                selection["y"] = json!(y);
                if let Some(index) = animation(f, "competition-selection", &selection, 28) {
                    f.sprites[index].playback = Some(Playback {
                        interval_ms: n(&selection, "/interval").max(1) as u32,
                        max_advances: 1,
                        ..Default::default()
                    });
                }
            }
        }
        text(
            f,
            &format!("competition-answer-text-{index}"),
            value,
            Rect::new(x, y, width, height),
            [255; 4],
        );
        if phase == "quizQuestion" {
            control(
                f,
                &id,
                &format!("{}: {answer}", (b'A' + index as u8) as char),
                Rect::new(x, y, width, height),
                game("competitionAction", "answer", Some(index)),
                &[],
            );
            f.controls.last_mut().unwrap().on_hover = Some(choose_answer(index));
            control(
                f,
                &format!("competition-select-key-{index}"),
                "选择答案",
                Rect::default(),
                choose_answer(index),
                &[&((b'A' + index as u8) as char).to_string()],
            );
        }
        y += height as i32;
    }
    if phase == "quizQuestion" {
        control(
            f,
            "competition-answer-enter",
            "提交答案",
            Rect::default(),
            game("competitionAction", "answer", Some(selected)),
            &["Enter"],
        );
        control(
            f,
            "competition-answer-up",
            "上一答案",
            Rect::default(),
            choose_answer(selected.saturating_sub(1)),
            &["ArrowUp"],
        );
        control(
            f,
            "competition-answer-down",
            "下一答案",
            Rect::default(),
            choose_answer((selected + 1).min(3)),
            &["ArrowDown"],
        );
    }
}

fn audio(f: &mut SceneFrame, rules: &Value, state: &Value) {
    let r = &rules["rendering"];
    let phase = s(state, "/phase");
    let mut loops = Vec::new();
    match phase {
        "opening" | "menu" => loops.push(0),
        "intro" => loops.push(2),
        "beauty" => {
            loops.push(3);
            if ["walkIn", "pose", "walkOut"].contains(&s(state, "/beauty/phase")) {
                loops.push(4);
            }
        }
        "running" => {
            loops.push(7);
            if ["resultPause", "podium"].contains(&s(state, "/running/phase")) {
                loops.push(10);
            }
        }
        p if p.starts_with("quiz") => {
            loops.push(11);
            if p == "quizResult" {
                loops.push(13);
            }
        }
        _ => {}
    }
    for index in loops {
        sound(
            f,
            &format!("competition-music-{index}"),
            r["sounds"][index].as_str().unwrap_or(""),
            true,
            0,
        );
    }
    if let Some(index) = match phase {
        "opening" => Some(15),
        "rankPrize" => Some(13),
        "moneyPrize" => Some(14),
        _ => None,
    } {
        sound(
            f,
            "competition-phase-sound",
            r["sounds"][index].as_str().unwrap_or(""),
            false,
            n(state, "/phaseSince") as u64 + index as u64,
        );
    }
    if phase == "beauty" && s(state, "/beauty/phase") == "judges" {
        sound(
            f,
            "competition-judges",
            r["sounds"][5].as_str().unwrap_or(""),
            false,
            n(state, "/beauty/contestant") as u64,
        );
    }
    if phase == "quizFeedback" {
        sound(
            f,
            "competition-answer-feedback",
            r["quizSounds"][usize::from(!b(state, "/lastCorrect"))]
                .as_str()
                .unwrap_or(""),
            false,
            n(state, "/exam/answered") as u64,
        );
    }
    if phase == "running" {
        for (index, cue) in a(state, "/running/sounds").iter().enumerate() {
            let resource = rules["running"]["rendering"]["sounds"][cue.as_str().unwrap_or("")]
                .as_str()
                .unwrap_or("");
            sound(
                f,
                &format!("competition-running-sound-{index}"),
                resource,
                false,
                n(state, "/clockMs") as u64,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules() -> Value {
        serde_json::from_str(include_str!("../../../../assets/catalog/rules.json")).unwrap()
    }
    fn snapshot(phase: &str) -> Value {
        json!({"pending":{"kind":"competition","text":"原版比赛解说","state":{"phase":phase,"name":"小明","sex":0,"stage":3,"clockMs":1234,"phaseSince":100}}})
    }
    fn frame(rules: &Value, snapshot: &Value, ui: &PresentationState) -> SceneFrame {
        let c = SceneContent {
            rules,
            maps: &Value::Null,
            manifest: &Value::Null,
        };
        let mut f = SceneFrame::default();
        compose(&mut f, &c, snapshot, ui);
        f
    }

    #[test]
    fn quiz_feedback_keeps_answered_text_and_keyboard_selection_defers_submission() {
        let rules = rules();
        let mut snapshot = snapshot("quizQuestion");
        snapshot["pending"]["state"]["exam"] = json!({"score":40,"answered":3,"current":{"question":"现在的问题","options":["甲","乙","丙","丁"]}});
        let mut ui = PresentationState::default();
        ui.profile["_competitionAnswer"] = json!(2);
        let f = frame(&rules, &snapshot, &ui);
        let answer = f
            .controls
            .iter()
            .find(|v| v.id == "competition-answer-enter")
            .unwrap();
        assert_eq!(answer.action, game("competitionAction", "answer", Some(2)));
        let key = f
            .controls
            .iter()
            .find(|v| v.id == "competition-select-key-1")
            .unwrap();
        assert_eq!(key.action, choose_answer(1));
        assert!(
            f.controls
                .iter()
                .find(|v| v.id == "competition-answer-1")
                .unwrap()
                .on_hover
                .is_some()
        );
        let bar = f
            .sprites
            .iter()
            .find(|v| v.id == "competition-scoreicon")
            .unwrap();
        assert_eq!((bar.x, bar.y), (544, 213));
        snapshot["pending"]["state"]["phase"] = json!("quizFeedback");
        snapshot["pending"]["state"]["answeredQuestion"] =
            json!({"question":"已经回答的原问题","options":["原甲","原乙","原丙","原丁"]});
        snapshot["pending"]["state"]["feedback"] = rules["competition"]["quizFeedback"].clone();
        snapshot["pending"]["state"]["lastCorrect"] = json!(true);
        let f = frame(&rules, &snapshot, &ui);
        assert!(
            f.texts
                .iter()
                .any(|v| v.id == "competition-question" && v.text == "已经回答的原问题")
        );
        assert!(f.controls.is_empty());
        assert!(f.sprites.iter().any(|v| v.id == "competition-feedback"));
    }
    #[test]
    fn beauty_preserves_explicit_layers_and_judge_scores() {
        let rules = rules();
        let mut snapshot = snapshot("beauty");
        snapshot["pending"]["state"]["beauty"] = json!({"phase":"scores","contestant":1,"scores":[[60,61,62],[70,71,72],[80,81,82],[90,91,92]],"totals":[183,213,243,273],"layers":[{"resource":"install/harvest/judges.flc","x":107,"y":99,"frame":7}]});
        let f = frame(&rules, &snapshot, &PresentationState::default());
        let layer = f
            .sprites
            .iter()
            .find(|v| v.id == "competition-beauty-0")
            .unwrap();
        assert_eq!((layer.frame, layer.x, layer.y, layer.z), (7, 107, 99, 30));
        assert_eq!(
            f.sprites
                .iter()
                .filter(|v| v.id.starts_with("competition-portrait-"))
                .count(),
            4
        );
        assert!(
            f.texts
                .iter()
                .any(|v| v.id == "competition-judge-score-2" && v.text == "72")
        );
        assert!(
            f.texts
                .iter()
                .any(|v| v.id == "competition-score-1" && v.text == "213")
        );
        assert!(!f.texts.iter().any(|v| v.id == "competition-score-2"));
    }
    #[test]
    fn running_cleared_podium_removes_old_score_panels_and_keeps_native_layer_order() {
        let rules = rules();
        let mut snapshot = snapshot("running");
        snapshot["pending"]["state"]["running"] = json!({"phase":"podium","clearedFrame":true,"showScorePanels":false,"contestant":4,"podiumPlace":4,"winners":[0,2,1],"layers":[{"resource":"podium.flc","x":90,"y":40,"frame":4},{"resource":"first.flc","x":300,"y":100,"frame":9}]});
        let f = frame(&rules, &snapshot, &PresentationState::default());
        assert!(!f.sprites.iter().any(|v|v.id=="competition-background"||v.id.starts_with("competition-panel-")));
        assert_eq!(
            f.sprites
                .iter()
                .filter(|v| v.id.starts_with("competition-running-"))
                .map(|v| (v.frame, v.z))
                .collect::<Vec<_>>(),
            [(4, 30), (9, 31)]
        );
        assert_eq!(
            f.texts
                .iter()
                .filter(|v| v.id.starts_with("competition-podium-name-"))
                .count(),
            3
        );
        assert!(
            f.texts
                .iter()
                .any(|v| v.id == "competition-podium-name-0" && v.text == "小明")
        );
    }
    #[test]
    fn presenter_animations_advance_once_per_visible_frame_without_catch_up() {
        let rules = rules();
        let mut snapshot = snapshot("quizQuestion");
        snapshot["pending"]["state"]["exam"] = json!({"score":0,"answered":0,"current":{"question":"问题","options":["甲","乙","丙","丁"]}});
        let host = s(&rules, "/competition/rendering/hostAnimation/resource");
        let select = s(&rules, "/competition/rendering/quizSelection/resource");
        let mut manifest = json!({"resources":{},"objects":[{"sha256":"host","media":{"frameCount":31}},{"sha256":"select","media":{"frameCount":2}}]});
        manifest["resources"][host] = json!({"sha256":"host"});
        manifest["resources"][select] = json!({"sha256":"select"});
        let c = SceneContent {
            rules: &rules,
            maps: &Value::Null,
            manifest: &manifest,
        };
        let mut ui = PresentationState::default();
        ui.screen = Screen::Game;
        super::super::update_presentation(&c, &snapshot, &mut ui, 1000);
        let f = super::super::compose(&c, &snapshot, &ui);
        assert_eq!(
            f.sprites
                .iter()
                .find(|v| v.id == "competition-host")
                .unwrap()
                .frame,
            1
        );
        assert_eq!(
            f.sprites
                .iter()
                .find(|v| v.id == "competition-selection")
                .unwrap()
                .frame,
            1
        );
        // Reading a frame while paused performs no clock update.
        let again = super::super::compose(&c, &snapshot, &ui);
        assert_eq!(
            again
                .sprites
                .iter()
                .find(|v| v.id == "competition-host")
                .unwrap()
                .frame,
            1
        );
    }
}
