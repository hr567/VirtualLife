//! Immutable native panel projections. Choice indices, eligibility and stock
//! come from the active original activity; reading a view never advances it.
use super::*;

impl GameSession {
    pub(super) fn native_view(&self) -> Option<Value> {
        if let Some(reward) = self.state.pending_allocation.as_ref() {
            let player = self.state.legacy_player.as_ref()?;
            let battles = i32::from(player.i16(0x4b64bc).ok()?);
            let wins = i32::from(player.i16(0x4b64be).ok()?);
            return Some(
                json!({"kind":"allocation","points":reward.points,"money":reward.money,
                "battles":battles,"wins":wins,"stageTurn":player.i16(0x4b64a8).ok()?,
                "winPercent":if battles==0{0}else{wins*100/battles}}),
            );
        }
        let options = self
            .state
            .pending
            .as_ref()
            .and_then(|pending| pending["options"].as_array());
        let option = |index: usize| {
            options.and_then(|options| {
                options
                    .iter()
                    .find(|option| option["index"].as_u64() == Some(index as u64))
            })
        };
        let available =
            |index: usize| option(index).is_some_and(|option| option["disabled"] != true);
        let mut view = match self.state.host_activity.as_ref()? {
            HostActivity::StatShop { shop } => {
                let offers = shop
                    .offers
                    .iter()
                    .enumerate()
                    .map(|(index, offer)| {
                        json!({"index":index,"name":offer.item.name,"resource":offer.item.resource,
                        "price":offer.item.price,"amount":offer.item.amount,"group":offer.group,
                        "bought":offer.bought,"disabled":!available(index)})
                    })
                    .collect::<Vec<_>>();
                json!({"kind":"statShop","offers":offers,"purchases":shop.purchases,"exitIndex":shop.offers.len()})
            }
            HostActivity::Career {
                careers,
                entering_adulthood,
            } => {
                let careers = careers
                    .iter()
                    .map(|career| {
                        let mut view =
                            serde_json::to_value(career).expect("Career rule is serializable");
                        view["index"] = json!(career.id);
                        view["eligible"] = json!(available(career.id));
                        view["disabled"] = json!(!available(career.id));
                        view["level"] = json!(self.state.career_levels.get(career.id));
                        view
                    })
                    .collect::<Vec<_>>();
                json!({"kind":"career","careers":careers,"enteringAdulthood":entering_adulthood,"exitIndex":24})
            }
            HostActivity::SchoolExam { exam } => {
                let choices = exam
                    .current
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, text)| json!({"index":index,"text":text}))
                    .collect::<Vec<_>>();
                json!({"kind":"schoolExam","started":exam.started,"answered":exam.answered,"score":exam.score,
                    "currentQuestion":{"id":exam.current.id,"question":exam.current.question,"prompt":exam.current.question,"options":choices}})
            }
            HostActivity::NativeShop { shop } => {
                let offers = shop
                    .offers
                    .iter()
                    .enumerate()
                    .map(|(index, offer)| {
                        let mut view =
                            serde_json::to_value(offer).expect("Native shop offer is serializable");
                        view["index"] = json!(index);
                        view["available"] = json!(available(index));
                        view
                    })
                    .collect::<Vec<_>>();
                json!({"kind":"itemShop","offers":offers,"exitIndex":shop.offers.len()})
            }
            HostActivity::EquipmentWorkshop {
                workshop,
                definitions,
            } => {
                let offered = definitions
                    .iter()
                    .find(|item| item.id == workshop.offered_id);
                let entries = definitions
                    .iter()
                    .filter_map(|item| {
                        let index = match workshop.mode.as_deref() {
                            Some("purchase") if item.id == workshop.offered_id => 0,
                            Some("research") => item.id,
                            _ => return None,
                        };
                        if workshop.outcome.is_some() || option(index).is_none() {
                            return None;
                        }
                        let known = self.state.known_equipment.get(item.id);
                        let carried = self
                            .state
                            .carried_equipment
                            .iter()
                            .find(|slot| slot.id == item.id as i16);
                        let mut view = serde_json::to_value(item)
                            .expect("Equipment definition is serializable");
                        view["index"] = json!(index);
                        view["resource"] = json!(item.icon);
                        view["researchPrice"] = json!(item.price / 2);
                        view["learned"] = json!(known.is_some_and(|known| known.learned));
                        view["level"] =
                            json!(carried.map(|slot| slot.level).unwrap_or_else(|| {
                                known.map_or(0, |known| i16::from(known.level))
                            }));
                        view["carried"] = json!(carried.is_some());
                        view["disabled"] = json!(!available(index));
                        Some(view)
                    })
                    .collect::<Vec<_>>();
                let exit_index = if workshop.outcome.is_some() {
                    None
                } else {
                    Some(match workshop.mode.as_deref() {
                        Some("purchase") => 1,
                        Some("research") => 18,
                        _ => 2,
                    })
                };
                json!({"kind":"equipmentWorkshop","mode":workshop.mode,"offeredId":workshop.offered_id,
                    "forcePurchase":workshop.force_purchase,"outcome":workshop.outcome,"offered":offered,
                    "definitions":definitions,"known":self.state.known_equipment,"carried":self.state.carried_equipment,
                    "entries":entries,"exitIndex":exit_index,"continueIndex":workshop.outcome.map(|_|0)})
            }
            HostActivity::Housing {
                catalog,
                result,
                preview_investment,
            } => {
                let special_greeting = self.state.legacy_player.as_ref().is_some_and(|player| {
                    player.i16(0x4b64cc).is_ok_and(|target| target >= 1)
                        && player.i16(0x4b64ce) == Ok(6)
                });
                json!({"kind":"housing","specialGreeting":special_greeting,"catalog":catalog,"housing":self.state.housing,"result":result,
                    "previewInvestment":preview_investment,
                    "nextInvestment":self.state.housing.next_investment(catalog).ok(),
                    "income":self.state.housing.income(catalog).ok(),
                    "exitIndex":if result.is_some(){None}else{Some(if self.state.housing.investment_stage==0{4}else{7})},
                    "continueIndex":result.as_ref().map(|_|0)})
            }
            HostActivity::Fortune { game } => {
                let special_greeting = self.state.legacy_player.as_ref().is_some_and(|player| {
                    player.i16(0x4b64cc).is_ok_and(|target| target >= 1)
                        && player.i16(0x4b64ce) == Ok(13)
                });
                let question = game.record.questions.get(game.question_index);
                // Keep future scoring tables and unasked questions out of the
                // display model. The active prompt/choices are already public.
                let current_question = question
                    .map(|question| json!({"prompt":question.prompt,"options":question.options}));
                json!({"kind":"fortune","specialGreeting":special_greeting,"recordId":game.record.id,"format":game.record.format,
                    "title":game.record.title,"phase":game.phase,"questionIndex":game.question_index,
                    "questionCount":game.record.questions.len(),"currentQuestion":current_question,
                    "answers":game.answers,"selectedAnswer":game.answers.get(game.question_index),
                    "score":game.score,"explanation":game.explanation,"percentages":game.percentages,
                    "displayOutcome":game.display_outcome,"statisticOutcome":game.statistic_outcome,
                    "exitIndex":if game.phase==FortunePhase::Question{question.map(|question|question.options.len())}else{Some(1)},
                    "continueIndex":if game.phase==FortunePhase::Explanation{Some(0)}else{None}})
            }
            // In particular, do not copy minigame/battle/ending state a second
            // time on every animated frame.
            _ => return None,
        };
        view["options"] = json!(options);
        Some(view)
    }
}
