//! Blocking original N1 transitions, map notices, payroll and opening movies.
//! Rules have already settled; these saved cues prevent later UI/ticks from
//! overtaking the original visible sequence. No cue consumes gameplay RNG.
use super::*;
use std::{collections::VecDeque, sync::OnceLock};

fn data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/catalog/native-events.json"
        ))
        .unwrap()
    })
}
pub fn blocking(ui: &PresentationState) -> bool {
    ui.runtime
        .sequence
        .as_ref()
        .is_some_and(|q| !q.cues.is_empty())
}
pub fn valid_sequence(ui: &PresentationState) -> bool {
    ui.runtime.sequence.as_ref().is_none_or(|sequence| {
        sequence.cues.len() <= 256
            && sequence.cues.iter().all(|cue| {
                if cue.elapsed_ms > 604_800_000 || cue.frame > 4096 {
                    return false;
                }
                match cue.kind.as_str() {
                    "background" => {
                        (1..=1024).contains(&n(&cue.data, "/frames"))
                            && cue.frame < n(&cue.data, "/frames") as u32
                    }
                    "portraits" => {
                        (1..=1024).contains(&a(&cue.data, "/positions").len())
                            && (cue.frame as usize) < a(&cue.data, "/positions").len()
                    }
                    "payroll" | "mastered" => {
                        (1..=4096).contains(&a(&cue.data, "/steps").len())
                            && (cue.frame as usize) < a(&cue.data, "/steps").len()
                            && (cue.kind == "mastered"
                                || (0..=2).contains(&n(&cue.data, "/mode"))
                                    && (-1..=24).contains(&n(&cue.data, "/career")))
                    }
                    "birthday" => s(&cue.data, "/resource").starts_with("install/bmp/"),
                    "lifeNotice" => (0..=1).contains(&n(&cue.data, "/index")),
                    "opening" => true,
                    "prologue" => {
                        cue.data["movie"].is_null()
                            || serde_json::from_value::<virtual_life_core::epilogue::EndingMovie>(
                                cue.data["movie"].clone(),
                            )
                            .is_ok_and(|movie| movie.segment < 2 && movie.subtitle_index <= 17)
                    }
                    _ => false,
                }
            })
    })
}
pub fn advance(ui: &mut PresentationState) {
    if let Some(q) = &mut ui.runtime.sequence {
        if q.cues
            .front()
            .is_some_and(|cue| ["opening", "prologue"].contains(&cue.kind.as_str()))
        {
            q.cues.clear();
        } else {
            q.cues.pop_front();
        }
        if q.cues.is_empty() {
            ui.runtime.sequence = None;
        }
    }
}
pub fn start_movie(ui: &mut PresentationState, kind: &str, snapshot: Value) {
    ui.runtime.sequence = Some(NativeSequence {
        snapshot,
        cues: VecDeque::from([NativeCue {
            kind: kind.into(),
            serial: ui.runtime.frame_number,
            ..Default::default()
        }]),
    });
}

fn payroll_steps(c: &SceneContent<'_>, mode: usize) -> Vec<Value> {
    let r = &data()["payroll"];
    let panel = c.media(s(&r["panels"][mode], ""));
    let h = n(&panel, "/height");
    let unit = h / 16;
    let mut steps = Vec::new();
    for i in 0..9 {
        steps.push(json!({"kind":"open","top":(8-i)*unit,"height":i*unit*2,"delayMs":10}));
    }
    steps.push(json!({"kind":"panel","delayMs":500}));
    for frame in 0..n(&c.media(s(r, "/counter")), "/frameCount") {
        steps.push(json!({"kind":"counter","frame":frame,"delayMs":25}));
    }
    steps.push(json!({"kind":"panel","delayMs":500}));
    let count = n(&c.media(s(&r["animations"][mode], "")), "/frameCount");
    if mode == 2 {
        // 00423b70: the demotion bitmap falls with increasing 16px steps.
        steps.push(json!({"kind":"fall","y":0,"delayMs":0}));
        let (mut y, mut velocity) = (16, 16);
        while y < 254 {
            steps.push(json!({"kind":"fall","y":y,"delayMs":0}));
            velocity += 16;
            y += velocity;
        }
        for observed in a(r, "/demotionBounce") {
            let mut step = observed.clone();
            step["kind"] = json!("bounce");
            steps.push(step);
        }
        steps.push(json!({"kind":"map","delayMs":1000}));
    } else {
        for frame in 0..count - 1 {
            steps.push(json!({"kind":"animation","frame":frame,"delayMs":25}));
        }
        if mode == 1 {
            steps.push(json!({"kind":"animation","frame":(count-2).max(0),"delayMs":1000}));
        }
    }
    steps
}

pub fn accept(
    c: &SceneContent<'_>,
    previous: &Value,
    next: &Value,
    effects: &[Value],
    ui: &mut PresentationState,
) {
    if ui.screen != Screen::Game {
        return;
    }
    let mut cues = VecDeque::new();
    for effect in effects {
        let kind = s(effect, "/type");
        let mut cue = NativeCue {
            scene: effect["scene"].clone(),
            data: effect.clone(),
            ..Default::default()
        };
        match kind {
            "background" => {
                cue.kind = "background".into();
                cue.data["frames"] = json!(45);
            }
            "portraits" => {
                cue.kind = "portraits".into();
                let (mut x, mut speed) = (0, 24);
                let mut xs = Vec::new();
                loop {
                    xs.push(x);
                    x += speed / 2;
                    speed = (speed - 1).max(2);
                    if x > 69 {
                        break;
                    }
                }
                xs.extend(std::iter::repeat_n(70, 50));
                cue.data["positions"] = json!(xs);
                cue.data["frames"] = json!(xs.len());
            }
            "clearPortraits" => {
                if n(effect, "/holdFrames") == 0 {
                    continue;
                }
                cue.kind = "background".into();
                cue.data["frames"] = effect["holdFrames"].clone();
            }
            "birthday" => cue.kind = "birthday".into(),
            "lifeNotice" => cue.kind = "lifeNotice".into(),
            "payday" => {
                let outcome = &effect["outcome"];
                if outcome["level"] != outcome["previousLevel"] {
                    let mode = if n(outcome, "/level") > n(outcome, "/previousLevel") {
                        1
                    } else {
                        2
                    };
                    let mut change = cue.clone();
                    change.kind = "payroll".into();
                    change.data["mode"] = json!(mode);
                    change.data["steps"] = json!(payroll_steps(c, mode));
                    cues.push_back(change);
                }
                cue.kind = "payroll".into();
                cue.data["mode"] = json!(0);
                cue.data["steps"] = json!(payroll_steps(c, 0));
                cues.push_back(cue);
                if b(outcome, "/mastered") {
                    let count = n(
                        &c.media(s(&data()["payroll"], "/masteredAnimation")),
                        "/frameCount",
                    );
                    let mut steps: Vec<_> = (0..count).map(|_| json!({"delayMs":25})).collect();
                    steps.push(json!({"delayMs":500}));
                    cues.push_back(NativeCue {
                        kind: "mastered".into(),
                        data: json!({"steps":steps}),
                        ..Default::default()
                    });
                }
                continue;
            }
            _ => continue,
        }
        cues.push_back(cue);
    }
    if cues.is_empty() {
        return;
    }
    for (i, cue) in cues.iter_mut().enumerate() {
        cue.serial = n(next, "/lastSeq") as u64 * 1024 + i as u64;
    }
    // A map roll first plays its existing dice/walk sequence. Notices then use
    // the landed map, before a queued career/stage screen replaces it.
    let mut snapshot = previous.clone();
    snapshot["stats"] = next["stats"].clone();
    if snapshot["life"].is_object() {
        snapshot["life"]["year"] = next["life"]["year"].clone();
    }
    for effect in effects {
        if s(effect, "/type") == "moved" {
            if let Some(tile) = a(effect, "/tiles").last() {
                snapshot["tileId"] = tile.clone();
            }
        }
    }
    snapshot["pending"] = Value::Null;
    snapshot["scene"] = Value::Null;
    if let Some(sequence) = &mut ui.runtime.sequence {
        sequence.cues.extend(cues);
    } else {
        ui.runtime.sequence = Some(NativeSequence { snapshot, cues });
    }
}

pub fn advance_scene(scene: &Value, ui: &mut PresentationState, dt: u32) {
    let key = format!(
        "{}:{}:{}",
        s(scene, "/patternResource"),
        n(scene, "/patternScrollMode"),
        n(scene, "/patternSerial")
    );
    let p = &mut ui.runtime.pattern;
    if p.key != key {
        *p = PatternAnimationState {
            key,
            direction: p.direction,
            ..Default::default()
        };
    } else {
        p.elapsed += u64::from(dt);
        if p.elapsed > 250 {
            p.elapsed = 0;
            let mode = n(scene, "/patternScrollMode");
            if mode == 3 || mode == 4 {
                p.x -= if mode == 4 { p.direction } else { 1 };
            }
            if mode == 1 || mode == 4 {
                p.y += 1
            } else if mode == 2 || mode == 3 {
                p.y -= 1;
            }
            if p.x < 0 {
                p.x = 640;
            } else if p.x > 640 {
                p.x = 0;
            }
            if p.y < 0 {
                p.y = 480;
            } else if p.y > 480 {
                p.y = 0;
            }
            p.counter = (p.counter + 1) % 3;
            if p.counter == 0 && mode == 4 {
                p.direction *= -1;
            }
        }
    }
    let key = format!(
        "{}:{}:{}",
        s(scene, "/backgroundResource"),
        n(scene, "/backgroundMode"),
        n(scene, "/backgroundSerial")
    );
    let bg = &mut ui.runtime.background;
    if bg.key != key {
        *bg = BackgroundAnimationState {
            key,
            started: true,
            ..Default::default()
        };
    } else if bg.started {
        bg.frame = bg.frame.saturating_add(1);
    } else {
        bg.started = true;
    }
}

fn movie_rules(c: &SceneContent<'_>) -> Value {
    let mut rules = c.rules["epilogue"].clone();
    rules["subtitles"] = json!([
        data()["prologue"]["subtitles"],
        data()["prologue"]["subtitles"]
    ]);
    rules["narration"] = json!([
        data()["prologue"]["narration"],
        data()["prologue"]["narration"]
    ]);
    rules
}
pub fn update(c: &SceneContent<'_>, ui: &mut PresentationState, dt: u32) {
    let Some(sequence) = &mut ui.runtime.sequence else {
        return;
    };
    let Some(cue) = sequence.cues.front_mut() else {
        ui.runtime.sequence = None;
        return;
    };
    let first = !cue.started;
    cue.started = true;
    if !first {
        cue.elapsed_ms = cue.elapsed_ms.saturating_add(u64::from(dt));
    }
    let mut done = false;
    match cue.kind.as_str() {
        "background" | "portraits" => {
            if !first {
                cue.frame += 1;
            }
            done = cue.frame >= n(&cue.data, "/frames") as u32;
        }
        "birthday" | "lifeNotice" => done = cue.elapsed_ms > 3000,
        "payroll" | "mastered" => {
            cue.data["stepMs"] = json!(
                cue.data["stepMs"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(if first { 0 } else { u64::from(dt) })
            );
            let steps = a(&cue.data, "/steps");
            if cue.data["stepMs"].as_u64().unwrap_or(0)
                >= steps
                    .get(cue.frame as usize)
                    .and_then(|v| v["delayMs"].as_u64())
                    .unwrap_or(0)
                && !first
            {
                cue.frame += 1;
                cue.data["stepMs"] = json!(0);
            }
            done = cue.frame as usize >= a(&cue.data, "/steps").len();
        }
        "opening" => {
            done = cue.elapsed_ms
                >= data()["prologue"]["openingMetadata"]["durationMs"]
                    .as_u64()
                    .unwrap()
        }
        "prologue" => {
            let duration = data()["prologue"]["movieMetadata"]["durationMs"]
                .as_u64()
                .unwrap();
            if cue.elapsed_ms >= duration {
                let rules: virtual_life_core::epilogue::EpilogueRules =
                    serde_json::from_value(movie_rules(c)).unwrap();
                if cue.data["movie"].is_null() {
                    cue.data["movie"] = serde_json::to_value(
                        virtual_life_core::epilogue::EndingMovie::new(false, &rules),
                    )
                    .unwrap();
                } else {
                    let mut movie: virtual_life_core::epilogue::EndingMovie =
                        serde_json::from_value(cue.data["movie"].clone()).unwrap();
                    movie.tick(dt, &rules);
                    // Only the two caption-loop segments belong to a prologue.
                    // The shared ending clock's Outro transition ends this cue.
                    done = movie.phase != virtual_life_core::epilogue::MoviePhase::Subtitles;
                    cue.data["movie"] = serde_json::to_value(movie).unwrap();
                }
            }
        }
        _ => done = true,
    }
    let scene = cue.scene.clone();
    if done {
        sequence.cues.pop_front();
        if sequence.cues.is_empty() {
            ui.runtime.sequence = None;
        }
    }
    advance_scene(&scene, ui, dt);
}

fn skip(f: &mut SceneFrame, label: &str, all_keys: bool) {
    control(
        f,
        "native-presentation-continue",
        label,
        Rect::new(0, 0, 640, 480),
        Action::Ui(UiAction::AdvancePresentation),
        if all_keys {
            &["Enter", "Space", "Escape"]
        } else {
            &["Escape:up"]
        },
    );
    if !all_keys {
        f.controls.last_mut().unwrap().bounds = Rect::new(0, 0, 0, 0);
    }
}
fn positioned_sound(
    f: &mut SceneFrame,
    id: &str,
    path: &str,
    time: u64,
    looped: bool,
    serial: u64,
) {
    sound(f, id, path, looped, serial);
    f.sounds.last_mut().unwrap().position_ms = Some(time);
}
fn original_text(
    f: &mut SceneFrame,
    id: &str,
    value: impl ToString,
    x: i32,
    y: i32,
    palette: &str,
) {
    text(
        f,
        id,
        value.to_string(),
        Rect::new(x, y, 300, 16),
        [0, 0, 0, 255],
    );
    let t = f.texts.last_mut().unwrap();
    t.palette = Some(palette.into());
    t.palette_index = Some(0);
}
pub fn compose(f: &mut SceneFrame, c: &SceneContent<'_>, ui: &PresentationState) -> bool {
    let Some(sequence) = &ui.runtime.sequence else {
        return false;
    };
    let Some(cue) = sequence.cues.front() else {
        return false;
    };
    f.key = format!("native-presentation:{}:{}", cue.kind, cue.serial);
    match cue.kind.as_str() {
        "background" | "portraits" => {
            let mut snapshot = sequence.snapshot.clone();
            snapshot["scene"] = cue.scene.clone();
            snapshot["pending"] = json!({"kind":"dialogue","text":""});
            map::event(f, c, &snapshot, ui);
            f.texts.clear();
            f.controls.clear();
            f.sprites
                .retain(|sp| !sp.id.starts_with("event-talk") && !sp.id.starts_with("event-next"));
            if cue.kind == "portraits" {
                let mode = n(&cue.data, "/transition");
                let x = cue.data["positions"][cue.frame as usize]
                    .as_i64()
                    .unwrap_or(70) as i32;
                if mode != 1 && mode != 2 && x != 70 {
                    f.sprites.retain(|sp| !sp.id.contains("PortraitResource"));
                } else if mode == 1 {
                    for sp in &mut f.sprites {
                        if sp.id == "leftPortraitResource" {
                            sp.x = x;
                        } else if sp.id == "rightPortraitResource" {
                            sp.x = 640 - x - 88;
                        }
                    }
                }
            }
        }
        "birthday" | "lifeNotice" | "payroll" | "mastered" => {
            if !sequence.snapshot["mapId"].is_null() {
                let mut map_snapshot = sequence.snapshot.clone();
                if cue.kind == "payroll" && n(&cue.data, "/mode") != 0 {
                    map_snapshot["stats"]["money"] =
                        json!(n(&map_snapshot, "/stats/money") - n(&cue.data, "/outcome/total"));
                }
                map::compose(f, c, &map_snapshot, ui);
            }
            f.controls.clear();
            if cue.kind == "payroll" {
                payroll(f, c, cue, &sequence.snapshot);
            } else if cue.kind == "mastered" {
                let path = s(&data()["payroll"], "/masteredAnimation");
                let i = sprite(f, "career-mastered", path, 220, 0, 220);
                f.sprites[i].frame = cue
                    .frame
                    .min(n(&c.media(path), "/frameCount").saturating_sub(1) as u32);
                f.sprites[i].transparent = Transparency::FirstPixel;
                positioned_sound(
                    f,
                    "mastered-sound",
                    "install/wav/levelup.wav",
                    cue.elapsed_ms,
                    false,
                    cue.serial,
                );
            } else {
                let resource = if cue.kind == "birthday" {
                    s(&cue.data, "/resource")
                } else {
                    s(&data()["lifeNotices"][n(&cue.data, "/index") as usize], "")
                };
                let i = sprite(f, "map-notice", resource, 195, 120, 220);
                f.sprites[i].transparent = if cue.kind == "birthday" {
                    Transparency::FirstPixel
                } else {
                    Transparency::Opaque
                };
                if cue.kind == "birthday" {
                    positioned_sound(
                        f,
                        "birthday-sound",
                        s(data(), "/birthdaySound"),
                        cue.elapsed_ms,
                        false,
                        cue.serial,
                    );
                }
                skip(f, "继续人生", true);
            }
        }
        "opening" | "prologue" => {
            let r = &data()["prologue"];
            if cue.kind == "opening" || cue.data["movie"].is_null() {
                let prefix = if cue.kind == "opening" {
                    "opening"
                } else {
                    "movie"
                };
                let fps = s(&r[format!("{prefix}Metadata")], "/frameRate")
                    .split('/')
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(24);
                f.video = Some(SceneVideo {
                    resource_id: s(r, &format!("/{prefix}")).into(),
                    time_ms: cue.elapsed_ms,
                    fps,
                    bounds: Rect::new(0, 60, 640, 360),
                    scanlines: true,
                });
                let audio = if cue.kind == "opening" {
                    s(r, "/openingAudio")
                } else {
                    s(&c.rules["epilogue"], "/music")
                };
                if c.manifest["resources"].get(audio).is_some() {
                    positioned_sound(
                        f,
                        if cue.kind == "opening" {
                            "opening-audio"
                        } else {
                            "prologue-music"
                        },
                        audio,
                        cue.elapsed_ms,
                        cue.kind == "prologue",
                        cue.serial,
                    );
                }
            } else {
                ending::movie(
                    f,
                    &json!({"movie":cue.data["movie"],"victory":false}),
                    &movie_rules(c),
                );
                // The reused renderer also adds an ending-domain action;
                // opening input belongs solely to this presentation queue.
                f.controls.clear();
                if let Some(music) = f
                    .sounds
                    .iter_mut()
                    .find(|sound| sound.id == "ending-movie-music")
                {
                    music.id = "prologue-music".into();
                    music.position_ms = Some(cue.elapsed_ms);
                    music.restart_serial = cue.serial;
                }
            }
            skip(f, "跳过开场影片", false);
            if cue.kind == "opening" {
                f.controls.last_mut().unwrap().keys = vec!["Escape".into()];
            }
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn content() -> SceneContent<'static> {
        static C: OnceLock<(Value, Value, Value)> = OnceLock::new();
        let (manifest, maps, rules) = C.get_or_init(|| {
            (
                serde_json::from_str(include_str!("../../../../assets/manifest.json")).unwrap(),
                serde_json::from_str(include_str!("../../../../assets/catalog/maps.json")).unwrap(),
                serde_json::from_str(include_str!("../../../../assets/catalog/rules.json"))
                    .unwrap(),
            )
        });
        SceneContent {
            manifest,
            maps,
            rules,
        }
    }
    fn ui() -> PresentationState {
        PresentationState {
            screen: Screen::Game,
            ..Default::default()
        }
    }
    #[test]
    fn n1_transition_queue_retains_intermediate_frames_and_restores_without_catchup() {
        let c = content();
        let mut ui = ui();
        let snapshot = json!({"lastSeq":5});
        let scene = json!({"patternResource":"install/bmp/bug06.bmp","patternScrollMode":2,"patternSerial":1,"backgroundResource":"install/bmp/bg253.bmp","backgroundMode":8,"backgroundSerial":1});
        accept(
            &c,
            &snapshot,
            &snapshot,
            &[
                json!({"type":"background","scene":scene}),
                json!({"type":"portraits","transition":1,"scene":scene}),
            ],
            &mut ui,
        );
        update(&c, &mut ui, 16);
        update(&c, &mut ui, 50000);
        assert_eq!(ui.runtime.sequence.as_ref().unwrap().cues[0].frame, 1);
        let mut restored: PresentationState =
            serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
        for _ in 0..130 {
            let mut a = SceneFrame::default();
            let mut b = SceneFrame::default();
            assert_eq!(compose(&mut a, &c, &ui), compose(&mut b, &c, &restored));
            assert_eq!(
                serde_json::to_value(a).unwrap(),
                serde_json::to_value(b).unwrap()
            );
            update(&c, &mut ui, 16);
            update(&c, &mut restored, 16);
        }
        assert!(!blocking(&ui));
    }
    #[test]
    fn n1_background_modes_keep_original_flash_palette_and_wave_bounds() {
        let c = content();
        let mut ui = ui();
        assert!(
            data()["background"]["wave"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_i64().unwrap() != 0)
        );
        for mode in 0..=8 {
            let scene = json!({"backgroundResource":"install/bmp/bg253.bmp","backgroundMode":mode});
            for frame in [0, 1, 4, 5, 20, 21, 40, 41, 66, 67, 101] {
                ui.runtime.background.frame = frame;
                let mut f = SceneFrame::default();
                background(&mut f, &c, &scene, &ui);
                if mode == 5 || mode == 6 {
                    assert_eq!(f.sprites.len(), 268);
                    assert_eq!(f.sprites[0].source, Some(Rect::new(0, 0, 500, 1)));
                } else if mode == 2 && frame % 21 < 5 || mode == 3 && (frame % 41 + 1) < 2 {
                    assert_eq!(f.fills[0].bounds, Rect::new(70, 31, 500, 268));
                } else {
                    assert_eq!(f.sprites[0].clip, Some(Rect::new(70, 31, 500, 268)));
                }
            }
        }
        let scene = json!({"patternResource":"install/bmp/bug06.bmp","patternScrollMode":2,"patternSerial":9});
        advance_scene(&scene, &mut ui, 16);
        advance_scene(&scene, &mut ui, 251);
        assert_eq!(ui.runtime.pattern.y, 480);
    }
    #[test]
    fn notices_payroll_and_movies_use_original_assets_and_never_expose_map_controls() {
        let c = content();
        let mut ui = ui();
        let snapshot = json!({"lastSeq":4,"player":{"name":"玩家"}});
        accept(
            &c,
            &snapshot,
            &snapshot,
            &[
                json!({"type":"birthday","resource":"install/bmp/jm01.bmp"}),
                json!({"type":"lifeNotice","index":1}),
                json!({"type":"payday","career":0,"outcome":{"previousLevel":4,"level":5,"mastered":true,"salary":8000,"spouseSalary":0,"total":8000}}),
            ],
            &mut ui,
        );
        let kinds: Vec<_> = ui
            .runtime
            .sequence
            .as_ref()
            .unwrap()
            .cues
            .iter()
            .map(|v| v.kind.as_str())
            .collect();
        assert_eq!(
            kinds,
            ["birthday", "lifeNotice", "payroll", "payroll", "mastered"]
        );
        for _ in 0..600 {
            let mut frame = SceneFrame::default();
            if !compose(&mut frame, &c, &ui) {
                break;
            }
            assert!(
                frame
                    .controls
                    .iter()
                    .all(|v| matches!(v.action, Action::Ui(_)))
            );
            for sp in frame.sprites {
                assert!(
                    c.manifest["resources"].get(&sp.resource_id).is_some(),
                    "{}",
                    sp.resource_id
                );
            }
            update(&c, &mut ui, 1000);
        }
        assert!(!blocking(&ui));
        start_movie(&mut ui, "opening", Value::Null);
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &ui);
        assert_eq!(f.video.as_ref().unwrap().fps, 15);
        assert!(f.video.unwrap().scanlines);
        advance(&mut ui);
        assert!(!blocking(&ui));
        start_movie(&mut ui, "prologue", Value::Null);
        update(&c, &mut ui, 16);
        update(&c, &mut ui, 5375);
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &ui);
        assert_eq!(
            f.video.as_ref().unwrap().resource_id,
            "derived/proepi/aopen02.webm"
        );
        update(&c, &mut ui, 250);
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &ui);
        assert!(
            f.sounds
                .iter()
                .any(|s| s.resource_id == "cd/data/proepi/enter.wav")
        );
        update(&c, &mut ui, 501);
        update(&c, &mut ui, 1);
        let mut f = SceneFrame::default();
        compose(&mut f, &c, &ui);
        assert!(f.texts.iter().any(|text| text.text == "孩子"));
        assert!(
            f.controls
                .iter()
                .all(|control| matches!(control.action, Action::Ui(_)))
        );
        for _ in 0..20 {
            let mut f = SceneFrame::default();
            if !compose(&mut f, &c, &ui) {
                break;
            }
            assert_eq!(f.video.unwrap().resource_id, "derived/proepi/aopen02.webm");
            update(&c, &mut ui, 100000);
        }
        assert!(!blocking(&ui));
    }

    #[test]
    fn imported_sequences_reject_out_of_range_frames_and_malformed_movie_state() {
        let mut ui = ui();
        start_movie(&mut ui, "prologue", Value::Null);
        assert!(valid_sequence(&ui));
        ui.runtime.sequence.as_mut().unwrap().cues[0].data =
            json!({"movie":{"phase":"subtitles","segment":7}});
        assert!(!valid_sequence(&ui));
        start_movie(&mut ui, "payroll", Value::Null);
        assert!(!valid_sequence(&ui));
        let cue = &mut ui.runtime.sequence.as_mut().unwrap().cues[0];
        cue.data = json!({"mode":0,"career":0,"steps":[{"delayMs":25}]});
        cue.frame = 1;
        assert!(!valid_sequence(&ui));
    }
}

fn payroll(f: &mut SceneFrame, c: &SceneContent<'_>, cue: &NativeCue, snapshot: &Value) {
    let r = &data()["payroll"];
    let mode = n(&cue.data, "/mode") as usize;
    let panel = s(&r["panels"][mode], "");
    let (x, y) = if mode == 0 { (175, 66) } else { (153, 13) };
    let step = &cue.data["steps"][cue.frame as usize];
    if s(step, "/kind") == "map" {
        return;
    }
    if s(step, "/kind") == "bounce" {
        f.offset.1 = n(step, "/offsetY");
    }
    let i = sprite(f, "payroll-panel", panel, x, y, 220);
    f.sprites[i].transparent = Transparency::Index(1);
    if s(step, "/kind") == "open" {
        f.sprites[i].clip = Some(Rect::new(
            x,
            y + n(step, "/top"),
            n(&c.media(panel), "/width").max(0) as u32,
            n(step, "/height").max(0) as u32,
        ));
    }
    let outcome = &cue.data["outcome"];
    if mode == 0 {
        for (i, key) in ["salary", "spouseSalary", "total"].iter().enumerate() {
            let v = n(outcome, &format!("/{key}")).to_string();
            original_text(
                f,
                &format!("payroll-value-{i}"),
                &v,
                x + 176 - v.len() as i32 * 4,
                y + 118 + i as i32 * 22,
                panel,
            );
        }
    } else {
        let career = n(&cue.data, "/career").max(0) as usize;
        let rank = (n(outcome, "/previousLevel") - 1).clamp(0, 4) as usize;
        for (i, row) in a(&r["rankPanels"][career][rank][mode - 1], "")
            .iter()
            .enumerate()
        {
            let mut value = s(row, "/text");
            let mut left = n(row, "/x");
            if value == "__NAME__" {
                value = s(snapshot, "/player/name");
                left += 4 * (8 - encoding_rs::GBK.encode(value).0.len() as i32);
            }
            original_text(
                f,
                &format!("payroll-rank-{i}"),
                value,
                x + left,
                y + n(row, "/y"),
                panel,
            );
        }
    }
    // Opening clipping also applies to text written onto the original panel.
    if let Some(clip) = f.sprites[i].clip {
        for t in &mut f.texts {
            if t.id.starts_with("payroll-") {
                t.clip = Some(clip);
            }
        }
    }
    if ["counter", "animation"].contains(&s(step, "/kind")) {
        let counter = s(step, "/kind") == "counter";
        let resource = if counter {
            s(r, "/counter")
        } else {
            s(&r["animations"][mode], "")
        };
        let position = &r[if counter {
            "counterPositions"
        } else {
            "animationPositions"
        }][mode];
        let i = sprite(
            f,
            "payroll-animation",
            resource,
            n(position, "/0"),
            n(position, "/1"),
            222,
        );
        f.sprites[i].frame = n(step, "/frame").max(0) as u32;
        f.sprites[i].transparent = Transparency::FirstPixel;
    }
    if s(step, "/kind") == "fall" {
        sprite(
            f,
            "payroll-demotion",
            s(r, "/demotionImage"),
            235,
            n(step, "/y"),
            223,
        );
    }
    if s(step, "/kind") == "bounce" && step["sprite"].is_object() {
        let resource = s(&r["animations"][mode], "");
        let i = sprite(
            f,
            "payroll-bounce",
            resource,
            n(step, "/sprite/x"),
            n(step, "/sprite/y") - f.offset.1,
            222,
        );
        f.sprites[i].frame = n(step, "/sprite/frame").max(0) as u32;
        f.sprites[i].transparent = Transparency::FirstPixel;
    }
    positioned_sound(
        f,
        "payroll-sound",
        if mode == 0 {
            "install/wav/money.wav"
        } else if mode == 1 {
            "install/wav/levelup.wav"
        } else {
            "install/wav/life_low.wav"
        },
        cue.elapsed_ms,
        false,
        cue.serial,
    );
}

pub fn background(f: &mut SceneFrame, c: &SceneContent<'_>, scene: &Value, ui: &PresentationState) {
    let resource = c.resolve(s(scene, "/backgroundResource"));
    if resource.is_empty() {
        return;
    }
    let meta = c.media(&resource);
    let framed = n(&meta, "/width") == 500 && n(&meta, "/height") == 268;
    let (x, y) = if framed { (70, 31) } else { (0, 0) };
    let mode = n(scene, "/backgroundMode");
    let frame = ui.runtime.background.frame;
    let clip = Rect::new(
        x,
        y,
        n(&meta, "/width").max(0) as u32,
        n(&meta, "/height").max(0) as u32,
    );
    if mode == 2 && frame % 21 < 5 || mode == 3 && (frame % 41 + 1) < 2 {
        f.fills.push(SceneFill {
            id: "event-background-flash".into(),
            bounds: clip,
            color: [255; 4],
            z: 2,
            ..Default::default()
        });
        return;
    }
    if (mode == 5 || mode == 6) && framed {
        let progress = frame.min(100);
        let progress = if mode == 5 { progress } else { 100 - progress };
        for row in 0..268 {
            let wave = n(
                &data()["background"]["wave"],
                &format!("/{}", (frame as usize % 67 + row as usize % 67) % 67),
            );
            let offset = (wave as f64
                * progress as f64
                * data()["background"]["waveFactor"].as_f64().unwrap()
                + data()["background"]["waveOrigin"].as_f64().unwrap())
            .trunc() as i32;
            let i = sprite(
                f,
                &format!("event-background-row-{row}"),
                &resource,
                offset,
                y + row,
                2,
            );
            f.sprites[i].source = Some(Rect::new(0, row, 500, 1));
            f.sprites[i].clip = Some(clip);
            f.sprites[i].transparent = Transparency::Opaque;
        }
        return;
    }
    let dy = if mode == 1 {
        [-2, 0, 2][frame as usize % 3]
    } else {
        0
    };
    let i = sprite(f, "event-background", resource, x, y + dy, 2);
    let sp = &mut f.sprites[i];
    sp.transparent = Transparency::Opaque;
    sp.clip = Some(clip);
    if [4, 7, 8].contains(&mode) {
        sp.palette_effect = Some(PaletteEffect {
            grayscale: mode == 4,
            subtract: if mode == 7 {
                (frame + 1).min(41) as u8 * 6
            } else {
                0
            },
            ceiling: if mode == 8 {
                frame.min(41) as u8 * 6
            } else {
                255
            },
        });
    }
}
