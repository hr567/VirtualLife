//! FUN_00436b80's original fortune panel, text surface and selection controls.
use super::*;
const FIELD: &str = "_nativeFortune";
fn choose(index: i32) -> Action {
    Action::Game(json!({"type":"chooseOption","index":index.max(0)}))
}
fn scope(snapshot: &Value) -> String {
    format!(
        "fortune:{}:{}:{}",
        n(snapshot, "/nativeView/recordId"),
        n(snapshot, "/turn"),
        n(snapshot, "/fortuneVisits")
    )
}
fn state(snapshot: &Value, ui: &PresentationState) -> Value {
    let old = &ui.profile[FIELD];
    if s(snapshot, "/nativeView/kind") != "fortune" {
        let mut result = old.clone();
        if s(&result, "/phase") == "settling" {
            result["phase"] = json!("result");
            result["elapsed"] = json!(0);
        }
        return result;
    }
    let scope = scope(snapshot);
    let page = format!(
        "{}:{}",
        s(snapshot, "/nativeView/phase"),
        n(snapshot, "/nativeView/questionIndex")
    );
    if s(old, "/scope") == scope && s(old, "/page") == page {
        return old.clone();
    }
    let phase = if s(snapshot, "/nativeView/phase") == "explanation" {
        "explanation"
    } else if s(old, "/scope") != scope
        && a(snapshot, "/nativeView/answers").is_empty()
        && n(snapshot, "/nativeView/questionIndex") == 0
    {
        "intro"
    } else {
        "question"
    };
    json!({"scope":scope,"page":page,"phase":phase,"selection":0,"scroll":0,"elapsed":0})
}
fn change(state: &Value, field: &str, value: Value) -> Action {
    let mut next = state.clone();
    next[field] = value;
    if field == "phase" {
        next["elapsed"] = json!(0);
    }
    Action::Ui(UiAction::SetField {
        field: FIELD.into(),
        value: next,
    })
}
fn rect(v: &Value) -> Rect {
    Rect::new(
        n(v, "/x"),
        n(v, "/y"),
        n(v, "/width").max(1) as u32,
        n(v, "/height").max(1) as u32,
    )
}
fn body(f: &mut SceneFrame, id: &str, value: &str, bounds: Rect, clip: Rect) {
    text(f, id, value, bounds, [255; 4]);
    let text = f.texts.last_mut().unwrap();
    text.font = TextFont::Gdi {
        size: 16,
        bold: true,
    };
    text.line_height = 16;
    text.clip = Some(clip);
}
fn delay(f: &mut SceneFrame, state: &Value, milliseconds: u64, action: Action) {
    f.requests.push(SceneRequest::AfterDelay {
        elapsed_ms: state["elapsed"].as_u64().unwrap_or(0),
        delay_ms: milliseconds,
        action,
    });
}
fn layout<'a>(meta: &'a Value, snapshot: &Value) -> &'a Value {
    &meta["records"][n(snapshot, "/nativeView/recordId").to_string()]["questions"]
        [n(snapshot, "/nativeView/questionIndex").max(0) as usize]
}
fn selection(state: &Value, index: i32, layout: &Value, height: i32) -> Action {
    let count = a(layout, "/options").len() as i32;
    let index = index.rem_euclid(count.max(1));
    let offset = n(&layout["options"][index as usize], "/y");
    let maximum = (n(layout, "/height") - height).max(0);
    let mut scroll = n(state, "/scroll").clamp(0, maximum);
    while offset >= scroll + height && scroll < maximum {
        scroll = (scroll + height).min(maximum);
    }
    while offset < scroll && scroll > 0 {
        scroll = (scroll - height).max(0);
    }
    let mut next = state.clone();
    next["selection"] = json!(index);
    next["scroll"] = json!(scroll);
    Action::Ui(UiAction::SetField {
        field: FIELD.into(),
        value: next,
    })
}
fn continue_action(snapshot: &Value, state: &Value) -> Action {
    if a(snapshot, "/nativeView/answers").len() as i32 >= n(snapshot, "/nativeView/questionCount") {
        change(state, "phase", json!("settling"))
    } else {
        choose(n(snapshot, "/nativeView/continueIndex"))
    }
}
pub(super) fn update(
    _c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    delta: u32,
) {
    if ui.screen != Screen::Game {
        return;
    }
    if s(snapshot, "/nativeView/kind") != "fortune"
        && !["settling", "result"].contains(&s(&ui.profile[FIELD], "/phase"))
    {
        return;
    }
    let mut next = state(snapshot, ui);
    let elapsed = next["elapsed"].as_u64().unwrap_or(0) + u64::from(delta);
    next["elapsed"] = json!(elapsed);
    if s(&next, "/phase") == "intro" && elapsed >= 10000 {
        next["phase"] = json!("question");
        next["elapsed"] = json!(0);
    }
    ui.profile[FIELD] = next;
}
pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) -> bool {
    let active = s(snapshot, "/nativeView/kind") == "fortune";
    if !active && !["settling", "result"].contains(&s(&ui.profile[FIELD], "/phase")) {
        return false;
    }
    let meta = c.native_fortune();
    let state = state(snapshot, ui);
    let phase = s(&state, "/phase");
    f.tick = None;
    f.key = format!(
        "native-fortune:{}:{phase}:{}:{}",
        s(&state, "/scope"),
        n(&state, "/selection"),
        n(&state, "/scroll")
    );
    for (i, layer) in a(meta, "/layers").iter().enumerate() {
        image(f, &format!("fortune-layer-{i}"), layer, i as i32);
    }
    f.palette = a(meta, "/layers")
        .first()
        .map(|layer| s(layer, "/resource").to_owned());
    for section in a(meta, "/paletteSections") {
        f.palette_sections.push((
            s(section, "/resource").into(),
            n(section, "/first") as u8,
            n(section, "/last") as u8,
        ));
    }
    sound(f, "fortune-music", s(meta, "/music"), true, 0);
    for entry in a(meta, "/hud/entries") {
        text(
            f,
            &format!("fortune-hud-{}", s(entry, "/field")),
            n(snapshot, &format!("/stats/{}", s(entry, "/field"))).to_string(),
            Rect::new(n(entry, "/x"), n(meta, "/hud/y"), 100, 18),
            [255; 4],
        );
        f.texts.last_mut().unwrap().font = TextFont::Gdi {
            size: 16,
            bold: false,
        };
    }
    let body_rect = rect(&meta["body"]);
    let bottom = rect(&meta["bottom"]);
    match phase {
        "intro" => {
            let visit = n(snapshot, "/fortuneVisits").clamp(0, 11) as usize;
            let mission = b(snapshot, "/nativeView/specialGreeting");
            body(
                f,
                "fortune-intro-text",
                if mission {
                    s(meta, "/messages/mission")
                } else {
                    s(&meta["messages"]["introductions"][visit], "")
                },
                bottom,
                bottom,
            );
            control(
                f,
                "fortune-intro",
                "继续",
                Rect::new(0, 0, 640, 480),
                change(&state, "phase", json!("question")),
                &["Enter", "Space"],
            );
        }
        "question" => {
            let layout = layout(meta, snapshot);
            let scroll = n(&state, "/scroll")
                .clamp(0, (n(layout, "/height") - body_rect.height as i32).max(0));
            let chosen = n(&state, "/selection");
            for option in a(layout, "/options") {
                let index = n(option, "/index");
                let y = body_rect.y + n(option, "/y") - scroll;
                let action = if chosen == index {
                    choose(index)
                } else {
                    selection(&state, index, layout, body_rect.height as i32)
                };
                if y >= body_rect.y && y + 16 <= body_rect.y + body_rect.height as i32 {
                    control(
                        f,
                        &format!("fortune-option-{index}"),
                        s(option, "/text"),
                        Rect::new(body_rect.x, y, body_rect.width, 16),
                        action,
                        &[],
                    );
                }
                control(
                    f,
                    &format!("fortune-key-{index}"),
                    "",
                    Rect::new(0, 0, 1, 1),
                    selection(&state, index, layout, body_rect.height as i32),
                    &[s(&meta["answerKeys"][index as usize], "")],
                );
                if index == chosen && y >= body_rect.y && y < body_rect.y + body_rect.height as i32
                {
                    let mut v = meta["selector"].clone();
                    v["x"] = json!(body_rect.x);
                    v["y"] = json!(y);
                    if let Some(i) = image(f, "fortune-selection", &v, 3) {
                        f.sprites[i].playback = Some(Playback {
                            interval_ms: n(&v, "/interval").max(1) as u32,
                            once: false,
                            max_advances: 1,
                            immediate: false,
                        });
                        f.sprites[i].clip = Some(body_rect);
                    }
                }
            }
            body(
                f,
                "fortune-question-text",
                s(layout, "/text"),
                Rect::new(
                    body_rect.x,
                    body_rect.y - scroll,
                    body_rect.width,
                    n(layout, "/height").max(1) as u32,
                ),
                body_rect,
            );
            control(
                f,
                "fortune-answer",
                "确认答案",
                Rect::new(0, 0, 1, 1),
                choose(chosen),
                &["Enter"],
            );
            control(
                f,
                "fortune-up",
                "上一项答案",
                Rect::new(0, 0, 1, 1),
                selection(&state, chosen - 1, layout, body_rect.height as i32),
                &["ArrowUp"],
            );
            control(
                f,
                "fortune-down",
                "下一项答案",
                Rect::new(0, 0, 1, 1),
                selection(&state, chosen + 1, layout, body_rect.height as i32),
                &["ArrowDown"],
            );
            if n(layout, "/height") > body_rect.height as i32 {
                scrollbar(f, meta, &state, layout, scroll, ui);
            }
        }
        "explanation" => {
            body(
                f,
                "fortune-explanation-text",
                s(snapshot, "/nativeView/explanation"),
                body_rect,
                body_rect,
            );
            let action = continue_action(snapshot, &state);
            control(
                f,
                "fortune-continue",
                "继续",
                Rect::new(0, 0, 640, 480),
                action.clone(),
                &["Enter", "Space"],
            );
            delay(f, &state, n(meta, "/delays/explanation") as u64 + 1, action);
        }
        "settling" if active => {
            delay(
                f,
                &state,
                0,
                choose(n(snapshot, "/nativeView/continueIndex")),
            );
        }
        "result" | "settling" => {
            body(
                f,
                "fortune-result-text",
                s(meta, "/messages/result"),
                bottom,
                bottom,
            );
            let done = change(&state, "phase", json!("done"));
            control(
                f,
                "fortune-result-continue",
                "继续",
                Rect::new(0, 0, 640, 480),
                done.clone(),
                &["Enter", "Space", "Escape"],
            );
            delay(f, &state, n(meta, "/delays/result") as u64, done);
        }
        _ => {}
    }
    if active && !["settling", "result"].contains(&phase) {
        control(
            f,
            "fortune-exit",
            "离开测验",
            Rect::new(0, 0, 1, 1),
            choose(n(snapshot, "/nativeView/exitIndex")),
            &["Escape"],
        );
    }
    // Original 00436b80/00458b00 source rectangle, including the held cursor frame.
    let cursor = sprite(
        f,
        "fortune-cursor",
        s(meta, "/cursor/resource"),
        ui.pointer.0,
        ui.pointer.1,
        2000,
    );
    f.sprites[cursor].source = Some(Rect::new(
        if ui.pressed.is_some() { 24 } else { 0 },
        0,
        24,
        24,
    ));
    f.requests.push(SceneRequest::CursorHidden(true));
    true
}
fn scrollbar(
    f: &mut SceneFrame,
    meta: &Value,
    state: &Value,
    layout: &Value,
    scroll: i32,
    ui: &PresentationState,
) {
    let parts = a(meta, "/scroll/parts");
    if parts.len() < 6 {
        return;
    }
    let x = n(meta, "/scroll/x");
    let y = n(meta, "/scroll/y");
    let arrow = n(&parts[0], "/height");
    let travel = n(&parts[4], "/height");
    let maximum = (n(layout, "/height") - n(meta, "/body/height")).max(1);
    for (id, index, top, delta) in [("up", 0, y, -10), ("down", 2, y + arrow + travel, 10)] {
        let mut v = parts
            [index + usize::from(ui.pressed.as_deref() == Some(&format!("fortune-scroll-{id}")))]
        .clone();
        v["x"] = json!(x);
        v["y"] = json!(top);
        image(f, &format!("fortune-scroll-{id}"), &v, 10);
        control(
            f,
            &format!("fortune-scroll-{id}"),
            if delta < 0 {
                "向上滚动"
            } else {
                "向下滚动"
            },
            rect(&v),
            change(state, "scroll", json!((scroll + delta).clamp(0, maximum))),
            &[],
        );
    }
    let mut track = parts[4].clone();
    track["x"] = json!(x);
    track["y"] = json!(y + arrow);
    image(f, "fortune-scroll-track", &track, 9);
    // Native dragging uses the current track row, converted to an integer
    // percentage. Row hitboxes avoid capturing a previous-compose pointer.
    let dragging = ui
        .pressed
        .as_deref()
        .is_some_and(|id| id.starts_with("fortune-track-"));
    for row in 0..travel {
        let action = change(state, "scroll", json!((row * 100 / travel) * maximum / 100));
        control(
            f,
            &format!("fortune-track-{row}"),
            "滚动测验文本",
            Rect::new(x, y + arrow + row, n(&parts[0], "/width") as u32, 1),
            action.clone(),
            &[],
        );
        let control = f.controls.last_mut().unwrap();
        control.on_press = Some(action.clone());
        if dragging {
            control.on_hover = Some(action);
        }
    }
    let mut thumb = parts[5].clone();
    thumb["x"] = json!(x);
    thumb["y"] = json!(
        y + arrow + ((scroll * 100 / maximum) * travel / 100).min(travel - n(&parts[5], "/height"))
    );
    image(f, "fortune-scroll-thumb", &thumb, 11);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    fn content() -> SceneContent<'static> {
        static DATA: OnceLock<(Value, Value, Value)> = OnceLock::new();
        let(manifest,maps,rules)=DATA.get_or_init(||(json!({"_nativeFortune":serde_json::from_str::<Value>(include_str!("../../../../assets/catalog/native-fortune.json")).unwrap()}),Value::Null,Value::Null));
        SceneContent {
            manifest,
            maps,
            rules,
        }
    }
    fn snapshot() -> Value {
        json!({"turn":10,"fortuneVisits":0,"stats":{"intelligence":100,"morality":100,"charm":100,"stamina":100},"nativeView":{"kind":"fortune","recordId":5,"format":1,"phase":"question","questionIndex":0,"questionCount":2,"answers":[],"exitIndex":6}})
    }
    fn ui() -> PresentationState {
        let mut ui = PresentationState::default();
        ui.screen = Screen::Game;
        ui
    }
    fn apply(ui: &mut PresentationState, action: &Action) {
        let Action::Ui(UiAction::SetField { field, value }) = action else {
            panic!("Expected presentation-only input")
        };
        ui.profile[field] = value.clone();
    }
    #[test]
    fn original_intro_clock_pauses_on_save_screen_and_question_uses_real_choice_indices() {
        let c = content();
        let snap = snapshot();
        let mut ui = ui();
        update(&c, &snap, &mut ui, 9999);
        assert_eq!(s(&ui.profile[FIELD], "/phase"), "intro");
        let saved = ui.profile[FIELD].clone();
        ui.screen = Screen::Saves;
        update(&c, &snap, &mut ui, 20000);
        assert_eq!(ui.profile[FIELD], saved);
        ui.screen = Screen::Game;
        update(&c, &snap, &mut ui, 1);
        let mut f = SceneFrame::default();
        assert!(compose(&mut f, &c, &snap, &ui));
        assert!(f.tick.is_none());
        assert_eq!(
            f.controls
                .iter()
                .filter(|control| control.id.starts_with("fortune-option-"))
                .count(),
            6
        );
        let first = f
            .controls
            .iter()
            .find(|control| control.id == "fortune-option-1")
            .unwrap()
            .action
            .clone();
        apply(&mut ui, &first);
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &snap, &ui);
        assert_eq!(
            f.controls
                .iter()
                .find(|control| control.id == "fortune-option-1")
                .unwrap()
                .action,
            choose(1)
        );
        assert!(matches!(
            f.controls
                .iter()
                .find(|control| control.id == "fortune-key-1")
                .unwrap()
                .action,
            Action::Ui(_)
        ));
        let text = f
            .texts
            .iter()
            .find(|text| text.id == "fortune-question-text")
            .unwrap();
        assert_eq!(text.clip, Some(Rect::new(117, 71, 260, 238)));
        assert_eq!(text.line_height, 16);
    }
    #[test]
    fn long_question_selection_scrolls_the_original_surface_and_preserves_visible_hitboxes() {
        let c = content();
        let records = c.native_fortune()["records"].as_object().unwrap();
        let (id, record) = records
            .iter()
            .find(|(_, record)| {
                a(record, "/questions")
                    .iter()
                    .any(|question| n(question, "/height") > 238)
            })
            .unwrap();
        let (question_index, layout) = a(record, "/questions")
            .iter()
            .enumerate()
            .find(|(_, layout)| n(layout, "/height") > 238)
            .unwrap();
        let mut snap = snapshot();
        snap["nativeView"]["recordId"] = json!(id.parse::<i32>().unwrap());
        snap["nativeView"]["questionIndex"] = json!(question_index);
        let mut ui = ui();
        let mut current = state(&snap, &ui);
        current["phase"] = json!("question");
        ui.profile[FIELD] = current.clone();
        apply(
            &mut ui,
            &selection(
                &current,
                a(layout, "/options").len() as i32 - 1,
                layout,
                238,
            ),
        );
        assert!(n(&ui.profile[FIELD], "/scroll") > 0);
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &snap, &ui);
        for control in f
            .controls
            .iter()
            .filter(|control| control.id.starts_with("fortune-option-"))
        {
            assert!(control.bounds.y >= 71);
            assert!(control.bounds.y + control.bounds.height as i32 <= 309);
        }
        assert!(
            f.controls
                .iter()
                .any(|control| control.id == "fortune-scroll-down")
        );
    }
    #[test]
    fn final_explanation_settles_once_then_keeps_a_restorable_result_overlay() {
        let c = content();
        let mut snap = snapshot();
        snap["nativeView"]["phase"] = json!("explanation");
        snap["nativeView"]["answers"] = json!([0, 1]);
        snap["nativeView"]["continueIndex"] = json!(0);
        let mut ui = ui();
        let current = state(&snap, &ui);
        apply(&mut ui, &continue_action(&snap, &current));
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &snap, &ui);
        assert!(f.requests.iter().any(
            |request| matches!(request,SceneRequest::AfterDelay{action,..}if action==&choose(0))
        ));
        snap["nativeView"] = Value::Null;
        snap["pending"] = json!({"kind":"minigame","game":"fist"});
        update(&c, &snap, &mut ui, 1000);
        let saved = serde_json::to_string(&ui).unwrap();
        let restored: PresentationState = serde_json::from_str(&saved).unwrap();
        let mut f = SceneFrame::default();
        assert!(compose(&mut f, &c, &snap, &restored));
        assert!(f.tick.is_none());
        assert!(f.texts.iter().any(|text| text.id == "fortune-result-text"));
        assert!(
            f.controls
                .iter()
                .any(|control| control.id == "fortune-result-continue")
        );
        assert!(!f.requests.iter().any(|request| matches!(
            request,
            SceneRequest::AfterDelay {
                action: Action::Game(_),
                ..
            }
        )));
    }
}
