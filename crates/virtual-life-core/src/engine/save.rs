//! Fresh Bevy save format. Legacy website and Windows saves are rejected.
use super::*;
pub(super) const SAVE_VERSION: u32 = 1;
pub(super) const SAVE_FORMAT: &str = "virtual-life-bevy-save";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Save {
    pub(super) format: String,
    pub(super) version: u32,
    pub(super) content_hash: String,
    pub(super) checksum: String,
    pub(super) state: State,
}

impl GameSession {
    pub fn save(&self) -> String {
        let state = serde_json::to_vec(&self.state).unwrap();
        serde_json::to_string(&Save {
            format: SAVE_FORMAT.into(),
            version: SAVE_VERSION,
            content_hash: self.content_hash.clone(),
            checksum: digest(&state),
            state: self.state.clone(),
        })
        .unwrap()
    }

    /// Check a domain save without changing gameplay or its random stream.
    pub fn validate_save(&self, save_json: &str) -> Result<(), GameError> {
        self.parse_save(save_json)
            .map(|_| ())
            .map_err(|(code, message)| GameError {
                code: code.into(),
                message,
            })
    }

    pub fn restore(&mut self, save_json: &str) -> GameResponse {
        match self.parse_save(save_json) {
            Ok(save) => {
                self.state = save.state;
                self.response(None, vec![json!({"type":"stateRestored"})])
            }
            Err(error) => self.response(Some(error), vec![]),
        }
    }

    fn parse_save(&self, save_json: &str) -> Result<Save, (&'static str, String)> {
        let save: Save = match serde_json::from_str(save_json) {
            Ok(save) => save,
            Err(e) => return Err(("invalid_save", e.to_string())),
        };
        if save.format != SAVE_FORMAT
            || save.version != SAVE_VERSION
            || save.state.schema_version != SAVE_VERSION
        {
            return Err(("save_version", "不支持此存档版本。".into()));
        }
        if save.content_hash != self.content_hash {
            return Err(("content_mismatch", "存档与当前原版资源清单不匹配。".into()));
        }
        if save.checksum != digest(&serde_json::to_vec(&save.state).unwrap()) {
            return Err(("save_checksum", "存档校验失败。".into()));
        }
        if ![
            "ready",
            "exploring",
            "playing",
            "event",
            "blocked",
            "completed",
        ]
        .contains(&save.state.phase.as_str())
        {
            return Err(("invalid_save", "此存档状态无法识别。".into()));
        }
        if save
            .state
            .battle_persistent
            .as_ref()
            .is_some_and(|persistent| !valid_battle_persistent(persistent))
            || !self.valid_saved_activity(&save.state)
        {
            return Err(("invalid_save", "存档的战斗或物品栏记录不完整。".into()));
        }
        if save.state.inventory.ordinary.len() != 13
            || save.state.inventory.special.len() != 27
            || save.state.career_levels.len() != 25
            || save.state.gas_target_state.is_some_and(|state| state > 3)
            || save.state.map_facing.is_some_and(|facing| facing > 3)
            || save.state.relationship_candidates.len() != 8
            || save.state.known_equipment.len() != 18
            || save.state.carried_equipment.len() != 8
            || save
                .state
                .carried_equipment
                .iter()
                .any(|slot| !(-1..18).contains(&slot.id) || !(0..=4).contains(&slot.level))
            || save.state.known_equipment.iter().any(|item| item.level > 4)
            || save
                .state
                .active_mission
                .as_ref()
                .is_some_and(|mission| mission.words.len() != 11)
        {
            return Err(("invalid_save", "存档的道具、职业或任务记录不完整。".into()));
        }
        if !(-1..=3).contains(&save.state.housing.style)
            || !(0..=15).contains(&save.state.housing.furniture)
            || save.state.housing.investment_stage > 13
            || save.state.housing.style == 3 && save.state.housing.furniture > 14
            || save.state.housing.style == -1
                && (save.state.housing.furniture != 0 || save.state.housing.investment_stage != 0)
            || save.state.housing.style >= 0 && save.state.housing.investment_stage == 0
        {
            return Err(("invalid_save", "存档的住宅状态无效。".into()));
        }
        match (&save.state.map_id, save.state.tile_id) {
            (Some(id), Some(tile))
                if self
                    .maps
                    .iter()
                    .any(|map| &map.id == id && tile < map.chips.len()) => {}
            (None, None) if save.state.phase == "ready" => {}
            _ => {
                return Err(("invalid_save", "存档地图或坐标无效。".into()));
            }
        }
        if let Some(player) = &save.state.legacy_player {
            if !player.valid()
                || save
                    .state
                    .player
                    .as_ref()
                    .is_none_or(|profile| profile.validate().is_err())
                || !["playing", "event", "blocked", "completed"]
                    .contains(&save.state.phase.as_str())
            {
                return Err(("invalid_save", "存档角色资料无效。".into()));
            }
            if save.state.stats != player.stats()
                || save.state.turn != player.i16(0x4b64aa).unwrap().max(0) as u32
            {
                return Err(("invalid_save", "存档参数与角色状态不一致。".into()));
            }
            if save.state.phase == "event"
                && ((save.state.vm.is_none()
                    && save.state.pending_allocation.is_none()
                    && save.state.host_activity.is_none())
                    || save.state.pending.is_none())
            {
                return Err(("invalid_save", "存档事件状态不完整。".into()));
            }
        } else if save.state.turn != 0
            || save.state.player.is_some()
            || save.state.stats != Stats::default()
            || !["ready", "exploring"].contains(&save.state.phase.as_str())
        {
            return Err(("invalid_save", "存档缺少原版角色状态。".into()));
        }
        Ok(save)
    }
    pub(super) fn valid_saved_activity(&self, state: &State) -> bool {
        match &state.host_activity {
            Some(HostActivity::Status {
                panel,
                continuation,
            }) => {
                let valid_transition = panel.transition.as_ref().is_none_or(|transition| {
                    panel.page == StatusPage::House
                        && transition.kind == crate::transition::TransitionKind::Rain
                        && transition.duration_seconds == 1
                        && transition.unit_size == 2
                        && !transition.finished
                        && transition.valid_saved_state()
                });
                let valid_continuation = match continuation {
                    Some(StatusContinuation::NativeShop { bonus, .. }) => {
                        panel.page == StatusPage::House
                            && bonus.is_none_or(|(address, _)| {
                                [0x4b6490, 0x4b6492, 0x4b648e, 0x4b6494].contains(&address)
                            })
                    }
                    Some(StatusContinuation::NativeReturn) => panel.page == StatusPage::Investment,
                    None => true,
                };
                let valid_overview = panel.overview_animation.as_ref().is_none_or(|saved| {
                    panel.page != StatusPage::Overview
                        && saved.back_tracks[0].frame < 6
                        && saved.back_tracks[1].frame < 5
                });
                valid_transition
                    && valid_continuation
                    && valid_overview
                    && panel.back_tracks[0].frame < 6
                    && panel.back_tracks[1].frame < 5
                    && match panel.page {
                        StatusPage::CareerHistory => {
                            panel.scroll <= state.career_history.len().saturating_sub(10)
                        }
                        StatusPage::Skills => panel.scroll <= 22,
                        _ => panel.scroll == 0,
                    }
            }
            Some(HostActivity::Inventory { panel }) => {
                let Some(persistent) = &state.battle_persistent else {
                    return false;
                };
                let Ok(rules) = self.inventory_rules() else {
                    return false;
                };
                let expected = InventoryPanel::new(
                    panel.tab,
                    &state.inventory.ordinary,
                    &state.inventory.special,
                    &state.carried_equipment,
                    &persistent.equipped_ids,
                );
                let definitions = match panel.tab {
                    InventoryTab::Ordinary => &rules.ordinary,
                    InventoryTab::Special => &rules.special,
                    InventoryTab::Equipment => &rules.equipment,
                };
                panel.entries.len() == expected.entries.len()
                    && panel
                        .entries
                        .iter()
                        .zip(&expected.entries)
                        .all(|(actual, expected)| {
                            actual.id == expected.id
                                && actual.slot == expected.slot
                                && actual.equipped == expected.equipped
                                && definitions
                                    .get(actual.id)
                                    .is_some_and(|definition| actual.frame < definition.frame_count)
                        })
                    && panel
                        .selected_index
                        .is_none_or(|index| index < panel.entries.len())
                    && (!panel.confirm_drop || panel.selected_index.is_some())
                    && panel.back_tracks[0].frame < 6
                    && panel.back_tracks[1].frame < 5
            }
            Some(HostActivity::FinalBattle { game }) => self
                .final_battle_rules()
                .is_ok_and(|rules| game.valid_saved_state(&rules)),
            Some(HostActivity::Battle { game }) => {
                let Ok(rules) = self.battle_rules() else {
                    return false;
                };
                let input = &game.input;
                if game.rules != rules
                    || !valid_battle_persistent(&game.persistent)
                    || input.sex > 1
                    || !(-1..25).contains(&input.career)
                    || !(0..=7).contains(&input.stage)
                    || input.carried.len() != 8
                    || input.equipped.len() != 6
                    || input.equipped.iter().any(|&id| !(-1..18).contains(&id))
                    || input
                        .carried
                        .iter()
                        .any(|item| !(-1..18).contains(&item.id) || !(0..=4).contains(&item.level))
                    || game.enemy >= 15
                    || game.background >= 9
                    || game.player_class < 0
                    || game.player_class > 30
                    || game.cards.len() != 5
                    || game.selected.len() != 5
                    || game.card_x.len() != 5
                    || game.used_cards.len() != 5
                    || game.used_selected.len() != 5
                    || game.card_count > game.card_capacity
                    || !(2..=5).contains(&game.card_capacity)
                    || game
                        .cards
                        .iter()
                        .chain(&game.used_cards)
                        .any(|card| !(0..15).contains(&card.kind))
                    || game.player_portrait_timers.len() != 4
                    || game.enemy_portrait_timers.len() != 4
                    || game.player_actor > 4
                    || game.enemy_actor > 4
                    || game.enemy_effect > 1
                    || game.god_effect > 2
                    || game.combo_count > 5
                    || game.attack_kind > 4
                    || game.help_page > 9
                    || game
                        .pending_select
                        .is_some_and(|index| index >= game.card_count)
                    || game.hovered_card.is_some_and(|index| index >= 5)
                {
                    return false;
                }
                let mut names = vec!["strong".to_owned()];
                for index in 0..5 {
                    for prefix in ["player", "enemy", "element", "hover", "selected"] {
                        names.push(format!("{prefix}{index}"));
                    }
                    names.push(format!("combo{}", index + 1));
                }
                for index in 0..2 {
                    for prefix in ["enemyEffect", "shield", "ready"] {
                        names.push(format!("{prefix}{index}"));
                    }
                }
                for index in 0..3 {
                    names.push(format!("god{index}"));
                }
                for index in 0..10 {
                    names.push(format!("help{index}"));
                }
                names.iter().all(|name| {
                    game.tracks.get(name).is_some_and(|track| {
                        track.frame_count > 0 && track.frame >= 0 && track.frame < track.frame_count
                    })
                })
            }
            _ => true,
        }
    }
}

fn valid_battle_persistent(persistent: &BattlePersistent) -> bool {
    persistent.enemies.len() == 15
        && persistent
            .enemies
            .iter()
            .all(|enemy| (0..=9).contains(&enemy.rank) && (0..=9).contains(&enemy.respawn_level))
        && persistent.cards.len() <= 5
        && persistent
            .cards
            .iter()
            .all(|card| (0..15).contains(&card.kind))
        && persistent.equipped_ids.len() == 6
        && persistent
            .equipped_ids
            .iter()
            .all(|&id| (-1..18).contains(&id))
}

pub(super) fn canonicalize_catalog_numbers(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                canonicalize_catalog_numbers(item);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                canonicalize_catalog_numbers(item);
            }
        }
        Value::Number(number) if number.is_f64() => {
            let float = number.as_f64().unwrap();
            // Beyond Number.MAX_SAFE_INTEGER JavaScript cannot preserve every
            // integer. Keep those representations distinct instead of rounding.
            if float.fract() == 0.0 && float.abs() <= 9_007_199_254_740_991.0 {
                *number = (float as i64).into();
            }
        }
        _ => {}
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
