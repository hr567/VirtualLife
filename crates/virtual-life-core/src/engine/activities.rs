//! Original native activity entry, update and settlement.
use super::*;

impl GameSession {
    pub(super) fn resume_event(&mut self, input: VmInput) -> GameResponse {
        if self.state.phase != "event" {
            return self.response(
                Some(("wrong_phase", "当前没有可继续的事件。".into())),
                vec![],
            );
        }
        if self.state.host_activity.is_some() {
            return self.resume_host_activity(input);
        }
        let Some(vm) = &mut self.state.vm else {
            return self.response(
                Some(("missing_event", "当前事件状态不存在。".into())),
                vec![],
            );
        };
        if let Err(error) = vm.resume(input) {
            return self.response(Some(("invalid_event_input", error)), vec![]);
        }
        self.state.pending = None;
        self.state.revision += 1;
        let mut events = Vec::new();
        self.advance_vm(&mut events);
        self.response(None, events)
    }

    pub(super) fn begin_stat_shop(&mut self) -> Result<(), String> {
        let player = self.state.legacy_player.as_ref().ok_or("No player")?;
        let young = player.life().stage <= 3;
        let groups = self
            .rules
            .as_ref()
            .and_then(|rules| rules.get("statShops"))
            .and_then(|shops| shops.get(if young { "young" } else { "adult" }))
            .ok_or("缺少原版商店商品表。")?;
        let groups: Vec<Vec<StatShopItem>> =
            serde_json::from_value(groups.clone()).map_err(|error| error.to_string())?;
        self.state.shop_visits = (self.state.shop_visits + 1) % 3;
        let shop = StatShop::generate_for_visit(
            &groups,
            !young && self.state.shop_visits == 0,
            self.state
                .relationship_candidates
                .iter()
                .any(|id| *id != -1),
            &mut self.state.rng,
        )?;
        let player = self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)? != -1 && player.i16(0x4b64ce)? == 4 {
            player.set_i16(0x4b64d0, 1)?;
        }
        self.state.host_activity = Some(HostActivity::StatShop { shop });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn begin_career(&mut self, entering_adulthood: bool) -> Result<(), String> {
        self.state.legacy_player.as_mut().unwrap().update_skills()?;
        let careers: Vec<CareerRule> = serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("careers"))
                .ok_or("缺少原版职业规则。")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        self.state.host_activity = Some(HostActivity::Career {
            careers,
            entering_adulthood,
        });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn begin_item_shop(&mut self) -> Result<(), String> {
        let rules = self
            .rules
            .as_ref()
            .ok_or("Missing original item shop tables")?;
        let catalog: ItemShopCatalog =
            serde_json::from_value(rules["itemShop"].clone()).map_err(|error| error.to_string())?;
        let equipment: Vec<EquipmentDefinition> =
            serde_json::from_value(rules["equipment"].clone())
                .map_err(|error| error.to_string())?;
        let player = self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)? != -1 && player.i16(0x4b64ce)? == 5 {
            player.set_i16(0x4b64d0, 1)?;
        }
        let shop = NativeShop::generate(
            &catalog,
            player,
            &self.state.housing,
            &self.state.known_equipment,
            &self.state.carried_equipment,
            &equipment,
            &mut self.state.rng,
        )?;
        self.state.host_activity = Some(HostActivity::NativeShop { shop });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn begin_housing(&mut self) -> Result<(), String> {
        let catalog: HousingCatalog = serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("housing"))
                .ok_or("Missing original housing tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        let player = self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)? != -1 && player.i16(0x4b64ce)? == 6 {
            player.set_i16(0x4b64d0, 1)?;
        }
        self.state.host_activity = Some(HostActivity::Housing {
            catalog,
            result: None,
            preview_investment: false,
        });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn choose_career(&mut self, career: i16) -> Result<(), String> {
        let player = self.state.legacy_player.as_mut().unwrap();
        let old = player.life().profession;
        player.set_i16(0x4b64a2, career)?;
        player.set_i16(
            0x4b64a6,
            *self
                .state
                .career_levels
                .get(career as usize)
                .ok_or("Invalid career")?,
        )?;
        player.update_skills()?;
        if old != career && self.state.career_history.len() < 98 {
            self.state.career_history.push(json!({"year":player.life().year,"career":career,"rank":player.i16(0x4b6508+career as u32*2)?}));
            if old != -1 {
                self.state
                    .life_history
                    .get_or_insert_with(Self::initial_life_history)[6]
                    .push(i32::from(player.life().year));
            }
        }
        Ok(())
    }

    /// FUN_00419d20: update the stage before selecting the original map table.
    pub(super) fn transition_stage(
        &mut self,
        stage: i16,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let data = self
            .rules
            .as_ref()
            .and_then(|rules| rules["stages"].as_array())
            .and_then(|stages| {
                stages
                    .iter()
                    .find(|item| item["id"].as_i64() == Some(i64::from(stage)))
            })
            .ok_or("Missing original stage table")?;
        let path = data["mapPath"].as_str().ok_or("Missing stage map")?;
        let id = path
            .rsplit('/')
            .next()
            .ok_or("Invalid stage map path")?
            .to_owned();
        let map = self
            .maps
            .iter()
            .find(|map| map.id == id)
            .ok_or("Missing stage map resources")?;
        let tile_id = map.tile_index(&map.start).unwrap();
        let player = self.state.legacy_player.as_mut().unwrap();
        let previous = player.life().stage;
        if let Some(index) = match stage {
            4 => Some(4),
            5 if previous == 3 => Some(3),
            6 if previous != 6 => Some(5),
            _ => None,
        } {
            self.state
                .life_history
                .get_or_insert_with(Self::initial_life_history)[index]
                .push(i32::from(player.life().year));
        }
        player.set_i16(0x4b64a8, 0)?;
        player.set_i16(0x4b64a0, previous)?;
        player.set_i16(0x4b649e, stage)?;
        if stage != 4 || previous != 3 {
            self.state.tile_id = Some(tile_id);
            self.state.entry_step_pending = true;
            self.state.map_facing = map.initial_facing();
        }
        self.state.map_id = Some(id.to_owned());
        self.load_map_decorations()?;
        self.state.scene = json!({});
        self.sync_player();
        events.push(json!({"type":"stageChanged","previous":previous,"stage":stage,"mapId":id,"tileId":self.state.tile_id}));
        if stage == 6 && previous != 6 {
            self.begin_career(true)?;
        }
        Ok(())
    }

    pub(super) fn enter_travel(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        let ids = self
            .rules
            .as_ref()
            .and_then(|rules| rules["travelMaps"].as_array())
            .ok_or("Missing original travel map table")?;
        if ids.len() != 4 {
            return Err("Original travel table must have four maps".into());
        }
        let index = self.state.rng.next_rand() as usize % 4;
        let path = ids[index]
            .as_str()
            .or_else(|| ids[index]["mapPath"].as_str())
            .ok_or("Missing travel path")?;
        let id = path.rsplit('/').next().ok_or("Invalid travel path")?;
        let id = id.to_owned();
        let map = self
            .maps
            .iter()
            .find(|map| map.id == id)
            .ok_or("Missing travel map")?;
        self.state.travel_origin = Some(MapPosition {
            map_id: self.state.map_id.clone().unwrap(),
            tile_id: self.state.tile_id.unwrap(),
            facing: self.state.map_facing,
        });
        self.state.map_id = Some(id.to_owned());
        self.state.tile_id = map.tile_index(&map.start);
        self.state.entry_step_pending = true;
        self.state.map_facing = map.initial_facing();
        self.load_map_decorations()?;
        let player = self.state.legacy_player.as_mut().unwrap();
        player.set_i16(0x4b64ac, 1)?;
        player.set_i16(0x4b64b0, 0)?;
        events.push(json!({"type":"travelStarted","mapId":id,"tileId":self.state.tile_id}));
        self.sync_player();
        Ok(())
    }

    pub(super) fn return_from_travel(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        let origin = self
            .state
            .travel_origin
            .take()
            .ok_or("Missing original travel return position")?;
        self.state.map_id = Some(origin.map_id.clone());
        self.state.tile_id = Some(origin.tile_id);
        self.state.entry_step_pending = true;
        self.state.map_facing = origin.facing;
        self.load_map_decorations()?;
        let player = self.state.legacy_player.as_mut().unwrap();
        player.set_i16(0x4b64ac, 0)?;
        player.set_i16(0x4b64b0, 0)?;
        events.push(json!({"type":"travelEnded","mapId":origin.map_id,"tileId":origin.tile_id}));
        self.sync_player();
        Ok(())
    }

    pub(super) fn load_map_decorations(&mut self) -> Result<(), String> {
        let id = self
            .state
            .map_id
            .as_ref()
            .ok_or("Missing decoration map")?
            .clone();
        let map = self
            .maps
            .iter()
            .find(|map| map.id == id)
            .ok_or("Missing decoration map")?;
        let delays_ms = map.decoration_delays(&mut self.state.rng);
        let load_serial = self
            .state
            .map_decorations
            .as_ref()
            .map_or(1, |state| state.load_serial.wrapping_add(1));
        self.state.map_decorations = Some(MapDecorations {
            map_id: id,
            load_serial,
            delays_ms,
        });
        Ok(())
    }

    pub(super) fn spirit_map_context(&self) -> Result<(Vec<Coordinate>, Coordinate), String> {
        let map = self.current_map().ok_or("Missing original spirit map")?;
        let tile = map
            .chips
            .get(self.state.tile_id.ok_or("Missing player tile")?)
            .ok_or("Invalid player tile")?;
        Ok((
            map.coordinate_index.clone(),
            Coordinate {
                row: tile.row,
                column: tile.column,
            },
        ))
    }

    pub(super) fn refresh_spirit_positions(
        &mut self,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let (pool, position) = self.spirit_map_context()?;
        self.state
            .spirits
            .change_map(&pool, &position, &mut self.state.rng)?;
        events.push(json!({"type":"spiritsRepositioned","positions":self.state.spirits.positions}));
        Ok(())
    }

    pub(super) fn begin_spirit_lottery(&mut self) -> Result<(), String> {
        let spirit = self.state.spirits.active.ok_or("Missing active spirit")?;
        let game = SpiritLottery::new(
            spirit,
            self.state.last_roll_steps % 2 == 1,
            &mut self.state.rng,
        )?;
        self.state.host_activity = Some(HostActivity::SpiritLottery { game });
        self.refresh_host_activity();
        Ok(())
    }

    /// FUN_004159a0 checks spirits after movement and before the landed event.
    pub(super) fn prepare_spirit_event(&mut self, events: &mut Vec<Value>) -> Result<bool, String> {
        let life = self.state.legacy_player.as_ref().unwrap().life();
        if life.traveling || life.stage == 2 {
            return Ok(false);
        }
        let (pool, position) = self.spirit_map_context()?;
        if let Some(spirit) = self.state.spirits.encounter(&position) {
            let player = self.state.legacy_player.as_mut().unwrap();
            self.state.spirits.attach(
                spirit,
                player.i16(0x4b64aa)?,
                &pool,
                &position,
                &mut self.state.rng,
            )?;
            player.set_i16(0x4b64d6, spirit as i16)?;
            events.push(json!({"type":"spiritAttached","spirit":spirit,"expiresTurn":self.state.spirits.expires_turn}));
            self.state.host_activity = Some(HostActivity::SpiritEncounter { spirit });
            self.refresh_host_activity();
            return Ok(true);
        }
        if self.state.spirits.active.is_some_and(|spirit| spirit >= 2) {
            self.begin_spirit_lottery()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn resume_spirit_encounter(&mut self, input: VmInput) -> GameResponse {
        if !matches!(input, VmInput::Continue | VmInput::Choose { index: 0 }) {
            return self.response(
                Some(("spirit_input", "请继续阅读神灵事件。".into())),
                vec![],
            );
        }
        let Some(HostActivity::SpiritEncounter { spirit }) = self.state.host_activity.take() else {
            unreachable!()
        };
        self.state.pending = None;
        self.state.scene = json!({});
        self.state.revision += 1;
        let mut events = Vec::new();
        if spirit >= 2 {
            if let Err(error) = self.begin_spirit_lottery() {
                self.block("spirit_lottery", error, Value::Null);
            }
        } else {
            self.state.phase = "playing".into();
            self.run_actions(&mut events);
        }
        self.response(None, events)
    }

    pub(super) fn resume_spirit_lottery(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
    ) -> GameResponse {
        let Some(HostActivity::SpiritLottery { game }) = self.state.host_activity.as_mut() else {
            unreachable!()
        };
        let result = if let Some(elapsed) = elapsed {
            game.tick(elapsed)
        } else if action == Some("stop") {
            game.stop(&mut self.state.rng)
        } else {
            Err("请选择停止转盘。".into())
        };
        if let Err(error) = result {
            return self.response(Some(("spirit_input", error)), vec![]);
        }
        self.state.revision += 1;
        let mut events = Vec::new();
        if game.phase == SpiritLotteryPhase::Finished {
            if let Err(error) = game.apply(self.state.legacy_player.as_mut().unwrap()) {
                self.block("spirit_result", error, Value::Null);
                return self.response(None, events);
            }
            events.push(json!({"type":"spiritEffect","spirit":game.spirit,"amount":game.amount(),"positiveMoney":game.positive_money,"attribute":game.attribute.value}));
            self.state.host_activity = None;
            self.state.pending = None;
            self.state.scene = json!({});
            self.sync_player();
            self.state.phase = "playing".into();
            self.run_actions(&mut events);
        } else {
            self.refresh_host_activity();
        }
        self.response(None, events)
    }

    pub(super) fn show_notice(
        &mut self,
        pages: Vec<i32>,
        continuation: NoticeContinuation,
    ) -> Result<(), String> {
        if pages.is_empty() {
            return Err("Original notice has no pages".into());
        }
        for id in &pages {
            if self.program.as_ref().unwrap().dialogue(*id).is_none() {
                return Err(format!("Missing original dialogue {id}"));
            }
        }
        self.state.host_activity = Some(HostActivity::Notice {
            pages,
            page: 0,
            continuation,
        });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn exam_questions(&self) -> Result<Vec<ExamQuestion>, String> {
        let records = self
            .questionnaires
            .as_ref()
            .and_then(|data| data["highTest"].get("records"))
            .ok_or("缺少原版高考题库。")?;
        serde_json::from_value(records.clone()).map_err(|error| error.to_string())
    }

    pub(super) fn begin_school_exam(&mut self) -> Result<(), String> {
        let questions = self.exam_questions()?;
        let exam = SchoolExam::new(&questions, &mut self.state.rng)?;
        self.state.host_activity = Some(HostActivity::SchoolExam { exam });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn begin_competition(&mut self, turn_outcome: TurnOutcome) -> Result<(), String> {
        let data = self
            .rules
            .as_ref()
            .and_then(|rules| rules.get("competition"))
            .ok_or("缺少原版学生大赛规则。")?;
        let rules: CompetitionRules =
            serde_json::from_value(data.clone()).map_err(|error| error.to_string())?;
        let player = self.state.legacy_player.as_ref().unwrap();
        let game = StudentCompetition::new(
            player.life().stage,
            player.i16(0x4b6498)? as usize,
            self.state.player.as_ref().unwrap().name.clone(),
            player.i16(0x4b6494)?,
            player.i16(0x4b648e)?,
            rules,
        );
        self.state.host_activity = Some(HostActivity::Competition { game, turn_outcome });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn resume_competition(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
    ) -> GameResponse {
        if elapsed.is_some_and(|ms| ms > 60000) {
            return self.response(
                Some(("competition_time", "比赛时间步长无效。".into())),
                vec![],
            );
        }
        let questions = if matches!(action, Some("choose" | "answer")) {
            match self.exam_questions() {
                Ok(questions) => questions,
                Err(error) => return self.response(Some(("competition_questions", error)), vec![]),
            }
        } else {
            Vec::new()
        };
        let Some(HostActivity::Competition { game, turn_outcome }) =
            self.state.host_activity.as_mut()
        else {
            return self.response(
                Some(("wrong_competition", "当前没有学生大赛。".into())),
                vec![],
            );
        };
        let result = if let Some(ms) = elapsed {
            game.tick(ms, &mut self.state.rng)
        } else {
            match action {
                Some("continue") => game.continue_action(&mut self.state.rng),
                Some("choose") => index
                    .ok_or("请选择比赛项目。".into())
                    .and_then(|index| game.select(index, &questions, &mut self.state.rng)),
                Some("answer") => index
                    .ok_or("请选择答案。".into())
                    .and_then(|index| game.answer(index, &questions, &mut self.state.rng)),
                Some("left") => game.running_input(true, false),
                Some("right") => game.running_input(false, true),
                _ => Err("比赛操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("competition_input", error)), vec![]);
        }
        let mut events = Vec::new();
        let finished = game.phase == CompetitionPhase::Finished;
        let outcome = turn_outcome.clone();
        if let Some(prize) = game.take_prize() {
            let player = self.state.legacy_player.as_mut().unwrap();
            player
                .set_i32(0x4b648a, player.i32(0x4b648a).unwrap().wrapping_add(prize))
                .unwrap();
            events.push(json!({"type":"competitionPrize","amount":prize,"place":game.place}));
        }
        if finished {
            events.push(json!({"type":"competitionCompleted","selected":game.selected,"place":game.place,"prize":game.prize}));
            self.state.host_activity = None;
            self.state.pending = None;
            self.state.scene = json!({});
            self.finish_turn_transitions(outcome, &mut events);
        } else {
            self.refresh_host_activity();
        }
        self.state.revision += 1;
        self.sync_player();
        self.response(None, events)
    }

    pub(super) fn collect_salary(
        &mut self,
        landed: bool,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let rules = self
            .rules
            .as_ref()
            .ok_or("Missing original salary tables")?;
        let player = self.state.legacy_player.as_ref().ok_or("Missing player")?;
        let career = player.life().profession;
        let definition = rules["careers"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["id"].as_i64() == Some(i64::from(career)))
            })
            .map(|item| serde_json::from_value::<CareerRule>(item.clone()))
            .transpose()
            .map_err(|error| error.to_string())?;
        let rank = payday_rank(player, definition.as_ref(), &self.state.inventory.special)?;
        let base = if career == -1 {
            rules["unemployedBaseSalary"]
                .as_i64()
                .ok_or("Missing original unemployed salary")?
        } else {
            rules["baseSalaries"]
                .get(career as usize)
                .and_then(Value::as_i64)
                .ok_or("Missing original base salary")?
        } as i16;
        let multipliers: Vec<i32> = serde_json::from_value(rules["salaryMultipliers"].clone())
            .map_err(|error| error.to_string())?;
        let spouse = if self.state.spouse_index == -1 {
            0
        } else {
            let index =
                i32::from(self.state.spouse_index) % 8 + i32::from(player.i16(0x4b6498)?) * 8;
            rules["spouseSalary"]
                .get(usize::try_from(index).map_err(|_| "Invalid original spouse index")?)
                .and_then(Value::as_i64)
                .ok_or("Missing original spouse salary")? as i16
        };
        if rank > player.life().profession_level {
            self.state.promotion_years.push(player.life().year);
        }
        let outcome = collect_payday(
            self.state.legacy_player.as_mut().unwrap(),
            rank,
            base,
            &multipliers,
            spouse,
            landed,
        )?;
        if career >= 0 {
            *self
                .state
                .career_levels
                .get_mut(career as usize)
                .ok_or("Invalid original career history")? = rank;
        }
        events.push(json!({"type":"payday","career":career,"landed":landed,"outcome":outcome}));
        self.trigger_skills(4, events)?;
        self.sync_player();
        Ok(())
    }

    pub(super) fn begin_minigame(&mut self) -> Result<(), String> {
        let variant = self.state.rng.next_rand() % 6;
        let player = self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64ce)? == 11 {
            let target = player.i16(0x4b64cc)?;
            if !(1..=6).contains(&target) || target == [3, 4, 5, 2, 6, 1][variant as usize] {
                player.set_i16(0x4b64d0, 1)?;
            }
        }
        self.state.host_activity = Some(match variant {
            0 => {
                let mut game = GasGame::new(player.i16(0x4b648e)?);
                game.target_state = self.state.gas_target_state.unwrap_or(0);
                HostActivity::Gas { game }
            }
            1 => HostActivity::Tracking {
                game: TrackingGame::with_intelligence(&mut self.state.rng, player.i16(0x4b6492)?),
            },
            2 => HostActivity::Hammer {
                game: HammerGame::with_morality(player.i16(0x4b6490)?),
            },
            3 => HostActivity::Fire {
                game: FireGame::new(player.i16(0x4b6494)?),
            },
            4 => HostActivity::Firebee {
                game: FirebeeGame::new(player.i32(0x4b648a)?),
            },
            5 => HostActivity::Fist {
                game: FistGame::new(player.i16(0x4b6490)?),
            },
            _ => return Err(format!("原版小游戏 {variant} 的规则与输入模块仍在还原。")),
        });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn begin_fortune(&mut self) -> Result<(), String> {
        let records: Vec<FortuneRecord> = serde_json::from_value(
            self.questionnaires
                .as_ref()
                .and_then(|data| data["fortune"].get("records"))
                .ok_or("Missing original fortune questionnaires")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        if records.len() != 50 {
            return Err("Original fortune questionnaire count must be 50".into());
        }
        let index = self.state.rng.next_rand() as usize % records.len();
        let player = self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)? > 0 && player.i16(0x4b64ce)? == 13 {
            player.set_i16(0x4b64d0, 1)?;
        }
        let game = FortuneGame::new(records[index].clone())?;
        self.state.host_activity = Some(HostActivity::Fortune { game });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn begin_relationship(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        let rules: RelationshipRules = serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("relationships"))
                .ok_or("Missing original relationship tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        let player = self.state.legacy_player.as_mut().unwrap();
        let target = player.i16(0x4b64cc)?;
        let mission = player.i16(0x4b64ce)? == 12;
        if target != -1 && mission {
            player.set_i32(0x4b64d0, 1)?;
        }
        let state = RelationshipState {
            sex: player.i16(0x4b6498)? as u8,
            stage: player.life().stage,
            money: player.i32(0x4b648a)?,
            charm: player.i16(0x4b6494)?,
            romance: player.i16(0x4b6496)?,
            contacts: self
                .state
                .relationship_candidates
                .as_slice()
                .try_into()
                .map_err(|_| "Invalid original contact storage")?,
            new_targets: self.state.relationship_new_targets,
            dating_target: self.state.dating_target,
            spouse: self.state.spouse_index,
            gift_level: self.state.gift_level,
        };
        let house =
            RelationshipHouse::new(&rules, state, target > 0 && mission, &mut self.state.rng)?;
        let finished = house.phase == RelationshipPhase::Finished;
        self.state.host_activity = Some(HostActivity::Relationship { house });
        if finished {
            self.finish_relationship(events)?;
        } else {
            self.refresh_host_activity();
        }
        Ok(())
    }

    pub(super) fn sync_relationship(&mut self) -> Result<(), String> {
        let Some(HostActivity::Relationship { house }) = self.state.host_activity.as_ref() else {
            return Err("Missing relationship activity".into());
        };
        let player = self.state.legacy_player.as_mut().unwrap();
        player.set_i32(0x4b648a, house.state.money)?;
        player.set_i16(0x4b6494, house.state.charm)?;
        player.set_i16(0x4b6496, house.state.romance)?;
        self.state.relationship_candidates = house.state.contacts.to_vec();
        self.state.relationship_new_targets = house.state.new_targets;
        self.state.dating_target = house.state.dating_target;
        self.state.spouse_index = house.state.spouse;
        self.state.gift_level = house.state.gift_level;
        self.sync_player();
        Ok(())
    }

    pub(super) fn finish_relationship(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        self.sync_relationship()?;
        let Some(HostActivity::Relationship { house }) = self.state.host_activity.as_ref() else {
            unreachable!()
        };
        let nodes = house.event_nodes()?;
        let kind = house.return_kind;
        if (1..=3).contains(&kind) {
            let year = &mut self.state.relationship_years[usize::from(kind - 1)];
            if year.is_none() {
                *year = Some(self.state.legacy_player.as_ref().unwrap().life().year);
            }
        }
        events.push(json!({"type":"relationshipCompleted","returnKind":kind,"eventNodes":nodes}));
        self.state.host_activity = None;
        self.state.pending = None;
        self.state.scene = json!({});
        self.state
            .vm
            .as_mut()
            .ok_or("Missing relationship continuation")?
            .resume_relationship(nodes, kind)?;
        self.advance_vm(events);
        Ok(())
    }

    pub(super) fn resume_relationship(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
    ) -> GameResponse {
        if self.state.phase != "event" {
            return self.response(
                Some(("wrong_phase", "当前没有正在进行的感情事件。".into())),
                vec![],
            );
        }
        let Some(HostActivity::Relationship { house }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_relationship", "当前没有正在进行的感情事件。".into())),
                vec![],
            );
        };
        let missing = || "请选择一个人物或答案。".to_string();
        let result = if let Some(elapsed) = elapsed {
            house.tick(elapsed, index, &mut self.state.rng)
        } else {
            match action {
                Some("meet") => house.meet(),
                Some("dates") => house.dates(),
                Some("marriages") => house.marriages(),
                Some("selectOffer") => index.ok_or_else(missing).and_then(|index| {
                    house.hover(index, &mut self.state.rng)?;
                    house.select_offer(index)
                }),
                Some("answer") => index
                    .ok_or_else(missing)
                    .and_then(|index| house.answer(index, &mut self.state.rng)),
                Some("advanceNotice") => house.advance_notice(),
                Some("selectDate") => index
                    .ok_or_else(missing)
                    .and_then(|index| house.select_date(index)),
                Some("selectMarriage") => index
                    .ok_or_else(missing)
                    .and_then(|index| house.select_marriage(index)),
                Some("reject") => house.reject_selection(),
                Some("confirm") => house.confirm_selection(),
                Some("close") => {
                    house.close();
                    Ok(())
                }
                _ => Err("感情事件操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("relationship_input", error)), vec![]);
        }
        let finished = house.phase == RelationshipPhase::Finished;
        let mut events = Vec::new();
        self.state.revision += 1;
        if let Err(error) = self.sync_relationship() {
            self.block("relationship_state", error, Value::Null);
        } else if finished {
            if let Err(error) = self.finish_relationship(&mut events) {
                self.block("relationship_continuation", error, Value::Null);
            }
        } else {
            self.refresh_host_activity();
        }
        self.response(None, events)
    }

    pub(super) fn begin_equipment_workshop(&mut self) -> Result<(), String> {
        let definitions: Vec<EquipmentDefinition> = serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("equipment"))
                .ok_or("Missing original equipment workshop catalog")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        let workshop = EquipmentWorkshop::new(
            &self.state.known_equipment,
            &self.state.carried_equipment,
            self.state.first_equipment_visit,
            &mut self.state.rng,
        )?;
        self.state.host_activity = Some(HostActivity::EquipmentWorkshop {
            workshop,
            definitions,
        });
        self.refresh_host_activity();
        Ok(())
    }

    pub(super) fn resume_equipment_workshop(&mut self, input: VmInput) -> GameResponse {
        let Some(HostActivity::EquipmentWorkshop {
            workshop,
            definitions,
        }) = self.state.host_activity.as_mut()
        else {
            unreachable!()
        };
        let mut finished = false;
        let mut events = Vec::new();
        let result = if workshop.outcome.is_some() {
            if matches!(input, VmInput::Continue | VmInput::Choose { index: 0 }) {
                finished = true;
                Ok(())
            } else {
                Err("请继续阅读装备结算。".into())
            }
        } else if let VmInput::Choose { index } = input {
            match workshop.mode.as_deref() {
                None => match index {
                    0 => workshop.set_mode("purchase"),
                    1 => workshop.set_mode("research"),
                    2 => {
                        finished = true;
                        Ok(())
                    }
                    _ => Err("请选择购买、研究或离开。".into()),
                },
                Some("purchase") => match index {
                    0 => {
                        let definition = definitions
                            .iter()
                            .find(|definition| definition.id == workshop.offered_id)
                            .cloned()
                            .ok_or("Missing original equipment offer");
                        match definition {
                            Err(error) => Err(error.into()),
                            Ok(definition) => {
                                let player = self.state.legacy_player.as_mut().unwrap();
                                let mut money = player.i32(0x4b648a).unwrap();
                                let outcome = workshop.purchase(
                                    &definition,
                                    &mut self.state.known_equipment,
                                    &self.state.carried_equipment,
                                    &mut self.state.first_equipment_visit,
                                    &mut money,
                                );
                                match outcome {
                                    Err(error) => Err(error),
                                    Ok(outcome) => {
                                        player.set_i32(0x4b648a, money).unwrap();
                                        events.push(json!({"type":"equipmentPurchase","id":definition.id,"outcome":outcome}));
                                        Ok(())
                                    }
                                }
                            }
                        }
                    }
                    1 => {
                        finished = true;
                        Ok(())
                    }
                    _ => Err("请选择购买或离开。".into()),
                },
                Some("research") => {
                    if index == 18 {
                        finished = true;
                        Ok(())
                    } else {
                        let definition = definitions
                            .iter()
                            .find(|definition| definition.id == index)
                            .cloned()
                            .ok_or("请选择携带中的装备。");
                        match definition {
                            Err(error) => Err(error.into()),
                            Ok(definition) => {
                                let player = self.state.legacy_player.as_mut().unwrap();
                                let mut money = player.i32(0x4b648a).unwrap();
                                match workshop.research(
                                    &definition,
                                    &mut self.state.known_equipment,
                                    &mut self.state.carried_equipment,
                                    &mut money,
                                ) {
                                    Err(error) => Err(error),
                                    Ok(outcome) => {
                                        player.set_i32(0x4b648a, money).unwrap();
                                        events.push(json!({"type":"equipmentResearch","id":definition.id,"outcome":outcome}));
                                        Ok(())
                                    }
                                }
                            }
                        }
                    }
                }
                _ => Err("装备商店状态无效。".into()),
            }
        } else {
            Err("请选择装备操作。".into())
        };
        if let Err(error) = result {
            return self.response(Some(("equipment_input", error)), vec![]);
        }
        self.state.revision += 1;
        self.sync_player();
        if finished {
            self.state.host_activity = None;
            self.state.pending = None;
            if let Err(error) = self
                .state
                .vm
                .as_mut()
                .ok_or("Missing equipment continuation")
                .and_then(|vm| {
                    vm.resume(VmInput::HostReturned { next_node: None })
                        .map_err(|_| "Invalid equipment continuation")
                })
            {
                self.block("equipment_continuation", error.into(), Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
        } else {
            self.refresh_host_activity();
        }
        self.response(None, events)
    }

    pub(super) fn resume_fortune(&mut self, input: VmInput) -> GameResponse {
        let Some(HostActivity::Fortune { game }) = self.state.host_activity.as_mut() else {
            unreachable!()
        };
        let result = match (game.phase, input) {
            (FortunePhase::Question, VmInput::Choose { index }) => {
                let count = game.record.questions[game.question_index].options.len();
                if index == count {
                    game.cancel()
                } else {
                    game.answer(index)
                }
            }
            (FortunePhase::Explanation, VmInput::Continue)
            | (FortunePhase::Explanation, VmInput::Choose { index: 0 }) => game.advance(),
            (FortunePhase::Explanation, VmInput::Choose { index: 1 }) => game.cancel(),
            _ => Err("请选择当前测验的答案或继续阅读解说。".into()),
        };
        if let Err(error) = result {
            return self.response(Some(("fortune_input", error)), vec![]);
        }
        self.state.revision += 1;
        if matches!(game.phase, FortunePhase::Finished | FortunePhase::Cancelled) {
            let cancelled = game.phase == FortunePhase::Cancelled;
            let record_id = game.record.id;
            let percentages = game.percentages;
            if !cancelled {
                let player = self.state.legacy_player.as_mut().unwrap();
                let addresses = [0x4b6490, 0x4b6492, 0x4b648e, 0x4b6494];
                let current = addresses.map(|address| player.i16(address).unwrap());
                for (address, value) in addresses
                    .into_iter()
                    .zip(game.final_stats(current).unwrap())
                {
                    player.set_i16(address, value).unwrap();
                }
            }
            self.state.fortune_visits = (self.state.fortune_visits + 1).min(11);
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"fortuneCompleted","recordId":record_id,"cancelled":cancelled,"percentages":percentages}),
            ];
            if let Err(error) = self
                .state
                .vm
                .as_mut()
                .ok_or("Missing fortune continuation")
                .and_then(|vm| {
                    vm.resume(VmInput::HostReturned { next_node: None })
                        .map_err(|_| "Invalid fortune continuation")
                })
            {
                self.block("fortune_continuation", error.into(), Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn resume_minigame(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
        point: Option<(i32, i32)>,
        keys: Option<(bool, bool, bool)>,
    ) -> GameResponse {
        if self.state.phase != "event" {
            return self.response(
                Some(("wrong_phase", "当前没有正在进行的小游戏。".into())),
                vec![],
            );
        }
        if matches!(
            self.state.host_activity,
            Some(HostActivity::SpiritLottery { .. })
        ) {
            self.resume_spirit_lottery(elapsed, action)
        } else if matches!(self.state.host_activity, Some(HostActivity::Hammer { .. })) {
            self.resume_hammer(elapsed, action, index, point)
        } else if matches!(self.state.host_activity, Some(HostActivity::Fist { .. })) {
            self.resume_fist(elapsed, action, keys)
        } else if matches!(self.state.host_activity, Some(HostActivity::Gas { .. })) {
            self.resume_gas(elapsed, action)
        } else if matches!(self.state.host_activity, Some(HostActivity::Fire { .. })) {
            self.resume_fire(elapsed, action)
        } else if matches!(self.state.host_activity, Some(HostActivity::Firebee { .. })) {
            self.resume_firebee(elapsed, action, keys)
        } else {
            self.resume_tracking(elapsed, action, index)
        }
    }

    pub(super) fn resume_firebee(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        keys: Option<(bool, bool, bool)>,
    ) -> GameResponse {
        let Some(HostActivity::Firebee { game }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_minigame", "当前没有射击游戏。".into())),
                vec![],
            );
        };
        let valid = if let Some(elapsed) = elapsed {
            game.tick(elapsed, &mut self.state.rng);
            true
        } else {
            match action {
                Some("start") => game.start(),
                Some("help") => game.show_help(),
                Some("input") => {
                    if game.phase != FirebeePhase::Playing {
                        false
                    } else {
                        let (left, right, fire) = keys.unwrap_or((false, false, false));
                        game.input(left, right, fire);
                        true
                    }
                }
                Some("finishRewardAnimation") => {
                    let valid = game.phase == FirebeePhase::Reward;
                    game.finish_reward_animation();
                    valid
                }
                _ => false,
            }
        };
        if !valid {
            return self.response(
                Some(("minigame_input", "当前射击游戏无法执行这项操作。".into())),
                vec![],
            );
        }
        self.state.revision += 1;
        if game.phase == FirebeePhase::Finished {
            let money = game.final_money().unwrap();
            let score = game.score;
            let failed = game.failed;
            let delta = money.wrapping_sub(game.starting_money);
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .set_i32(0x4b648a, money)
                .unwrap();
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"minigameCompleted","game":"firebee","failed":failed,"score":score,"moneyDelta":delta}),
            ];
            if let Err(error) = self.trigger_skills(2, &mut events).and_then(|_| {
                self.state
                    .vm
                    .as_mut()
                    .ok_or("Missing minigame continuation")?
                    .resume(VmInput::HostReturned { next_node: None })
            }) {
                self.block("minigame_continuation", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn resume_fire(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
    ) -> GameResponse {
        let Some(HostActivity::Fire { game }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_minigame", "当前没有生火游戏。".into())),
                vec![],
            );
        };
        let result = if let Some(elapsed) = elapsed {
            game.tick(elapsed, &mut self.state.rng)
        } else {
            match action {
                Some("start") => game.start(),
                Some("left") => game.input(FireInput::Left),
                Some("right") => game.input(FireInput::Right),
                Some("releaseLeft") => game.input(FireInput::ReleaseLeft),
                Some("releaseRight") => game.input(FireInput::ReleaseRight),
                Some("finishRewardAnimation") => game.finish_reward_animation(),
                _ => Err("小游戏操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("minigame_input", error)), vec![]);
        }
        self.state.revision += 1;
        if game.reward_started {
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .set_i16(0x4b6494, game.charm)
                .unwrap();
        }
        if game.phase == FirePhase::Finished {
            let charm = game.final_charm().unwrap();
            let delta = charm.wrapping_sub(game.starting_charm);
            let failed = game.failed;
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .set_i16(0x4b6494, charm)
                .unwrap();
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"minigameCompleted","game":"fire","failed":failed,"charmDelta":delta}),
            ];
            if let Err(error) = self.trigger_skills(2, &mut events).and_then(|_| {
                self.state
                    .vm
                    .as_mut()
                    .ok_or("Missing minigame continuation")?
                    .resume(VmInput::HostReturned { next_node: None })
            }) {
                self.block("minigame_continuation", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.sync_player();
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn resume_gas(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
    ) -> GameResponse {
        let Some(HostActivity::Gas { game }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_minigame", "当前没有狗熊捕鱼游戏。".into())),
                vec![],
            );
        };
        let result = if let Some(elapsed) = elapsed {
            game.tick(elapsed, &mut self.state.rng)
        } else {
            match action {
                Some("start") => game.start(),
                Some("help") => game.show_help(),
                Some("throw") => game.throw(),
                Some("finishRewardAnimation") => game.finish_reward_animation(),
                _ => Err("小游戏操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("minigame_input", error)), vec![]);
        }
        self.state.revision += 1;
        if game.phase == GasPhase::Finished {
            let successes = game.successes;
            self.state.gas_target_state = Some(game.target_state);
            let player = self.state.legacy_player.as_mut().unwrap();
            let old = player.i16(0x4b648e).unwrap();
            let stamina = game.final_stamina(old).unwrap();
            player.set_i16(0x4b648e, stamina).unwrap();
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"minigameCompleted","game":"gas","successes":successes,"staminaDelta":stamina-old}),
            ];
            if let Err(error) = self.trigger_skills(2, &mut events).and_then(|_| {
                self.state
                    .vm
                    .as_mut()
                    .ok_or("Missing minigame continuation")?
                    .resume(VmInput::HostReturned { next_node: None })
            }) {
                self.block("minigame_continuation", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn resume_fist(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        keys: Option<(bool, bool, bool)>,
    ) -> GameResponse {
        let Some(HostActivity::Fist { game }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_minigame", "当前没有拳击游戏。".into())),
                vec![],
            );
        };
        let result = if let Some(elapsed) = elapsed {
            game.tick(elapsed, &mut self.state.rng)
        } else {
            match action {
                Some("start") => game.start(),
                Some("help") => game.show_help(),
                Some("input") => {
                    if game.phase != FistPhase::Playing {
                        Err("请先开始拳击游戏。".into())
                    } else {
                        let (left, right, fire) = keys.unwrap_or((false, false, false));
                        game.input(left, right, fire);
                        Ok(())
                    }
                }
                Some("finishRewardAnimation") => game.finish_reward_animation(),
                _ => Err("小游戏操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("minigame_input", error)), vec![]);
        }
        self.state.revision += 1;
        if game.phase == FistPhase::Finished {
            let score = game.display_score();
            let player = self.state.legacy_player.as_mut().unwrap();
            let old = player.i16(0x4b6490).unwrap();
            let morality = game.final_morality(old).unwrap();
            player.set_i16(0x4b6490, morality).unwrap();
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"minigameCompleted","game":"fist","score":score,"moralityDelta":morality-old}),
            ];
            if let Err(error) = self.trigger_skills(2, &mut events).and_then(|_| {
                self.state
                    .vm
                    .as_mut()
                    .ok_or("Missing minigame continuation")?
                    .resume(VmInput::HostReturned { next_node: None })
            }) {
                self.block("minigame_continuation", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn resume_hammer(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
        point: Option<(i32, i32)>,
    ) -> GameResponse {
        if self.state.phase != "event" {
            return self.response(
                Some(("wrong_phase", "当前没有正在进行的小游戏。".into())),
                vec![],
            );
        }
        let Some(HostActivity::Hammer { game }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_minigame", "当前没有槌击游戏。".into())),
                vec![],
            );
        };
        let result = if let Some(elapsed) = elapsed {
            game.tick(elapsed, &mut self.state.rng)
        } else {
            match action {
                Some("start") => game.start(),
                Some("strikeIndex") => index
                    .ok_or_else(|| "请选择一个目标。".to_string())
                    .and_then(|index| game.strike_index(index).map(|_| ())),
                Some("strike") => point
                    .ok_or_else(|| "缺少槌击位置。".to_string())
                    .and_then(|(x, y)| game.strike(x, y).map(|_| ())),
                Some("finishRewardAnimation") => game.finish_reward_animation(),
                _ => Err("小游戏操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("minigame_input", error)), vec![]);
        }
        self.state.revision += 1;
        if game.phase == HammerPhase::Finished {
            let score = game.score;
            let player = self.state.legacy_player.as_mut().unwrap();
            let old = player.i16(0x4b6490).unwrap();
            let morality = game.final_morality(old).unwrap();
            player.set_i16(0x4b6490, morality).unwrap();
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"minigameCompleted","game":"hammer","score":score,"moralityDelta":morality-old}),
            ];
            if let Err(error) = self.trigger_skills(2, &mut events).and_then(|_| {
                self.state
                    .vm
                    .as_mut()
                    .ok_or("Missing minigame continuation")?
                    .resume(VmInput::HostReturned { next_node: None })
            }) {
                self.block("minigame_continuation", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn resume_tracking(
        &mut self,
        elapsed: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
    ) -> GameResponse {
        if self.state.phase != "event" {
            return self.response(
                Some(("wrong_phase", "当前没有正在进行的小游戏。".into())),
                vec![],
            );
        }
        let Some(HostActivity::Tracking { game }) = self.state.host_activity.as_mut() else {
            return self.response(
                Some(("wrong_minigame", "当前没有五物追踪游戏。".into())),
                vec![],
            );
        };
        let result = if let Some(elapsed) = elapsed {
            game.tick(elapsed, &mut self.state.rng)
        } else {
            match action {
                Some("startShuffle") => game.start_shuffle(&mut self.state.rng),
                Some("choose") => index
                    .ok_or_else(|| "请选择一个目标。".to_string())
                    .and_then(|index| game.choose(index).map(|_| ())),
                Some("nextRound") => game.next_round(&mut self.state.rng),
                Some("finishIdleAnimation") => {
                    game.finish_idle_animation();
                    Ok(())
                }
                Some("finishRewardAnimation") => game.finish_reward_animation(),
                _ => Err("小游戏操作无效。".into()),
            }
        };
        if let Err(error) = result {
            return self.response(Some(("minigame_input", error)), vec![]);
        }
        self.state.revision += 1;
        if game.phase == TrackingPhase::Finished {
            let correct = game.correct;
            let player = self.state.legacy_player.as_mut().unwrap();
            let old = player.i16(0x4b6492).unwrap();
            let intelligence = game.final_intelligence(old).unwrap();
            player.set_i16(0x4b6492, intelligence).unwrap();
            self.state.host_activity = None;
            self.state.pending = None;
            let mut events = vec![
                json!({"type":"minigameCompleted","game":"tracking","correct":correct,"intelligenceDelta":intelligence-old}),
            ];
            if let Err(error) = self.trigger_skills(2, &mut events).and_then(|_| {
                self.state
                    .vm
                    .as_mut()
                    .ok_or("Missing minigame continuation")?
                    .resume(VmInput::HostReturned { next_node: None })
            }) {
                self.block("minigame_continuation", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            self.sync_player();
            return self.response(None, events);
        }
        self.refresh_host_activity();
        self.response(None, vec![])
    }

    pub(super) fn complete_school_exam(
        &mut self,
        result: u8,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let stage = self.state.legacy_player.as_ref().unwrap().life().stage;
        if stage == 3 && result > 0 {
            self.state.study_origin = Some(MapPosition {
                map_id: self.state.map_id.clone().unwrap(),
                tile_id: self.state.tile_id.unwrap(),
                facing: self.state.map_facing,
            });
        }
        if result == 2 {
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .set_i16(0x4b64b8, 1)?;
        }
        let next = match result {
            0 => 6,
            1 => 5,
            2 => 4,
            _ => return Err("Invalid original exam result".into()),
        };
        self.transition_stage(next, events)?;
        if next == 4 {
            if let Some(origin) = &self.state.study_origin {
                self.state.map_id = Some(origin.map_id.clone());
                self.state.tile_id = Some(origin.tile_id);
                self.state.entry_step_pending = true;
                self.state.map_facing = origin.facing;
            }
        }
        if self.state.host_activity.is_none() {
            self.refresh_spirit_positions(events)?;
            self.state.legacy_player.as_mut().unwrap().update_skills()?;
            self.sync_player();
            self.state.phase = "playing".into();
        }
        Ok(())
    }

    pub(super) fn status_overview(&self) -> Result<Value, String> {
        crate::status::overview(
            self.state
                .legacy_player
                .as_ref()
                .ok_or("Missing status player")?,
            &self.state.housing,
            self.state.spouse_index,
            self.state
                .active_mission
                .as_ref()
                .map(|mission| mission.index),
            self.rules.as_ref().ok_or("Missing status tables")?,
        )
    }
    pub(super) fn open_status(&mut self) -> GameResponse {
        if self.state.phase != "playing"
            || self.state.host_activity.is_some()
            || self.state.pending.is_some()
        {
            return self.response(Some(("wrong_phase", "请先完成当前事件。".into())), vec![]);
        }
        if let Err(error) = self.status_overview() {
            return self.response(Some(("status_data", error)), vec![]);
        }
        self.state.host_activity = Some(HostActivity::Status {
            panel: StatusPanel::default(),
            continuation: None,
        });
        self.state.revision += 1;
        self.refresh_host_activity();
        self.response(None, vec![])
    }
    pub(super) fn begin_property_preview(
        &mut self,
        page: StatusPage,
        continuation: StatusContinuation,
    ) -> Result<(), String> {
        let overview = self.status_overview()?;
        let mut panel = StatusPanel::default();
        panel.page(page, &overview, &mut self.state.rng)?;
        self.state.host_activity = Some(HostActivity::Status {
            panel,
            continuation: Some(continuation),
        });
        self.refresh_host_activity();
        Ok(())
    }
    pub(super) fn resume_status(
        &mut self,
        elapsed_ms: Option<u32>,
        action: Option<&str>,
        page: Option<&str>,
        delta: i32,
        index: Option<usize>,
    ) -> GameResponse {
        let Some(HostActivity::Status {
            mut panel,
            continuation,
        }) = self.state.host_activity.clone()
        else {
            return self.response(
                Some(("wrong_phase", "当前没有打开参数页面。".into())),
                vec![],
            );
        };
        let mut closed = false;
        let result: Result<(), String> = (|| {
            if let Some(elapsed) = elapsed_ms {
                panel.tick(elapsed, &mut self.state.rng);
                closed = continuation.is_some() && panel.page == StatusPage::Overview;
                return Ok(());
            }
            if panel.transition.is_some() {
                return Err("请等待住宅转场结束。".into());
            }
            match action {
                Some("close") => {
                    closed = panel.close() || continuation.is_some();
                }
                Some("page") => {
                    if continuation.is_some() {
                        return Err("请先关闭当前房产预览。".into());
                    }
                    let overview = self.status_overview()?;
                    panel.page(
                        StatusPage::parse(page.ok_or("请选择参数页面。")?)?,
                        &overview,
                        &mut self.state.rng,
                    )?;
                }
                Some("scroll") => {
                    panel.scroll(delta, index, self.state.career_history.len())?;
                }
                _ => return Err("未知的参数页面操作。".into()),
            }
            Ok(())
        })();
        if let Err(error) = result {
            return self.response(Some(("status_input", error)), vec![]);
        }
        let mut events = Vec::new();
        self.state.revision += 1;
        if closed {
            self.state.host_activity = None;
            self.state.pending = None;
            self.state.scene = json!({});
            match continuation {
                Some(StatusContinuation::NativeShop { shop, bonus }) => {
                    if let Some((address, value)) = bonus {
                        self.state
                            .legacy_player
                            .as_mut()
                            .unwrap()
                            .set_i16(address, value)
                            .unwrap();
                        self.sync_player();
                    }
                    self.state.host_activity = Some(HostActivity::NativeShop { shop });
                    self.refresh_host_activity();
                }
                Some(StatusContinuation::NativeReturn) => {
                    if let Some(vm) = self.state.vm.as_mut() {
                        if let Err(error) = vm.resume(VmInput::HostReturned { next_node: None }) {
                            self.block("native_resume", error, Value::Null);
                        } else {
                            self.advance_vm(&mut events);
                        }
                    } else {
                        self.state.phase = "playing".into();
                    }
                }
                None => self.state.phase = "playing".into(),
            }
        } else {
            self.state.host_activity = Some(HostActivity::Status {
                panel,
                continuation,
            });
            self.refresh_host_activity();
        }
        self.response(None, events)
    }
    pub(super) fn inventory_rules(&self) -> Result<InventoryRules, String> {
        serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("inventory"))
                .ok_or("Missing original inventory tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())
    }
    pub(super) fn open_inventory(&mut self, tab: Option<&str>) -> GameResponse {
        if self.state.phase != "playing"
            || self.state.host_activity.is_some()
            || self.state.pending.is_some()
        {
            return self.response(Some(("wrong_phase", "请先完成当前事件。".into())), vec![]);
        }
        let tab = match InventoryTab::parse(tab.unwrap_or("ordinary")) {
            Ok(tab) => tab,
            Err(error) => return self.response(Some(("inventory_tab", error)), vec![]),
        };
        if let Err(error) = self.inventory_rules() {
            return self.response(Some(("inventory_rules", error)), vec![]);
        }
        if self.state.battle_persistent.is_none() {
            let rules = match self.battle_rules() {
                Ok(rules) => rules,
                Err(error) => return self.response(Some(("inventory_rules", error)), vec![]),
            };
            self.state.battle_persistent = Some(BattlePersistent::new(&rules));
        }
        let panel = InventoryPanel::new(
            tab,
            &self.state.inventory.ordinary,
            &self.state.inventory.special,
            &self.state.carried_equipment,
            &self.state.battle_persistent.as_ref().unwrap().equipped_ids,
        );
        self.state.host_activity = Some(HostActivity::Inventory { panel });
        self.state.revision += 1;
        self.refresh_host_activity();
        self.response(None, vec![])
    }
    pub(super) fn resume_inventory(
        &mut self,
        elapsed_ms: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
        tab: Option<&str>,
    ) -> GameResponse {
        let Some(HostActivity::Inventory { mut panel }) = self.state.host_activity.clone() else {
            return self.response(Some(("wrong_phase", "当前没有打开物品栏。".into())), vec![]);
        };
        let rules = match self.inventory_rules() {
            Ok(rules) => rules,
            Err(error) => return self.response(Some(("inventory_rules", error)), vec![]),
        };
        if panel.confirm_drop
            && elapsed_ms.is_none()
            && !matches!(action, Some("confirmDrop" | "cancelDrop"))
        {
            return self.response(
                Some(("inventory_confirm", "请先回答是否丢弃这件物品。".into())),
                vec![],
            );
        }
        let mut events = vec![];
        let mut rebuild = false;
        let result: Result<(), String> = (|| {
            if let Some(elapsed) = elapsed_ms {
                panel.tick(elapsed, &rules);
                return Ok(());
            }
            match action {
                Some("close") => {}
                Some("tab") => {
                    let tab = InventoryTab::parse(tab.ok_or("请选择物品分类。")?)?;
                    let clock = panel.clock_ms;
                    panel = InventoryPanel::new(
                        tab,
                        &self.state.inventory.ordinary,
                        &self.state.inventory.special,
                        &self.state.carried_equipment,
                        &self
                            .state
                            .battle_persistent
                            .as_ref()
                            .ok_or("Missing equipment state")?
                            .equipped_ids,
                    );
                    panel.clock_ms = clock;
                }
                Some("select") => {
                    let selected = index
                        .filter(|index| *index < panel.entries.len())
                        .ok_or("这件物品不存在。")?;
                    panel.selected_index = Some(selected);
                }
                Some("drop") => {
                    panel.selected_id()?;
                    panel.confirm_drop = true;
                }
                Some("cancelDrop") => {
                    if !panel.confirm_drop {
                        return Err("当前没有待确认的丢弃操作。".into());
                    }
                    panel.confirm_drop = false;
                }
                Some("confirmDrop") => {
                    if !panel.confirm_drop {
                        return Err("请先选择要丢弃的物品。".into());
                    }
                    let id = panel.selected_id()?;
                    match panel.tab {
                        InventoryTab::Ordinary => {
                            inventory::drop_ordinary(id, &mut self.state.inventory.ordinary)?
                        }
                        InventoryTab::Special => {
                            inventory::drop_special(id, &mut self.state.inventory.special)?
                        }
                        InventoryTab::Equipment => inventory::drop_equipment(
                            id,
                            &mut self.state.known_equipment,
                            &mut self.state.carried_equipment,
                            &mut self
                                .state
                                .battle_persistent
                                .as_mut()
                                .ok_or("Missing equipment state")?
                                .equipped_ids,
                        )?,
                    }
                    panel.confirm_drop = false;
                    panel.preview_origin = crate::inventory_view::PreviewOrigin {
                        x: if panel.tab == InventoryTab::Equipment {
                            312
                        } else {
                            302
                        },
                        y: 47,
                    };
                    rebuild = true;
                    events.push(
                        json!({"type":"inventoryChanged","action":"drop","tab":panel.tab,"id":id}),
                    );
                }
                Some("use") => {
                    if panel.tab != InventoryTab::Ordinary {
                        return Err("这种物品不能直接使用。".into());
                    }
                    let id = panel.selected_id()?;
                    let player = self
                        .state
                        .legacy_player
                        .as_mut()
                        .ok_or("Missing inventory player")?;
                    let mut hp = player.i16(0x4b6486)?;
                    let max_hp = player.i16(0x4b6484)?;
                    inventory::use_ordinary(
                        id,
                        &mut self.state.inventory.ordinary,
                        &mut hp,
                        max_hp,
                        &rules,
                    )?;
                    player.set_i16(0x4b6486, hp)?;
                    panel.preview_origin = crate::inventory_view::PreviewOrigin { x: 294, y: 40 };
                    rebuild = true;
                    events.push(json!({"type":"inventoryChanged","action":"use","id":id,"hp":hp}));
                }
                Some("wear" | "unequip") => {
                    if panel.tab != InventoryTab::Equipment {
                        return Err("请先选择装备。".into());
                    }
                    let id = panel.selected_id()?;
                    let equipped = &mut self
                        .state
                        .battle_persistent
                        .as_mut()
                        .ok_or("Missing equipment state")?
                        .equipped_ids;
                    if action == Some("wear") {
                        inventory::wear_equipment(id, &self.state.carried_equipment, equipped)?;
                    } else {
                        inventory::unequip_equipment(id, &self.state.carried_equipment, equipped)?;
                    }
                    for entry in &mut panel.entries {
                        entry.equipped = equipped.get(entry.id / 3) == Some(&(entry.id as i16));
                    }
                    events.push(json!({"type":"inventoryChanged","action":action,"id":id}));
                }
                _ => return Err("未知的物品栏操作。".into()),
            }
            Ok(())
        })();
        if let Err(error) = result {
            return self.response(Some(("inventory_input", error)), vec![]);
        }
        if rebuild {
            panel.rebuild(
                &self.state.inventory.ordinary,
                &self.state.inventory.special,
                &self.state.carried_equipment,
                &self.state.battle_persistent.as_ref().unwrap().equipped_ids,
            );
        }
        self.state.revision += 1;
        self.sync_player();
        if action == Some("close") {
            self.state.host_activity = None;
            self.state.pending = None;
            self.state.scene = json!({});
            self.state.phase = "playing".into();
        } else {
            self.state.host_activity = Some(HostActivity::Inventory { panel });
            self.refresh_host_activity();
        }
        self.response(None, events)
    }
    pub(super) fn battle_rules(&self) -> Result<BattleRules, String> {
        serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("battle"))
                .ok_or("Missing original combat tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())
    }
    pub(super) fn final_battle_rules(&self) -> Result<FinalBattleRules, String> {
        serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("finalBattle"))
                .ok_or("Missing original final-combat tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())
    }

    pub(super) fn begin_battle(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        let rules = self.battle_rules()?;
        let persistent = self
            .state
            .battle_persistent
            .clone()
            .unwrap_or_else(|| BattlePersistent::new(&rules));
        let player = self
            .state
            .legacy_player
            .as_ref()
            .ok_or("Missing combat player")?;
        let life = player.life();
        let input = BattleInput {
            hp: player.i16(0x4b6486)?,
            max_hp: player.i16(0x4b6484)?,
            experience: player.i16(0x4b6488)?,
            money: player.i32(0x4b648a)?,
            sex: player.i16(0x4b6498)? as usize,
            stage: life.stage,
            career: life.profession,
            career_level: life.profession_level,
            life_level: life.life_level,
            stamina: player.i16(0x4b648e)?,
            intelligence: player.i16(0x4b6492)?,
            morality: player.i16(0x4b6490)?,
            charm: player.i16(0x4b6494)?,
            mission_kind: player.i16(0x4b64ce)?,
            mission_target: player.i16(0x4b64cc)?,
            god: player.i16(0x4b64d6)?,
            traveling: life.traveling,
            invulnerable: false,
            equipped: persistent.equipped_ids.clone(),
            carried: self.state.carried_equipment.clone(),
        };
        let game = BattleGame::new(input, persistent, &rules, &mut self.state.rng)?;
        if game.input.hp > 0 && game.mission_started {
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .set_i16(0x4b64d0, 1)?;
        }
        if game.phase == BattlePhase::Finished {
            self.finish_battle(game, events)?;
        } else {
            self.state.host_activity = Some(HostActivity::Battle { game });
            self.refresh_host_activity();
        }
        Ok(())
    }
    pub(super) fn finish_battle(
        &mut self,
        game: BattleGame,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let result = game.return_code.ok_or("Original combat has not returned")?;
        let player = self
            .state
            .legacy_player
            .as_mut()
            .ok_or("Missing combat player")?;
        player.set_i16(0x4b6486, game.hp)?;
        player.set_i16(0x4b6488, game.experience as i16)?;
        player.set_i32(0x4b648a, game.money)?;
        player.add_i16(0x4b64bc, 1)?;
        if result == 1 {
            player.add_i16(0x4b64be, 1)?;
            player.add_i16(0x4b64f0, 1)?;
        }
        self.state.battle_persistent = Some(game.persistent);
        self.state.host_activity = None;
        self.state.pending = None;
        events.push(json!({"type":"battleCompleted","enemy":game.enemy,"result":result,"hp":game.hp,"money":game.money,"experience":game.experience}));
        if game.hp > 0 {
            self.trigger_skills(0, events)?;
        }
        self.state
            .vm
            .as_mut()
            .ok_or("Missing combat event continuation")?
            .resume(VmInput::HostReturned { next_node: None })?;
        self.sync_player();
        self.advance_vm(events);
        Ok(())
    }
    /// The fourteen native liquidation rows (441860). All products and sums
    /// wrap at their original signed 32-bit width before division.
    pub(super) fn final_battle_input(&self) -> Result<FinalBattleInput, String> {
        let player = self
            .state
            .legacy_player
            .as_ref()
            .ok_or("Missing final combat player")?;
        let life = player.life();
        let rules = self.rules.as_ref().ok_or("Missing original rules")?;
        let final_rules = &rules["finalBattle"];
        let housing: HousingCatalog =
            serde_json::from_value(rules["housing"].clone()).map_err(|error| error.to_string())?;
        let salary = if life.profession < 0 {
            0
        } else {
            rules["baseSalaries"]
                .get(life.profession as usize)
                .and_then(Value::as_i64)
                .ok_or("Missing final career salary")? as i32
        };
        let mut home = 0i32;
        if self.state.housing.style >= 0 {
            let style = self.state.housing.style as usize;
            home = housing
                .styles
                .get(style)
                .ok_or("Invalid final house style")?
                .price;
            for index in 0..self.state.housing.furniture.max(0) as usize {
                home = home.wrapping_add(
                    final_rules["furnitureValues"][style][index]
                        .as_i64()
                        .ok_or("Missing final furniture value")? as i32,
                );
            }
        }
        let progress = self.state.housing.investment_stage.saturating_sub(1) as usize;
        let mut investments = 0i32;
        for row in housing.investment_rows.iter().take(progress / 3) {
            for value in &row[..3] {
                investments = investments.wrapping_add(*value);
            }
        }
        if progress % 3 > 0 {
            let row = housing
                .investment_rows
                .get(progress / 3)
                .ok_or("Invalid final investment stage")?;
            for value in row.iter().take(progress % 3) {
                investments = investments.wrapping_add(*value);
            }
        }
        let spouse = if self.state.spouse_index < 0 {
            0
        } else {
            final_rules["spouseValues"][self.state.spouse_index as usize]
                .as_i64()
                .ok_or("Missing final spouse value")? as i32
        };
        let wins = self.state.battle_persistent.as_ref().map_or(0, |state| {
            state
                .enemies
                .iter()
                .fold(0i32, |sum, enemy| sum.wrapping_add(i32::from(enemy.wins)))
        });
        let valuation = [
            player.i32(0x4b648a)?,
            salary.wrapping_mul(3),
            home.wrapping_mul(2) / 10,
            investments.wrapping_mul(2) / 10,
            spouse / 10,
            i32::from(life.profession_level) * 1000,
            i32::from(life.life_level) * 80,
            i32::from(player.i16(0x4b6492)?) * 10,
            i32::from(player.i16(0x4b6490)?) * 10,
            i32::from(player.i16(0x4b6494)?) * 10,
            i32::from(player.i16(0x4b648e)?) * 10,
            wins.wrapping_mul(500),
            i32::from(player.i16(0x4b64f2)?),
            i32::from(player.i16(0x4b64e2)?) / 2 + i32::from(player.i16(0x4b64d4)?) / 2,
        ];
        Ok(FinalBattleInput {
            sex: player.i16(0x4b6498)? as usize,
            career: life.profession,
            life_level: life.life_level,
            equipped_count: self.state.battle_persistent.as_ref().map_or(0, |state| {
                state.equipped_ids.iter().filter(|&&id| id != -1).count() as i16
            }),
            valuation,
        })
    }
    pub(super) fn begin_final_battle(&mut self) -> Result<(), String> {
        let rules = self.final_battle_rules()?;
        self.state
            .legacy_player
            .as_mut()
            .ok_or("Missing final combat player")?
            .update_skills()?;
        let game = FinalBattleGame::new(self.final_battle_input()?, &rules)?;
        self.state
            .legacy_player
            .as_mut()
            .unwrap()
            .set_i32(0x4b648a, game.money)?;
        self.state.host_activity = Some(HostActivity::FinalBattle { game });
        self.sync_player();
        self.refresh_host_activity();
        Ok(())
    }
    pub(super) fn resume_battle(
        &mut self,
        elapsed_ms: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
    ) -> GameResponse {
        let Some(mut activity) = self.state.host_activity.clone() else {
            return self.response(
                Some(("wrong_phase", "当前没有正在进行的战斗。".into())),
                vec![],
            );
        };
        let result = match &mut activity {
            HostActivity::Battle { game } => {
                if let Some(elapsed) = elapsed_ms {
                    game.tick(elapsed, &mut self.state.rng);
                    Ok(())
                } else {
                    match action {
                        Some("continue" | "start") => game.start(),
                        Some("selectCard") => index
                            .ok_or("请选择卡片。".into())
                            .and_then(|index| game.select_card(index)),
                        Some("hoverCard") => game.hover_card(index),
                        Some("attack") => game.attack(),
                        Some("help") => game.show_help(),
                        Some("finishRewardAnimation") => {
                            game.skip_reward();
                            Ok(())
                        }
                        _ => Err("未知的战斗操作。".into()),
                    }
                }
            }
            HostActivity::FinalBattle { game } => match self.final_battle_rules() {
                Ok(rules) => {
                    if let Some(elapsed) = elapsed_ms {
                        game.tick(elapsed, &rules, &mut self.state.rng)
                    } else {
                        game.action(action.unwrap_or(""), index, &rules)
                    }
                }
                Err(error) => Err(error),
            },
            _ => Err("当前没有正在进行的战斗。".into()),
        };
        if let Err(error) = result {
            return self.response(Some(("battle_input", error)), vec![]);
        }
        let mut events = vec![];
        self.state.revision += 1;
        match activity {
            HostActivity::Battle { game } if game.phase == BattlePhase::Finished => {
                if let Err(error) = self.finish_battle(game, &mut events) {
                    self.block("battle_continuation", error, Value::Null);
                }
            }
            HostActivity::FinalBattle { game } => {
                self.state
                    .legacy_player
                    .as_mut()
                    .unwrap()
                    .set_i32(0x4b648a, game.money)
                    .unwrap();
                if game.finished {
                    if let Err(error) = self.complete_final_battle(game.hp > 0, &mut events) {
                        self.block("final_battle_continuation", error, Value::Null);
                    }
                } else {
                    self.state.host_activity = Some(HostActivity::FinalBattle { game });
                    self.refresh_host_activity();
                }
            }
            activity => {
                self.state.host_activity = Some(activity);
                self.refresh_host_activity();
            }
        }
        self.sync_player();
        self.response(None, events)
    }

    /// Continuation after FUN_00443b40. Defeat reincarnates after the ending;
    /// victory adds the original FUN_0041c040 yes/no choice.
    pub fn complete_final_battle(
        &mut self,
        victory: bool,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        self.state
            .legacy_player
            .as_mut()
            .ok_or("Missing final-battle player")?
            .update_skills()?;
        let rules: EndingSummaryRules = serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("endingSummary"))
                .ok_or("Missing original ending-summary tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        let player = self.state.legacy_player.as_ref().unwrap();
        let stage = player.life().stage;
        let career = player.i16(0x4b64a2)?;
        // The native constructor returns false outside stages 5/6, or when an
        // unemployed player's edm00/eum00 resources cannot be loaded. Its caller
        // proceeds directly to the movie, rather than blocking the ending.
        let summary = if stage == 5 || (stage == 6 && (0..26).contains(&career)) {
            Some(EndingSummary::new(
                victory,
                player.i16(0x4b6498)? as usize,
                stage,
                career,
                &self.ending_history(),
                &rules,
            )?)
        } else {
            None
        };
        let epilogue: EpilogueRules = serde_json::from_value(
            self.rules
                .as_ref()
                .and_then(|rules| rules.get("epilogue"))
                .ok_or("Missing original epilogue tables")?
                .clone(),
        )
        .map_err(|error| error.to_string())?;
        self.state.vm = None;
        self.state.pending_allocation = None;
        self.state.action_queue.clear();
        self.state.turn_in_progress = false;
        self.state.host_activity = Some(HostActivity::Ending {
            flow: EndingFlow::new(victory, summary, &epilogue),
        });
        self.sync_player();
        events.push(json!({"type":"lifeCompleted","victory":victory,"stats":self.state.stats,"life":self.state.legacy_player.as_ref().unwrap().life()}));
        self.refresh_host_activity();
        Ok(())
    }

    /// FUN_00414830 / FUN_0041b0c0 initialize the two school-history entries.
    pub(super) fn initial_life_history() -> [Vec<i32>; 16] {
        let mut history = std::array::from_fn(|_| Vec::new());
        history[1].push(2016);
        history[2].push(2019);
        history
    }
    /// FUN_004159a0 records these values before entering the final battle.
    pub fn prepare_final_history(&mut self) -> Result<(), String> {
        let player = self
            .state
            .legacy_player
            .as_ref()
            .ok_or("Missing final-battle player")?;
        let history = self
            .state
            .life_history
            .get_or_insert_with(Self::initial_life_history);
        history[14].push(i32::from(player.i16(0x4b64f0)?).min(99));
        history[15].push(i32::from(player.life().year));
        Ok(())
    }
    pub(super) fn ending_history(&self) -> [Vec<i32>; 16] {
        let mut history = self
            .state
            .life_history
            .clone()
            .unwrap_or_else(Self::initial_life_history);
        history[7] = self
            .state
            .promotion_years
            .iter()
            .copied()
            .map(i32::from)
            .collect();
        history[8] = self
            .state
            .housing
            .purchase_years
            .iter()
            .copied()
            .map(i32::from)
            .collect();
        for (index, year) in [11, 12, 10].into_iter().zip(self.state.relationship_years) {
            history[index] = year.into_iter().map(i32::from).collect();
        }
        history
    }

    /// State outside the packed character that FUN_0041b0c0 resets. Rank-five
    /// careers and fully mastered skills survive; belongings and history do not.
    pub(super) fn reincarnate(&mut self, events: &mut Vec<Value>) -> Result<(), String> {
        self.state
            .legacy_player
            .as_mut()
            .ok_or("Missing reincarnation player")?
            .reincarnate()?;
        for rank in &mut self.state.career_levels {
            if *rank < 5 {
                *rank = 1;
            }
        }
        self.state.inventory = Inventory::default();
        self.state.career_history.clear();
        self.state.promotion_years.clear();
        self.state.shop_visits = 0;
        self.state.spouse_index = -1;
        self.state.relationship_candidates.fill(-1);
        self.state.gift_level = 1;
        self.state.fortune_visits = 0;
        self.state.relationship_new_targets.fill(-1);
        self.state.dating_target = -1;
        self.state.relationship_years = [None; 3];
        self.state.known_equipment.fill(EquipmentKnowledge {
            learned: false,
            level: 0,
        });
        self.state
            .carried_equipment
            .fill(EquipmentSlot { id: -1, level: 0 });
        self.state.first_equipment_visit = true;
        self.state.housing = HousingState::default();
        self.state.active_mission = None;
        self.state.active_condition = None;
        self.state.battle_persistent = None;
        self.state.life_history = Some(Self::initial_life_history());
        self.state.pending_allocation = None;
        self.state.host_activity = None;
        self.state.pending = None;
        self.state.vm = None;
        self.state.travel_origin = None;
        self.state.study_origin = None;
        self.state.action_queue.clear();
        self.state.queued_die = None;
        self.state.turn_in_progress = false;
        self.state.spirits.active = None;
        self.state.spirits.expires_turn = 0;
        self.state.last_roll_steps = 0;
        self.transition_stage(2, events)?;
        self.refresh_spirit_positions(events)?;
        self.state
            .legacy_player
            .as_mut()
            .unwrap()
            .set_i16(0x4b64d6, -1)?;
        self.state.legacy_player.as_mut().unwrap().update_skills()?;
        self.sync_player();
        self.state.phase = "playing".into();
        self.state.message = "新的人生开始了。".into();
        events.push(json!({"type":"reincarnated","incarnation":self.state.legacy_player.as_ref().unwrap().life().incarnation}));
        Ok(())
    }

    pub(super) fn resume_ending(
        &mut self,
        elapsed_ms: Option<u32>,
        action: Option<&str>,
        index: Option<usize>,
    ) -> GameResponse {
        let Some(HostActivity::Ending { mut flow }) = self.state.host_activity.clone() else {
            return self.response(
                Some(("wrong_phase", "当前没有正在播放的人生结局。".into())),
                vec![],
            );
        };
        let mut events = Vec::new();
        if flow.phase == EndingPhase::Reincarnation && action == Some("choose") {
            match index {
                Some(0) => {
                    if let Err(error) = self.reincarnate(&mut events) {
                        self.block("reincarnation", error, Value::Null);
                    }
                }
                Some(1) => {
                    self.state.host_activity = None;
                    self.state.pending = None;
                    self.state.phase = "completed".into();
                    self.state.message = "这段人生已经圆满结束。".into();
                    events.push(json!({"type":"gameCompleted"}));
                }
                _ => {
                    return self.response(
                        Some(("ending_input", "请选择继续新的人生或结束游戏。".into())),
                        vec![],
                    );
                }
            }
        } else {
            if let Some(elapsed_ms) = elapsed_ms {
                let rules = self
                    .rules
                    .as_ref()
                    .and_then(|rules| rules.get("epilogue"))
                    .ok_or("Missing original epilogue tables")
                    .and_then(|value| {
                        serde_json::from_value::<EpilogueRules>(value.clone())
                            .map_err(|_| "Invalid original epilogue tables")
                    });
                let rules = match rules {
                    Ok(rules) => rules,
                    Err(error) => {
                        return self.response(Some(("ending_rules", error.into())), vec![]);
                    }
                };
                flow.tick(elapsed_ms, &rules);
            } else if let Some(action) = action {
                if let Err(error) = flow.action(action) {
                    return self.response(Some(("ending_input", error)), vec![]);
                }
            }
            if flow.phase == EndingPhase::Finished {
                if let Err(error) = self.reincarnate(&mut events) {
                    self.block("reincarnation", error, Value::Null);
                }
            } else {
                self.state.host_activity = Some(HostActivity::Ending { flow });
                self.refresh_host_activity();
            }
        }
        self.state.revision += 1;
        self.response(None, events)
    }

    pub(super) fn begin_mission(&mut self) -> Result<(), String> {
        if let Some(mission) = &mut self.state.active_mission {
            mission.multiplier = mission.multiplier.wrapping_add(1);
            self.state.scene = json!({"backgroundResource":"install/bmp/money.bmp"});
            return self.show_notice(vec![24999], NoticeContinuation::Native);
        }
        let player = self.state.legacy_player.as_mut().unwrap();
        let selected = select_mission(
            &|address| {
                if address == 0x4b64da {
                    player.i32(address).ok()
                } else {
                    player.i16(address).ok().map(i32::from)
                }
            },
            &mut || self.state.rng.next_rand(),
        )?;
        for write in selected.writes {
            player.set_i16(write.address, write.value as i16)?;
        }
        let Some(index) = selected.mission_index else {
            return self.show_notice(vec![25014], NoticeContinuation::Native);
        };
        let record = self
            .rules
            .as_ref()
            .and_then(|rules| rules["missions"].as_array())
            .and_then(|missions| missions.get(index as usize))
            .ok_or("Missing original mission table")?;
        let words: Vec<i16> =
            serde_json::from_value(record["words"].clone()).map_err(|error| error.to_string())?;
        if words.len() != 11 {
            return Err("Original mission must contain eleven words".into());
        }
        let mut pages = Vec::new();
        if player.i16(0x4b64de)? == 1 {
            player.set_i16(0x4b64de, 0)?;
            pages.extend([25000, 25001, 25002]);
        } else if player.i16(0x4b64d4)? > 0
            && player.i16(0x4b64d4)? % 5 == 0
            && player.i32(0x4b64ec)? == 1
        {
            pages.push(i32::from(player.i16(0x4b64d4)?) / 5 + 25002);
            player.set_i32(0x4b64ec, 0)?;
        }
        pages.extend((0..words[3].max(1)).map(|offset| i32::from(words[0]) + i32::from(offset)));
        self.state.scene = json!({"backgroundResource":self.program.as_ref().unwrap().background_resource(i32::from(words[9])),"patternResource":"install/bmp/missionbg.bmp"});
        self.show_notice(
            pages,
            NoticeContinuation::AcceptMission {
                mission: ActiveMission {
                    index,
                    words,
                    multiplier: 1,
                },
            },
        )
    }

    pub(super) fn resolve_mission(
        &mut self,
        success: bool,
        events: &mut Vec<Value>,
    ) -> Result<(), String> {
        let mission = self
            .state
            .active_mission
            .as_ref()
            .ok_or("Condition has no original mission record")?;
        let player = self.state.legacy_player.as_ref().unwrap();
        let inventory = &self.state.inventory;
        let effects = self.program.as_ref().unwrap().mission_result_effects(
            i32::from(mission.words[if success { 7 } else { 8 }]),
            mission.multiplier,
            &|address| {
                inventory.read(address).or_else(|| {
                    if address == 0x4b648a {
                        player.i32(address).ok()
                    } else {
                        player.i16(address).ok().map(i32::from)
                    }
                })
            },
        )?;
        let player = self.state.legacy_player.as_mut().unwrap();
        for write in &effects.writes {
            if !self
                .state
                .inventory
                .write(write.address, write.value as i16)
            {
                if write.address == 0x4b648a {
                    player.set_i32(write.address, write.value)?;
                } else {
                    player.set_i16(write.address, write.value as i16)?;
                }
            }
        }
        if success {
            player.add_i16(0x4b64d4, 1)?;
        }
        player.set_i32(0x4b64ec, i32::from(success))?;
        player.set_i32(0x4b64d0, 0)?;
        player.set_i16(0x4b64cc, -1)?;
        player.set_i16(0x4b64ce, -1)?;
        events.push(json!({"type":"missionResolved","missionIndex":mission.index,"success":success,"effects":effects}));
        self.state.active_mission = None;
        self.state.active_condition = None;
        self.sync_player();
        Ok(())
    }
}
