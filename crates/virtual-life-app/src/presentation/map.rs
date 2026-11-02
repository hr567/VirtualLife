use super::*;

fn map_for<'a>(c: &'a SceneContent<'_>, snapshot: &Value) -> &'a Value {
    a(c.maps, "/maps")
        .iter()
        .find(|m| m["id"] == snapshot["mapId"])
        .unwrap_or(&Value::Null)
}
pub(super) fn world(map: &Value, tile: usize) -> (i32, i32) {
    let tile = &map["chips"][tile];
    let col = n(tile, "/column") + 1;
    let row = n(tile, "/row") + 1;
    (
        n(map, "/chipOrigin/x") + (n(map, "/chipSize/width") * col).div_euclid(2)
            - col.div_euclid(2),
        n(map, "/chipOrigin/y") + (n(map, "/chipSize/height") * row).div_euclid(2),
    )
}
pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let map = map_for(c, snapshot);
    let actor = if snapshot["_actorPosition"].is_array() {
        (
            n(snapshot, "/_actorPosition/0"),
            n(snapshot, "/_actorPosition/1"),
        )
    } else {
        world(map, n(snapshot, "/tileId").max(0) as usize)
    };
    let (cx, cy) = if ui.camera == (0, 0) && !b(&ui.profile, "/_cameraManual") {
        (actor.0 - 320, actor.1 - 245)
    } else {
        ui.camera
    };
    for (i, tile) in a(map, "/backgroundTiles").iter().enumerate() {
        let index = sprite(
            f,
            &format!("map-{i}"),
            s(tile, "/resourceId"),
            n(tile, "/x") - cx,
            n(tile, "/y") - cy,
            0,
        );
        let sp = &mut f.sprites[index];
        sp.transparent = Transparency::Opaque;
        sp.width = Some(n(tile, "/width").max(1) as u32);
        sp.height = Some(n(tile, "/height").max(1) as u32);
    }
    let stage = n(snapshot, "/life/stage");
    let profession = n(snapshot, "/life/profession");
    let character = if s(snapshot, "/player/sex") == "female" {
        31
    } else {
        0
    } + stage
        + if profession >= 0 { profession } else { 0 };
    let facing = n(snapshot, "/mapFacing").clamp(0, 3) as usize;
    let directory = c.rules["characters"]["directories"][character as usize]
        .as_str()
        .unwrap_or("");
    let suffix = c.rules["characters"]["animationSuffixes"][facing]
        .as_str()
        .unwrap_or("");
    let model = directory
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("m00");
    let resource = format!(
        "{directory}{}",
        suffix.replacen("M00", model, 1).replacen("m00", model, 1)
    );
    let actor_resource = s(snapshot, "/_actorResource");
    let index = sprite(
        f,
        "map-player",
        if actor_resource.is_empty() {
            c.resolve(&resource)
        } else {
            actor_resource.into()
        },
        actor.0 - 100 - cx,
        actor.1 - 149 - cy,
        30,
    );
    f.sprites[index].frame = n(snapshot, "/_actorFrame").max(0) as u32;
    f.sprites[index].transparent = Transparency::FirstPixel;
    for item in &ui.runtime.map.decorations {
        let index = sprite(
            f,
            &item.id,
            &item.resource,
            item.position.0 - cx,
            item.position.1 - cy,
            if item.foreground { 60 } else { 10 },
        );
        let sp = &mut f.sprites[index];
        sp.transparent = Transparency::FirstPixel;
        sp.frame = item.displayed_frame;
    }
    let palette = s(&map["backgroundTiles"][0], "/resourceId");
    for cycle in &ui.runtime.map.cycles {
        for index in cycle.first..=cycle.last {
            f.palette_overrides.push(PaletteOverride {
                resource_id: palette.into(),
                index,
                source_index: cycle.mapping[index as usize],
            });
        }
    }
    if stage != 2 && !b(snapshot, "/life/traveling") {
        let ages = [1, 1, 1, 2, 2, 4, 3];
        let age = ages.get(stage.max(0) as usize).copied().unwrap_or(1);
        for (i, cell) in a(snapshot, "/spirits/positions").iter().enumerate().take(5) {
            let attached = snapshot["spirits"]["active"].as_u64() == Some(i as u64);
            let resource = c.resolve(&format!(
                "spirit/{age}{}{}.flc",
                ["xr", "dl", "sa", "fd", "yb"][i],
                ["3a", "3b", "3c", "3d"][if attached { facing } else { 0 }]
            ));
            let width = n(&c.media(&resource), "/width");
            let (x, y) = if attached {
                (270, 90)
            } else {
                (
                    (n(map, "/chipSize/width") - width) / 2
                        + n(cell, "/column") * n(map, "/chipSize/width") / 2
                        + n(map, "/chipOrigin/x")
                        - cx,
                    n(cell, "/row") * n(map, "/chipSize/height") / 2 - 50 + n(map, "/chipOrigin/y")
                        - cy,
                )
            };
            let index = sprite(
                f,
                &format!("spirit-{i}"),
                resource,
                x,
                y,
                if attached {
                    40
                } else if y < 190 {
                    20
                } else {
                    50
                },
            );
            f.sprites[index].frame = ui.runtime.map.spirit_frame;
            f.sprites[index].transparent = Transparency::FirstPixel;
        }
    }
    for (resource, x, y, w, h) in [
        ("pm06a011", 0, 0, 4, 480),
        ("pm06a013", 0, 476, 640, 4),
        ("pm06a012", 512, 0, 128, 480),
        ("pm06a010", 0, 0, 640, 30),
    ] {
        let index = sprite(
            f,
            resource,
            format!("install/bmp/{resource}.bmp"),
            x,
            y,
            100,
        );
        f.sprites[index].width = Some(w);
        f.sprites[index].height = Some(h);
        if resource == "pm06a011" || resource == "pm06a013" {
            f.sprites[index].transparent = Transparency::Opaque;
        }
    }
    if let Some(portrait) = c.rules["characters"]["portraits"][character as usize].as_str() {
        sprite(f, "portrait", c.resolve(portrait), 541, 35, 101);
    }
    sprite(f, "hp-frame", "install/bmp/pm06a04.bmp", 156, 4, 102);
    let hp = (n(snapshot, "/stats/hp") * 102 / n(snapshot, "/stats/maxHp").max(1)).clamp(0, 102);
    if hp > 0 {
        let i = sprite(f, "hp-bar", "install/bmp/pm06a05.bmp", 161, 7, 103);
        f.sprites[i].source = Some(Rect::new(0, 0, hp as u32, 9));
        f.sprites[i].width = Some(hp as u32);
        f.sprites[i].height = Some(9);
    }
    for (key, right) in [
        ("money", 101),
        ("intelligence", 360),
        ("morality", 452),
        ("charm", 544),
        ("stamina", 636),
    ] {
        let value = snapshot["stats"][key]
            .as_i64()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into());
        let x = right - value.len() as i32 * 8;
        text(f, key, value, Rect::new(x, 5, 120, 18), [255; 4]);
    }
    text(
        f,
        "name",
        s(snapshot, "/player/name"),
        Rect::new(537, 159, 88, 18),
        [255; 4],
    );
    f.texts.last_mut().unwrap().centered = true;
    text(
        f,
        "age",
        format!("{}岁", n(snapshot, "/stats/age")),
        Rect::new(535, 216, 80, 18),
        [255; 4],
    );
    f.texts.last_mut().unwrap().centered = true;
    for (i, (label, y, action)) in [
        ("参数一览", 247, Action::Game(json!({"type":"openStatus"}))),
        (
            "道具一览",
            284,
            Action::Game(json!({"type":"openInventory"})),
        ),
        ("地图浏览", 321, Action::Ui(UiAction::MapOverview)),
        ("系统操作", 358, Action::Ui(UiAction::Options)),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("hud-{i}");
        let idx = sprite(
            f,
            &id,
            format!("install/bmp/pm06a{:02}.bmp", i + 9),
            558,
            y,
            104,
        );
        let sp = &mut f.sprites[idx];
        sp.width = Some(52);
        sp.height = Some(30);
        sp.frame_count = Some(3);
        sp.frame = if ui.pressed.as_deref() == Some(&id) {
            2
        } else if ui.hover.as_deref() == Some(&id) {
            1
        } else {
            0
        };
        control(f, &id, label, Rect::new(558, y, 52, 30), action, &[]);
    }
    if snapshot["pending"].is_null()
        && s(snapshot, "/phase") == "playing"
        && ui.runtime.motion.is_none()
    {
        let cannon = sprite(f, "dice-cannon", "install/ani/pm06a14.flc", 544, 373, 105);
        f.sprites[cannon].transparent = Transparency::FirstPixel;
        f.sprites[cannon].frame = if ui.runtime.map.cannon_wrapped {
            n(&c.media("install/ani/pm06a14.flc"), "/frameCount").max(1) as u32 - 1
        } else {
            ui.runtime.map.cannon_frame
        };
    }
    let controlled =
        snapshot["spirits"]["active"].as_u64() == Some(0) && !b(snapshot, "/life/traveling");
    let dice_open = b(&ui.profile, "/_diceChoices");
    if controlled && dice_open {
        for i in 0..8 {
            let id = format!("dice-{i}");
            let index = sprite(
                f,
                &id,
                format!("install/bmp/zhan0{}.bmp", i + 1),
                120 + i * 51,
                407,
                110,
            );
            f.sprites[index].width = Some(51);
            f.sprites[index].height = Some(63);
            f.sprites[index].frame_count = Some(3);
            f.sprites[index].frame = if ui.pressed.as_deref() == Some(&id) {
                2
            } else if ui.hover.as_deref() == Some(&id) {
                1
            } else {
                0
            };
            control(
                f,
                &id,
                &format!("选择 {} 点", i + 1),
                Rect::new(120 + i * 51, 407, 51, 63),
                Action::Game(json!({"type":"rollDice","value":i+1})),
                &[&format!("{}", i + 1)],
            );
        }
    } else {
        control(
            f,
            "roll",
            "掷骰子",
            Rect::new(549, 402, 73, 68),
            if controlled {
                Action::Ui(UiAction::SetField {
                    field: "_diceChoices".into(),
                    value: json!(true),
                })
            } else {
                Action::Game(json!({"type":"rollDice"}))
            },
            &["Space", "Enter"],
        );
        f.controls.last_mut().unwrap().enabled =
            s(snapshot, "/phase") == "playing" && snapshot["pending"].is_null();
    }
    if let Some(stage) = a(c.rules, "/stages").iter().find(|v| n(v, "/id") == stage) {
        sound(f, "map-music", c.resolve(s(stage, "/musicPath")), true, 0);
    }
    if s(snapshot, "/pending/kind") == "allocation" {
        super::native_allocation::compose(f, c, snapshot, ui);
    } else if s(snapshot, "/pending/kind") == "blocked" {
        text(
            f,
            "blocked",
            s(snapshot, "/pending/text"),
            Rect::new(160, 116, 320, 200),
            [255; 4],
        );
        control(
            f,
            "blocked-save",
            "保存进度",
            Rect::new(230, 330, 180, 40),
            Action::Ui(UiAction::Save),
            &[],
        );
    }
}
pub(super) fn event(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    if snapshot["pending"]["spirit"].as_u64().is_some() {
        // 0041ea10 draws the 374x348 description at (0x85, 0x42) over
        // the current map. It does not use the N1 frame or dialogue box.
        compose(f, c, snapshot, ui);
        f.controls.clear();
        f.tick = None;
        let resource = c.resolve(s(snapshot, "/scene/backgroundResource"));
        sprite(f, "spirit-description", resource, 133, 66, 150);
        control(
            f,
            "spirit-continue",
            "继续",
            Rect::new(0, 0, 640, 480),
            Action::Game(json!({"type":"chooseOption","index":0})),
            &["Enter", "Space", "Escape"],
        );
        return;
    }
    let scene = &snapshot["scene"];
    let pattern = c.resolve(s(scene, "/patternResource"));
    if !pattern.is_empty() {
        let media = c.media(&pattern);
        if n(&media, "/width") == 80 && n(&media, "/height") == 160 {
            let step = ui.runtime.pattern.counter;
            let x = ui.runtime.pattern.x;
            let y = ui.runtime.pattern.y;
            for row in -1..7 {
                for col in -1..9 {
                    let index = sprite(
                        f,
                        &format!("pattern-{row}-{col}"),
                        &pattern,
                        col * 80 - x % 80,
                        row * 80 - y % 80,
                        0,
                    );
                    let sp = &mut f.sprites[index];
                    sp.source = Some(Rect::new(0, if step % 3 > 1 { 80 } else { 0 }, 80, 80));
                    sp.width = Some(80);
                    sp.height = Some(80);
                    sp.transparent = Transparency::Opaque;
                }
            }
        } else {
            let i = sprite(f, "event-pattern", pattern, 0, 0, 0);
            f.sprites[i].transparent = Transparency::Opaque;
        }
    }
    sprite(f, "event-frame", "install/bmp/frame.bmp", 0, 0, 1);
    super::events::background(f, c, scene, ui);
    for (key, x) in [("leftPortraitResource", 70), ("rightPortraitResource", 482)] {
        let path = s(scene, &format!("/{key}"));
        if !path.is_empty() {
            sprite(f, key, c.resolve(path), x, 304, 10);
        }
    }
    let i = sprite(f, "event-talk", "install/bmp/talk.bmp", 166, 304, 11);
    f.sprites[i].transparent = Transparency::Index(0);
    let choice = s(snapshot, "/pending/kind") == "choice";
    if choice {
        let options = a(snapshot, "/pending/options");
        // Other non-N1 host prompts still carry
        // meaningful explanatory text. N1 itself draws only its four options.
        if !options.iter().any(|option| option.get("nodeId").is_some()) {
            text(
                f,
                "event-text",
                s(snapshot, "/pending/text"),
                Rect::new(181, 278, 278, 24),
                [255; 4],
            );
        }
        let count = options.len();
        let scope =
            json!([snapshot["revision"], snapshot["turn"], snapshot["pending"]]).to_string();
        let selection = if s(&ui.profile, "/_eventChoice/scope") == scope {
            n(&ui.profile, "/_eventChoice/selection").max(0) as usize
        } else {
            0
        }
        .min(count.saturating_sub(1));
        let select = |selection: usize| {
            Action::Ui(UiAction::SetField {
                field: "_eventChoice".into(),
                value: json!({"scope":scope,"selection":selection}),
            })
        };
        let arrow = sprite(
            f,
            "event-selector",
            "install/ani/select.flc",
            176,
            320 + selection as i32 * 24,
            20,
        );
        f.sprites[arrow].playback = Some(Playback {
            interval_ms: 200,
            max_advances: 1,
            ..Default::default()
        });
        for (i, option) in options.iter().enumerate() {
            let y = 324 + i as i32 * 24;
            text(
                f,
                &format!("choice-{i}"),
                s(option, "/text"),
                Rect::new(210, y, 264, 16),
                [255; 4],
            );
            let t = f.texts.last_mut().unwrap();
            t.font = TextFont::Original;
            t.palette = Some("install/bmp/frame.bmp".into());
            t.palette_index = Some(255);
            t.clip = Some(Rect::new(166, 304, 308, 112));
            control(
                f,
                &format!("choice-{i}"),
                s(option, "/text"),
                Rect::new(196, y - 4, 278, 24),
                Action::Game(json!({"type":"chooseOption","index":n(option,"/index")})),
                &[],
            );
            f.controls.last_mut().unwrap().enabled = !b(option, "/disabled");
            f.controls.last_mut().unwrap().on_hover = Some(select(i));
        }
        if count > 0 {
            control(
                f,
                "event-choice-submit",
                &format!("确定选择：{}", s(&options[selection], "/text")),
                Rect::new(0, 0, 0, 0),
                Action::Game(
                    json!({"type":"chooseOption","index":n(&options[selection],"/index")}),
                ),
                &["Enter:up"],
            );
            f.controls.last_mut().unwrap().enabled = !b(&options[selection], "/disabled");
            for (id, next, keys) in [
                (
                    "up",
                    (selection + count - 1) % count,
                    ["ArrowUp:up", "WheelUp"],
                ),
                (
                    "down",
                    (selection + 1) % count,
                    ["ArrowDown:up", "WheelDown"],
                ),
            ] {
                control(
                    f,
                    &format!("event-choice-{id}"),
                    if id == "up" {
                        "上一个选项"
                    } else {
                        "下一个选项"
                    },
                    Rect::new(0, 0, 0, 0),
                    select(next),
                    &keys,
                );
            }
        }
    } else {
        dialogue_text(
            f,
            s(snapshot, "/pending/text"),
            s(snapshot, "/pending/speaker"),
        );
        let i = sprite(f, "event-next", "install/ani/3.flc", 446, 396, 20);
        f.sprites[i].playback = Some(Playback {
            interval_ms: 250,
            once: false,
            max_advances: 1,
            immediate: false,
        });
        control(
            f,
            "event-continue",
            "继续对话",
            Rect::new(0, 0, 640, 480),
            Action::Game(json!({"type":"advanceEvent"})),
            &["Enter", "Space"],
        );
    }
}

/// N1 opcode 4: 0040b430 -> 00409d70 -> 0040e580 uses the original
/// 8x16/16x16 bitmap font. The first line is 32 GBK bytes; the next four
/// are 36 bytes, 18 pixels apart. These offsets are relative to talk.bmp.
fn dialogue_text(f: &mut SceneFrame, body: &str, speaker: &str) {
    let clip = Rect::new(166, 304, 308, 112);
    let mut line = |id: &str, value: String, x: i32, y: i32, width: u32, index: u8| {
        text(f, id, value, Rect::new(x, y, width, 16), [255; 4]);
        let t = f.texts.last_mut().unwrap();
        t.font = TextFont::Original;
        t.line_height = 16;
        t.clip = Some(clip);
        t.palette = Some("install/bmp/frame.bmp".into());
        t.palette_index = Some(index);
    };
    if !speaker.is_empty() {
        line("event-speaker", speaker.into(), 172, 306, 296, 1);
    }
    let (encoded, _, _) = encoding_rs::GBK.encode(body);
    for (row, start) in [0, 32, 68, 104, 140].into_iter().enumerate() {
        if start >= encoded.len() {
            break;
        }
        let count = if row == 0 { 32 } else { 36 };
        let bytes = &encoded[start..(start + count).min(encoded.len())];
        let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
        line(
            &format!("event-text-{row}"),
            decoded.into_owned(),
            if row == 0 { 204 } else { 172 },
            if row == 0 { 324 } else { 324 + row as i32 * 18 },
            count as u32 * 8,
            255,
        );
    }
}

pub(super) fn overview(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let map = map_for(c, snapshot);
    let stage = n(snapshot, "/life/stage");
    let special = b(snapshot, "/life/traveling");
    let meta = a(c.rules, "/mapOverview/maps").iter().find(|v| {
        v["mapId"] == snapshot["mapId"]
            && b(v, "/special") == special
            && (special || n(v, "/index") == stage)
    });
    if let Some(meta) = meta {
        let palette = s(&meta["tiles"][0], "/resource");
        f.palette = Some(palette.into());
        for (i, tile) in a(meta, "/tiles").iter().enumerate() {
            if let Some(index) = image(f, &format!("overview-{i}"), tile, 0) {
                f.sprites[index].width = Some(n(meta, "/tileWidth") as u32);
                f.sprites[index].height = Some(n(meta, "/tileHeight") as u32);
                f.sprites[index].transparent = Transparency::Opaque;
            }
        }
        let cols = n(meta, "/columns").max(1);
        let rows = n(meta, "/rows").max(1);
        let actor = world(map, n(snapshot, "/tileId").max(0) as usize);
        let camera = if ui.camera == (0, 0) && !b(&ui.profile, "/_cameraManual") {
            (actor.0 - 320, actor.1 - 245)
        } else {
            ui.camera
        };
        let x = camera.0 / cols;
        let y = camera.1 / rows;
        let width = n(meta, "/tileWidth");
        let height = n(meta, "/tileHeight");
        for (id, left, top, w, h) in [
            ("top", x, y, width + 1, 1),
            ("bottom", x, y + height, width + 1, 1),
            ("left", x, y, 1, height + 1),
            ("right", x + width, y, 1, height + 1),
        ] {
            f.fills.push(SceneFill {
                id: format!("overview-box-{id}"),
                bounds: Rect::new(left, top, w.max(1) as u32, h.max(1) as u32),
                color: [255; 4],
                z: 1,
                palette: Some(palette.into()),
                palette_index: Some(242 + ui.runtime.overview_color),
            });
        }
        let markers = &c.rules["mapOverview"]["markers"];
        let player = (x + width / 2 - 24, y + height / 2 - 48);
        let mut marker = markers[0].clone();
        marker["x"] = json!(player.0);
        marker["y"] = json!(player.1);
        marker["frame"] = json!(ui.runtime.overview_frame);
        marker["transparent"] = json!("firstPixel");
        animation(f, "overview-player", &marker, 7);
        if stage != 2 && !special {
            for (i, cell) in a(snapshot, "/spirits/positions").iter().enumerate().take(5) {
                let mut marker = markers[i + 1].clone();
                let attached = snapshot["spirits"]["active"].as_u64() == Some(i as u64);
                let world = (
                    (n(map, "/chipSize/width") - 100) / 2
                        + n(cell, "/column") * n(map, "/chipSize/width") / 2
                        + n(map, "/chipOrigin/x"),
                    n(cell, "/row") * n(map, "/chipSize/height") / 2 - 50 + n(map, "/chipOrigin/y"),
                );
                let position = if attached {
                    (player.0, y + height / 2 - 72)
                } else {
                    (world.0 / cols, world.1 / rows)
                };
                marker["x"] = json!(position.0);
                marker["y"] = json!(position.1);
                marker["frame"] = json!(ui.runtime.overview_frame);
                marker["transparent"] = json!("firstPixel");
                animation(f, &format!("overview-spirit-{i}"), &marker, 2 + i as i32);
            }
        }
    }
    control(
        f,
        "overview-back",
        "关闭地图浏览",
        Rect::new(0, 0, 640, 480),
        Action::Ui(UiAction::Back),
        &["Escape"],
    );
}
