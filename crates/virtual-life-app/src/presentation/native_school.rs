//! Native high-school examination, recovered from 0043e030/0043f100/0043ed90.
use super::*;

pub(super) fn owns_result(snapshot: &Value, ui: &PresentationState) -> bool {
    s(snapshot, "/nativeView/kind") != "schoolExam" && ui.profile["_nativeSchool"]["active"] == true
}

fn choose(index: i32) -> Action {
    Action::Game(json!({"type":"chooseOption","index":index}))
}
fn state(snapshot: &Value, ui: &PresentationState) -> Value {
    let current = &ui.profile["_nativeSchool"];
    if current["active"] == true {
        return current.clone();
    }
    if s(snapshot, "/nativeView/kind") != "schoolExam" {
        return Value::Null;
    }
    json!({"active":true,"phase":if b(snapshot,"/nativeView/started"){"slide"}else{"entrance"},"view":snapshot["nativeView"],"selection":0,"elapsed":0,"x":0,"y":3,"panelX":400,"seenAnswer":ui.profile["_schoolAnswerEvent"]["revision"],"name":snapshot["player"]["name"]})
}
fn set(state: &Value, key: &str, value: Value) -> Action {
    let mut state = state.clone();
    state[key] = value;
    state["elapsed"] = json!(0);
    Action::Ui(UiAction::SetField {
        field: "_nativeSchool".into(),
        value: state,
    })
}
fn animation(f: &mut SceneFrame, id: &str, v: &Value, once: bool, z: i32) {
    if let Some(i) = image(f, id, v, z) {
        f.sprites[i].frame_count = v["frameCount"].as_u64().map(|v| v as u32);
        f.sprites[i].playback = Some(Playback {
            interval_ms: n(v, "/interval").max(1) as u32,
            once,
            max_advances: 1,
            immediate: true,
        });
    }
}
fn ended(ui: &PresentationState, id: &str, v: &Value) -> bool {
    ui.runtime
        .animations
        .get(id)
        .is_some_and(|clock| clock.frame + 1 >= n(v, "/frameCount") as u32)
}
fn box_text(f: &mut SceneFrame, id: &str, value: impl Into<String>, v: &Value, color: [u8; 4]) {
    text(
        f,
        id,
        value,
        Rect::new(
            n(v, "/x"),
            n(v, "/y"),
            n(v, "/width") as u32,
            n(v, "/height") as u32,
        ),
        color,
    );
    let t = f.texts.last_mut().unwrap();
    t.z = 20;
    t.font = TextFont::Gdi {
        size: 16,
        bold: true,
    };
    t.line_height = 16;
    t.clip = Some(t.bounds);
}
fn button(
    f: &mut SceneFrame,
    id: &str,
    v: &Value,
    action: Action,
    label: &str,
    ui: &PresentationState,
) {
    let i = image(f, id, v, 30).unwrap();
    let sp = &mut f.sprites[i];
    sp.frame_count = Some(3);
    sp.height = Some(n(v, "/frameHeight") as u32);
    sp.frame = if ui.pressed.as_deref() == Some(id) {
        2
    } else if ui.hover.as_deref() == Some(id) {
        1
    } else {
        0
    };
    let r = Rect::new(sp.x, sp.y, sp.width.unwrap_or(59), sp.height.unwrap_or(25));
    control(f, id, label, r, action, &[]);
}
fn next(state: &mut Value, phase: &str) {
    state["phase"] = json!(phase);
    state["elapsed"] = json!(0);
}
pub(super) fn update(
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    elapsed_ms: u32,
) {
    if ui.screen != Screen::Game {
        return;
    }
    let mut st = state(snapshot, ui);
    if st.is_null() {
        return;
    }
    let phase = s(&st, "/phase").to_owned();
    let active = s(snapshot, "/nativeView/kind") == "schoolExam";
    st["elapsed"] = json!(st["elapsed"].as_u64().unwrap_or(0) + u64::from(elapsed_ms));
    let event = &ui.profile["_schoolAnswerEvent"];
    if event["revision"] != st["seenAnswer"]
        && !event.is_null()
        && event["answer"]["answered"].as_i64().unwrap_or(0) > n(&st, "/view/answered") as i64
    {
        st["seenAnswer"] = event["revision"].clone();
        st["feedback"] = event["previousView"].clone();
        st["correct"] = event["answer"]["correct"].clone();
        st["score"] = event["answer"]["score"].clone();
        st["answered"] = event["answer"]["answered"].clone();
        st["selection"] = json!(0);
        if active {
            st["view"] = snapshot["nativeView"].clone();
        }
        next(&mut st, "feedback");
    } else {
        let meta = c.native_school();
        match phase.as_str() {
            "entrance" => {
                st["x"] = json!((n(&st, "/x") + 8).min(70));
                st["y"] = json!((n(&st, "/y") + 4).min(38));
                if n(&st, "/x") == 70 && n(&st, "/y") == 38 {
                    next(&mut st, "intro");
                }
            }
            "intro" if n(&st, "/elapsed") >= 10000 => next(&mut st, "invite"),
            "invite" if active && b(snapshot, "/nativeView/started") => {
                st["view"] = snapshot["nativeView"].clone();
                next(&mut st, "slide");
            }
            "slide" => {
                st["panelX"] = json!((n(&st, "/panelX") - 1).max(289));
                if n(&st, "/panelX") == 289 {
                    next(&mut st, "question");
                }
            }
            "feedback" => {
                let right = b(&st, "/correct");
                let v = &meta[if right { "correct" } else { "wrong" }];
                if ended(ui, "school-feedback", v) {
                    if n(&st, "/answered") >= 10 {
                        next(&mut st, "resultOpening");
                    } else {
                        next(&mut st, "question");
                    }
                }
            }
            "resultOpening" => {
                if ended(ui, "school-result-door", &meta["resultAnimations"][1]) {
                    next(&mut st, "resultBand");
                }
            }
            "resultBand" => {
                let part = if n(&st, "/score") >= 60 { 2 } else { 3 };
                if ended(ui, "school-result-band", &meta["resultAnimations"][part]) {
                    next(&mut st, "result");
                }
            }
            "result" => {
                let v = &meta[if n(&st, "/score") >= 60 {
                    "win"
                } else {
                    "lost"
                }];
                if ended(ui, "school-result-final", v) {
                    st["active"] = json!(false);
                    next(&mut st, "done");
                }
            }
            _ => {}
        }
    }
    if active && phase == "entrance" && b(snapshot, "/nativeView/started") {
        st["view"] = snapshot["nativeView"].clone();
        next(&mut st, "slide");
    }
    if !active && matches!(s(&st, "/phase"), "invite" | "intro" | "entrance") {
        st["active"] = json!(false);
    }
    ui.profile["_nativeSchool"] = st;
}
fn rows(text: &str, width: i32) -> i32 {
    let mut x = 0;
    let mut rows = 1;
    for ch in text.chars() {
        if ch == '\n' {
            rows += 1;
            x = 0;
            continue;
        }
        let w = if ch.is_ascii() { 8 } else { 16 };
        if x + w > width {
            rows += 1;
            x = 0;
        }
        x += w;
    }
    rows
}
pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) -> bool {
    let st = state(snapshot, ui);
    if st.is_null() || st["active"] != true {
        return false;
    }
    let meta = c.native_school();
    let phase = s(&st, "/phase");
    f.key = format!("school:{phase}:{}", n(&st, "/view/answered"));
    f.palette = Some(s(meta, "/background/resource").into());
    for section in a(meta, "/paletteSections") {
        f.palette_sections.push((
            s(section, "/resource").into(),
            n(section, "/first") as u8,
            n(section, "/last") as u8,
        ));
    }
    image(f, "school-background", &meta["background"], 0);
    sound(f, "school-music", s(meta, "/music"), true, 0);
    let mut actor = meta["presenter"].clone();
    actor["x"] = json!(if phase == "entrance" {
        n(&st, "/x")
    } else {
        70
    });
    actor["y"] = json!(if phase == "entrance" {
        n(&st, "/y")
    } else {
        38
    });
    animation(f, "school-presenter", &actor, false, 2);
    if matches!(phase, "entrance" | "intro" | "invite") {
        if phase != "invite" {
            sound(f, "school-intro-voice", s(meta, "/sounds/intro"), true, 0);
        }
        box_text(
            f,
            "school-introduction",
            s(
                meta,
                if phase == "invite" {
                    "/messages/invite"
                } else {
                    "/messages/intro"
                },
            ),
            &meta["introText"],
            [255; 4],
        );
        if phase == "intro" {
            control(
                f,
                "school-intro-next",
                "继续",
                Rect::new(0, 0, 640, 480),
                set(&st, "phase", json!("invite")),
                &["Enter", "Space"],
            );
        }
        if phase == "invite" {
            button(f, "school-start", &meta["yes"], choose(0), "参加考试", ui);
            button(
                f,
                "school-decline",
                &meta["no"],
                choose(1),
                "放弃考试，进入社会",
                ui,
            );
        }
        return finish(f, meta);
    }
    if phase.starts_with("result") {
        f.requests.push(SceneRequest::CursorHidden(true));
        f.sprites.clear();
        f.palette_sections.clear();
        f.palette = Some(s(meta, "/resultAnimations/0/resource").into());
        animation(
            f,
            "school-result-background",
            &meta["resultAnimations"][0],
            false,
            0,
        );
        animation(
            f,
            "school-result-door",
            &meta["resultAnimations"][1],
            true,
            1,
        );
        if phase == "resultBand" {
            animation(
                f,
                "school-result-band",
                &meta["resultAnimations"][if n(&st, "/score") >= 60 { 2 } else { 3 }],
                true,
                2,
            );
        }
        if phase == "result" {
            f.sprites.clear();
            f.palette = Some(
                s(
                    meta,
                    if n(&st, "/score") >= 60 {
                        "/win/resource"
                    } else {
                        "/lost/resource"
                    },
                )
                .into(),
            );
            animation(
                f,
                "school-result-final",
                &meta[if n(&st, "/score") >= 60 {
                    "win"
                } else {
                    "lost"
                }],
                true,
                3,
            );
        }
        return finish(f, meta);
    }
    let mut panel = meta["questionPanel"].clone();
    panel["x"] = json!(if phase == "slide" {
        n(&st, "/panelX")
    } else {
        289
    });
    image(f, "school-question-panel", &panel, 3);
    if phase == "slide" {
        return finish(f, meta);
    }
    let view = if phase == "feedback" {
        &st["feedback"]
    } else {
        &st["view"]
    };
    let question = &view["currentQuestion"];
    // 0045e552 preserves the source LF. 0045bae0 adds tmHeight both for
    // that LF and once after every append; the separate 21-space append adds
    // another row before the four answers.
    let prompt = s(question, "/prompt");
    let mut y = 146;
    box_text(
        f,
        "school-question",
        prompt,
        &json!({"x":300,"y":y,"width":240,"height":232}),
        [0, 255, 255, 255],
    );
    y += (rows(prompt, 240) + 2) * 16;
    let selection = n(&st, "/selection").clamp(0, 3);
    let mut selected_y = y;
    let mut selected_width = 54;
    for (i, option) in a(question, "/options").iter().enumerate() {
        let value = s(option, "/text");
        let rendered = format!(
            "{}{}",
            meta["answerPrefixes"][i].as_str().unwrap_or(""),
            value
        );
        let h = (rows(&rendered, 240) + 1) * 16;
        let top = y;
        box_text(
            f,
            &format!("school-answer-text-{i}"),
            rendered.clone(),
            &json!({"x":300,"y":top,"width":240,"height":h}),
            [0, 255, 255, 255],
        );
        if selection == i as i32 {
            selected_y = top;
            let encoded = format!("{rendered}\n");
            let (bytes, _, _) = encoding_rs::GBK.encode(&encoded);
            selected_width = (bytes.len() as i32 * 8 - 32).min(186) + 54;
        }
        if phase == "question" {
            control(
                f,
                &format!("school-answer-{i}"),
                value,
                Rect::new(300, top, 240, 16),
                if selection == i as i32 {
                    choose(n(option, "/index"))
                } else {
                    set(&st, "selection", json!(i))
                },
                &[],
            );
            let ctl = f.controls.last_mut().unwrap();
            ctl.on_hover = Some(set(&st, "selection", json!(i)));
            ctl.on_leave = None;
            control(
                f,
                &format!("school-key-{i}"),
                &format!("选择答案 {}", (b'A' + i as u8) as char),
                Rect::new(0, 0, 0, 0),
                set(&st, "selection", json!(i)),
                &[&format!("Key{}:up", (b'A' + i as u8) as char)],
            );
        }
        y += h;
    }
    f.fills.push(SceneFill {
        id: "school-selected-answer".into(),
        bounds: Rect::new(
            326,
            selected_y,
            selected_width.saturating_sub(26) as u32,
            16,
        ),
        palette: Some(s(meta, "/background/resource").into()),
        palette_index: Some(2),
        z: 4,
        ..Default::default()
    });
    let mut select = meta["selector"].clone();
    select["x"] = json!(300);
    select["y"] = json!(selected_y);
    animation(f, "school-selector", &select, false, 5);
    for (id, value, x, y) in [
        ("name", s(&st, "/name").to_owned(), 349, 104),
        (
            "score",
            if phase == "feedback" {
                n(&st, "/score")
            } else {
                n(view, "/score")
            }
            .to_string(),
            457,
            104,
        ),
        (
            "answered",
            if phase == "feedback" {
                n(&st, "/answered")
            } else {
                n(view, "/answered")
            }
            .to_string(),
            506,
            359,
        ),
        ("total", "10".into(), 525, 368),
    ] {
        text(
            f,
            &format!("school-{id}"),
            value,
            Rect::new(x, y, 100, 18),
            [0, 0, 0, 255],
        );
        f.texts.last_mut().unwrap().font = TextFont::Gdi {
            size: 16,
            bold: true,
        };
    }
    if phase == "feedback" {
        animation(
            f,
            "school-feedback",
            &meta[if b(&st, "/correct") {
                "correct"
            } else {
                "wrong"
            }],
            true,
            50,
        );
    } else {
        let options = a(question, "/options");
        let actual = options
            .get(selection as usize)
            .map_or(selection, |v| n(v, "/index"));
        control(
            f,
            "school-submit",
            "提交答案",
            Rect::new(0, 0, 0, 0),
            choose(actual),
            &["Enter:up"],
        );
        for (id, delta, key) in [("up", -1, "ArrowUp:up"), ("down", 1, "ArrowDown:up")] {
            control(
                f,
                &format!("school-{id}"),
                if delta < 0 {
                    "上一个答案"
                } else {
                    "下一个答案"
                },
                Rect::new(0, 0, 0, 0),
                set(&st, "selection", json!((selection + delta).clamp(0, 3))),
                &[key],
            );
        }
    }
    finish(f, meta)
}
fn finish(f: &mut SceneFrame, meta: &Value) -> bool {
    for sprite in &mut f.sprites {
        if let Some(mapping) = meta["gdiIndexMaps"][&sprite.resource_id].as_array() {
            sprite.index_map = mapping
                .iter()
                .map(|pair| [n(pair, "/0") as u8, n(pair, "/1") as u8])
                .collect();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::OnceLock};
    fn catalogs() -> &'static (Value, Value) {
        static DATA: OnceLock<(Value, Value)> = OnceLock::new();
        DATA.get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let read = |p: &str| {
                serde_json::from_slice::<Value>(&std::fs::read(root.join(p)).unwrap()).unwrap()
            };
            let mut manifest = read("assets/manifest.json");
            manifest["_nativeSchool"] = read("assets/catalog/native-school.json");
            (manifest, read("assets/catalog/questionnaires.json"))
        })
    }
    fn snapshot(question: &Value) -> Value {
        json!({"revision":1,"player":{"name":"测试"},"nativeView":{"kind":"schoolExam","started":true,"answered":0,"score":0,"currentQuestion":{"id":question["id"],"prompt":question["question"],"options":question["options"].as_array().unwrap().iter().enumerate().map(|(i,t)|json!({"index":i,"text":t})).collect::<Vec<_>>()}}})
    }
    fn game(snapshot: &Value, phase: &str) -> PresentationState {
        let mut ui = PresentationState::default();
        ui.screen = Screen::Game;
        ui.profile["_nativeSchool"] = state(snapshot, &ui);
        ui.profile["_nativeSchool"]["phase"] = json!(phase);
        ui
    }
    fn clock(ui: &mut PresentationState, id: &str, last: u32) {
        ui.runtime.animations.insert(
            id.into(),
            AnimationClock {
                frame: last,
                ..Default::default()
            },
        );
    }
    #[test]
    fn every_original_exam_prompt_and_four_answers_fit_the_native_question_window() {
        let (manifest, q) = catalogs();
        let none = Value::Null;
        let c = SceneContent {
            manifest,
            maps: &none,
            rules: &none,
        };
        for question in a(q, "/highTest/records") {
            let snapshot = snapshot(question);
            let ui = game(&snapshot, "question");
            let mut f = SceneFrame::default();
            assert!(compose(&mut f, &c, &snapshot, &ui));
            for text in f
                .texts
                .iter()
                .filter(|v| v.id == "school-question" || v.id.starts_with("school-answer-text-"))
            {
                assert!(
                    text.bounds.x == 300
                        && text.bounds.y >= 146
                        && text.bounds.y + text.bounds.height as i32 <= 378,
                    "Question {} clipped: {} {:?}",
                    question["id"],
                    text.id,
                    text.bounds
                );
            }
            for control in &f.controls {
                assert!(
                    control.bounds.x >= 0
                        && control.bounds.y >= 0
                        && control.bounds.x + control.bounds.width as i32 <= 640
                        && control.bounds.y + control.bounds.height as i32 <= 480
                );
            }
            assert_eq!(
                f.controls
                    .iter()
                    .filter(|v| v.id.starts_with("school-answer-"))
                    .count(),
                4
            );
            assert!(
                f.controls
                    .iter()
                    .any(|v| v.id == "school-submit" && v.action == choose(0))
            );
        }
    }
    #[test]
    fn native_question_panel_moves_one_pixel_per_frame_even_after_a_long_frame() {
        let (manifest, q) = catalogs();
        let none = Value::Null;
        let c = SceneContent {
            manifest,
            maps: &none,
            rules: &none,
        };
        let snapshot = snapshot(&q["highTest"]["records"][0]);
        let mut ui = game(&snapshot, "slide");
        update(&c, &snapshot, &mut ui, 60000);
        assert_eq!(n(&ui.profile, "/_nativeSchool/panelX"), 399);
        for _ in 0..110 {
            update(&c, &snapshot, &mut ui, 16);
        }
        assert_eq!(s(&ui.profile, "/_nativeSchool/phase"), "question");
        assert_eq!(n(&ui.profile, "/_nativeSchool/panelX"), 289);
    }
    #[test]
    fn final_answer_feedback_and_result_cover_the_following_core_scene_until_the_last_frame() {
        let (manifest, q) = catalogs();
        let none = Value::Null;
        let c = SceneContent {
            manifest,
            maps: &none,
            rules: &none,
        };
        let mut before = snapshot(&q["highTest"]["records"][0]);
        before["nativeView"]["answered"] = json!(9);
        let mut ui = game(&before, "question");
        ui.profile["_schoolAnswerEvent"] = json!({"revision":2,"previousView":before["nativeView"],"answer":{"answered":10,"score":50,"correct":false}});
        let after = json!({"revision":2,"pending":null});
        super::super::events::accept(
            &c,
            &before,
            &after,
            &[json!({"type":"birthday","resource":"install/bmp/jm01.bmp"})],
            &mut ui,
        );
        super::super::update_presentation(&c, &after, &mut ui, 16);
        assert_eq!(s(&ui.profile, "/_nativeSchool/phase"), "feedback");
        let mut f = SceneFrame::default();
        assert!(compose(&mut f, &c, &after, &ui));
        assert!(f.tick.is_none());
        assert!(
            f.sprites
                .iter()
                .any(|v| v.id == "school-feedback" && v.resource_id == "install/w/w01.flc")
        );
        let saved = serde_json::to_value(&ui).unwrap();
        let mut ui: PresentationState = serde_json::from_value(saved).unwrap();
        clock(&mut ui, "school-feedback", 7);
        super::super::update_presentation(&c, &after, &mut ui, 16);
        assert_eq!(s(&ui.profile, "/_nativeSchool/phase"), "resultOpening");
        clock(&mut ui, "school-result-door", 14);
        super::super::update_presentation(&c, &after, &mut ui, 16);
        clock(&mut ui, "school-result-band", 4);
        super::super::update_presentation(&c, &after, &mut ui, 16);
        let last = n(&manifest["_nativeSchool"]["lost"], "/frameCount") as u32 - 1;
        clock(&mut ui, "school-result-final", last);
        super::super::update_presentation(&c, &after, &mut ui, 16);
        assert!(!compose(&mut SceneFrame::default(), &c, &after, &ui));
        assert_eq!(ui.runtime.sequence.as_ref().unwrap().cues[0].elapsed_ms, 0);
        let f = super::super::compose(&c, &after, &ui);
        assert!(f.sprites.iter().any(|sprite| sprite.id == "map-notice"));
    }
}
