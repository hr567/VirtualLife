//! Typed Bevy integration contracts and fresh-save boundaries.
use super::*;

fn content() -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |name: &str| {
        serde_json::from_str::<Value>(
            &std::fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap(),
        )
        .unwrap()
    };
    let mut content = read("maps");
    for name in ["events", "rules", "questionnaires"] {
        content[name] = read(name);
    }
    content
}
fn started() -> GameSession {
    let mut session = GameSession::from_content(content()).unwrap();
    let response = session.send(GameCommand::NewGame {
        seq: 0,
        profile: Profile {
            name: "Bevy 序列".into(),
            sex: "male".into(),
            birth_month: 1,
            birth_day: 1,
            blood_type: "A".into(),
        },
        seed: 1,
    });
    assert!(response.ok, "{:?}", response.error);
    session
}
#[test]
fn typed_inputs_preserve_strict_sequence_and_automatic_ui_sequence() {
    let mut session = started();
    let before = session.save();
    let rejected = session.dispatch(GameCommand::RollDice {
        seq: 90,
        value: None,
    });
    assert!(!rejected.ok);
    assert_eq!(rejected.error.unwrap().code, "out_of_order");
    assert_eq!(before, session.save());
    let response = session.send(GameCommand::RollDice {
        seq: 0,
        value: None,
    });
    assert!(response.ok, "{:?}", response.error);
    assert_eq!(response.snapshot.last_seq, 2);
    assert!(
        response
            .events
            .iter()
            .any(|effect| effect.kind == GameEffectKind::DiceRolled)
    );
    assert_eq!(
        serde_json::to_value(response.snapshot).unwrap(),
        serde_json::to_value(session.snapshot()).unwrap()
    );
}
#[test]
fn save_validation_and_normal_or_controlled_roll_preflight_do_not_change_state() {
    let mut session = started();
    let before = session.save();
    assert!(session.validate_save(&before).is_ok());
    assert!(session.validate_save("{}").is_err());
    assert!(session.validate_roll(None).is_ok());
    assert!(session.validate_roll(Some(1)).is_err());
    assert_eq!(session.save(), before);
    session.state.spirits.active = Some(0);
    let before = session.save();
    for value in 1..=8 {
        assert!(session.validate_roll(Some(value)).is_ok());
    }
    assert!(session.validate_roll(Some(0)).is_err());
    assert!(session.validate_roll(Some(9)).is_err());
    assert_eq!(session.save(), before);
}
#[test]
fn typed_frames_and_restored_frames_preserve_native_rng_and_timers() {
    let mut original = started();
    original.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    original.state.rng = ReplayRng::new(
        (0..100)
            .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 2)
            .unwrap(),
    );
    original.state.vm = Some(LegacyVm::new(30000));
    original.advance_vm(&mut Vec::new());
    let response = original.send(GameCommand::MinigameAction {
        seq: 0,
        action: "start".into(),
        index: None,
        x: None,
        y: None,
        left: None,
        right: None,
        fire: None,
    });
    assert!(response.ok);
    for elapsed_ms in [16, 71, 101, 500] {
        assert!(original.frame(FrameStep { elapsed_ms }).ok);
    }
    let saved = original.save();
    let mut resumed = GameSession::from_content(content()).unwrap();
    // The synthetic VM program is evidence setup, not persisted content or browser data.
    resumed.program = original.program.clone();
    assert!(resumed.restore(&saved).ok);
    assert_eq!(saved, resumed.save());
    // A pause runs no frames, including while reading the snapshot or saving.
    for _ in 0..3 {
        let _ = resumed.snapshot();
        assert_eq!(saved, resumed.save());
    }
    for elapsed_ms in [1, 16, 101, 999, 5000, 16] {
        let a = original.frame(FrameStep { elapsed_ms });
        let b = resumed.frame(FrameStep { elapsed_ms });
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
        assert_eq!(original.state.rng, resumed.state.rng);
        assert_eq!(original.save(), resumed.save());
    }
}
#[test]
fn fresh_save_uses_sha256_and_rejects_legacy_browser_saves_atomically() {
    let mut session = started();
    let saved = session.save();
    let envelope: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(envelope["format"], "virtual-life-bevy-save");
    assert_eq!(envelope["version"], 1);
    assert_eq!(envelope["contentHash"].as_str().unwrap().len(), 64);
    assert_eq!(envelope["checksum"].as_str().unwrap().len(), 64);
    assert_eq!(
        digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let legacy = include_str!("../../../../tests/fixtures/browser-battle.json");
    assert!(!session.restore(legacy).ok);
    assert_eq!(saved, session.save());
    let mut tampered = envelope;
    tampered["state"]["rng"]["draws"] = json!(999999);
    assert_eq!(
        session.restore(&tampered.to_string()).error.unwrap().code,
        "save_checksum"
    );
    assert_eq!(saved, session.save());
}

fn assert_native_view_is_read_only_and_restores(session: &mut GameSession, kind: &str) -> Value {
    let saved = session.save();
    let rng = session.state.rng.clone();
    let snapshot = session.snapshot();
    let view = snapshot.native_view.unwrap();
    assert_eq!(view["kind"], kind);
    assert_eq!(view["options"], snapshot.pending.unwrap()["options"]);
    for _ in 0..3 {
        assert_eq!(session.snapshot().native_view.as_ref(), Some(&view));
        assert_eq!(session.state.rng, rng);
        assert_eq!(session.save(), saved);
    }
    let envelope: Value = serde_json::from_str(&saved).unwrap();
    assert!(envelope["state"].get("nativeView").is_none());
    let mut restored = GameSession::from_content(content()).unwrap();
    assert!(restored.restore(&saved).ok);
    assert_eq!(restored.snapshot().native_view.as_ref(), Some(&view));
    assert_eq!(restored.save(), saved);
    view
}

#[test]
fn all_six_native_panel_views_preserve_state_rng_and_fresh_save_round_trips() {
    let mut session = started();
    session.begin_stat_shop().unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "statShop");
    assert_eq!(view["offers"].as_array().unwrap().len(), 8);
    assert_eq!(view["exitIndex"], 8);

    let mut session = started();
    session.begin_item_shop().unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "itemShop");
    for (index, offer) in view["offers"].as_array().unwrap().iter().enumerate() {
        assert_eq!(offer["index"], index);
        assert_eq!(offer["available"], true);
        assert!(offer["resource"].as_str().is_some());
    }

    let mut session = started();
    session.begin_career(false).unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "career");
    assert_eq!(view["careers"].as_array().unwrap().len(), 24);
    for career in view["careers"].as_array().unwrap() {
        let index = career["index"].as_u64().unwrap() as usize;
        let option = session.state.pending.as_ref().unwrap()["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|option| option["index"] == index)
            .unwrap();
        assert_eq!(career["disabled"], option["disabled"]);
    }

    let mut session = started();
    session.begin_equipment_workshop().unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "equipmentWorkshop");
    assert_eq!(view["mode"], "purchase");
    assert_eq!(view["entries"].as_array().unwrap().len(), 1);
    assert_eq!(view["entries"][0]["index"], 0);
    assert_eq!(view["entries"][0]["id"], view["offeredId"]);

    let mut session = started();
    session.begin_housing().unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "housing");
    assert_eq!(view["catalog"]["styles"].as_array().unwrap().len(), 4);
    assert_eq!(view["exitIndex"], 4);
    assert!(view["nextInvestment"].is_null());

    let mut session = started();
    session.begin_fortune().unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "fortune");
    assert_eq!(view["phase"], "question");
    assert!(view.get("record").is_none());
    assert!(view["currentQuestion"].get("scores").is_none());
    let response = session.send(GameCommand::ChooseOption { seq: 0, index: 0 });
    assert!(response.ok, "{:?}", response.error);
    let view = assert_native_view_is_read_only_and_restores(&mut session, "fortune");
    assert_eq!(view["phase"], "explanation");
    assert_eq!(view["selectedAnswer"], 0);
    assert_eq!(view["continueIndex"], 0);
}

#[test]
fn native_shop_purchases_keep_original_noncontiguous_choice_indices() {
    let mut session = started();
    session.begin_item_shop().unwrap();
    let view = session.snapshot().native_view.unwrap();
    let money = session.state.stats.money.unwrap();
    let index = view["offers"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|offer| offer["price"].as_i64().is_some_and(|price| price <= money))
        .unwrap()["index"]
        .as_u64()
        .unwrap() as usize;
    let response = session.send(GameCommand::ChooseOption { seq: 0, index });
    assert!(response.ok, "{:?}", response.error);
    let view = assert_native_view_is_read_only_and_restores(&mut session, "itemShop");
    assert_eq!(view["offers"][index]["index"], index);
    assert_eq!(view["offers"][index]["purchased"], true);
    assert_eq!(view["offers"][index]["available"], false);
    assert!(
        !view["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|option| option["index"] == index)
    );
}

#[test]
fn normal_map_and_animated_minigames_do_not_duplicate_native_activity_state() {
    let mut session = started();
    assert!(session.snapshot().native_view.is_none());
    assert!(
        serde_json::to_value(session.snapshot())
            .unwrap()
            .get("nativeView")
            .is_none()
    );
    session.state.host_activity = Some(HostActivity::Fist {
        game: FistGame::new(100),
    });
    session.refresh_host_activity();
    assert!(session.snapshot().native_view.is_none());
    assert_eq!(session.snapshot().pending.unwrap()["game"], "fist");
}

#[test]
fn school_and_allocation_views_expose_only_public_display_data_without_saving_a_projection() {
    let mut session = started();
    session.begin_school_exam().unwrap();
    let view = assert_native_view_is_read_only_and_restores(&mut session, "schoolExam");
    assert_eq!(view["started"], false);
    assert!(view.get("correct").is_none());
    assert!(view["currentQuestion"].get("correct").is_none());
    assert_eq!(
        view["currentQuestion"]["options"].as_array().unwrap().len(),
        4
    );
    assert!(
        session
            .send(GameCommand::ChooseOption { seq: 0, index: 0 })
            .ok
    );
    let view = assert_native_view_is_read_only_and_restores(&mut session, "schoolExam");
    assert_eq!(view["started"], true);
    assert_eq!(view["options"], view["currentQuestion"]["options"]);
    let mut session = started();
    let reward =
        StudentReward::calculate(session.state.legacy_player.as_ref().unwrap(), false).unwrap();
    session.state.pending = Some(
        json!({"kind":"allocation","points":reward.points,"money":reward.money,"defaults":reward.default_allocation()}),
    );
    session.state.phase = "event".into();
    session.state.pending_allocation = Some(reward);
    let view = assert_native_view_is_read_only_and_restores(&mut session, "allocation");
    assert_eq!(view["battles"], 0);
    assert_eq!(view["wins"], 0);
    assert_eq!(view["winPercent"], 0);
    assert_eq!(
        view["points"],
        session.state.pending.as_ref().unwrap()["points"]
    );
}
