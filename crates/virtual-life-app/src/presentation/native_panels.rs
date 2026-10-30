//! Original native storefronts and career selector. Catalog coordinates come
//! from the fixed executable; the view only projects the core's real choices.
use super::*;

fn scope(snapshot: &Value) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        s(snapshot, "/nativeView/kind"),
        n(snapshot, "/revision"),
        n(snapshot, "/turn"),
        s(snapshot, "/nativeView/mode"),
        s(snapshot, "/nativeView/outcome")
    )
}
fn panel(snapshot: &Value, ui: &PresentationState) -> Value {
    let scope = scope(snapshot);
    if s(&ui.profile, "/_nativePanel/scope") == scope {
        return ui.profile["_nativePanel"].clone();
    }
    let mut state = json!({"scope":scope,"selection":-1,"tab":-1,"scroll":0,"elapsed":0,"purchases":n(snapshot,"/nativeView/purchases"),
        "phase":match s(snapshot,"/nativeView/kind") {"career"=>"intro","itemShop"=>"greeting","equipmentWorkshop" if s(snapshot,"/nativeView/mode")=="purchase"=>"intro",_=>"menu"}});
    let old = &ui.profile["_nativePanel"];
    let selection = selected(old);
    if s(snapshot, "/nativeView/kind") == "itemShop"
        && selection >= 0
        && s(old, "/scope").starts_with("itemShop:")
        && a(snapshot, "/nativeView/offers")
            .iter()
            .any(|v| n(v, "/index") == selection && b(v, "/purchased"))
    {
        state["tab"] = old["tab"].clone();
        state["selection"] = json!(selection);
        state["phase"] = json!("result");
    }
    state
}
fn change(state: &Value, field: &str, value: Value) -> Action {
    let mut state = state.clone();
    state[field] = value;
    state["elapsed"] = json!(0);
    if field == "selection" && selected(&state) < 0 && s(&state, "/phase") == "result" {
        state["phase"] = json!("menu");
    }
    Action::Ui(UiAction::SetField {
        field: "_nativePanel".into(),
        value: state,
    })
}
fn choose(index: i32) -> Action {
    Action::Game(json!({"type":"chooseOption","index":index}))
}
fn selected(state: &Value) -> i32 {
    state["selection"].as_i64().unwrap_or(-1) as i32
}
fn hovered(snapshot: &Value, ui: &PresentationState) -> i32 {
    if s(&ui.profile, "/_nativeHover/scope") == scope(snapshot) {
        ui.profile["_nativeHover"]["index"].as_i64().unwrap_or(-1) as i32
    } else {
        -1
    }
}
fn hover(f: &mut SceneFrame, snapshot: &Value, index: i32) {
    let control = f.controls.last_mut().unwrap();
    control.on_hover = Some(Action::Ui(UiAction::SetField {
        field: "_nativeHover".into(),
        value: json!({"scope":scope(snapshot),"index":index}),
    }));
    control.on_leave = Some(Action::Ui(UiAction::SetField {
        field: "_nativeHover".into(),
        value: Value::Null,
    }));
}
fn rect(v: &Value) -> Rect {
    Rect::new(
        n(v, "/x"),
        n(v, "/y"),
        n(v, "/width").max(1) as u32,
        n(v, "/height").max(1) as u32,
    )
}
fn layer(f: &mut SceneFrame, id: &str, v: &Value, z: i32) {
    image(f, id, v, z);
}
fn moving(f: &mut SceneFrame, id: &str, v: &Value, z: i32, animate: bool, once: bool) {
    if let Some(index) = image(f, id, v, z) {
        let sp = &mut f.sprites[index];
        sp.frame_count = v["frameCount"].as_u64().map(|v| v as u32);
        if animate {
            sp.playback = Some(Playback {
                interval_ms: n(v, "/interval").max(1) as u32,
                once,
                max_advances: 1,
                immediate: true,
            });
        }
    }
}
fn strip(
    f: &mut SceneFrame,
    id: &str,
    v: &Value,
    action: Action,
    label: &str,
    ui: &PresentationState,
    disabled: bool,
) {
    let index = image(f, id, v, 30).expect("Native button metadata has a resource");
    let sp = &mut f.sprites[index];
    sp.frame_count = Some(3);
    sp.height = Some(
        v["frameHeight"]
            .as_u64()
            .unwrap_or(n(v, "/height").max(3) as u64 / 3) as u32,
    );
    sp.frame = if disabled || ui.pressed.as_deref() == Some(id) {
        2
    } else if ui.hover.as_deref() == Some(id) {
        1
    } else {
        0
    };
    let bounds = Rect::new(sp.x, sp.y, sp.width.unwrap_or(51), sp.height.unwrap_or(51));
    control(f, id, label, bounds, action, &[]);
    f.controls.last_mut().unwrap().enabled = !disabled;
}
fn body(f: &mut SceneFrame, common: &Value, message: impl Into<String>) {
    text(f, "native-text", message, rect(&common["text"]), [255; 4]);
    let text = f.texts.last_mut().unwrap();
    text.z = 20;
    text.font = TextFont::Gdi {
        size: 16,
        bold: true,
    };
    text.line_height = 20;
    text.clip = Some(text.bounds);
}
fn delay(f: &mut SceneFrame, state: &Value, milliseconds: u64, action: Action) {
    f.requests.push(SceneRequest::AfterDelay {
        elapsed_ms: state["elapsed"].as_u64().unwrap_or(0),
        delay_ms: milliseconds,
        action,
    });
}
fn error_message<'a>(ui: &'a PresentationState) -> &'a str {
    s(&ui.profile, "/_nativeError")
}
fn number(f: &mut SceneFrame, id: &str, value: i32, x: i32, y: i32, color: [u8; 4]) {
    text(f, id, value.to_string(), Rect::new(x, y, 100, 18), color);
    f.texts.last_mut().unwrap().font = TextFont::Gdi {
        size: 16,
        bold: false,
    };
}
fn hud(f: &mut SceneFrame, snapshot: &Value, kind: &str) {
    let (money_y, stats_y) = match kind {
        "itemShop" => (3, 24),
        "career" => (46, 17),
        _ => (17, 46),
    };
    number(
        f,
        "native-money",
        n(snapshot, "/stats/money"),
        77,
        money_y,
        [255; 4],
    );
    if kind == "statShop" || kind == "itemShop" {
        number(
            f,
            "native-hp",
            n(
                snapshot,
                if kind == "statShop" {
                    "/life/affection"
                } else {
                    "/stats/hp"
                },
            ),
            if kind == "statShop" { 251 } else { 229 },
            money_y,
            [255; 4],
        );
    }
    for (key, x) in [
        ("intelligence", 77),
        ("morality", 229),
        ("charm", 379),
        ("stamina", 530),
    ] {
        number(
            f,
            &format!("native-{key}"),
            n(snapshot, &format!("/stats/{key}")),
            x,
            stats_y,
            [255; 4],
        );
    }
}
fn base(f: &mut SceneFrame, meta: &Value, state: &Value) {
    for (i, v) in a(meta, "/layers").iter().enumerate() {
        layer(f, &format!("native-layer-{i}"), v, i as i32);
    }
    if let Some(first) = a(meta, "/layers").first() {
        f.palette = Some(s(first, "/resource").into());
    }
    if n(state, "/elapsed") % 2100 > 2000 {
        layer(f, "native-blink", &meta["blink"], 3);
    }
    for entry in a(meta, "/paletteSections") {
        f.palette_sections.push((
            s(entry, "/resource").into(),
            n(entry, "/first") as u8,
            n(entry, "/last") as u8,
        ));
    }
    sound(f, "native-music", s(meta, "/music"), true, 0);
}
fn back(f: &mut SceneFrame, meta: &Value, ui: &PresentationState, exit: i32) {
    let active = ui.hover.as_deref() == Some("native-back");
    let v = &meta[usize::from(active)];
    moving(f, "native-back", v, 60, true, false);
    control(f, "native-back", "离开", rect(v), choose(exit), &["Escape"]);
}
fn confirm(
    f: &mut SceneFrame,
    common: &Value,
    state: &Value,
    ui: &PresentationState,
    index: i32,
    disabled: bool,
) {
    strip(
        f,
        "native-yes",
        &common["yes"],
        choose(index),
        "确定购买",
        ui,
        disabled,
    );
    strip(
        f,
        "native-no",
        &common["no"],
        change(state, "selection", json!(-1)),
        "取消",
        ui,
        false,
    );
}

/// Presentation timers have no access to the shared gameplay RNG or save data.
pub(super) fn update(
    _c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    elapsed_ms: u32,
) {
    if ui.screen != Screen::Game {
        return;
    }
    if !matches!(
        s(snapshot, "/nativeView/kind"),
        "statShop" | "itemShop" | "equipmentWorkshop" | "career"
    ) {
        return;
    }
    let mut state = panel(snapshot, ui);
    if s(snapshot, "/nativeView/kind") == "itemShop"
        && s(&ui.profile, "/_nativePanel/scope") != scope(snapshot)
    {
        let old = &ui.profile["_nativePanel"];
        let selection = selected(old);
        if selection >= 0
            && s(old, "/scope").starts_with("itemShop:")
            && a(snapshot, "/nativeView/offers")
                .iter()
                .any(|v| n(v, "/index") == selection && b(v, "/purchased"))
        {
            state["tab"] = old["tab"].clone();
            state["selection"] = json!(selection);
            state["phase"] = json!("result");
        }
    }
    let elapsed = state["elapsed"].as_u64().unwrap_or(0) + u64::from(elapsed_ms);
    state["elapsed"] = json!(elapsed);
    let next = match (s(snapshot, "/nativeView/kind"), s(&state, "/phase")) {
        ("career", "intro") if elapsed >= 10000 => Some("menu"),
        ("itemShop", "greeting") if elapsed >= 12000 => Some("menu"),
        ("equipmentWorkshop", "intro") if elapsed >= 10000 => Some("description"),
        ("equipmentWorkshop", "description") if elapsed >= 10000 => Some("confirm"),
        ("career", "accepted") if elapsed >= 10000 => Some("transition"),
        _ => None,
    };
    if let Some(next) = next {
        state["phase"] = json!(next);
        state["elapsed"] = json!(0);
    }
    if s(snapshot, "/nativeView/kind") == "career" && s(&state, "/phase") == "transition" {
        state["actorY"] = json!((n(&state, "/actorY") + 20).min(100));
    }
    ui.profile["_nativePanel"] = state;
}

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) -> bool {
    let kind = s(snapshot, "/nativeView/kind");
    if !matches!(
        kind,
        "statShop" | "itemShop" | "equipmentWorkshop" | "career"
    ) {
        return false;
    }
    let state = panel(snapshot, ui);
    let catalog = c.native_ui();
    let view = &snapshot["nativeView"];
    f.key = format!(
        "native:{kind}:{}:{}:{}:{}",
        scope(snapshot),
        s(&state, "/phase"),
        selected(&state),
        n(&state, "/tab")
    );
    match kind {
        "statShop" => stat_shop(
            f,
            c,
            snapshot,
            ui,
            &catalog["common"],
            &catalog[kind],
            view,
            &state,
        ),
        "itemShop" => item_shop(
            f,
            c,
            snapshot,
            ui,
            &catalog["common"],
            &catalog[kind],
            view,
            &state,
        ),
        "equipmentWorkshop" => equipment(
            f,
            c,
            snapshot,
            ui,
            &catalog["common"],
            &catalog[kind],
            view,
            &state,
        ),
        "career" => career(
            f,
            c,
            snapshot,
            ui,
            &catalog["common"],
            &catalog[kind],
            view,
            &state,
        ),
        _ => unreachable!(),
    };
    // The native bitmap constructor uses LoadImageA -> StretchBlt into the
    // construction-time display palette. Stored BMP indices are not surface
    // indices; later FLC palette changes must recolor the converted indices.
    apply_gdi(f, &catalog[kind]["gdiIndexMaps"]);
    true
}
fn apply_gdi(f: &mut SceneFrame, maps: &Value) {
    for sprite in &mut f.sprites {
        if let Some(mapping) = maps[&sprite.resource_id].as_array() {
            sprite.index_map = mapping
                .iter()
                .map(|pair| [n(pair, "/0") as u8, n(pair, "/1") as u8])
                .collect();
        }
    }
}
fn stat_shop(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
    common: &Value,
    meta: &Value,
    view: &Value,
    state: &Value,
) {
    base(f, meta, state);
    hud(f, snapshot, "statShop");
    let selection = selected(state);
    let over = hovered(snapshot, ui);
    for (i, offer) in a(view, "/offers").iter().enumerate() {
        let mut v = meta["slots"][i].clone();
        v["resource"] = json!(c.resolve(s(offer, "/resource")));
        v["height"] = json!(153);
        v["frameHeight"] = json!(51);
        strip(
            f,
            &format!("native-stat-{i}"),
            &v,
            change(state, "selection", json!(n(offer, "/index"))),
            s(offer, "/name"),
            ui,
            b(offer, "/disabled") || selection >= 0 && selection != n(offer, "/index"),
        );
        hover(f, snapshot, n(offer, "/index"));
    }
    let entry = a(view, "/offers")
        .iter()
        .find(|v| n(v, "/index") == if selection >= 0 { selection } else { over });
    let message = if !error_message(ui).is_empty() {
        s(meta, "/messages/insufficient").into()
    } else if n(view, "/purchases") >= 3 {
        s(meta, "/messages/completed").into()
    } else if selection >= 0 {
        s(meta, "/messages/confirm").into()
    } else if let Some(entry) = entry {
        let resource = c.resolve(s(entry, "/resource"));
        let description = if n(snapshot, "/life/stage") <= 3 {
            meta["descriptions"]["young"][&resource]
                .as_str()
                .unwrap_or("")
        } else {
            meta["descriptions"]["adult"][&resource]
                [usize::from(s(snapshot, "/player/sex") == "female")]
            .as_str()
            .unwrap_or("")
        };
        format!(
            "{}{}{}{}{}{}{}{}",
            s(meta, "/messages/select"),
            s(entry, "/name"),
            s(meta, "/messages/separator"),
            s(meta, "/messages/price"),
            n(entry, "/price"),
            s(meta, "/messages/currency"),
            s(meta, "/messages/separator"),
            description
        )
    } else {
        s(meta, "/messages/greeting").into()
    };
    body(f, common, message);
    if let Some(entry) = entry.filter(|_| selection >= 0) {
        confirm(f, common, state, ui, selection, b(entry, "/disabled"));
    }
    if n(view, "/purchases") >= 3 {
        delay(f, state, 2000, choose(n(view, "/exitIndex")));
    }
    back(f, &common["back"], ui, n(view, "/exitIndex"));
}
fn item_shop(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
    common: &Value,
    meta: &Value,
    view: &Value,
    state: &Value,
) {
    base(f, meta, state);
    hud(f, snapshot, "itemShop");
    let tab = state["tab"].as_i64().unwrap_or(-1) as i32;
    let selection = selected(state);
    let over = hovered(snapshot, ui);
    if s(state, "/phase") == "greeting" {
        control(
            f,
            "native-item-enter",
            "继续",
            Rect::new(0, 0, 640, 480),
            change(state, "phase", json!("menu")),
            &["Enter", "Space"],
        );
    }
    for v in a(meta, "/tabs") {
        let category = n(v, "/category");
        let mut next = state.clone();
        next["tab"] = json!(category);
        next["selection"] = json!(-1);
        next["phase"] = json!("menu");
        next["elapsed"] = json!(0);
        strip(
            f,
            &format!("native-category-{category}"),
            v,
            Action::Ui(UiAction::SetField {
                field: "_nativePanel".into(),
                value: next,
            }),
            ["家具", "战斗装备领取", "普通道具", "职业道具"][category as usize],
            ui,
            false,
        );
    }
    let result = s(state, "/phase") == "result";
    let offers: Vec<_> = a(view, "/offers")
        .iter()
        .filter(|v| {
            n(v, "/category") == tab
                && (!b(v, "/purchased") || result && n(v, "/index") == selection)
        })
        .collect();
    if let Some(first) = offers.first() {
        let resource = if tab == 1 {
            s(
                &meta["equipmentAnimations"][n(first, "/catalogIndex") as usize],
                "/resource",
            )
            .to_owned()
        } else {
            c.resolve(s(first, "/resource"))
        };
        f.palette_sections.push((resource, 2, 141));
    }
    for (i, offer) in offers.iter().take(3).enumerate() {
        let index = n(offer, "/index");
        if selection >= 0 && selection != index {
            continue;
        }
        let mut media = meta["slots"][i].clone();
        let resource = if tab == 1 {
            s(
                &meta["equipmentAnimations"][n(offer, "/catalogIndex") as usize],
                "/resource",
            )
            .to_owned()
        } else {
            c.resolve(s(offer, "/resource"))
        };
        let raw = c.media(&resource);
        media["resource"] = json!(resource);
        media["frameCount"] = raw["frames"].clone();
        media["interval"] = raw["speedRaw"].clone();
        media["transparent"] = json!("firstPixel");
        moving(
            f,
            &format!("native-item-{index}"),
            &media,
            20,
            selection == index || over == index,
            false,
        );
        if !result {
            control(
                f,
                &format!("native-item-{index}"),
                s(offer, "/description"),
                rect(&meta["slots"][i]),
                change(state, "selection", json!(index)),
                &[],
            );
            hover(f, snapshot, index);
        }
    }
    let entry = offers
        .iter()
        .find(|v| n(v, "/index") == if selection >= 0 { selection } else { over });
    body(
        f,
        common,
        if !error_message(ui).is_empty() {
            error_message(ui)
        } else if result {
            s(meta, "/purchasedText")
        } else if selection >= 0 {
            if tab == 1 {
                s(meta, "/retrieveText")
            } else {
                s(meta, "/confirmText")
            }
        } else if let Some(entry) = entry {
            s(entry, "/description")
        } else {
            s(meta, "/greeting")
        },
    );
    f.texts.last_mut().unwrap().color = [100, 255, 255, 255];
    if result {
        delay(f, state, 2000, change(state, "selection", json!(-1)));
        control(
            f,
            "native-item-result-continue",
            "继续",
            Rect::new(128, 371, 400, 100),
            change(state, "selection", json!(-1)),
            &["Enter", "Space"],
        );
    } else if let Some(entry) = entry.filter(|_| selection >= 0) {
        confirm(f, common, state, ui, selection, !b(entry, "/available"));
    }
    back(f, &common["back"], ui, n(view, "/exitIndex"));
}
fn equipment(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
    common: &Value,
    meta: &Value,
    view: &Value,
    state: &Value,
) {
    base(f, meta, state);
    text(
        f,
        "native-equipment-money",
        n(snapshot, "/stats/money").to_string(),
        Rect::new(83, 17, 150, 20),
        [255; 4],
    );
    let mode = s(view, "/mode");
    let selection = selected(state);
    let over = hovered(snapshot, ui);
    let phase = s(state, "/phase");
    if mode.is_empty() {
        for (i, v) in a(meta, "/modes").iter().enumerate() {
            strip(
                f,
                &format!("native-equipment-mode-{i}"),
                v,
                choose(i as i32),
                if i == 0 {
                    "购买装备"
                } else {
                    "研究装备"
                },
                ui,
                false,
            );
        }
        body(f, common, s(meta, "/messages/greeting"));
    } else if mode == "purchase" {
        let id = n(view, "/offeredId").max(0) as usize;
        f.palette_sections
            .push((s(&meta["animations"][id], "/resource").into(), 2, 161));
        moving(
            f,
            &format!("native-equipment-offer-{id}"),
            &meta["animations"][id],
            10,
            true,
            false,
        );
        let outcome = s(view, "/outcome");
        if !outcome.is_empty() {
            body(f, common, s(meta, &format!("/messages/{outcome}")));
            control(
                f,
                "native-equipment-continue",
                "继续",
                Rect::new(128, 371, 400, 100),
                choose(n(view, "/continueIndex")),
                &["Enter", "Space"],
            );
            delay(f, state, 10000, choose(n(view, "/continueIndex")));
        } else if phase == "confirm" {
            body(f, common, s(meta, "/messages/purchase"));
            strip(
                f,
                "native-yes",
                &common["yes"],
                choose(0),
                "购买装备",
                ui,
                false,
            );
            strip(f, "native-no", &common["no"], choose(1), "离开", ui, false);
        } else {
            body(
                f,
                common,
                if phase == "intro" {
                    s(meta, "/messages/greeting")
                } else {
                    s(view, "/offered/description")
                },
            );
            control(
                f,
                "native-equipment-description",
                "继续",
                Rect::new(0, 0, 640, 480),
                change(
                    state,
                    "phase",
                    json!(if phase == "intro" {
                        "description"
                    } else {
                        "confirm"
                    }),
                ),
                &["Enter", "Space"],
            );
        }
    } else {
        layer(
            f,
            "native-research-background",
            &meta["researchBackground"],
            0,
        );
        f.palette_sections
            .push((s(meta, "/research/resource").into(), 2, 161));
        moving(
            f,
            "native-research-machine",
            &meta["research"],
            10,
            true,
            false,
        );
        let outcome = s(view, "/outcome");
        for (i, entry) in a(view, "/entries").iter().take(8).enumerate() {
            let mut v = meta["slots"][i].clone();
            v["resource"] = json!(c.resolve(s(entry, "/resource")));
            v["height"] = json!(153);
            v["frameHeight"] = json!(51);
            strip(
                f,
                &format!("native-research-{}", n(entry, "/index")),
                &v,
                change(state, "selection", entry["index"].clone()),
                s(entry, "/name"),
                ui,
                b(entry, "/disabled") || selection >= 0 && selection != n(entry, "/index"),
            );
            hover(f, snapshot, n(entry, "/index"));
        }
        let entry = a(view, "/entries")
            .iter()
            .find(|e| n(e, "/index") == if selection >= 0 { selection } else { over });
        let description = entry.map(|entry| {
            format!(
                "{}{}{}{}{}",
                s(meta, "/messages/researchItem"),
                s(entry, "/name"),
                s(meta, "/messages/itemSeparator"),
                s(meta, "/messages/researchPrice"),
                n(entry, "/researchPrice")
            )
        });
        body(
            f,
            common,
            if !outcome.is_empty() {
                s(meta, &format!("/messages/{outcome}"))
            } else if selection >= 0 {
                s(meta, "/messages/confirmResearch")
            } else if let Some(description) = &description {
                description
            } else {
                s(meta, "/messages/research")
            },
        );
        if !outcome.is_empty() {
            if outcome == "upgraded" {
                moving(
                    f,
                    "native-research-result",
                    &meta["resultAnimation"],
                    11,
                    true,
                    true,
                );
            }
            control(
                f,
                "native-equipment-continue",
                "继续",
                Rect::new(128, 371, 400, 100),
                choose(n(view, "/continueIndex")),
                &["Enter", "Space"],
            );
            delay(f, state, 10000, choose(n(view, "/continueIndex")));
        } else if let Some(entry) = entry.filter(|_| selection >= 0) {
            confirm(f, common, state, ui, selection, b(entry, "/disabled"));
        }
    }
    if let Some(exit) = view["exitIndex"]
        .as_i64()
        .or_else(|| view["continueIndex"].as_i64())
    {
        back(f, &common["back"], ui, exit as i32);
    }
}
fn career(
    f: &mut SceneFrame,
    _c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
    common: &Value,
    meta: &Value,
    view: &Value,
    state: &Value,
) {
    layer(f, "native-career-background", &meta["background"], 0);
    f.palette = Some(s(meta, "/background/resource").into());
    hud(f, snapshot, "career");
    sound(f, "native-music", s(meta, "/music"), true, 0);
    let selection = selected(state);
    let over = hovered(snapshot, ui);
    let sex = usize::from(s(snapshot, "/player/sex") == "female");
    f.palette_sections.push((
        s(&meta["entries"][0]["actors"][sex], "/resource").into(),
        2,
        141,
    ));
    f.palette_sections
        .push(("install/public/button/yes_a.flc".into(), 163, 200));
    if s(state, "/phase") == "transition" && selection >= 0 {
        f.sprites.clear();
        f.texts.clear();
        f.palette_sections.clear();
        f.palette = Some(s(meta, "/transitionIn/resource").into());
        moving(
            f,
            "native-career-transition-in",
            &meta["transitionIn"],
            0,
            true,
            true,
        );
        let mut actor = meta["entries"][selection as usize]["actors"][sex].clone();
        actor["x"] = json!(260);
        actor["y"] = json!(n(state, "/actorY"));
        moving(f, "native-career-transition-actor", &actor, 1, true, false);
        moving(
            f,
            "native-career-transition-out",
            &meta["transitionOut"],
            2,
            true,
            true,
        );
        if ui
            .runtime
            .animations
            .get("native-career-transition-in")
            .is_some_and(|clock| clock.frame + 1 >= n(&meta["transitionIn"], "/frameCount") as u32)
        {
            f.requests.push(SceneRequest::AfterDelay {
                elapsed_ms: 0,
                delay_ms: 0,
                action: choose(selection),
            });
        }
        return;
    }
    if s(state, "/phase") == "intro" {
        moving(f, "native-career-intro", &meta["intro"], 10, true, true);
        layer(f, "native-career-detail", &meta["defaultDetail"], 11);
        body(f, common, s(meta, "/messages/greeting"));
        control(
            f,
            "native-career-enter",
            "继续",
            Rect::new(0, 0, 640, 480),
            change(state, "phase", json!("menu")),
            &["Enter", "Space"],
        );
        return;
    }
    let entries: Vec<_> = a(view, "/careers")
        .iter()
        .filter(|e| n(e, "/id") != n(snapshot, "/life/profession"))
        .collect();
    let start = n(state, "/scroll")
        .max(0)
        .min((entries.len() as i32 - 10).max(0)) as usize;
    for (row, entry) in entries.iter().skip(start).take(10).enumerate() {
        let index = n(entry, "/index");
        let mut v = meta["entries"][index as usize]["label"].clone();
        v["x"] = meta["rows"][row]["x"].clone();
        v["y"] = meta["rows"][row]["y"].clone();
        let i = image(f, &format!("native-career-label-{index}"), &v, 20).unwrap();
        let sp = &mut f.sprites[i];
        sp.frame_count = Some(3);
        sp.height = Some(n(&v, "/height") as u32 / 3);
        sp.frame = if over == index || selection == index {
            1
        } else if b(entry, "/disabled") {
            2
        } else {
            0
        };
        control(
            f,
            &format!("native-career-{index}"),
            s(entry, "/name"),
            rect(&meta["rows"][row]),
            change(state, "selection", json!(index)),
            &[],
        );
        hover(f, snapshot, index);
        if n(snapshot, &format!("/life/skills/{index}")) == 5 {
            let mut star = meta["marker"].clone();
            star["x"] = v["x"].clone();
            star["y"] = v["y"].clone();
            let i = image(f, &format!("native-career-star-{index}"), &star, 22).unwrap();
            f.sprites[i].frame_count = Some(2);
            f.sprites[i].height = Some(17);
        }
    }
    let scroll = &meta["scroll"];
    let parts = a(scroll, "/parts");
    let x = n(scroll, "/x");
    let y = n(scroll, "/y");
    for (id, part, top, delta) in [
        ("up", 0, y, -1),
        (
            "down",
            2,
            y + n(&parts[0], "/height") + n(&parts[4], "/height"),
            1,
        ),
    ] {
        let mut v = parts[part
            + usize::from(ui.pressed.as_deref() == Some(&format!("native-career-scroll-{id}")))]
        .clone();
        v["x"] = json!(x);
        v["y"] = json!(top);
        layer(f, &format!("native-career-scroll-{id}"), &v, 25);
        control(
            f,
            &format!("native-career-scroll-{id}"),
            if delta < 0 {
                "上一项职业"
            } else {
                "下一项职业"
            },
            rect(&v),
            change(
                state,
                "scroll",
                json!((start as i32 + delta).clamp(0, (entries.len() as i32 - 10).max(0))),
            ),
            &[if delta < 0 { "ArrowUp" } else { "ArrowDown" }],
        );
    }
    let mut track = parts[4].clone();
    track["x"] = json!(x);
    track["y"] = json!(y + n(&parts[0], "/height"));
    layer(f, "native-career-track", &track, 24);
    let range = (n(&parts[4], "/height") - n(&parts[5], "/height")).max(0);
    let mut thumb = parts[5].clone();
    thumb["x"] = json!(x);
    thumb["y"] = json!(n(&track, "/y") + start as i32 * range / (entries.len() as i32 - 10).max(1));
    layer(f, "native-career-thumb", &thumb, 26);
    if let Some(entry) = entries.iter().find(|e| n(e, "/index") == selection) {
        let e = &meta["entries"][selection as usize];
        layer(f, "native-career-detail", &e["detail"], 5);
        moving(
            f,
            &format!("native-career-actor-{selection}"),
            &e["actors"][sex],
            6,
            true,
            false,
        );
        let mut y = if selection == 21 || selection == 22 {
            168
        } else {
            173
        };
        let step = if selection == 21 || selection == 22 {
            21
        } else {
            25
        };
        for key in [
            "intelligence",
            "morality",
            "charm",
            "stamina",
            "money",
            "baseSalary",
        ] {
            let value = n(entry, &format!("/{key}"));
            if value != 0 {
                number(
                    f,
                    &format!("native-career-requirement-{key}"),
                    value,
                    448,
                    y,
                    [255, 255, 0, 255],
                );
                y += step;
            }
        }
        text(
            f,
            "native-career-effect",
            s(e, "/description"),
            Rect::new(310, 276, 220, 30),
            [0, 0, 0, 255],
        );
        body(
            f,
            common,
            if s(state, "/phase") == "accepted" {
                s(meta, "/messages/selected")
            } else if b(entry, "/eligible") {
                s(meta, "/messages/confirm")
            } else {
                s(meta, "/messages/insufficient")
            },
        );
        if s(state, "/phase") == "accepted" {
            control(
                f,
                "native-career-accepted",
                "继续",
                Rect::new(128, 371, 400, 100),
                change(state, "phase", json!("transition")),
                &["Enter", "Space"],
            );
        } else {
            strip(
                f,
                "native-yes",
                &common["yes"],
                change(state, "phase", json!("accepted")),
                "确定职业",
                ui,
                !b(entry, "/eligible"),
            );
            strip(
                f,
                "native-no",
                &common["no"],
                change(state, "selection", json!(-1)),
                "取消",
                ui,
                false,
            );
        }
    } else {
        layer(f, "native-career-detail", &meta["defaultDetail"], 5);
        body(f, common, s(meta, "/messages/greeting"));
    }
    if s(state, "/phase") == "accepted" {
        for control in &mut f.controls {
            if control.id != "native-career-accepted" {
                control.enabled = false;
            }
        }
    }
    back(
        f,
        &meta["exit"],
        ui,
        if s(state, "/phase") == "accepted" {
            selection
        } else {
            n(view, "/exitIndex")
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::OnceLock};
    fn content() -> SceneContent<'static> {
        static DATA: OnceLock<(Value, Value, Value)> = OnceLock::new();
        let (manifest, maps, rules) = DATA.get_or_init(|| {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let read = |file: &str| {
                serde_json::from_slice::<Value>(&std::fs::read(root.join(file)).unwrap()).unwrap()
            };
            let mut manifest = read("assets/manifest.json");
            manifest["_nativeUi"] = read("assets/catalog/native-ui.json");
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
    fn snapshot(kind: &str) -> Value {
        json!({"revision":1,"turn":0,"nativeView":{"kind":kind},"life":{"stage":2,"profession":-1,"skills":vec![0;24]},"player":{"sex":"male"},"stats":{"money":20000,"hp":100,"intelligence":100,"morality":100,"stamina":100,"charm":100}})
    }
    fn view_ui(snapshot: &Value) -> PresentationState {
        let mut ui = PresentationState::default();
        ui.screen = Screen::Game;
        ui.profile["_nativePanel"] = panel(snapshot, &ui);
        ui
    }
    fn field(ui: &mut PresentationState, action: &Action) {
        match action {
            Action::Ui(UiAction::SetField { field, value }) => ui.profile[field] = value.clone(),
            _ => panic!("Expected presentation-only selection"),
        }
    }
    fn render(c: &SceneContent<'_>, snapshot: &Value, ui: &PresentationState) -> SceneFrame {
        let mut frame = SceneFrame::default();
        assert!(compose(&mut frame, c, snapshot, ui));
        for control in &frame.controls {
            assert!(
                control.bounds.x >= 0
                    && control.bounds.y >= 0
                    && control.bounds.x + control.bounds.width as i32 <= 640
                    && control.bounds.y + control.bounds.height as i32 <= 480,
                "Native hitbox is outside canvas: {} {:?}",
                control.id,
                control.bounds
            );
        }
        for sprite in &frame.sprites {
            assert!(
                c.manifest["resources"].get(&sprite.resource_id).is_some(),
                "Missing native artwork {}",
                sprite.resource_id
            );
            if let Some(expected) = c.native_ui()[s(snapshot, "/nativeView/kind")]["gdiIndexMaps"]
                .get(&sprite.resource_id)
            {
                assert_eq!(
                    serde_json::to_value(&sprite.index_map).unwrap(),
                    *expected,
                    "Native GDI constructor mapping was not applied: {}",
                    sprite.resource_id
                );
            }
        }
        frame
    }
    #[test]
    fn native_bmp_surfaces_match_all_fixed_gdi_observations() {
        // This tests the original constructor's LoadImageA/StretchBlt palette
        // conversion, not an unrecorded original-game screenshot. It covers
        // every pixel, including all three frames of BMP strip buttons.
        let c = content();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture: Value = serde_json::from_slice(
            &std::fs::read(root.join("tests/fixtures/original-native-ui-gdi.json")).unwrap(),
        )
        .unwrap();
        let school: Value = serde_json::from_slice(
            &std::fs::read(root.join("assets/catalog/native-school.json")).unwrap(),
        )
        .unwrap();
        let mut bank = crate::pixels::ImageBank::default();
        let mut observed = 0;
        for (kind, context) in fixture["contexts"].as_object().unwrap() {
            let maps = if kind == "school" {
                &school["gdiIndexMaps"]
            } else {
                &c.native_ui()[if kind == "equipmentResearch" {
                    "equipmentWorkshop"
                } else {
                    kind
                }]["gdiIndexMaps"]
            };
            for (resource, row) in context["images"].as_object().unwrap() {
                let hash = &c.manifest["resources"][resource]["sha256"];
                let object = a(c.manifest, "/objects")
                    .iter()
                    .find(|object| object["sha256"] == *hash)
                    .unwrap();
                bank.insert(
                    resource.clone(),
                    std::fs::read(root.join(s(object, "/path"))).unwrap(),
                );
                let mut frame = SceneFrame {
                    palette_rgba: Some(
                        context["targetPalette"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as u8)
                            .collect(),
                    ),
                    sprites: vec![SceneSprite {
                        resource_id: resource.clone(),
                        transparent: Transparency::Index(1),
                        ..Default::default()
                    }],
                    ..Default::default()
                };
                apply_gdi(&mut frame, maps);
                assert!(
                    !frame.sprites[0].index_map.is_empty(),
                    "Missing GDI conversion for {kind}/{resource}"
                );
                let mut canvas = crate::pixels::PixelCanvas::default();
                canvas.compose(&frame, &mut bank);
                let width = n(row, "/width") as usize;
                let height = n(row, "/height") as usize;
                let mut hash = 2166136261u32;
                for y in 0..height {
                    for byte in &canvas.rgba[y * 640 * 4..y * 640 * 4 + width * 4] {
                        hash = (hash ^ u32::from(*byte)).wrapping_mul(16777619);
                    }
                }
                assert_eq!(
                    hash,
                    row["colorKeyedBlackRgbaFnv32"].as_u64().unwrap() as u32,
                    "GDI full-image pixels differ: {kind}/{resource}"
                );
                observed += 1;
            }
        }
        assert_eq!(observed, 151);
    }
    fn by_id<'a>(f: &'a SceneFrame, id: &str) -> &'a SceneControl {
        f.controls.iter().find(|v| v.id == id).unwrap()
    }
    #[test]
    fn eight_stat_products_use_original_grid_and_exact_confirmation_index() {
        let c = content();
        let mut snapshot = snapshot("statShop");
        let entries=a(c.rules,"/statShops/young").iter().flat_map(|v|v.as_array().unwrap()).take(8).enumerate().map(|(index,item)|json!({"index":index,"group":0,"resource":item["resource"],"name":item["name"],"price":item["price"],"disabled":index==2,"bought":index==2})).collect::<Vec<_>>();
        snapshot["nativeView"]["offers"] = json!(entries);
        snapshot["nativeView"]["exitIndex"] = json!(8);
        let mut ui = view_ui(&snapshot);
        let f = render(&c, &snapshot, &ui);
        assert_eq!(
            by_id(&f, "native-stat-7").bounds,
            Rect::new(257, 172, 51, 51)
        );
        assert!(!by_id(&f, "native-stat-2").enabled);
        field(&mut ui, &by_id(&f, "native-stat-7").action);
        let f = render(&c, &snapshot, &ui);
        assert_eq!(by_id(&f, "native-yes").action, choose(7));
        assert!(
            f.sprites
                .iter()
                .any(|v| v.id == "native-stat-2" && v.frame == 2)
        );
        field(&mut ui, &by_id(&f, "native-no").action);
        let f = render(&c, &snapshot, &ui);
        field(
            &mut ui,
            by_id(&f, "native-stat-1").on_hover.as_ref().unwrap(),
        );
        ui.hover = Some("native-stat-1".into());
        let f = render(&c, &snapshot, &ui);
        assert!(
            f.sprites
                .iter()
                .any(|v| v.id == "native-stat-1" && v.frame == 1)
        );
        assert!(
            f.texts
                .iter()
                .all(|v| matches!(v.font, TextFont::Gdi { .. }))
        );
    }
    #[test]
    fn native_item_categories_keep_three_original_animation_slots_and_stable_indices() {
        let c = content();
        let mut snapshot = snapshot("itemShop");
        snapshot["nativeView"]["offers"]=json!(a(c.rules,"/itemShop/ordinaryYoung").iter().take(3).enumerate().map(|(i,item)|json!({"index":i+5,"category":2,"catalogIndex":item["index"],"resource":item["resource"],"description":item["description"],"available":true,"purchased":false})).collect::<Vec<_>>());
        snapshot["nativeView"]["exitIndex"] = json!(8);
        let mut ui = view_ui(&snapshot);
        let f = render(&c, &snapshot, &ui);
        field(&mut ui, &by_id(&f, "native-category-2").action);
        let f = render(&c, &snapshot, &ui);
        for (id, x) in [(5, 9), (6, 220), (7, 431)] {
            assert_eq!(
                by_id(&f, &format!("native-item-{id}")).bounds,
                Rect::new(x, 44, 177, 194)
            );
        }
        field(&mut ui, &by_id(&f, "native-item-7").action);
        let f = render(&c, &snapshot, &ui);
        assert_eq!(by_id(&f, "native-yes").action, choose(7));
        snapshot["nativeView"]["offers"][1]["purchased"] = json!(true);
        ui.profile["_nativePanel"]["selection"] = json!(-1);
        let f = render(&c, &snapshot, &ui);
        assert!(f.controls.iter().all(|v| v.id != "native-item-6"));
        assert_eq!(by_id(&f, "native-item-7").bounds.x, 220);
    }
    #[test]
    fn equipment_intro_confirmation_and_eight_research_icons_stay_in_original_bounds() {
        let c = content();
        let mut snapshot = snapshot("equipmentWorkshop");
        snapshot["nativeView"]["mode"] = json!("purchase");
        snapshot["nativeView"]["offeredId"] = json!(4);
        snapshot["nativeView"]["offered"] = c.rules["equipment"][4].clone();
        snapshot["nativeView"]["exitIndex"] = json!(1);
        let mut ui = view_ui(&snapshot);
        let f = render(&c, &snapshot, &ui);
        field(&mut ui, &by_id(&f, "native-equipment-description").action);
        update(&c, &snapshot, &mut ui, 10000);
        let f = render(&c, &snapshot, &ui);
        assert_eq!(by_id(&f, "native-yes").action, choose(0));
        snapshot["revision"] = json!(2);
        snapshot["nativeView"]["mode"] = json!("research");
        snapshot["nativeView"]["exitIndex"] = json!(18);
        snapshot["nativeView"]["entries"]=json!(a(c.rules,"/equipment").iter().take(8).enumerate().map(|(i,item)|json!({"index":i,"resource":item["icon"],"name":item["name"],"description":item["description"],"researchPrice":1000})).collect::<Vec<_>>());
        let mut ui = view_ui(&snapshot);
        let f = render(&c, &snapshot, &ui);
        assert_eq!(
            by_id(&f, "native-research-7").bounds,
            Rect::new(210, 101, 51, 51)
        );
        field(&mut ui, &by_id(&f, "native-research-7").action);
        let f = render(&c, &snapshot, &ui);
        assert_eq!(by_id(&f, "native-yes").action, choose(7));
    }
    #[test]
    fn original_career_scroll_makes_all_twenty_four_real_choices_reachable() {
        let c = content();
        let mut snapshot = snapshot("career");
        snapshot["nativeView"]["careers"] = json!(
            a(c.rules, "/careers")
                .iter()
                .enumerate()
                .map(|(index, career)| {
                    let mut c = career.clone();
                    c["index"] = json!(index);
                    c["eligible"] = json!(index % 2 == 1);
                    c["disabled"] = json!(index % 2 == 0);
                    c
                })
                .collect::<Vec<_>>()
        );
        snapshot["nativeView"]["exitIndex"] = json!(24);
        let mut ui = view_ui(&snapshot);
        update(&c, &snapshot, &mut ui, 10000);
        let mut seen = std::collections::BTreeSet::new();
        for start in 0..=14 {
            ui.profile["_nativePanel"]["scroll"] = json!(start);
            let f = render(&c, &snapshot, &ui);
            for control in &f.controls {
                if let Some(index) = control
                    .id
                    .strip_prefix("native-career-")
                    .and_then(|v| v.parse::<usize>().ok())
                {
                    seen.insert(index);
                }
            }
        }
        assert_eq!(seen.len(), 24);
        let f = render(&c, &snapshot, &ui);
        field(&mut ui, &by_id(&f, "native-career-23").action);
        let f = render(&c, &snapshot, &ui);
        assert!(by_id(&f, "native-yes").enabled);
        field(&mut ui, &by_id(&f, "native-yes").action);
        assert_eq!(s(&ui.profile, "/_nativePanel/phase"), "accepted");
        ui.profile["_nativePanel"]["phase"] = json!("menu");
        field(&mut ui, &by_id(&f, "native-career-22").action);
        let f = render(&c, &snapshot, &ui);
        assert!(!by_id(&f, "native-yes").enabled);
    }
}
