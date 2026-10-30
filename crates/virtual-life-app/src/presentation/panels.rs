use super::*;

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    match s(snapshot, "/pending/kind") {
        "inventory" => inventory(f, c, snapshot, ui),
        "status" => status(f, c, snapshot, ui),
        "relationship" => relationship(f, c, snapshot, ui),
        _ => {}
    }
}
fn indexed(
    f: &mut SceneFrame,
    id: &str,
    value: impl ToString,
    x: i32,
    y: i32,
    color: u8,
    width: Option<i32>,
    palette: &str,
) {
    let value = value.to_string();
    let length = value
        .chars()
        .map(|ch| if ch.is_ascii() { 8 } else { 16 })
        .sum::<i32>();
    text(
        f,
        id,
        value,
        Rect::new(
            x + width.map(|w| (w * 8 - length) / 2).unwrap_or(0),
            y,
            length.max(1) as u32,
            16,
        ),
        [255; 4],
    );
    let t = f.texts.last_mut().unwrap();
    t.palette = Some(palette.into());
    t.palette_index = Some(color);
    t.font = TextFont::Original;
    t.z = 10;
}
fn item_button(
    f: &mut SceneFrame,
    id: &str,
    label: &str,
    meta: &Value,
    action: Action,
    selected: bool,
    ui: &PresentationState,
) {
    button(f, id, meta, action, label, 6, ui);
    if selected {
        if let Some(sp) = f.sprites.last_mut() {
            sp.frame = 2
        }
        if let Some(ctrl) = f.controls.last_mut() {
            ctrl.enabled = false
        }
    }
}
fn back(
    f: &mut SceneFrame,
    metadata: &Value,
    tracks: &Value,
    x: Option<i32>,
    action: Action,
    label: &str,
    ui: &PresentationState,
) {
    let hovered = ui.hover.as_deref() == Some("panel-back");
    let mut v = metadata[usize::from(hovered)].clone();
    if let Some(x) = x {
        v["x"] = json!(x)
    }
    v["frame"] = json!(n(&tracks[usize::from(hovered)], "/frame"));
    animation(f, "panel-back", &v, 5);
    control(
        f,
        "panel-back",
        label,
        Rect::new(
            n(&v, "/x"),
            n(&v, "/y"),
            n(&v, "/width") as u32,
            n(&v, "/height") as u32,
        ),
        action,
        &["Escape"],
    );
}

fn inventory(f: &mut SceneFrame, c: &SceneContent<'_>, snapshot: &Value, ui: &PresentationState) {
    let st = &snapshot["pending"]["state"];
    let rules = &c.rules["inventory"];
    let r = &rules["rendering"];
    let tab = s(st, "/tab");
    let tabs = ["ordinary", "special", "equipment"];
    let equipment = tab == "equipment";
    let background = &r["backgrounds"][tabs.iter().position(|v| *v == tab).unwrap_or(0)];
    image(f, "inventory-background", background, 0);
    let palette = s(background, "/resource");
    f.palette = Some(palette.into());
    f.palette_sections
        .push((s(&r["confirmation"], "/resource").into(), 191, 201));
    f.palette_sections
        .push((s(&r["back"][0], "/resource").into(), 162, 190));
    if let Some(first) = a(st, "/entries").first() {
        f.palette_sections.push((
            s(&rules[tab][n(first, "/id").max(0) as usize], "/animation").into(),
            2,
            161,
        ));
    }
    if tab == "ordinary" {
        f.palette_sections
            .push((s(&r["hpFill"], "/resource").into(), 142, 161));
    }
    if !equipment {
        for row in 0..8 {
            let mut v = r["rows"].clone();
            v["x"] = json!(43);
            v["y"] = json!(80 + row * 38);
            image(f, &format!("inventory-row-{row}"), &v, 1);
        }
    }
    if tab == "ordinary" {
        image(f, "inventory-hp-label", &r["hpLabel"], 2);
        image(f, "inventory-hp-frame", &r["hpFrame"], 2);
        let width =
            (n(snapshot, "/stats/hp") * 260 / n(snapshot, "/stats/maxHp").max(1)).clamp(0, 260);
        if width > 0 {
            if let Some(i) = image(f, "inventory-hp-fill", &r["hpFill"], 3) {
                f.sprites[i].source = Some(Rect::new(0, 0, width as u32, 12));
                f.sprites[i].width = Some(width as u32);
                f.sprites[i].height = Some(12);
            }
        }
    }
    let selected_index = st["selectedIndex"].as_u64().map(|n| n as usize);
    let selected = selected_index.and_then(|index| a(st, "/entries").get(index));
    let definition = selected.map(|entry| &rules[tab][n(entry, "/id").max(0) as usize]);
    if let (Some(entry), Some(definition)) = (selected, definition) {
        let i = sprite(
            f,
            "inventory-preview",
            s(definition, "/animation"),
            st["previewOrigin"]["x"]
                .as_i64()
                .map(|n| n as i32)
                .unwrap_or(if equipment { 312 } else { 302 }),
            st["previewOrigin"]["y"]
                .as_i64()
                .map(|n| n as i32)
                .unwrap_or(47),
            4,
        );
        f.sprites[i].frame = n(entry, "/frame").max(0) as u32;
    }
    let equipped = a(snapshot, "/pending/equipped");
    for (i, entry) in a(st, "/entries").iter().enumerate() {
        let definition = &rules[tab][n(entry, "/id").max(0) as usize];
        let color = if equipment && (b(entry, "/equipped") || equipped.contains(&entry["id"])) {
            211
        } else {
            255
        };
        indexed(
            f,
            &format!("inventory-name-{i}"),
            s(definition, "/name"),
            if equipment {
                49 + i as i32 / 5 * 114
            } else {
                50
            },
            if equipment {
                100 + i as i32 % 5 * 29
            } else {
                86 + i as i32 * 38
            },
            color,
            None,
            palette,
        );
        control(
            f,
            &format!("inventory-entry-{i}"),
            &format!("选择道具 {} {}", s(definition, "/name"), i + 1),
            Rect::new(
                if equipment {
                    46 + i as i32 / 5 * 114
                } else {
                    43
                },
                if equipment {
                    97 + i as i32 % 5 * 29
                } else {
                    80 + i as i32 * 38
                },
                if equipment { 100 } else { 178 },
                if equipment { 21 } else { 27 },
            ),
            game("inventoryAction", "select", Some(i)),
            &[],
        );
    }
    if equipment {
        for slot in 0..6 {
            let id = equipped.get(slot).and_then(Value::as_i64).unwrap_or(-1);
            let name = if id < 0 {
                s(r, "/emptyEquipmentLabel")
            } else {
                s(&rules["equipment"][id as usize], "/name")
            };
            indexed(
                f,
                &format!("equipped-{slot}"),
                name,
                49 + slot as i32 / 3 * 114,
                316 + slot as i32 % 3 * 29,
                255,
                None,
                palette,
            );
        }
    }
    if let Some(selected) = definition {
        for (i, description) in a(selected, "/descriptions").iter().enumerate() {
            indexed(
                f,
                &format!("inventory-description-{i}"),
                description.as_str().unwrap_or(""),
                if equipment { 320 } else { 312 },
                if equipment { 288 } else { 291 } + i as i32 * 20,
                255,
                None,
                palette,
            );
        }
    }
    for (i, tab_name) in tabs.iter().enumerate() {
        item_button(
            f,
            &format!("inventory-tab-{tab_name}"),
            ["一般道具", "特殊道具", "装备道具"][i],
            &r["buttons"][i],
            Action::Game(json!({"type":"inventoryAction","action":"tab","tab":tab_name})),
            *tab_name == tab,
            ui,
        );
    }
    if equipment {
        for (i, (label, action)) in [
            ("装备选中道具", "wear"),
            ("卸下选中装备", "unequip"),
            ("丢弃选中道具", "drop"),
        ]
        .iter()
        .enumerate()
        {
            item_button(
                f,
                &format!("inventory-action-{action}"),
                label,
                &r["equipmentButtons"][i],
                game("inventoryAction", action, None),
                false,
                ui,
            );
            if let Some(ctrl) = f.controls.last_mut() {
                ctrl.enabled = !a(st, "/entries").is_empty();
            }
        }
    } else {
        if tab == "ordinary" {
            item_button(
                f,
                "inventory-use",
                "使用选中道具",
                &r["buttons"][3],
                game("inventoryAction", "use", None),
                false,
                ui,
            );
            if let Some(ctrl) = f.controls.last_mut() {
                ctrl.enabled = !a(st, "/entries").is_empty();
            }
        }
        let mut drop = r["buttons"][4].clone();
        drop["x"] = json!(if tab == "special" { 316 } else { 403 });
        item_button(
            f,
            "inventory-drop",
            "丢弃选中道具",
            &drop,
            game("inventoryAction", "drop", None),
            false,
            ui,
        );
        if let Some(ctrl) = f.controls.last_mut() {
            ctrl.enabled = !a(st, "/entries").is_empty();
        }
    }
    back(
        f,
        &r["back"],
        &st["backTracks"],
        Some(if equipment { 553 } else { 543 }),
        game("inventoryAction", "close", None),
        "返回地图",
        ui,
    );
    if b(st, "/confirmDrop") {
        for ctrl in &mut f.controls {
            ctrl.enabled = false;
        }
        image(f, "inventory-confirmation", &r["confirmation"], 30);
        button(
            f,
            "inventory-confirm-yes",
            &r["confirmYes"],
            game("inventoryAction", "confirmDrop", None),
            "确定丢弃",
            32,
            ui,
        );
        button(
            f,
            "inventory-confirm-no",
            &r["confirmNo"],
            game("inventoryAction", "cancelDrop", None),
            "取消丢弃",
            32,
            ui,
        );
    }
    f.tick = Some(json!({"type":"inventoryTick"}));
}

fn status(f: &mut SceneFrame, c: &SceneContent<'_>, snapshot: &Value, ui: &PresentationState) {
    let st = &snapshot["pending"]["state"];
    let r = &c.rules["statusRendering"];
    let page = s(st, "/page");
    let property = page == "house" || page == "investment";
    let list = if page == "skills" || page == "careerHistory" {
        &r[page]
    } else {
        &Value::Null
    };
    let housing = &snapshot["pending"]["housing"];
    let transition = &st["transition"];
    let background = if page == "house" {
        &r["house"]["styles"][n(housing, "/style").max(0) as usize]["layers"][0]
    } else if page == "investment" {
        &r["investment"]["background"]
    } else if !list.is_null() {
        &list["background"]
    } else {
        &r["background"]
    };
    let palette = s(background, "/resource");
    f.palette = Some(palette.into());
    if page != "house" {
        image(f, "parameters-background", background, 0);
    }
    if page == "overview" {
        let overview = &snapshot["statusOverview"];
        let portrait = &r["portraits"][n(overview, "/portraitIndex").max(0) as usize];
        image(f, "parameters-portrait", portrait, 2);
        f.palette_sections
            .push((s(portrait, "/resource").into(), 202, 221));
        let variant = n(
            &r["hpGdi"]["portraitVariants"][n(overview, "/portraitIndex").max(0) as usize],
            "",
        );
        for kind in ["hpLabel", "hpFrame", "hpFill"] {
            if kind == "hpFill" && n(overview, "/hpWidth") <= 0 {
                continue;
            }
            let rule = &r["hpGdi"]["variants"][variant.max(0) as usize]["images"][kind];
            if let Some(i) = image(
                f,
                &format!("parameters-{kind}"),
                &r[kind],
                if kind == "hpFill" { 3 } else { 2 },
            ) {
                let sp = &mut f.sprites[i];
                sp.index_map = a(rule, "/indexMap")
                    .iter()
                    .map(|v| [n(v, "/0") as u8, n(v, "/1") as u8])
                    .collect();
                if kind == "hpFill" {
                    let width = n(overview, "/hpWidth").max(0) as u32;
                    sp.width = Some(width);
                    sp.height = Some(9);
                    sp.source = Some(Rect::new(0, 0, width, 9));
                }
            }
        }
        for entry in a(r, "/texts") {
            let field = s(entry, "/field");
            let value = if field == "name" {
                snapshot["player"][field].clone()
            } else {
                overview["fields"]
                    .get(field)
                    .or_else(|| snapshot["life"].get(field))
                    .or_else(|| snapshot["stats"].get(field))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            indexed(
                f,
                &format!("parameters-{field}"),
                display(&value),
                n(entry, "/x"),
                n(entry, "/y"),
                n(entry, "/color") as u8,
                entry["width"].as_i64().map(|v| v as i32),
                palette,
            );
        }
        for (id, value, x, y, color, width) in [
            (
                "hp-shadow",
                overview["hpText"].clone(),
                n(r, "/hpText/shadowX"),
                n(r, "/hpText/shadowY"),
                0,
                None,
            ),
            (
                "hp-text",
                overview["hpText"].clone(),
                n(r, "/hpText/x"),
                n(r, "/hpText/y"),
                255,
                None,
            ),
            (
                "mission-count",
                overview["missionCount"].clone(),
                n(r, "/missionRank/valueX"),
                n(r, "/missionRank/valueY"),
                0,
                None,
            ),
            (
                "mission-rank",
                overview["missionRank"].clone(),
                n(r, "/missionRank/textX"),
                n(r, "/missionRank/textY"),
                255,
                Some(10),
            ),
        ] {
            indexed(f, id, display(&value), x, y, color, width, palette);
        }
        if !b(overview, "/hasMission") {
            indexed(
                f,
                "mission-empty",
                s(overview, "/mission"),
                n(r, "/mission/x"),
                n(r, "/mission/emptyY"),
                255,
                Some(30),
                palette,
            );
        } else {
            for (i, line) in wrap_bytes(s(overview, "/mission"), 30).iter().enumerate() {
                indexed(
                    f,
                    &format!("mission-{i}"),
                    line,
                    n(r, "/mission/x"),
                    n(r, "/mission/y") + i as i32 * 18,
                    255,
                    None,
                    palette,
                );
            }
        }
        for (i, (page, label)) in [
            ("house", "住宅一览"),
            ("investment", "投资一览"),
            ("careerHistory", "职业记录"),
            ("skills", "技能一览"),
        ]
        .iter()
        .enumerate()
        {
            let enabled = i > 1
                || if i == 0 {
                    b(overview, "/canInspectHouse")
                } else {
                    b(overview, "/canInspectInvestment")
                };
            if enabled {
                button(
                    f,
                    &format!("parameters-page-{page}"),
                    &r["buttons"][i],
                    Action::Game(json!({"type":"statusAction","action":"page","page":page})),
                    label,
                    6,
                    ui,
                );
            } else {
                image(
                    f,
                    &format!("parameters-disabled-{i}"),
                    &r["disabledButtons"][i],
                    6,
                );
            }
        }
    } else if property {
        if page == "house" {
            let layers = &r["house"]["styles"][n(housing, "/style").max(0) as usize]["layers"];
            let count = (n(housing, "/furniture") + if transition.is_null() { 1 } else { 0 }).max(0)
                as usize;
            for (i, layer) in a(layers, "").iter().take(count).enumerate() {
                image(f, &format!("house-layer-{i}"), layer, i as i32 + 1);
            }
            if !transition.is_null() {
                let unit = n(transition, "/unitSize").max(1);
                for (i, col) in a(transition, "/columns").iter().enumerate() {
                    let revealed = if b(col, "/finished") {
                        480
                    } else {
                        n(col, "/y").clamp(0, 480)
                    };
                    if revealed < 480 {
                        f.fills.push(SceneFill {
                            id: format!("rain-mask-{i}"),
                            bounds: Rect::new(
                                i as i32 * unit,
                                revealed,
                                unit as u32,
                                (480 - revealed) as u32,
                            ),
                            color: [0, 0, 0, 255],
                            z: 20,
                            ..Default::default()
                        });
                    }
                }
            }
        } else {
            if let Some(building) = a(r, "/investment/stages")
                .iter()
                .find(|v| v["investmentStage"] == housing["investmentStage"])
            {
                image(f, "investment-building", building, 1);
            }
        }
        sound(
            f,
            "property-sound",
            s(&r[page], "/sound"),
            false,
            n(st, "/clockMs") as u64 / 100000,
        );
        control(
            f,
            "property-close",
            if page == "house" {
                "关闭住宅预览"
            } else {
                "关闭投资预览"
            },
            Rect::new(0, 0, 640, 480),
            game("statusAction", "close", None),
            &["Enter", "Escape:up"],
        );
        f.controls.last_mut().unwrap().enabled = transition.is_null();
        f.requests.push(SceneRequest::CursorHidden(true));
    } else if !list.is_null() {
        let offset = n(st, "/scroll").max(0) as usize;
        let history = a(snapshot, "/statusDetails/careerHistory");
        let skills = a(snapshot, "/statusDetails/skills");
        let mut thumb = list["scrollThumb"].clone();
        thumb["y"] = json!(
            n(&thumb, "/y")
                + if page == "skills" {
                    offset as i32 * 9
                } else if history.len() > 10 {
                    171 / (history.len() - 10) as i32 * offset as i32
                } else {
                    0
                }
        );
        image(f, "parameters-scroll-thumb", &thumb, 2);
        if page == "skills" {
            for (i, row) in skills.iter().skip(offset).take(4).enumerate() {
                let dy = i as i32 * n(list, "/rowHeight");
                indexed(
                    f,
                    &format!("skill-name-{i}"),
                    s(row, "/name"),
                    n(list, "/nameX"),
                    n(list, "/rowY") + dy,
                    255,
                    list["nameWidth"].as_i64().map(|v| v as i32),
                    palette,
                );
                for (j, col) in a(row, "/columns").iter().enumerate() {
                    let x = n(&list["columns"][j], "");
                    let width = list["columnWidth"].as_i64().map(|v| v as i32);
                    if !s(col, "/second").is_empty() {
                        indexed(
                            f,
                            &format!("skill-first-{i}-{j}"),
                            s(col, "/first"),
                            x,
                            n(list, "/firstLineY") + dy,
                            n(col, "/color") as u8,
                            width,
                            palette,
                        );
                        indexed(
                            f,
                            &format!("skill-second-{i}-{j}"),
                            s(col, "/second"),
                            x,
                            n(list, "/secondLineY") + dy,
                            n(col, "/color") as u8,
                            width,
                            palette,
                        );
                    } else {
                        indexed(
                            f,
                            &format!("skill-empty-{i}-{j}"),
                            s(col, "/first"),
                            x,
                            n(list, "/rowY") + dy,
                            n(col, "/color") as u8,
                            width,
                            palette,
                        );
                    }
                }
            }
        } else {
            for (i, row) in history.iter().skip(offset).take(10).enumerate() {
                for (j, key) in ["year", "name", "description"].iter().enumerate() {
                    indexed(
                        f,
                        &format!("career-{i}-{j}"),
                        display(&row[key]),
                        n(&list["columns"][j], ""),
                        n(list, "/rowY") + i as i32 * n(list, "/rowHeight"),
                        255,
                        list["widths"][j].as_i64().map(|v| v as i32),
                        palette,
                    );
                }
            }
        }
        for (i, meta) in a(list, "/scrollButtons").iter().enumerate() {
            button(
                f,
                &format!("parameters-scroll-{i}"),
                meta,
                Action::Game(
                    json!({"type":"statusAction","action":"scroll","delta":if i==0{-1}else{1}}),
                ),
                if i == 0 {
                    "参数列表向上"
                } else {
                    "参数列表向下"
                },
                6,
                ui,
            );
        }
        for (i, meta) in a(list, "/tabs").iter().enumerate() {
            button(
                f,
                &format!("parameters-tab-{i}"),
                meta,
                Action::Game(json!({"type":"statusAction","action":"page","page":"skills"})),
                "职业技能",
                6,
                ui,
            );
            if let Some(sp) = f.sprites.last_mut() {
                sp.frame = 2;
            }
            if let Some(ctrl) = f.controls.last_mut() {
                ctrl.enabled = false;
            }
        }
        let rect = &list["scrollRect"];
        let left = n(rect, "/0");
        let top = n(rect, "/1");
        let right = n(rect, "/2");
        let bottom = n(rect, "/3");
        for y in top..bottom {
            let index = if page == "skills" {
                (y - top) / 9
            } else if history.len() > 10 {
                (y - top) / (171 / (history.len() - 10) as i32).max(1)
            } else {
                0
            };
            control(
                f,
                &format!("parameters-track-{y}"),
                "参数列表滚动条",
                Rect::new(left, y, (right - left).max(0) as u32, 1),
                Action::Game(json!({"type":"statusAction","action":"scroll","index":index})),
                &[],
            );
        }
    }
    if !property {
        let backs = if list.is_null() {
            &r["back"]
        } else {
            &list["back"]
        };
        f.palette_sections
            .push((s(&backs[0], "/resource").into(), 162, 190));
        back(
            f,
            backs,
            &st["backTracks"],
            None,
            game("statusAction", "close", None),
            if page == "overview" {
                "返回地图"
            } else {
                "返回参数一览"
            },
            ui,
        );
    }
    f.tick = Some(json!({"type":"statusTick"}));
}
fn display(v: &Value) -> String {
    match v {
        Value::String(v) => v.clone(),
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}
fn wrap_bytes(value: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    let mut line = String::new();
    let mut bytes = 0;
    for ch in value.chars() {
        let count = if ch.is_ascii() { 1 } else { 2 };
        if ch == '\n' || bytes + count > width {
            out.push(std::mem::take(&mut line));
            bytes = 0;
            if ch == '\n' {
                continue;
            }
        }
        line.push(ch);
        bytes += count;
    }
    if !line.is_empty() {
        out.push(line)
    }
    out
}

fn relationship(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let house = &snapshot["pending"]["state"];
    let phase = s(house, "/phase");
    sprite(
        f,
        "relationship-background",
        "install/pm0l/pm0lb01.bmp",
        0,
        0,
        0,
    );
    f.sprites.last_mut().unwrap().transparent = Transparency::Opaque;
    let career = n(snapshot, "/life/profession");
    let stage = n(house, "/state/stage");
    let index = if career < 0 {
        if stage < 2 {
            25
        } else if stage <= 3 {
            26
        } else {
            27
        }
    } else {
        career
    };
    let portrait = c.rules["relationships"]["playerPortraits"]
        [n(house, "/state/sex").max(0) as usize][index.max(0) as usize]
        .as_str()
        .unwrap_or("");
    sprite(f, "relationship-player", c.resolve(portrait), 27, 265, 1);
    let candidates: Vec<i32> = if phase == "dates" || phase == "dateConfirm" {
        a(house, "/state/contacts")
            .iter()
            .filter_map(Value::as_i64)
            .filter(|v| *v >= 0)
            .take(4)
            .map(|v| v as i32)
            .collect()
    } else if phase == "marriages" || phase == "marriageConfirm" {
        a(house, "/marriageCandidates")
            .iter()
            .filter_map(Value::as_i64)
            .map(|v| v as i32)
            .collect()
    } else {
        a(house, "/offers")
            .iter()
            .filter_map(Value::as_i64)
            .map(|v| v as i32)
            .collect()
    };
    if phase != "main" {
        sprite(
            f,
            "relationship-portrait-board",
            c.resolve("pm0l/pm0102.bmp"),
            194,
            66,
            2,
        );
        for (i, id) in candidates.iter().enumerate() {
            let person = &house["people"][*id as usize];
            let key = if house["selectedPerson"].as_i64() == Some(*id as i64) {
                "selectedPortrait"
            } else {
                "portrait"
            };
            sprite(
                f,
                &format!("relationship-person-{i}"),
                c.resolve(s(person, &format!("/{key}"))),
                210 + i as i32 * 101,
                78,
                3,
            );
        }
    }
    sprite(
        f,
        "relationship-front",
        c.resolve("pm0l/bl201a.bmp"),
        -91,
        134,
        4,
    );
    let exit = sprite(
        f,
        "relationship-exit",
        "install/public/button/exita.flc",
        519,
        292,
        50,
    );
    f.sprites[exit].playback = Some(Playback {
        interval_ms: n(&c.media("install/public/button/exita.flc"), "/frameDelayMs").max(1) as u32,
        once: false,
        max_advances: 1,
        immediate: false,
    });
    control(
        f,
        "relationship-exit",
        "离开关系屋",
        Rect::new(519, 292, 62, 62),
        game("relationshipAction", "close", None),
        &["Escape"],
    );
    for (key, x) in [("money", 77), ("charm", 229), ("romance", 401)] {
        text(
            f,
            &format!("relationship-number-{key}"),
            display(&house["state"][key]),
            Rect::new(x, 17, 130, 18),
            [255; 4],
        );
    }
    let count = a(house, "/state/contacts")
        .iter()
        .take(4)
        .filter(|v| v.as_i64().unwrap_or(-1) >= 0)
        .count();
    let mut hover = None;
    if phase == "main" && stage >= 3 {
        for (index, action_name, label, available) in [
            (4, "meet", "结识新朋友", count < 4),
            (5, "dates", "选择约会对象", count > 0),
            (
                6,
                "marriages",
                "选择结婚对象",
                !a(house, "/marriageCandidates").is_empty(),
            ),
        ] {
            if !available {
                continue;
            }
            let x = 366 + (index - 4) * 85;
            let id = format!("relationship-menu-{index}");
            for suffix in ["a", "b"] {
                let resource = format!("install/pm0l/pm0lb0{index}{suffix}.flc");
                let k = sprite(f, &format!("{id}-{suffix}"), resource, x, 212, 50);
                f.sprites[k].playback = Some(Playback {
                    interval_ms: n(&c.media(&f.sprites[k].resource_id), "/frameDelayMs").max(1)
                        as u32,
                    once: false,
                    max_advances: 1,
                    immediate: false,
                });
                let selected = if ui.hover.as_deref() == Some(&id) {
                    "b"
                } else {
                    "a"
                };
                if suffix != selected {
                    f.sprites[k].clip = Some(Rect::new(0, 0, 0, 0));
                }
            }
            control(
                f,
                &id,
                label,
                Rect::new(x, 212, 70, 70),
                game("relationshipAction", action_name, None),
                &[],
            );
        }
    } else if phase == "main" || phase == "notice" {
        control(
            f,
            "relationship-continue",
            "继续关系屋对话",
            Rect::new(0, 0, 640, 480),
            game(
                "relationshipAction",
                if phase == "notice" {
                    "advanceNotice"
                } else {
                    "close"
                },
                None,
            ),
            &["Enter", "Space"],
        );
    } else if phase == "question" {
        let question = &snapshot["pending"]["question"];
        let selected = n(&ui.profile, "/_relationshipAnswer").clamp(0, 2) as usize;
        let lines = wrap_bytes(s(question, "/prompt"), 51);
        let mut y = 371;
        for (i, line) in lines.iter().enumerate() {
            text(
                f,
                &format!("relationship-question-{i}"),
                line,
                Rect::new(112, y, 410, 16),
                [255; 4],
            );
            f.texts.last_mut().unwrap().font = TextFont::Gdi {
                size: 16,
                bold: true,
            };
            y += 16;
        }
        for (i, answer) in a(question, "/answers").iter().enumerate() {
            let prefix = c.rules["relationships"]["answerPrefixes"][i]
                .as_str()
                .unwrap_or("");
            let value = format!("{prefix}{}", answer.as_str().unwrap_or(""));
            let rows = wrap_bytes(&value, 51);
            let height = rows.len().max(1) as u32 * 16;
            text(
                f,
                &format!("relationship-answer-{i}"),
                value,
                Rect::new(112, y, 410, height),
                [255; 4],
            );
            f.texts.last_mut().unwrap().font = TextFont::Gdi {
                size: 16,
                bold: true,
            };
            control(
                f,
                &format!("relationship-answer-{i}"),
                answer.as_str().unwrap_or(""),
                Rect::new(112, y, 410, height),
                if selected == i {
                    game("relationshipAction", "answer", Some(i))
                } else {
                    Action::Ui(UiAction::SetField {
                        field: "_relationshipAnswer".into(),
                        value: json!(i),
                    })
                },
                &[],
            );
            // 4389d0 selects on the first click and submits an already selected
            // row on the next click; DIK_A/B/C submit their own rows directly.
            control(
                f,
                &format!("relationship-answer-key-{i}"),
                answer.as_str().unwrap_or(""),
                Rect::new(-1, -1, 0, 0),
                game("relationshipAction", "answer", Some(i)),
                &[["KeyA", "KeyB", "KeyC"][i]],
            );
            if selected == i {
                let k = sprite(
                    f,
                    "relationship-answer-cursor",
                    "install/pm0l/select1.flc",
                    114,
                    y,
                    51,
                );
                f.sprites[k].playback = Some(Playback {
                    interval_ms: n(&c.media("install/pm0l/select1.flc"), "/frameDelayMs").max(1)
                        as u32,
                    once: false,
                    max_advances: 1,
                    immediate: false,
                });
            }
            y += height as i32;
        }
        for (key, delta) in [("ArrowUp", -1), ("ArrowDown", 1)] {
            control(
                f,
                &format!("relationship-select-{key}"),
                "选择答案",
                Rect::new(0, 0, 0, 0),
                Action::Ui(UiAction::SetField {
                    field: "_relationshipAnswer".into(),
                    value: json!((selected as i32 + delta).clamp(0, 2)),
                }),
                &[key],
            );
        }
        control(
            f,
            "relationship-answer-submit",
            "提交答案",
            Rect::new(0, 0, 0, 0),
            game("relationshipAction", "answer", Some(selected)),
            &["Enter"],
        );
    } else {
        for (i, _id) in candidates.iter().enumerate() {
            let action_name = if phase == "meet" {
                "selectOffer"
            } else if phase == "dates" || phase == "dateConfirm" {
                "selectDate"
            } else {
                "selectMarriage"
            };
            let id = format!("relationship-offer-{i}");
            control(
                f,
                &id,
                &format!("选择第 {} 位人物", i + 1),
                Rect::new(210 + i as i32 * 101, 78, 90, 100),
                game("relationshipAction", action_name, Some(i)),
                &[],
            );
            if phase == "meet" && ui.hover.as_deref() == Some(&id) {
                hover = Some(i);
            }
        }
        if phase == "dateConfirm" || phase == "marriageConfirm" {
            for (name, x, action_name, label) in [
                ("pm0lb02", 300, "confirm", "确定人选"),
                ("pm0lb08", 350, "reject", "重新选择"),
            ] {
                let id = format!("relationship-{action_name}");
                let i = sprite(f, &id, format!("install/pm0l/{name}.bmp"), x, 419, 50);
                f.sprites[i].width = Some(46);
                f.sprites[i].height = Some(13);
                f.sprites[i].frame_count = Some(3);
                f.sprites[i].frame = if ui.pressed.as_deref() == Some(&id) {
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
                    Rect::new(x, 419, 46, 13),
                    game("relationshipAction", action_name, None),
                    &[],
                );
            }
        }
    }
    if phase != "question" {
        text(
            f,
            "relationship-message",
            s(house, "/message"),
            Rect::new(112, 371, 410, 100),
            [255; 4],
        );
        f.texts.last_mut().unwrap().font = TextFont::Gdi {
            size: 16,
            bold: true,
        };
    }
    let mut tick = json!({"type":"relationshipTick"});
    if let Some(index) = hover {
        tick["hoverIndex"] = json!(index)
    }
    f.tick = Some(tick);
    sound(
        f,
        "relationship-music",
        c.resolve("sound/music/n2love.wav"),
        true,
        0,
    );
}

#[cfg(test)]
mod relationship_input_tests {
    use super::*;
    use virtual_life_core::{
        relationships::{RelationshipHouse, RelationshipRules, RelationshipState},
        rng::ReplayRng,
    };

    fn frame(
        c: &SceneContent<'_>,
        house: &RelationshipHouse,
        ui: &PresentationState,
    ) -> SceneFrame {
        let snapshot = json!({"life":{"profession":-1},"pending":{"kind":"relationship","state":house,"question":house.question().unwrap()}});
        let mut frame = SceneFrame::default();
        relationship(&mut frame, c, &snapshot, ui);
        frame
    }

    fn answer(control: &SceneControl, house: &mut RelationshipHouse, rng: &mut ReplayRng) {
        let Action::Game(value) = &control.action else {
            panic!("Expected a real relationship command");
        };
        assert_eq!(value["type"], "relationshipAction");
        assert_eq!(value["action"], "answer");
        house
            .answer(value["index"].as_u64().unwrap() as usize, rng)
            .unwrap();
    }

    #[test]
    fn relationship_click_selects_before_submitting_and_abc_answers_real_questions() {
        let rules: Value =
            serde_json::from_str(include_str!("../../../../assets/catalog/rules.json")).unwrap();
        let relationship_rules: RelationshipRules =
            serde_json::from_value(rules["relationships"].clone()).unwrap();
        let c = SceneContent {
            manifest: &Value::Null,
            maps: &Value::Null,
            rules: &rules,
        };
        let mut rng = ReplayRng::new(42);
        let mut house = RelationshipHouse::new(
            &relationship_rules,
            RelationshipState {
                sex: 0,
                stage: 3,
                money: 100_000,
                charm: 9999,
                romance: 100,
                contacts: [-1; 8],
                new_targets: [-1; 8],
                dating_target: -1,
                spouse: -1,
                gift_level: 1,
            },
            false,
            &mut rng,
        )
        .unwrap();
        house.meet().unwrap();
        house.hover(0, &mut rng).unwrap();
        house.select_offer(0).unwrap();
        let before = house.clone();
        let before_rng = rng.clone();
        let mut ui = PresentationState {
            screen: Screen::Game,
            ..Default::default()
        };
        let first = frame(&c, &house, &ui);
        let choice = first
            .controls
            .iter()
            .find(|control| control.id == "relationship-answer-1")
            .unwrap();
        assert!(
            choice.on_hover.is_none(),
            "Moving over an answer must not select or submit it"
        );
        let Action::Ui(UiAction::SetField { field, value }) = &choice.action else {
            panic!("First click must only select");
        };
        ui.profile[field] = value.clone();
        assert_eq!(house, before);
        assert_eq!(rng, before_rng);
        let restored_ui: PresentationState =
            serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
        let selected = frame(&c, &house, &restored_ui);
        let choice = selected
            .controls
            .iter()
            .find(|control| control.id == "relationship-answer-1")
            .unwrap();
        answer(choice, &mut house, &mut rng);
        assert_eq!(house.question_index, 1);
        let second = frame(&c, &house, &ui);
        for (index, key) in ["KeyA", "KeyB", "KeyC"].iter().enumerate() {
            let control = second
                .controls
                .iter()
                .find(|control| control.keys.iter().any(|candidate| candidate == key))
                .unwrap();
            let Action::Game(value) = &control.action else {
                panic!("ABC must submit directly");
            };
            assert_eq!(value["index"], index);
        }
        let key_c = second
            .controls
            .iter()
            .find(|control| control.keys.contains(&"KeyC".into()))
            .unwrap();
        answer(key_c, &mut house, &mut rng);
        assert_eq!(
            house.question_index, 0,
            "The second real answer settles and resets the questionnaire"
        );
        assert_eq!(
            house.phase,
            virtual_life_core::relationships::RelationshipPhase::Notice
        );
    }
}
