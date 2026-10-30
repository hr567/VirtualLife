use super::*;
const CONTENT: &str = r#"{"maps":[{"id":"test","start":{"row":0,"column":0},"chips":[{"row":0,"column":0,"eventId":1,"next":[{"row":1,"column":1}]},{"row":1,"column":1,"eventId":2,"next":[{"row":0,"column":0}]}]}]}"#;
#[test]
fn save_catalog_identity_ignores_json_format_but_rejects_changed_data() {
    let original = Runtime::new(CONTENT).unwrap();
    let save = original.serialize();
    let reordered = r#"{ "maps": [ { "chips": [{ "next": [{"column":1,"row":1}], "eventId":1,"column":0,"row":0 }, {"next":[{"column":0,"row":0}],"eventId":2,"column":1,"row":1}], "start":{"column":0,"row":0}, "id":"test" } ] }"#;
    let mut same = Runtime::new(reordered).unwrap();
    assert_eq!(original.content_hash, same.content_hash);
    assert_eq!(
        serde_json::from_str::<Value>(&same.restore(&save)).unwrap()["ok"],
        true
    );
    let mut changed = Runtime::new(&reordered.replace("\"eventId\":2", "\"eventId\":3")).unwrap();
    let rejected: Value = serde_json::from_str(&changed.restore(&save)).unwrap();
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["error"]["code"], "content_mismatch");
}
#[test]
fn save_catalog_identity_normalizes_nested_javascript_numbers_and_round_trips() {
    let catalog = |rules: &str| format!("{},\"rules\":{rules}}}", &CONTENT[..CONTENT.len() - 1]);
    let native = catalog(
        r#"{"nested":{"integer":1.0,"zero":-0.0,"array":[0.0,-2.0,1e3,0.25,-0.75,1e-7,{"maximum":9007199254740991.0,"minimum":-9007199254740991.0}]}}"#,
    );
    let browser = catalog(
        r#"{"nested":{"array":[0,-2,1000,0.25,-0.75,0.0000001,{"minimum":-9007199254740991,"maximum":9007199254740991}],"zero":0,"integer":1}}"#,
    );
    let mut native = Runtime::new(&native).unwrap();
    let mut browser_runtime = Runtime::new(&browser).unwrap();
    assert_eq!(native.content_hash, browser_runtime.content_hash);
    let original = native.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&browser_runtime.restore(&original)).unwrap()["ok"],
        true
    );
    let browser_save = browser_runtime.serialize();
    assert_eq!(original, browser_save);
    assert_eq!(
        serde_json::from_str::<Value>(&native.restore(&browser_save)).unwrap()["ok"],
        true
    );
    assert_eq!(original, native.serialize());
    let changed = browser.replace("0.25", "0.2500000000000001");
    let mut changed = Runtime::new(&changed).unwrap();
    assert_ne!(native.content_hash, changed.content_hash);
    assert_eq!(
        serde_json::from_str::<Value>(&changed.restore(&original)).unwrap()["error"]["code"],
        "content_mismatch"
    );
    for number in ["9007199254740992.0", "-9007199254740992.0"] {
        let mut value: Value = serde_json::from_str(number).unwrap();
        canonicalize_catalog_numbers(&mut value);
        assert!(value.as_number().unwrap().is_f64());
    }
}
fn dispatch(runtime: &mut Runtime, command: Value) -> Value {
    serde_json::from_str(&runtime.dispatch(&command.to_string())).unwrap()
}
fn real_runtime() -> Runtime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |name: &str| -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap(),
        )
        .unwrap()
    };
    let mut content = read("maps");
    for name in ["events", "rules", "questionnaires"] {
        content[name] = read(name);
    }
    let mut runtime = Runtime::new(&content.to_string()).unwrap();
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":1,"type":"newGame","seed":1,"profile":{"name":"测试","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}})
        )["ok"],
        true
    );
    runtime
}
fn next(runtime: &mut Runtime, kind: &str, index: usize) -> Value {
    dispatch(
        runtime,
        json!({"seq":runtime.state.last_seq+1,"type":kind,"index":index}),
    )
}
fn relationship_action(runtime: &mut Runtime, action: &str, index: usize) -> Value {
    dispatch(
        runtime,
        json!({"seq":runtime.state.last_seq+1,"type":"relationshipAction","action":action,"index":index}),
    )
}
#[test]
fn status_pages_restore_scroll_and_return_without_changing_life() {
    let mut runtime = real_runtime();
    let run = |runtime: &mut Runtime, action: &str, page: Option<&str>, index: Option<usize>| {
        let seq = runtime.state.last_seq + 1;
        dispatch(
            runtime,
            json!({"seq":seq,"type":"statusAction","action":action,"page":page,"index":index}),
        )
    };
    assert!(runtime.snapshot_value()["statusOverview"].is_null());
    assert!(runtime.snapshot_value()["statusDetails"].is_null());
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(&mut runtime, json!({"seq":seq,"type":"openStatus"}))["ok"],
        true
    );
    assert_eq!(run(&mut runtime, "page", Some("house"), None)["ok"], false);
    assert_eq!(run(&mut runtime, "close", None, None)["ok"], true);
    runtime.state.housing = HousingState {
        style: 0,
        furniture: 2,
        investment_stage: 5,
        purchase_years: vec![2025],
    };
    runtime.state.spouse_index = 3;
    runtime.state.career_history = (0..12)
        .map(|index| json!({"year":2022+index,"career":index,"rank":1+index%3}))
        .collect();
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b649e, 6).unwrap();
    player.set_i16(0x4b649c, 2040).unwrap();
    player.set_i16(0x4b64a2, 1).unwrap();
    player.set_i16(0x4b64a6, 3).unwrap();
    player.set_i16(0x4b6508, 1).unwrap();
    player.set_i16(0x4b650a, 2).unwrap();
    player.set_i16(0x4b650c, 3).unwrap();
    player.set_i16(0x4b653a, 3).unwrap();
    runtime.sync_player();
    let before = (
        runtime.state.legacy_player.clone(),
        runtime.state.rng.clone(),
        runtime.state.turn,
    );
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(&mut runtime, json!({"seq":seq,"type":"openStatus"}))["ok"],
        true
    );
    assert_eq!(
        runtime.snapshot_value()["statusOverview"]["canInspectHouse"],
        true
    );
    assert_eq!(
        runtime.snapshot_value()["statusDetails"]["careerHistory"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("status.vlsave.json"), runtime.serialize()).unwrap();
    }
    let mut expected_rng = runtime.state.rng.clone();
    let mut rain = crate::transition::NativeTransition::rain(1, 2, &mut expected_rng);
    rain.tick(1000, &mut expected_rng);
    assert!(rain.finished);
    for _ in 0..2 {
        crate::transition::NativeTransition::mosaic(2, 16, &mut expected_rng)
            .tick(0, &mut expected_rng);
    }
    for page in ["house", "investment"] {
        assert_eq!(run(&mut runtime, "page", Some(page), None)["ok"], true);
        if page == "house" {
            let rng = runtime.state.rng.clone();
            assert_eq!(run(&mut runtime, "close", None, None)["ok"], false);
            assert_eq!(
                run(&mut runtime, "page", Some("investment"), None)["ok"],
                false
            );
            assert_eq!(runtime.state.rng, rng);
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"statusTick","elapsedMs":27})
                )["ok"],
                true
            );
            let state = runtime.state.clone();
            let save = runtime.serialize();
            assert_eq!(
                serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
                true
            );
            assert_eq!(runtime.state, state);
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"statusTick","elapsedMs":973})
                )["ok"],
                true
            );
        }
        assert_eq!(run(&mut runtime, "close", None, None)["ok"], true);
        assert_eq!(
            runtime.snapshot_value()["pending"]["state"]["page"],
            "overview"
        );
    }
    assert_eq!(
        run(&mut runtime, "page", Some("careerHistory"), None)["ok"],
        true
    );
    assert_eq!(run(&mut runtime, "scroll", None, Some(99))["ok"], true);
    assert_eq!(runtime.snapshot_value()["pending"]["state"]["scroll"], 2);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"statusTick","elapsedMs":115})
        )["ok"],
        true
    );
    let saved_state = runtime.state.clone();
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, saved_state);
    assert_eq!(run(&mut runtime, "page", Some("skills"), None)["ok"], true);
    assert_eq!(run(&mut runtime, "scroll", None, Some(99))["ok"], true);
    assert_eq!(runtime.snapshot_value()["pending"]["state"]["scroll"], 22);
    assert_eq!(run(&mut runtime, "close", None, None)["ok"], true);
    assert_eq!(run(&mut runtime, "close", None, None)["ok"], true);
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(
        (runtime.state.legacy_player.clone(), runtime.state.turn),
        (before.0, before.2)
    );
    assert_eq!(runtime.state.rng, expected_rng);
    assert!(runtime.snapshot_value()["statusOverview"].is_null());
    assert!(runtime.snapshot_value()["statusDetails"].is_null());
}
#[test]
fn property_purchase_previews_restore_and_resume_the_original_caller() {
    let mut runtime = real_runtime();
    runtime.state.housing = HousingState {
        style: 0,
        furniture: 0,
        investment_stage: 1,
        purchase_years: vec![2016],
    };
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i32(0x4b648a, 10_000_000)
        .unwrap();
    runtime.sync_player();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,17,30001,0,0,0]},{"words":[30001,18,30002,0,0,0]},{"words":[30002,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    let Some(HostActivity::NativeShop { shop }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let offer = shop
        .offers
        .iter()
        .position(|offer| offer.category == 0)
        .unwrap();
    let item = shop.offers[offer].clone();
    let attribute =
        [0x4b6490, 0x4b6492, 0x4b648e, 0x4b6494][item.bonus_attribute.unwrap() as usize];
    let old = runtime
        .state
        .legacy_player
        .as_ref()
        .unwrap()
        .i16(attribute)
        .unwrap();
    let old_money = runtime.state.stats.money.unwrap();
    let mut expected_rng = runtime.state.rng.clone();
    assert_eq!(next(&mut runtime, "chooseOption", offer)["ok"], true);
    assert_eq!(runtime.state.housing.furniture, 1);
    assert_eq!(
        runtime.state.stats.money,
        Some(old_money - i64::from(item.price))
    );
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(attribute)
            .unwrap(),
        old
    );
    assert_eq!(runtime.snapshot_value()["pending"]["returnTo"], "shop");
    let rng = runtime.state.rng.clone();
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"statusAction","action":"close"})
        )["ok"],
        false
    );
    assert_eq!(runtime.state.rng, rng);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"statusTick","elapsedMs":35})
        )["ok"],
        true
    );
    let state = runtime.state.clone();
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, state);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"statusTick","elapsedMs":965})
        )["ok"],
        true
    );
    let mut rain = crate::transition::NativeTransition::rain(1, 2, &mut expected_rng);
    rain.tick(1000, &mut expected_rng);
    crate::transition::NativeTransition::mosaic(2, 16, &mut expected_rng)
        .tick(0, &mut expected_rng);
    assert_eq!(runtime.state.rng, expected_rng);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"statusAction","action":"close"})
        )["ok"],
        true
    );
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(attribute)
            .unwrap(),
        (i32::from(old) + i32::from(item.bonus_amount)).min(9999) as i16
    );
    let Some(HostActivity::NativeShop { shop }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    assert!(shop.offers[offer].purchased);
    let exit = shop.offers.len();
    assert_eq!(next(&mut runtime, "chooseOption", exit)["ok"], true);
    assert!(matches!(
        runtime.state.host_activity,
        Some(HostActivity::Housing { .. })
    ));
    assert_eq!(next(&mut runtime, "chooseOption", 5)["ok"], true);
    assert_eq!(runtime.state.rng, expected_rng);
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(next(&mut runtime, "chooseOption", 0)["ok"], true);
    crate::transition::NativeTransition::mosaic(2, 16, &mut expected_rng)
        .tick(0, &mut expected_rng);
    assert_eq!(runtime.state.rng, expected_rng);
    assert_eq!(runtime.snapshot_value()["pending"]["returnTo"], "housing");
    let state = runtime.state.clone();
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, state);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"statusTick","elapsedMs":8001})
        )["ok"],
        true
    );
    assert!(runtime.state.vm.is_none());
    assert!(runtime.state.host_activity.is_none());
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(runtime.state.rng, expected_rng);
}
#[test]
fn daily_inventory_commands_preserve_equipment_preview_and_confirmed_mutations() {
    let mut runtime = real_runtime();
    runtime.state.inventory.ordinary[0] = 2;
    runtime.state.inventory.ordinary[2] = 1;
    runtime.state.inventory.special[0] = 2;
    runtime.state.carried_equipment[0] = EquipmentSlot { id: 0, level: 2 };
    runtime.state.carried_equipment[3] = EquipmentSlot { id: 3, level: 3 };
    runtime.state.known_equipment[0] = EquipmentKnowledge {
        learned: true,
        level: 2,
    };
    runtime.state.known_equipment[3] = EquipmentKnowledge {
        learned: true,
        level: 3,
    };
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b6486, 30)
        .unwrap();
    runtime.sync_player();
    let rng = runtime.state.rng.clone();
    let turn = runtime.state.turn;
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(&mut runtime, json!({"seq":seq,"type":"openInventory"}))["ok"],
        true
    );
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("inventory.vlsave.json"), runtime.serialize()).unwrap();
    }
    let run = |runtime: &mut Runtime, action: &str, index: Option<usize>, tab: Option<&str>| {
        let seq = runtime.state.last_seq + 1;
        dispatch(
            runtime,
            json!({"seq":seq,"type":"inventoryAction","action":action,"index":index,"tab":tab}),
        )
    };
    for elapsed in [101, 101] {
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"seq":seq,"type":"inventoryTick","elapsedMs":elapsed})
            )["ok"],
            true
        );
    }
    let state = runtime.snapshot_value();
    assert_eq!(state["pending"]["state"]["entries"][0]["frame"], 1);
    assert_eq!(state["pending"]["state"]["entries"][1]["frame"], 0);
    assert_eq!(run(&mut runtime, "use", None, None)["ok"], true);
    assert_eq!(runtime.state.inventory.ordinary[0], 1);
    assert!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(0x4b6486)
            .unwrap()
            > 30
    );
    assert_eq!(
        runtime.snapshot_value()["pending"]["state"]["previewOrigin"],
        json!({"x":294,"y":40})
    );
    assert_eq!(
        runtime.snapshot_value()["pending"]["state"]["entries"][0]["frame"],
        0
    );
    assert_eq!(run(&mut runtime, "drop", None, None)["ok"], true);
    let save = runtime.serialize();
    let before = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, before);
    assert_eq!(run(&mut runtime, "use", None, None)["ok"], false);
    assert_eq!(runtime.state.inventory.ordinary[0], 1);
    assert_eq!(run(&mut runtime, "cancelDrop", None, None)["ok"], true);
    assert_eq!(runtime.state.inventory.ordinary[0], 1);
    assert_eq!(
        run(&mut runtime, "tab", None, Some("equipment"))["ok"],
        true
    );
    assert_eq!(
        runtime.snapshot_value()["pending"]["state"]["previewOrigin"]["x"],
        312
    );
    assert_eq!(run(&mut runtime, "wear", None, None)["ok"], true);
    assert_eq!(
        runtime
            .state
            .battle_persistent
            .as_ref()
            .unwrap()
            .equipped_ids[0],
        0
    );
    assert_eq!(run(&mut runtime, "unequip", None, None)["ok"], true);
    assert_eq!(
        runtime
            .state
            .battle_persistent
            .as_ref()
            .unwrap()
            .equipped_ids[0],
        -1
    );
    assert_eq!(run(&mut runtime, "wear", None, None)["ok"], true);
    assert_eq!(run(&mut runtime, "drop", None, None)["ok"], true);
    assert_eq!(run(&mut runtime, "confirmDrop", None, None)["ok"], true);
    assert_eq!(
        runtime
            .state
            .battle_persistent
            .as_ref()
            .unwrap()
            .equipped_ids[0],
        -1
    );
    assert_eq!(runtime.state.carried_equipment[0].id, -1);
    assert_eq!(runtime.state.carried_equipment[3].id, 3);
    assert!(!runtime.state.known_equipment[0].learned);
    assert_eq!(
        runtime.snapshot_value()["pending"]["state"]["entries"][0]["id"],
        3
    );
    assert_eq!(run(&mut runtime, "tab", None, Some("special"))["ok"], true);
    assert_eq!(run(&mut runtime, "use", None, None)["ok"], false);
    assert_eq!(run(&mut runtime, "drop", None, None)["ok"], true);
    assert_eq!(run(&mut runtime, "confirmDrop", None, None)["ok"], true);
    assert_eq!(runtime.state.inventory.special[0], 1);
    assert_eq!(run(&mut runtime, "close", None, None)["ok"], true);
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(runtime.state.turn, turn);
    assert_eq!(runtime.state.rng, rng);
}
#[test]
fn saved_battle_and_inventory_indices_are_validated_before_restore() {
    let mut runtime = real_runtime();
    runtime.state.inventory.ordinary[0] = 1;
    assert_eq!(runtime.open_inventory(None).ok, true);
    for shape in 0..3 {
        let mut malformed: Save = serde_json::from_str(&runtime.serialize()).unwrap();
        match shape {
            0 => malformed
                .state
                .battle_persistent
                .as_mut()
                .unwrap()
                .equipped_ids
                .clear(),
            1 => {
                let Some(HostActivity::Inventory { panel }) =
                    malformed.state.host_activity.as_mut()
                else {
                    panic!()
                };
                panel.entries[0].id = 13;
            }
            _ => {
                let Some(HostActivity::Inventory { panel }) =
                    malformed.state.host_activity.as_mut()
                else {
                    panic!()
                };
                panel.selected_index = Some(99);
            }
        }
        malformed.checksum = digest(&serde_json::to_vec(&malformed.state).unwrap());
        let before = runtime.state.clone();
        assert_eq!(
            serde_json::from_str::<Value>(
                &runtime.restore(&serde_json::to_string(&malformed).unwrap())
            )
            .unwrap()["error"]["code"],
            "invalid_save"
        );
        assert_eq!(runtime.state, before);
    }
    runtime.state.host_activity = None;
    runtime.state.pending = None;
    runtime.state.phase = "playing".into();
    runtime.begin_final_battle().unwrap();
    let mut malformed: Save = serde_json::from_str(&runtime.serialize()).unwrap();
    let Some(HostActivity::FinalBattle { game }) = malformed.state.host_activity.as_mut() else {
        panic!()
    };
    game.enemy_tracks.clear();
    malformed.checksum = digest(&serde_json::to_vec(&malformed.state).unwrap());
    let before = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(
            &runtime.restore(&serde_json::to_string(&malformed).unwrap())
        )
        .unwrap()["error"]["code"],
        "invalid_save"
    );
    assert_eq!(runtime.state, before);
}
#[test]
fn native_battle_dispatches_restores_and_commits_result_once() {
    let mut runtime = real_runtime();
    let node = (0..32768)
        .find(|id| {
            runtime
                .program
                .as_ref()
                .unwrap()
                .node_words(*id)
                .is_some_and(|words| words[1] == 15)
        })
        .unwrap();
    runtime.state.vm = Some(LegacyVm::new(node));
    runtime.advance_vm(&mut vec![]);
    assert_eq!(runtime.snapshot_value()["pending"]["game"], "ordinary");
    assert!(matches!(
        runtime.state.host_activity,
        Some(HostActivity::Battle { .. })
    ));
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("battle.vlsave.json"), runtime.serialize()).unwrap();
    }
    let before = runtime.state.clone();
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, before);
    let player = runtime.state.legacy_player.as_ref().unwrap();
    let battles = player.i16(0x4b64bc).unwrap();
    let wins = player.i16(0x4b64be).unwrap();
    let total = player.i16(0x4b64f0).unwrap();
    let Some(HostActivity::Battle { game }) = runtime.state.host_activity.as_mut() else {
        panic!()
    };
    game.phase = BattlePhase::Finished;
    game.return_code = Some(1);
    game.hp = 80;
    game.money = 12345;
    game.experience = 321;
    game.persistent.visits = 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":before.last_seq+1,"type":"battleTick","elapsedMs":17})
        )["ok"],
        true
    );
    let player = runtime.state.legacy_player.as_ref().unwrap();
    assert_eq!(player.i16(0x4b64bc).unwrap(), battles + 1);
    assert_eq!(player.i16(0x4b64be).unwrap(), wins + 1);
    assert_eq!(player.i16(0x4b64f0).unwrap(), total + 1);
    assert_eq!(runtime.state.battle_persistent.as_ref().unwrap().visits, 1);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"battleTick","elapsedMs":17})
        )["ok"],
        false
    );
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(0x4b64f0)
            .unwrap(),
        total + 1
    );
    runtime.reincarnate(&mut vec![]).unwrap();
    assert!(runtime.state.battle_persistent.is_none());
}
#[test]
fn final_battle_valuation_matches_original_x86_and_enters_the_existing_ending() {
    let mut runtime = real_runtime();
    let battle_rules = runtime.battle_rules().unwrap();
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/original-final-battle.json"
    ))
    .unwrap();
    for row in vectors["valuations"].as_array().unwrap() {
        for field in row["playerFields"].as_array().unwrap() {
            let address = field[0].as_u64().unwrap() as u32;
            let value = field[1].as_i64().unwrap();
            if field[2] == 4 {
                runtime
                    .state
                    .legacy_player
                    .as_mut()
                    .unwrap()
                    .set_i32(address, value as i32)
                    .unwrap();
            } else {
                runtime
                    .state
                    .legacy_player
                    .as_mut()
                    .unwrap()
                    .set_i16(address, value as i16)
                    .unwrap();
            }
        }
        runtime.state.housing = HousingState {
            style: row["style"].as_i64().unwrap() as i8,
            furniture: row["furniture"].as_i64().unwrap() as i16,
            investment_stage: row["investmentStage"].as_u64().unwrap() as u8,
            purchase_years: vec![],
        };
        runtime.state.spouse_index = row["spouse"].as_i64().unwrap() as i8;
        let mut persistent = BattlePersistent::new(&battle_rules);
        for (enemy, wins) in persistent
            .enemies
            .iter_mut()
            .zip(row["wins"].as_array().unwrap())
        {
            enemy.wins = wins.as_i64().unwrap() as i16;
        }
        runtime.state.battle_persistent = Some(persistent);
        assert_eq!(
            serde_json::to_value(runtime.final_battle_input().unwrap().valuation).unwrap(),
            row["valuation"]
        );
    }
    let mut runtime = real_runtime();
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b649e, 6).unwrap();
    player.set_i16(0x4b64a2, 1).unwrap();
    player.set_i16(0x4b64a4, 30).unwrap();
    player.set_i16(0x4b64a6, 5).unwrap();
    player.set_i16(0x4b64f2, 3).unwrap();
    player.set_i32(0x4b648a, 500000).unwrap();
    runtime.prepare_final_history().unwrap();
    runtime.begin_final_battle().unwrap();
    assert_eq!(runtime.snapshot_value()["pending"]["game"], "final");
    assert_eq!(runtime.state.stats.money, Some(0));
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        let initial = runtime.serialize();
        std::fs::write(directory.join("final-battle.vlsave.json"), &initial).unwrap();
        for _ in 0..1000 {
            let phase = runtime.snapshot_value()["pending"]["state"]["phase"]
                .as_str()
                .unwrap()
                .to_owned();
            if phase == "shop" {
                break;
            }
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"battleAction","action":"continue"})
                )["ok"],
                true
            );
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"battleTick","elapsedMs":101})
                )["ok"],
                true
            );
        }
        assert_eq!(
            runtime.snapshot_value()["pending"]["state"]["phase"],
            "shop"
        );
        std::fs::write(
            directory.join("final-shop.vlsave.json"),
            runtime.serialize(),
        )
        .unwrap();
        for _ in 0..2 {
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"battleAction","action":"continue"})
                )["ok"],
                true
            );
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"battleTick","elapsedMs":101})
                )["ok"],
                true
            );
        }
        for _ in 0..50 {
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"battleTick","elapsedMs":17})
                )["ok"],
                true
            );
        }
        assert_eq!(
            runtime.snapshot_value()["pending"]["state"]["phase"],
            "playing"
        );
        std::fs::write(
            directory.join("final-combat.vlsave.json"),
            runtime.serialize(),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&runtime.restore(&initial)).unwrap()["ok"],
            true
        );
    }
    let before = runtime.state.clone();
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, before);
    let Some(HostActivity::FinalBattle { game }) = runtime.state.host_activity.as_mut() else {
        panic!()
    };
    game.finished = true;
    game.hp = 0;
    game.money = 22222;
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"battleTick","elapsedMs":17})
        )["ok"],
        true
    );
    assert_eq!(runtime.state.stats.money, Some(22222));
    assert_eq!(runtime.snapshot_value()["pending"]["kind"], "ending");
    let Some(HostActivity::Ending { flow }) = &runtime.state.host_activity else {
        panic!()
    };
    assert!(!flow.victory);
}
#[test]
fn map_load_decoration_draws_precede_gods_and_match_original_x86() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/original-map-load.json"
    ))
    .unwrap();
    let mut runtime = real_runtime();
    for case in cases["cases"].as_array().unwrap() {
        let map = runtime
            .maps
            .iter()
            .find(|map| map.id == case["map"].as_str().unwrap())
            .unwrap();
        let mut rng = ReplayRng::new(case["seed"].as_u64().unwrap());
        assert_eq!(
            serde_json::to_value(map.decoration_delays(&mut rng)).unwrap(),
            case["delaysMs"]
        );
        let spirits = Spirits::initialize(&map.coordinate_index, &map.start, &mut rng).unwrap();
        assert_eq!(
            serde_json::to_value(spirits.positions).unwrap(),
            case["positions"]
        );
        assert_eq!(rng.draws, case["rngDraws"].as_u64().unwrap());
        assert_eq!(u64::from(rng.state), case["rngState"].as_u64().unwrap());
        if case["map"] == "park" && case["seed"] == 1 {
            assert_eq!(runtime.state.rng, rng);
            assert_eq!(
                serde_json::to_value(&runtime.state.map_decorations.as_ref().unwrap().delays_ms)
                    .unwrap(),
                case["delaysMs"]
            );
        }
    }
    let before = runtime.state.clone();
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, before);
    runtime.transition_stage(3, &mut Vec::new()).unwrap();
    let before = runtime.state.rng.draws;
    let serial = runtime.state.map_decorations.as_ref().unwrap().load_serial;
    runtime.transition_stage(4, &mut Vec::new()).unwrap();
    assert_eq!(runtime.state.rng.draws - before, 4);
    assert_eq!(
        runtime.state.map_decorations.as_ref().unwrap().load_serial,
        serial + 1
    );
    let before = runtime.state.rng.draws;
    runtime.enter_travel(&mut Vec::new()).unwrap();
    assert_eq!(
        runtime.state.rng.draws - before,
        1 + runtime.current_map().unwrap().decoration_count() as u64
    );
    let before = runtime.state.rng.draws;
    runtime.return_from_travel(&mut Vec::new()).unwrap();
    assert_eq!(
        runtime.state.rng.draws - before,
        runtime.current_map().unwrap().decoration_count() as u64
    );
}
#[test]
fn ending_history_records_only_actual_transitions_and_resets_on_reincarnation() {
    let mut runtime = real_runtime();
    assert_eq!(runtime.ending_history()[1], [2016]);
    assert_eq!(runtime.ending_history()[2], [2019]);
    runtime.transition_stage(3, &mut Vec::new()).unwrap();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649c, 2019)
        .unwrap();
    runtime.complete_school_exam(1, &mut Vec::new()).unwrap();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649c, 2020)
        .unwrap();
    runtime.complete_school_exam(1, &mut Vec::new()).unwrap();
    assert_eq!(runtime.ending_history()[3], [2019]);
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649c, 2021)
        .unwrap();
    runtime.complete_school_exam(2, &mut Vec::new()).unwrap();
    assert_eq!(runtime.ending_history()[4], [2021]);
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649c, 2022)
        .unwrap();
    runtime.transition_stage(6, &mut Vec::new()).unwrap();
    assert_eq!(runtime.ending_history()[5], [2022]);
    runtime.choose_career(4).unwrap();
    assert!(runtime.ending_history()[6].is_empty());
    runtime.choose_career(5).unwrap();
    runtime.choose_career(5).unwrap();
    assert_eq!(runtime.ending_history()[6], [2022]);
    runtime.state.promotion_years = vec![2023, 2024];
    runtime.state.housing.purchase_years = vec![2025];
    runtime.state.relationship_years = [Some(2026), Some(2027), Some(2028)];
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b64f0, 123).unwrap();
    player.set_i16(0x4b649c, 2060).unwrap();
    runtime.prepare_final_history().unwrap();
    let history = runtime.ending_history();
    for (index, expected) in [
        (7, vec![2023, 2024]),
        (8, vec![2025]),
        (10, vec![2028]),
        (11, vec![2026]),
        (12, vec![2027]),
        (14, vec![99]),
        (15, vec![2060]),
    ] {
        assert_eq!(history[index], expected);
    }
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.ending_history(), history);
    runtime.reincarnate(&mut Vec::new()).unwrap();
    assert_eq!(runtime.ending_history(), Runtime::initial_life_history());
}
#[test]
fn original_map_facing_uses_column_then_row_and_restores_travel_heading() {
    let mut runtime = real_runtime();
    for (id, expected) in [
        ("acient", 1),
        ("business", 2),
        ("desert", 2),
        ("lost", 2),
        ("park", 0),
        ("play", 1),
        ("social", 3),
        ("study", 0),
    ] {
        assert_eq!(
            runtime
                .maps
                .iter()
                .find(|map| map.id == id)
                .unwrap()
                .initial_facing(),
            Some(expected),
            "{id}"
        );
    }
    runtime.state.map_facing = Some(3);
    let tile = runtime.state.tile_id;
    runtime.enter_travel(&mut Vec::new()).unwrap();
    assert_eq!(
        runtime.state.travel_origin.as_ref().unwrap().facing,
        Some(3)
    );
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    runtime.return_from_travel(&mut Vec::new()).unwrap();
    assert_eq!(runtime.state.map_facing, Some(3));
    assert_eq!(runtime.state.tile_id, tile);
    runtime.transition_stage(3, &mut Vec::new()).unwrap();
    runtime.state.map_facing = Some(2);
    runtime.transition_stage(4, &mut Vec::new()).unwrap();
    assert_eq!(runtime.state.map_facing, Some(2));
}
#[test]
fn student_competition_saves_settles_once_and_resumes_queued_travel() {
    for traveling in [false, true] {
        let mut runtime = real_runtime();
        let player = runtime.state.legacy_player.as_mut().unwrap();
        player.set_i16(0x4b64a8, 6).unwrap();
        player.set_i16(0x4b64aa, 6).unwrap();
        if traveling {
            player.set_i16(0x4b64b0, 1).unwrap();
        }
        runtime.state.turn_in_progress = true;
        let mut events = Vec::new();
        runtime.finish_turn(&mut events);
        assert_eq!(runtime.state.turn, 7);
        assert_eq!(
            events
                .iter()
                .filter(|event| event["type"] == "turnCompleted")
                .count(),
            1
        );
        assert!(
            !runtime
                .state
                .legacy_player
                .as_ref()
                .unwrap()
                .life()
                .traveling
        );
        assert!(matches!(
            runtime.state.host_activity,
            Some(HostActivity::Competition { .. })
        ));
        assert!(!runtime.state.turn_in_progress);
        for _ in 0..200 {
            if matches!(
                runtime.state.host_activity,
                Some(HostActivity::Competition {
                    game: StudentCompetition {
                        phase: CompetitionPhase::Menu,
                        ..
                    },
                    ..
                })
            ) {
                break;
            }
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"type":"competitionTick","seq":seq,"elapsedMs":100})
                )["ok"],
                true
            );
        }
        if !traveling && std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../output/test-fixtures/competition.vlsave.json");
            std::fs::write(path, runtime.serialize()).unwrap();
        }
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"type":"competitionAction","seq":seq,"action":"choose","index":2})
            )["ok"],
            true
        );
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"type":"competitionAction","seq":seq,"action":"continue"})
            )["ok"],
            true
        );
        let mut restored = false;
        let mut prize_events = 0;
        let mut result_events = 0;
        for _ in 0..3000 {
            let Some(HostActivity::Competition { game, .. }) = runtime.state.host_activity.as_ref()
            else {
                break;
            };
            let phase = game.phase;
            let answer = game.exam.as_ref().map(|exam| exam.correct).unwrap_or(0);
            if phase == CompetitionPhase::QuizFeedback && !restored {
                let save = runtime.serialize();
                let state = runtime.state.clone();
                assert_eq!(
                    serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
                    true
                );
                assert_eq!(runtime.state, state);
                restored = true;
            }
            let seq = runtime.state.last_seq + 1;
            let response = if phase == CompetitionPhase::QuizQuestion {
                dispatch(
                    &mut runtime,
                    json!({"type":"competitionAction","seq":seq,"action":"answer","index":answer}),
                )
            } else {
                dispatch(
                    &mut runtime,
                    json!({"type":"competitionTick","seq":seq,"elapsedMs":100}),
                )
            };
            assert_eq!(response["ok"], true);
            prize_events += response["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["type"] == "competitionPrize")
                .count();
            result_events += response["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["type"] == "competitionCompleted")
                .count();
            assert!(
                !response["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|event| event["type"] == "turnCompleted")
            );
        }
        assert_eq!(runtime.state.phase, "playing");
        assert_eq!(runtime.state.turn, 7);
        assert_eq!(runtime.state.stats.money, Some(20000));
        assert_eq!(prize_events, 1);
        assert_eq!(result_events, 1);
        assert_eq!(
            runtime
                .state
                .legacy_player
                .as_ref()
                .unwrap()
                .life()
                .traveling,
            traveling
        );
        assert!(restored);
    }
}
#[test]
fn running_competition_commands_restore_inputs_and_pay_only_once() {
    let mut runtime = real_runtime();
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b64a8, 6).unwrap();
    player.set_i16(0x4b64aa, 6).unwrap();
    runtime.state.turn_in_progress = true;
    runtime.finish_turn(&mut Vec::new());
    for _ in 0..200 {
        if matches!(
            runtime.state.host_activity,
            Some(HostActivity::Competition {
                game: StudentCompetition {
                    phase: CompetitionPhase::Menu,
                    ..
                },
                ..
            })
        ) {
            break;
        }
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"type":"competitionTick","seq":seq,"elapsedMs":100})
            )["ok"],
            true
        );
    }
    let draws = runtime.state.rng.draws;
    let action = |runtime: &mut Runtime, action: &str| {
        let seq = runtime.state.last_seq + 1;
        dispatch(
            runtime,
            json!({"type":"competitionAction","seq":seq,"action":action}),
        )
    };
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"type":"competitionAction","seq":seq,"action":"choose","index":1})
        )["ok"],
        true
    );
    for _ in 0..6 {
        assert_eq!(action(&mut runtime, "continue")["ok"], true);
    }
    assert_eq!(runtime.state.rng.draws, draws + 3);
    assert_eq!(action(&mut runtime, "left")["ok"], true);
    let save = runtime.serialize();
    let before = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, before);
    let initial_money = runtime.state.stats.money.unwrap();
    let mut prizes = Vec::new();
    let mut completed = 0;
    for frame in 0..3000 {
        let Some(HostActivity::Competition { game, .. }) = runtime.state.host_activity.as_ref()
        else {
            break;
        };
        if game.phase == CompetitionPhase::Running {
            assert_eq!(
                action(&mut runtime, if frame % 2 == 0 { "right" } else { "left" })["ok"],
                true
            );
        }
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"type":"competitionTick","seq":seq,"elapsedMs":100}),
        );
        assert_eq!(response["ok"], true);
        for event in response["events"].as_array().unwrap() {
            match event["type"].as_str() {
                Some("competitionPrize") => prizes.push(event["amount"].as_i64().unwrap()),
                Some("competitionCompleted") => completed += 1,
                Some("turnCompleted") => panic!("competition settled the turn twice"),
                _ => {}
            }
        }
    }
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(runtime.state.turn, 7);
    assert_eq!(prizes.len(), 1);
    assert_eq!(completed, 1);
    assert_eq!(runtime.state.stats.money, Some(initial_money + prizes[0]));
    assert_eq!(runtime.state.rng.draws, draws + 3);
    let money = runtime.state.stats.money;
    assert_eq!(action(&mut runtime, "left")["ok"], false);
    assert_eq!(runtime.state.stats.money, money);
}
#[test]
fn relationship_real_catalog_entry_produces_a_restorable_house() {
    let mut runtime = real_runtime();
    runtime.transition_stage(3, &mut Vec::new()).unwrap();
    runtime.refresh_spirit_positions(&mut Vec::new()).unwrap();
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b6494, 9999).unwrap();
    player.set_i16(0x4b6496, 99).unwrap();
    player.set_i32(0x4b648a, 1_000_000).unwrap();
    runtime.state.gift_level = 2;
    runtime.sync_player();
    let node = (0..32768)
        .find(|id| {
            runtime
                .program
                .as_ref()
                .unwrap()
                .node_words(*id)
                .is_some_and(|words| words[1] == 24)
        })
        .unwrap();
    runtime.state.vm = Some(LegacyVm::new(node));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(
        runtime.state.pending.as_ref().unwrap()["kind"],
        "relationship"
    );
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("relationship.vlsave.json"), save).unwrap();
    }
}
#[test]
fn spirit_encounter_pauses_landed_tile_and_saved_slots_finish_before_turn_settlement() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649e, 3)
        .unwrap();
    let tile = runtime.state.tile_id.unwrap();
    let (_, position) = runtime.spirit_map_context().unwrap();
    runtime.state.spirits.positions[2] = position.clone();
    runtime.state.last_roll_steps = 1;
    runtime.state.turn_in_progress = true;
    runtime.state.action_queue = vec![TileAction {
        tile_id: tile,
        event_id: 0x5dec,
        landed: true,
        path: vec![tile],
        spirit_checked: false,
    }];
    let mut events = Vec::new();
    runtime.run_actions(&mut events);
    assert!(matches!(
        runtime.state.host_activity,
        Some(HostActivity::SpiritEncounter { spirit: 2 })
    ));
    assert!(!events.iter().any(|event| event["type"] == "tileTriggered"));
    assert_eq!(runtime.state.spirits.active, Some(2));
    assert_eq!(runtime.state.spirits.expires_turn, 10);
    let money = runtime.state.stats.money.unwrap();
    let draws = runtime.state.rng.draws;
    next(&mut runtime, "chooseOption", 0);
    assert_eq!(runtime.state.rng.draws, draws + 4);
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("spirit-slots.vlsave.json"),
            runtime.serialize(),
        )
        .unwrap();
        let mut controlled = real_runtime();
        controlled.transition_stage(3, &mut Vec::new()).unwrap();
        controlled
            .refresh_spirit_positions(&mut Vec::new())
            .unwrap();
        controlled.state.spirits.active = Some(0);
        controlled.state.spirits.expires_turn = 8;
        controlled
            .state
            .legacy_player
            .as_mut()
            .unwrap()
            .set_i16(0x4b64d6, 0)
            .unwrap();
        controlled.sync_player();
        let save = controlled.serialize();
        assert_eq!(
            serde_json::from_str::<Value>(&controlled.restore(&save)).unwrap()["ok"],
            true
        );
        std::fs::write(directory.join("controlled-dice.vlsave.json"), save).unwrap();
    }
    let seq = runtime.state.last_seq + 1;
    dispatch(
        &mut runtime,
        json!({"seq":seq,"type":"minigameAction","action":"stop"}),
    );
    assert_eq!(runtime.state.rng.draws, draws + 5);
    let save = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    let mut triggered = 0;
    for _ in 0..5000 {
        if runtime.state.host_activity.is_none() {
            break;
        }
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameTick","elapsedMs":17}),
        );
        assert_eq!(response["ok"], true);
        triggered += response["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["type"] == "tileTriggered")
            .count();
    }
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(runtime.state.turn, 1);
    assert_eq!(triggered, 1);
    assert!(runtime.state.stats.money.unwrap() >= money);
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b64aa, 10)
        .unwrap();
    runtime.state.turn_in_progress = true;
    runtime.finish_turn(&mut Vec::new());
    assert_eq!(runtime.state.spirits.active, None);
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(0x4b64d6)
            .unwrap(),
        -1
    );
    assert_ne!(runtime.state.spirits.positions[2], position);
}
#[test]
fn relationship_meeting_saves_answers_pays_once_and_runs_all_native_followups() {
    let mut runtime = real_runtime();
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b649e, 3).unwrap();
    player.set_i16(0x4b6494, 9999).unwrap();
    player.set_i32(0x4b648a, 1_000_000).unwrap();
    runtime.sync_player();
    let mut nodes = vec![
        json!({"words":[30000,24,30001,0,0,0]}),
        json!({"words":[30001,12,0,0,0,0]}),
    ];
    for id in 4000..4008 {
        nodes.push(json!({"words":[id,12,0,0,0,0]}));
    }
    runtime.program =
        Some(LegacyProgram::from_json(&json!({"program":{"nodes":nodes}}).to_string()).unwrap());
    runtime.state.relationship_new_targets[1] = 7;
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(
        runtime.state.pending.as_ref().unwrap()["kind"],
        "relationship"
    );
    assert_eq!(relationship_action(&mut runtime, "meet", 0)["ok"], true);
    let draws = runtime.state.rng.draws;
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"relationshipTick","elapsedMs":16,"hoverIndex":0})
        )["ok"],
        true
    );
    assert_eq!(runtime.state.rng.draws, draws + 2);
    assert_eq!(
        relationship_action(&mut runtime, "selectOffer", 0)["ok"],
        true
    );
    assert_eq!(runtime.state.rng.draws, draws + 4);
    let Some(HostActivity::Relationship { house }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let person = house.selected_person.unwrap();
    let cost = i64::from(house.people[person as usize].cost);
    let answer = house.answer_positions[0];
    assert_eq!(
        relationship_action(&mut runtime, "answer", answer)["ok"],
        true
    );
    let saved = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    let Some(HostActivity::Relationship { house }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let wrong = (house.answer_positions[1] + 1) % 3;
    assert_eq!(
        relationship_action(&mut runtime, "answer", wrong)["ok"],
        true
    );
    assert_eq!(runtime.state.stats.money, Some(1_000_000 - cost));
    assert_eq!(runtime.state.relationship_candidates[0], person as i8);
    let response = relationship_action(&mut runtime, "advanceNotice", 0);
    assert_eq!(response["ok"], true);
    let completed = response["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["type"] == "relationshipCompleted")
        .unwrap();
    assert_eq!(
        completed["eventNodes"],
        json!([4000 + i32::from(person), 4007])
    );
    assert_eq!(runtime.state.relationship_years[0], Some(2016));
    assert_eq!(runtime.state.phase, "playing");
    assert!(runtime.state.vm.is_none());
}
#[test]
fn relationship_married_branch_skips_house_rng_and_marriage_exit_preserves_selection() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649e, 6)
        .unwrap();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b6496, 99)
        .unwrap();
    runtime.sync_player();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,24,30001,0,0,0]},{"words":[30001,12,0,0,0,0]},{"words":[4050,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.relationship_candidates[0] = 0;
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(
        relationship_action(&mut runtime, "marriages", 0)["ok"],
        true
    );
    assert_eq!(
        relationship_action(&mut runtime, "selectMarriage", 0)["ok"],
        true
    );
    assert_eq!(runtime.state.spouse_index, 0);
    assert_eq!(relationship_action(&mut runtime, "close", 0)["ok"], true);
    assert_eq!(runtime.state.spouse_index, 0);
    assert_eq!(runtime.state.relationship_years[2], None);
    let draws = runtime.state.rng.draws;
    runtime.state.vm = Some(LegacyVm::new(30000));
    let mut events = Vec::new();
    runtime.advance_vm(&mut events);
    assert_eq!(runtime.state.rng.draws, draws);
    assert!(runtime.state.host_activity.is_none());
    assert_eq!(runtime.state.phase, "playing");
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "relationshipCompleted"
                && event["returnKind"] == 4
                && event["eventNodes"] == json!([4050]))
    );
}
#[test]
fn native_travel_opcode_marks_mission_and_enters_the_original_travel_map() {
    let mut runtime = real_runtime();
    let original_map = runtime.state.map_id.clone();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b64cc, 999)
        .unwrap();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b64ce, 10)
        .unwrap();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,22,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.state.turn_in_progress = true;
    let mut events = Vec::new();
    runtime.advance_vm(&mut events);
    let player = runtime.state.legacy_player.as_ref().unwrap();
    assert!(player.life().traveling);
    assert_eq!(player.i16(0x4b64d0).unwrap(), 1);
    assert_eq!(
        runtime.state.travel_origin.as_ref().unwrap().map_id,
        original_map.unwrap()
    );
    assert!(events.iter().any(|event| event["type"] == "travelStarted"));
}
#[test]
fn ending_skips_unavailable_original_career_art() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649e, 6)
        .unwrap();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b64a2, -1)
        .unwrap();
    runtime.prepare_final_history().unwrap();
    runtime
        .complete_final_battle(false, &mut Vec::new())
        .unwrap();
    let Some(HostActivity::Ending { flow }) = &runtime.state.host_activity else {
        panic!()
    };
    assert_eq!(flow.phase, EndingPhase::Movie);
    assert!(flow.summary.is_none());
    let before = runtime.state.rng.clone();
    let response = dispatch(
        &mut runtime,
        json!({"seq":2,"type":"endingTick","elapsedMs":201}),
    );
    assert_eq!(response["ok"], true);
    assert_eq!(runtime.state.rng, before);
    let save = runtime.serialize();
    let expected = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, expected);
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649e, 5)
        .unwrap();
    runtime
        .complete_final_battle(true, &mut Vec::new())
        .unwrap();
    let Some(HostActivity::Ending { flow }) = &runtime.state.host_activity else {
        panic!()
    };
    assert_eq!(flow.phase, EndingPhase::Summary);
    assert_eq!(flow.summary.as_ref().unwrap().career, 25);
}
#[test]
fn ending_defeat_reincarnates_and_victory_can_complete_or_continue() {
    fn finish_presentation(runtime: &mut Runtime) {
        for _ in 0..100 {
            let Some(HostActivity::Ending { flow }) = &runtime.state.host_activity else {
                return;
            };
            let action = match flow.phase {
                EndingPhase::Reincarnation => return,
                EndingPhase::Summary | EndingPhase::EndImage => "continue",
                _ => "escape",
            };
            let result = dispatch(
                runtime,
                json!({"seq":runtime.state.last_seq+1,"type":"endingAction","action":action}),
            );
            assert_eq!(result["ok"], true, "{result}");
            let save = runtime.serialize();
            let expected = runtime.state.clone();
            assert_eq!(
                serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
                true
            );
            assert_eq!(runtime.state, expected);
            let result = dispatch(
                runtime,
                json!({"seq":runtime.state.last_seq+1,"type":"endingTick","elapsedMs":1000}),
            );
            assert_eq!(result["ok"], true, "{result}");
        }
        panic!("Ending presentation did not finish");
    }
    let mut runtime = real_runtime();
    let player = runtime.state.legacy_player.as_mut().unwrap();
    player.set_i16(0x4b649e, 6).unwrap();
    player.set_i16(0x4b64a2, 1).unwrap();
    player.set_i16(0x4b64a6, 5).unwrap();
    player.set_i16(0x4b6488, 1000).unwrap();
    runtime.state.career_levels[1] = 5;
    runtime.state.career_levels[2] = 4;
    runtime.state.career_history.push(json!({"career":1}));
    runtime.state.promotion_years.push(2050);
    runtime.state.inventory.special[0] = 5;
    runtime.state.inventory.ordinary[0] = 5;
    runtime.state.spouse_index = 3;
    runtime.state.relationship_candidates[0] = 2;
    runtime.state.gift_level = 4;
    runtime.state.fortune_visits = 11;
    runtime.state.shop_visits = 2;
    runtime.state.housing.style = 1;
    runtime.state.housing.investment_stage = 12;
    runtime.state.known_equipment[0] = EquipmentKnowledge {
        learned: true,
        level: 2,
    };
    runtime.state.carried_equipment[0] = EquipmentSlot { id: 0, level: 2 };
    runtime.state.first_equipment_visit = false;
    runtime.prepare_final_history().unwrap();
    runtime
        .complete_final_battle(false, &mut Vec::new())
        .unwrap();
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("ending-loss.vlsave.json"),
            runtime.serialize(),
        )
        .unwrap();
    }
    finish_presentation(&mut runtime);
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(
        runtime.state.legacy_player.as_ref().unwrap().life().stage,
        2
    );
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .life()
            .incarnation,
        2
    );
    assert_eq!(
        runtime.state.legacy_player.as_ref().unwrap().life().skills[1],
        3
    );
    assert_eq!(runtime.state.career_levels[1], 5);
    assert_eq!(runtime.state.career_levels[2], 1);
    assert_eq!(runtime.state.inventory, Inventory::default());
    assert!(runtime.state.career_history.is_empty());
    assert!(runtime.state.promotion_years.is_empty());
    assert_eq!(runtime.state.spouse_index, -1);
    assert!(
        runtime
            .state
            .relationship_candidates
            .iter()
            .all(|id| *id == -1)
    );
    assert_eq!(runtime.state.gift_level, 1);
    assert_eq!(runtime.state.fortune_visits, 0);
    assert_eq!(runtime.state.housing, HousingState::default());
    assert!(
        runtime
            .state
            .known_equipment
            .iter()
            .all(|item| !item.learned && item.level == 0)
    );
    assert!(
        runtime
            .state
            .carried_equipment
            .iter()
            .all(|item| item.id == -1)
    );
    assert!(runtime.state.first_equipment_visit);
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b649e, 6)
        .unwrap();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b64a2, 1)
        .unwrap();
    runtime.prepare_final_history().unwrap();
    runtime
        .complete_final_battle(true, &mut Vec::new())
        .unwrap();
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("ending-win.vlsave.json"),
            runtime.serialize(),
        )
        .unwrap();
    }
    finish_presentation(&mut runtime);
    let saved = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    assert_eq!(next(&mut runtime, "chooseOption", 1)["ok"], true);
    assert_eq!(runtime.state.phase, "completed");
    let saved = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    assert_eq!(
        next(&mut runtime, "rollDice", 0)["error"]["code"],
        "cannot_roll"
    );
    runtime
        .complete_final_battle(true, &mut Vec::new())
        .unwrap();
    finish_presentation(&mut runtime);
    next(&mut runtime, "chooseOption", 0);
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .life()
            .incarnation,
        3
    );
}
#[test]
fn fire_commands_commit_charm_when_reward_starts_and_preserve_saved_animation() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b6494, 1000)
        .unwrap();
    runtime.sync_player();
    let seed = (0..100)
        .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 3)
        .unwrap();
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let mut fixture = real_runtime();
        fixture.state.rng = ReplayRng::new(seed);
        let node = (0..32768)
            .find(|id| {
                fixture
                    .program
                    .as_ref()
                    .unwrap()
                    .node_words(*id)
                    .is_some_and(|words| words[1] == 23)
            })
            .unwrap();
        fixture.state.vm = Some(LegacyVm::new(node));
        fixture.advance_vm(&mut Vec::new());
        assert_eq!(fixture.state.pending.as_ref().unwrap()["game"], "fire");
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("fire.vlsave.json"), fixture.serialize()).unwrap();
    }
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.rng = ReplayRng::new(seed);
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameAction","action":"start"})
        )["ok"],
        true
    );
    let mut rewarded = false;
    let mut completed = false;
    for _ in 0..2000 {
        let Some(HostActivity::Fire { game }) = runtime.state.host_activity.as_ref() else {
            panic!()
        };
        if game.win_stage == 0 {
            let action = if game.hand { "right" } else { "left" };
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"minigameAction","action":action})
                )["ok"],
                true
            );
        }
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameTick","elapsedMs":17}),
        );
        assert_eq!(response["ok"], true);
        if response["snapshot"]["pending"]["state"]["rewardStarted"] == true && !rewarded {
            assert_eq!(runtime.state.stats.charm, Some(1100));
            let save = runtime.serialize();
            let state = runtime.state.clone();
            assert_eq!(
                serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
                true
            );
            assert_eq!(runtime.state, state);
            rewarded = true;
        }
        if let Some(event) = response["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["type"] == "minigameCompleted" && event["game"] == "fire")
        {
            assert_eq!(event["charmDelta"], 100);
            assert_eq!(event["failed"], false);
            completed = true;
            break;
        }
    }
    assert!(rewarded && completed);
    assert!(runtime.state.host_activity.is_none());
    assert!(runtime.state.vm.is_none());
    assert_eq!(runtime.state.phase, "playing");
}
#[test]
fn gas_commands_restore_during_a_throw_and_resume_after_three_successes() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b648e, 1000)
        .unwrap();
    runtime.sync_player();
    let seed = (0..100)
        .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 0)
        .unwrap();
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let mut fixture = real_runtime();
        fixture.state.rng = ReplayRng::new(seed);
        let node = (0..32768)
            .find(|id| {
                fixture
                    .program
                    .as_ref()
                    .unwrap()
                    .node_words(*id)
                    .is_some_and(|words| words[1] == 23)
            })
            .unwrap();
        fixture.state.vm = Some(LegacyVm::new(node));
        fixture.advance_vm(&mut Vec::new());
        assert_eq!(fixture.state.pending.as_ref().unwrap()["game"], "gas");
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("gas.vlsave.json"), fixture.serialize()).unwrap();
    }
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.rng = ReplayRng::new(seed);
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(runtime.state.pending.as_ref().unwrap()["game"], "gas");
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameAction","action":"start"})
        )["ok"],
        true
    );
    let mut saved = false;
    let mut completed = false;
    for _ in 0..3000 {
        let Some(HostActivity::Gas { game }) = runtime.state.host_activity.as_ref() else {
            panic!()
        };
        if game.phase == GasPhase::Hit && !saved {
            let state = runtime.state.clone();
            let save = runtime.serialize();
            assert_eq!(
                serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
                true
            );
            assert_eq!(runtime.state, state);
            saved = true;
        }
        let Some(HostActivity::Gas { game }) = runtime.state.host_activity.as_ref() else {
            panic!()
        };
        if game.phase == GasPhase::Playing
            && !game.ending_pending
            && game.player_state != 1
            && (261..=280).contains(&game.targets[game.target_state].x)
        {
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"seq":seq,"type":"minigameAction","action":"throw"})
                )["ok"],
                true
            );
        }
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameTick","elapsedMs":17}),
        );
        assert_eq!(response["ok"], true);
        if let Some(event) = response["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["type"] == "minigameCompleted" && event["game"] == "gas")
        {
            assert_eq!(event["successes"], 3);
            assert_eq!(event["staminaDelta"], 100);
            completed = true;
            break;
        }
    }
    assert!(saved && completed);
    assert!(runtime.state.host_activity.is_none());
    assert!(runtime.state.vm.is_none());
    assert_eq!(runtime.state.phase, "playing");
    assert!(runtime.state.gas_target_state.is_some());
}
#[test]
fn firebee_commands_save_inputs_pause_and_pay_after_reward() {
    let mut runtime = real_runtime();
    let seed = (0..100)
        .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 4)
        .unwrap();
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let mut fixture = real_runtime();
        fixture.state.rng = ReplayRng::new(seed);
        let node = (0..32768)
            .find(|id| {
                fixture
                    .program
                    .as_ref()
                    .unwrap()
                    .node_words(*id)
                    .is_some_and(|words| words[1] == 23)
            })
            .unwrap();
        fixture.state.vm = Some(LegacyVm::new(node));
        fixture.advance_vm(&mut Vec::new());
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("firebee.vlsave.json"), fixture.serialize()).unwrap();
    }
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.rng = ReplayRng::new(seed);
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(runtime.state.pending.as_ref().unwrap()["game"], "firebee");
    let action = |runtime: &mut Runtime, value: Value| {
        let mut value = value;
        value["type"] = json!("minigameAction");
        value["seq"] = json!(runtime.state.last_seq + 1);
        dispatch(runtime, value)
    };
    assert_eq!(action(&mut runtime, json!({"action":"start"}))["ok"], true);
    assert_eq!(
        action(
            &mut runtime,
            json!({"action":"input","left":true,"fire":true})
        )["ok"],
        true
    );
    let save = runtime.serialize();
    let state = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, state);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"type":"minigameTick","seq":seq,"elapsedMs":400})
        )["ok"],
        true
    );
    assert_eq!(action(&mut runtime, json!({"action":"help"}))["ok"], true);
    let Some(HostActivity::Firebee { game }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let player_x = game.player_x;
    let row_y = game.row_y;
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"type":"minigameTick","seq":seq,"elapsedMs":5000})
        )["ok"],
        true
    );
    let Some(HostActivity::Firebee { game }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    assert_eq!(game.player_x, player_x);
    assert_eq!(game.row_y, row_y);
    assert_eq!(action(&mut runtime, json!({"action":"start"}))["ok"], true);
    let Some(HostActivity::Firebee { game }) = runtime.state.host_activity.as_mut() else {
        panic!()
    };
    game.finished = true;
    game.score = 12;
    let mut completed = false;
    let mut reward_seen = false;
    for _ in 0..1000 {
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"type":"minigameTick","seq":seq,"elapsedMs":100}),
        );
        assert_eq!(response["ok"], true);
        if response["snapshot"]["pending"]["state"]["phase"] == "reward" {
            reward_seen = true;
            assert_eq!(runtime.state.stats.money, Some(10000));
        }
        if let Some(event) = response["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["type"] == "minigameCompleted" && event["game"] == "firebee")
        {
            assert_eq!(event["moneyDelta"], 132);
            completed = true;
            break;
        }
    }
    assert!(reward_seen && completed);
    assert_eq!(runtime.state.stats.money, Some(10132));
    assert!(runtime.state.vm.is_none());
    assert_eq!(runtime.state.phase, "playing");
}
#[test]
fn fist_commands_save_held_inputs_and_settle_only_after_original_reward() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i16(0x4b6490, 1000)
        .unwrap();
    runtime.sync_player();
    if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some() {
        let mut fixture = real_runtime();
        fixture.state.rng = ReplayRng::new(
            (0..100)
                .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 5)
                .unwrap(),
        );
        let node = (0..32768)
            .find(|id| {
                fixture
                    .program
                    .as_ref()
                    .unwrap()
                    .node_words(*id)
                    .is_some_and(|words| words[1] == 23)
            })
            .unwrap();
        fixture.state.vm = Some(LegacyVm::new(node));
        fixture.advance_vm(&mut Vec::new());
        assert_eq!(fixture.state.pending.as_ref().unwrap()["game"], "fist");
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("fist.vlsave.json"), fixture.serialize()).unwrap();
    }
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.rng = ReplayRng::new(
        (0..100)
            .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 5)
            .unwrap(),
    );
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(runtime.state.pending.as_ref().unwrap()["game"], "fist");
    let action = |runtime: &mut Runtime, value: Value| {
        let mut value = value;
        value["type"] = json!("minigameAction");
        value["seq"] = json!(runtime.state.last_seq + 1);
        dispatch(runtime, value)
    };
    assert_eq!(action(&mut runtime, json!({"action":"start"}))["ok"], true);
    assert_eq!(
        action(&mut runtime, json!({"action":"input","left":true}))["ok"],
        true
    );
    let saved = runtime.serialize();
    let state = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, state);
    let seq = runtime.state.last_seq + 1;
    let response = dispatch(
        &mut runtime,
        json!({"type":"minigameTick","seq":seq,"elapsedMs":16}),
    );
    assert_eq!(response["snapshot"]["pending"]["state"]["playerX"], 256);
    assert_eq!(action(&mut runtime, json!({"action":"help"}))["ok"], true);
    let seq = runtime.state.last_seq + 1;
    let response = dispatch(
        &mut runtime,
        json!({"type":"minigameTick","seq":seq,"elapsedMs":5000}),
    );
    assert_eq!(
        response["snapshot"]["pending"]["state"]["remainingSeconds"],
        60
    );
    assert_eq!(action(&mut runtime, json!({"action":"start"}))["ok"], true);
    let Some(HostActivity::Fist { game }) = runtime.state.host_activity.as_mut() else {
        panic!()
    };
    game.remaining_seconds = 1;
    game.score = 20;
    let mut completed = false;
    let mut reward_seen = false;
    for _ in 0..1000 {
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"type":"minigameTick","seq":seq,"elapsedMs":101}),
        );
        assert_eq!(response["ok"], true);
        if response["snapshot"]["pending"]["state"]["phase"] == "reward" {
            reward_seen = true;
            assert_eq!(runtime.state.stats.morality, Some(1000));
        }
        if let Some(event) = response["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["type"] == "minigameCompleted" && event["game"] == "fist")
        {
            assert_eq!(event["moralityDelta"], 30);
            completed = true;
            break;
        }
    }
    assert!(reward_seen && completed);
    assert!(runtime.state.host_activity.is_none());
    assert!(runtime.state.vm.is_none());
    assert_eq!(runtime.state.phase, "playing");
}
#[test]
fn hammer_commands_preserve_a_saved_game_and_resume_the_native_event() {
    let mut runtime = real_runtime();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.rng = ReplayRng::new(
        (0..100)
            .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 2)
            .unwrap(),
    );
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(runtime.state.pending.as_ref().unwrap()["game"], "hammer");
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameAction","action":"start"})
        )["ok"],
        true
    );
    let Some(HostActivity::Hammer { game }) = runtime.state.host_activity.as_mut() else {
        panic!()
    };
    game.targets[0].phase = 2;
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameAction","action":"strikeIndex","index":0})
        )["snapshot"]["pending"]["state"]["score"],
        1
    );
    let saved = runtime.serialize();
    let expected = runtime.state.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    assert_eq!(runtime.state, expected);
    let seq = runtime.state.last_seq + 1;
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameTick","elapsedMs":45501})
        )["snapshot"]["pending"]["state"]["phase"],
        "lost"
    );
    let mut completed = false;
    for _ in 0..33 {
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"seq":seq,"type":"minigameTick","elapsedMs":31}),
        );
        assert_eq!(response["ok"], true);
        completed |= response["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["type"] == "minigameCompleted" && event["game"] == "hammer");
    }
    assert!(completed);
    assert!(runtime.state.host_activity.is_none());
    assert!(runtime.state.vm.is_none());
    assert_eq!(runtime.state.phase, "playing");
}
#[test]
fn original_equipment_purchase_then_pickup_preserves_separate_ownership() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i32(0x4b648a, 1_000_000)
        .unwrap();
    runtime.sync_player();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,27,30001,0,0,0]},{"words":[30001,17,30002,0,0,0]},{"words":[30002,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    let Some(HostActivity::EquipmentWorkshop { workshop, .. }) =
        runtime.state.host_activity.as_ref()
    else {
        panic!()
    };
    let id = workshop.offered_id;
    assert_eq!(next(&mut runtime, "chooseOption", 0)["ok"], true);
    assert!(runtime.state.known_equipment[id].learned);
    assert!(
        runtime
            .state
            .carried_equipment
            .iter()
            .all(|slot| slot.id == -1)
    );
    let saved = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    assert_eq!(next(&mut runtime, "chooseOption", 0)["ok"], true);
    let Some(HostActivity::NativeShop { shop }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let offer = shop
        .offers
        .iter()
        .position(|offer| offer.category == 1 && offer.inventory_id == id as i16)
        .unwrap();
    let previous = runtime.state.stats.money;
    assert_eq!(next(&mut runtime, "chooseOption", offer)["ok"], true);
    assert_eq!(runtime.state.stats.money, previous);
    assert_eq!(runtime.state.carried_equipment[0].id, id as i16);
}
#[test]
fn original_fortune_cancel_has_no_stat_effect_and_counts_the_visit() {
    let mut runtime = real_runtime();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,25,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    runtime.state.rng = ReplayRng::new(
        (0..100)
            .find(|seed| ReplayRng::new(*seed).next_rand() % 50 == 0)
            .unwrap(),
    );
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    let Some(HostActivity::Fortune { game }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let cancel = game.record.questions[0].options.len();
    let previous = runtime.state.stats.clone();
    let saved = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
    let result = next(&mut runtime, "chooseOption", cancel);
    assert_eq!(result["snapshot"]["phase"], "playing");
    assert_eq!(runtime.state.stats, previous);
    assert_eq!(runtime.state.fortune_visits, 1);
}
#[test]
fn tracking_commands_restore_mid_shuffle_and_resume_the_original_event() {
    let mut runtime = real_runtime();
    runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
    let seed = (0..100)
        .find(|seed| ReplayRng::new(*seed).next_rand() % 6 == 1)
        .unwrap();
    runtime.state.rng = ReplayRng::new(seed);
    runtime.state.vm = Some(LegacyVm::new(30000));
    runtime.advance_vm(&mut Vec::new());
    assert_eq!(runtime.state.pending.as_ref().unwrap()["kind"], "minigame");
    let initial = runtime.state.stats.intelligence.unwrap();
    for round in 1..=3 {
        let mut ticks = 0;
        loop {
            let Some(HostActivity::Tracking { game }) = runtime.state.host_activity.as_ref() else {
                panic!()
            };
            if game.phase == TrackingPhase::Memorize {
                break;
            }
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"type":"minigameTick","seq":seq,"elapsedMs":16})
                )["ok"],
                true
            );
            ticks += 1;
            assert!(ticks < 400);
        }
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"type":"minigameAction","seq":seq,"action":"startShuffle"})
            )["ok"],
            true
        );
        let save = runtime.serialize();
        let before = runtime.state.clone();
        assert_eq!(
            serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],
            true
        );
        assert_eq!(runtime.state, before);
        ticks = 0;
        let answer = loop {
            let Some(HostActivity::Tracking { game }) = runtime.state.host_activity.as_ref() else {
                panic!()
            };
            if game.phase == TrackingPhase::Choose {
                break game.target;
            }
            let seq = runtime.state.last_seq + 1;
            assert_eq!(
                dispatch(
                    &mut runtime,
                    json!({"type":"minigameTick","seq":seq,"elapsedMs":16})
                )["ok"],
                true
            );
            ticks += 1;
            assert!(ticks < 500);
        };
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"type":"minigameAction","seq":seq,"action":"choose","index":answer})
            )["ok"],
            true
        );
        let seq = runtime.state.last_seq + 1;
        let response = dispatch(
            &mut runtime,
            json!({"type":"minigameAction","seq":seq,"action":"nextRound"}),
        );
        assert_eq!(response["ok"], true);
        if round < 3 {
            assert_eq!(response["snapshot"]["pending"]["state"]["round"], round + 1);
        } else {
            assert_eq!(response["snapshot"]["pending"]["state"]["phase"], "reward");
        }
    }
    let draws = runtime.state.rng.draws;
    for _ in 0..500 {
        if runtime.state.host_activity.is_none() {
            break;
        }
        let seq = runtime.state.last_seq + 1;
        assert_eq!(
            dispatch(
                &mut runtime,
                json!({"type":"minigameTick","seq":seq,"elapsedMs":17})
            )["ok"],
            true
        );
    }
    assert!(runtime.state.rng.draws > draws);
    assert_eq!(runtime.state.phase, "playing");
    assert_eq!(
        runtime.state.stats.intelligence,
        Some(initial + initial / 10)
    );
    assert!(runtime.state.vm.is_none());
}
#[test]
fn original_attribute_shop_enforces_payment_capacity_and_saved_sold_state() {
    let mut runtime = real_runtime();
    runtime
        .state
        .legacy_player
        .as_mut()
        .unwrap()
        .set_i32(0x4b648a, 100_000)
        .unwrap();
    runtime.sync_player();
    runtime.begin_stat_shop().unwrap();
    let before = runtime.state.stats.clone();
    let Some(HostActivity::StatShop { shop }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let offer = shop.offers[0].clone();
    assert_eq!(next(&mut runtime, "chooseOption", 0)["ok"], true);
    assert_eq!(
        runtime.state.stats.money,
        Some(before.money.unwrap() - i64::from(offer.item.price))
    );
    assert_eq!(
        runtime.state.stats.charm,
        Some(before.charm.unwrap() + i64::from(offer.item.amount))
    );
    assert_eq!(
        next(&mut runtime, "chooseOption", 0)["error"]["code"],
        "cannot_purchase"
    );
    assert_eq!(next(&mut runtime, "chooseOption", 2)["ok"], true);
    assert_eq!(next(&mut runtime, "chooseOption", 4)["ok"], true);
    assert_eq!(
        next(&mut runtime, "chooseOption", 6)["error"]["code"],
        "cannot_purchase"
    );
    let saved = runtime.serialize();
    assert_eq!(
        serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
        true
    );
}
#[test]
fn original_graduation_exam_passes_to_university_and_preserves_extra_question_draw() {
    let mut runtime = real_runtime();
    runtime.transition_stage(3, &mut Vec::new()).unwrap();
    runtime.begin_school_exam().unwrap();
    assert_eq!(next(&mut runtime, "chooseOption", 0)["ok"], true);
    for _ in 0..9 {
        let Some(HostActivity::SchoolExam { exam }) = runtime.state.host_activity.as_ref() else {
            panic!()
        };
        let answer = exam.correct;
        assert_eq!(next(&mut runtime, "chooseOption", answer)["ok"], true);
        let saved = runtime.serialize();
        assert_eq!(
            serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],
            true
        );
    }
    let Some(HostActivity::SchoolExam { exam }) = runtime.state.host_activity.as_ref() else {
        panic!()
    };
    let answer = exam.correct;
    let previous_draws = runtime.state.rng.draws;
    let result = next(&mut runtime, "chooseOption", answer);
    assert_eq!(result["snapshot"]["life"]["stage"], 4);
    assert_eq!(result["snapshot"]["mapId"], "study");
    assert_eq!(
        runtime
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(0x4b64b8)
            .unwrap(),
        1
    );
    assert!(runtime.state.rng.draws >= previous_draws + 2);
}
#[test]
fn ordered_commands_and_explicit_graph_edges() {
    let mut runtime = Runtime::new(CONTENT).unwrap();
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":2,"type":"exploreMap","mapId":"test"})
        )["error"]["code"],
        "out_of_order"
    );
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":1,"type":"exploreMap","mapId":"test"})
        )["snapshot"]["tileId"],
        0
    );
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":2,"type":"followLink","branch":0})
        )["snapshot"]["tileId"],
        1
    );
    assert_eq!(
        dispatch(
            &mut runtime,
            json!({"seq":3,"type":"followLink","branch":1})
        )["error"]["code"],
        "invalid_link"
    );
    assert_eq!(runtime.state.turn, 0);
}
#[test]
fn save_round_trip_and_corruption_are_atomic() {
    let mut original = Runtime::new(CONTENT).unwrap();
    dispatch(
        &mut original,
        json!({"seq":1,"type":"exploreMap","mapId":"test"}),
    );
    let save = original.serialize();
    let mut restored = Runtime::new(CONTENT).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&restored.restore(&save)).unwrap()["ok"],
        true
    );
    assert_eq!(original.state, restored.state);
    let before = restored.state.clone();
    let broken = save.replace("\"turn\":0", "\"turn\":7");
    assert_eq!(
        serde_json::from_str::<Value>(&restored.restore(&broken)).unwrap()["error"]["code"],
        "save_checksum"
    );
    assert_eq!(before, restored.state);
    for shape in 0..3 {
        let mut malformed: Save = serde_json::from_str(&save).unwrap();
        match shape {
            0 => malformed.state.inventory.ordinary.clear(),
            1 => malformed.state.carried_equipment[0].id = 18,
            _ => malformed.state.housing.style = 4,
        }
        malformed.checksum = digest(&serde_json::to_vec(&malformed.state).unwrap());
        assert_eq!(
            serde_json::from_str::<Value>(
                &restored.restore(&serde_json::to_string(&malformed).unwrap())
            )
            .unwrap()["error"]["code"],
            "invalid_save"
        );
        assert_eq!(before, restored.state);
    }
}
#[test]
fn unsupported_gameplay_does_not_invent_state() {
    let mut runtime = Runtime::new(CONTENT).unwrap();
    let result = dispatch(
        &mut runtime,
        json!({"seq":1,"type":"newGame","seed":1,"profile":{"name":"测试","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}}),
    );
    assert_eq!(result["error"]["code"], "missing_events");
    assert_eq!(runtime.state.stats, Stats::default());
    assert!(runtime.state.player.is_none());
    assert_eq!(runtime.state.rng.draws, 0);
}
