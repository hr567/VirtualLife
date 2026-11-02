use super::*;

pub(super) fn menu(f: &mut SceneFrame, ui: &PresentationState) {
    sprite(f, "menu-background", "install/bmp/pm0201.bmp", 0, 0, 0);
    f.sprites.last_mut().unwrap().transparent = Transparency::Opaque;
    for (i, (y, label, action)) in [
        (143, "开始新游戏", UiAction::NewProfile),
        (192, "继续游戏", UiAction::Load),
        (242, "游戏选项", UiAction::Options),
        (290, "退出游戏", UiAction::MainMenu),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("menu-{i}");
        let idx = sprite(f, &id, format!("install/bmp/pm020{}.bmp", i + 2), 237, y, 1);
        let sp = &mut f.sprites[idx];
        sp.width = Some(155);
        sp.height = Some(25);
        sp.frame_count = Some(3);
        sp.frame = if ui.pressed.as_deref() == Some(&id) {
            2
        } else if ui.hover.as_deref() == Some(&id) {
            1
        } else {
            0
        };
        control(
            f,
            &id,
            label,
            Rect::new(237, y, 155, 25),
            Action::Ui(action),
            &[],
        );
    }
}
pub(super) fn profile(f: &mut SceneFrame, c: &SceneContent<'_>, ui: &PresentationState) {
    let r = c.native_profile();
    // 00420db0 prepares the button WAVs before entering the creator. Loading
    // one for the first time during hover would pause input between down/up.
    for key in ["hoverSound", "clickSound"] {
        if let Some(resource) = r[key].as_str().filter(|id| !id.is_empty()) {
            f.required_resources.push(resource.into());
        }
    }
    // The constructor also prepares hidden hover/sex alternatives and every
    // zodiac/blood image. A first hover must never become a loading boundary.
    for key in ["actors", "confirm", "cancel", "buttons", "zodiac", "blood"] {
        for image in a(r, &format!("/{key}")) {
            if let Some(resource) = image["resource"].as_str() {
                f.required_resources.push(resource.into());
            }
        }
    }
    // The original loads each BMP through GDI into pm0301's indexed display
    // palette before its destination-index-1 color key is applied.
    fn bitmap(f: &mut SceneFrame, r: &Value, id: &str, v: &Value, z: i32) -> usize {
        let index = image(f, id, v, z).expect("verified creator bitmap");
        f.sprites[index].index_map = a(
            &r["gdiIndexMaps"],
            &format!(
                "/{}",
                s(v, "/resource").replace('~', "~0").replace('/', "~1")
            ),
        )
        .iter()
        .filter_map(|row| Some([row[0].as_u64()? as u8, row[1].as_u64()? as u8]))
        .collect();
        index
    }
    fn animated(
        f: &mut SceneFrame,
        id: &str,
        v: &Value,
        visible: bool,
        always_advance: bool,
        z: i32,
    ) {
        let index = image(f, id, v, z).expect("verified creator animation");
        let sp = &mut f.sprites[index];
        sp.frame_count = Some(n(v, "/frameCount").max(1) as u32);
        sp.playback = Some(Playback {
            interval_ms: if visible || always_advance {
                n(v, "/interval").max(1) as u32
            } else {
                u32::MAX
            },
            max_advances: 1,
            immediate: visible || always_advance,
            ..Default::default()
        });
        // Both native objects stay alive when their counterpart is displayed.
        // Keep their independent frame cursors while advancing only the active
        // object, at most once per visible frame (004585c0).
        if !visible {
            sp.x = -1000;
            sp.y = -1000;
        }
    }
    let month = n(&ui.profile, "/birthMonth").clamp(1, 12);
    let day = n(&ui.profile, "/birthDay").clamp(1, 31);
    let days = a(r, "/monthDays");
    let max_day = days
        .get((month - 1) as usize)
        .and_then(Value::as_i64)
        .unwrap_or(31) as i32;
    let blood = s(&ui.profile, "/bloodType");
    let blood_rows = a(r, "/blood");
    let blood_index = blood_rows
        .iter()
        .position(|v| s(v, "/bloodType") == blood)
        .unwrap_or(0);
    let female = s(&ui.profile, "/sex") == "female";
    f.palette = Some(s(r, "/background/resource").into());
    bitmap(f, r, "profile-background", &r["background"], 0);
    for v in a(r, "/buttons") {
        let native_id = s(v, "/id");
        let id = format!("profile-{native_id}");
        let action = match native_id {
            "name" => UiAction::EditName,
            "male" | "female" => UiAction::SetField {
                field: "sex".into(),
                value: json!(native_id),
            },
            "month-next" | "month-previous" => UiAction::SetField {
                field: "birthMonth".into(),
                value: json!((month - 1 + if native_id == "month-next" { 1 } else { 11 }) % 12 + 1),
            },
            "day-next" | "day-previous" => UiAction::SetField {
                field: "birthDay".into(),
                value: json!(if native_id == "day-next" {
                    if day >= max_day { 1 } else { day + 1 }
                } else if day == 1 {
                    max_day
                } else {
                    day - 1
                }),
            },
            "blood-next" | "blood-previous" => {
                let next = (blood_index + if native_id == "blood-next" { 1 } else { 3 }) % 4;
                UiAction::SetField {
                    field: "bloodType".into(),
                    value: blood_rows[next]["bloodType"].clone(),
                }
            }
            _ => continue,
        };
        let index = bitmap(f, r, &id, v, 1);
        let sp = &mut f.sprites[index];
        sp.height = Some(n(v, "/frameHeight") as u32);
        sp.frame_count = Some(3);
        let latched = (native_id == "female" && female) || (native_id == "male" && !female);
        sp.frame = if latched || ui.pressed.as_deref() == Some(&id) {
            2
        } else if ui.hover.as_deref() == Some(&id) {
            1
        } else {
            0
        };
        let bounds = Rect::new(sp.x, sp.y, sp.width.unwrap(), sp.height.unwrap());
        control(f, &id, s(v, "/label"), bounds, Action::Ui(action), &[]);
    }
    // FUN_00420d90 sums the original 29-day February table. Month arrows do
    // not normalize the day, including the original temporary February 31.
    let ordinal = |m: i32, d: i32| -> i32 {
        days.iter()
            .take((m - 1).max(0) as usize)
            .map(|v| v.as_i64().unwrap_or(0) as i32)
            .sum::<i32>()
            + d
    };
    let date = ordinal(month, day);
    if let Some(v) = a(r, "/zodiac").iter().find(|v| {
        let start = ordinal(n(v, "/startMonth"), n(v, "/startDay"));
        let end = ordinal(n(v, "/endMonth"), n(v, "/endDay"));
        if start > end {
            date >= start || date <= end
        } else {
            date >= start && date <= end
        }
    }) {
        bitmap(f, r, "profile-zodiac", v, 3);
    }
    if let Some(v) = blood_rows.get(blood_index) {
        bitmap(f, r, "profile-blood", v, 3);
    }
    for (index, v) in a(r, "/actors").iter().enumerate() {
        animated(
            f,
            &format!("profile-actor-{index}"),
            v,
            index == usize::from(female),
            false,
            4,
        );
    }
    for (key, label, action, keys) in [
        ("confirm", "确定", UiAction::StartGame, &["Enter"][..]),
        ("cancel", "返回", UiAction::MainMenu, &["Escape"][..]),
    ] {
        let id = format!("profile-{key}");
        let hovered = ui.hover.as_deref() == Some(&id) || ui.pressed.as_deref() == Some(&id);
        for (index, v) in a(r, &format!("/{key}")).iter().enumerate() {
            // 00459d80 advances both the normal and hover object every
            // visible frame, even while only one of them is drawn.
            animated(
                f,
                &format!("{id}-{index}"),
                v,
                index == usize::from(hovered),
                true,
                2,
            );
        }
        let v = &r[key][0];
        control(
            f,
            &id,
            label,
            Rect::new(
                n(v, "/x"),
                n(v, "/y"),
                n(v, "/width") as u32,
                n(v, "/height") as u32,
            ),
            Action::Ui(action),
            keys,
        );
    }
    for (key, value) in [
        ("month", month.to_string()),
        ("day", day.to_string()),
        ("name", s(&ui.profile, "/name").to_owned()),
    ] {
        let v = &r["text"][key];
        let width = if key == "name" {
            n(v, "/capacityBytes") * 8
        } else {
            24
        };
        let byte_len = encoding_rs::GBK.encode(&value).0.len() as i32;
        let x = n(v, "/x")
            + if key == "name" {
                (width / 8 - byte_len).max(0) * 4
            } else {
                0
            };
        text(
            f,
            &format!("profile-{key}-value"),
            value,
            Rect::new(x, n(v, "/y"), width as u32, 16),
            [255; 4],
        );
        let label = f.texts.last_mut().unwrap();
        label.font = TextFont::Original;
        label.palette_index = Some(n(r, "/text/colorIndex") as u8);
        label.clip = Some(Rect::new(n(v, "/x"), n(v, "/y"), width as u32, 16));
    }
}
#[cfg(test)]
mod profile_tests {
    use super::*;

    fn catalogs() -> (Value, Value) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let metadata: Value = serde_json::from_slice(
            &std::fs::read(root.join("assets/catalog/native-profile.json")).unwrap(),
        )
        .unwrap();
        (json!({"_nativeProfile":metadata}), Value::Null)
    }
    fn state() -> PresentationState {
        PresentationState {
            screen: Screen::Profile,
            profile: json!({"name":"丁马克","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}),
            ..Default::default()
        }
    }
    fn field(frame: &SceneFrame, id: &str) -> (String, Value) {
        match &frame.controls.iter().find(|v| v.id == id).unwrap().action {
            Action::Ui(UiAction::SetField { field, value }) => (field.clone(), value.clone()),
            _ => panic!("creator field control"),
        }
    }
    #[test]
    fn profile_uses_original_buttons_fonts_blood_and_zodiac() {
        let (manifest, empty) = catalogs();
        let c = SceneContent {
            manifest: &manifest,
            maps: &empty,
            rules: &empty,
        };
        let mut frame = SceneFrame::default();
        profile(&mut frame, &c, &state());
        assert_eq!(frame.controls.len(), 11);
        assert!(
            frame
                .texts
                .iter()
                .all(|v| matches!(v.font, TextFont::Original) && v.palette_index == Some(31))
        );
        assert_eq!(
            frame
                .sprites
                .iter()
                .find(|v| v.id == "profile-male")
                .unwrap()
                .frame,
            2
        );
        assert_eq!(
            frame
                .sprites
                .iter()
                .find(|v| v.id == "profile-female")
                .unwrap()
                .frame,
            0
        );
        assert_eq!(
            frame
                .sprites
                .iter()
                .find(|v| v.id == "profile-zodiac")
                .unwrap()
                .resource_id,
            "install/bmp/pm04k12.bmp"
        );
        assert_eq!(
            frame
                .sprites
                .iter()
                .find(|v| v.id == "profile-name")
                .unwrap()
                .y,
            91
        );
        assert_eq!(
            field(&frame, "profile-blood-next"),
            ("bloodType".into(), json!("AB"))
        );
        assert_eq!(
            field(&frame, "profile-blood-previous"),
            ("bloodType".into(), json!("O"))
        );
    }
    #[test]
    fn profile_preserves_original_february29_table_and_unclamped_month_change() {
        let (manifest, empty) = catalogs();
        let c = SceneContent {
            manifest: &manifest,
            maps: &empty,
            rules: &empty,
        };
        let mut ui = state();
        ui.profile["birthDay"] = json!(31);
        let mut january = SceneFrame::default();
        profile(&mut january, &c, &ui);
        assert_eq!(
            field(&january, "profile-month-next"),
            ("birthMonth".into(), json!(2))
        );
        ui.profile["birthMonth"] = json!(2);
        let mut february = SceneFrame::default();
        profile(&mut february, &c, &ui);
        assert_eq!(
            february
                .texts
                .iter()
                .find(|v| v.id == "profile-day-value")
                .unwrap()
                .text,
            "31"
        );
        assert_eq!(field(&february, "profile-day-previous").1, json!(30));
        assert_eq!(field(&february, "profile-day-next").1, json!(1));
        ui.profile["birthDay"] = json!(1);
        let mut first = SceneFrame::default();
        profile(&mut first, &c, &ui);
        assert_eq!(field(&first, "profile-day-previous").1, json!(29));
    }
    #[test]
    fn profile_actor_cursors_survive_gender_changes_and_buttons_advance_together() {
        let (manifest, empty) = catalogs();
        let c = SceneContent {
            manifest: &manifest,
            maps: &empty,
            rules: &empty,
        };
        let mut ui = state();
        super::super::clocks::update_presentation(&c, &empty, &mut ui, 0);
        super::super::clocks::update_presentation(&c, &empty, &mut ui, 101);
        assert_eq!(ui.runtime.animations["profile-actor-0"].frame, 1);
        assert_eq!(ui.runtime.animations["profile-actor-1"].frame, 0);
        super::super::clocks::update_presentation(&c, &empty, &mut ui, 14);
        assert_eq!(ui.runtime.animations["profile-confirm-0"].frame, 1);
        assert_eq!(ui.runtime.animations["profile-confirm-1"].frame, 1);
        ui.profile["sex"] = json!("female");
        super::super::clocks::update_presentation(&c, &empty, &mut ui, 1000);
        assert_eq!(ui.runtime.animations["profile-actor-0"].frame, 1);
        assert_eq!(ui.runtime.animations["profile-actor-1"].frame, 0);
        super::super::clocks::update_presentation(&c, &empty, &mut ui, 101);
        assert_eq!(ui.runtime.animations["profile-actor-1"].frame, 1);
        ui.profile["sex"] = json!("male");
        super::super::clocks::update_presentation(&c, &empty, &mut ui, 0);
        assert_eq!(ui.runtime.animations["profile-actor-0"].frame, 2);
    }
}
pub(super) fn options(f: &mut SceneFrame, c: &SceneContent<'_>, ui: &PresentationState) {
    let r = &c.rules["optionsRendering"];
    // The original system menu constructs this cursor before accepting input.
    // A keyboard entry can leave it offscreen until the first button press.
    if let Some(resource) = r["cursor"]["resource"].as_str() {
        f.required_resources.push(resource.into());
    }
    let page = s(&ui.profile, "/_optionsPage");
    let selected = if page == "volume" || page == "exit" {
        &r[page]
    } else {
        r
    };
    for back in a(selected, "/back") {
        f.required_resources.push(s(back, "/resource").into());
    }
    image(f, "options-background", &selected["background"], 0);
    f.palette = selected["background"]["resource"]
        .as_str()
        .map(str::to_owned);
    if page == "volume" {
        image(f, "volume-track", &r["volume"]["track"], 1);
        let mut thumb = r["volume"]["thumb"].clone();
        let track = &r["volume"]["track"];
        thumb["x"] = json!(
            n(track, "/x") + (n(track, "/width") - n(&thumb, "/width")) * ui.volume as i32 / 100
        );
        thumb["y"] = json!(n(track, "/y") - (n(&thumb, "/height") - n(track, "/height")) / 2);
        image(f, "volume-thumb", &thumb, 2);
        let x = n(track, "/x");
        let y = n(&thumb, "/y");
        let width = n(track, "/width").max(1);
        let travel = (width - n(&thumb, "/width")).max(1);
        // 45a6d0 follows the held pointer; 45a6a0 divides by the
        // track-minus-thumb travel, not by the full track width.
        for pixel in 0..width {
            let value = (pixel * 100 / travel).clamp(0, 100);
            control(
                f,
                &format!("volume-{pixel}"),
                "总音量",
                Rect::new(x + pixel, y, 1, n(&thumb, "/height") as u32),
                Action::Ui(UiAction::SetVolume(value)),
                &[],
            );
            f.controls.last_mut().unwrap().on_press = Some(Action::Ui(UiAction::SetVolume(value)));
        }
    } else if page == "exit" {
        button(
            f,
            "exit-yes",
            &r["exit"]["yes"],
            Action::Ui(UiAction::MainMenu),
            "确定离开游戏",
            1,
            ui,
        );
        f.controls.last_mut().unwrap().keys.push("KeyY".into());
        button(
            f,
            "exit-no",
            &r["exit"]["no"],
            Action::Ui(UiAction::SetField {
                field: "_optionsPage".into(),
                value: json!("menu"),
            }),
            "取消离开游戏",
            1,
            ui,
        );
        f.controls.last_mut().unwrap().keys.push("KeyN".into());
    } else {
        for (i, v) in a(r, "/buttons").iter().enumerate() {
            let (action, label) = match s(v, "/action") {
                "volume" => (
                    UiAction::SetField {
                        field: "_optionsPage".into(),
                        value: json!("volume"),
                    },
                    "音量设置",
                ),
                "save" => (UiAction::Save, "保存游戏"),
                "load" => (UiAction::Load, "读取游戏"),
                _ => (
                    UiAction::SetField {
                        field: "_optionsPage".into(),
                        value: json!("exit"),
                    },
                    "离开游戏",
                ),
            };
            button(
                f,
                &format!("options-{i}"),
                v,
                Action::Ui(action),
                label,
                1,
                ui,
            );
        }
    }
    let hover = ui.hover.as_deref() == Some("options-back");
    let mut back = selected["back"][usize::from(hover)].clone();
    back["frame"] = json!(ui.runtime.options_frames[usize::from(hover)]);
    animation(f, "options-back", &back, 4);
    let rect = Rect::new(
        n(&back, "/x"),
        n(&back, "/y"),
        n(&back, "/width") as u32,
        n(&back, "/height") as u32,
    );
    let action = if page == "volume" || page == "exit" {
        UiAction::SetField {
            field: "_optionsPage".into(),
            value: json!("menu"),
        }
    } else {
        UiAction::Back
    };
    control(
        f,
        "options-back",
        "返回",
        rect,
        Action::Ui(action),
        &["Escape"],
    );
    if let Some(index) = image(f, "options-cursor", &r["cursor"], 100) {
        let cursor = &mut f.sprites[index];
        cursor.x = ui.pointer.0;
        cursor.y = ui.pointer.1;
        cursor.frame_count = Some(2);
        cursor.height = cursor.height.map(|height| height / 2);
        // 59640(..., 2, 250) sets a two-frame cursor's interval to 250ms;
        // its 58b20(..., 1) call sets the actual transparent palette index.
        cursor.transparent = Transparency::Index(1);
        cursor.playback = Some(Playback {
            interval_ms: 250,
            once: false,
            max_advances: 1,
            immediate: true,
        });
        if !(0..640).contains(&ui.pointer.0) || !(0..480).contains(&ui.pointer.1) {
            cursor.x = -1000;
            cursor.y = -1000;
        }
        f.requests.push(SceneRequest::CursorHidden(true));
    }
}
fn save_button(
    f: &mut SceneFrame,
    ui: &PresentationState,
    id: &str,
    label: &str,
    bounds: Rect,
    action: UiAction,
    enabled: bool,
) {
    let color = if !enabled {
        [35, 40, 54, 220]
    } else if ui.pressed.as_deref() == Some(id) {
        [46, 78, 115, 255]
    } else if ui.hover.as_deref() == Some(id) {
        [82, 109, 140, 255]
    } else {
        [48, 66, 92, 255]
    };
    f.fills.push(SceneFill {
        id: format!("{id}-background"),
        bounds,
        color,
        z: 910,
        ..Default::default()
    });
    text(
        f,
        &format!("{id}-text"),
        label,
        Rect::new(
            bounds.x + 6,
            bounds.y + 4,
            bounds.width.saturating_sub(8),
            bounds.height,
        ),
        if enabled {
            [255; 4]
        } else {
            [135, 140, 150, 255]
        },
    );
    f.texts.last_mut().unwrap().z = 920;
    control(f, id, label, bounds, Action::Ui(action), &[]);
    f.controls.last_mut().unwrap().enabled = enabled;
}

pub(super) fn saves(f: &mut SceneFrame, c: &SceneContent<'_>, ui: &PresentationState) {
    f.requests.push(SceneRequest::CursorHidden(false));
    let native = &c.rules["optionsRendering"]["save"];
    // Both states must be ready before input: loading on the first hover can
    // otherwise swallow the press/release that should leave this overlay.
    for back in a(native, "/back") {
        f.required_resources.push(s(back, "/resource").into());
    }
    image(f, "save-background", &native["background"], 0);
    f.palette = native["background"]["resource"].as_str().map(str::to_owned);
    let menu = &ui.saves;
    let available = menu.ready && !menu.busy && !menu.confirm;
    for slot in 0..9 {
        let y = if slot == 0 {
            37
        } else {
            71 + (slot as i32 - 1) * 36
        };
        let bounds = Rect::new(144, y, 305, 25);
        if slot == 0 || slot == menu.selected {
            f.fills.push(SceneFill {
                id: format!("save-slot-{slot}-highlight"),
                bounds,
                color: if slot == menu.selected {
                    [49, 86, 125, 245]
                } else {
                    [42, 48, 64, 240]
                },
                z: 1,
                ..Default::default()
            });
        }
        let record = &menu.slots[slot];
        let description = if !menu.ready {
            "读取中".to_owned()
        } else if record["occupied"] != true {
            "空".to_owned()
        } else if record["valid"] != true {
            "损坏/版本不符".to_owned()
        } else {
            s(record, "/name")
                .chars()
                .take(if slot == 0 { 4 } else { 8 })
                .collect()
        };
        let prefix = if slot == 0 {
            "0 自动".to_owned()
        } else {
            format!("{slot}")
        };
        text(
            f,
            &format!("save-slot-{slot}-text"),
            format!("{prefix} · {description}"),
            Rect::new(148, y + 4, 188, 20),
            [255; 4],
        );
        f.texts.last_mut().unwrap().clip = Some(Rect::new(148, y, 188, 25));
        control(
            f,
            &format!("save-slot-{slot}"),
            &format!("选择槽位 {slot}"),
            Rect::new(144, y, 192, 25),
            Action::Ui(UiAction::SelectSave(slot)),
            &[],
        );
        f.controls.last_mut().unwrap().enabled = available;
        for (id, label, x, action, enabled) in [
            (
                format!("save-import-{slot}"),
                "导入",
                342,
                UiAction::Import(slot),
                available,
            ),
            (
                format!("save-export-{slot}"),
                "导出",
                394,
                UiAction::Export(slot),
                available && record["valid"] == true,
            ),
        ] {
            save_button(f, ui, &id, label, Rect::new(x, y, 48, 25), action, enabled);
            f.controls.last_mut().unwrap().label = format!("{label}槽位 {slot}");
        }
    }
    let record = &menu.slots[menu.selected];
    let details = if menu.busy {
        "正在处理存档…".into()
    } else if let Some(time) = record["savedAt"].as_str() {
        format!(
            "保存时间 {}",
            time.replace('T', " ").chars().take(24).collect::<String>()
        )
    } else {
        format!("已选择槽位 {}", menu.selected)
    };
    text(
        f,
        "save-details",
        details,
        Rect::new(144, 350, 350, 18),
        [255; 4],
    );
    save_button(
        f,
        ui,
        "save-selected",
        "保存选中槽位",
        Rect::new(144, 374, 144, 28),
        UiAction::Save,
        available && menu.selected > 0 && menu.can_save,
    );
    save_button(
        f,
        ui,
        "load-selected",
        "读取选中槽位",
        Rect::new(302, 374, 144, 28),
        UiAction::Load,
        available && record["valid"] == true,
    );
    save_button(
        f,
        ui,
        "save-refresh",
        "刷新列表",
        Rect::new(144, 408, 104, 25),
        UiAction::OpenSaves,
        !menu.busy && !menu.confirm,
    );
    let hover = usize::from(ui.hover.as_deref() == Some("save-back"));
    let mut back = native["back"][hover].clone();
    back["frame"] = json!(ui.runtime.options_frames[hover]);
    animation(f, "save-back-icon", &back, 4);
    control(
        f,
        "save-back",
        "返回游戏",
        Rect::new(
            n(&back, "/x"),
            n(&back, "/y"),
            n(&back, "/width") as u32,
            n(&back, "/height") as u32,
        ),
        Action::Ui(UiAction::Back),
        &[],
    );
    f.controls.last_mut().unwrap().enabled = !menu.confirm;
    f.controls.last_mut().unwrap().keys.push("Escape".into());
    if menu.confirm {
        for control in &mut f.controls {
            control.enabled = false;
        }
        f.fills.push(SceneFill {
            id: "save-confirm-panel".into(),
            bounds: Rect::new(138, 170, 330, 140),
            color: [18, 28, 46, 255],
            z: 11000,
            ..Default::default()
        });
        text(
            f,
            "save-confirm-question",
            format!("覆盖槽位 {} 的已有存档？", menu.selected),
            Rect::new(158, 192, 300, 24),
            [255; 4],
        );
        f.texts.last_mut().unwrap().z = 11001;
        for (id, label, x, action, key) in [
            (
                "save-confirm-yes",
                "确定覆盖",
                170,
                UiAction::ConfirmSave,
                "Enter",
            ),
            (
                "save-confirm-no",
                "取消",
                324,
                UiAction::CancelSave,
                "Escape",
            ),
        ] {
            save_button(f, ui, id, label, Rect::new(x, 254, 116, 32), action, true);
            f.fills.last_mut().unwrap().z = 11001;
            f.texts.last_mut().unwrap().z = 11002;
            let control = f.controls.last_mut().unwrap();
            control.z = 11003;
            control.keys.push(key.into());
        }
    }
}

#[cfg(test)]
mod options_tests {
    use super::*;

    #[test]
    fn original_volume_travel_press_controls_and_system_cursor_are_preserved() {
        let rules: Value =
            serde_json::from_str(include_str!("../../../../assets/catalog/rules.json")).unwrap();
        let c = SceneContent {
            manifest: &Value::Null,
            maps: &Value::Null,
            rules: &rules,
        };
        let mut ui = PresentationState {
            screen: Screen::Options,
            pointer: (310, 226),
            ..Default::default()
        };
        ui.profile["_optionsPage"] = json!("volume");
        for (volume, expected_x) in [(0, 233), (50, 305), (100, 378)] {
            ui.volume = volume;
            let mut frame = SceneFrame::default();
            options(&mut frame, &c, &ui);
            let thumb = frame
                .sprites
                .iter()
                .find(|sprite| sprite.id == "volume-thumb")
                .unwrap();
            assert_eq!(thumb.x, expected_x);
            assert_eq!(thumb.y, 218);
            let segments: Vec<_> = frame
                .controls
                .iter()
                .filter(|control| control.id.starts_with("volume-"))
                .collect();
            assert_eq!(segments.len(), 155);
            for (pixel, expected_value) in [(0, 0), (72, 49), (144, 99), (145, 100), (154, 100)] {
                let segment = segments[pixel];
                assert_eq!(segment.bounds, Rect::new(233 + pixel as i32, 218, 1, 24));
                assert_eq!(
                    segment.action,
                    Action::Ui(UiAction::SetVolume(expected_value))
                );
                assert_eq!(segment.on_press, Some(segment.action.clone()));
            }
            let cursor = frame
                .sprites
                .iter()
                .find(|sprite| sprite.id == "options-cursor")
                .unwrap();
            assert_eq!(cursor.resource_id, "install/bmp/mpm06e01.bmp");
            assert_eq!(
                (cursor.x, cursor.y, cursor.width, cursor.height),
                (310, 226, Some(24), Some(24))
            );
            assert_eq!(cursor.frame_count, Some(2));
            assert_eq!(cursor.transparent, Transparency::Index(1));
            assert_eq!(cursor.playback.as_ref().unwrap().interval_ms, 250);
            assert!(
                frame
                    .requests
                    .iter()
                    .any(|request| matches!(request, SceneRequest::CursorHidden(true)))
            );
        }
        ui.pointer = (-1000, -1000);
        for page in ["menu", "volume", "exit"] {
            ui.profile["_optionsPage"] = json!(page);
            let mut frame = SceneFrame::default();
            options(&mut frame, &c, &ui);
            assert!(
                frame
                    .required_resources
                    .contains(&"install/bmp/mpm06e01.bmp".into())
            );
            let cursor = frame
                .sprites
                .iter()
                .find(|sprite| sprite.id == "options-cursor")
                .unwrap();
            assert_eq!((cursor.x, cursor.y), (-1000, -1000));
        }
        ui.profile["_optionsPage"] = json!("exit");
        let mut frame = SceneFrame::default();
        options(&mut frame, &c, &ui);
        assert!(
            frame
                .controls
                .iter()
                .find(|control| control.id == "exit-yes")
                .unwrap()
                .keys
                .contains(&"KeyY".into())
        );
        assert!(
            frame
                .controls
                .iter()
                .find(|control| control.id == "exit-no")
                .unwrap()
                .keys
                .contains(&"KeyN".into())
        );
    }
}
