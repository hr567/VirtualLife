//! Slot I/O and pre-roll persistence. No asynchronous reply owns the live session.
use super::*;

pub(super) struct PendingWrite {
    operation: &'static str,
    payload: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn host() -> GameHost {
        static CONTENT: OnceLock<Value> = OnceLock::new();
        let content = CONTENT.get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let read = |name: &str| {
                serde_json::from_slice::<Value>(
                    &std::fs::read(root.join(format!("assets/catalog/{name}.json"))).unwrap(),
                )
                .unwrap()
            };
            let mut content = read("maps");
            for name in ["rules", "events", "questionnaires"] {
                content[name] = read(name);
            }
            content
        });
        let mut session = GameSession::from_content(content.clone()).unwrap();
        let response=session.send(serde_json::from_value(json!({"type":"newGame","seq":0,"seed":1,"profile":{"name":"存档回归","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}})).unwrap());
        assert!(response.ok);
        let mut host = GameHost {
            snapshot: serde_json::to_value(response.snapshot).unwrap(),
            session: Some(session),
            loading: false,
            ..Default::default()
        };
        host.ui.screen = Screen::Game;
        host
    }

    #[test]
    fn saves_pre_roll_state_once_and_resumes_once_after_the_reply() {
        let mut host = host();
        let mut requests = PlatformRequests::default();
        let before = host.session.as_ref().unwrap().save();
        let command = json!({"type":"rollDice"});
        host.begin_roll(&mut requests, command.clone());
        host.begin_roll(&mut requests, command);
        assert!(host.paused());
        assert_eq!(requests.0.len(), 1);
        let request = requests.0.pop_front().unwrap();
        assert_eq!(request.payload["slot"], "slot-0");
        assert_eq!(
            serde_json::from_str::<Value>(
                &decode_save(request.payload["payload"].as_str().unwrap())
                    .unwrap()
                    .0
            )
            .unwrap(),
            serde_json::from_str::<Value>(&before).unwrap()
        );
        assert_eq!(host.session.as_ref().unwrap().save(), before);
        host.receive_storage(request.id, "save", &Value::Null, None, &mut requests);
        host.hidden = true;
        host.resume_roll();
        assert_eq!(host.session.as_ref().unwrap().save(), before);
        host.hidden = false;
        host.resume_roll();
        let after = host.session.as_ref().unwrap().save();
        assert_ne!(after, before);
        host.receive_storage(request.id, "save", &Value::Null, None, &mut requests);
        host.resume_roll();
        assert_eq!(host.session.as_ref().unwrap().save(), after);
    }

    #[test]
    fn failed_save_still_rolls_but_navigation_discards_late_replies() {
        let mut host = host();
        let mut requests = PlatformRequests::default();
        host.begin_roll(&mut requests, json!({"type":"rollDice"}));
        let request = requests.0.pop_front().unwrap();
        host.receive_storage(
            request.id,
            "save",
            &Value::Null,
            Some("quota"),
            &mut requests,
        );
        host.resume_roll();
        assert!(host.notice.contains("自动存档失败"));
        assert_eq!(host.snapshot["lastSeq"], 2);
        let mut host = super::tests::host();
        host.begin_roll(&mut requests, json!({"type":"rollDice"}));
        let request = requests.0.pop_front().unwrap();
        let before = host.session.as_ref().unwrap().save();
        host.set_screen(Screen::Menu);
        host.receive_storage(request.id, "save", &Value::Null, None, &mut requests);
        host.resume_roll();
        assert_eq!(host.session.as_ref().unwrap().save(), before);
    }

    #[test]
    fn invalid_dice_requests_do_not_save_and_import_validation_does_not_restore() {
        let mut host = host();
        let mut requests = PlatformRequests::default();
        host.begin_roll(&mut requests, json!({"type":"rollDice","value":0}));
        assert!(requests.0.is_empty());
        let before = host.session.as_ref().unwrap().save();
        assert!(
            host.session
                .as_ref()
                .unwrap()
                .validate_save(&before)
                .is_ok()
        );
        assert!(host.session.as_ref().unwrap().validate_save("{}").is_err());
        assert_eq!(host.session.as_ref().unwrap().save(), before);
        host.ui.screen = Screen::Saves;
        host.ui.saves.ready = true;
        host.ui.saves.slots = json!([{}, {}]);
        host.storage_request = Some((77, 1));
        let record = json!({"payload":encode_save(host.session.as_ref().unwrap(),&host.ui)});
        host.receive_storage(77, "import", &record, None, &mut requests);
        assert_eq!(host.ui.screen, Screen::Saves);
        assert_eq!(host.session.as_ref().unwrap().save(), before);
        assert_eq!(requests.0.pop_front().unwrap().operation, "commitImport");
    }
}

impl GameHost {
    fn content_id(&self) -> Option<String> {
        self.session
            .as_ref()
            .map(|session| session.content_identity().to_owned())
    }

    fn slot_request(
        &mut self,
        requests: &mut PlatformRequests,
        operation: &str,
        payload: Value,
        slot: usize,
    ) {
        let id = self.request(requests, operation, payload);
        self.storage_request = Some((id, slot));
        self.ui.saves.busy = true;
    }

    pub(super) fn refresh_saves(&mut self, requests: &mut PlatformRequests) {
        self.slot_request(
            requests,
            "listSaves",
            json!({"contentId":self.content_id()}),
            self.ui.saves.selected,
        );
    }

    pub(super) fn open_saves(&mut self, requests: &mut PlatformRequests) {
        let can_save = if self.ui.screen == Screen::Saves {
            self.ui.saves.can_save
        } else {
            self.ui.screen == Screen::Game
                || matches!(self.ui.screen, Screen::Options | Screen::MapOverview)
                    && self.return_screen == Screen::Game
        };
        if self.ui.screen != Screen::Saves {
            self.ui.saves.parent_return = self.return_screen;
            self.return_screen = self.ui.screen;
            self.set_screen(Screen::Saves);
        }
        self.notice.clear();
        self.ui.saves.can_save = can_save && !self.snapshot["player"].is_null();
        self.ui.saves.ready = false;
        self.refresh_saves(requests);
    }

    fn confirm_or_commit(
        &mut self,
        requests: &mut PlatformRequests,
        operation: &'static str,
        payload: Value,
    ) {
        self.notice.clear();
        let slot = payload["slot"]
            .as_str()
            .and_then(|slot| slot.strip_prefix("slot-"))
            .and_then(|slot| slot.parse::<usize>().ok())
            .unwrap();
        self.ui.saves.selected = slot;
        self.pending_write = Some(PendingWrite { operation, payload });
        if self.ui.saves.slots[slot]["occupied"] == true {
            self.ui.saves.confirm = true;
        } else {
            self.commit_slot(requests);
        }
    }

    pub(super) fn save_selected(&mut self, requests: &mut PlatformRequests) {
        if self.ui.screen != Screen::Saves {
            self.open_saves(requests);
            return;
        }
        let slot = self.ui.saves.selected;
        if slot == 0
            || slot > 8
            || !self.ui.saves.ready
            || !self.ui.saves.can_save
            || self.ui.saves.busy
            || self.snapshot["player"].is_null()
        {
            return;
        }
        if let Some(session) = &self.session {
            let payload = json!({"slot":format!("slot-{slot}"),"payload":encode_save(session,&self.ui),"contentId":session.content_identity(),"expected":self.ui.saves.slots[slot]["token"]});
            self.confirm_or_commit(requests, "save", payload);
        }
    }

    pub(super) fn commit_slot(&mut self, requests: &mut PlatformRequests) {
        self.ui.saves.confirm = false;
        if let Some(write) = self.pending_write.take() {
            self.slot_request(
                requests,
                write.operation,
                write.payload,
                self.ui.saves.selected,
            );
        }
    }

    pub(super) fn load_selected(&mut self, requests: &mut PlatformRequests) {
        if self.ui.screen != Screen::Saves {
            self.open_saves(requests);
            return;
        }
        let slot = self.ui.saves.selected;
        if self.ui.saves.busy || self.ui.saves.slots[slot]["valid"] != true {
            return;
        }
        self.slot_request(
            requests,
            "load",
            json!({"slot":format!("slot-{slot}"),"contentId":self.content_id()}),
            slot,
        );
    }

    pub(super) fn transfer_slot(
        &mut self,
        requests: &mut PlatformRequests,
        slot: usize,
        operation: &str,
    ) {
        if slot > 8 || self.ui.saves.busy || self.ui.saves.confirm || !self.ui.saves.ready {
            return;
        }
        self.ui.saves.selected = slot;
        self.notice.clear();
        self.slot_request(
            requests,
            operation,
            json!({"slot":format!("slot-{slot}"),"contentId":self.content_id()}),
            slot,
        );
    }

    pub(super) fn begin_roll(&mut self, requests: &mut PlatformRequests, value: Value) {
        if self.pending_roll.is_some()
            || self.ready_roll.is_some()
            || self.ui.screen != Screen::Game
            || self.ui.runtime.motion.is_some()
        {
            return;
        }
        let Ok(GameCommand::RollDice { value: forced, .. }) =
            serde_json::from_value::<GameCommand>({
                let mut command = value.clone();
                command["seq"] = json!(0);
                command
            })
        else {
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        if let Err(error) = session.validate_roll(forced) {
            self.notice = error;
            return;
        }
        let payload = json!({"slot":"slot-0","payload":encode_save(session,&self.ui),"contentId":session.content_identity()});
        let id = self.request(requests, "save", payload);
        self.pending_roll = Some((id, value));
    }

    pub(super) fn resume_roll(&mut self) {
        if self.hidden || self.loading || self.error.is_some() || self.ui.screen != Screen::Game {
            return;
        }
        if let Some(value) = self.ready_roll.take() {
            self.ui.profile["_diceChoices"] = json!(false);
            self.send(value);
            self.discard_next_frame = true;
        }
    }

    pub(super) fn receive_storage(
        &mut self,
        id: u32,
        operation: &str,
        value: &Value,
        error: Option<&str>,
        requests: &mut PlatformRequests,
    ) -> bool {
        if ![
            "save",
            "load",
            "listSaves",
            "import",
            "commitImport",
            "export",
        ]
        .contains(&operation)
        {
            return false;
        }
        if operation == "save"
            && self
                .pending_roll
                .as_ref()
                .is_some_and(|pending| pending.0 == id)
        {
            let (_, command) = self.pending_roll.take().unwrap();
            self.ready_roll = Some(command);
            self.autosave_warning = error.is_some();
            if let Some(error) = error {
                self.notice = format!("自动存档失败，本回合继续：{error}");
            } else {
                self.notice.clear();
            }
            return true;
        }
        let Some((expected_id, slot)) = self.storage_request else {
            return true;
        };
        if expected_id != id || self.ui.screen != Screen::Saves {
            return true;
        }
        self.storage_request = None;
        self.ui.saves.busy = false;
        if let Some(error) = error {
            self.notice = error.into();
            if ["save", "commitImport"].contains(&operation) {
                self.refresh_saves(requests);
            }
            return true;
        }
        match operation {
            "listSaves" => {
                self.ui.saves.slots = value.clone();
                self.ui.saves.ready = true;
            }
            "import" => {
                let validation = value["payload"]
                    .as_str()
                    .ok_or("存档缺少内容。".to_owned())
                    .and_then(decode_save)
                    .and_then(|(domain, _)| {
                        self.session
                            .as_ref()
                            .ok_or("游戏尚未初始化。".to_owned())?
                            .validate_save(&domain)
                            .map_err(|error| error.message)
                    });
                if let Err(error) = validation {
                    self.notice = error;
                    return true;
                }
                let payload = json!({"slot":format!("slot-{slot}"),"record":value,"contentId":self.content_id(),"expected":self.ui.saves.slots[slot]["token"]});
                self.confirm_or_commit(requests, "commitImport", payload);
            }
            "save" | "commitImport" => {
                self.notice = if operation == "save" {
                    "进度已保存。"
                } else {
                    "存档已导入，请选择读取以继续游戏。"
                }
                .into();
                self.refresh_saves(requests);
            }
            "load" => {
                let restored = value["payload"]
                    .as_str()
                    .ok_or("该槽位没有存档。".to_owned())
                    .and_then(decode_save);
                match restored {
                    Err(error) => self.notice = error,
                    Ok((domain, presentation)) => {
                        if let Some(session) = self.session.as_mut() {
                            let response = session.restore(&domain);
                            let success = response.ok;
                            self.response(response);
                            if success {
                                let volume = self.ui.volume;
                                self.set_screen(Screen::Game);
                                self.ui = presentation.unwrap_or_default();
                                self.ui.screen = Screen::Game;
                                self.ui.volume = volume;
                                self.autosave_warning = false;
                                self.discard_next_frame = true;
                                if self.snapshot["phase"] == "completed" {
                                    self.set_screen(Screen::Menu);
                                }
                                self.notice = "已恢复进度。".into();
                            }
                        }
                    }
                }
            }
            "export" => self.notice = "存档已导出。".into(),
            _ => {}
        }
        true
    }
}
