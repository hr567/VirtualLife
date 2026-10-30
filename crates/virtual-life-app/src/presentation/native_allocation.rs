//! Original 00420840 reward preview and 00420250 button/bitmap-font allocation.
use super::*;
fn state(snapshot: &Value, ui: &PresentationState) -> Value {
    let scope = format!(
        "{}:{}:{}",
        n(snapshot, "/revision"),
        n(snapshot, "/turn"),
        n(snapshot, "/pending/points")
    );
    if s(&ui.profile, "/_allocation/scope") == scope {
        return ui.profile["_allocation"].clone();
    }
    json!({"scope":scope,"phase":"preview","elapsed":0})
}
fn change(state: &Value) -> Action {
    let mut state = state.clone();
    state["phase"] = json!("allocate");
    state["elapsed"] = json!(0);
    Action::Ui(UiAction::SetField {
        field: "_allocation".into(),
        value: state,
    })
}
pub(super) fn update(
    _c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    elapsed: u32,
) {
    if ui.screen != Screen::Game || s(snapshot, "/pending/kind") != "allocation" {
        return;
    }
    let mut current = state(snapshot, ui);
    if s(&ui.profile, "/_allocation/scope") != s(&current, "/scope") {
        ui.allocations = json!({"intelligence":0,"morality":0,"charm":0,"stamina":0});
    }
    current["elapsed"] = json!(current["elapsed"].as_u64().unwrap_or(0) + u64::from(elapsed));
    if s(&current, "/phase") == "preview" && n(&current, "/elapsed") >= 5001 {
        current["phase"] = json!("allocate");
        current["elapsed"] = json!(0);
    }
    if s(&current, "/phase") == "allocate" {
        if let Some(id) = ui.pressed.as_deref() {
            for key in ["intelligence", "morality", "charm", "stamina"] {
                let delta = if id == format!("allocation-{key}-1") {
                    1
                } else if id == format!("allocation-{key}--1") {
                    -1
                } else {
                    0
                };
                let total = ui
                    .allocations
                    .as_object()
                    .map(|v| v.values().filter_map(Value::as_i64).sum::<i64>())
                    .unwrap_or(0);
                let value = n(&ui.allocations, &format!("/{key}"));
                if delta < 0 && value > 0
                    || delta > 0 && total < i64::from(n(snapshot, "/pending/points"))
                {
                    ui.allocations[key] = json!(value + delta);
                }
            }
        }
    }
    ui.profile["_allocation"] = current;
}
fn number(f: &mut SceneFrame, id: &str, value: impl ToString, x: i32, y: i32, palette: &str) {
    text(
        f,
        id,
        value.to_string(),
        Rect::new(x, y, 80, 16),
        [0, 0, 0, 255],
    );
    let text = f.texts.last_mut().unwrap();
    text.font = TextFont::Original;
    text.centered = true;
    text.palette = Some(palette.into());
    text.palette_index = Some(0);
    text.z = 503;
}
fn ok(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    meta: &Value,
    action: Action,
    enabled: bool,
    ui: &PresentationState,
) {
    let hovered = ui.hover.as_deref() == Some("allocation-confirm");
    let resource = s(meta, if hovered { "/hover" } else { "/resource" });
    let i = sprite(
        f,
        "allocation-confirm",
        resource,
        n(meta, "/x"),
        n(meta, "/y"),
        505,
    );
    let sp = &mut f.sprites[i];
    sp.frame_count = Some(n(&c.media(resource), "/frameCount").max(1) as u32);
    sp.playback = Some(Playback {
        interval_ms: 114,
        once: false,
        max_advances: 1,
        immediate: false,
    });
    control(
        f,
        "allocation-confirm",
        "确定",
        Rect::new(n(meta, "/x"), n(meta, "/y"), 62, 62),
        action,
        &["Enter"],
    );
    f.controls.last_mut().unwrap().enabled = enabled;
    f.controls.last_mut().unwrap().z = 505;
}
pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let metadata = c.native_allocation();
    let local = state(snapshot, ui);
    let preview = s(&local, "/phase") == "preview";
    let meta = &metadata[if preview { "preview" } else { "allocation" }];
    let resource = s(meta, "/resource");
    if resource.is_empty() {
        return;
    }
    let x = n(meta, "/x");
    let y = n(meta, "/y");
    let i = sprite(f, "allocation-panel", resource, x, y, 500);
    f.sprites[i].transparent = Transparency::Index(1);
    if preview {
        let view = &snapshot["nativeView"];
        for (i, value) in [
            n(view, "/battles").to_string(),
            format!("{}%", n(view, "/winPercent")),
            n(view, "/stageTurn").to_string(),
            n(snapshot, "/pending/points").to_string(),
            n(snapshot, "/pending/money").to_string(),
        ]
        .iter()
        .enumerate()
        {
            number(
                f,
                &format!("allocation-preview-{i}"),
                value,
                x + 96,
                y + 29 + i as i32 * 38,
                resource,
            );
        }
        ok(f, c, &meta["ok"], change(&local), true, ui);
    } else {
        let total = ui
            .allocations
            .as_object()
            .map(|v| v.values().filter_map(Value::as_i64).sum::<i64>())
            .unwrap_or(0);
        let remaining = i64::from(n(snapshot, "/pending/points")) - total;
        number(
            f,
            "allocation-remaining",
            remaining,
            x + 105,
            y + 27,
            resource,
        );
        for (row, key) in ["intelligence", "morality", "charm", "stamina"]
            .iter()
            .enumerate()
        {
            number(
                f,
                &format!("allocation-value-{key}"),
                n(&ui.allocations, &format!("/{key}")),
                x + 105,
                y + 65 + row as i32 * 38,
                resource,
            );
        }
        for (index, button_meta) in a(metadata, "/buttons").iter().enumerate() {
            let action = if index == 0 {
                Action::Ui(UiAction::AutoAllocate)
            } else {
                Action::Ui(UiAction::AdjustAllocation {
                    field: ["intelligence", "morality", "charm", "stamina"][(index - 1) % 4].into(),
                    delta: if index <= 4 { 1 } else { -1 },
                })
            };
            let id = if index == 0 {
                "allocation-auto".into()
            } else {
                format!(
                    "allocation-{}-{}",
                    ["intelligence", "morality", "charm", "stamina"][(index - 1) % 4],
                    if index <= 4 { 1 } else { -1 }
                )
            };
            button(
                f,
                &id,
                button_meta,
                action,
                if index == 0 {
                    "自动分配"
                } else if index <= 4 {
                    "增加"
                } else {
                    "减少"
                },
                504,
                ui,
            );
            if let Some(control) = f.controls.last_mut() {
                control.z = 504;
            }
        }
        ok(
            f,
            c,
            &meta["ok"],
            Action::Ui(UiAction::ConfirmAllocation),
            remaining <= 0,
            ui,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> Value {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let read = |path: &str| {
            serde_json::from_str::<Value>(&std::fs::read_to_string(root.join(path)).unwrap())
                .unwrap()
        };
        let mut manifest = read("assets/manifest.json");
        manifest["_nativeAllocation"] = read("assets/catalog/native-allocation.json");
        manifest
    }
    fn snapshot() -> Value {
        json!({"revision":4,"turn":1,"pending":{"kind":"allocation","points":1000,"money":4000,"defaults":{"intelligence":250,"morality":250,"charm":250,"stamina":250}},"nativeView":{"kind":"allocation","battles":7,"winPercent":85,"stageTurn":9}})
    }
    #[test]
    fn original_reward_panels_and_all_nine_controls_match_executable_coordinates() {
        let manifest = catalog();
        let c = SceneContent {
            manifest: &manifest,
            maps: &Value::Null,
            rules: &Value::Null,
        };
        let snapshot = snapshot();
        let mut ui = PresentationState {
            screen: Screen::Game,
            ..Default::default()
        };
        update(&c, &snapshot, &mut ui, 0);
        let mut frame = SceneFrame::default();
        compose(&mut frame, &c, &snapshot, &ui);
        assert_eq!((frame.sprites[0].x, frame.sprites[0].y), (186, 73));
        assert_eq!(frame.texts[0].text, "7");
        assert!(matches!(frame.texts[0].font, TextFont::Original));
        assert_eq!(frame.texts[0].palette_index, Some(0));
        update(&c, &snapshot, &mut ui, 5001);
        let mut frame = SceneFrame::default();
        compose(&mut frame, &c, &snapshot, &ui);
        assert_eq!((frame.sprites[0].x, frame.sprites[0].y), (167, 74));
        assert_eq!(frame.controls.len(), 10);
        assert_eq!(
            frame
                .controls
                .iter()
                .find(|c| c.id == "allocation-auto")
                .unwrap()
                .bounds,
            Rect::new(212, 300, 82, 26)
        );
        assert_eq!(
            frame
                .controls
                .iter()
                .find(|c| c.id == "allocation-intelligence-1")
                .unwrap()
                .bounds,
            Rect::new(238, 136, 21, 21)
        );
        assert_eq!(
            frame
                .controls
                .iter()
                .find(|c| c.id == "allocation-stamina--1")
                .unwrap()
                .bounds,
            Rect::new(358, 250, 21, 21)
        );
        assert!(
            !frame
                .controls
                .iter()
                .find(|c| c.id == "allocation-confirm")
                .unwrap()
                .enabled
        );
        for control in &frame.controls {
            assert!(
                control.bounds.x >= 0
                    && control.bounds.y >= 0
                    && control.bounds.x + control.bounds.width as i32 <= 640
                    && control.bounds.y + control.bounds.height as i32 <= 480
            );
        }
    }
    #[test]
    fn preview_pause_and_held_arrows_keep_native_frame_semantics() {
        let manifest = catalog();
        let c = SceneContent {
            manifest: &manifest,
            maps: &Value::Null,
            rules: &Value::Null,
        };
        let snapshot = snapshot();
        let mut ui = PresentationState {
            screen: Screen::Game,
            ..Default::default()
        };
        update(&c, &snapshot, &mut ui, 2345);
        ui.screen = Screen::Saves;
        update(&c, &snapshot, &mut ui, 60000);
        assert_eq!(ui.profile["_allocation"]["elapsed"], 2345);
        ui = serde_json::from_value(serde_json::to_value(ui).unwrap()).unwrap();
        ui.screen = Screen::Game;
        update(&c, &snapshot, &mut ui, 2656);
        assert_eq!(ui.profile["_allocation"]["phase"], "allocate");
        ui.pressed = Some("allocation-intelligence-1".into());
        update(&c, &snapshot, &mut ui, 16);
        update(&c, &snapshot, &mut ui, 16);
        assert_eq!(ui.allocations["intelligence"], 2);
    }
}
