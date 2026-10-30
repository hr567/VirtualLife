use super::*;

/// Called exactly once per visible, unpaused application frame, before composition.
/// Frame-based original animations advance once; only tracking's FLIC player
/// retains its explicit maximum-four catch-up loop.
pub fn update_presentation(
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    elapsed_ms: u32,
) {
    ui.runtime.frame_number += 1;
    super::preload::update(c, snapshot, ui);
    let key = format!("{:?}:{}", ui.screen, s(&ui.profile, "/_optionsPage"));
    if ui.runtime.last_key != key {
        ui.runtime.last_key = key;
        ui.runtime.options_elapsed = 0;
        ui.runtime.options_frames = [0, 0];
        ui.runtime.options_first = true;
        ui.runtime.overview_elapsed = 0;
        ui.runtime.overview_frame = 0;
        ui.runtime.overview_color = 0;
    }
    if ui.screen == Screen::Options {
        ui.runtime.options_elapsed += u64::from(elapsed_ms);
        if ui.runtime.options_elapsed > 114 {
            ui.runtime.options_elapsed = 0;
            if ui.runtime.options_first {
                ui.runtime.options_first = false
            } else {
                ui.runtime.options_frames = [
                    (ui.runtime.options_frames[0] + 1) % 6,
                    (ui.runtime.options_frames[1] + 1) % 5,
                ];
            }
        }
    }
    if ui.screen == Screen::MapOverview {
        ui.runtime.overview_elapsed += u64::from(elapsed_ms);
        if ui.runtime.overview_elapsed > 100 {
            ui.runtime.overview_elapsed = 0;
            ui.runtime.overview_frame += 1;
            ui.runtime.overview_color = (ui.runtime.overview_color + 1) % 5;
        }
    }
    if ui.screen == Screen::Game && ["dialogue", "choice"].contains(&s(snapshot, "/pending/kind")) {
        let scene = &snapshot["scene"];
        let pattern = c.resolve(s(scene, "/patternResource"));
        let key = format!(
            "{pattern}:{}:{}",
            n(scene, "/patternScrollMode"),
            s(scene, "/backgroundResource")
        );
        let p = &mut ui.runtime.pattern;
        if p.key != key {
            *p = PatternAnimationState {
                key,
                ..Default::default()
            };
        } else {
            p.elapsed += u64::from(elapsed_ms);
            if p.elapsed > 250 {
                p.elapsed = 0;
                let mode = n(scene, "/patternScrollMode");
                if mode == 3 || mode == 4 {
                    p.x -= if mode == 4 { p.direction } else { 1 };
                }
                if mode == 1 || mode == 4 {
                    p.y += 1
                } else if mode == 2 || mode == 3 {
                    p.y -= 1
                }
                p.x %= 80;
                p.y %= 80;
                p.counter = (p.counter + 1) % 3;
                if p.counter == 0 && mode == 4 {
                    p.direction *= -1;
                }
            }
        }
    } else {
        ui.runtime.pattern = PatternAnimationState::default();
    }
    let round = n(snapshot, "/pending/state/round");
    if s(snapshot, "/pending/kind") == "relationship"
        && s(snapshot, "/pending/state/phase") == "question"
    {
        let key = format!(
            "{}:{}",
            snapshot["pending"]["state"]["selectedPerson"],
            snapshot["pending"]["state"]["questionIndex"]
        );
        if ui.runtime.relationship_question != key {
            ui.runtime.relationship_question = key;
            ui.profile["_relationshipAnswer"] = json!(0);
        }
    } else {
        ui.runtime.relationship_question.clear();
    }
    if s(snapshot, "/pending/game") == "gas"
        && s(snapshot, "/pending/state/phase") == "lostShake"
        && s(&ui.runtime.previous_snapshot, "/pending/state/phase") != "lostShake"
    {
        ui.runtime.gas_shake_scene = Some(ui.runtime.previous_snapshot["pending"]["state"].clone());
    }
    if s(snapshot, "/pending/game") == "tracking" && ui.runtime.tracking_round != round {
        ui.runtime.tracking_round = round;
        ui.runtime.tracking_transformed = false;
        ui.runtime
            .animations
            .retain(|id, _| !id.starts_with("tracking-"));
    }
    if let Some(motion) = ui.runtime.motion.as_mut() {
        if motion.started {
            motion.cursor += 1
        } else {
            motion.started = true;
        }
        if motion.cursor >= motion.frames.len() {
            ui.runtime.motion = None;
        }
    }
    let map_snapshot = ui.runtime.motion.as_ref().map(|motion| {
        let mut map_snapshot = motion.snapshot.clone();
        if let Some(pose) = motion.frames.get(motion.cursor) {
            map_snapshot["_pauseSpirits"] = json!(!pose.walking);
            map_snapshot["_pauseDecorations"] = json!(pose.die.is_some() && motion.next_pending);
            if let Some((x, y)) = pose.actor_position {
                map_snapshot["_actorPosition"] = json!([x, y]);
            }
        }
        map_snapshot
    });
    update_map(c, map_snapshot.as_ref().unwrap_or(snapshot), ui, elapsed_ms);
    let frame = super::compose(c, snapshot, ui);
    let mut alive = std::collections::BTreeSet::new();
    for sprite in frame.sprites {
        let Some(playback) = sprite.playback else {
            continue;
        };
        alive.insert(sprite.id.clone());
        let clock = ui.runtime.animations.entry(sprite.id).or_default();
        if clock.resource != sprite.resource_id {
            *clock = AnimationClock {
                resource: sprite.resource_id.clone(),
                initial: true,
                ..Default::default()
            };
        }
        let frames = sprite
            .frame_count
            .unwrap_or_else(|| n(&c.media(&sprite.resource_id), "/frameCount").max(1) as u32);
        if clock.finished {
            continue;
        }
        clock.elapsed_ms = clock.elapsed_ms.saturating_add(u64::from(elapsed_ms));
        let interval = u64::from(playback.interval_ms.max(1));
        let max = playback.max_advances.max(1);
        let mut advances = 0;
        if clock.initial && playback.immediate {
            clock.initial = false;
            clock.elapsed_ms = 0;
            continue;
        }
        while clock.elapsed_ms > interval && advances < max {
            clock.elapsed_ms = if max == 1 {
                0
            } else {
                clock.elapsed_ms - interval
            };
            clock.initial = false;
            advances += 1;
            if playback.once && clock.frame + 1 >= frames {
                clock.finished = true;
                break;
            }
            clock.frame = (clock.frame + 1) % frames;
            if playback.once && clock.frame + 1 == frames {
                clock.finished = true;
            }
        }
        if clock.finished && sprite.resource_id == "install/gbs/gbu12.flc" {
            ui.runtime.tracking_transformed = true;
        }
    }
    ui.runtime.animations.retain(|id, _| alive.contains(id));
    update_sounds(c, snapshot, ui, elapsed_ms);
    if !crate::scene::same_json_representation(&ui.runtime.previous_snapshot, snapshot) {
        ui.runtime.previous_snapshot = snapshot.clone();
    }
}

#[cfg(test)]
mod map_snapshot_tests {
    use super::*;

    #[test]
    fn previous_snapshot_preserves_nested_signed_zero_without_a_sequence_change() {
        let c = SceneContent {
            manifest: &Value::Null,
            maps: &Value::Null,
            rules: &Value::Null,
        };
        let mut ui = PresentationState::default();
        let mut snapshot = json!({
            "lastSeq":7,"revision":3,
            "pending":{"state":{"values":[{"value":0.0}]}}
        });
        update_presentation(&c, &snapshot, &mut ui, 16);
        snapshot["pending"]["state"]["values"][0]["value"] = json!(-0.0);
        assert_eq!(ui.runtime.previous_snapshot, snapshot);
        assert_ne!(
            ui.runtime.previous_snapshot.to_string(),
            snapshot.to_string()
        );
        update_presentation(&c, &snapshot, &mut ui, 16);
        assert_eq!(
            serde_json::to_vec(&ui.runtime.previous_snapshot).unwrap(),
            serde_json::to_vec(&snapshot).unwrap()
        );
        assert_eq!(ui.runtime.previous_snapshot["lastSeq"], 7);
        assert_eq!(ui.runtime.previous_snapshot["revision"], 3);
    }

    #[test]
    fn map_animation_pauses_for_dice_uses_walk_position_and_resumes_at_rest() {
        let manifest = json!({"resources":{"cd/data/map/test/art.flc":{"sha256":"a"}},"objects":[{"sha256":"a","media":{"frames":5,"width":20,"height":20,"speedRaw":100}}]});
        let maps = json!({"maps":[{"id":"test","sourceResource":"cd/data/map/test/test.mmi","chips":[{"row":0,"column":0}],"chipOrigin":{"x":320,"y":245},"chipSize":{"width":0,"height":0},"legacySections":{"Spirit_0":{"File":"art.flc","Pos":{"signed":[10,10]},"Speed":{"signed":[10]},"Repeat":{"signed":[1]}}}}]});
        let c = SceneContent {
            manifest: &manifest,
            maps: &maps,
            rules: &Value::Null,
        };
        let snapshot = json!({"mapId":"test","tileId":0,"pending":null,"mapDecorations":{"loadSerial":1,"delaysMs":[5000]}});
        let original = snapshot.clone();
        let mut ui = PresentationState {
            screen: Screen::Game,
            ..Default::default()
        };
        update_presentation(&c, &snapshot, &mut ui, 16);
        update_presentation(&c, &snapshot, &mut ui, 101);
        assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 1);
        assert_eq!(ui.runtime.map.spirit_frame, 1);
        assert!(ui.runtime.map.cannon_visible);

        ui.runtime.motion = Some(MotionSequence {
            snapshot: snapshot.clone(),
            frames: vec![
                MotionPose {
                    die: Some(("install/ani/pm06a17.flc".into(), 0)),
                    ..Default::default()
                },
                MotionPose {
                    actor_position: Some((2000, 2000)),
                    walking: true,
                    ..Default::default()
                },
                MotionPose {
                    actor_position: Some((320, 245)),
                    walking: true,
                    ..Default::default()
                },
            ],
            next_pending: true,
            ..Default::default()
        });
        update_presentation(&c, &snapshot, &mut ui, 101);
        assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 1);
        assert_eq!(ui.runtime.map.spirit_frame, 1);
        assert!(!ui.runtime.map.cannon_visible);
        update_presentation(&c, &snapshot, &mut ui, 101);
        assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 1);
        assert_eq!(ui.runtime.map.spirit_frame, 2);
        update_presentation(&c, &snapshot, &mut ui, 101);
        assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 2);
        assert_eq!(ui.runtime.map.spirit_frame, 3);
        assert_eq!(ui.runtime.motion.as_ref().unwrap().snapshot, original);
        update_presentation(&c, &snapshot, &mut ui, 101);
        assert!(ui.runtime.motion.is_none());
        assert!(ui.runtime.map.cannon_visible);
        assert_eq!(ui.runtime.map.decorations[0].displayed_frame, 3);
        assert_eq!(ui.runtime.map.spirit_frame, 4);
        assert_eq!(snapshot, original);
        assert_eq!(ui.runtime.previous_snapshot, original);
    }
}

/// Preserve the original per-display-frame dice, ten-frame turns and
/// twenty-four integer movement increments before revealing the next scene.
pub fn accept_response(
    c: &SceneContent<'_>,
    previous: &Value,
    next: &Value,
    events: &[Value],
    ui: &mut PresentationState,
) {
    if let Some(answer) = events
        .iter()
        .find(|event| s(event, "/type") == "examAnswer")
    {
        ui.profile["_schoolAnswerEvent"] = json!({"revision":next["revision"],"answer":answer,"previousView":previous["nativeView"]});
    }
    if events
        .iter()
        .any(|event| ["newGameStarted", "reincarnated"].contains(&s(event, "/type")))
    {
        ui.runtime.motion = None;
        ui.allocations = json!({"intelligence":0,"morality":0,"charm":0,"stamina":0});
        for key in [
            "_allocation",
            "_nativeSchool",
            "_schoolAnswerEvent",
            "_nativePanel",
            "_nativeFortune",
            "_nativeCuePhase",
            "_schoolAudioPhase",
            "_schoolAudioSelection",
            "_housingAudioPhase",
        ] {
            ui.profile[key] = Value::Null;
        }
    }
    if previous["mapId"] != next["mapId"]
        || events
            .iter()
            .any(|event| ["newGameStarted", "reincarnated"].contains(&s(event, "/type")))
    {
        ui.camera = (0, 0);
        ui.profile["_cameraManual"] = json!(false);
    }
    if ui.screen != Screen::Game || previous["player"].is_null() {
        return;
    }
    let Some(map) = a(c.maps, "/maps")
        .iter()
        .find(|v| v["id"] == previous["mapId"])
    else {
        return;
    };
    let mut sequence = MotionSequence {
        snapshot: previous.clone(),
        next_pending: !next["pending"].is_null(),
        ..Default::default()
    };
    let mut facing = n(previous, "/mapFacing").clamp(0, 3) as u8;
    if let Some(die) = events
        .iter()
        .find(|event| s(event, "/type") == "diceRolled")
    {
        let rolled = n(die, "/rolled");
        if (1..=8).contains(&rolled) {
            let resource = format!("install/ani/pm06a{}.flc", rolled + 14);
            let count = n(&c.media(&resource), "/frameCount").max(1) as u32;
            for frame in 0..count {
                sequence.frames.push(MotionPose {
                    die: Some((resource.clone(), frame)),
                    facing,
                    ..Default::default()
                });
            }
        }
    }
    let character = (if s(previous, "/player/sex") == "female" {
        31
    } else {
        0
    }) + n(previous, "/life/stage")
        + n(previous, "/life/profession").max(0);
    let directory = c.rules["characters"]["directories"][character.max(0) as usize]
        .as_str()
        .unwrap_or("");
    let model = directory
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("m00");
    let resource = |direction: usize| {
        let suffix = c.rules["characters"]["animationSuffixes"][direction]
            .as_str()
            .unwrap_or("");
        c.resolve(&format!(
            "{directory}{}",
            suffix.replacen("M00", model, 1).replacen("m00", model, 1)
        ))
    };
    let directions = [(-1, 1), (-1, -1), (1, -1), (1, 1)];
    let turns: [[Option<usize>; 4]; 4] = [
        [Some(0), Some(8), None, Some(4)],
        [Some(5), Some(1), Some(9), None],
        [None, Some(6), Some(2), Some(10)],
        [Some(11), None, Some(7), Some(3)],
    ];
    for movement in events.iter().filter(|event| s(event, "/type") == "moved") {
        let mut previous_tile = n(movement, "/fromTileId").max(0) as usize;
        let mut position = super::map::world(map, previous_tile);
        for tile in a(movement, "/tiles") {
            let tile = tile.as_u64().unwrap_or(0) as usize;
            let from = &map["chips"][previous_tile];
            let to = &map["chips"][tile];
            let delta = (
                n(to, "/column") - n(from, "/column"),
                n(to, "/row") - n(from, "/row"),
            );
            let Some(direction) = directions.iter().position(|value| *value == delta) else {
                continue;
            };
            if let Some(turn) = turns[direction][facing as usize] {
                if turn > 3 {
                    let actor_resource = resource(turn);
                    let count = n(&c.media(&actor_resource), "/frameCount").max(1) as u32;
                    for frame in 0..10 {
                        sequence.frames.push(MotionPose {
                            actor_resource: Some(actor_resource.clone()),
                            actor_position: Some(position),
                            actor_frame: frame % count,
                            facing,
                            ..Default::default()
                        });
                    }
                }
            }
            facing = direction as u8;
            let actor_resource = resource(direction);
            let count = n(&c.media(&actor_resource), "/frameCount").max(1) as u32;
            let origin = position;
            for frame in 0..24 {
                position = (
                    origin.0 + delta.0.signum() * (n(map, "/chipSize/width") * (frame + 1) / 48),
                    origin.1 + delta.1.signum() * (n(map, "/chipSize/height") * (frame + 1) / 48),
                );
                sequence.frames.push(MotionPose {
                    actor_resource: Some(actor_resource.clone()),
                    actor_position: Some(position),
                    actor_frame: frame as u32 % count,
                    facing,
                    walking: true,
                    ..Default::default()
                });
            }
            previous_tile = tile;
        }
    }
    if !sequence.frames.is_empty() {
        ui.runtime.motion = Some(sequence);
        ui.runtime.map.cannon_frame = 0;
        ui.runtime.map.cannon_elapsed = 0;
        ui.runtime.map.cannon_animating = false;
        ui.runtime.map.cannon_wrapped = false;
        ui.profile["_diceChoices"] = json!(false);
    }
}

fn field(v: &Value, name: &str, index: usize) -> i32 {
    v[name]["signed"][index]
        .as_i64()
        .or_else(|| v[name]["bytes"][index].as_i64())
        .unwrap_or(0) as i32
}
fn update_map(c: &SceneContent<'_>, snapshot: &Value, ui: &mut PresentationState, dt: u32) {
    let Some(map) = a(c.maps, "/maps")
        .iter()
        .find(|v| v["id"] == snapshot["mapId"])
    else {
        return;
    };
    let in_motion = ui.runtime.motion.is_some();
    let state = &mut ui.runtime.map;
    let map_id = s(map, "/id");
    let load_serial = snapshot["mapDecorations"]["loadSerial"]
        .as_i64()
        .unwrap_or(-1);
    if state.map_id != map_id || state.load_serial != load_serial {
        *state = MapAnimationState {
            map_id: map_id.into(),
            load_serial,
            ..Default::default()
        };
        let directory = s(map, "/sourceResource")
            .rsplit_once('/')
            .map(|(v, _)| v)
            .unwrap_or("");
        if let Some(sections) = map["legacySections"].as_object() {
            for (name, v) in sections {
                if name.starts_with("Spirit_") || name.starts_with("Floater_") {
                    let foreground = name.starts_with("Floater_");
                    let resource = c.resolve(&format!("{directory}/{}", s(v, "/File")));
                    let meta = c.media(&resource);
                    let index = name
                        .rsplit_once('_')
                        .and_then(|(_, v)| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    state.decorations.push(MapDecoration {
                        id: format!("map-decoration-{name}"),
                        resource,
                        position: (field(v, "Pos", 0), field(v, "Pos", 1)),
                        dimensions: (n(&meta, "/width"), n(&meta, "/height")),
                        foreground,
                        frames: n(&meta, "/frameCount").max(1) as u32,
                        interval: if foreground {
                            u64::MAX
                        } else {
                            1000 / field(v, "Speed", 0).max(1) as u64
                        },
                        repeat: field(v, "Repeat", 0) == 1,
                        delay: snapshot["mapDecorations"]["delaysMs"][index]
                            .as_u64()
                            .unwrap_or(u64::MAX),
                        initial: true,
                        playing: true,
                        ..Default::default()
                    });
                } else if name.starts_with("PalCycling_") {
                    let first = field(v, "FirstColor", 0).clamp(0, 255) as u8;
                    let last = field(v, "LastColor", 0).clamp(0, 255) as u8;
                    state.cycles.push(PaletteCycle {
                        first,
                        last,
                        kind: field(v, "Type", 0) as u8,
                        interval: 1000 / field(v, "Speed", 0).max(1) as u64,
                        position: first,
                        direction: 1,
                        mapping: (0..=255).collect(),
                        ..Default::default()
                    });
                }
            }
        }
    }
    let pending = !snapshot["pending"].is_null();
    let map_active = ui.screen == Screen::Game && !pending;
    if !map_active {
        state.cannon_visible = false;
        return;
    }
    if in_motion {
        state.cannon_visible = false;
    } else if !state.cannon_visible {
        state.cannon_visible = true;
        state.cannon_frame = 0;
        state.cannon_elapsed = 0;
        state.cannon_animating = false;
        state.cannon_wrapped = false;
    }
    state.clock = state.clock.saturating_add(u64::from(dt));
    let clock = state.clock;
    let actor = if snapshot["_actorPosition"].is_array() {
        (
            n(snapshot, "/_actorPosition/0"),
            n(snapshot, "/_actorPosition/1"),
        )
    } else {
        super::map::world(map, n(snapshot, "/tileId").max(0) as usize)
    };
    let (cx, cy) = if ui.camera == (0, 0) && !b(&ui.profile, "/_cameraManual") {
        (actor.0 - 320, actor.1 - 245)
    } else {
        ui.camera
    };
    for item in &mut state.decorations {
        if b(snapshot, "/_pauseDecorations") {
            continue;
        }
        if item.foreground {
            continue;
        }
        let (x, y) = (item.position.0 - cx, item.position.1 - cy);
        if x <= -item.dimensions.0 || y <= -item.dimensions.1 || x >= 640 || y >= 480 {
            continue;
        }
        let elapsed = item
            .last_tick
            .map(|last| clock.saturating_sub(last))
            .unwrap_or(u64::MAX);
        if item.repeat {
            if elapsed > item.interval {
                item.last_tick = Some(clock);
                if item.initial {
                    item.initial = false
                } else {
                    item.frame = (item.frame + 1) % item.frames;
                    item.displayed_frame = item.frame;
                }
            }
        } else if item.playing {
            if elapsed > item.interval {
                item.last_tick = Some(clock);
                if item.frame + 1 == item.frames {
                    item.playing = false;
                    item.frame = 0;
                } else {
                    item.frame += 1;
                    item.displayed_frame = item.frame;
                }
            }
        } else if elapsed > item.delay {
            item.last_tick = Some(clock);
            item.playing = true;
        }
    }
    for cycle in &mut state.cycles {
        if b(snapshot, "/_pauseDecorations") {
            continue;
        }
        if cycle
            .last_tick
            .map(|last| clock.saturating_sub(last) > cycle.interval)
            .unwrap_or(true)
        {
            cycle.last_tick = Some(clock);
            if cycle.first >= cycle.last {
                continue;
            }
            if cycle.kind == 0 {
                cycle.mapping[cycle.first as usize..=cycle.last as usize].rotate_right(1);
            } else if cycle.kind == 1 {
                cycle.position = (i16::from(cycle.position) + i16::from(cycle.direction)) as u8;
                if cycle.position == cycle.last || cycle.position == cycle.first {
                    cycle.direction *= -1;
                } else {
                    cycle.mapping[cycle.position as usize] = if cycle.direction == 1 {
                        cycle.first
                    } else {
                        cycle.position
                    };
                }
            }
        }
    }
    if b(snapshot, "/_pauseSpirits") {
        state.spirit_elapsed = 0;
    } else {
        state.spirit_elapsed += u64::from(dt);
        if state.spirit_elapsed > 100 {
            state.spirit_elapsed = 0;
            state.spirit_frame += 1;
        }
    }
    if in_motion {
        return;
    }
    state.cannon_elapsed += u64::from(dt);
    let interval = if state.cannon_animating { 100 } else { 5000 };
    if state.cannon_elapsed > interval {
        state.cannon_elapsed = 0;
        if !state.cannon_animating {
            state.cannon_animating = true;
        } else {
            let frames = n(&c.media("install/ani/pm06a14.flc"), "/frameCount").max(1) as u32;
            state.cannon_frame += 1;
            if state.cannon_frame == frames {
                state.cannon_frame = 0;
                state.cannon_animating = false;
                state.cannon_wrapped = true;
            } else {
                state.cannon_wrapped = false;
            }
        }
    }
}

fn advance_cues(c: &SceneContent<'_>, sounds: &mut Vec<SceneSound>, elapsed_ms: u32) {
    sounds.retain_mut(|sound| {
        let Some(duration) = c.manifest["_audioDurations"]
            .get(&sound.resource_id)
            .and_then(Value::as_u64)
        else {
            return true;
        };
        let position = sound
            .position_ms
            .unwrap_or(0)
            .saturating_add(u64::from(elapsed_ms));
        sound.position_ms = Some(position);
        position < duration
    });
}
fn ui_sounds(c: &SceneContent<'_>, ui: &mut PresentationState, elapsed_ms: u32) -> Vec<SceneSound> {
    let (old_ui, game): (Vec<_>, Vec<_>) = std::mem::take(&mut ui.runtime.one_shots)
        .into_iter()
        .partition(|sound| sound.id.starts_with("ui-"));
    ui.runtime.one_shots = game;
    let mut sounds = old_ui;
    if matches!(ui.screen, Screen::Menu | Screen::Profile | Screen::Game) {
        advance_cues(c, &mut sounds, elapsed_ms);
    }
    let meta = c.native_profile();
    let button = |id: &str| {
        a(meta, "/buttons")
            .iter()
            .any(|v| id == format!("profile-{}", s(v, "/id")))
    };
    let clickable = |id: &str| button(id) || ["profile-confirm", "profile-cancel"].contains(&id);
    let hover = ui
        .hover
        .as_deref()
        .filter(|id| ui.screen == Screen::Profile && button(id))
        .unwrap_or("")
        .to_owned();
    let pressed = ui
        .pressed
        .as_deref()
        .filter(|id| ui.screen == Screen::Profile && clickable(id))
        .unwrap_or("")
        .to_owned();
    let serial = ui.profile["_uiClickSerial"].as_u64().unwrap_or(0);
    let seen = ui.profile["_uiSoundClickSerial"].as_u64().unwrap_or(0);
    let click = serial != seen && clickable(s(&ui.profile, "/_uiClickControl"))
        || serial == seen && !pressed.is_empty() && pressed != s(&ui.profile, "/_uiSoundPressed");
    let mut add = |id: &str, resource: &str| {
        if resource.is_empty() {
            return;
        }
        sounds.retain(|sound| sound.id != id);
        sounds.push(SceneSound {
            id: id.into(),
            resource_id: c.resolve(resource),
            restart_serial: ui.runtime.frame_number,
            position_ms: Some(0),
            ..Default::default()
        });
    };
    if !hover.is_empty() && hover != s(&ui.profile, "/_uiSoundHover") {
        add("ui-profile-hover", s(meta, "/hoverSound"));
    }
    if click {
        add("ui-profile-click", s(meta, "/clickSound"));
    }
    ui.profile["_uiSoundHover"] = json!(hover);
    ui.profile["_uiSoundPressed"] = json!(pressed);
    ui.profile["_uiSoundClickSerial"] = json!(serial);
    sounds
}

fn update_sounds(
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    elapsed_ms: u32,
) {
    let ui_sounds = ui_sounds(c, ui, elapsed_ms);
    if matches!(ui.screen, Screen::Menu | Screen::Profile) {
        // UI clicks have their own visible clock. Entering the menu/profile
        // clears game sounds, while original button cues keep their WAV tail.
        ui.runtime.one_shots = ui_sounds;
        ui.runtime.competition_answer = None;
        return;
    }
    // The visible save/options overlay freezes the current scene's playback
    // position. It must not consume or re-trigger the edge that created a cue.
    if ui.screen != Screen::Game {
        ui.runtime.one_shots.extend(ui_sounds);
        return;
    }
    let previous = &ui.runtime.previous_snapshot;
    let old = &previous["pending"]["state"];
    let st = &snapshot["pending"]["state"];
    let phase = s(st, "/phase");
    let old_phase = s(old, "/phase");
    let game = s(snapshot, "/pending/game");
    let same_module = previous["pending"]["kind"] == snapshot["pending"]["kind"]
        && previous["pending"]["game"] == snapshot["pending"]["game"]
        && previous["nativeView"]["kind"] == snapshot["nativeView"]["kind"];
    let mut sounds = if same_module {
        std::mem::take(&mut ui.runtime.one_shots)
    } else {
        ui.runtime.one_shots.clear();
        Vec::new()
    };
    // Until verified WAV metadata arrives, playback remains parked at zero.
    advance_cues(c, &mut sounds, elapsed_ms);
    let serial = ui.runtime.frame_number;
    let mut add = |id: &str, path: String| {
        sounds.retain(|sound| sound.id != id);
        sounds.push(SceneSound {
            id: id.into(),
            resource_id: path,
            restart_serial: serial,
            position_ms: Some(0),
            ..Default::default()
        })
    };
    let native_phase = json!([
        ui.profile["_nativePanel"]["scope"],
        ui.profile["_nativePanel"]["phase"]
    ]);
    if s(snapshot, "/nativeView/kind") == "career"
        && s(&ui.profile, "/_nativePanel/phase") == "accepted"
        && ui.profile["_nativeCuePhase"] != native_phase
    {
        add(
            "native-career-selected",
            s(c.native_ui(), "/career/selectedSound").to_owned(),
        );
    }
    if s(snapshot, "/nativeView/kind") == "statShop"
        && s(previous, "/nativeView/kind") == "statShop"
        && n(snapshot, "/nativeView/purchases") > n(previous, "/nativeView/purchases")
    {
        add(
            "native-stat-buy",
            s(c.native_ui(), "/statShop/buySound").to_owned(),
        );
    }
    ui.profile["_nativeCuePhase"] = native_phase;
    if s(snapshot, "/nativeView/kind") == "housing"
        && s(previous, "/nativeView/kind") == "housing"
        && !snapshot["nativeView"]["result"].is_null()
        && previous["nativeView"]["housing"] != snapshot["nativeView"]["housing"]
    {
        add(
            "native-housing-buy",
            s(c.native_housing(), "/buySound").to_owned(),
        );
    }
    let housing_phase = s(&ui.profile, "/_nativePanel/phase").to_owned();
    if s(snapshot, "/nativeView/kind") == "housing"
        && s(&ui.profile, "/_housingAudioPhase") == "gain"
        && housing_phase == "completed"
    {
        add(
            "native-house-acquired",
            s(c.native_housing(), "/houseSound").to_owned(),
        );
    }
    ui.profile["_housingAudioPhase"] = json!(housing_phase);
    let school_phase = s(&ui.profile, "/_nativeSchool/phase").to_owned();
    let old_school = s(&ui.profile, "/_schoolAudioPhase").to_owned();
    if school_phase != old_school {
        if school_phase == "feedback" {
            add(
                "school-answer-confirm",
                s(c.native_school(), "/sounds/confirm").to_owned(),
            );
            add(
                "school-answer-result",
                s(
                    c.native_school(),
                    if b(&ui.profile, "/_nativeSchool/correct") {
                        "/sounds/correct"
                    } else {
                        "/sounds/wrong"
                    },
                )
                .to_owned(),
            );
        }
        if school_phase == "resultOpening" {
            add(
                "school-result",
                s(
                    c.native_school(),
                    if n(&ui.profile, "/_nativeSchool/score") >= 60 {
                        "/sounds/win"
                    } else {
                        "/sounds/lost"
                    },
                )
                .to_owned(),
            );
        }
    }
    if school_phase == "question"
        && old_school == "question"
        && ui.profile["_schoolAudioSelection"] != ui.profile["_nativeSchool"]["selection"]
    {
        add(
            "school-selection",
            s(c.native_school(), "/sounds/select").to_owned(),
        );
    }
    ui.profile["_schoolAudioPhase"] = json!(school_phase);
    ui.profile["_schoolAudioSelection"] = ui.profile["_nativeSchool"]["selection"].clone();
    if s(snapshot, "/pending/kind") == "competition"
        && s(previous, "/pending/kind") == "competition"
    {
        if old_phase == "menu" && phase != "menu" {
            add("competition-button", "install/sound/wav/botten.wav".into());
        }
        if old_phase == "quizQuestion" && phase == "quizFeedback" {
            add(
                "competition-answer-confirm",
                "install/sound/a0/a0s04.wav".into(),
            );
        }
        let answer = n(&ui.profile, "/_competitionAnswer");
        if phase == "quizQuestion"
            && old_phase == "quizQuestion"
            && ui
                .runtime
                .competition_answer
                .is_some_and(|old| old != answer)
        {
            add(
                "competition-answer-select",
                "install/sound/a0/a0s03.wav".into(),
            );
        }
        ui.runtime.competition_answer = Some(answer);
    } else {
        ui.runtime.competition_answer = None;
    }
    if previous["pending"]["game"] == snapshot["pending"]["game"]
        && previous["pending"]["kind"] == snapshot["pending"]["kind"]
    {
        match game {
            "hammer" => {
                if old_phase == "playing" {
                    for (i, target) in a(st, "/targets").iter().enumerate() {
                        if n(target, "/hits") > n(&old["targets"][i], "/hits") {
                            let name = if n(target, "/scoreFrame") == 0 {
                                if n(target, "/kind") == 2 {
                                    "gbsd06"
                                } else {
                                    "gbsd02"
                                }
                            } else {
                                "gbsd03"
                            };
                            add(
                                &format!("hammer-hit-{i}"),
                                format!("install/hammer/sound/{name}.wav"),
                            );
                        }
                    }
                }
            }
            "fist" => {
                if old_phase == "playing" && phase == "playing" {
                    if n(old, "/strikeFrame") == -1 && n(st, "/strikeFrame") == 0 {
                        add("fist-punch", "install/fistgame/gbsf02.wav".into());
                    }
                    if n(old, "/strikeFrame") == 5 && n(st, "/strikeFrame") == 6 {
                        add("fist-impact", "install/fistgame/gbsf03.wav".into());
                    }
                    if n(old, "/strikeFrame") == 6 && n(st, "/playerState") == 0 {
                        if n(st, "/targetState") == 6 {
                            add("fist-hit", "install/fistgame/gbsf04.wav".into());
                            add("fist-score", "install/fistgame/gbsf05.wav".into());
                        } else {
                            add("fist-miss", "install/fistgame/gbsf06.wav".into());
                        }
                    }
                }
            }
            "gas" => {
                if (phase == "won" || phase == "wonWait")
                    && old_phase != "won"
                    && old_phase != "wonWait"
                {
                    add("gas-won", "install/sound/gb/gbsb06.wav".into());
                }
                if phase == "lost"
                    && n(st, "/animations/7/frame") == 14
                    && n(old, "/animations/7/frame") != 14
                {
                    add("gas-loss", "install/sound/gb/gbsb05.wav".into());
                }
            }
            "fire" => {
                for (key, name) in [
                    ("hand", "gbse01"),
                    ("shakeActive", "gbse04"),
                    ("lossAnimation", "gbse08"),
                ] {
                    let changed = if key == "hand" {
                        st[key] != old[key]
                    } else {
                        b(st, &format!("/{key}")) && !b(old, &format!("/{key}"))
                    };
                    if changed {
                        add(key, format!("install/fire/sound/{name}.wav"));
                    }
                }
                if b(st, "/lightningActive")
                    && !b(old, "/lightningActive")
                    && n(st, "/lightningSound") > 0
                {
                    add(
                        "fire-lightning",
                        format!("install/fire/sound/gbse0{}.wav", n(st, "/lightningSound")),
                    );
                }
            }
            _ => {}
        }
        let old_reward = &old["reward"];
        let reward = &st["reward"];
        if b(reward, "/visible")
            && !old_reward.is_null()
            && reward["increment"] != old_reward["increment"]
        {
            add("bonus-counter", "install/public/bonus/bonus.wav".into());
        }
    }
    sounds.extend(ui_sounds);
    ui.runtime.one_shots = sounds;
}

#[cfg(test)]
mod sound_tests {
    use super::*;
    use std::path::PathBuf;
    fn original_punch() -> (Value, u64) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("assets/manifest.json")).unwrap())
                .unwrap();
        let resource = "install/fistgame/gbsf02.wav";
        let hash = &manifest["resources"][resource]["sha256"];
        let object = manifest["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["sha256"] == *hash)
            .unwrap();
        let bytes = std::fs::read(root.join(object["path"].as_str().unwrap())).unwrap();
        let wave = crate::audio::WaveInfo::parse(&bytes).unwrap();
        let duration = wave.duration();
        let milliseconds =
            duration.as_millis() as u64 + u64::from(duration.subsec_nanos() % 1_000_000 != 0);
        assert!(
            milliseconds > 32,
            "The real punch WAV must span multiple displayed frames"
        );
        (
            json!({"_audioDurations":{(resource):milliseconds}}),
            milliseconds,
        )
    }
    fn state(frame: i32) -> Value {
        json!({"pending":{"kind":"minigame","game":"fist","state":{"phase":"playing","strikeFrame":frame,"playerState":0}}})
    }
    fn playing() -> PresentationState {
        let mut ui = PresentationState::default();
        ui.screen = Screen::Game;
        ui.runtime.frame_number = 1;
        ui.runtime.previous_snapshot = state(-1);
        ui
    }
    #[test]
    fn original_profile_cues_keep_real_wav_lifetimes_and_confirm_survives_entering_game() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("assets/manifest.json")).unwrap())
                .unwrap();
        let profile: Value = serde_json::from_slice(
            &std::fs::read(root.join("assets/catalog/native-profile.json")).unwrap(),
        )
        .unwrap();
        let mut durations = serde_json::Map::new();
        for key in ["hoverSound", "clickSound"] {
            let resource = s(&profile, &format!("/{key}"));
            let sha = &manifest["resources"][resource]["sha256"];
            let object = a(&manifest, "/objects")
                .iter()
                .find(|v| v["sha256"] == *sha)
                .unwrap();
            let bytes = std::fs::read(root.join(s(object, "/path"))).unwrap();
            let d = crate::audio::WaveInfo::parse(&bytes).unwrap().duration();
            let duration = d.as_millis() as u64 + u64::from(d.subsec_nanos() % 1_000_000 != 0);
            assert!(duration > 16);
            durations.insert(resource.into(), json!(duration));
        }
        manifest["_nativeProfile"] = profile;
        manifest["_audioDurations"] = Value::Object(durations);
        let c = SceneContent {
            manifest: &manifest,
            maps: &Value::Null,
            rules: &Value::Null,
        };
        let mut ui = PresentationState {
            screen: Screen::Profile,
            hover: Some("profile-name".into()),
            ..Default::default()
        };
        ui.runtime.frame_number = 1;
        update_sounds(&c, &Value::Null, &mut ui, 16);
        let hover = ui.runtime.one_shots[0].clone();
        assert_eq!(hover.id, "ui-profile-hover");
        assert_eq!(hover.position_ms, Some(0));
        update_sounds(&c, &Value::Null, &mut ui, 16);
        assert_eq!(ui.runtime.one_shots[0].position_ms, Some(16));
        assert_eq!(ui.runtime.one_shots[0].restart_serial, hover.restart_serial);
        ui.hover = Some("profile-confirm".into());
        update_sounds(&c, &Value::Null, &mut ui, 1);
        assert_eq!(
            ui.runtime.one_shots[0].restart_serial, hover.restart_serial,
            "Original confirm has no nine-button hover cue"
        );
        // A down/up inside one physical Bevy frame leaves pressed=None. The
        // source input serial must still create exactly one confirmation cue.
        ui.profile["_uiClickControl"] = json!("profile-confirm");
        ui.profile["_uiClickSerial"] = json!(1);
        ui.runtime.frame_number = 2;
        ui.screen = Screen::Game;
        update_sounds(&c, &state(0), &mut ui, 1);
        let click = ui
            .runtime
            .one_shots
            .iter()
            .find(|v| v.id == "ui-profile-click")
            .unwrap()
            .clone();
        assert_eq!(click.position_ms, Some(0));
        let mut restored: PresentationState =
            serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
        update_sounds(&c, &state(0), &mut restored, 16);
        assert_eq!(
            restored
                .runtime
                .one_shots
                .iter()
                .find(|v| v.id == click.id)
                .unwrap()
                .position_ms,
            Some(16)
        );
        restored.screen = Screen::Saves;
        update_sounds(&c, &state(0), &mut restored, 60_000);
        assert_eq!(
            restored
                .runtime
                .one_shots
                .iter()
                .find(|v| v.id == click.id)
                .unwrap()
                .position_ms,
            Some(16)
        );
        restored.screen = Screen::Menu;
        let duration = manifest["_audioDurations"][&click.resource_id]
            .as_u64()
            .unwrap();
        update_sounds(&c, &state(0), &mut restored, (duration - 17) as u32);
        assert!(restored.runtime.one_shots.iter().any(|v| v.id == click.id));
        update_sounds(&c, &state(0), &mut restored, 1);
        assert!(
            !restored.runtime.one_shots.iter().any(|v| v.id == click.id),
            "The original WAV must expire exactly at its verified duration"
        );
    }
    #[test]
    fn unchanged_revision_sound_edges_keep_the_verified_wav_lifetime() {
        let (manifest, duration) = original_punch();
        let empty = Value::Null;
        let c = SceneContent {
            manifest: &manifest,
            maps: &empty,
            rules: &empty,
        };
        let mut next = state(0);
        next["revision"] = json!(7);
        next["lastSeq"] = json!(12);
        let mut ui = playing();
        ui.runtime.previous_snapshot["revision"] = json!(7);
        ui.runtime.previous_snapshot["lastSeq"] = json!(12);
        update_presentation(&c, &next, &mut ui, 16);
        assert_eq!(ui.runtime.one_shots.len(), 1);
        assert_eq!(ui.runtime.one_shots[0].position_ms, Some(0));
        assert_eq!(ui.runtime.previous_snapshot, next);
        let serial = ui.runtime.one_shots[0].restart_serial;
        update_presentation(&c, &next, &mut ui, 16);
        assert_eq!(ui.runtime.one_shots[0].position_ms, Some(16));
        assert_eq!(ui.runtime.one_shots[0].restart_serial, serial);
        update_presentation(&c, &next, &mut ui, (duration - 17) as u32);
        assert_eq!(ui.runtime.one_shots[0].position_ms, Some(duration - 1));
        update_presentation(&c, &next, &mut ui, 1);
        assert!(ui.runtime.one_shots.is_empty());
    }
    #[test]
    fn save_options_freeze_cues_and_restored_cues_resume_without_retriggering() {
        let (manifest, _) = original_punch();
        let empty = Value::Null;
        let c = SceneContent {
            manifest: &manifest,
            maps: &empty,
            rules: &empty,
        };
        let next = state(0);
        let mut ui = playing();
        update_sounds(&c, &next, &mut ui, 16);
        ui.runtime.previous_snapshot = next.clone();
        for screen in [Screen::Saves, Screen::Options] {
            ui.screen = screen;
            update_sounds(&c, &next, &mut ui, 60000);
            assert_eq!(ui.runtime.one_shots[0].position_ms, Some(0));
        }
        let mut restored: PresentationState =
            serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
        restored.screen = Screen::Game;
        update_sounds(&c, &next, &mut restored, 17);
        assert_eq!(restored.runtime.one_shots[0].position_ms, Some(17));
        assert_eq!(restored.runtime.one_shots[0].restart_serial, 1);
        restored.runtime.previous_snapshot = state(-1);
        restored.runtime.frame_number = 2;
        update_sounds(&c, &next, &mut restored, 17);
        assert_eq!(restored.runtime.one_shots.len(), 1);
        assert_eq!(restored.runtime.one_shots[0].position_ms, Some(0));
        assert_eq!(restored.runtime.one_shots[0].restart_serial, 2);
        restored.runtime.previous_snapshot = next.clone();
        let other = json!({"pending":{"kind":"minigame","game":"hammer","state":{"phase":"help"}}});
        update_sounds(&c, &other, &mut restored, 1);
        assert!(restored.runtime.one_shots.is_empty());
        for screen in [Screen::Menu, Screen::Profile] {
            restored.runtime.previous_snapshot = state(-1);
            restored.screen = Screen::Game;
            update_sounds(&c, &next, &mut restored, 1);
            assert!(!restored.runtime.one_shots.is_empty());
            restored.screen = screen;
            update_sounds(&c, &next, &mut restored, 1);
            assert!(restored.runtime.one_shots.is_empty());
        }
    }
}
