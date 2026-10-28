use serde_json::Value;
use virtual_life_core::{final_battle::{FinalBattleGame, FinalBattleInput, FinalBattlePhase, FinalBattleRules}, rng::ReplayRng};

#[test]
fn final_intro_and_shop_frames_match_original_x86() {
    let catalog: Value = serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
    let rules: FinalBattleRules = serde_json::from_value(catalog["finalBattle"].clone()).unwrap();
    let fixture: Value = serde_json::from_str(include_str!("../../../tests/fixtures/original-final-intro-shop.json")).unwrap();
    for kind in ["intro", "shop"] {
        for (case, row) in fixture[kind].as_array().unwrap().iter().enumerate() {
            let input: FinalBattleInput = if kind == "intro" { serde_json::from_value(row["input"].clone()).unwrap() }
                else { FinalBattleInput { sex: 0, career: -1, life_level: 1, equipped_count: 0, valuation: [0; 14] } };
            let mut game = FinalBattleGame::new(input, &rules).unwrap();
            game.clock_ms = row["initialClock"].as_u64().unwrap_or(0) as u32;
            game.intro_started_ms = game.clock_ms;
            if kind == "shop" {
                game.phase = FinalBattlePhase::Shop;
                game.money = row["money"].as_i64().unwrap() as i32;
                game.items = serde_json::from_value(row["items"].clone()).unwrap();
                game.item_scroll = row["itemScroll"].as_u64().unwrap_or(0) as usize;
                if let Some(track) = row.get("introTrack") { game.intro_tracks[1] = serde_json::from_value(track.clone()).unwrap(); }
            }
            let mut rng = ReplayRng::new(42);
            for (frame, input) in row["frames"].as_array().unwrap().iter().enumerate() {
                if let Some(action) = input["action"].as_str() {
                    game.action(action, input["index"].as_u64().map(|id| id as usize), &rules)
                        .unwrap_or_else(|error| panic!("{kind} case {case} frame {frame}: {error}"));
                }
                let restored = if frame % 17 == 1 { Some(serde_json::from_str::<FinalBattleGame>(&serde_json::to_string(&game).unwrap()).unwrap()) } else { None };
                game.tick(input["elapsedMs"].as_u64().unwrap() as u32, &rules, &mut rng).unwrap();
                if let Some(mut restored) = restored {
                    restored.tick(input["elapsedMs"].as_u64().unwrap() as u32, &rules, &mut ReplayRng::new(42)).unwrap();
                    assert_eq!(game, restored, "{kind} case {case} frame {frame} save/resume");
                }
                let actual = serde_json::to_value(&game).unwrap();
                for (key, expected) in input["after"].as_object().unwrap() {
                    assert_eq!(&actual[key], expected, "{kind} case {case} frame {frame} field {key}");
                }
                assert_eq!(rng.draws, 0, "The original introduction/shop does not draw random numbers");
            }
        }
    }
}
