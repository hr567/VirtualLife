//! Runs the production ordered systems with browser I/O replaced by in-memory queues.
use bevy::prelude::*;
use serde_json::{Value, json};
use virtual_life_app::{
    PlatformEvent, PlatformEvents, PlatformRequests,
    pixels::ImageBank,
    runtime::{Actions, CurrentScene, GameHost, advance_game, apply_actions, receive_platform},
    scene::{Action, Screen, UiAction},
};

fn catalog() -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |name: &str| {
        serde_json::from_str::<Value>(
            &std::fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap(),
        )
        .unwrap()
    };
    json!({"maps":read("maps"),"events":read("events"),"rules":read("rules"),"questionnaires":read("questionnaires"),"seed":1,"settings":{"volume":50}})
}
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<GameHost>()
        .init_resource::<Actions>()
        .init_resource::<CurrentScene>()
        .init_resource::<ImageBank>()
        .init_resource::<PlatformRequests>()
        .init_resource::<PlatformEvents>();
    app.add_systems(
        Update,
        (receive_platform, apply_actions, advance_game).chain(),
    );
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            operation: "init".into(),
            value: catalog(),
            ..Default::default()
        });
    {
        let mut host = app.world_mut().resource_mut::<GameHost>();
        host.test_clock = true;
        host.loading = false;
        host.ui.profile["name"] = json!("调度回归");
    }
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::StartGame));
    app.update();
    assert_eq!(app.world().resource::<GameHost>().snapshot["lastSeq"], 1);
    app
}
fn frame(app: &mut App, elapsed_ms: u32) {
    app.world_mut().resource_mut::<GameHost>().manual_frame = Some(elapsed_ms);
    app.update();
}
fn input(app: &mut App, value: Value) {
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Game(value));
}
fn saved_state(app: &App) -> String {
    app.world()
        .resource::<GameHost>()
        .session
        .as_ref()
        .unwrap()
        .save()
}

#[test]
fn a_save_restores_native_ui_time_camera_and_motion_without_replaying_held_inputs() {
    use virtual_life_app::scene::{AnimationClock, MotionPose, MotionSequence};
    let mut app = app();
    let domain = saved_state(&app);
    {
        let mut host = app.world_mut().resource_mut::<GameHost>();
        host.ui.time_ms = 12000;
        host.ui.camera = (66, 38);
        host.ui.profile["_nativePanel"] = json!({"scope":"shop","selection":2,"elapsed":1375});
        host.ui.profile["_imeActive"] = json!(true);
        host.ui.profile["_testKey"] = json!({"key":"Space"});
        host.ui.pressed = Some("fire".into());
        host.ui.runtime.animations.insert(
            "shop-boss".into(),
            AnimationClock {
                frame: 7,
                elapsed_ms: 19,
                ..Default::default()
            },
        );
        host.ui.runtime.motion = Some(MotionSequence {
            frames: vec![
                MotionPose::default(),
                MotionPose {
                    actor_frame: 3,
                    ..Default::default()
                },
            ],
            cursor: 1,
            ..Default::default()
        });
    }
    open_saves_ready(&mut app);
    app.world_mut().resource_mut::<PlatformRequests>().0.clear();
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::Save));
    frame(&mut app, 0);
    let payload = app
        .world()
        .resource::<PlatformRequests>()
        .0
        .iter()
        .find(|r| r.operation == "save")
        .unwrap()
        .payload["payload"]
        .as_str()
        .unwrap()
        .to_owned();
    app.world_mut().resource_mut::<GameHost>().ui = Default::default();
    event(&mut app, "load", json!({"payload":payload}));
    frame(&mut app, 0);
    let host = app.world().resource::<GameHost>();
    assert_eq!(host.ui.screen, Screen::Game);
    assert_eq!(host.ui.time_ms, 12000);
    assert_eq!(host.ui.camera, (66, 38));
    assert_eq!(host.ui.profile["_nativePanel"]["elapsed"], 1375);
    assert_eq!(host.ui.runtime.animations["shop-boss"].frame, 7);
    assert_eq!(host.ui.runtime.motion.as_ref().unwrap().cursor, 1);
    assert!(host.ui.pressed.is_none());
    assert!(host.ui.profile["_testKey"].is_null());
    assert!(host.ui.profile["_imeActive"].is_null());
    assert_eq!(saved_state(&app), domain);
}

#[test]
fn an_invalid_presentation_cannot_partially_restore_the_domain() {
    let mut app = app();
    let before = saved_state(&app);
    let mut bad: Value = serde_json::from_str(&before).unwrap();
    bad["presentation"] = json!({"profile":"broken"});
    event(&mut app, "load", json!({"payload":bad.to_string()}));
    frame(&mut app, 0);
    assert_eq!(saved_state(&app), before);
    assert!(
        app.world()
            .resource::<GameHost>()
            .notice
            .contains("场景状态")
    );
}

#[test]
fn real_clock_quantizes_global_endpoints_without_losing_submillisecond_remainders() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    app.world_mut().resource_mut::<GameHost>().test_clock = false;
    let before = app.world().resource::<GameHost>().ui.time_ms;
    let mut total = 0;
    for _ in 0..60 {
        app.world_mut()
            .resource_mut::<Time<bevy::time::Real>>()
            .advance_by(std::time::Duration::from_nanos(16_666_667));
        app.world_mut().run_system_once(advance_game).unwrap();
        total += u64::from(app.world().resource::<GameHost>().frame_ms);
    }
    assert_eq!(total, 1000);
    assert_eq!(app.world().resource::<GameHost>().ui.time_ms - before, 1000);
}

#[test]
fn real_submillisecond_frames_update_once_but_explicit_zero_test_time_stays_frozen() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    app.world_mut().resource_mut::<GameHost>().test_clock = false;
    let before = app.world().resource::<GameHost>().ui.runtime.frame_number;
    app.world_mut()
        .resource_mut::<Time<bevy::time::Real>>()
        .advance_by(std::time::Duration::from_nanos(100_000));
    app.world_mut().run_system_once(advance_game).unwrap();
    assert_eq!(app.world().resource::<GameHost>().frame_ms, 0);
    assert_eq!(
        app.world().resource::<GameHost>().ui.runtime.frame_number,
        before + 1
    );
    app.world_mut().resource_mut::<GameHost>().test_clock = true;
    frame(&mut app, 0);
    assert_eq!(
        app.world().resource::<GameHost>().ui.runtime.frame_number,
        before + 1
    );
}

#[test]
fn foreground_long_frames_keep_real_milliseconds_but_visibility_resume_discards_background_time() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    let before = saved_state(&app);
    event(&mut app, "visibility", json!({"hidden":true}));
    frame(&mut app, 16);
    event(&mut app, "visibility", json!({"hidden":false}));
    frame(&mut app, 30000);
    assert_eq!(saved_state(&app), before);
    assert_eq!(app.world().resource::<GameHost>().frame_ms, 0);
    app.world_mut().resource_mut::<GameHost>().test_clock = false;
    app.world_mut()
        .resource_mut::<Time<bevy::time::Real>>()
        .advance_by(std::time::Duration::from_secs(5));
    app.world_mut().run_system_once(advance_game).unwrap();
    assert_eq!(app.world().resource::<GameHost>().frame_ms, 5000);
}
fn event(app: &mut App, operation: &str, value: Value) {
    let id = if matches!(operation, "load" | "import") {
        restore_request(app, operation)
    } else {
        0
    };
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            id,
            operation: operation.into(),
            value,
            ..Default::default()
        });
}

fn open_saves_ready(app: &mut App) {
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::OpenSaves));
    frame(app, 0);
    let id = app
        .world()
        .resource::<PlatformRequests>()
        .0
        .iter()
        .rev()
        .find(|request| request.operation == "listSaves")
        .unwrap()
        .id;
    let slots=json!((0..9).map(|slot|json!({"occupied":false,"valid":true,"slot":format!("slot-{slot}"),"token":null})).collect::<Vec<_>>());
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            id,
            operation: "listSaves".into(),
            value: slots,
            ..Default::default()
        });
    frame(app, 0);
}
fn restore_request(app: &mut App, operation: &str) -> u32 {
    open_saves_ready(app);
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(if operation == "import" {
            UiAction::Import(1)
        } else {
            UiAction::Load
        }));
    frame(app, 0);
    app.world()
        .resource::<PlatformRequests>()
        .0
        .iter()
        .rev()
        .find(|request| request.operation == operation)
        .unwrap()
        .id
}

#[test]
fn late_restore_results_cannot_override_navigation_or_a_newer_slot_request() {
    let mut app = app();
    let before = saved_state(&app);
    let old = restore_request(&mut app, "import");
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::MainMenu));
    frame(&mut app, 0);
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            id: old,
            operation: "import".into(),
            value: json!({"payload":before}),
            ..Default::default()
        });
    frame(&mut app, 0);
    assert_eq!(app.world().resource::<GameHost>().ui.screen, Screen::Menu);
    assert_eq!(saved_state(&app), before);

    let old = restore_request(&mut app, "load");
    let current = restore_request(&mut app, "load");
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            id: old,
            operation: "load".into(),
            error: Some("old storage failure".into()),
            ..Default::default()
        });
    frame(&mut app, 0);
    assert!(
        !app.world()
            .resource::<GameHost>()
            .notice
            .contains("old storage failure")
    );
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            id: current,
            operation: "load".into(),
            value: json!({"payload":before}),
            ..Default::default()
        });
    frame(&mut app, 0);
    assert_eq!(app.world().resource::<GameHost>().ui.screen, Screen::Game);
    assert_eq!(saved_state(&app), before);
}

#[test]
fn a_failed_obsolete_download_releases_its_id_for_a_later_scene_visit() {
    let mut app = app();
    let generation = app.world().resource::<GameHost>().generation;
    let resource = "install/bmp/pm0201.bmp";
    app.world_mut()
        .resource_mut::<ImageBank>()
        .requested
        .insert(resource.into());
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::MainMenu));
    frame(&mut app, 0);
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            operation: "asset".into(),
            value: json!({"resourceId":resource,"generation":generation}),
            error: Some("obsolete network failure".into()),
            ..Default::default()
        });
    frame(&mut app, 0);
    let bank = app.world().resource::<ImageBank>();
    assert!(!bank.requested.contains(resource));
    assert!(!bank.failures.contains_key(resource));
    assert!(app.world().resource::<GameHost>().error.is_none());
}

#[test]
fn the_original_month_switch_keeps_day_31_and_can_start_and_restore_that_profile() {
    let mut app = app();
    for action in [
        UiAction::NewProfile,
        UiAction::SetField {
            field: "birthDay".into(),
            value: json!(31),
        },
        UiAction::SetField {
            field: "birthMonth".into(),
            value: json!(2),
        },
        UiAction::StartGame,
    ] {
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Ui(action));
    }
    frame(&mut app, 0);
    assert_eq!(
        app.world().resource::<GameHost>().snapshot["player"]["birthDay"],
        31
    );
    assert_eq!(
        app.world().resource::<GameHost>().snapshot["player"]["birthMonth"],
        2
    );
    let save = saved_state(&app);
    event(&mut app, "load", json!({"payload":save}));
    frame(&mut app, 0);
    assert_eq!(saved_state(&app), save);
}

#[test]
fn leaving_a_walk_does_not_save_and_a_new_creator_starts_with_clean_original_defaults() {
    use virtual_life_app::scene::{MotionPose, MotionSequence};
    let mut app = app();
    {
        let mut host = app.world_mut().resource_mut::<GameHost>();
        host.ui.volume = 75;
        host.ui.profile["sex"] = json!("female");
        host.ui.profile["birthMonth"] = json!(12);
        host.ui.runtime.motion = Some(MotionSequence {
            frames: vec![MotionPose::default()],
            ..Default::default()
        });
    }
    app.world_mut().resource_mut::<PlatformRequests>().0.clear();
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::MainMenu));
    frame(&mut app, 0);
    assert!(
        !app.world()
            .resource::<PlatformRequests>()
            .0
            .iter()
            .any(|request| request.operation == "save")
    );
    assert!(
        app.world()
            .resource::<GameHost>()
            .ui
            .runtime
            .motion
            .is_none()
    );
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::NewProfile));
    frame(&mut app, 0);
    let host = app.world().resource::<GameHost>();
    assert_eq!(host.ui.screen, Screen::Profile);
    assert_eq!(host.ui.profile["name"], "丁马克");
    assert_eq!(host.ui.profile["sex"], "male");
    assert_eq!(host.ui.profile["birthMonth"], 1);
    assert_eq!(host.ui.profile["birthDay"], 1);
    assert_eq!(host.ui.profile["bloodType"], "A");
    assert_eq!(host.ui.volume, 75);
}

#[test]
fn late_autosave_success_does_not_overlay_the_original_creator_but_explicit_save_reports_success() {
    let mut app = app();
    app.world_mut().resource_mut::<GameHost>().notice = "进度已保存。".into();
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::NewProfile));
    frame(&mut app, 0);
    event(&mut app, "save", Value::Null);
    frame(&mut app, 0);
    assert!(app.world().resource::<GameHost>().notice.is_empty());
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::OpenSaves));
    frame(&mut app, 0);
    let mut app = crate::app();
    open_saves_ready(&mut app);
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::Save));
    frame(&mut app, 0);
    let id = app
        .world()
        .resource::<PlatformRequests>()
        .0
        .iter()
        .rev()
        .find(|request| request.operation == "save")
        .unwrap()
        .id;
    app.world_mut()
        .resource_mut::<PlatformEvents>()
        .0
        .push_back(PlatformEvent {
            id,
            operation: "save".into(),
            ..Default::default()
        });
    frame(&mut app, 0);
    assert_eq!(app.world().resource::<GameHost>().notice, "进度已保存。");
}

#[test]
fn an_invalid_new_creator_cannot_reopen_the_previous_life() {
    let mut app = app();
    let previous = saved_state(&app);
    for action in [
        UiAction::NewProfile,
        UiAction::SetField {
            field: "name".into(),
            value: json!(""),
        },
        UiAction::StartGame,
    ] {
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Ui(action));
    }
    frame(&mut app, 0);
    let host = app.world().resource::<GameHost>();
    assert_eq!(host.ui.screen, Screen::Profile);
    assert!(host.notice.contains("姓名"));
    let mut previous: Value = serde_json::from_str(&previous).unwrap();
    let current: Value = serde_json::from_str(&saved_state(&app)).unwrap();
    // Rejected public commands still consume their sequence number; every
    // actual life field, including the random stream, must remain unchanged.
    assert_eq!(
        current["state"]["lastSeq"].as_u64().unwrap(),
        previous["state"]["lastSeq"].as_u64().unwrap() + 1
    );
    previous["state"]["lastSeq"] = current["state"]["lastSeq"].clone();
    assert_eq!(current["state"], previous["state"]);
}

#[test]
fn a_movie_clip_switch_closes_its_decoder_and_discards_late_frames_without_duplicate_requests() {
    use bevy::ecs::system::RunSystemOnce;
    use virtual_life_app::runtime::prepare_scene;
    let mut app = app();
    app.world_mut().resource_mut::<PlatformRequests>().0.clear();
    app.world_mut().resource_mut::<GameHost>().snapshot["pending"] = json!({
        "kind":"ending", "state":{"phase":"movie","victory":false,
            "movie":{"phase":"subtitles","displayedVideoTimeMs":0}}
    });
    app.world_mut().run_system_once(prepare_scene).unwrap();
    let old_generation = app.world().resource::<GameHost>().generation;
    app.world_mut().resource_mut::<PlatformRequests>().0.clear();
    app.world_mut().resource_mut::<GameHost>().snapshot["pending"]["state"]["movie"]["phase"] =
        json!("outro");
    app.world_mut().run_system_once(prepare_scene).unwrap();
    assert_ne!(
        app.world().resource::<GameHost>().generation,
        old_generation
    );
    let requests = &app.world().resource::<PlatformRequests>().0;
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.operation == "videoClose")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.operation == "videoFrame")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .find(|r| r.operation == "videoFrame")
            .unwrap()
            .payload["resourceId"],
        "derived/proepi/aopen03.webm"
    );
    app.world_mut().resource_mut::<PlatformEvents>().0.push_back(PlatformEvent {
        operation: "videoFrame".into(),
        value: json!({"resourceId":"derived/proepi/aopen02.webm","frame":0,"generation":old_generation,"width":1,"height":1}),
        bytes: vec![255;4], ..Default::default()
    });
    frame(&mut app, 0);
    assert!(app.world().resource::<ImageBank>().video.is_none());
    app.world_mut().run_system_once(prepare_scene).unwrap();
    assert_eq!(
        app.world()
            .resource::<PlatformRequests>()
            .0
            .iter()
            .filter(|r| r.operation == "videoFrame")
            .count(),
        1
    );
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::MainMenu));
    frame(&mut app, 0);
    app.world_mut().run_system_once(prepare_scene).unwrap();
    assert_eq!(
        app.world()
            .resource::<PlatformRequests>()
            .0
            .iter()
            .filter(|r| r.operation == "videoClose")
            .count(),
        2
    );
}

#[test]
fn received_catalog_actions_then_original_tick_share_one_ordered_bevy_update() {
    let mut app = app();
    input(&mut app, json!({"type":"openStatus"}));
    app.world_mut().resource_mut::<CurrentScene>().0.tick = Some(json!({"type":"statusTick"}));
    frame(&mut app, 250);
    let host = app.world().resource::<GameHost>();
    assert_eq!(host.snapshot["lastSeq"], 3);
    assert_eq!(host.snapshot["pending"]["kind"], "status");
    assert_eq!(host.snapshot["pending"]["state"]["clockMs"], 250);
    let saves = app
        .world()
        .resource::<PlatformRequests>()
        .0
        .iter()
        .filter(|request| request.operation == "save")
        .collect::<Vec<_>>();
    assert!(
        saves.is_empty(),
        "Ordinary actions and ticks must not autosave"
    );
}
#[test]
fn options_visibility_and_loading_freeze_domain_state_without_catching_up() {
    let mut app = app();
    input(&mut app, json!({"type":"openStatus"}));
    app.world_mut().resource_mut::<CurrentScene>().0.tick = Some(json!({"type":"statusTick"}));
    frame(&mut app, 250);
    let before = saved_state(&app);
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::Options));
    frame(&mut app, 5000);
    assert_eq!(saved_state(&app), before);
    assert_eq!(
        app.world().resource::<GameHost>().ui.screen,
        Screen::Options
    );
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::Ui(UiAction::Back));
    frame(&mut app, 16);
    assert_eq!(
        app.world().resource::<GameHost>().snapshot["pending"]["state"]["clockMs"],
        266
    );
    let before = saved_state(&app);
    event(&mut app, "visibility", json!({"hidden":true}));
    frame(&mut app, 50_000);
    assert_eq!(saved_state(&app), before);
    input(&mut app, json!({"type":"statusAction","action":"close"}));
    frame(&mut app, 50_000);
    assert_eq!(saved_state(&app), before);
    event(&mut app, "visibility", json!({"hidden":false}));
    frame(&mut app, 16);
    assert_eq!(
        app.world().resource::<GameHost>().snapshot["pending"]["state"]["clockMs"],
        266
    );
    frame(&mut app, 16);
    assert_eq!(
        app.world().resource::<GameHost>().snapshot["pending"]["state"]["clockMs"],
        282
    );
    let before = saved_state(&app);
    app.world_mut().resource_mut::<GameHost>().loading = true;
    frame(&mut app, 50_000);
    assert_eq!(saved_state(&app), before);
}

#[test]
fn a_save_overlay_preserves_the_options_return_path_to_the_game() {
    let mut app = app();
    for action in [
        UiAction::Options,
        UiAction::Save,
        UiAction::Back,
        UiAction::Back,
    ] {
        app.world_mut()
            .resource_mut::<Actions>()
            .0
            .push_back(Action::Ui(action));
        frame(&mut app, 0);
    }
    assert_eq!(app.world().resource::<GameHost>().ui.screen, Screen::Game);
}
#[test]
fn a_legacy_load_event_cannot_replace_current_bevy_domain_state() {
    let mut app = app();
    let before = saved_state(&app);
    event(
        &mut app,
        "load",
        json!({"payload":include_str!("../../../tests/fixtures/browser-battle.json")}),
    );
    frame(&mut app, 0);
    assert_eq!(saved_state(&app), before);
    let host = app.world().resource::<GameHost>();
    assert!(host.error.is_none());
    assert!(!host.notice.is_empty());
}
