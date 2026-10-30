//! Housing counter recovered from 004296b0 / 0042a550. The core remains the
//! sole owner of transactions; this module owns the original counter's cursor,
//! introduction, error wait and frame-driven attribute settlement presentation.
use super::*;

fn scope(snap: &Value) -> String {
    format!(
        "housing:{}:{}:{}:{}",
        n(snap, "/revision"),
        n(snap, "/turn"),
        n(snap, "/nativeView/housing/investmentStage"),
        s(snap, "/nativeView/result")
    )
}
fn panel(snap: &Value, ui: &PresentationState) -> Value {
    let key = scope(snap);
    let old = &ui.profile["_nativePanel"];
    if s(old, "/scope") == key {
        return with_error(old.clone(), ui);
    }
    let result = !s(snap, "/nativeView/result").is_empty();
    let purchased = result
        && n(snap, "/nativeView/housing/investmentStage") == 1
        && !b(snap, "/nativeView/previewInvestment");
    let old_stats = if result && s(old, "/scope").starts_with("housing:") {
        old["oldStats"].clone()
    } else {
        snap["stats"].clone()
    };
    with_error(
        json!({"scope":key,"phase":if purchased {"gain"} else if result {"completed"} else {"intro"},
        "selection":if result {n(snap,"/nativeView/housing/style").max(0)} else {0},
        "elapsed":0,"counterElapsed":if result && s(old,"/scope").starts_with("housing:") {old["counterElapsed"].as_u64().unwrap_or(0)} else {0},"oldStats":old_stats,"shownStats":old_stats,
        "gainIndex":0,"gainPhase":"effect","gainDigit":0,"gainX":0,"gainY":230,"error":""}),
        ui,
    )
}
fn with_error(mut state: Value, ui: &PresentationState) -> Value {
    let error = s(&ui.profile, "/_nativeError");
    if !error.is_empty() && s(&state, "/error") != error {
        state["error"] = json!(error);
        state["phase"] = json!("insufficient");
        state["elapsed"] = json!(0);
    }
    state
}
fn change(state: &Value, field: &str, value: Value) -> Action {
    let mut state = state.clone();
    state[field] = value;
    state["elapsed"] = json!(0);
    Action::Ui(UiAction::SetField {
        field: "_nativePanel".into(),
        value: state,
    })
}
fn choose(index: i32) -> Action {
    Action::Game(json!({"type":"chooseOption","index":index}))
}
fn rect(v: &Value) -> Rect {
    Rect::new(
        n(v, "/x"),
        n(v, "/y"),
        n(v, "/width").max(1) as u32,
        n(v, "/height").max(1) as u32,
    )
}
fn moving(f: &mut SceneFrame, id: &str, v: &Value, z: i32, once: bool) {
    if let Some(i) = image(f, id, v, z) {
        let sp = &mut f.sprites[i];
        sp.frame_count = v["frameCount"].as_u64().map(|v| v as u32);
        sp.playback = Some(Playback {
            interval_ms: n(v, "/interval").max(1) as u32,
            once,
            max_advances: 1,
            immediate: true,
        });
    }
}
fn strip(
    f: &mut SceneFrame,
    id: &str,
    v: &Value,
    label: &str,
    action: Action,
    ui: &PresentationState,
    enabled: bool,
    keys: &[&str],
) {
    if let Some(i) = image(f, id, v, 50) {
        let sp = &mut f.sprites[i];
        sp.frame_count = Some(3);
        sp.height = Some(n(v, "/frameHeight").max(1) as u32);
        sp.frame = if !enabled || ui.pressed.as_deref() == Some(id) {
            2
        } else if ui.hover.as_deref() == Some(id) {
            1
        } else {
            0
        };
        let bounds = Rect::new(sp.x, sp.y, sp.width.unwrap_or(59), sp.height.unwrap_or(25));
        control(f, id, label, bounds, action, keys);
        f.controls.last_mut().unwrap().enabled = enabled;
    }
}
fn back(f: &mut SceneFrame, meta: &Value, ui: &PresentationState, index: i32) {
    let v = &meta["back"][usize::from(ui.hover.as_deref() == Some("native-housing-back"))];
    moving(f, "native-housing-back", v, 60, false);
    control(
        f,
        "native-housing-back",
        "离开房产店",
        rect(v),
        choose(index),
        &["Escape"],
    );
    f.controls.last_mut().unwrap().z = 1100;
}
fn body(f: &mut SceneFrame, meta: &Value, message: &str) {
    text(
        f,
        "native-housing-text",
        message,
        rect(&meta["text"]),
        [255; 4],
    );
    let t = f.texts.last_mut().unwrap();
    // The original text surface is blitted before the confirmation buttons.
    t.z = 40;
    t.font = TextFont::Gdi {
        size: 16,
        bold: true,
    };
    t.line_height = 16;
    t.clip = Some(t.bounds);
}
fn hud(f: &mut SceneFrame, meta: &Value, stats: &Value) {
    for key in ["money", "intelligence", "morality", "charm", "stamina"] {
        let v = &meta["hud"][key];
        text(
            f,
            &format!("native-housing-{key}"),
            n(stats, &format!("/{key}")).to_string(),
            Rect::new(n(v, "/x"), n(v, "/y"), 110, 18),
            [255; 4],
        );
        f.texts.last_mut().unwrap().font = TextFont::Gdi {
            size: 16,
            bold: false,
        };
    }
}
fn has_option(view: &Value, index: i32) -> bool {
    a(view, "/options")
        .iter()
        .any(|v| n(v, "/index") == index && !b(v, "/disabled"))
}
fn gain_amount(snap: &Value, state: &Value, entry: &Value) -> i32 {
    let key = s(entry, "/attribute");
    (n(&snap["stats"], &format!("/{key}")) - n(&state["oldStats"], &format!("/{key}"))).max(0)
}

/// Timers advance only in the active Bevy game screen. Neither UI selection nor
/// these frame counters consume the original RNG or alter the domain snapshot.
pub(super) fn update(c: &SceneContent<'_>, snap: &Value, ui: &mut PresentationState, delta: u32) {
    if ui.screen != Screen::Game || s(snap, "/nativeView/kind") != "housing" {
        return;
    }
    let mut state = panel(snap, ui);
    let elapsed = state["elapsed"]
        .as_u64()
        .unwrap_or(0)
        .saturating_add(u64::from(delta));
    state["elapsed"] = json!(elapsed);
    state["counterElapsed"] = json!(
        state["counterElapsed"]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(u64::from(delta))
    );
    let error = s(&ui.profile, "/_nativeError");
    if !error.is_empty() && s(&state, "/error") != error {
        state["error"] = json!(error);
        state["phase"] = json!("insufficient");
        state["elapsed"] = json!(0);
    }
    let meta = c.native_housing();
    if s(&state, "/phase") == "intro" && elapsed > n(meta, "/clocks/introMs").max(1) as u64 {
        state["phase"] = json!("menu");
        state["elapsed"] = json!(0);
    }
    if s(&state, "/phase") == "gain" {
        let order = a(meta, "/gains/order");
        let mut index = n(&state, "/gainIndex").max(0) as usize;
        // A zero bonus has no native animation; skip at most the four known
        // attributes, without a time catch-up loop.
        while index < order.len() && gain_amount(snap, &state, &order[index]) == 0 {
            index += 1;
        }
        state["gainIndex"] = json!(index);
        if index >= order.len() {
            state["phase"] = json!("completed");
            state["elapsed"] = json!(0);
        } else {
            let entry = &order[index];
            let digits = gain_amount(snap, &state, entry).to_string();
            if s(&state, "/gainPhase") == "effect" {
                if ui
                    .runtime
                    .animations
                    .get(&format!("native-housing-gain-effect-{index}"))
                    .is_some_and(|clock| clock.finished)
                {
                    state["gainPhase"] = json!("digits");
                    state["gainDigit"] = json!(0);
                    state["gainX"] = entry["x"].clone();
                    state["gainY"] = json!(230);
                }
            } else {
                let x = (n(&state, "/gainX") - 6).max(n(entry, "/x"));
                let y = (n(&state, "/gainY") - 16).max(n(entry, "/y"));
                state["gainX"] = json!(x);
                state["gainY"] = json!(y);
                if x == n(entry, "/x") && y == n(entry, "/y") {
                    let digit = n(&state, "/gainDigit").max(0) as usize;
                    if let Some(value) = digits.as_bytes().get(digit) {
                        let key = s(entry, "/attribute");
                        let power = 10i32.pow((digits.len() - 1 - digit) as u32);
                        let value = n(&state["shownStats"], &format!("/{key}"))
                            + i32::from(value - b'0') * power;
                        state["shownStats"][key] = json!(value);
                    }
                    let next = digit + 1;
                    if next >= digits.len() {
                        state["gainIndex"] = json!(index + 1);
                        state["gainPhase"] = json!("effect");
                    } else {
                        state["gainDigit"] = json!(next);
                        state["gainX"] = json!(n(entry, "/x") + next as i32 * 30);
                        state["gainY"] = json!(230);
                    }
                }
            }
        }
    }
    ui.profile["_nativePanel"] = state;
}

fn gain(f: &mut SceneFrame, meta: &Value, snap: &Value, state: &Value) {
    image(f, "native-housing-background", &meta["layers"][0], 0);
    let index = n(state, "/gainIndex").max(0) as usize;
    if let Some(entry) = a(meta, "/gains/order").get(index) {
        let amount = gain_amount(snap, state, entry);
        let digits = amount.to_string();
        let moving_digit = n(state, "/gainDigit").max(0) as usize;
        if s(state, "/gainPhase") == "effect" {
            moving(
                f,
                &format!("native-housing-gain-effect-{index}"),
                &entry["effect"],
                20,
                true,
            );
        }
        for (i, value) in digits.bytes().enumerate() {
            if s(state, "/gainPhase") == "digits" && i < moving_digit {
                continue;
            }
            let mut v = meta["gains"]["digits"][usize::from(value - b'0')].clone();
            v["x"] = json!(if s(state, "/gainPhase") == "digits" && i == moving_digit {
                n(state, "/gainX")
            } else {
                n(entry, "/x") + i as i32 * 30
            });
            v["y"] = json!(if s(state, "/gainPhase") == "digits" && i == moving_digit {
                n(state, "/gainY")
            } else {
                230
            });
            image(f, &format!("native-housing-gain-digit-{i}"), &v, 30);
        }
    }
    let mut stats = state["shownStats"].clone();
    stats["money"] = snap["stats"]["money"].clone();
    hud(f, meta, &stats);
    control(
        f,
        "native-housing-settlement-skip",
        "跳过属性动画",
        Rect::new(0, 0, 640, 480),
        choose(0),
        &["Escape"],
    );
}

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snap: &Value,
    ui: &PresentationState,
) -> bool {
    if s(snap, "/nativeView/kind") != "housing" {
        return false;
    }
    let meta = c.native_housing();
    let view = &snap["nativeView"];
    let state = panel(snap, ui);
    let stage = n(view, "/housing/investmentStage");
    let selection = n(&state, "/selection").clamp(0, 3) as usize;
    let phase = s(&state, "/phase");
    f.key = format!(
        "native:housing:{}:{phase}:{selection}:{}:{}",
        scope(snap),
        n(&state, "/gainIndex"),
        s(&state, "/gainPhase")
    );
    f.palette = Some(s(&meta["layers"][0], "/resource").into());
    let preview = if stage == 0 || (stage == 1 && !s(view, "/result").is_empty()) {
        &meta["houses"][selection]
    } else {
        &meta["investments"][((stage - 1).max(0) % 12) as usize]
    };
    let resource = s(preview, "/resource");
    if !resource.is_empty() {
        // Both original calls push end=0x8d, start=2 (0042a6d7 / 0042a725).
        // The FLC's undeclared high entries are black; copying them would erase
        // the counter background/HUD palette at 224..255.
        f.palette_sections.push((
            resource.into(),
            n(meta, "/previewPalette/first") as u8,
            n(meta, "/previewPalette/last") as u8,
        ));
    }
    for part in a(meta, "/paletteSections") {
        f.palette_sections.push((
            s(part, "/resource").into(),
            n(part, "/first") as u8,
            n(part, "/last") as u8,
        ));
    }
    sound(f, "native-housing-music", s(meta, "/music"), true, 0);
    f.sounds.last_mut().unwrap().position_ms = Some(state["counterElapsed"].as_u64().unwrap_or(0));
    if phase == "gain" {
        gain(f, meta, snap, &state);
        return true;
    }
    for (i, v) in a(meta, "/layers").iter().enumerate() {
        image(f, &format!("native-housing-layer-{i}"), v, i as i32);
    }
    if n(&state, "/counterElapsed") % 2100 > 2000 {
        image(f, "native-housing-blink", &meta["blink"], 3);
    }
    moving(f, "native-housing-preview", preview, 10, false);
    if stage > 0 && s(view, "/result").is_empty() {
        if let Some(sp) = f
            .sprites
            .iter_mut()
            .find(|v| v.id == "native-housing-preview")
        {
            sp.frame_count = sp.frame_count.map(|v| v.saturating_sub(1).max(1));
        }
    }
    hud(f, meta, &snap["stats"]);
    let exit = if stage == 0 { 4 } else { 7 };
    if phase == "intro" {
        let message = if b(view, "/specialGreeting") {
            s(meta, "/messages/specialGreeting")
        } else if stage == 0 {
            s(meta, "/messages/greeting")
        } else {
            a(meta, "/stageDescriptions")
                .get((stage - 1).max(0) as usize)
                .and_then(Value::as_str)
                .unwrap_or("")
        };
        body(f, meta, message);
        control(
            f,
            "native-housing-intro",
            "继续房产介绍",
            Rect::new(0, 0, 640, 480),
            change(&state, "phase", json!("menu")),
            &["Enter"],
        );
        f.controls.last_mut().unwrap().z = 900;
        back(f, meta, ui, exit);
    } else if phase == "insufficient" {
        body(f, meta, s(meta, "/messages/insufficient"));
        control(
            f,
            "native-housing-continue",
            "离开资金不足提示",
            Rect::new(0, 0, 640, 480),
            choose(exit),
            &["Enter"],
        );
        f.requests.push(SceneRequest::AfterDelay {
            elapsed_ms: state["elapsed"].as_u64().unwrap_or(0),
            delay_ms: 10000,
            action: choose(exit),
        });
        back(f, meta, ui, exit);
    } else if phase == "completed" {
        body(f, meta, s(meta, "/messages/completed"));
        control(
            f,
            "native-housing-continue",
            "继续房产结算",
            Rect::new(0, 0, 640, 480),
            choose(0),
            &["Enter", "Escape"],
        );
        // Native 0042a550 returns immediately after its blocking settlement.
        // The core then opens the original investment preview when appropriate.
        f.requests.push(SceneRequest::AfterDelay {
            elapsed_ms: state["elapsed"].as_u64().unwrap_or(0),
            delay_ms: 0,
            action: choose(0),
        });
    } else if stage == 0 {
        body(f, meta, s(&meta["houses"][selection], "/description"));
        control(
            f,
            &format!("native-housing-style-{selection}"),
            ["欧洲式住宅", "美国式住宅", "日本式住宅", "中国式住宅"][selection],
            rect(preview),
            change(&state, "selection", json!(selection)),
            &[],
        );
        if selection > 0 {
            strip(
                f,
                "native-housing-prev",
                &meta["previous"],
                "上一种住宅",
                change(&state, "selection", json!(selection - 1)),
                ui,
                true,
                &["ArrowLeft"],
            );
        }
        if selection < 3 {
            strip(
                f,
                "native-housing-next",
                &meta["next"],
                "下一种住宅",
                change(&state, "selection", json!(selection + 1)),
                ui,
                true,
                &["ArrowRight"],
            );
        }
        strip(
            f,
            "native-housing-yes",
            &meta["yes"],
            "确定购买住宅",
            choose(selection as i32),
            ui,
            has_option(view, selection as i32),
            &["Enter"],
        );
        back(f, meta, ui, exit);
    } else {
        body(
            f,
            meta,
            s(
                meta,
                if stage == 1 {
                    "/messages/investOnly"
                } else if stage >= 13 {
                    "/messages/incomeOnly"
                } else {
                    "/messages/investAndIncome"
                },
            ),
        );
        if stage == 1 {
            strip(
                f,
                "native-housing-yes",
                &meta["yes"],
                "确定房产投资",
                choose(5),
                ui,
                has_option(view, 5),
                &["Enter"],
            );
            strip(
                f,
                "native-housing-no",
                &meta["no"],
                "取消房产投资",
                choose(exit),
                ui,
                true,
                &[],
            );
        } else {
            for (id, key, label, index) in [
                ("native-housing-invest", "invest", "继续投资", 5),
                ("native-housing-income", "income", "提取房产收益", 6),
            ] {
                if has_option(view, index) {
                    let v = &meta[key][usize::from(ui.hover.as_deref() == Some(id))];
                    moving(f, id, v, 50, false);
                    control(f, id, label, rect(v), choose(index), &[]);
                }
            }
        }
        back(f, meta, ui, exit);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::OnceLock};
    fn content() -> SceneContent<'static> {
        static DATA: OnceLock<(Value, Value, Value)> = OnceLock::new();
        let (manifest, maps, rules) = DATA.get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let read = |p: &str| {
                serde_json::from_slice::<Value>(&std::fs::read(root.join(p)).unwrap()).unwrap()
            };
            let mut manifest = read("assets/manifest.json");
            manifest["_nativeHousing"] = read("assets/catalog/native-housing.json");
            (
                manifest,
                read("assets/catalog/maps.json"),
                read("assets/catalog/rules.json"),
            )
        });
        SceneContent {
            manifest,
            maps,
            rules,
        }
    }
    fn snap(stage: i32) -> Value {
        let options = if stage == 0 {
            json!([{"index":0},{"index":1},{"index":2},{"index":3},{"index":4}])
        } else if stage == 1 {
            json!([{"index":5},{"index":7}])
        } else if stage == 13 {
            json!([{"index":6},{"index":7}])
        } else {
            json!([{"index":5},{"index":6},{"index":7}])
        };
        json!({"revision":1,"turn":0,"nativeView":{"kind":"housing","housing":{"style":if stage==0{-1}else{0},"investmentStage":stage},"options":options},"stats":{"money":2460,"morality":100,"intelligence":100,"charm":100,"stamina":100}})
    }
    fn ui(snap: &Value) -> PresentationState {
        let mut ui = PresentationState::default();
        ui.screen = Screen::Game;
        ui.profile["_nativePanel"] = panel(snap, &ui);
        ui
    }
    fn render(c: &SceneContent<'_>, s: &Value, ui: &PresentationState) -> SceneFrame {
        let mut f = SceneFrame::default();
        assert!(compose(&mut f, c, s, ui));
        for sp in &f.sprites {
            assert!(
                !c.manifest["resources"][&sp.resource_id].is_null(),
                "Missing {}",
                sp.resource_id
            );
        }
        f
    }
    fn get<'a>(f: &'a SceneFrame, id: &str) -> &'a SceneControl {
        f.controls.iter().find(|c| c.id == id).unwrap()
    }
    fn apply(ui: &mut PresentationState, action: &Action) {
        if let Action::Ui(UiAction::SetField { field, value }) = action {
            ui.profile[field] = value.clone();
        } else {
            panic!("Expected local UI action");
        }
    }
    #[test]
    fn four_original_house_animations_browse_without_issuing_a_transaction() {
        let c = content();
        let snap = snap(0);
        let before = snap.clone();
        let mut ui = ui(&snap);
        let f = render(&c, &snap, &ui);
        apply(&mut ui, &get(&f, "native-housing-intro").action);
        for selection in 0..4 {
            let f = render(&c, &snap, &ui);
            assert!(
                f.palette_sections.iter().all(|(_, _, last)| *last <= 200),
                "The room's unused colors must not erase the original 224..255 counter palette"
            );
            assert!(f.palette_sections.iter().any(|(id, first, last)| id
                == s(
                    &c.native_housing()["houses"][selection as usize],
                    "/resource"
                )
                && *first == 2
                && *last == 141));
            assert_eq!(
                get(&f, &format!("native-housing-style-{selection}")).bounds,
                Rect::new(357, 51, 200, 200)
            );
            assert_eq!(
                get(&f, "native-housing-yes").bounds,
                Rect::new(393, 423, 59, 25)
            );
            assert_eq!(get(&f, "native-housing-yes").action, choose(selection));
            assert_eq!(
                f.controls.iter().any(|v| v.id == "native-housing-prev"),
                selection > 0
            );
            assert_eq!(
                f.controls.iter().any(|v| v.id == "native-housing-next"),
                selection < 3
            );
            if selection < 3 {
                apply(&mut ui, &get(&f, "native-housing-next").action);
            }
        }
        assert_eq!(snap, before);
        let restored: PresentationState =
            serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
        assert_eq!(
            get(&render(&c, &snap, &restored), "native-housing-yes").action,
            choose(3)
        );
    }
    #[test]
    fn insufficient_balance_uses_the_original_message_wait_and_exit_index() {
        let c = content();
        let snap = snap(0);
        let mut ui = ui(&snap);
        ui.profile["_nativeError"] = json!("持有金必须高于房价。");
        update(&c, &snap, &mut ui, 16);
        let f = render(&c, &snap, &ui);
        assert_eq!(s(&ui.profile, "/_nativePanel/phase"), "insufficient");
        assert_eq!(get(&f, "native-housing-continue").action, choose(4));
        assert!(
            f.texts
                .iter()
                .any(|t| t.text == s(c.native_housing(), "/messages/insufficient"))
        );
        assert!(f.requests.iter().any(
            |r| matches!(r,SceneRequest::AfterDelay{delay_ms:10000,action,..} if action==&choose(4))
        ));
        assert!(f.controls.iter().all(|c| !c.id.starts_with("choice-")));
    }
    #[test]
    fn investment_stages_expose_only_original_live_options_and_coordinates() {
        let c = content();
        for stage in 1..=13 {
            let snap = snap(stage);
            let mut ui = ui(&snap);
            ui.profile["_nativePanel"]["phase"] = json!("menu");
            let f = render(&c, &snap, &ui);
            if stage == 1 {
                assert_eq!(get(&f, "native-housing-yes").action, choose(5));
                assert_eq!(get(&f, "native-housing-no").action, choose(7));
            } else {
                assert_eq!(get(&f, "native-housing-income").action, choose(6));
                assert_eq!(
                    get(&f, "native-housing-income").bounds,
                    Rect::new(458, 423, 59, 25)
                );
                assert_eq!(
                    f.controls.iter().any(|v| v.id == "native-housing-invest"),
                    stage < 13
                );
            }
            assert_eq!(get(&f, "native-housing-back").action, choose(7));
        }
    }
    #[test]
    fn native_attribute_digits_advance_one_visible_step_and_survive_a_paused_save() {
        let c = content();
        let mut snap = snap(0);
        let mut ui = ui(&snap);
        ui.profile["_nativePanel"]["phase"] = json!("menu");
        snap["revision"] = json!(2);
        snap["nativeView"]["housing"] = json!({"style":0,"investmentStage":1});
        snap["nativeView"]["result"] = json!("住宅购买成功。");
        for key in ["charm", "morality", "intelligence", "stamina"] {
            snap["stats"][key] = json!(400);
        }
        update(&c, &snap, &mut ui, 16);
        let f = render(&c, &snap, &ui);
        assert!(
            f.sprites
                .iter()
                .any(|s| s.id == "native-housing-gain-effect-0" && s.x == 356 && s.y == 160)
        );
        ui.runtime.animations.insert(
            "native-housing-gain-effect-0".into(),
            AnimationClock {
                finished: true,
                ..Default::default()
            },
        );
        update(&c, &snap, &mut ui, 60000);
        assert_eq!(n(&ui.profile, "/_nativePanel/gainY"), 230);
        update(&c, &snap, &mut ui, 60000);
        assert_eq!(n(&ui.profile, "/_nativePanel/gainY"), 214);
        ui.screen = Screen::Saves;
        let serialized = serde_json::to_value(&ui).unwrap();
        update(&c, &snap, &mut ui, 60000);
        assert_eq!(serde_json::to_value(&ui).unwrap(), serialized);
        let mut restored: PresentationState = serde_json::from_value(serialized).unwrap();
        restored.screen = Screen::Game;
        update(&c, &snap, &mut restored, 16);
        assert_eq!(n(&restored.profile, "/_nativePanel/gainY"), 198);
        assert_eq!(n(&restored.profile, "/_nativePanel/shownStats/charm"), 100);
        assert_eq!(n(&snap, "/stats/charm"), 400);
    }
}
