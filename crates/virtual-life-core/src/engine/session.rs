//! Typed session orchestration and original turn commands.
use super::*;

impl GameSession {
    pub fn from_json(content_json: &str) -> Result<Self, String> {
        let content = serde_json::from_str(content_json)
            .map_err(|error| format!("Invalid content catalog: {error}"))?;
        Self::from_content(content)
    }

    pub fn from_content(mut canonical: Value) -> Result<Self, String> {
        canonicalize_catalog_numbers(&mut canonical);
        let content_hash = digest(canonical.to_string().as_bytes());
        let content: Content = serde_json::from_value(canonical)
            .map_err(|e| format!("Invalid content catalog: {e}"))?;
        let mut ids = BTreeSet::new();
        for map in &content.maps {
            if map.id.is_empty() || !ids.insert(&map.id) {
                return Err("Duplicate or empty map identifier".into());
            }
            let mut coordinates = BTreeSet::new();
            for tile in &map.chips {
                if !coordinates.insert((tile.row, tile.column)) {
                    return Err(format!("Duplicate tile in map {}", map.id));
                }
            }
            if map.tile_index(&map.start).is_none() {
                return Err(format!("Map {} start does not identify a tile", map.id));
            }
        }
        let program = content
            .events
            .as_ref()
            .map(|value| LegacyProgram::from_json(&value.to_string()))
            .transpose()?;
        Ok(Self {
            maps: content.maps,
            program,
            rules: content.rules,
            questionnaires: content.questionnaires,
            content_hash,
            state: State {
                schema_version: SAVE_VERSION,
                revision: 0,
                last_seq: 0,
                phase: "ready".into(),
                map_id: None,
                tile_id: None,
                turn: 0,
                player: None,
                stats: Stats::default(),
                blockers: INITIAL_BLOCKERS.iter().map(|s| (*s).into()).collect(),
                message: "请选择开始新游戏或继续游戏。".into(),
                rng: ReplayRng::new(0),
                legacy_player: None,
                entry_step_pending: true,
                vm: None,
                pending: None,
                scene: json!({}),
                turn_in_progress: false,
                action_queue: vec![],
                pending_allocation: None,
                active_condition: None,
                host_activity: None,
                shop_visits: 0,
                career_levels: vec![1; 25],
                career_history: vec![],
                active_mission: None,
                inventory: Inventory::default(),
                queued_die: None,
                travel_origin: None,
                study_origin: None,
                spouse_index: -1,
                promotion_years: vec![],
                relationship_candidates: vec![-1; 8],
                relationship_new_targets: [-1; 8],
                dating_target: -1,
                relationship_years: [None; 3],
                gift_level: 1,
                fortune_visits: 0,
                known_equipment: vec![
                    EquipmentKnowledge {
                        learned: false,
                        level: 0
                    };
                    18
                ],
                carried_equipment: vec![EquipmentSlot { id: -1, level: 0 }; 8],
                first_equipment_visit: true,
                housing: HousingState::default(),
                spirits: Spirits::default(),
                last_roll_steps: 0,
                gas_target_state: None,
                map_facing: None,
                life_history: None,
                map_decorations: None,
                battle_persistent: None,
            },
        })
    }

    pub(super) fn current_map(&self) -> Option<&Map> {
        self.state
            .map_id
            .as_ref()
            .and_then(|id| self.maps.iter().find(|map| &map.id == id))
    }
    /// Immutable presentation model, independent of Bevy entities and GPU handles.
    pub fn snapshot(&self) -> GameSnapshot {
        let status_open = matches!(self.state.host_activity, Some(HostActivity::Status { .. }));
        GameSnapshot {
            schema_version: self.state.schema_version.clone(),
            revision: self.state.revision.clone(),
            last_seq: self.state.last_seq.clone(),
            phase: self.state.phase.clone(),
            map_id: self.state.map_id.clone(),
            tile_id: self.state.tile_id.clone(),
            turn: self.state.turn.clone(),
            player: self.state.player.clone(),
            stats: self.state.stats.clone(),
            blockers: self.state.blockers.clone(),
            message: self.state.message.clone(),
            pending: self.state.pending.clone(),
            native_view: self.native_view(),
            scene: self.event_scene_snapshot(),
            active_condition: self.state.active_condition.clone(),
            shop_visits: self.state.shop_visits.clone(),
            career_levels: self.state.career_levels.clone(),
            career_history: self.state.career_history.clone(),
            active_mission: self.state.active_mission.clone(),
            inventory: self.state.inventory.clone(),
            queued_die: self.state.queued_die.clone(),
            travel_origin: self.state.travel_origin.clone(),
            study_origin: self.state.study_origin.clone(),
            spouse_index: self.state.spouse_index.clone(),
            promotion_years: self.state.promotion_years.clone(),
            relationship_candidates: self.state.relationship_candidates.clone(),
            relationship_new_targets: self.state.relationship_new_targets.clone(),
            dating_target: self.state.dating_target.clone(),
            relationship_years: self.state.relationship_years.clone(),
            gift_level: self.state.gift_level.clone(),
            fortune_visits: self.state.fortune_visits.clone(),
            known_equipment: self.state.known_equipment.clone(),
            carried_equipment: self.state.carried_equipment.clone(),
            first_equipment_visit: self.state.first_equipment_visit.clone(),
            housing: self.state.housing.clone(),
            spirits: self.state.spirits.clone(),
            last_roll_steps: self.state.last_roll_steps.clone(),
            gas_target_state: self.state.gas_target_state.clone(),
            map_facing: self.state.map_facing.clone(),
            life_history: self.state.life_history.clone(),
            map_decorations: self.state.map_decorations.clone(),
            battle_persistent: self.state.battle_persistent.clone(),
            life: self.state.legacy_player.as_ref().map(LegacyPlayer::life),
            status_overview: if status_open {
                self.status_overview().ok()
            } else {
                None
            },
            status_details: if status_open {
                self.state
                    .legacy_player
                    .as_ref()
                    .zip(self.rules.as_ref())
                    .and_then(|(player, rules)| {
                        crate::status::detail_rows(player, &self.state.career_history, rules).ok()
                    })
            } else {
                None
            },
            selected_tile: self
                .current_map()
                .and_then(|map| self.state.tile_id.and_then(|id| map.chips.get(id)))
                .cloned(),
        }
    }
    pub(super) fn response(
        &self,
        error: Option<(&str, String)>,
        events: Vec<Value>,
    ) -> GameResponse {
        GameResponse {
            ok: error.is_none(),
            snapshot: self.snapshot(),
            events: events.into_iter().map(GameEffect::from_value).collect(),
            error: error.map(|(code, message)| GameError {
                code: code.into(),
                message,
            }),
        }
    }

    /// Apply one externally sequenced input without a JSON serialization boundary.
    pub fn dispatch(&mut self, command: GameCommand) -> GameResponse {
        let expected = match self.state.last_seq.checked_add(1) {
            Some(seq) => seq,
            None => {
                return self.response(
                    Some(("sequence_exhausted", "命令序号已用尽。".into())),
                    vec![],
                );
            }
        };
        if command.seq() != expected {
            return self.response(
                Some((
                    "out_of_order",
                    format!("Expected sequence {expected}, got {}", command.seq()),
                )),
                vec![],
            );
        }
        // Valid, ordered commands are consumed even when a gameplay precondition fails.
        self.state.last_seq = expected;
        match command {
            GameCommand::ExploreMap { map_id, .. } => {
                if self.state.legacy_player.is_some() {
                    return self.response(
                        Some(("game_active", "正在进行的人生不能直接切换浏览地图。".into())),
                        vec![],
                    );
                }
                let Some(map) = self.maps.iter().find(|map| map.id == map_id) else {
                    return self.response(
                        Some(("unknown_map", format!("Unknown map {map_id}"))),
                        vec![],
                    );
                };
                let tile_id = map.tile_index(&map.start).unwrap();
                self.state.map_id = Some(map_id.clone());
                self.state.tile_id = Some(tile_id);
                self.state.phase = "exploring".into();
                self.state.message = "正在浏览原版地图与格子数据；这不是已完成的游戏流程。".into();
                self.state.revision += 1;
                self.response(
                    None,
                    vec![json!({"type":"mapSelected","mapId":map_id,"tileId":tile_id})],
                )
            }
            GameCommand::InspectTile { tile_id, .. } => {
                if self.state.phase != "exploring" {
                    return self.response(
                        Some(("wrong_phase", "只有地图浏览可直接选择格子。".into())),
                        vec![],
                    );
                }
                let Some(map) = self.current_map() else {
                    return self.response(Some(("no_map", "尚未选择地图。".into())), vec![]);
                };
                if tile_id >= map.chips.len() {
                    return self.response(Some(("unknown_tile", "地图格不存在。".into())), vec![]);
                }
                self.state.tile_id = Some(tile_id);
                self.state.revision += 1;
                self.response(None, vec![json!({"type":"tileSelected","tileId":tile_id})])
            }
            GameCommand::FollowLink { branch, .. } => {
                if self.state.phase != "exploring" {
                    return self.response(
                        Some(("wrong_phase", "只有地图浏览可直接选择连接路径。".into())),
                        vec![],
                    );
                }
                let Some(map) = self.current_map() else {
                    return self.response(Some(("no_map", "尚未选择地图。".into())), vec![]);
                };
                let path = match map.trace_path(self.state.tile_id.unwrap(), &[branch]) {
                    Ok(path) => path,
                    Err(e) => return self.response(Some(("invalid_link", e)), vec![]),
                };
                self.state.tile_id = path.last().copied();
                self.state.revision += 1;
                self.response(None, vec![json!({"type":"pathTraced","tiles":path})])
            }
            GameCommand::NewGame { profile, seed, .. } => {
                if let Err(e) = profile.validate() {
                    return self.response(Some(("invalid_profile", e)), vec![]);
                }
                if self.program.is_none() {
                    return self.response(
                        Some(("missing_events", "缺少原版事件程序，无法开始游戏。".into())),
                        vec![],
                    );
                }
                let Some(map) = self.maps.iter().find(|map| map.id == "park") else {
                    return self
                        .response(Some(("missing_map", "缺少原版公园地图。".into())), vec![]);
                };
                let tile_id = map.tile_index(&map.start).unwrap();
                let map_facing = map.initial_facing();
                let mut rng = ReplayRng::new(seed);
                let decoration_delays = map.decoration_delays(&mut rng);
                let spirits = match Spirits::initialize(&map.coordinate_index, &map.start, &mut rng)
                {
                    Ok(spirits) => spirits,
                    Err(error) => return self.response(Some(("spirit_positions", error)), vec![]),
                };
                let mut player = LegacyPlayer::initialize();
                player
                    .set_i16(0x4b6498, i16::from(profile.sex == "female"))
                    .unwrap();
                // The main menu sets this default at 0x00420db0 before initialization.
                player.set_i16(0x4b64e6, 5).unwrap();
                self.state.stats = player.stats();
                self.state.legacy_player = Some(player);
                self.state.player = Some(profile);
                self.state.rng = rng;
                self.state.map_id = Some("park".into());
                self.state.tile_id = Some(tile_id);
                self.state.map_facing = map_facing;
                self.state.map_decorations = Some(MapDecorations {
                    map_id: "park".into(),
                    load_serial: 1,
                    delays_ms: decoration_delays,
                });
                self.state.turn = 0;
                self.state.entry_step_pending = true;
                self.state.vm = None;
                self.state.pending = None;
                self.state.scene = json!({});
                self.state.action_queue.clear();
                self.state.turn_in_progress = false;
                self.state.pending_allocation = None;
                self.state.active_condition = None;
                self.state.host_activity = None;
                self.state.shop_visits = 0;
                self.state.career_levels = vec![1; 25];
                self.state.career_history.clear();
                self.state.active_mission = None;
                self.state.inventory = Inventory::default();
                self.state.queued_die = None;
                self.state.travel_origin = None;
                self.state.study_origin = None;
                self.state.spouse_index = -1;
                self.state.promotion_years.clear();
                self.state.relationship_candidates = vec![-1; 8];
                self.state.gift_level = 1;
                self.state.fortune_visits = 0;
                self.state.relationship_new_targets = [-1; 8];
                self.state.dating_target = -1;
                self.state.relationship_years = [None; 3];
                self.state.known_equipment = vec![
                    EquipmentKnowledge {
                        learned: false,
                        level: 0
                    };
                    18
                ];
                self.state.carried_equipment = vec![EquipmentSlot { id: -1, level: 0 }; 8];
                self.state.first_equipment_visit = true;
                self.state.housing = HousingState::default();
                self.state.battle_persistent = None;
                self.state.life_history = Some(Self::initial_life_history());
                self.state.spirits = spirits;
                self.state.last_roll_steps = 0;
                self.state.phase = "playing".into();
                self.state.message = "新的人生从这里开始。".into();
                self.state.revision += 1;
                self.response(
                    None,
                    vec![json!({"type":"newGameStarted","mapId":"park","tileId":tile_id})],
                )
            }
            GameCommand::RollDice { value, .. } => match self.roll(value) {
                Ok(events) => self.response(None, events),
                Err(e) => self.response(Some(("cannot_roll", e)), vec![]),
            },
            GameCommand::AdvanceEvent { .. } => self.resume_event(VmInput::Continue),
            GameCommand::ChooseOption { index, .. } => self.resume_event(VmInput::Choose { index }),
            GameCommand::MinigameTick { elapsed_ms, .. } => {
                self.resume_minigame(Some(elapsed_ms), None, None, None, None)
            }
            GameCommand::MinigameAction {
                action,
                index,
                x,
                y,
                left,
                right,
                fire,
                ..
            } => self.resume_minigame(
                None,
                Some(action.as_str()),
                index,
                x.zip(y),
                Some((
                    left.unwrap_or(false),
                    right.unwrap_or(false),
                    fire.unwrap_or(false),
                )),
            ),
            GameCommand::RelationshipTick {
                elapsed_ms,
                hover_index,
                ..
            } => self.resume_relationship(Some(elapsed_ms), None, hover_index),
            GameCommand::RelationshipAction { action, index, .. } => {
                self.resume_relationship(None, Some(action.as_str()), index)
            }
            GameCommand::CompetitionTick { elapsed_ms, .. } => {
                self.resume_competition(Some(elapsed_ms), None, None)
            }
            GameCommand::CompetitionAction { action, index, .. } => {
                self.resume_competition(None, Some(action.as_str()), index)
            }
            GameCommand::EndingTick { elapsed_ms, .. } => {
                self.resume_ending(Some(elapsed_ms), None, None)
            }
            GameCommand::EndingAction { action, index, .. } => {
                self.resume_ending(None, Some(action.as_str()), index)
            }
            GameCommand::BattleTick { elapsed_ms, .. } => {
                self.resume_battle(Some(elapsed_ms), None, None)
            }
            GameCommand::BattleAction { action, index, .. } => {
                self.resume_battle(None, Some(action.as_str()), index)
            }
            GameCommand::OpenInventory { tab, .. } => self.open_inventory(tab.as_deref()),
            GameCommand::InventoryAction {
                action, index, tab, ..
            } => self.resume_inventory(None, Some(action.as_str()), index, tab.as_deref()),
            GameCommand::InventoryTick { elapsed_ms, .. } => {
                self.resume_inventory(Some(elapsed_ms), None, None, None)
            }
            GameCommand::OpenStatus { .. } => self.open_status(),
            GameCommand::StatusAction {
                action,
                page,
                delta,
                index,
                ..
            } => self.resume_status(
                None,
                Some(action.as_str()),
                page.as_deref(),
                delta.unwrap_or(0),
                index,
            ),
            GameCommand::StatusTick { elapsed_ms, .. } => {
                self.resume_status(Some(elapsed_ms), None, None, 0, None)
            }
            GameCommand::AllocateStats { allocations, .. } => {
                let Some(reward) = self.state.pending_allocation.clone() else {
                    return self.response(
                        Some(("wrong_phase", "当前没有待分配的成长点数。".into())),
                        vec![],
                    );
                };
                if let Err(error) =
                    reward.apply(self.state.legacy_player.as_mut().unwrap(), &allocations)
                {
                    return self.response(Some(("invalid_allocation", error)), vec![]);
                }
                self.state.pending_allocation = None;
                self.state.pending = None;
                self.state.phase = "playing".into();
                self.state.revision += 1;
                self.sync_player();
                let mut events = vec![
                    json!({"type":"statsAllocated","allocations":allocations,"money":reward.money}),
                ];
                self.run_actions(&mut events);
                self.response(None, events)
            }
        }
    }

    pub(super) fn sync_player(&mut self) {
        if let Some(player) = &self.state.legacy_player {
            self.state.stats = player.stats();
            self.state.turn = player.i16(0x4b64aa).unwrap().max(0) as u32;
        }
    }

    pub(super) fn trigger_skills(
        &mut self,
        trigger: i16,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let Some(rules) = self.rules.as_ref().and_then(|rules| rules.get("skills")) else {
            return Err("缺少原版技能规则表。".into());
        };
        let rules: Vec<SkillDefinition> =
            serde_json::from_value(rules.clone()).map_err(|error| error.to_string())?;
        let player = self.state.legacy_player.as_mut().ok_or("No player")?;
        let levels = std::array::from_fn(|i| player.i16(0x4b6508 + i as u32 * 2).unwrap());
        let effects = skill_effects(trigger, &rules, &levels, &|address| {
            if address == 0x4b648a {
                player.i32(address).ok()
            } else {
                player.i16(address).ok().map(i32::from)
            }
        })?;
        for write in effects.writes {
            if write.address == 0x4b648a {
                player.set_i32(write.address, write.value)?;
            } else {
                player.set_i16(write.address, write.value as i16)?;
            }
        }
        for effect in effects.activations {
            events.push(json!({"type":"skillApplied","effect":effect}));
        }
        self.sync_player();
        Ok(())
    }

    pub(super) fn select_event(&mut self, events: &mut Vec<Value>) -> Result<i32, String> {
        let player = self.state.legacy_player.as_ref().ok_or("No player")?;
        let rules = self.rules.as_ref().ok_or("Missing original event tables")?;
        let selected = select_ordinary_event(
            &|address| {
                if address == 0x4b648a {
                    player.i32(address).ok()
                } else {
                    player.i16(address).ok().map(i32::from)
                }
            },
            &|address| {
                rules["tableSegments"]
                    .as_array()?
                    .iter()
                    .find_map(|segment| {
                        let base = u32::from_str_radix(
                            segment["baseAddress"].as_str()?.trim_start_matches("0x"),
                            16,
                        )
                        .ok()?;
                        let offset = address.checked_sub(base)?;
                        if offset % 2 != 0 {
                            return None;
                        }
                        segment["values"]
                            .as_array()?
                            .get(offset as usize / 2)?
                            .as_i64()
                            .map(|value| value as i16)
                    })
            },
            &mut || self.state.rng.next_rand(),
        )?;
        let player = self.state.legacy_player.as_mut().unwrap();
        for write in selected.writes {
            player.set_i16(write.address, write.value as i16)?;
        }
        self.trigger_skills(selected.skill_trigger, events)?;
        Ok(selected.next_node)
    }

    pub(super) fn block(&mut self, code: &str, message: String, details: Value) {
        self.sync_player();
        self.state.phase = "blocked".into();
        self.state.message = message.clone();
        self.state.pending =
            Some(json!({"kind":"blocked","code":code,"text":message,"details":details}));
    }

    /// Check a roll on a cloned random stream, before committing an autosave.
    pub fn validate_roll(&self, forced: Option<u8>) -> Result<(), String> {
        self.planned_roll(forced).map(|_| ())
    }
    fn planned_roll(
        &self,
        forced: Option<u8>,
    ) -> Result<(ReplayRng, u8, crate::original_rules::Movement), String> {
        if self.state.phase != "playing" || self.state.turn_in_progress {
            return Err("请先完成当前事件。".into());
        }
        if forced.is_some()
            && (self.state.spirits.active != Some(0)
                || self.state.legacy_player.as_ref().unwrap().life().traveling)
        {
            return Err("只有当前骰子神的能力可以指定点数。".into());
        }
        let map = self.current_map().ok_or("没有正在进行的地图。")?;
        let start = self.state.tile_id.ok_or("没有角色位置。")?;
        let mut rng = self.state.rng.clone();
        let rolled = roll_dice(&mut rng, forced)?;
        let movement = plan_movement(map, start, rolled, self.state.entry_step_pending, &mut rng)?;
        Ok((rng, rolled, movement))
    }
    pub(super) fn roll(&mut self, forced: Option<u8>) -> Result<Vec<Value>, String> {
        let (rng, rolled, movement) = self.planned_roll(forced)?;
        let map = self.current_map().ok_or("没有正在进行的地图。")?;
        let mut queue = Vec::new();
        let mut segment = Vec::new();
        for (index, tile_id) in movement.path.iter().copied().enumerate() {
            segment.push(tile_id);
            let event_id = map.chips[tile_id].event_id as i32;
            let landed = index + 1 == movement.path.len();
            if landed || [0x5dc4, 0x5dcc, 0x5ddb, 0x5de7, 0x5dda].contains(&event_id) {
                queue.push(TileAction {
                    tile_id,
                    event_id,
                    landed,
                    path: std::mem::take(&mut segment),
                    spirit_checked: false,
                });
            }
        }
        let mut events = vec![json!({"type":"diceRolled","rolled":rolled})];
        self.state.rng = rng;
        self.state.entry_step_pending = movement.entry_step_pending;
        self.state.last_roll_steps = movement.path.len() as u8;
        self.state.action_queue = queue;
        self.state.turn_in_progress = true;
        self.state.pending = None;
        self.state.scene = json!({});
        self.state.queued_die = Some(rolled);
        if movement.reached_end {
            let player = self.state.legacy_player.as_mut().unwrap();
            player
                .set_i16(
                    if player.life().traveling {
                        0x4b64b0
                    } else {
                        0x4b64b4
                    },
                    1,
                )
                .unwrap();
        }
        self.state.revision += 1;
        self.run_actions(&mut events);
        Ok(events)
    }
}

/// One visible frame. A long delta is handled by each original algorithm itself; no global catch-up.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrameStep {
    pub elapsed_ms: u32,
}

impl GameSession {
    /// Convenience input entrypoint that assigns the next monotonically increasing sequence.
    pub fn send(&mut self, command: GameCommand) -> GameResponse {
        let Some(seq) = self.state.last_seq.checked_add(1) else {
            return self.response(
                Some(("sequence_exhausted", "命令序号已用尽。".into())),
                vec![],
            );
        };
        self.dispatch(command.with_sequence(seq))
    }

    /// Advance only the active original activity, once. Paused application states must not call this.
    pub fn frame(&mut self, step: FrameStep) -> GameResponse {
        let seq = 0;
        let elapsed_ms = step.elapsed_ms;
        let command = match self.state.host_activity.as_ref() {
            Some(
                HostActivity::Tracking { .. }
                | HostActivity::Hammer { .. }
                | HostActivity::Fist { .. }
                | HostActivity::Gas { .. }
                | HostActivity::Fire { .. }
                | HostActivity::Firebee { .. }
                | HostActivity::SpiritLottery { .. },
            ) => GameCommand::MinigameTick { seq, elapsed_ms },
            Some(HostActivity::Relationship { .. }) => GameCommand::RelationshipTick {
                seq,
                elapsed_ms,
                hover_index: None,
            },
            Some(HostActivity::Competition { .. }) => {
                GameCommand::CompetitionTick { seq, elapsed_ms }
            }
            Some(HostActivity::Ending { .. }) => GameCommand::EndingTick { seq, elapsed_ms },
            Some(HostActivity::Battle { .. } | HostActivity::FinalBattle { .. }) => {
                GameCommand::BattleTick { seq, elapsed_ms }
            }
            Some(HostActivity::Status { .. }) => GameCommand::StatusTick { seq, elapsed_ms },
            Some(HostActivity::Inventory { .. }) => GameCommand::InventoryTick { seq, elapsed_ms },
            _ => return self.response(None, vec![]),
        };
        self.send(command)
    }
    pub fn content_identity(&self) -> &str {
        &self.content_hash
    }
}
