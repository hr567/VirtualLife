use super::*;
use std::{path::PathBuf, sync::OnceLock};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn catalogs() -> &'static (Value, Value, Value) {
    static C: OnceLock<(Value, Value, Value)> = OnceLock::new();
    C.get_or_init(|| {
        let read = |path: &str| {
            serde_json::from_slice::<Value>(&std::fs::read(root().join(path)).unwrap()).unwrap()
        };
        let mut manifest = read("assets/manifest.json");
        let index = manifest["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|object| {
                (
                    object["sha256"].as_str().unwrap().to_owned(),
                    object.clone(),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        manifest["_objects"] = Value::Object(index);
        for (key, file) in [
            ("_nativeUi", "native-ui"),
            ("_nativeHousing", "native-housing"),
            ("_nativeFortune", "native-fortune"),
            ("_nativeSchool", "native-school"),
            ("_nativeAllocation", "native-allocation"),
            ("_nativeProfile", "native-profile"),
        ] {
            manifest[key] = read(&format!("assets/catalog/{file}.json"));
        }
        (
            manifest,
            read("assets/catalog/maps.json"),
            read("assets/catalog/rules.json"),
        )
    })
}
fn content() -> SceneContent<'static> {
    let (m, mp, r) = catalogs();
    SceneContent {
        manifest: m,
        maps: mp,
        rules: r,
    }
}

#[test]
fn nine_save_positions_preserve_original_manual_rows_and_modal_hit_targets() {
    let c = content();
    let mut ui = PresentationState {
        screen: Screen::Saves,
        ..Default::default()
    };
    ui.saves.ready = true;
    ui.saves.can_save = true;
    ui.saves.slots=json!((0..9).map(|slot|json!({"occupied":true,"valid":true,"name":format!("存档角色{slot}"),"savedAt":"2026-11-02T01:00:00.000Z"})).collect::<Vec<_>>());
    let frame = compose(&c, &Value::Null, &ui);
    for back in a(&c.rules["optionsRendering"]["save"], "/back") {
        assert!(
            frame
                .required_resources
                .iter()
                .any(|resource| resource == s(back, "/resource"))
        );
    }
    assert_eq!(
        frame
            .controls
            .iter()
            .filter(|control| control.id.starts_with("save-import-"))
            .count(),
        9
    );
    assert_eq!(
        frame
            .controls
            .iter()
            .filter(|control| control.id.starts_with("save-export-"))
            .count(),
        9
    );
    for slot in 1..=8 {
        let row = frame
            .controls
            .iter()
            .find(|control| control.id == format!("save-slot-{slot}"))
            .unwrap();
        assert_eq!(row.bounds.y, 71 + (slot - 1) * 36);
    }
    let enabled: Vec<_> = frame
        .controls
        .iter()
        .filter(|control| control.enabled)
        .collect();
    for (index, left) in enabled.iter().enumerate() {
        for right in enabled.iter().skip(index + 1) {
            let a = left.bounds;
            let b = right.bounds;
            assert!(
                a.x + a.width as i32 <= b.x
                    || b.x + b.width as i32 <= a.x
                    || a.y + a.height as i32 <= b.y
                    || b.y + b.height as i32 <= a.y,
                "overlapping controls: {} and {}",
                left.id,
                right.id
            );
        }
    }
    ui.saves.selected = 0;
    let automatic = compose(&c, &Value::Null, &ui);
    assert!(
        !automatic
            .controls
            .iter()
            .find(|control| control.id == "save-selected")
            .unwrap()
            .enabled
    );
    ui.saves.confirm = true;
    let modal = compose(&c, &Value::Null, &ui);
    assert_eq!(
        modal
            .controls
            .iter()
            .filter(|control| control.enabled)
            .count(),
        2
    );
    if let Some(directory) = std::env::var_os("VIRTUAL_LIFE_QA_IMAGE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut bank = crate::pixels::ImageBank::default();
        let bytes = |resource: &str| {
            let sha = s(&c.manifest["resources"][resource], "/sha256");
            let object = c.manifest["objects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|object| object["sha256"] == sha)
                .unwrap();
            std::fs::read(root().join(s(object, "/path"))).unwrap()
        };
        for sprite in &frame.sprites {
            bank.insert(sprite.resource_id.clone(), bytes(&sprite.resource_id));
        }
        let metadata: Value = serde_json::from_slice(
            &std::fs::read(root().join("assets/catalog/glyphs.json")).unwrap(),
        )
        .unwrap();
        let mut glyphs: crate::pixels::GlyphAtlas = serde_json::from_value(metadata).unwrap();
        glyphs
            .set_pixels(bytes(crate::font::UI_FONT_RESOURCE))
            .unwrap();
        bank.glyphs = Some(glyphs);
        for (name, frame) in [("save-slots.bmp", frame), ("save-confirm.bmp", modal)] {
            let mut canvas = crate::pixels::PixelCanvas::default();
            canvas.compose(&frame, &mut bank);
            let mut bmp = vec![0; 54];
            bmp[0..2].copy_from_slice(b"BM");
            bmp[2..6].copy_from_slice(&(54 + canvas.rgba.len() as u32).to_le_bytes());
            bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
            bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
            bmp[18..22].copy_from_slice(&640i32.to_le_bytes());
            bmp[22..26].copy_from_slice(&(-480i32).to_le_bytes());
            bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
            bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
            for pixel in canvas.rgba.chunks_exact(4) {
                bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
            std::fs::write(directory.join(name), bmp).unwrap();
        }
    }
}
fn fixture(name: &str) -> Value {
    let v: Value = serde_json::from_slice(
        &std::fs::read(root().join(format!("tests/fixtures/browser-{name}.json"))).unwrap(),
    )
    .unwrap();
    let mut snapshot = v["state"].clone();
    snapshot["life"] = json!({"stage":2,"profession":-1});
    snapshot["statusOverview"] = snapshot["pending"]["overview"].clone();
    snapshot["statusDetails"] = snapshot["pending"]["details"].clone();
    snapshot
}
fn check_resources(c: &SceneContent<'_>, frame: &SceneFrame, case: &str) {
    for resource in frame
        .sprites
        .iter()
        .map(|v| v.resource_id.as_str())
        .chain(frame.sounds.iter().map(|v| v.resource_id.as_str()))
    {
        let resolved = c.resolve(resource);
        assert!(
            c.manifest["resources"].get(&resolved).is_some(),
            "{case}: unknown original resource {resource}"
        );
    }
    for ctrl in &frame.controls {
        if let Action::Game(value) = &ctrl.action {
            let mut value = value.clone();
            value["seq"] = json!(0);
            serde_json::from_value::<virtual_life_core::GameCommand>(value)
                .unwrap_or_else(|error| panic!("{case}: invalid command {}: {error}", ctrl.id));
        }
    }
}

#[test]
fn native_scene_fixtures_reference_only_verified_original_resources() {
    let c = content();
    let mut ui = PresentationState {
        screen: Screen::Game,
        ..Default::default()
    };
    for name in [
        "battle",
        "competition",
        "controlled-dice",
        "ending-loss",
        "ending-win",
        "final-battle",
        "final-combat",
        "final-shop",
        "fire",
        "firebee",
        "fist",
        "gas",
        "inventory",
        "relationship",
        "spirit-slots",
        "status",
    ] {
        let snapshot = fixture(name);
        update_presentation(&c, &snapshot, &mut ui, 16);
        let frame = compose(&c, &snapshot, &ui);
        assert!(!frame.sprites.is_empty(), "empty {name} scene");
        check_resources(&c, &frame, name);
    }
}

#[test]
fn native_games_retain_their_original_ready_screens_and_input_regions() {
    let c = content();
    let ui = PresentationState {
        screen: Screen::Game,
        ..Default::default()
    };
    for (name, background, help) in [
        ("gas", "install/gas/bg01.bmp", "install/gas/bg01_1.bmp"),
        (
            "fist",
            "install/fistgame/fistbg.bmp",
            "install/fistgame/fisthelp.bmp",
        ),
        ("fire", "install/fire/yao7.bmp", "install/fire/help.bmp"),
        (
            "firebee",
            "install/firebee/bg06.bmp",
            "install/firebee/bg06_1.bmp",
        ),
    ] {
        let snapshot = fixture(name);
        let frame = compose(&c, &snapshot, &ui);
        assert!(
            frame.sprites.iter().any(|sp| sp.resource_id == background),
            "{name} background"
        );
        assert!(
            frame.sprites.iter().any(|sp| sp.resource_id == help),
            "{name} original help"
        );
        assert!(
            frame
                .controls
                .iter()
                .any(|v| v.keys.iter().any(|key| key == "Enter")),
            "{name} start input"
        );
    }
}

#[test]
fn map_decorations_pause_offscreen_and_advance_once_after_a_long_frame() {
    let manifest = json!({"resources":{"cd/data/map/test/art.flc":{"sha256":"a"}},"objects":[{"sha256":"a","media":{"frames":5,"width":20,"height":20,"speedRaw":100}}]});
    let maps = json!({"maps":[{"id":"test","sourceResource":"cd/data/map/test/test.mmi","chips":[{"row":0,"column":0}],"chipOrigin":{"x":320,"y":245},"chipSize":{"width":0,"height":0},"legacySections":{"Spirit_0":{"File":"art.flc","Pos":{"signed":[10,10]},"Speed":{"signed":[10]},"Repeat":{"signed":[1]}}}}]});
    let c = SceneContent {
        manifest: &manifest,
        maps: &maps,
        rules: &Value::Null,
    };
    let mut snapshot = json!({"mapId":"test","tileId":0,"pending":null,"mapDecorations":{"mapId":"test","loadSerial":1,"delaysMs":[5000]}});
    let mut ui = PresentationState {
        screen: Screen::Game,
        ..Default::default()
    };
    update_presentation(&c, &snapshot, &mut ui, 16);
    assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 0);
    update_presentation(&c, &snapshot, &mut ui, 5000);
    assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 1);
    ui.camera = (1000, 1000);
    update_presentation(&c, &snapshot, &mut ui, 5000);
    assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 1);
    ui.camera = (0, 0);
    snapshot["pending"] = json!({"kind":"dialogue"});
    update_presentation(&c, &snapshot, &mut ui, 5000);
    assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 1);
}

#[test]
fn original_bitmap_animation_metadata_and_fli_jiffy_units_are_normalized() {
    let manifest = json!({"resources":{"a.fli":{"sha256":"x"}},"objects":[{"sha256":"x","media":{"magic":"af11","frames":9,"speedRaw":5}}]});
    let c = SceneContent {
        manifest: &manifest,
        maps: &Value::Null,
        rules: &Value::Null,
    };
    let m = c.media("a.fli");
    assert_eq!(m["frameCount"], 9);
    assert_eq!(m["frameDelayMs"], 72);
}

#[test]
fn dice_and_each_original_walk_segment_have_explicit_display_frames() {
    let c = content();
    let mut snapshot = fixture("controlled-dice");
    snapshot["pending"] = Value::Null;
    let map = a(c.maps, "/maps")
        .iter()
        .find(|map| map["id"] == snapshot["mapId"])
        .unwrap();
    let from = n(&snapshot, "/tileId").max(0) as usize;
    let next = &map["chips"][from]["next"][0];
    let target = a(map, "/chips")
        .iter()
        .position(|tile| tile["row"] == next["row"] && tile["column"] == next["column"])
        .unwrap();
    let events = [
        json!({"type":"diceRolled","rolled":3}),
        json!({"type":"moved","fromTileId":from,"tiles":[target]}),
    ];
    let mut ui = PresentationState {
        screen: Screen::Game,
        ..Default::default()
    };
    accept_response(&c, &snapshot, &snapshot, &events, &mut ui);
    let motion = ui.runtime.motion.as_ref().unwrap();
    let die = n(&c.media("install/ani/pm06a17.flc"), "/frameCount") as usize;
    assert_eq!(
        motion
            .frames
            .iter()
            .filter(|frame| frame.die.is_some())
            .count(),
        die
    );
    let moving = motion
        .frames
        .iter()
        .filter(|frame| frame.actor_position.is_some())
        .count();
    assert!(moving == 24 || moving == 34);
    assert!(
        compose(&c, &snapshot, &ui)
            .controls
            .iter()
            .all(|control| !control.enabled)
    );
}

fn original_bitmap(c: &SceneContent<'_>, resource: &str) -> virtual_life_core::bmp::Image {
    let hash = c.manifest["resources"][resource]["sha256"]
        .as_str()
        .unwrap();
    let object = &c.manifest["_objects"][hash];
    virtual_life_core::bmp::decode(
        &std::fs::read(root().join(object["path"].as_str().unwrap())).unwrap(),
    )
    .unwrap()
}

#[test]
fn hud_geometry_and_three_frame_crops_match_cutouts_measured_from_original_artwork() {
    let c = content();
    let panel = original_bitmap(&c, "install/bmp/pm06a012.bmp");
    let indices = panel.indices.as_ref().unwrap();
    let width = panel.width as usize;
    let at = |x: usize, y: usize| indices[y * width + x];
    let mut slots = vec![];
    let mut y = 240;
    while y < 398 {
        if at(72, y) != 1 {
            y += 1;
            continue;
        }
        let top = y;
        let mut left = width;
        let mut right = 0;
        while y < 398 && at(72, y) == 1 {
            for x in 14..width {
                if at(x, y) == 1 {
                    left = left.min(x);
                    right = right.max(x);
                }
            }
            y += 1;
        }
        slots.push(Rect::new(
            512 + left as i32,
            top as i32,
            (right - left + 1) as u32,
            (y - top) as u32,
        ));
    }
    assert_eq!(slots.len(), 4);
    let snapshot = fixture("controlled-dice");
    for state in 0..3 {
        for (index, expected) in slots.iter().enumerate() {
            let id = format!("hud-{index}");
            let mut ui = PresentationState {
                screen: Screen::Game,
                ..Default::default()
            };
            if state > 0 {
                ui.hover = Some(id.clone())
            }
            if state == 2 {
                ui.pressed = Some(id.clone())
            }
            let frame = compose(&c, &snapshot, &ui);
            let control = frame.controls.iter().find(|v| v.id == id).unwrap();
            assert_eq!(control.bounds, *expected);
            let sprite = frame.sprites.iter().find(|v| v.id == id).unwrap();
            let artwork = original_bitmap(&c, &sprite.resource_id);
            assert_eq!(artwork.width as u32, expected.width);
            assert_eq!(artwork.height as u32, expected.height * 3);
            assert_eq!(sprite.width, Some(expected.width));
            assert_eq!(sprite.height, Some(expected.height));
            assert_eq!(sprite.frame_count, Some(3));
            assert_eq!(sprite.frame, state);
        }
    }
}

#[test]
fn every_native_game_phase_and_panel_tab_keeps_valid_original_assets_and_commands() {
    let c = content();
    let mut ui = PresentationState {
        screen: Screen::Game,
        ..Default::default()
    };
    let variants: [(&str, &[&str]); 7] = [
        (
            "gas",
            &[
                "ready", "help", "playing", "hit", "miss", "won", "wonWait", "lost", "lostFall",
                "reward",
            ],
        ),
        (
            "fist",
            &[
                "ready",
                "help",
                "playing",
                "resultPause",
                "won",
                "lost",
                "lostShake",
                "lostWait",
                "reward",
            ],
        ),
        ("fire", &["ready", "playing"]),
        ("firebee", &["ready", "help", "playing", "reward"]),
        (
            "relationship",
            &[
                "main",
                "meet",
                "question",
                "notice",
                "dates",
                "dateConfirm",
                "marriages",
                "marriageConfirm",
            ],
        ),
        ("inventory", &["ordinary", "special", "equipment"]),
        (
            "status",
            &["overview", "skills", "careerHistory", "house", "investment"],
        ),
    ];
    for (name, phases) in variants {
        for phase in phases {
            let mut snapshot = fixture(name);
            let field = if name == "status" {
                "page"
            } else if name == "inventory" {
                "tab"
            } else {
                "phase"
            };
            snapshot["pending"]["state"][field] = json!(phase);
            if name == "relationship" && *phase == "question" {
                snapshot["pending"]["question"] =
                    json!({"prompt":"问题","answers":["甲","乙","丙"]});
            }
            if name == "inventory" {
                snapshot["pending"]["state"]["entries"] = json!([{ "id":0,"frame":0 }]);
            }
            update_presentation(&c, &snapshot, &mut ui, 16);
            let frame = compose(&c, &snapshot, &ui);
            check_resources(&c, &frame, &format!("{name}/{phase}"));
        }
    }
}

#[test]
fn held_controls_and_hammer_strikes_preserve_pointer_down_and_release_semantics() {
    let c = content();
    let ui = PresentationState {
        screen: Screen::Game,
        pointer: (230, 150),
        ..Default::default()
    };
    for name in ["fist", "firebee"] {
        let mut snapshot = fixture(name);
        snapshot["pending"]["state"]["phase"] = json!("playing");
        let frame = compose(&c, &snapshot, &ui);
        for side in ["left", "right"] {
            let ctrl = frame
                .controls
                .iter()
                .find(|v| v.id == format!("{name}-{side}"))
                .unwrap();
            assert!(ctrl.on_press.is_some());
            assert!(ctrl.on_release.is_some());
        }
    }
    let snapshot = json!({"pending":{"kind":"minigame","game":"hammer","state":{"phase":"playing","targets":[{"x":100,"y":100,"phase":0,"kind":0,"impactUntil":null,"scoreFrame":null}]}}});
    let frame = compose(&c, &snapshot, &ui);
    for id in ["hammer-field", "hammer-hole-0"] {
        assert!(
            frame
                .controls
                .iter()
                .find(|v| v.id == id)
                .unwrap()
                .on_press
                .is_some(),
            "{id}"
        );
    }
}

#[test]
fn longest_original_n1_dialogue_draws_all_five_bitmap_rows_without_clipping() {
    let c = content();
    let events: Value =
        serde_json::from_slice(&std::fs::read(root().join("assets/catalog/events.json")).unwrap())
            .unwrap();
    let raw = a(&events, "/dialogues/records")
        .iter()
        .find(|v| n(v, "/id") == 13110)
        .unwrap();
    let parsed = virtual_life_core::legacy_vm::split_dialogue(s(raw, "/text"));
    let speaker = match parsed.speaker {
        Some(virtual_life_core::legacy_vm::DialogueSpeaker::Player) => "测试".into(),
        Some(virtual_life_core::legacy_vm::DialogueSpeaker::Named { name }) => name,
        None => String::new(),
    };
    let snapshot =
        json!({"scene":{},"pending":{"kind":"dialogue","text":parsed.text,"speaker":speaker}});
    let mut frame = SceneFrame::default();
    map::event(&mut frame, &c, &snapshot, &PresentationState::default());
    let lines: Vec<_> = frame
        .texts
        .iter()
        .filter(|t| t.id.starts_with("event-text-"))
        .collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[4].bounds.y, 396);
    for line in &lines {
        assert!(matches!(line.font, TextFont::Original));
        assert!(line.bounds.y + 16 <= 416);
    }
    let mut bank = crate::pixels::ImageBank::default();
    for resource in [
        "install/font/ascfont.16",
        "install/font/stdfont.16",
        "install/bmp/frame.bmp",
    ] {
        let sha = &c.manifest["resources"][resource]["sha256"];
        let object = a(c.manifest, "/objects")
            .iter()
            .find(|object| object["sha256"] == *sha)
            .unwrap();
        bank.insert(
            resource.into(),
            std::fs::read(root().join(s(object, "/path"))).unwrap(),
        );
    }
    let ascii = bank.bytes("install/font/ascfont.16").unwrap();
    let chinese = bank.bytes("install/font/stdfont.16").unwrap();
    let palette = bank.palette("install/bmp/frame.bmp").unwrap();
    // Independent oracle follows original byte slicing and bitmap bit loops;
    // it does not use the scene's text layout or its glyph renderer.
    let mut expected = vec![0; 640 * 480 * 4];
    for alpha in expected.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }
    let mut draw = |bytes: &[u8], mut x: usize, y: usize, color: [u8; 4]| {
        let mut i = 0;
        while i < bytes.len() {
            let (font, start, width, row_bytes) = if bytes[i] < 0xa1 {
                (&ascii[..], bytes[i] as usize * 16, 8, 1)
            } else {
                let a = bytes[i] as usize;
                let b = bytes[i + 1] as usize;
                (&chinese[..], ((a - 0xa1) * 94 + b - 0xa1) * 32, 16, 2)
            };
            for gy in 0..16 {
                for gx in 0..width {
                    if font[start + gy * row_bytes + gx / 8] & (128 >> (gx % 8)) != 0 {
                        let p = ((y + gy) * 640 + x + gx) * 4;
                        expected[p..p + 4].copy_from_slice(&color);
                    }
                }
            }
            i += row_bytes;
            x += width;
        }
    };
    let (body, _, bad) = encoding_rs::GBK.encode(s(&snapshot, "/pending/text"));
    assert!(!bad);
    assert_eq!(body.len(), 160);
    for (row, start) in [0, 32, 68, 104, 140].into_iter().enumerate() {
        let count = if row == 0 { 32 } else { 36 };
        draw(
            &body[start..(start + count).min(body.len())],
            if row == 0 { 204 } else { 172 },
            if row == 0 { 324 } else { 324 + row * 18 },
            palette[255],
        );
    }
    if !speaker.is_empty() {
        let (bytes, _, bad) = encoding_rs::GBK.encode(&speaker);
        assert!(!bad);
        draw(&bytes, 172, 306, palette[1]);
    }
    frame.sprites.clear();
    frame.sounds.clear();
    frame.controls.clear();
    let mut canvas = crate::pixels::PixelCanvas::default();
    canvas.compose(&frame, &mut bank);
    assert_eq!(
        canvas.rgba, expected,
        "The longest real dialogue, including its final row, must match the original bitmap glyph pixels"
    );
}

#[test]
fn public_n1_choice_cycles_and_submits_the_real_selected_branch() {
    let c = content();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root().join("tests/fixtures/bevy-n1-choice.json")).unwrap(),
    )
    .unwrap();
    let save = fixture["save"].as_str().unwrap();
    let mut catalog = c.maps.clone();
    catalog["rules"] = c.rules.clone();
    for name in ["events", "questionnaires"] {
        catalog[name] = serde_json::from_slice::<Value>(
            &std::fs::read(root().join(format!("assets/catalog/{name}.json"))).unwrap(),
        )
        .unwrap();
    }
    let mut actual = virtual_life_core::GameSession::from_content(catalog.clone()).unwrap();
    assert!(actual.restore(save).ok);
    let mut expected = virtual_life_core::GameSession::from_content(catalog).unwrap();
    assert!(expected.restore(save).ok);
    let snapshot = serde_json::to_value(actual.snapshot()).unwrap();
    let options = a(&snapshot, "/pending/options");
    assert!(options.len() >= 2 && options.iter().all(|v| v["nodeId"].is_i64()));
    let mut ui = PresentationState {
        screen: Screen::Game,
        ..Default::default()
    };
    let render = |ui: &PresentationState| {
        let mut f = SceneFrame::default();
        map::event(&mut f, &c, &snapshot, ui);
        f
    };
    let local = |ui: &mut PresentationState, id: &str| {
        let frame = render(ui);
        let control = frame
            .controls
            .iter()
            .find(|v| v.id == id && v.enabled)
            .unwrap();
        let Action::Ui(UiAction::SetField { field, value }) = &control.action else {
            panic!("Selection must remain presentation-only")
        };
        ui.profile[field] = value.clone();
    };
    local(&mut ui, "event-choice-up");
    assert_eq!(
        n(&ui.profile, "/_eventChoice/selection"),
        options.len() as i32 - 1
    );
    local(&mut ui, "event-choice-down");
    assert_eq!(n(&ui.profile, "/_eventChoice/selection"), 0);
    local(&mut ui, "event-choice-down");
    assert_eq!(n(&ui.profile, "/_eventChoice/selection"), 1);
    assert_eq!(
        actual.save(),
        expected.save(),
        "Cycling the cursor cannot consume a game command or RNG draw"
    );
    let restored_ui: PresentationState =
        serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
    let frame = render(&restored_ui);
    let arrow = frame
        .sprites
        .iter()
        .find(|s| s.id == "event-selector")
        .unwrap();
    assert_eq!((arrow.x, arrow.y), (176, 344));
    assert_eq!(arrow.resource_id, "install/ani/select.flc");
    for (i, option) in options.iter().enumerate() {
        let text = frame
            .texts
            .iter()
            .find(|v| v.id == format!("choice-{i}"))
            .unwrap();
        assert_eq!((text.bounds.x, text.bounds.y), (210, 324 + i as i32 * 24));
        assert_eq!(text.text, s(option, "/text"));
        assert!(matches!(text.font, TextFont::Original));
    }
    let submit = frame
        .controls
        .iter()
        .find(|v| v.id == "event-choice-submit")
        .unwrap();
    let Action::Game(mut command) = submit.action.clone() else {
        panic!("Enter must submit a real core command")
    };
    command["seq"] = json!(0);
    let response = actual.send(serde_json::from_value(command).unwrap());
    assert!(response.ok, "N1 UI submission failed: {:?}", response.error);
    let expected = expected.send(virtual_life_core::GameCommand::ChooseOption {
        seq: 0,
        index: n(&options[1], "/index") as usize,
    });
    assert!(expected.ok);
    assert_eq!(
        serde_json::to_value(response.snapshot).unwrap(),
        serde_json::to_value(expected.snapshot).unwrap(),
        "The cursor must commit the same original branch as its real option index"
    );
}

#[test]
fn public_minigame_reward_skip_finishes_its_original_counting_panels() {
    let c = content();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root().join("tests/fixtures/bevy-battle-controls.json")).unwrap(),
    )
    .unwrap();
    let mut catalog = c.maps.clone();
    catalog["rules"] = c.rules.clone();
    for name in ["events", "questionnaires"] {
        catalog[name] = serde_json::from_slice::<Value>(
            &std::fs::read(root().join(format!("assets/catalog/{name}.json"))).unwrap(),
        )
        .unwrap();
    }
    let mut session = virtual_life_core::GameSession::from_content(catalog).unwrap();
    assert!(
        session
            .restore(fixture["cases"]["gasReward"]["save"].as_str().unwrap())
            .ok
    );
    let mut count = 0;
    for _ in 0..8 {
        let snapshot = serde_json::to_value(session.snapshot()).unwrap();
        if s(&snapshot, "/pending/kind") != "minigame"
            || s(&snapshot, "/pending/state/phase") != "reward"
        {
            break;
        }
        for _ in 0..3 {
            if n(
                &serde_json::to_value(session.snapshot()).unwrap(),
                "/pending/state/reward/valuePhase",
            ) == 2
            {
                break;
            }
            assert!(
                session
                    .frame(virtual_life_core::FrameStep { elapsed_ms: 1601 })
                    .ok
            );
        }
        let snapshot = serde_json::to_value(session.snapshot()).unwrap();
        assert_eq!(n(&snapshot, "/pending/state/reward/valuePhase"), 2);
        let mut frame = SceneFrame::default();
        minigames::compose(&mut frame, &c, &snapshot, &PresentationState::default());
        let control = frame
            .controls
            .iter()
            .find(|v| v.id == "minigame-finish-reward")
            .unwrap();
        assert!(control.keys.iter().any(|key| key == "Escape"));
        assert!(control.keys.iter().any(|key| key == "MouseRight"));
        let Action::Game(mut command) = control.action.clone() else {
            panic!("Reward input must dispatch to the core")
        };
        command["seq"] = json!(0);
        let response = session.send(serde_json::from_value(command).unwrap());
        assert!(
            response.ok,
            "Minigame reward input rejected: {:?}",
            response.error
        );
        assert!(
            session
                .frame(virtual_life_core::FrameStep { elapsed_ms: 16 })
                .ok
        );
        assert_eq!(
            n(
                &serde_json::to_value(session.snapshot()).unwrap(),
                "/pending/state/reward/valuePhase"
            ),
            3
        );
        assert!(
            session
                .frame(virtual_life_core::FrameStep { elapsed_ms: 1501 })
                .ok
        );
        count += 1;
    }
    let snapshot = serde_json::to_value(session.snapshot()).unwrap();
    assert!(count > 0);
    assert!(
        s(&snapshot, "/pending/game") != "gas" || s(&snapshot, "/pending/state/phase") != "reward"
    );
}
