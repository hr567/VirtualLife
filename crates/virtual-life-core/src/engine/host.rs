//! Original event VM continuation, queued actions and turn transitions.
use super::*;

impl GameSession {
    // 0040a190: zero preserves the current portrait. Only the left portrait
    // resolves 998..1006 through the player/career table at 00477258.
    fn event_portrait(&self, id: i32, left: bool) -> Option<String> {
        if left && (998..=1006).contains(&id) {
            let player = self.state.legacy_player.as_ref()?;
            let life = player.life();
            let index = i32::from(player.i16(0x4b6498).ok()?) * 31
                + i32::from(life.stage)
                + i32::from(life.profession.max(0));
            // Both original 62-entry tables contain identical file names;
            // the HUD table uses face/, while N1 loads bmp/.
            let path = self.rules.as_ref()?["characters"]["portraits"]
                .get(usize::try_from(index).ok()?)?
                .as_str()?;
            let mut name = path.rsplit('/').next()?.as_bytes().to_vec();
            if id > 1000 {
                // The original adjusts byte 9 of "bmp\\umf02a.bmp".
                *name.get_mut(5)? = name[5].wrapping_add((id - 1001) as u8);
            }
            return Some(format!("install/bmp/{}", String::from_utf8(name).ok()?));
        }
        self.program.as_ref()?.portrait_resource(id)
    }

    fn update_event_portraits(&mut self, left: i32, right: i32) {
        for (id, is_left, key) in [(left, true, "left"), (right, false, "right")] {
            if id != 0 {
                let resource = self.event_portrait(id, is_left);
                self.state.scene[format!("{key}PortraitId")] = json!(id);
                self.state.scene[format!("{key}PortraitResource")] = json!(resource);
            }
        }
    }

    pub(super) fn event_scene_snapshot(&self) -> Value {
        let mut scene = self.state.scene.clone();
        if let Some(pending) = self
            .state
            .pending
            .as_ref()
            .filter(|v| v["kind"] == "dialogue")
        {
            // Previously saved Bevy dialogues retain the actual N1 portrait
            // arguments. Repair their read-only projection without rewriting
            // the saved state, checksum or zero/preserve semantics.
            for (key, left) in [("left", true), ("right", false)] {
                if let Some(id) = pending[format!("{key}PortraitId")]
                    .as_i64()
                    .filter(|id| *id != 0)
                {
                    scene[format!("{key}PortraitId")] = json!(id);
                    scene[format!("{key}PortraitResource")] =
                        json!(self.event_portrait(id as i32, left));
                }
            }
        }
        scene
    }
    pub(super) fn refresh_host_activity(&mut self) {
        match self.state.host_activity.as_ref() {
            Some(HostActivity::StatShop { shop }) => {
                let mut options=shop.offers.iter().enumerate().map(|(index,offer)|{
                    let benefit=if offer.group==4 && offer.item.amount<=0{"鲜花礼物".to_string()}else{format!("{} +{}",["魅力","智力","体能","道德","爱情"][offer.group],offer.item.amount)};
                    json!({"index":index,"text":format!("{} · {}元 · {}{}",offer.item.name,offer.item.price,benefit,if offer.bought{"（已购买）"}else{""}),"disabled":offer.bought||shop.purchases>=3})
                }).collect::<Vec<_>>();
                options.push(json!({"index":shop.offers.len(),"text":"离开商店"}));
                self.state.pending = Some(
                    json!({"kind":"choice","text":format!("属性商店 · 本次已购买 {}/3 件",shop.purchases),"options":options}),
                );
                self.state.scene = json!({"backgroundResource":"pm0g/pm0g01.bmp"});
                self.state.phase = "event".into();
            }
            Some(HostActivity::Career { careers, .. }) => {
                let player = self.state.legacy_player.as_ref().unwrap();
                let mut options=careers.iter().map(|career|json!({"index":career.id,"text":career.name,"disabled":!career.eligible(player,&self.state.career_levels).unwrap_or(false)})).collect::<Vec<_>>();
                options.push(json!({"index":24,"text":"离开人才市场"}));
                self.state.pending =
                    Some(json!({"kind":"choice","text":"选择职业","options":options}));
                self.state.phase = "event".into();
            }
            Some(HostActivity::Notice { pages, page, .. }) => {
                let text = split_dialogue(
                    self.program
                        .as_ref()
                        .unwrap()
                        .dialogue(pages[*page])
                        .unwrap(),
                );
                self.state.pending = Some(
                    json!({"kind":"dialogue","text":text.text,"dialogueId":pages[*page],"speaker":Value::Null}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::SchoolExam { exam }) => {
                if !exam.started {
                    self.state.pending = Some(
                        json!({"kind":"choice","text":"高中毕业考试：共十题，每题十分。是否参加？","options":[{"index":0,"text":"参加考试"},{"index":1,"text":"放弃考试，进入社会"}]}),
                    );
                } else {
                    self.state.pending = Some(
                        json!({"kind":"choice","text":format!("第 {}/10 题 · {} 分\n{}",exam.answered+1,exam.score,exam.current.question),"options":exam.current.options.iter().enumerate().map(|(index,text)|json!({"index":index,"text":text})).collect::<Vec<_>>()}),
                    );
                }
                self.state.phase = "event".into();
            }
            Some(HostActivity::Tracking { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"tracking","state":game,"text":"记住目标，追踪它移动后的位置。"}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::Hammer { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"hammer","state":game,"text":"击中普通目标或铠甲目标，避开友好目标。"}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::Fist { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"fist","state":game,"text":"左右移动，按空格出拳；连续命中可以获得更多分数。"}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::Gas { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"gas","state":game,"text":"狗熊捕鱼：看准大鱼的位置，按空格或回车捕鱼。共有三次机会。"}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::Firebee { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"firebee","state":game,"text":"用左右方向键移动，按空格射击。按 F1 查看说明。"}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::Fire { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"fire","state":game,"text":"交替按左右键，赶在时间耗尽前生起火。"}),
                );
                self.state.phase = "event".into();
            }
            Some(HostActivity::Ending { flow }) => {
                self.state.pending = Some(json!({"kind":"ending","state":flow,"text":"人生回顾"}));
                self.state.phase = "event".into();
                self.state.scene = json!({});
            }
            Some(HostActivity::Battle { game }) => {
                self.state.pending =
                    Some(json!({"kind":"battle","game":"ordinary","state":game,"text":game.text}));
                self.state.phase = "event".into();
                self.state.scene = json!({});
            }
            Some(HostActivity::FinalBattle { game }) => {
                self.state.pending =
                    Some(json!({"kind":"battle","game":"final","state":game,"text":"最终战斗"}));
                self.state.phase = "event".into();
                self.state.scene = json!({});
            }
            Some(HostActivity::Inventory { panel }) => {
                self.state.pending = Some(
                    json!({"kind":"inventory","state":panel,"ordinary":self.state.inventory.ordinary,"special":self.state.inventory.special,"carried":self.state.carried_equipment,"equipped":self.state.battle_persistent.as_ref().map(|persistent|&persistent.equipped_ids),"rules":self.rules.as_ref().and_then(|rules|rules.get("inventory"))}),
                );
                self.state.phase = "event".into();
                self.state.scene = json!({});
            }
            Some(HostActivity::Status {
                panel,
                continuation,
            }) => {
                self.state.pending = Some(
                    json!({"kind":"status","state":panel,"overview":self.status_overview().ok(),"housing":self.state.housing,"returnTo":match continuation{Some(StatusContinuation::NativeShop{..})=>"shop",Some(StatusContinuation::NativeReturn)=>"housing",None=>"overview"}}),
                );
                self.state.phase = "event".into();
                self.state.scene = json!({});
            }
            Some(HostActivity::Relationship { house }) => {
                self.state.pending = Some(
                    json!({"kind":"relationship","state":house,"question":house.question().ok(),"text":house.message}),
                );
                self.state.scene = json!({});
                self.state.phase = "event".into();
            }
            Some(HostActivity::SpiritEncounter { spirit }) => {
                self.state.pending = Some(
                    json!({"kind":"choice","text":"神灵将与你同行。","options":[{"index":0,"text":"继续"}],"spirit":spirit}),
                );
                self.state.phase = "event".into();
                self.state.scene = json!({"backgroundResource":format!("cd/data/spirit/sen_{:02}.bmp",[3,2,1,4,5][*spirit])});
            }
            Some(HostActivity::SpiritLottery { game }) => {
                self.state.pending = Some(
                    json!({"kind":"minigame","game":"spiritSlots","state":game,"text":"停止转盘，查看神灵的影响。"}),
                );
                self.state.phase = "event".into();
                self.state.scene = json!({});
            }
            Some(HostActivity::Fortune { game }) => {
                let (text, options) = if game.phase == FortunePhase::Question {
                    let question = &game.record.questions[game.question_index];
                    let mut options = question
                        .options
                        .iter()
                        .enumerate()
                        .map(|(index, text)| json!({"index":index,"text":text}))
                        .collect::<Vec<_>>();
                    options.push(json!({"index":options.len(),"text":"离开测验"}));
                    (
                        format!(
                            "{} · {}/{}\n{}",
                            game.record.title,
                            game.question_index + 1,
                            game.record.questions.len(),
                            question.prompt
                        ),
                        options,
                    )
                } else {
                    (
                        game.explanation.clone(),
                        vec![
                            json!({"index":0,"text":"继续"}),
                            json!({"index":1,"text":"离开测验"}),
                        ],
                    )
                };
                self.state.pending = Some(json!({"kind":"choice","text":text,"options":options}));
                self.state.phase = "event".into();
            }
            Some(HostActivity::EquipmentWorkshop {
                workshop,
                definitions,
            }) => {
                let (text, options) = if let Some(outcome) = workshop.outcome {
                    (
                        match outcome {
                            EquipmentOutcome::Purchased => "装备购买成功，请到道具商店领取后携带。",
                            EquipmentOutcome::Upgraded => "装备研究完成。",
                            EquipmentOutcome::InsufficientMoney => "持有金不足。",
                            EquipmentOutcome::CapacityReached => "拥有的战斗装备已达八件。",
                            EquipmentOutcome::MaximumLevel => "这件装备已达到最高等级。",
                        }
                        .to_owned(),
                        vec![json!({"index":0,"text":"继续"})],
                    )
                } else {
                    match workshop.mode.as_deref() {
                        Some("purchase") => {
                            let item = definitions
                                .iter()
                                .find(|item| item.id == workshop.offered_id)
                                .unwrap();
                            (
                                format!("{} · {}元\n{}", item.name, item.price, item.description),
                                vec![
                                    json!({"index":0,"text":"购买"}),
                                    json!({"index":1,"text":"离开"}),
                                ],
                            )
                        }
                        Some("research") => {
                            let mut options=self.state.carried_equipment.iter().filter(|slot|slot.id>=0).filter_map(|slot|definitions.iter().find(|item|item.id==slot.id as usize).map(|item|json!({"index":item.id,"text":format!("{} · 研究费用 {}元",item.name,item.price/2)}))).collect::<Vec<_>>();
                            options.push(json!({"index":18,"text":"离开"}));
                            ("选择要研究的装备".into(), options)
                        }
                        _ => (
                            "装备购买与研究".into(),
                            vec![
                                json!({"index":0,"text":"购买装备"}),
                                json!({"index":1,"text":"研究装备"}),
                                json!({"index":2,"text":"离开"}),
                            ],
                        ),
                    }
                };
                self.state.pending = Some(json!({"kind":"choice","text":text,"options":options}));
                self.state.phase = "event".into();
            }
            Some(HostActivity::NativeShop { shop }) => {
                let mut options=shop.offers.iter().enumerate().filter(|(_,offer)|!offer.purchased).map(|(index,offer)|json!({"index":index,"text":format!("{} · {}元\n{}",["家具","战斗装备领取","普通道具","职业道具"][offer.category as usize],offer.price,offer.description)})).collect::<Vec<_>>();
                options.push(json!({"index":shop.offers.len(),"text":"离开道具商店"}));
                self.state.pending =
                    Some(json!({"kind":"choice","text":"道具商店","options":options}));
                self.state.phase = "event".into();
            }
            Some(HostActivity::Housing {
                catalog, result, ..
            }) => {
                let (text, options) = if let Some(result) = result {
                    (result.clone(), vec![json!({"index":0,"text":"继续"})])
                } else if self.state.housing.investment_stage == 0 {
                    let mut options=catalog.styles.iter().map(|style|json!({"index":style.id,"text":format!("{}元 · {}",style.price,style.description)})).collect::<Vec<_>>();
                    options.push(json!({"index":4,"text":"离开"}));
                    ("选择住宅".into(), options)
                } else {
                    let mut options = Vec::new();
                    if let Ok(price) = self.state.housing.next_investment(catalog) {
                        options.push(json!({"index":5,"text":format!("投资 {}元",price)}));
                    }
                    if let Ok(income) = self.state.housing.income(catalog) {
                        options.push(json!({"index":6,"text":format!("领取房产收益 {}元",income)}));
                    }
                    options.push(json!({"index":7,"text":"离开"}));
                    (
                        catalog
                            .stage_descriptions
                            .get(usize::from(self.state.housing.investment_stage - 1))
                            .cloned()
                            .unwrap_or_default(),
                        options,
                    )
                };
                self.state.pending = Some(json!({"kind":"choice","text":text,"options":options}));
                self.state.phase = "event".into();
            }
            Some(HostActivity::Competition { game, .. }) => {
                let choices: Vec<Value> = match game.phase {
                    CompetitionPhase::Menu => ["造型大赛", "飞人大赛", "智力大赛"]
                        .iter()
                        .enumerate()
                        .map(|(index, text)| json!({"index":index,"text":text}))
                        .collect(),
                    CompetitionPhase::Intro
                    | CompetitionPhase::QuizIntro
                    | CompetitionPhase::QuizResult => vec![json!({"index":0,"text":"继续"})],
                    CompetitionPhase::QuizQuestion => game
                        .exam
                        .as_ref()
                        .unwrap()
                        .current
                        .options
                        .iter()
                        .enumerate()
                        .map(|(index, text)| json!({"index":index,"text":text}))
                        .collect(),
                    _ => vec![],
                };
                self.state.pending = Some(
                    json!({"kind":"competition","state":game,"text":game.text(),"choices":choices}),
                );
                self.state.phase = "event".into();
            }
            None => {}
        }
    }

    pub(super) fn resume_host_activity(&mut self, input: VmInput) -> GameResponse {
        if matches!(
            self.state.host_activity,
            Some(HostActivity::Battle { .. } | HostActivity::FinalBattle { .. })
        ) {
            return self.resume_battle(None, Some("continue"), None);
        }
        if let Some(HostActivity::Competition { game, .. }) = &self.state.host_activity {
            let action = match game.phase {
                CompetitionPhase::Menu => "choose",
                CompetitionPhase::QuizQuestion => "answer",
                _ => "continue",
            };
            return self.resume_competition(
                None,
                Some(action),
                match input {
                    VmInput::Choose { index } => Some(index),
                    _ => None,
                },
            );
        }
        if matches!(
            self.state.host_activity,
            Some(HostActivity::SpiritEncounter { .. })
        ) {
            return self.resume_spirit_encounter(input);
        }
        if let Some(HostActivity::Ending { flow }) = &self.state.host_activity {
            let (action, index) = match input {
                VmInput::Continue => ("continue", None),
                VmInput::Choose { index } if flow.phase == EndingPhase::Reincarnation => {
                    ("choose", Some(index))
                }
                VmInput::Choose { index: 0 } => ("continue", None),
                _ => {
                    return self
                        .response(Some(("ending_input", "无效的结局操作。".into())), vec![]);
                }
            };
            return self.resume_ending(None, Some(action), index);
        }
        if matches!(
            self.state.host_activity,
            Some(HostActivity::EquipmentWorkshop { .. })
        ) {
            return self.resume_equipment_workshop(input);
        }
        if matches!(self.state.host_activity, Some(HostActivity::Fortune { .. })) {
            return self.resume_fortune(input);
        }
        if matches!(
            self.state.host_activity,
            Some(HostActivity::SchoolExam { .. })
        ) {
            let VmInput::Choose { index } = input else {
                return self.response(
                    Some(("invalid_exam_input", "请选择考试答案。".into())),
                    vec![],
                );
            };
            let questions = match self.exam_questions() {
                Ok(questions) => questions,
                Err(error) => return self.response(Some(("exam_catalog", error)), vec![]),
            };
            let Some(HostActivity::SchoolExam { exam }) = self.state.host_activity.as_mut() else {
                unreachable!()
            };
            let mut result = None;
            let mut events = Vec::new();
            if !exam.started {
                match index {
                    0 => exam.started = true,
                    1 => result = Some(0),
                    _ => {
                        return self
                            .response(Some(("invalid_exam_input", "选择无效。".into())), vec![]);
                    }
                }
            } else {
                match exam.answer(index,&questions,&mut self.state.rng){Ok(correct)=>events.push(json!({"type":"examAnswer","correct":correct,"score":exam.score,"answered":exam.answered})),Err(error)=>return self.response(Some(("exam_answer",error)),vec![])}
                if exam.answered == 10 {
                    result = Some(if exam.score >= 60 { 2 } else { 1 });
                    events.push(
                        json!({"type":"examCompleted","score":exam.score,"passed":exam.score>=60}),
                    );
                }
            }
            self.state.revision += 1;
            if let Some(result) = result {
                self.state.host_activity = None;
                self.state.pending = None;
                if let Err(error) = self.complete_school_exam(result, &mut events) {
                    self.block("exam_transition", error, Value::Null);
                }
            } else {
                self.refresh_host_activity();
            }
            return self.response(None, events);
        }
        if let Some(HostActivity::Notice {
            pages,
            page,
            continuation,
        }) = self.state.host_activity.as_mut()
        {
            if !matches!(input, VmInput::Continue) {
                return self.response(
                    Some(("invalid_event_input", "请继续阅读当前事件。".into())),
                    vec![],
                );
            }
            *page += 1;
            self.state.revision += 1;
            if *page < pages.len() {
                self.refresh_host_activity();
                return self.response(None, vec![]);
            }
            let continuation = continuation.clone();
            self.state.host_activity = None;
            self.state.pending = None;
            self.state.scene = json!({});
            let mut events = Vec::new();
            match continuation {
                NoticeContinuation::AcceptMission { mission } => {
                    let setup = match self
                        .program
                        .as_ref()
                        .unwrap()
                        .condition_setup(i32::from(mission.words[6]))
                    {
                        Ok(setup) => setup,
                        Err(error) => {
                            self.block("mission_condition", error, Value::Null);
                            return self.response(None, events);
                        }
                    };
                    let player = self.state.legacy_player.as_mut().unwrap();
                    let active = ActiveCondition::from_setup(&setup, &|address| {
                        if address == 0x4b648a {
                            player.i32(address).ok()
                        } else {
                            player.i16(address).ok().map(i32::from)
                        }
                    });
                    match active {
                        Ok(active) => self.state.active_condition = Some(active),
                        Err(error) => {
                            self.block("mission_condition", error, Value::Null);
                            return self.response(None, events);
                        }
                    }
                    if let Some(target) = setup.mission {
                        player.set_i16(0x4b64ce, target.kind as i16).unwrap();
                        player.set_i16(0x4b64cc, target.target as i16).unwrap();
                        player.set_i32(0x4b64d0, 0).unwrap();
                    }
                    events.push(json!({"type":"missionAccepted","index":mission.index}));
                    self.state.active_mission = Some(mission);
                }
                NoticeContinuation::ResolveMission { success } => {
                    if let Err(error) = self.resolve_mission(success, &mut events) {
                        self.block("mission_result", error, Value::Null);
                        return self.response(None, events);
                    }
                    self.state.phase = "playing".into();
                    self.finish_turn(&mut events);
                    return self.response(None, events);
                }
                NoticeContinuation::Native => {}
            }
            if let Err(error) = self
                .state
                .vm
                .as_mut()
                .unwrap()
                .resume(VmInput::HostReturned { next_node: None })
            {
                self.block("native_resume", error, Value::Null);
            } else {
                self.advance_vm(&mut events);
            }
            return self.response(None, events);
        }
        let VmInput::Choose { index } = input else {
            return self.response(
                Some(("invalid_event_input", "请选择商品或离开。".into())),
                vec![],
            );
        };
        let mut events = Vec::new();
        let mut selected_career = None;
        let mut entering_adulthood = false;
        let mut property_preview = None;
        let finished = match self.state.host_activity.as_mut().unwrap() {
            HostActivity::NativeShop { shop } => {
                if index == shop.offers.len() {
                    true
                } else {
                    let bonus_address = shop
                        .offers
                        .get(index)
                        .filter(|offer| offer.category == 0)
                        .and_then(|offer| offer.bonus_attribute)
                        .filter(|attribute| (0..=3).contains(attribute))
                        .map(|attribute| {
                            [0x4b6490, 0x4b6492, 0x4b648e, 0x4b6494][attribute as usize]
                        });
                    let old_bonus = bonus_address.map(|address| {
                        (
                            address,
                            self.state
                                .legacy_player
                                .as_ref()
                                .unwrap()
                                .i16(address)
                                .unwrap(),
                        )
                    });
                    match shop.purchase(
                        index,
                        self.state.legacy_player.as_mut().unwrap(),
                        &mut self.state.housing,
                        &mut self.state.inventory.ordinary,
                        &mut self.state.inventory.special,
                        &self.state.known_equipment,
                        &mut self.state.carried_equipment,
                    ) {
                        Ok(purchase) => {
                            if purchase.category == 0 {
                                // 43ac82 opens the house before applying its furniture reward.
                                let bonus = old_bonus.map(|(address, old)| {
                                    let player = self.state.legacy_player.as_mut().unwrap();
                                    let value = player.i16(address).unwrap();
                                    player.set_i16(address, old).unwrap();
                                    (address, value)
                                });
                                property_preview = Some((
                                    StatusPage::House,
                                    StatusContinuation::NativeShop {
                                        shop: shop.clone(),
                                        bonus,
                                    },
                                ));
                            }
                            events.push(json!({"type":"nativeItemPurchased","purchase":purchase}));
                        }
                        Err(error) => {
                            return self.response(Some(("cannot_purchase", error)), vec![]);
                        }
                    }
                    false
                }
            }
            HostActivity::Housing {
                catalog,
                result,
                preview_investment,
            } => {
                if result.is_some() {
                    if index == 0 {
                        if *preview_investment {
                            property_preview =
                                Some((StatusPage::Investment, StatusContinuation::NativeReturn));
                        }
                        true
                    } else {
                        return self.response(
                            Some(("housing_input", "请继续阅读房产结算。".into())),
                            vec![],
                        );
                    }
                } else if self.state.housing.investment_stage == 0 {
                    if index == 4 {
                        true
                    } else {
                        let Some(style) = catalog.styles.iter().find(|style| style.id == index)
                        else {
                            return self
                                .response(Some(("housing_input", "住宅不存在。".into())), vec![]);
                        };
                        if let Err(error) = self
                            .state
                            .housing
                            .buy(style, self.state.legacy_player.as_mut().unwrap())
                        {
                            return self.response(Some(("housing_purchase", error)), vec![]);
                        }
                        *result = Some("住宅购买成功。".into());
                        events.push(
                            json!({"type":"housePurchased","style":index,"price":style.price}),
                        );
                        false
                    }
                } else {
                    match index {
                        5 => {
                            match self
                                .state
                                .housing
                                .invest(catalog, self.state.legacy_player.as_mut().unwrap())
                            {
                                Ok(price) => {
                                    *preview_investment = true;
                                    *result = Some(format!("本期房产投资完成，支付 {price} 元。"));
                                    events.push(json!({"type":"propertyInvestment","price":price,"stage":self.state.housing.investment_stage}));
                                }
                                Err(error) => {
                                    return self
                                        .response(Some(("housing_investment", error)), vec![]);
                                }
                            }
                            false
                        }
                        6 => {
                            match self
                                .state
                                .housing
                                .collect_income(catalog, self.state.legacy_player.as_mut().unwrap())
                            {
                                Ok(income) => {
                                    *result = Some(format!("已领取房产收益 {income} 元。"));
                                    events.push(json!({"type":"propertyIncome","amount":income}));
                                }
                                Err(error) => {
                                    return self.response(Some(("housing_income", error)), vec![]);
                                }
                            }
                            false
                        }
                        7 => true,
                        _ => {
                            return self.response(
                                Some(("housing_input", "房产操作无效。".into())),
                                vec![],
                            );
                        }
                    }
                }
            }
            HostActivity::StatShop { shop } => {
                if index == shop.offers.len() {
                    true
                } else {
                    match shop.purchase(index, self.state.legacy_player.as_mut().unwrap()) {
                        Ok(Some(gift)) => self.state.gift_level = gift,
                        Ok(None) => {}
                        Err(error) => {
                            return self.response(Some(("cannot_purchase", error)), vec![]);
                        }
                    }
                    events.push(json!({"type":"itemPurchased","offer":shop.offers[index]}));
                    false
                }
            }
            HostActivity::Career {
                careers,
                entering_adulthood: entering,
            } => {
                entering_adulthood = *entering;
                if index == 24 {
                    true
                } else {
                    let Some(career) = careers.iter().find(|career| career.id == index) else {
                        return self
                            .response(Some(("invalid_career", "职业不存在。".into())), vec![]);
                    };
                    if !career
                        .eligible(
                            self.state.legacy_player.as_ref().unwrap(),
                            &self.state.career_levels,
                        )
                        .unwrap_or(false)
                    {
                        return self.response(
                            Some(("career_requirements", "尚未满足这个职业的条件。".into())),
                            vec![],
                        );
                    }
                    selected_career = Some(index as i16);
                    true
                }
            }
            HostActivity::Notice { .. } => unreachable!(),
            HostActivity::SchoolExam { .. } => unreachable!(),
            HostActivity::Tracking { .. }
            | HostActivity::Hammer { .. }
            | HostActivity::Fist { .. }
            | HostActivity::Gas { .. }
            | HostActivity::Fire { .. }
            | HostActivity::Firebee { .. } => {
                return self.response(
                    Some(("minigame_input", "请使用小游戏中的操作。".into())),
                    vec![],
                );
            }
            HostActivity::Fortune { .. } => unreachable!(),
            HostActivity::EquipmentWorkshop { .. } => unreachable!(),
            HostActivity::Ending { .. } => unreachable!(),
            HostActivity::Relationship { .. } => {
                return self.response(
                    Some(("relationship_input", "请使用感情事件中的操作。".into())),
                    vec![],
                );
            }
            HostActivity::SpiritEncounter { .. } => unreachable!(),
            HostActivity::SpiritLottery { .. } => {
                return self.response(
                    Some(("spirit_input", "请使用转盘中的操作。".into())),
                    vec![],
                );
            }
            HostActivity::Competition { .. }
            | HostActivity::Battle { .. }
            | HostActivity::FinalBattle { .. } => unreachable!(),
            HostActivity::Inventory { .. } => {
                return self.response(
                    Some(("inventory_input", "请使用物品栏中的操作。".into())),
                    vec![],
                );
            }
            HostActivity::Status { .. } => {
                return self.response(
                    Some(("status_input", "请使用参数页面中的操作。".into())),
                    vec![],
                );
            }
        };
        if let Some(career) = selected_career {
            if let Err(error) = self.choose_career(career) {
                return self.response(Some(("career_change", error)), vec![]);
            }
            events.push(json!({"type":"careerChanged","career":career}));
        }
        if entering_adulthood && self.state.legacy_player.as_ref().unwrap().life().profession == -1
        {
            self.choose_career(4).unwrap();
        }
        self.sync_player();
        self.state.revision += 1;
        if let Some((page, continuation)) = property_preview {
            if let Err(error) = self.begin_property_preview(page, continuation) {
                self.block("property_preview", error, Value::Null);
            }
            return self.response(None, events);
        }
        if finished {
            self.state.host_activity = None;
            self.state.pending = None;
            self.state.scene = json!({});
            if entering_adulthood {
                if let Err(error) = self.refresh_spirit_positions(&mut events) {
                    self.block("spirit_positions", error, Value::Null);
                    return self.response(None, events);
                }
            }
            if let Some(vm) = self.state.vm.as_mut() {
                if let Err(error) = vm.resume(VmInput::HostReturned { next_node: None }) {
                    self.block("native_resume", error, Value::Null);
                } else {
                    self.advance_vm(&mut events);
                }
            } else {
                self.state.phase = "playing".into();
            }
        } else {
            self.refresh_host_activity();
        }
        self.response(None, events)
    }

    pub(super) fn run_actions(&mut self, events: &mut Vec<Value>) {
        while !self.state.action_queue.is_empty() && self.state.phase != "blocked" {
            let mut action = self.state.action_queue.remove(0);
            if !action.spirit_checked {
                let from_facing = self.state.map_facing;
                let map = self.current_map().unwrap();
                let mut from = self.state.tile_id.unwrap();
                let mut facing = from_facing;
                for &to in &action.path {
                    if let Some(direction) = map.facing_between(from, to) {
                        facing = Some(direction);
                    }
                    from = to;
                }
                self.state.map_facing = facing;
                events.push(json!({"type":"moved","fromTileId":self.state.tile_id,"tiles":action.path,"rolled":self.state.queued_die.take(),"fromFacing":from_facing,"facing":facing}));
                self.state.tile_id = Some(action.tile_id);
            }
            if action.landed && !action.spirit_checked {
                action.spirit_checked = true;
                match self.prepare_spirit_event(events) {
                    Ok(true) => {
                        action.path.clear();
                        self.state.action_queue.insert(0, action);
                        return;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.block("spirit_event", error, Value::Null);
                        return;
                    }
                }
            }
            events.push(json!({"type":"tileTriggered","tileId":action.tile_id,"eventId":action.event_id,"landed":action.landed}));
            match action.event_id {
                0x5dc4 | 0x5dcc | 0x5ddb | 0x5de7 => {
                    match StudentReward::calculate(
                        self.state.legacy_player.as_ref().unwrap(),
                        action.landed,
                    ) {
                        Ok(reward) => {
                            self.state.pending = Some(
                                json!({"kind":"allocation","text":"分配成长点数","points":reward.points,"money":reward.money,"defaults":reward.default_allocation()}),
                            );
                            self.state.pending_allocation = Some(reward);
                            self.state.phase = "event".into();
                        }
                        Err(error) => self.block("student_reward", error, Value::Null),
                    }
                    return;
                }
                0x5dda => {
                    if let Err(error) = self.collect_salary(action.landed, events) {
                        self.block("payday_handler", error, json!(action));
                        return;
                    }
                }
                0x5dcd | 0x5ddc => {
                    let player = self.state.legacy_player.as_mut().unwrap();
                    if player.i16(0x4b64cc).unwrap() != -1 && player.i16(0x4b64ce).unwrap() == 10 {
                        player.set_i16(0x4b64d0, 1).unwrap();
                    }
                    player.set_i16(0x4b64b0, 1).unwrap();
                }
                0x5dec | 0x5df6 | 0x5e01 | 0x5e0b | 0x5e15 => {}
                id => {
                    self.state.vm = Some(LegacyVm::new(id));
                    self.state.phase = "event".into();
                    self.advance_vm(events);
                    return;
                }
            }
        }
        if self.state.phase != "blocked" {
            self.finish_turn(events);
        }
    }

    pub(super) fn advance_vm(&mut self, events: &mut Vec<Value>) {
        let Some(program) = &self.program else {
            self.block("missing_events", "缺少原版事件数据。".into(), Value::Null);
            return;
        };
        let Some(vm) = &mut self.state.vm else {
            return;
        };
        let batch = match vm.advance(program, &mut || self.state.rng.next_rand()) {
            Ok(batch) => batch,
            Err(error) => {
                self.block("event_decode", error, Value::Null);
                return;
            }
        };
        for effect in batch.effects {
            events.push(serde_json::to_value(&effect).unwrap());
            match effect {
                VmEffect::Pattern {
                    resource_id,
                    scroll_mode,
                    ..
                } => {
                    self.state.scene["patternResource"] = json!(resource_id);
                    self.state.scene["patternScrollMode"] = json!(scroll_mode);
                    self.state.scene["patternSerial"] =
                        json!(self.state.scene["patternSerial"].as_u64().unwrap_or(0) + 1);
                    self.state.scene["backgroundResource"] = Value::Null;
                }
                VmEffect::Background {
                    resource_id, mode, ..
                } => {
                    self.state.scene["backgroundResource"] = json!(resource_id);
                    self.state.scene["backgroundMode"] = json!(mode);
                    self.state.scene["backgroundSerial"] =
                        json!(self.state.scene["backgroundSerial"].as_u64().unwrap_or(0) + 1);
                }
                VmEffect::Portraits {
                    left_id, right_id, ..
                } => {
                    self.update_event_portraits(left_id, right_id);
                }
                VmEffect::Dialogue {
                    dialogue_id,
                    content,
                    left_id,
                    right_id,
                } => {
                    self.update_event_portraits(left_id, right_id);
                    let speaker = content.speaker.map(|speaker| match speaker {
                        DialogueSpeaker::Player => self
                            .state
                            .player
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_default(),
                        DialogueSpeaker::Named { name } => name,
                    });
                    self.state.pending = Some(
                        json!({"kind":"dialogue","text":content.text,"speaker":speaker,"dialogueId":dialogue_id,"leftPortraitId":left_id,"rightPortraitId":right_id}),
                    );
                    self.state.message = "".into();
                }
                VmEffect::ClearPortraits { left, right, .. } => {
                    if left {
                        self.state.scene["leftPortraitResource"] = Value::Null;
                    }
                    if right {
                        self.state.scene["rightPortraitResource"] = Value::Null;
                    }
                }
                VmEffect::Choice { options, .. } => {
                    self.state.pending =
                        Some(json!({"kind":"choice","text":"请选择","options":options}));
                }
                VmEffect::ApplyResult { result } => {
                    let player = self.state.legacy_player.as_mut().unwrap();
                    for delta in result.changes {
                        let current = if delta.wrapping_bits == 32 {
                            player.i32(delta.address)
                        } else {
                            player.i16(delta.address).map(i32::from)
                        };
                        let changed = current.and_then(|current| {
                            if delta.wrapping_bits == 32 {
                                player.set_i32(delta.address, delta.apply(current))
                            } else {
                                player.set_i16(delta.address, delta.apply(current) as i16)
                            }
                        });
                        if let Err(error) = changed {
                            self.block("result_field", error, json!({"address":delta.address}));
                            return;
                        }
                    }
                    if let Some(career) = result.unlock_career {
                        if player.life().profession != career {
                            player.set_i16(0x4b64a2, career).unwrap();
                            player.set_i16(0x4b64a6, 5).unwrap();
                            player.set_i32(0x4b6538, 3).unwrap();
                            if self.state.career_history.len() < 98 {
                                self.state.career_history.push(
                                    json!({"year":player.life().year,"career":career,"rank":3}),
                                );
                            }
                            events.push(json!({"type":"careerChanged","career":career}));
                        }
                    }
                }
                VmEffect::ConfigureCondition { condition } => {
                    let player = self.state.legacy_player.as_mut().unwrap();
                    let active = ActiveCondition::from_setup(&condition, &|address| {
                        if address == 0x4b648a {
                            player.i32(address).ok()
                        } else {
                            player.i16(address).ok().map(i32::from)
                        }
                    });
                    if let Some(mission) = &condition.mission {
                        player.set_i16(0x4b64ce, mission.kind as i16).unwrap();
                        player.set_i16(0x4b64cc, mission.target as i16).unwrap();
                        player.set_i16(0x4b64d0, 0).unwrap();
                    }
                    match active {
                        Ok(active) => self.state.active_condition = Some(active),
                        Err(error) => {
                            self.block("condition_handler", error, json!(condition));
                            return;
                        }
                    }
                }
                VmEffect::SetValue { address, value } => {
                    if let Err(error) = self
                        .state
                        .legacy_player
                        .as_mut()
                        .unwrap()
                        .set_i16(address, value as i16)
                    {
                        self.block("player_field", error, json!({"address":address}));
                        return;
                    }
                }
                VmEffect::NativeCall { call } => {
                    if call.opcode == 15 {
                        if let Err(error) = self.begin_battle(events) {
                            self.block("battle_handler", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 17 {
                        if let Err(error) = self.begin_item_shop() {
                            self.block("item_shop", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 18 {
                        if let Err(error) = self.begin_housing() {
                            self.block("housing", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 27 {
                        if let Err(error) = self.begin_equipment_workshop() {
                            self.block("equipment_handler", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 25 {
                        if let Err(error) = self.begin_fortune() {
                            self.block("fortune_handler", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 24 {
                        if let Err(error) = self.begin_relationship(events) {
                            self.block("relationship_handler", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 22 {
                        let player = self.state.legacy_player.as_mut().unwrap();
                        if player.i16(0x4b64cc).unwrap() > 0 && player.i16(0x4b64ce).unwrap() == 10
                        {
                            player.set_i16(0x4b64d0, 1).unwrap();
                        }
                        player.set_i32(0x4b64b0, 1).unwrap();
                        if let Err(error) = self
                            .state
                            .vm
                            .as_mut()
                            .unwrap()
                            .resume(VmInput::HostReturned { next_node: None })
                        {
                            self.block("native_resume", error, json!(call));
                            return;
                        }
                        self.advance_vm(events);
                        return;
                    }
                    if call.opcode == 23 {
                        if let Err(error) = self.begin_minigame() {
                            self.block("minigame_handler", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 26 {
                        if let Err(error) = self.begin_mission() {
                            self.block("mission_selection", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 19 {
                        if let Err(error) = self.begin_career(false) {
                            self.block("career_selection", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 16 {
                        if let Err(error) = self.begin_stat_shop() {
                            self.block("stat_shop", error, json!(call));
                        }
                        return;
                    }
                    if call.opcode == 28 {
                        let next = match self.select_event(events) {
                            Ok(next) => next,
                            Err(error) => {
                                self.block("ordinary_event", error, json!(call));
                                return;
                            }
                        };
                        if let Err(error) =
                            self.state
                                .vm
                                .as_mut()
                                .unwrap()
                                .resume(VmInput::HostReturned {
                                    next_node: Some(next),
                                })
                        {
                            self.block("native_resume", error, json!(call));
                            return;
                        }
                        self.advance_vm(events);
                        return;
                    }
                    self.block(
                        "native_event_handler",
                        "此事件需要尚未还原的原版玩法模块，进度已保留。".into(),
                        json!(call),
                    );
                    return;
                }
                VmEffect::Finished => {}
            }
            // Preserve intermediate graphic states for the original blocking
            // transitions, even when several N1 instructions share one batch.
            if let Some(event) = events.last_mut() {
                if ["pattern", "background", "portraits", "clearPortraits"]
                    .contains(&event["type"].as_str().unwrap_or(""))
                {
                    event["scene"] = self.state.scene.clone();
                }
            }
        }
        self.sync_player();
        if batch.finished {
            self.state.vm = None;
            self.state.pending = None;
            self.state.scene = json!({});
            self.state.phase = "playing".into();
            self.run_actions(events);
        } else if matches!(
            batch.pending,
            Some(VmPending::Dialogue { .. } | VmPending::Choice { .. })
        ) {
            self.state.phase = "event".into();
        }
    }

    pub(super) fn finish_turn(&mut self, events: &mut Vec<Value>) {
        if !self.state.turn_in_progress {
            return;
        }
        if let Some(condition) = &self.state.active_condition {
            let player = self.state.legacy_player.as_ref().unwrap();
            match condition.evaluate(&|address| {
                if address == 0x4b648a {
                    player.i32(address).ok()
                } else {
                    player.i16(address).ok().map(i32::from)
                }
            }) {
                Ok(ConditionOutcome::Waiting) => {}
                Ok(outcome) => {
                    let success = outcome == ConditionOutcome::Succeeded;
                    let Some(mission) = self.state.active_mission.as_ref() else {
                        self.block(
                            "condition_resolution",
                            "此条件缺少原版任务索引，无法确定结算表。".into(),
                            json!({"outcome":outcome,"conditionId":condition.setup.condition_id}),
                        );
                        return;
                    };
                    let mode = if success { 1 } else { 2 };
                    let pages = (0..mission.words[3 + mode].max(1))
                        .map(|offset| i32::from(mission.words[mode]) + i32::from(offset))
                        .collect();
                    self.state.scene = json!({"backgroundResource":self.program.as_ref().unwrap().background_resource(i32::from(mission.words[9]))});
                    if let Err(error) =
                        self.show_notice(pages, NoticeContinuation::ResolveMission { success })
                    {
                        self.block("mission_dialogue", error, Value::Null);
                    }
                    return;
                }
                Err(error) => {
                    self.block("condition_state", error, Value::Null);
                    return;
                }
            }
        }
        let life = self.state.legacy_player.as_ref().unwrap().life();
        if !life.traveling && life.stage != 2 {
            let (pool, position) = match self.spirit_map_context() {
                Ok(context) => context,
                Err(error) => {
                    self.block("spirit_positions", error, Value::Null);
                    return;
                }
            };
            let total_turn = self
                .state
                .legacy_player
                .as_ref()
                .unwrap()
                .i16(0x4b64aa)
                .unwrap();
            match self
                .state
                .spirits
                .expire(total_turn, &pool, &position, &mut self.state.rng)
            {
                Ok(Some(spirit)) => {
                    self.state
                        .legacy_player
                        .as_mut()
                        .unwrap()
                        .set_i16(0x4b64d6, -1)
                        .unwrap();
                    events.push(json!({"type":"spiritDeparted","spirit":spirit}));
                }
                Ok(None) => {}
                Err(error) => {
                    self.block("spirit_departure", error, Value::Null);
                    return;
                }
            }
        }
        let player = self.state.legacy_player.as_mut().unwrap();
        let outcome = match player.settle_turn_numbers() {
            Ok(outcome) => outcome,
            Err(error) => {
                self.block("turn_numbers", error, Value::Null);
                return;
            }
        };
        let stage = player.life().stage;
        let traveling = player.life().traveling;
        self.state.turn_in_progress = false;
        self.sync_player();
        events.push(json!({"type":"turnCompleted","outcome":outcome}));
        if traveling {
            if self
                .state
                .legacy_player
                .as_ref()
                .unwrap()
                .i16(0x4b64b0)
                .unwrap()
                == 1
                || outcome.recovered_from_zero_hp
            {
                if let Err(error) = self.return_from_travel(events) {
                    self.block("travel_return", error, Value::Null);
                    return;
                }
            }
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .update_skills()
                .unwrap();
            self.sync_player();
            self.state.phase = "playing".into();
            return;
        }
        if outcome.aged && (2..=4).contains(&stage) {
            // FUN_0041ba90 only shows a sex/stage/year specific picture and sound.
            let sex = self
                .state
                .legacy_player
                .as_ref()
                .unwrap()
                .i16(0x4b6498)
                .unwrap();
            let prefix = match (stage, sex) {
                (2, 0) => "jm",
                (2, _) => "jm0",
                (3, 0) => "sm",
                (3, _) => "sm0",
                (4, 0) => "un",
                _ => "un0",
            };
            events.push(json!({"type":"birthday","resource":format!("install/bmp/{prefix}{:02}.bmp",outcome.stage_turn/5),"age":self.state.stats.age}));
        }
        let limit = self
            .rules
            .as_ref()
            .and_then(|rules| rules["stages"].as_array())
            .and_then(|stages| {
                stages
                    .iter()
                    .find(|item| item["id"].as_i64() == Some(i64::from(stage)))
            })
            .and_then(|item| item["stageTurns"].as_i64());
        if stage == 6 {
            if let Some(remaining) = limit.map(|limit| limit - i64::from(outcome.stage_turn)) {
                if remaining == 10 || remaining == 3 {
                    events.push(json!({"type":"lifeNotice","index":if remaining==10 {0}else{1}}));
                }
            }
        }
        if (2..=4).contains(&stage)
            && limit.is_some_and(|limit| i64::from(outcome.stage_turn) == limit / 2)
        {
            if let Err(error) = self.begin_competition(outcome) {
                self.block("student_competition", error, json!({"stage":stage}));
            }
            return;
        }
        self.finish_turn_transitions(outcome, events);
    }

    pub(super) fn finish_turn_transitions(
        &mut self,
        outcome: TurnOutcome,
        events: &mut Vec<Value>,
    ) {
        let stage = self.state.legacy_player.as_ref().unwrap().life().stage;
        let limit = self
            .rules
            .as_ref()
            .and_then(|rules| rules["stages"].as_array())
            .and_then(|stages| {
                stages
                    .iter()
                    .find(|item| item["id"].as_i64() == Some(i64::from(stage)))
            })
            .and_then(|item| item["stageTurns"].as_i64());
        if self
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(0x4b64b0)
            .unwrap()
            == 1
        {
            if let Err(error) = self.enter_travel(events) {
                self.block("travel_transition", error, Value::Null);
                return;
            }
            self.state
                .legacy_player
                .as_mut()
                .unwrap()
                .update_skills()
                .unwrap();
            self.sync_player();
            self.state.phase = "playing".into();
            return;
        }
        if let Some(limit) = limit {
            if i64::from(outcome.stage_turn) >= limit && limit > 0 {
                self.state
                    .legacy_player
                    .as_mut()
                    .unwrap()
                    .set_i16(0x4b64bc, 0)
                    .unwrap();
                self.state
                    .legacy_player
                    .as_mut()
                    .unwrap()
                    .set_i16(0x4b64be, 0)
                    .unwrap();
                match stage {
                    0 | 1 | 2 | 4 => {
                        if let Err(error) =
                            self.transition_stage(if stage == 4 { 6 } else { stage + 1 }, events)
                        {
                            self.block("stage_transition", error, json!({"stage":stage}));
                            return;
                        }
                        if self.state.host_activity.is_some() {
                            return;
                        }
                        if let Err(error) = self.refresh_spirit_positions(events) {
                            self.block("spirit_positions", error, Value::Null);
                            return;
                        }
                    }
                    3 => {
                        if let Err(error) = self.begin_school_exam() {
                            self.block("graduation_exam", error, Value::Null);
                        }
                        return;
                    }
                    _ => {
                        if let Err(error) = self
                            .prepare_final_history()
                            .and_then(|_| self.begin_final_battle())
                        {
                            self.block("final_battle", error, json!({"stage":stage}));
                        }
                        return;
                    }
                }
            }
        }
        if self
            .state
            .legacy_player
            .as_ref()
            .unwrap()
            .i16(0x4b64b4)
            .unwrap()
            == 1
        {
            let player = self.state.legacy_player.as_mut().unwrap();
            player.set_i16(0x4b64b4, 0).unwrap();
            player.set_i16(0x4b64bc, 0).unwrap();
            player.set_i16(0x4b64be, 0).unwrap();
            if let Err(error) = self.begin_school_exam() {
                self.block("graduation_exam", error, Value::Null);
            }
            return;
        }
        self.state
            .legacy_player
            .as_mut()
            .unwrap()
            .update_skills()
            .unwrap();
        self.sync_player();
        self.state.phase = "playing".into();
        self.state.message = "可以继续掷骰子。".into();
    }
}
