use super::*;
fn bmp(
    f: &mut SceneFrame,
    id: &str,
    dir: &str,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
    source: Option<Rect>,
) -> usize {
    let i = sprite(f, id, format!("install/{dir}/{name}.bmp"), x, y, z);
    if let Some(source) = source {
        f.sprites[i].width = Some(source.width);
        f.sprites[i].height = Some(source.height);
        f.sprites[i].source = Some(source);
    }
    i
}
fn flc(
    f: &mut SceneFrame,
    id: &str,
    dir: &str,
    name: &str,
    x: i32,
    y: i32,
    z: i32,
    frame: u32,
) -> usize {
    let i = sprite(f, id, format!("install/{dir}/{name}.flc"), x, y, z);
    f.sprites[i].frame = frame;
    i
}
fn anim(f: &mut SceneFrame, id: &str, v: &Value, z: i32) -> usize {
    let i = sprite(f, id, s(v, "/resource"), n(v, "/x"), n(v, "/y"), z);
    f.sprites[i].frame = n(v, "/frame").max(0) as u32;
    i
}
fn action(
    f: &mut SceneFrame,
    id: &str,
    label: &str,
    r: Rect,
    name: &str,
    index: Option<usize>,
    keys: &[&str],
) {
    control(f, id, label, r, game("minigameAction", name, index), keys)
}
fn start(f: &mut SceneFrame, name: &str) {
    action(
        f,
        "minigame-start",
        name,
        Rect::new(0, 0, 640, 480),
        "start",
        None,
        &["Enter", "Space", "Escape"],
    )
}
fn held(f: &mut SceneFrame, id: &str, label: &str, r: Rect, side: &str, keys: &[&str], st: &Value) {
    let pressed = Action::Game(
        json!({"type":"minigameAction","action":"input","left":side=="left"||b(st,"/left"),"right":side=="right"||b(st,"/right"),"fire":b(st,"/fire")}),
    );
    control(f, id, label, r, pressed.clone(), keys);
    let ctrl = f.controls.last_mut().unwrap();
    ctrl.on_press = Some(pressed);
    ctrl.on_release = Some(Action::Game(
        json!({"type":"minigameAction","action":"input","left":side!="left"&&b(st,"/left"),"right":side!="right"&&b(st,"/right"),"fire":b(st,"/fire")}),
    ));
}
fn score(f: &mut SceneFrame, id: &str, value: i32, x: i32, y: i32) {
    text(f, id, value.to_string(), Rect::new(x, y, 120, 16), [255; 4]);
    f.texts.last_mut().unwrap().font = TextFont::Ascii;
}

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let state = &snapshot["pending"]["state"];
    let name = s(snapshot, "/pending/game");
    match name {
        "tracking" => tracking(f, c, state, ui),
        "hammer" => hammer(f, state, ui),
        "fist" => fist(f, state),
        "gas" => gas(f, state, ui),
        "fire" => fire(f, state),
        "firebee" => firebee(f, state),
        "spiritSlots" => {
            super::map::compose(f, c, snapshot, ui);
            slots(f, c, state)
        }
        _ => {}
    }
    if s(state, "/phase") == "reward" && name != "spiritSlots" {
        action(
            f,
            "minigame-finish-reward",
            "完成小游戏奖励",
            Rect::new(0, 0, 640, 480),
            "finishRewardAnimation",
            None,
            &["Escape", "MouseRight", "Enter", "Space"],
        );
    }
    for sp in &mut f.sprites {
        if [
            "tracking-background",
            "tracking-board",
            "hammer-background",
            "hammer-board",
            "fist-background",
            "fist-board",
            "gas-background",
            "gas-board",
            "fire-background",
            "fire-board",
            "firebee-background",
        ]
        .contains(&sp.id.as_str())
        {
            sp.transparent = Transparency::Opaque;
        }
    }
    f.tick = Some(json!({"type":"minigameTick"}));
}

fn bonus_at(f: &mut SceneFrame, kind: &str, state: &Value, _clock: u64) {
    if !b(state, "/visible") {
        return;
    }
    let (x, y) = match kind {
        "ham" | "fire" => (191, 142),
        "targ" => (213, 125),
        "fly" => (133, 96),
        _ => (173, 96),
    };
    bmp(
        f,
        "bonus-description",
        "public/bonus",
        &format!("{kind}desc"),
        x,
        y,
        100,
        None,
    );
    let i = flc(
        f,
        "bonus-animation",
        "public/bonus",
        &format!("{kind}bns"),
        x + 51,
        y + 62,
        101,
        0,
    );
    f.sprites[i].playback = Some(Playback {
        interval_ms: 40,
        once: true,
        max_advances: 1,
        immediate: false,
    });
    for (key, units, dy) in [
        ("value", "valueUnits", 7),
        ("increment", "incrementUnits", 70),
    ] {
        let value = n(state, &format!("/{key}"));
        let mut digits = value
            .unsigned_abs()
            .to_string()
            .chars()
            .map(|ch| ch.to_digit(10).unwrap_or(0))
            .collect::<Vec<_>>();
        if let Some(unit) = state[units].as_u64() {
            if let Some(last) = digits.last_mut() {
                *last = unit as u32;
            }
        }
        let width = n(state, "/digitCount").max(0) as usize;
        for index in 0..width {
            if index + digits.len() < width {
                continue;
            }
            let digit = digits[index + digits.len() - width];
            bmp(
                f,
                &format!("bonus-{key}-{index}"),
                "public/bonus",
                &format!("{kind}fig"),
                x + 157 + index as i32 * 28,
                y + dy,
                102,
                Some(Rect::new(digit as i32 * 26, 0, 26, 38)),
            );
        }
    }
}

fn tracking(f: &mut SceneFrame, c: &SceneContent<'_>, st: &Value, ui: &PresentationState) {
    let phase = s(st, "/phase");
    bmp(f, "tracking-background", "gbs", "bg02", 0, 0, 0, None);
    if phase == "reward" {
        bonus_at(f, "alie", &st["reward"], ui.scene_ms);
        return;
    }
    bmp(f, "tracking-board", "gbs", "gbs01", 101, 51, 1, None);
    bmp(f, "tracking-goal", "gbs", "yi", 0, 172, 20, None);
    for round in 0..3 {
        let result = &st["outcomes"][round];
        bmp(
            f,
            &format!("tracking-outcome-{round}"),
            "gbs",
            if result.as_bool() == Some(true) {
                "yao2"
            } else {
                "yao1"
            },
            213 + round as i32 * 76,
            417,
            21,
            None,
        );
        if result.as_bool() == Some(false) {
            bmp(
                f,
                &format!("tracking-x-{round}"),
                "gbs",
                "x",
                219 + round as i32 * 76,
                422,
                22,
                None,
            );
        }
    }
    if phase == "reveal" {
        bmp(f, "tracking-prompt", "gbs", "gbs02", 252, 266, 100, None);
    }
    for (i, obj) in a(st, "/objects").iter().enumerate() {
        let sprites = [
            "gbu01", "gbu02", "gbu09", "gbu03", "gbu10", "gbu07", "gbu08", "gbu04", "gbu05",
            "gbu06",
        ];
        let mut name = sprites
            .get(n(obj, "/sprite").max(0) as usize)
            .copied()
            .unwrap_or("gbu01");
        let mut y = n(obj, "/y");
        let mut once = n(obj, "/sprite") >= 5;
        if phase == "reveal"
            && n(st, "/revealedTarget") == i as i32
            && !ui.runtime.tracking_transformed
        {
            name = if n(obj, "/x") < 561 { "gbu12" } else { "gbn02" };
            y -= 20;
            once = name == "gbu12";
        }
        if phase == "roundResult" {
            if n(st, "/selected") == i as i32 {
                name = if n(st, "/target") == i as i32 {
                    "gbn01"
                } else {
                    "gbn07"
                };
                once = true;
            } else if n(st, "/target") == i as i32 {
                name = "change_n";
                once = true;
            }
        }
        let resource = format!("install/gbs/{name}.flc");
        let meta = c.media(&resource);
        let count = n(&meta, "/frameCount").max(1) as u64;
        let delay = n(&meta, "/frameDelayMs").max(16) as u64;
        let frame = if once {
            (ui.scene_ms / delay).min(count - 1)
        } else {
            ui.scene_ms / delay % count
        };
        let k = flc(
            f,
            &format!("tracking-object-{i}"),
            "gbs",
            name,
            n(obj, "/x"),
            y,
            i as i32 + 2,
            frame as u32,
        );
        f.sprites[k].transparent = Transparency::FirstPixel;
        f.sprites[k].playback = Some(Playback {
            interval_ms: delay.saturating_sub(1).max(1) as u32,
            once,
            max_advances: 4,
            immediate: false,
        });
        if n(st, "/idleAnimation/0") == i as i32 && !st["idleAnimation"].is_null() {
            f.requests.push(SceneRequest::AfterAnimation {
                resource_id: resource,
                elapsed_ms: if ui
                    .runtime
                    .animations
                    .get(&format!("tracking-object-{i}"))
                    .is_some_and(|clock| clock.finished)
                {
                    u64::MAX
                } else {
                    0
                },
                action: game("minigameAction", "finishIdleAnimation", None),
            });
        }
        if phase == "choose" {
            action(
                f,
                &format!("tracking-choose-{i}"),
                &format!("选择第 {} 个", i + 1),
                Rect::new(n(obj, "/x"), n(obj, "/y"), 70, 80),
                "choose",
                Some(i),
                &[&format!("{}", i + 1)],
            );
        }
    }
    if phase == "memorize" || phase == "roundResult" {
        let act = if phase == "memorize" {
            "startShuffle"
        } else {
            "nextRound"
        };
        action(
            f,
            "tracking-next",
            if phase == "memorize" {
                "开始追踪"
            } else {
                "下一轮"
            },
            Rect::new(240, 350, 160, 36),
            act,
            None,
            &["Enter", "Space"],
        );
    }
    let snd = if phase == "shuffle" || phase == "settle" {
        "gbsc01"
    } else if phase == "roundResult" {
        if st["selected"] == st["target"] {
            "gbsc02"
        } else {
            "gbsc03"
        }
    } else {
        "gbsc04"
    };
    sound(
        f,
        "tracking-sound",
        format!("install/sound/gb/{snd}.wav"),
        phase != "roundResult",
        n(st, "/round") as u64,
    );
}
fn hammer(f: &mut SceneFrame, st: &Value, ui: &PresentationState) {
    let phase = s(st, "/phase");
    bmp(f, "hammer-background", "hammer", "pattern", 0, 0, 0, None);
    bmp(f, "hammer-board", "hammer", "bk", 102, 59, 1, None);
    if phase == "ready" {
        bmp(f, "hammer-help", "hammer", "help", 133, 66, 100, None);
        start(f, "开始打地鼠");
        return;
    }
    if phase == "playing" {
        control(
            f,
            "hammer-field",
            "击打地鼠",
            Rect::new(0, 0, 640, 480),
            Action::Game(
                json!({"type":"minigameAction","action":"strike","x":ui.pointer.0.clamp(102,467),"y":ui.pointer.1.clamp(59,339)}),
            ),
            &[],
        );
        let control = f.controls.last_mut().unwrap();
        control.z = 999;
        control.on_press = Some(control.action.clone());
    }
    for (i, target) in a(st, "/targets").iter().enumerate() {
        let kind = n(target, "/kind");
        if n(target, "/phase") != 0 {
            bmp(
                f,
                &format!("hammer-target-{i}"),
                "hammer",
                &format!("mouse0{}", kind + 1),
                n(target, "/x"),
                n(target, "/y"),
                i as i32 * 2 + 2,
                Some(Rect::new(0, n(target, "/spriteY"), 124, 85)),
            );
        }
        if !target["impactUntil"].is_null() {
            bmp(
                f,
                &format!("hammer-impact-{i}"),
                "hammer",
                "bow",
                n(target, "/impactPosition/0"),
                n(target, "/impactPosition/1"),
                i as i32 * 2 + 3,
                None,
            );
        }
        if !target["scoreFrame"].is_null() {
            bmp(
                f,
                &format!("hammer-score-{i}"),
                "hammer",
                &format!("score0{}", kind + 1),
                n(target, "/x") + 35,
                n(target, "/y") - 20,
                30 + i as i32,
                Some(Rect::new(0, n(target, "/scoreFrame") % 2 * 25, 43, 25)),
            );
        }
        if phase == "playing" {
            action(
                f,
                &format!("hammer-hole-{i}"),
                &format!("击打第 {} 个洞", i + 1),
                Rect::new(n(target, "/x") + 25, n(target, "/y") + 12, 70, 55),
                "strikeIndex",
                Some(i),
                &[&format!("{}", i + 1)],
            );
            let control = f.controls.last_mut().unwrap();
            control.on_press = Some(control.action.clone());
        }
    }
    bmp(
        f,
        "hammer-pointer",
        "hammer",
        "ham",
        ui.pointer.0.clamp(102, 467),
        ui.pointer.1.clamp(59, 339),
        19,
        Some(Rect::new(
            0,
            if ui.pressed.is_some() { 68 } else { 0 },
            68,
            68,
        )),
    );
    f.requests.push(SceneRequest::CursorHidden(true));
    bmp(f, "hammer-goal", "hammer", "goal", 0, 172, 20, None);
    if phase != "reward" {
        bmp(f, "hammer-time", "hammer", "time", 293, 72, 40, None);
        for (key, x, digit) in [
            ("tens", 300, n(st, "/remainingSeconds") / 10),
            ("ones", 316, n(st, "/remainingSeconds") % 10),
        ] {
            bmp(
                f,
                key,
                "hammer",
                "fig",
                x,
                87,
                41,
                Some(Rect::new(digit * 16, 0, 16, 23)),
            );
        }
    }
    if phase == "lost" {
        let y = loss_y(n(st, "/lossFrame"));
        bmp(f, "hammer-lost", "hammer", "lost", 156, y, 100, None);
    }
    if phase == "won" {
        let i = flc(f, "hammer-win", "hammer", "win", 213, 179, 50, 0);
        f.sprites[i].playback = Some(Playback {
            interval_ms: 100,
            once: false,
            max_advances: 1,
            immediate: false,
        });
    }
    score(f, "hammer-score", n(st, "/score"), 490, 437);
    f.texts.last_mut().unwrap().font = TextFont::Gdi {
        size: 20,
        bold: true,
    };
    if phase == "reward" {
        bonus_at(f, "ham", &st["reward"], ui.scene_ms);
    }
}
fn loss_y(frame: i32) -> i32 {
    [
        -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100,
        -100, -100, -100, -100, -100, -50, 0, 50, 120, 200, 200, 120, 80, 120, 200, 200, 120, 200,
    ]
    .get(frame.max(0) as usize)
    .copied()
    .unwrap_or(200)
}
fn fist(f: &mut SceneFrame, st: &Value) {
    let phase = s(st, "/phase");
    bmp(f, "fist-background", "fistgame", "fistbg", 0, 0, 0, None);
    bmp(f, "fist-board", "fistgame", "back", 101, 51, 1, None);
    if phase != "ready" {
        let i = flc(
            f,
            "fist-target",
            "fistgame",
            &format!("boss0{}", n(st, "/targetState") / 2 + 1),
            n(st, "/targetX"),
            145,
            2,
            n(st, "/targetFrame").max(0) as u32,
        );
        f.sprites[i].clip = Some(Rect::new(101, 51, 433, 348));
        if n(st, "/targetState") == 6 && n(st, "/targetFrame") == 0 {
            bmp(
                f,
                "fist-hit",
                "fistgame",
                "ping",
                n(st, "/targetX") + 50,
                199,
                3,
                None,
            );
        }
        bmp(f, "fist-front", "fistgame", "back1", 101, 51, 4, None);
        if n(st, "/playerState") == 3 {
            flc(
                f,
                "fist-punch",
                "fistgame",
                "shut",
                n(st, "/playerX"),
                219,
                6,
                n(st, "/strikeFrame").max(0) as u32,
            );
        } else {
            bmp(
                f,
                "fist-gun",
                "fistgame",
                "gun0",
                n(st, "/playerX"),
                309,
                6,
                None,
            );
        }
        bmp(f, "fist-time", "fistgame", "time", 290, 56, 7, None);
        bmp(
            f,
            "fist-tens",
            "fistgame",
            &(n(st, "/remainingSeconds") / 10).to_string(),
            290,
            83,
            8,
            None,
        );
        bmp(
            f,
            "fist-ones",
            "fistgame",
            &(n(st, "/remainingSeconds") % 10).to_string(),
            316,
            83,
            8,
            None,
        );
        let value = n(st, "/score") as i16 as i32;
        score(
            f,
            "fist-score",
            value,
            527 - value.to_string().len() as i32 * 8,
            437,
        );
    }
    bmp(f, "fist-goal", "fistgame", "zhong", 0, 172, 5, None);
    if phase == "ready" || phase == "help" {
        bmp(f, "fist-help", "fistgame", "fisthelp", 133, 46, 200, None);
        start(
            f,
            if phase == "ready" {
                "开始拳击"
            } else {
                "继续拳击"
            },
        );
    }
    if phase == "won" && n(st, "/winFrame") % 2 == 0 {
        bmp(f, "fist-win", "fistgame", "win", 101, 197, 90, None);
    }
    if ["lost", "lostShake", "lostWait"].contains(&phase) {
        bmp(
            f,
            "fist-loss",
            "fistgame",
            "lost",
            154,
            n(st, "/lossY"),
            90,
            None,
        );
    }
    if phase == "lostShake" {
        let mut offsets = vec![];
        for bounce in 0..8 {
            offsets.push(0);
            let mut y = 12 - bounce * 12 / 8;
            let mut velocity = y - 2;
            while y > 0 {
                offsets.push(-y);
                y += velocity;
                velocity -= 2;
            }
        }
        f.offset = (
            0,
            offsets
                .get(n(st, "/lossShakeFrame").max(0) as usize)
                .copied()
                .unwrap_or(-2),
        );
    } else if phase == "lostWait" {
        f.offset = (0, -2);
    }
    if phase == "playing" {
        held(
            f,
            "fist-left",
            "向左移动",
            Rect::new(101, 260, 180, 147),
            "left",
            &["ArrowLeft"],
            st,
        );
        held(
            f,
            "fist-right",
            "向右移动",
            Rect::new(354, 260, 180, 147),
            "right",
            &["ArrowRight"],
            st,
        );
        control(
            f,
            "fist-fire",
            "出拳",
            Rect::new(281, 260, 73, 147),
            Action::Game(
                json!({"type":"minigameAction","action":"input","left":b(st,"/left"),"right":b(st,"/right"),"fire":true}),
            ),
            &["Space"],
        );
        action(
            f,
            "fist-help",
            "拳击说明",
            Rect::new(0, 172, 96, 140),
            "help",
            None,
            &["F1"],
        );
    }
    if phase == "reward" {
        bonus_at(f, "targ", &st["reward"], n(st, "/clockMs") as u64);
    }
    if ["ready", "help", "playing", "resultPause"].contains(&phase) {
        sound(f, "fist-music", "install/fistgame/gbsf11.wav", true, 0);
    }
}
fn gas(f: &mut SceneFrame, st: &Value, ui: &PresentationState) {
    let phase = s(st, "/phase");
    if phase == "lostShake" {
        if let Some(old) = &ui.runtime.gas_shake_scene {
            gas(f, old, ui);
        }
        let shake = [15, -15, 10, -10, 7, -7, 5, -5, 3, -3, 2, -2, 1, -1, 0, 0];
        f.offset = (
            0,
            15 + shake
                .get(n(st, "/shakeFrame").max(0) as usize)
                .copied()
                .unwrap_or(0),
        );
        return;
    }
    bmp(f, "gas-background", "gas", "bg01", 0, 0, 0, None);
    if phase == "reward" {
        bonus_at(f, "bear", &st["reward"], n(st, "/clockMs") as u64);
        return;
    }
    bmp(f, "gas-board", "gas", "gas01", 101, 51, 1, None);
    let an = &st["animations"];
    if phase == "ready" || phase == "help" {
        bmp(f, "gas-help", "gas", "bg01_1", 131, 51, 50, None);
        start(
            f,
            if phase == "ready" {
                "开始狗熊捕鱼"
            } else {
                "继续狗熊捕鱼"
            },
        );
    } else {
        let player = match phase {
            "playing" => Some(match n(st, "/playerState") {
                0 => 0,
                1 => 1,
                _ => 2,
            }),
            "hit" => Some(4),
            "miss" => Some(5),
            "won" | "wonWait" => Some(0),
            "lost" => Some(7),
            "lostFall" => Some(8),
            _ => None,
        };
        if let Some(player) = player {
            anim(f, "gas-player", &an[player], 2);
        }
        if phase == "playing" && n(st, "/playerState") == 2 {
            anim(f, "gas-smoke", &an[3], 3);
        }
        if phase == "won" || phase == "wonWait" {
            if phase == "won" && n(st, "/clockMs") - n(st, "/phaseLastTick") < 3000 {
                anim(f, "gas-victory", &an[6], 4);
            } else {
                bmp(f, "gas-victory", "gas", "gas03", 192, 137, 4, None);
            }
        }
        if ["playing", "miss", "lost", "lostFall"].contains(&phase)
            || phase == "hit" && n(st, "/animations/4/frame") < 6
        {
            let i = anim(
                f,
                "gas-target",
                &st["targets"][n(st, "/targetState").max(0) as usize],
                5,
            );
            f.sprites[i].clip = Some(Rect::new(101, 51, 433, 348));
        }
        bmp(f, "gas-front", "gas", "gas02", 349, 303, 6, None);
        if phase == "lostFall" {
            let i = bmp(f, "gas-fall", "gas", "gas04", 149, n(st, "/fallY"), 7, None);
            f.sprites[i].clip = Some(Rect::new(101, 51, 433, 348));
        }
        if phase == "playing" {
            action(
                f,
                "gas-throw",
                "捕鱼",
                Rect::new(0, 0, 640, 480),
                "throw",
                None,
                &["Space", "Enter"],
            );
            action(
                f,
                "gas-help",
                "捕鱼说明",
                Rect::new(0, 0, 0, 0),
                "help",
                None,
                &["F1"],
            );
        }
    }
    bmp(f, "gas-goal", "gas", "gou", 0, 169, 60, None);
    for i in 0..3 {
        if st["results"][i].as_bool() == Some(true) {
            anim(f, &format!("gas-fish-{i}"), &an[9 + i], 61);
        } else {
            bmp(
                f,
                &format!("gas-fish-{i}"),
                "gas",
                "fish1",
                213 + i as i32 * 76,
                417,
                61,
                None,
            );
            if i < (n(st, "/attempts").max(0) as usize) {
                bmp(
                    f,
                    &format!("gas-x-{i}"),
                    "gas",
                    "x",
                    219 + i as i32 * 76,
                    422,
                    62,
                    None,
                );
            }
        }
    }
    sound(f, "gas-music", "install/sound/gb/gbsb07.wav", true, 0);
    if phase == "ready" {
        sound(f, "gas-ready", "install/sound/gb/gbsa03.wav", true, 0);
    }
    if phase == "playing" && n(st, "/playerState") == 2 {
        sound(
            f,
            "gas-player-throw",
            "install/sound/gb/gbsb09.wav",
            true,
            0,
        );
    }
    if phase == "hit" && n(st, "/animations/4/frame") > 9 {
        sound(f, "gas-hit", "install/sound/gb/gbsb02.wav", true, 0);
    }
    if phase == "miss" && n(st, "/animations/5/frame") > 4 {
        sound(f, "gas-miss", "install/sound/gb/gbsb10.wav", true, 0);
    }
    if phase != "won" && phase != "wonWait" {
        f.stopped_sounds.push("install/sound/gb/gbsb06.wav".into());
    }
    if phase == "lostFall" {
        f.stopped_sounds.push("install/sound/gb/gbsb05.wav".into());
    }
    if phase == "playing" {
        sound(
            f,
            "gas-target-sound",
            format!(
                "install/sound/gb/{}.wav",
                if n(st, "/targetState") == 3 {
                    "gbsb04"
                } else {
                    "gbsb11"
                }
            ),
            true,
            0,
        );
    }
}
fn firebee(f: &mut SceneFrame, st: &Value) {
    let phase = s(st, "/phase");
    bmp(f, "firebee-background", "firebee", "bg06", 0, 0, 0, None);
    if phase == "reward" {
        bonus_at(f, "fly", &st["reward"], n(st, "/clockMs") as u64);
        return;
    }
    bmp(f, "firebee-board", "firebee", "back", 103, 66, 1, None);
    for (i, enemy) in a(st, "/drawnEnemies").iter().enumerate() {
        let row = n(enemy, "/row");
        let name = if row == 15 {
            "xma01".into()
        } else {
            format!("mos{:02}a", row + 1)
        };
        let k = flc(
            f,
            &format!("firebee-enemy-{i}"),
            "firebee",
            &name,
            n(enemy, "/x"),
            n(enemy, "/y"),
            2,
            n(enemy, "/frame").max(0) as u32,
        );
        f.sprites[k].clip = Some(Rect::new(103, 66, 433, 348));
    }
    for (key, name, z) in [
        ("drawnEnemyShots", "shut01", 3),
        ("drawnPlayerShots", "shut02", 4),
    ] {
        for (i, shot) in a(st, &format!("/{key}")).iter().enumerate() {
            bmp(
                f,
                &format!("{key}-{i}"),
                "firebee",
                name,
                n(shot, "/0"),
                n(shot, "/1"),
                z,
                None,
            );
        }
    }
    for (i, v) in a(st, "/drawnExplosions").iter().enumerate() {
        flc(
            f,
            &format!("firebee-explosion-{i}"),
            "firebee",
            "bomb",
            n(v, "/x"),
            n(v, "/y"),
            5,
            n(v, "/frame").max(0) as u32,
        );
    }
    bmp(
        f,
        "firebee-player",
        "firebee",
        ["plane", "plane1", "plane2"]
            .get(n(st, "/playerState").max(0) as usize)
            .copied()
            .unwrap_or("plane"),
        n(st, "/playerX"),
        350,
        6,
        None,
    );
    if b(st, "/failed") {
        flc(
            f,
            "firebee-death",
            "firebee",
            "jb",
            n(st, "/death/x"),
            n(st, "/death/y"),
            7,
            n(st, "/death/frame").max(0) as u32,
        );
    }
    score(f, "firebee-score", n(st, "/score"), 480, 432);
    if phase == "ready" || phase == "help" {
        bmp(f, "firebee-help", "firebee", "bg06_1", 131, 51, 8, None);
        start(
            f,
            if phase == "ready" {
                "开始射击"
            } else {
                "继续射击"
            },
        );
    } else if phase == "playing" {
        held(
            f,
            "firebee-left",
            "向左飞行",
            Rect::new(103, 66, 125, 348),
            "left",
            &["ArrowLeft"],
            st,
        );
        held(
            f,
            "firebee-right",
            "向右飞行",
            Rect::new(411, 66, 125, 348),
            "right",
            &["ArrowRight"],
            st,
        );
        control(
            f,
            "firebee-fire",
            "发射",
            Rect::new(228, 66, 183, 348),
            Action::Game(
                json!({"type":"minigameAction","action":"input","left":b(st,"/left"),"right":b(st,"/right"),"fire":true}),
            ),
            &["Space"],
        );
        action(
            f,
            "firebee-help",
            "射击说明",
            Rect::new(536, 66, 100, 50),
            "help",
            None,
            &["F1"],
        );
    }
    sound(f, "firebee-music", "install/sound/gb/gbsg01.wav", true, 0);
    sound(f, "firebee-engine", "install/sound/gb/gbsg09.wav", true, 0);
    for (i, v) in a(st, "/sounds").iter().enumerate() {
        sound(
            f,
            &format!("firebee-fx-{i}"),
            format!("install/sound/gb/gbsg0{}.wav", v.as_i64().unwrap_or(0)),
            false,
            n(st, "/clockMs") as u64,
        );
    }
}
fn fire(f: &mut SceneFrame, st: &Value) {
    let phase = s(st, "/phase");
    let shake = n(st, "/shakeOffset");
    if !b(st, "/lossAnimation") && !b(st, "/lossBanner") {
        bmp(f, "fire-background", "fire", "yao7", 0, shake, 0, None);
        bmp(f, "fire-board", "fire", "yao4", 104, 69 + shake, 1, None);
    }
    if b(st, "/lossBanner") {
        bmp(
            f,
            "fire-loss",
            "fire",
            "yao13",
            156,
            loss_y(n(st, "/lossFrame")),
            100,
            None,
        );
        return;
    }
    if b(st, "/lossAnimation") {
        flc(
            f,
            "fire-loss",
            "fire",
            "yao14",
            214,
            189,
            100,
            n(st, "/tracks/2/frame") as u32,
        );
        return;
    }
    let body = n(st, "/bodyState");
    let frame = n(st, "/bodyFrame");
    let body_frame = match body {
        0 => 0,
        1 => 2,
        2 => 1,
        3 => 3 + frame % 2,
        4 => 5 + frame % 2,
        _ => 7 + frame.min(3),
    };
    bmp(
        f,
        "fire-body",
        "fire",
        "yao3",
        254,
        284 + shake,
        2,
        Some(Rect::new(0, body_frame * 102, 73, 102)),
    );
    if phase == "ready" {
        bmp(f, "fire-help", "fire", "help", 133, 66, 100, None);
        start(f, "开始钻木取火");
        return;
    }
    bmp(f, "fire-goal", "fire", "zhuan", 0, 172, 1, None);
    let win = n(st, "/winStage");
    if win != 0 {
        flc(
            f,
            "fire-victory",
            "fire",
            if win == 1 { "1" } else { "2" },
            173,
            186,
            3,
            n(
                st,
                &format!("/tracks/{}/frame", if win == 1 { 0 } else { 1 }),
            ) as u32,
        );
    }
    let spark = match n(st, "/sparksState") {
        1 => Some(if n(st, "/sparksFrame") == 0 { 4 } else { 3 }),
        2 => Some(n(st, "/sparksFrame") % 2),
        3 => Some(2),
        _ => None,
    };
    if let Some(spark) = spark {
        bmp(
            f,
            "fire-sparks",
            "fire",
            "yao6",
            248,
            346,
            4,
            Some(Rect::new(spark * 34, 0, 34, 20)),
        );
    }
    bmp(
        f,
        "fire-actor",
        "fire",
        "yao5",
        n(st, "/actorX"),
        n(st, "/actorY"),
        5,
        Some(Rect::new(n(st, "/actorType") * 259, 0, 259, 160)),
    );
    bmp(f, "fire-meter", "fire", "yao8", 129, 269, 6, None);
    let lines = (67 * n(st, "/gauge") / 1000).max(0);
    if lines > 0 {
        bmp(
            f,
            "fire-gauge-head",
            "fire",
            "yao9",
            140,
            341 - lines,
            7,
            Some(Rect::new(0, 0, 4, 1)),
        );
        let i = bmp(
            f,
            "fire-gauge",
            "fire",
            "yao9",
            140,
            342 - lines,
            7,
            Some(Rect::new(0, 1, 4, 1)),
        );
        f.sprites[i].height = Some(lines as u32);
        f.sprites[i].repeat = true;
    }
    if !b(st, "/rewardStarted") {
        let elapsed = if win != 0 {
            n(st, "/wonAtElapsed")
        } else {
            n(st, "/elapsed")
        };
        let remaining = if b(st, "/failed") {
            0
        } else {
            ((n(st, "/duration") as u32).wrapping_sub(elapsed as u32) / 1000) % 100
        };
        bmp(f, "fire-time", "fire", "time", 298, 82, 8, None);
        for (id, x, digit) in [
            ("fire-tens", 305, remaining / 10),
            ("fire-ones", 321, remaining % 10),
        ] {
            bmp(
                f,
                id,
                "fire",
                "yao10",
                x,
                97,
                8,
                Some(Rect::new(digit as i32 * 16, 0, 16, 23)),
            );
        }
    }
    bmp(
        f,
        "fire-bottom",
        "fire",
        "yao7",
        0,
        416,
        9,
        Some(Rect::new(0, 416, 640, 64)),
    );
    if b(st, "/winBanner") && !b(st, "/rewardStarted") {
        flc(
            f,
            "fire-win",
            "fire",
            "yao13",
            214,
            189,
            10,
            n(st, "/tracks/3/frame") as u32,
        );
    }
    if !b(st, "/winBanner") {
        for (key, name, track, z) in [("intro", "yao01", 4, 11), ("randomOverlay", "yao02", 5, 12)]
        {
            if b(st, &format!("/{key}")) {
                flc(
                    f,
                    key,
                    "fire",
                    name,
                    429,
                    169,
                    z,
                    n(st, &format!("/tracks/{track}/frame")) as u32,
                );
            }
        }
    }
    if !b(st, "/failed") && b(st, "/lightningActive") && [0, 2].contains(&n(st, "/lightningFrame"))
    {
        f.color_scale = Some(([63, 254], [2.0; 3]));
    } else if win != 0 && b(st, "/fireFlash") {
        f.color_scale = Some(([63, 254], [2.5, 1.2, 0.5]));
    }
    if n(st, "/actorType") == 1 {
        let fade = (500 - (n(st, "/clockMs") - n(st, "/fadeAt"))).max(0) as f32 / 500.0;
        f.color_scale = Some(([0, 255], [fade; 3]));
    }
    if n(st, "/helpState") != 0 {
        bmp(
            f,
            "fire-help",
            "fire",
            "help",
            n(st, "/helpX"),
            n(st, "/helpY"),
            200,
            None,
        );
    }
    if b(st, "/rewardStarted") {
        bonus_at(f, "fire", &st["reward"], n(st, "/clockMs") as u64);
    }
    for (side, x, key, release, label) in [
        ("left", 104, "ArrowLeft", "releaseLeft", "左手生火"),
        ("right", 320, "ArrowRight", "releaseRight", "右手生火"),
    ] {
        action(
            f,
            &format!("fire-{side}"),
            label,
            Rect::new(x, 69, 216, 347),
            side,
            None,
            &[key],
        );
        let ctrl = f.controls.last_mut().unwrap();
        ctrl.on_press = Some(game("minigameAction", side, None));
        ctrl.on_release = Some(game("minigameAction", release, None));
    }
    if win == 2 {
        sound(f, "fire-burning", "install/fire/sound/gbse07.wav", true, 0);
    }
}
fn slots(f: &mut SceneFrame, c: &SceneContent<'_>, st: &Value) {
    let spirit = n(st, "/spirit");
    let name = if spirit == 2 {
        if b(st, "/positiveMoney") { "l1" } else { "l2" }
    } else if spirit == 3 {
        "l4"
    } else {
        "l3"
    };
    sprite(
        f,
        "spirit-panel",
        c.resolve(&format!("spirit/{name}.bmp")),
        191,
        141,
        200,
    );
    if spirit != 2 {
        let i = sprite(
            f,
            "spirit-attribute",
            c.resolve("spirit/l6.bmp"),
            216,
            186,
            201,
        );
        f.sprites[i].source = Some(Rect::new(0, n(st, "/attribute/value") * 21, 49, 21));
    }
    for index in if spirit == 2 { 0 } else { 1 }..4 {
        let i = sprite(
            f,
            &format!("spirit-digit-{index}"),
            c.resolve("spirit/l5.bmp"),
            303 + index * 32,
            185,
            201,
        );
        f.sprites[i].source = Some(Rect::new(
            0,
            n(st, &format!("/digits/{index}/value")) * 21,
            11,
            21,
        ));
    }
    if s(st, "/phase") == "spinning" {
        action(
            f,
            "spirit-stop",
            "停止神灵转盘",
            Rect::new(0, 0, 640, 480),
            "stop",
            None,
            &["Enter", "Space", "Escape"],
        );
    }
}
