//! Original ordinary and final encounters: layer order, palette ranges and input rectangles.
use super::*;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];

fn label(
    f: &mut SceneFrame,
    id: &str,
    value: impl Into<String>,
    x: i32,
    y: i32,
    size: u32,
    color: [u8; 4],
    bold: bool,
) {
    text(f, id, value, Rect::new(x, y, 640, size + 4), color);
    if let Some(t) = f.texts.last_mut() {
        t.font = TextFont::Gdi { size, bold };
        t.line_height = size;
    }
}
fn at(f: &mut SceneFrame, id: &str, meta: &Value, x: i32, y: i32, z: i32) -> Option<usize> {
    let index = image(f, id, meta, z)?;
    f.sprites[index].x = x;
    f.sprites[index].y = y;
    Some(index)
}
fn crop(f: &mut SceneFrame, id: &str, meta: &Value, x: i32, y: i32, z: i32, source: Rect) {
    if let Some(index) = at(f, id, meta, x, y, z) {
        let sp = &mut f.sprites[index];
        sp.source = Some(source);
        sp.width = Some(source.width);
        sp.height = Some(source.height);
    }
}
fn strip(f: &mut SceneFrame, id: &str, meta: &Value, x: i32, y: i32, width: i32, z: i32) {
    if width <= 0 {
        return;
    }
    if let Some(index) = at(f, id, meta, x, y, z) {
        f.sprites[index].width = Some(width as u32);
        f.sprites[index].repeat = true;
    }
}
fn fill(f: &mut SceneFrame, id: &str, x: i32, y: i32, width: i32, height: i32, z: i32) {
    if width > 0 && height > 0 {
        f.fills.push(SceneFill {
            id: id.into(),
            bounds: Rect::new(x, y, width as u32, height as u32),
            color: [0, 0, 0, 255],
            z,
            ..Default::default()
        });
    }
}
fn find_meta<'a>(value: &'a Value, resource: &str) -> Option<&'a Value> {
    if s(value, "/resource") == resource && value.get("frameCount").is_some() {
        return Some(value);
    }
    match value {
        Value::Array(items) => items.iter().find_map(|v| find_meta(v, resource)),
        Value::Object(items) => items.values().find_map(|v| find_meta(v, resource)),
        _ => None,
    }
}
fn ordinary_animation(f: &mut SceneFrame, id: &str, layer: &Value, r: &Value, z: i32) {
    let mut meta = find_meta(r, s(layer, "/resource"))
        .cloned()
        .unwrap_or_else(|| layer.clone());
    if let (Some(meta), Some(layer)) = (meta.as_object_mut(), layer.as_object()) {
        meta.extend(layer.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    animation(f, id, &meta, z);
}
fn enabled_control(
    f: &mut SceneFrame,
    id: &str,
    label: &str,
    bounds: Rect,
    action: Action,
    enabled: bool,
    keys: &[&str],
) {
    control(f, id, label, bounds, action, keys);
    f.controls.last_mut().unwrap().enabled = enabled;
}
fn selected(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}
fn attack_ready(st: &Value) -> bool {
    s(st, "/phase") == "playing"
        && !b(st, "/help")
        && !b(st, "/resultStarted")
        && !b(st, "/won")
        && !b(st, "/lost")
        && !b(st, "/playerAttacking")
        && !b(st, "/enemyAttacking")
        && !b(st, "/dealing")
        && a(st, "/selected").iter().any(selected)
        && n(st, "/playerCooldown") < n(st, "/playerTenths") / 10
        && n(st, "/playerCooldown") <= n(st, "/playerSeconds")
}

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let state = &snapshot["pending"]["state"];
    f.tick = Some(json!({ "type": "battleTick" }));
    if s(snapshot, "/pending/game") == "final" {
        final_battle(f, c, snapshot, state, ui);
    } else {
        ordinary(f, c, snapshot, state, ui);
    }
}

fn ordinary(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    st: &Value,
    ui: &PresentationState,
) {
    let rules = &c.rules["battle"];
    let r = &rules["rendering"];
    let phase = s(st, "/phase");
    let enemy = n(st, "/enemy").max(0) as usize;
    let sex = n(st, "/input/sex").max(0) as usize;
    let class = n(st, "/playerClass").max(0) as usize;
    let background = n(st, "/background").max(0) as usize;
    let opening = b(st, "/showOpening");
    let help = b(st, "/help");
    let clock = n(st, "/clockMs").max(0) as u64;
    sound(f, "battle-music", s(r, "/sounds/music"), true, 0);
    if opening {
        f.palette = Some(s(&r["versusDoors"][0], "/resource").into());
        for side in 0..2 {
            at(
                f,
                &format!("versus-door-{side}"),
                &r["versusDoors"][side],
                n(
                    st,
                    if side == 0 {
                        "/openingLeftX"
                    } else {
                        "/openingRightX"
                    },
                ),
                0,
                1,
            );
        }
        if n(st, "/openingLeftX") == 0 && n(st, "/openingRightX") == 228 {
            let stage = n(st, "/input/stage");
            let career = n(st, "/input/career");
            let portrait = if stage < 2 {
                25
            } else if stage < 4 {
                26
            } else if stage == 4 {
                27
            } else if career < 0 {
                28
            } else {
                career as usize
            };
            at(
                f,
                "versus-enemy",
                &r["versusEnemyPortraits"][enemy],
                23,
                24,
                2,
            );
            at(
                f,
                "versus-player",
                &r["versusPlayerPortraits"][sex][portrait],
                523,
                347,
                2,
            );
            let rank = n(&st["persistent"]["enemies"][enemy], "/rank");
            for (i, value, x, y) in [
                (0, s(&rules["enemyNames"][enemy], "").to_owned(), 34, 161),
                (1, (rank + 1).to_string(), 34, 213),
                (
                    2,
                    s(&rules["enemyDescriptions"][enemy], "").to_owned(),
                    34,
                    267,
                ),
                (
                    3,
                    if rank == 9 {
                        "0".into()
                    } else {
                        n(st, "/enemyHp").to_string()
                    },
                    34,
                    322,
                ),
                (4, s(snapshot, "/player/name").to_owned(), 526, 137),
                (5, n(st, "/input/lifeLevel").to_string(), 556, 189),
                (6, s(&r["classNames"][sex][class], "").to_owned(), 526, 243),
                (7, n(st, "/hp").to_string(), 504, 298),
            ] {
                label(
                    f,
                    &format!("versus-label-{i}"),
                    value,
                    x,
                    y,
                    16,
                    WHITE,
                    false,
                );
            }
        }
        sound(f, "battle-door", s(r, "/sounds/closeDoor"), false, 0);
        return;
    }
    image(f, "battle-background", &r["backgrounds"][background], 0);
    f.palette = Some(s(&r["backgrounds"][background], "/resource").into());
    for (meta, first, last) in [
        (&r["panels"][2], 142, 161),
        (&r["cards"][0], 202, 254),
        (&r["enemyActors"][enemy][0], 102, 141),
        (&r["actors"][sex][class][0], 162, 201),
    ] {
        f.palette_sections
            .push((s(meta, "/resource").into(), first, last));
    }
    if phase != "intro" && !s(st, "/paletteResource").is_empty() {
        f.palette_sections
            .push((s(st, "/paletteResource").into(), 62, 101));
    }
    if phase == "reward" {
        reward(f, st, &r["bonus"]);
        if n(&st["bonus"], "/incrementPhase") == 1 {
            sound(
                f,
                "battle-bonus-count",
                "install/public/bonus/bonus.wav",
                false,
                n(&st["bonus"], "/incrementLastTick").max(0) as u64,
            );
        }
        control(
            f,
            "battle-reward",
            "完成战斗奖励",
            Rect::new(0, 0, 640, 480),
            game("battleAction", "finishRewardAnimation", None),
            &["Enter", "Space", "Escape", "MouseRight"],
        );
        return;
    }
    if help {
        if background < 5 {
            image(
                f,
                "battle-foreground",
                &r["backgroundOverlays"][background],
                10,
            );
        }
        let track = a(st, "/layers")
            .first()
            .unwrap_or(&st["tracks"][format!("help{}", n(st, "/helpPage"))]);
        ordinary_animation(f, "battle-help", track, r, 20);
        control(
            f,
            "battle-help-next",
            "下一页战斗教学",
            Rect::new(0, 0, 640, 480),
            game("battleAction", "start", None),
            &["Enter"],
        );
        return;
    }
    let playing = phase != "intro";
    if playing {
        for (index, panel) in a(r, "/panels").iter().enumerate() {
            image(f, &format!("battle-panel-{index}"), panel, 1);
        }
        for digit in 0..2 {
            let number = if digit == 0 {
                n(st, "/remaining") / 10
            } else {
                n(st, "/remaining") % 10
            };
            crop(
                f,
                &format!("battle-time-{digit}"),
                &r["timeDigits"],
                304 + digit * 16,
                37,
                2,
                Rect::new(0, number * 23, 16, 23),
            );
        }
        at(f, "battle-help-marker", &r["healthBar"], 620, 0, 2);
        ordinary_health(f, st, r);
    }
    let actor_resources: Vec<&str> = a(&r["actors"][sex][class], "")
        .iter()
        .chain(a(&r["enemyActors"][enemy], ""))
        .map(|v| s(v, "/resource"))
        .collect();
    let mut effects_started = false;
    for (i, layer) in a(st, "/layers").iter().enumerate() {
        let resource = s(layer, "/resource");
        let actor = actor_resources.contains(&resource);
        let ready = a(r, "/healthFlares")
            .iter()
            .any(|v| s(v, "/resource") == resource);
        if !ready && !actor {
            effects_started = true;
        }
        ordinary_animation(
            f,
            &format!("battle-layer-{i}"),
            layer,
            r,
            if ready {
                5
            } else if actor && !effects_started {
                10 + i as i32
            } else {
                40 + i as i32
            },
        );
    }
    if a(st, "/layers").is_empty() {
        for (i, name) in [
            format!("player{}", n(st, "/playerActor")),
            format!("enemy{}", n(st, "/enemyActor")),
        ]
        .iter()
        .enumerate()
        {
            ordinary_animation(
                f,
                &format!("battle-initial-{i}"),
                &st["tracks"][name],
                r,
                10 + i as i32,
            );
        }
    }
    if background < 5 {
        image(
            f,
            "battle-foreground",
            &r["backgroundOverlays"][background],
            25,
        );
    }
    for i in 5..8 {
        if playing || i == 5 {
            image(f, &format!("battle-front-panel-{i}"), &r["panels"][i], 26);
        }
    }
    if playing && !b(st, "/resultStarted") {
        for i in 0..n(st, "/cardCount").max(0) as usize {
            let card = &st["cards"][i];
            let is_selected = selected(&st["selected"][i]) && !b(st, "/dealing");
            let position = if is_selected {
                &r["selectedCardPositions"][i]
            } else {
                &rules["cardPositions"][i]
            };
            let x = if is_selected {
                n(position, "/0")
            } else {
                n(&st["cardX"][i], "")
            };
            at(
                f,
                &format!("battle-card-{i}"),
                &r[if is_selected {
                    "selectedCards"
                } else {
                    "cards"
                }][n(card, "/kind").max(0) as usize],
                x,
                n(position, "/1"),
                30,
            );
        }
        if b(st, "/playerAttacking") && n(st, "/attackPhase") == 0 {
            let mut slot = 0;
            for i in 0..n(st, "/cardCount").max(0) as usize {
                if selected(&st["usedSelected"][i]) {
                    let p = &r["comboCardPositions"]
                        [n(st, "/comboCount").saturating_sub(1) as usize][slot];
                    slot += 1;
                    at(
                        f,
                        &format!("battle-used-card-{i}"),
                        &r["cards"][n(&st["usedCards"][i], "/kind").max(0) as usize],
                        n(p, "/0"),
                        n(p, "/1"),
                        40,
                    );
                }
            }
        }
        if b(st, "/strongAttack") && b(st, "/playerAttacking") && n(st, "/attackPhase") == 1 {
            let career = n(st, "/input/career");
            let portraits = if career < 0 {
                &r["studentPortraits"][sex][n(st, "/input/stage").min(4).max(0) as usize]
            } else {
                &r["careerPortraits"][sex][career as usize]
            };
            for (i, timer) in a(st, "/playerPortraitTimers").iter().enumerate() {
                if timer.as_i64() == Some(0) {
                    at(
                        f,
                        &format!("battle-player-portrait-{i}"),
                        &portraits[i],
                        300 + i as i32 * 80,
                        100,
                        70,
                    );
                }
            }
        }
        if b(st, "/enemyAttacking") && n(st, "/attackPhase") == 4 && n(st, "/enemyEffect") == 1 {
            for (i, timer) in a(st, "/enemyPortraitTimers").iter().enumerate() {
                if timer.as_i64() == Some(0) {
                    at(
                        f,
                        &format!("battle-enemy-portrait-{i}"),
                        &r["enemyPortraits"][enemy][i],
                        200 + i as i32 * 80,
                        100,
                        70,
                    );
                }
            }
        }
        if n(st, "/attackPhase") == 9 {
            for i in 0..8 {
                let word = if i < 4 {
                    i
                } else {
                    4 + n(st, "/godEffect") as usize * 4 + i - 4
                };
                at(
                    f,
                    &format!("battle-god-{i}"),
                    &r["words"][word],
                    (if i < 4 { 100 } else { 140 }) + (i % 4) as i32 * 80,
                    if i < 4 { 50 } else { 120 },
                    70,
                );
            }
        }
    }
    if b(st, "/lost") && b(st, "/resultStarted") && n(st, "/remaining") != 0 {
        at(f, "battle-loss", &r["cross"], 163, n(st, "/lossY"), 90);
    }
    if phase == "intro" {
        control(
            f,
            "battle-start",
            "开始战斗",
            Rect::new(0, 0, 640, 480),
            game("battleAction", "start", None),
            &["Enter"],
        );
    } else {
        control(
            f,
            "battle-help",
            "战斗帮助",
            Rect::new(620, 0, 20, 24),
            game("battleAction", "help", None),
            &["F1"],
        );
        if phase == "playing" {
            for i in 0..n(st, "/cardCount").max(0) as usize {
                let p = &rules["cardPositions"][i];
                enabled_control(
                    f,
                    &format!("battle-card-{i}"),
                    &format!("选择战斗卡片 {}", i + 1),
                    Rect::new(n(p, "/0"), n(p, "/1"), 48, 60),
                    game("battleAction", "selectCard", Some(i)),
                    !b(st, "/playerAttacking") && !b(st, "/dealing") && !b(st, "/resultStarted"),
                    &[],
                );
                let ctl = f.controls.last_mut().unwrap();
                ctl.on_hover = Some(game("battleAction", "hoverCard", Some(i)));
                ctl.on_leave = Some(game("battleAction", "hoverCard", None));
            }
            if attack_ready(st) {
                button(
                    f,
                    "battle-attack",
                    &r["attackButton"],
                    game("battleAction", "attack", None),
                    "出招",
                    80,
                    ui,
                );
            }
        }
        label(
            f,
            "battle-enemy-name",
            s(&rules["enemyNames"][enemy], ""),
            10,
            1,
            16,
            RED,
            true,
        );
        label(
            f,
            "battle-attack-power",
            n(st, "/attackPower").to_string(),
            478,
            426,
            16,
            RED,
            true,
        );
        label(
            f,
            "battle-defense-power",
            n(st, "/defensePower").to_string(),
            478,
            441,
            16,
            RED,
            true,
        );
    }
    if !s(st, "/text").is_empty() {
        label(f, "battle-talk", s(st, "/text"), 120, 360, 16, WHITE, true);
    }
    for (show, damage, x) in [
        ("/shownPlayerDamage", "/playerDamage", 420),
        ("/shownEnemyDamage", "/enemyDamage", 100),
    ] {
        if b(st, show) {
            label(
                f,
                &format!("battle-damage-{x}"),
                format!(
                    "{}{}{}",
                    s(r, "/damagePrefix"),
                    n(st, damage),
                    s(r, "/damageSuffix")
                ),
                x,
                2,
                16,
                RED,
                true,
            );
        }
    }
    let hover = n(st, "/hoveredCard");
    if !st["hoveredCard"].is_null()
        && hover >= 0
        && hover < n(st, "/cardCount")
        && !b(st, "/dealing")
        && !b(st, "/resultStarted")
    {
        let card = &st["cards"][hover as usize];
        let p = &rules["cardPositions"][hover as usize];
        if n(card, "/kind") < 4 {
            for (id, title, value, y) in [
                ("attack", s(r, "/cardAttackLabel"), n(card, "/attack"), 70),
                (
                    "defense",
                    s(r, "/cardDefenseLabel"),
                    n(card, "/defense"),
                    82,
                ),
            ] {
                label(
                    f,
                    &format!("battle-card-{id}-label"),
                    title,
                    n(p, "/0") + 10,
                    n(p, "/1") + y,
                    12,
                    RED,
                    false,
                );
                label(
                    f,
                    &format!("battle-card-{id}-value"),
                    value.to_string(),
                    n(p, "/0") + 60,
                    n(p, "/1") + y,
                    12,
                    RED,
                    false,
                );
            }
        } else {
            for (i, description) in a(
                &r["cardDescriptions"][n(card, "/kind").saturating_sub(4) as usize],
                "",
            )
            .iter()
            .enumerate()
            {
                label(
                    f,
                    &format!("battle-card-description-{i}"),
                    s(description, ""),
                    n(p, "/0") + 10,
                    n(p, "/1") + 70 + i as i32 * 12,
                    12,
                    RED,
                    false,
                );
            }
        }
    }
    ordinary_sounds(f, st, &r["sounds"], clock);
}

fn ordinary_health(f: &mut SceneFrame, st: &Value, r: &Value) {
    let images = &r["hpDigits"];
    for side in 0..2 {
        let width = n(st, if side == 1 { "/maxHp" } else { "/enemyMaxHp" }).min(270);
        let hp = n(
            st,
            if side == 1 {
                "/displayedHp"
            } else {
                "/displayedEnemyHp"
            },
        );
        let band = hp / 270;
        let remainder = hp % 270;
        let x = if side == 1 { 621 - width } else { 16 };
        at(
            f,
            &format!("hp-start-{side}"),
            &images[if side == 1 { 8 } else { 0 }],
            if side == 1 { 617 - width } else { 10 },
            20,
            3,
        );
        at(
            f,
            &format!("hp-end-{side}"),
            &images[if side == 1 { 9 } else { 2 }],
            if side == 1 { 621 } else { width + 16 },
            20,
            3,
        );
        strip(f, &format!("hp-border-{side}"), &images[1], x, 20, width, 3);
        if band > 0 {
            strip(
                f,
                &format!("hp-under-{side}"),
                &images[(band + 2).min(7) as usize],
                x,
                23,
                width,
                4,
            );
        } else {
            fill(f, &format!("hp-empty-{side}"), x, 23, width, 10, 4);
        }
        strip(
            f,
            &format!("hp-full-{side}"),
            &images[(band + 3).clamp(0, 7) as usize],
            if side == 1 { 622 - remainder } else { 16 },
            23,
            remainder,
            4,
        );
    }
    for (name, numerator, denominator, factor, start, end) in [
        (
            "player",
            n(st, "/playerTenths"),
            n(st, "/playerCooldown") * 10,
            148,
            474,
            623,
        ),
        (
            "enemy",
            n(st, "/enemyTenths"),
            n(st, "/enemyCooldown") * 10,
            146,
            15,
            162,
        ),
    ] {
        if numerator < denominator && denominator > 0 {
            let x = numerator * factor / denominator + start;
            fill(f, &format!("{name}-cooldown"), x, 45, end - x, 11, 4);
        }
    }
}

fn reward(f: &mut SceneFrame, st: &Value, r: &Value) {
    let bonus = &st["bonus"];
    if !b(bonus, "/visible") {
        return;
    }
    let loss = n(st, "/bonusKind") == 2;
    let animation_meta = &r[if loss { "loss" } else { "gain" }];
    image(
        f,
        "battle-bonus-title",
        &r[if n(st, "/bonusKind") == 1 {
            "experience"
        } else {
            "money"
        }],
        100,
    );
    let mut animation_meta = animation_meta.clone();
    animation_meta["frame"] = json!(0);
    if let Some(index) = animation(
        f,
        &format!("battle-bonus-animation-{}", n(st, "/bonusKind")),
        &animation_meta,
        101,
    ) {
        f.sprites[index].playback = Some(Playback {
            interval_ms: 40,
            once: true,
            max_advances: 1,
            immediate: false,
        });
    }
    for (name, field, units, y) in [
        ("value", "/value", "/valueUnits", 103),
        ("increment", "/increment", "/incrementUnits", 166),
    ] {
        let mut digits: Vec<u32> = n(bonus, field)
            .unsigned_abs()
            .to_string()
            .chars()
            .filter_map(|ch| ch.to_digit(10))
            .collect();
        if !bonus.pointer(units).unwrap_or(&Value::Null).is_null() {
            if let Some(last) = digits.last_mut() {
                *last = n(bonus, units) as u32;
            }
        }
        for (i, digit) in digits.iter().enumerate() {
            crop(
                f,
                &format!("bonus-{name}-{i}"),
                &r["digits"],
                330 + (n(bonus, "/digitCount") - digits.len() as i32 + i as i32) * 28,
                y,
                102,
                Rect::new(*digit as i32 * 26, 0, 26, 38),
            );
        }
    }
}

fn ordinary_cue<'a>(st: &Value, r: &'a Value, name: &str) -> &'a str {
    let sex = n(st, "/input/sex").max(0) as usize;
    let stage = n(st, "/input/stage");
    let enemy = n(st, "/enemy").max(0) as usize;
    let career = if stage < 4 {
        [29, 26, 27, 28][stage.max(0) as usize]
    } else if n(st, "/input/career") < 0 {
        25
    } else {
        n(st, "/input/career")
    } as usize;
    match name {
        "playerWeak" => s(&r["playerVoices"][sex][if stage < 3 { 0 } else { 1 }], ""),
        "playerStrong" => s(&r["playerCareerVoice"][sex][career], ""),
        "enemyDefence" => s(&r["enemyDefence"][enemy], ""),
        "shieldBounce0" => s(&r["cardSelect"][0], ""),
        "shieldBounce1" => s(&r["cardSelect"][1], ""),
        "playerHit" => s(&r["playerVoices"][sex][if stage < 3 { 2 } else { 3 }], ""),
        "enemyStrong" => s(&r["enemyAttack"][enemy][1], ""),
        "enemyDamage" => s(&r["enemyDamage"][enemy], ""),
        "playerLoss" => s(&r["playerVoices"][sex][if stage < 3 { 4 } else { 5 }], ""),
        name if name.starts_with("combo") => s(
            &r["cardCombo"][name[5..].parse::<usize>().unwrap_or(1).saturating_sub(1)],
            "",
        ),
        name => s(&r[name], ""),
    }
}
fn ordinary_sounds(f: &mut SceneFrame, st: &Value, r: &Value, clock: u64) {
    for (i, cue) in a(st, "/sounds").iter().enumerate() {
        let name = s(cue, "");
        sound(
            f,
            &format!("battle-cue-{i}-{name}"),
            ordinary_cue(st, r, name),
            false,
            clock,
        );
    }
    for cue in a(st, "/soundStops") {
        let resource = ordinary_cue(st, r, s(cue, ""));
        if !resource.is_empty() {
            f.stopped_sounds.push(resource.to_owned());
        }
    }
}

fn final_resource(rules: &Value, address: u32) -> &Value {
    &rules["resourcesByAddress"][format!("0x{address:08x}")]
}
fn final_image(rules: &Value, address: u32, x: i32, y: i32) -> Value {
    let mut meta = final_resource(rules, address).clone();
    if meta.is_object() {
        meta["x"] = json!(x);
        meta["y"] = json!(y);
        meta["transparent"] = json!(1);
    }
    meta
}
fn final_animation(
    f: &mut SceneFrame,
    id: &str,
    resource: &Value,
    frame: i32,
    x: i32,
    y: i32,
    z: i32,
) {
    let mut meta = resource.clone();
    if !meta.is_object() {
        return;
    }
    meta["x"] = json!(x);
    meta["y"] = json!(y);
    meta["frame"] = json!(frame);
    meta["transparent"] = json!(1);
    animation(f, id, &meta, z);
}
fn final_layer(f: &mut SceneFrame, id: &str, layer: &Value, rules: &Value, z: i32) {
    let resource = if !s(layer, "/resource").is_empty() {
        s(layer, "/resource")
    } else {
        s(layer, "/assetPath")
    };
    let meta = rules["resourcesByAddress"]
        .as_object()
        .into_iter()
        .flat_map(|v| v.values())
        .chain(
            rules["runtimeObjects"]
                .as_object()
                .into_iter()
                .flat_map(|v| v.values()),
        )
        .find(|v| s(v, "/resource") == resource);
    let mut merged = meta.cloned().unwrap_or_else(|| layer.clone());
    if !merged.is_object() {
        return;
    }
    merged["resource"] = json!(resource);
    merged["x"] = layer["x"].clone();
    merged["y"] = layer["y"].clone();
    merged["frame"] = layer["frame"].clone();
    merged["transparent"] = if b(layer, "/firstPixelTransparent") {
        json!("firstPixel")
    } else {
        layer["transparentIndex"].clone()
    };
    if ["flc", "fli"].contains(&s(&merged, "/kind")) {
        animation(f, id, &merged, z);
    } else {
        crop(
            f,
            id,
            &merged,
            n(layer, "/x"),
            n(layer, "/y"),
            z,
            Rect::new(0, n(layer, "/frame") * 66, 55, 66),
        );
    }
}

fn final_battle(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    st: &Value,
    ui: &PresentationState,
) {
    let rules = &c.rules["finalBattle"];
    let phase = s(st, "/phase");
    let shop = phase == "shop";
    if phase == "opening" {
        let time = n(st, "/clockMs")
            .saturating_sub(n(st, "/openingStartedMs"))
            .max(0) as u64;
        f.video = Some(SceneVideo {
            resource_id: s(rules, "/openingVideo").into(),
            time_ms: time,
            fps: 24,
            bounds: Rect::new(0, 60, 640, 360),
            scanlines: true,
        });
        sound(f, "final-opening", s(rules, "/openingAudio"), false, 0);
        f.sounds.last_mut().unwrap().position_ms = Some(time);
        control(
            f,
            "final-continue",
            "跳过最终战开场",
            Rect::new(0, 0, 640, 480),
            game("battleAction", "continue", None),
            &["Enter", "Escape"],
        );
        return;
    }
    sound(
        f,
        "final-music",
        s(final_resource(rules, 0x496208), "/resource"),
        true,
        0,
    );
    if phase == "intro" || shop {
        image(
            f,
            "final-intro-background",
            &final_image(rules, 0x4960a4, 0, 0),
            0,
        );
        f.palette = Some(s(final_resource(rules, 0x4960a4), "/resource").into());
        if shop {
            image(
                f,
                "final-shop-panel",
                &final_image(
                    rules,
                    0x4960a8,
                    328,
                    st["shopDisplayedPanelY"]
                        .as_i64()
                        .map(|v| v as i32)
                        .unwrap_or_else(|| n(st, "/shopPanelY")),
                ),
                2,
            );
            image(
                f,
                "final-money-panel",
                &final_image(rules, 0x496200, 27, 333),
                3,
            );
            image(
                f,
                "final-card-count",
                &final_image(rules, 0x496088, 209, 322),
                3,
            );
        }
        image(
            f,
            "final-dialogue-panel",
            &final_image(rules, 0x496204, 0, 356),
            4,
        );
        for (i, layer) in a(st, "/layers").iter().enumerate() {
            final_layer(
                f,
                &format!("final-intro-layer-{i}"),
                layer,
                rules,
                5 + i as i32,
            );
        }
        let step = st["introDisplayStep"]
            .as_i64()
            .map(|v| v as i32)
            .unwrap_or_else(|| n(st, "/introStep"));
        if !shop && (step == 2 || step > 3) {
            image(
                f,
                "final-valuation-panel",
                &final_image(rules, 0x495fc4, 328, 9),
                40,
            );
        }
        if shop {
            final_items(f, st, rules, true);
        }
    } else {
        let background_resource = s(final_resource(rules, 0x4961fc), "/resource");
        let background = a(st, "/layers").iter().find(|layer| {
            s(layer, "/resource") == background_resource
                || s(layer, "/assetPath") == background_resource
        });
        final_animation(
            f,
            "final-background",
            final_resource(rules, 0x4961fc),
            background.map(|v| n(v, "/frame")).unwrap_or(0),
            0,
            0,
            0,
        );
        f.sprites.last_mut().unwrap().transparent = Transparency::Opaque;
        f.palette = Some(background_resource.into());
        final_health(f, st, rules);
        image(
            f,
            "final-attack-label",
            &final_image(rules, 0x49608c, 387, 440),
            5,
        );
        image(
            f,
            "final-defense-label",
            &final_image(rules, 0x496090, 387, 456),
            5,
        );
        for (i, card) in a(st, "/cards").iter().enumerate() {
            let is_selected = b(card, "/selected") && !b(st, "/cardsMoving");
            let p = &rules[if is_selected {
                "selectedCardPositions"
            } else {
                "cardPositions"
            }][i];
            let x = if is_selected {
                n(p, "/0")
            } else {
                n(card, "/x")
            };
            let y = n(p, "/1");
            let meta = final_image(
                rules,
                (if is_selected { 0x4960f0 } else { 0x496128 }) + n(card, "/kind") as u32 * 4,
                x,
                y,
            );
            if is_selected {
                image(f, &format!("final-card-{i}"), &meta, 10);
            } else {
                crop(
                    f,
                    &format!("final-card-{i}"),
                    &meta,
                    x,
                    y,
                    10,
                    Rect::new(0, 0, 55, 66),
                );
            }
            for (wanted, address, name, z) in [
                (is_selected, 0x496098, "selected", 11),
                (
                    ui.hover.as_deref() == Some(&format!("final-card-{i}"))
                        && !b(st, "/cardsMoving"),
                    0x496094,
                    "hover",
                    12,
                ),
            ] {
                if wanted {
                    let resource = final_resource(rules, address);
                    let frame = n(st, "/clockMs") / n(resource, "/frameDurationMs").max(1) % 2;
                    final_animation(
                        f,
                        &format!("final-card-{name}-{i}"),
                        resource,
                        frame,
                        if name == "hover" {
                            n(&rules["cardPositions"][i], "/0")
                        } else {
                            x
                        },
                        y,
                        z,
                    );
                }
            }
        }
        final_items(f, st, rules, false);
        if let Some(selected) = st["selectedItem"].as_u64() {
            image(
                f,
                "final-selected-item",
                &final_image(
                    rules,
                    0x496100 + n(&st["items"][selected as usize], "") as u32 * 4,
                    463,
                    293,
                ),
                22,
            );
            image(
                f,
                "final-use-item",
                &final_image(rules, 0x4960c4, 538, 344),
                22,
            );
        }
        for (i, layer) in a(st, "/layers").iter().enumerate() {
            if Some(layer) != background {
                final_layer(f, &format!("final-layer-{i}"), layer, rules, 30 + i as i32);
            }
        }
        for (left, x, address) in [(true, 3, 0x4960b4), (false, 417, 0x4960b8)] {
            if if left {
                n(st, "/itemScroll") > 0
            } else {
                n(st, "/itemScroll") + 16 < a(st, "/items").len() as i32
            } {
                let resource = final_resource(rules, address);
                let frame = n(st, "/clockMs") / n(resource, "/frameDurationMs").max(1)
                    % n(resource, "/frameCount").max(1);
                final_animation(
                    f,
                    &format!("final-scroll-{left}"),
                    resource,
                    frame,
                    x,
                    342,
                    80,
                );
            }
        }
    }
    final_controls(f, st, rules, ui);
    final_text(f, snapshot, st, rules, ui);
    for (i, cue) in a(st, "/sounds").iter().enumerate() {
        let resource = s(cue, "");
        if c.manifest["resources"].get(resource).is_some() {
            sound(
                f,
                &format!("final-cue-{i}-{resource}"),
                resource,
                false,
                n(st, "/soundSerial").max(0) as u64,
            );
        }
    }
    f.stopped_sounds.extend(
        a(st, "/soundStops")
            .iter()
            .map(|v| s(v, "").to_owned())
            .filter(|resource| !resource.is_empty()),
    );
}

fn final_items(f: &mut SceneFrame, st: &Value, r: &Value, shop: bool) {
    let positions = &r[if shop {
        "heldShopPositions"
    } else {
        "heldBattlePositions"
    }];
    let scroll = n(st, "/itemScroll").max(0) as usize;
    let items: Vec<&Value> = if shop && st["shopVisibleItems"].is_array() {
        a(st, "/shopVisibleItems").iter().collect()
    } else {
        a(st, "/items")
            .iter()
            .skip(scroll)
            .take(a(positions, "").len())
            .collect()
    };
    for (slot, item) in items.iter().enumerate() {
        let kind = n(item, "");
        if !(0..=9).contains(&kind) {
            continue;
        }
        let p = &positions[slot];
        image(
            f,
            &format!("final-held-{slot}"),
            &final_image(
                r,
                (if shop { 0x495ff4 } else { 0x495fc8 }) + kind as u32 * 4,
                n(p, "/0"),
                n(p, "/1"),
            ),
            20,
        );
    }
    if let Some(selected) = st[if shop {
        "shopSelectedSell"
    } else {
        "selectedItem"
    }]
    .as_u64()
    {
        if let Some(p) = selected
            .checked_sub(scroll as u64)
            .and_then(|i| positions.get(i as usize))
        {
            image(
                f,
                "final-item-selection",
                &final_image(r, 0x495ff0, n(p, "/0") - 1, n(p, "/1") - 1),
                21,
            );
        }
    }
}
fn final_health(f: &mut SceneFrame, st: &Value, r: &Value) {
    for (side, hp, range, x, y) in [
        (0, n(st, "/displayedEnemyHp"), 6000, 12, 12),
        (1, n(st, "/displayedHp"), 12000, 17, 317),
    ] {
        let band = (hp / range).max(0);
        let remainder = (hp % range / (range / 600)).max(0);
        image(
            f,
            &format!("final-hp-start-{side}"),
            &final_image(r, 0x495f2c, x - 4, y),
            2,
        );
        image(
            f,
            &format!("final-hp-end-{side}"),
            &final_image(r, 0x495f34, x + if side == 1 { 601 } else { 600 }, y),
            2,
        );
        strip(
            f,
            &format!("final-hp-border-{side}"),
            &final_image(r, 0x495f30, 0, 0),
            x,
            y,
            600,
            2,
        );
        if band > 0 {
            strip(
                f,
                &format!("final-hp-under-{side}"),
                &final_image(r, 0x495f34 + band.min(5) as u32 * 4, 0, 0),
                x,
                y + 3,
                600,
                3,
            );
        } else {
            fill(f, &format!("final-hp-empty-{side}"), x, y + 3, 600, 10, 3);
        }
        strip(
            f,
            &format!("final-hp-full-{side}"),
            &final_image(r, 0x495f38 + band.min(4) as u32 * 4, 0, 0),
            x,
            y + 3,
            remainder,
            3,
        );
    }
}
fn final_button(
    f: &mut SceneFrame,
    id: &str,
    label: &str,
    rect: Rect,
    action: &str,
    index: Option<usize>,
    address: Option<u32>,
    r: &Value,
    ui: &PresentationState,
    enabled: bool,
) {
    if let Some(address) = address {
        button(
            f,
            id,
            &final_image(r, address, rect.x, rect.y),
            game("battleAction", action, index),
            label,
            if address == 0x4960c0 { 25 } else { 90 },
            ui,
        );
        f.controls.last_mut().unwrap().enabled = enabled;
    } else {
        enabled_control(
            f,
            id,
            label,
            rect,
            game("battleAction", action, index),
            enabled,
            &[],
        );
    }
}
fn final_controls(f: &mut SceneFrame, st: &Value, r: &Value, ui: &PresentationState) {
    let shop = s(st, "/phase") == "shop";
    if s(st, "/phase") == "intro" {
        control(
            f,
            "final-continue",
            "继续最终战介绍",
            Rect::new(0, 0, 640, 480),
            game("battleAction", "continue", None),
            &["Enter"],
        );
        return;
    }
    if shop {
        for (i, p) in a(r, "/shopCardPositions").iter().enumerate() {
            control(
                f,
                &format!("final-buy-{i}"),
                &format!("查看最终战道具 {}", i + 1),
                Rect::new(n(p, "/0"), n(p, "/1"), 48, 48),
                game("battleAction", "inspectBuy", Some(i)),
                &[],
            );
        }
        final_button(
            f,
            "final-start",
            "开始最终战",
            Rect::new(542, 385, 52, 59),
            "continue",
            None,
            Some(0x4960bc),
            r,
            ui,
            true,
        );
        let selected_buy = st["shopSelectedBuy"].as_u64();
        let selected_sell = st["shopSelectedSell"].as_u64();
        if let Some(index) = selected_buy.or(selected_sell) {
            for (id, resource, x, action, label, selected) in [
                (
                    "final-trade-confirm",
                    "y01",
                    393,
                    if selected_buy.is_some() {
                        "buy"
                    } else {
                        "sell"
                    },
                    if selected_buy.is_some() {
                        "购买最终战道具"
                    } else {
                        "出售最终战道具"
                    },
                    Some(index as usize),
                ),
                (
                    "final-trade-cancel",
                    "n01",
                    458,
                    "cancelShop",
                    "取消道具交易",
                    None,
                ),
            ] {
                button(
                    f,
                    id,
                    &json!({"resource":format!("install/public/button/{resource}.bmp"),"x":x,"y":423,"width":59,"height":75,"transparent":1}),
                    game("battleAction", action, selected),
                    label,
                    90,
                    ui,
                );
            }
        }
    } else {
        for (i, p) in a(r, "/cardPositions")
            .iter()
            .take(a(st, "/cards").len())
            .enumerate()
        {
            enabled_control(
                f,
                &format!("final-card-{i}"),
                &format!("选择最终战卡片 {}", i + 1),
                Rect::new(n(p, "/0"), n(p, "/1"), 48, 60),
                game("battleAction", "selectCard", Some(i)),
                n(st, "/combatState") == 0 && !b(st, "/cardsMoving"),
                &[],
            );
        }
        final_button(
            f,
            "final-attack",
            "最终战出招",
            Rect::new(375, 389, 53, 51),
            "attack",
            None,
            Some(0x4960c0),
            r,
            ui,
            n(st, "/combatState") == 0,
        );
        if st["selectedItem"].as_u64().is_some() {
            enabled_control(
                f,
                "final-use",
                "使用最终战道具",
                Rect::new(538, 344, 47, 25),
                game("battleAction", "useItem", None),
                n(st, "/combatState") == 0 && !b(st, "/itemUsed"),
                &[],
            );
        }
    }
    let positions = &r[if shop {
        "heldShopPositions"
    } else {
        "heldBattlePositions"
    }];
    let scroll = n(st, "/itemScroll").max(0) as usize;
    for (slot, p) in a(positions, "")
        .iter()
        .take(a(st, "/items").len().saturating_sub(scroll))
        .enumerate()
    {
        enabled_control(
            f,
            &format!("final-item-{slot}"),
            &format!("选择最终战道具 {}", slot + scroll + 1),
            Rect::new(n(p, "/0"), n(p, "/1"), 21, 25),
            game(
                "battleAction",
                if shop { "inspectSell" } else { "selectItem" },
                Some(slot + scroll),
            ),
            if shop {
                st["shopAnimation"].is_null()
            } else {
                !b(st, "/itemUsed")
            },
            &[],
        );
    }
    for (left, label, action, x) in [
        (
            true,
            "最终战道具向左",
            "scrollLeft",
            if shop { 24 } else { 3 },
        ),
        (
            false,
            "最终战道具向右",
            "scrollRight",
            if shop { 295 } else { 417 },
        ),
    ] {
        if if left {
            scroll > 0
        } else {
            scroll + a(positions, "").len() < a(st, "/items").len()
        } {
            final_button(
                f,
                &format!("final-scroll-{left}"),
                label,
                Rect::new(
                    x,
                    if shop { 45 } else { 342 },
                    if shop { 21 } else { 30 },
                    if shop { 29 } else { 20 },
                ),
                action,
                None,
                if shop {
                    Some(if left { 0x4960ac } else { 0x4960b0 })
                } else {
                    None
                },
                r,
                ui,
                true,
            );
        }
    }
}
fn final_text(f: &mut SceneFrame, snapshot: &Value, st: &Value, r: &Value, ui: &PresentationState) {
    let phase = s(st, "/phase");
    let shop = phase == "shop";
    let step = st["introDisplayStep"]
        .as_i64()
        .map(|v| v as i32)
        .unwrap_or_else(|| n(st, "/introStep"));
    if phase == "intro" || shop {
        if shop || step > 0 {
            let message = if shop {
                s(st, "/shopMessage")
            } else if st["introDisplayedMessage"].is_string() {
                s(st, "/introDisplayedMessage")
            } else {
                s(&r["introTexts"][(step - 1).clamp(0, 3) as usize], "")
            };
            label(f, "final-dialogue", message, 128, 371, 16, WHITE, true);
            f.texts.last_mut().unwrap().bounds = Rect::new(128, 371, 400, 84);
        }
        if shop {
            label(
                f,
                "final-money",
                n(st, "/money").to_string(),
                77,
                334,
                14,
                WHITE,
                false,
            );
            label(
                f,
                "final-item-count",
                st["shopDisplayedItemCount"]
                    .as_u64()
                    .unwrap_or(a(st, "/items").len() as u64)
                    .to_string(),
                269,
                326,
                14,
                WHITE,
                false,
            );
        } else if step == 2 || step > 3 {
            let career = snapshot
                .pointer("/life/profession")
                .and_then(Value::as_i64)
                .unwrap_or(-1);
            let (labels, values): (Vec<String>, Vec<String>) = if step == 2 {
                (
                    a(r, "/battleIndexLabels")
                        .iter()
                        .map(|v| s(v, "").into())
                        .collect(),
                    vec![
                        snapshot
                            .pointer("/life/lifeLevel")
                            .and_then(Value::as_i64)
                            .unwrap_or(1)
                            .to_string(),
                        if career < 0 {
                            s(r, "/unemployedCareerName").into()
                        } else {
                            s(&r["careerNames"][career as usize], "").into()
                        },
                        format!(
                            "{:.1}",
                            n(&r["careerHpMultipliers"][(career + 1) as usize], "") as f64 / 10.0
                        ),
                        format!(
                            "{:.1}",
                            n(&r["careerPower"][(career + 1) as usize], "") as f64 / 10.0
                        ),
                        "0.0".into(),
                        format!("{:.1}", n(st, "/power") as f64 / 10.0),
                        n(st, "/hp").to_string(),
                    ],
                )
            } else {
                (
                    a(r, "/valuationLabels")
                        .iter()
                        .map(|v| s(v, "").to_owned())
                        .chain(std::iter::once(s(r, "/totalValuationLabel").into()))
                        .collect(),
                    a(st, "/valuation")
                        .iter()
                        .map(|v| n(v, "").to_string())
                        .chain(std::iter::once(n(st, "/money").to_string()))
                        .collect(),
                )
            };
            for (i, title) in labels.iter().enumerate() {
                let p = &r["valuationLabelPositions"][i];
                let value_p = &r["valuationValuePositions"][i];
                label(
                    f,
                    &format!("final-stat-title-{i}"),
                    title,
                    328 + n(p, "/0"),
                    9 + n(p, "/1"),
                    14,
                    WHITE,
                    false,
                );
                label(
                    f,
                    &format!("final-stat-value-{i}"),
                    values.get(i).cloned().unwrap_or_default(),
                    328 + n(value_p, "/0"),
                    9 + n(value_p, "/1"),
                    14,
                    WHITE,
                    false,
                );
            }
        }
    } else {
        label(
            f,
            "final-attack-value",
            n(st, "/attack").to_string(),
            403,
            440,
            14,
            RED,
            false,
        );
        label(
            f,
            "final-defense-value",
            n(st, "/defense").to_string(),
            403,
            456,
            14,
            RED,
            false,
        );
        if n(st, "/hpTextY") > 300 && n(st, "/hpDelta") != 0 {
            label(
                f,
                "final-player-damage",
                format!(
                    "{}{}点",
                    if n(st, "/hpDelta") < 0 {
                        "减少"
                    } else {
                        "增加"
                    },
                    n(st, "/hpDelta").abs()
                ),
                50,
                300,
                16,
                WHITE,
                false,
            );
        }
        if n(st, "/enemyTextY") > 0 {
            label(
                f,
                "final-enemy-damage",
                format!("减少{}点", n(st, "/enemyHpDelta")),
                50,
                0,
                16,
                WHITE,
                false,
            );
        }
        if let Some(index) = st["selectedItem"].as_u64() {
            label(
                f,
                "final-item-description",
                s(
                    &r["itemDescriptions"][n(&st["items"][index as usize], "").max(0) as usize],
                    "",
                ),
                470,
                381,
                16,
                WHITE,
                true,
            );
            f.texts.last_mut().unwrap().bounds = Rect::new(470, 381, 160, 64);
        }
        if !b(st, "/cardsMoving") {
            for (i, card) in a(st, "/cards").iter().enumerate() {
                if ui.hover.as_deref() == Some(&format!("final-card-{i}")) {
                    let p = &r["cardPositions"][i];
                    label(
                        f,
                        "final-card-attack",
                        format!("攻击力 {}", n(card, "/attack")),
                        n(p, "/0") + 10,
                        n(p, "/1") + 60,
                        12,
                        RED,
                        false,
                    );
                    label(
                        f,
                        "final-card-defense",
                        format!("防御力 {}", n(card, "/defense")),
                        n(p, "/0") + 10,
                        n(p, "/1") + 72,
                        12,
                        RED,
                        false,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn public_session(phase: &str) -> (virtual_life_core::GameSession, Value, Value, Value) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let read = |name: &str| {
            serde_json::from_slice::<Value>(&std::fs::read(root.join(name)).unwrap()).unwrap()
        };
        let maps = read("assets/catalog/maps.json");
        let rules = read("assets/catalog/rules.json");
        let mut content = maps.clone();
        content["rules"] = rules.clone();
        for name in ["events", "questionnaires"] {
            content[name] = read(&format!("assets/catalog/{name}.json"));
        }
        let mut session = virtual_life_core::GameSession::from_content(content).unwrap();
        let fixture = read("tests/fixtures/bevy-battle-controls.json");
        let restored = session.restore(fixture["cases"][phase]["save"].as_str().unwrap());
        assert!(
            restored.ok,
            "Public battle save must restore: {:?}",
            restored.error
        );
        (session, read("assets/manifest.json"), maps, rules)
    }
    fn public_control(
        session: &mut virtual_life_core::GameSession,
        c: &SceneContent<'_>,
        id: &str,
    ) {
        let snapshot = serde_json::to_value(session.snapshot()).unwrap();
        let mut frame = SceneFrame::default();
        compose(
            &mut frame,
            c,
            &snapshot,
            &PresentationState {
                screen: Screen::Game,
                ..Default::default()
            },
        );
        let control = frame
            .controls
            .iter()
            .find(|control| control.id == id && control.enabled)
            .unwrap_or_else(|| panic!("Missing public battle control {id}"));
        let Action::Game(mut command) = control.action.clone() else {
            panic!("Battle control must dispatch a domain command")
        };
        command["seq"] = json!(0);
        let response = session.send(serde_json::from_value(command).unwrap());
        assert!(
            response.ok,
            "Control {id} must be accepted by the core: {:?}",
            response.error
        );
    }
    #[test]
    fn ordinary_public_help_input_opens_and_closes_all_original_help_pages() {
        let (mut session, manifest, maps, rules) = public_session("playing");
        let c = SceneContent {
            manifest: &manifest,
            maps: &maps,
            rules: &rules,
        };
        public_control(&mut session, &c, "battle-help");
        let snapshot = serde_json::to_value(session.snapshot()).unwrap();
        assert!(b(&snapshot, "/pending/state/help"));
        assert_eq!(n(&snapshot, "/pending/state/helpPage"), 0);
        for page in 1..=9 {
            public_control(&mut session, &c, "battle-help-next");
            let snapshot = serde_json::to_value(session.snapshot()).unwrap();
            assert_eq!(n(&snapshot, "/pending/state/helpPage"), page);
            assert_eq!(b(&snapshot, "/pending/state/help"), page < 9);
        }
        assert_eq!(
            s(
                &serde_json::to_value(session.snapshot()).unwrap(),
                "/pending/game"
            ),
            "ordinary"
        );
    }
    #[test]
    fn ordinary_public_reward_input_finishes_real_battle_rewards() {
        let (mut session, manifest, maps, rules) = public_session("reward");
        let c = SceneContent {
            manifest: &manifest,
            maps: &maps,
            rules: &rules,
        };
        let mut skipped = 0;
        for _ in 0..8 {
            let snapshot = serde_json::to_value(session.snapshot()).unwrap();
            if s(&snapshot, "/pending/game") != "ordinary"
                || s(&snapshot, "/pending/state/phase") != "reward"
            {
                break;
            }
            // Original Escape/skip is sampled only during value_phase 2,
            // after its 1600 ms introduction and before its 1500 ms outro.
            for _ in 0..3 {
                if n(
                    &serde_json::to_value(session.snapshot()).unwrap(),
                    "/pending/state/bonus/valuePhase",
                ) == 2
                {
                    break;
                }
                let response = session.frame(virtual_life_core::FrameStep { elapsed_ms: 1601 });
                assert!(response.ok);
            }
            assert_eq!(
                n(
                    &serde_json::to_value(session.snapshot()).unwrap(),
                    "/pending/state/bonus/valuePhase"
                ),
                2
            );
            public_control(&mut session, &c, "battle-reward");
            skipped += 1;
            let response = session.frame(virtual_life_core::FrameStep { elapsed_ms: 16 });
            assert!(
                response.ok,
                "Reward frame must resolve: {:?}",
                response.error
            );
            assert_eq!(
                n(
                    &serde_json::to_value(session.snapshot()).unwrap(),
                    "/pending/state/bonus/valuePhase"
                ),
                3
            );
            let response = session.frame(virtual_life_core::FrameStep { elapsed_ms: 1501 });
            assert!(response.ok);
        }
        let snapshot = serde_json::to_value(session.snapshot()).unwrap();
        assert!(skipped > 0);
        assert!(
            s(&snapshot, "/pending/game") != "ordinary"
                || s(&snapshot, "/pending/state/phase") != "reward",
            "All original reward panels must complete after their public skip input"
        );
    }
    #[test]
    fn ordinary_attack_respects_original_cooldown_and_selected_cards() {
        let mut state = json!({"phase":"playing","selected":[true],"playerCooldown":2,"playerTenths":30,"playerSeconds":3});
        assert!(attack_ready(&state));
        state["playerTenths"] = json!(20);
        assert!(!attack_ready(&state));
        state["playerTenths"] = json!(30);
        state["enemyAttacking"] = json!(true);
        assert!(!attack_ready(&state));
        state["enemyAttacking"] = json!(false);
        state["selected"] = json!([false]);
        assert!(!attack_ready(&state));
    }
    #[test]
    fn original_door_animation_has_no_skip_hitbox() {
        let rules = json!({"battle":{"rendering":{"versusDoors":[{"resource":"left.bmp","width":412,"height":480},{"resource":"right.bmp","width":412,"height":480}],"sounds":{}}}});
        let content = SceneContent {
            manifest: &Value::Null,
            maps: &Value::Null,
            rules: &rules,
        };
        let state = json!({"showOpening":true,"openingLeftX":-396,"openingRightX":624});
        let mut frame = SceneFrame::default();
        ordinary(
            &mut frame,
            &content,
            &Value::Null,
            &state,
            &PresentationState::default(),
        );
        assert!(frame.controls.is_empty());
        assert_eq!(frame.sprites[0].x, -396);
        assert_eq!(frame.sprites[1].x, 624);
        assert_eq!(frame.palette, Some("left.bmp".into()));
    }
    #[test]
    fn ordinary_sound_stops_resolve_the_same_original_voice() {
        let mut frame = SceneFrame::default();
        let state = json!({"input":{"sex":0,"stage":1},"enemy":0,"sounds":["playerWeak"],"soundStops":["playerWeak"]});
        ordinary_sounds(
            &mut frame,
            &state,
            &json!({"playerVoices":[["weak.wav","strong.wav"]]}),
            40,
        );
        assert_eq!(frame.sounds[0].resource_id, "weak.wav");
        assert_eq!(frame.stopped_sounds, vec!["weak.wav"]);
    }
    #[test]
    fn final_shop_controls_keep_original_buy_and_sell_coordinates() {
        let mut frame = SceneFrame::default();
        let rules = json!({"resourcesByAddress":{"0x004960bc":{"resource":"start.bmp","width":52,"height":177}},"shopCardPositions":[[10,20]],"heldShopPositions":[[50,60]]});
        final_controls(
            &mut frame,
            &json!({"phase":"shop","shopSelectedBuy":2,"items":[1],"itemScroll":0}),
            &rules,
            &PresentationState::default(),
        );
        let confirmation = frame
            .controls
            .iter()
            .find(|control| control.id == "final-trade-confirm")
            .unwrap();
        assert_eq!(confirmation.bounds, Rect::new(393, 423, 59, 25));
        assert_eq!(confirmation.action, game("battleAction", "buy", Some(2)));
        assert_eq!(
            frame
                .controls
                .iter()
                .find(|control| control.id == "final-item-0")
                .unwrap()
                .bounds,
            Rect::new(50, 60, 21, 25)
        );
    }
}
