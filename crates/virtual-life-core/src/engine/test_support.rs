//! Offline evidence construction, excluded from production builds.
use super::*;

impl GameSession {
    /// Isolate unchanged original event instructions under explicit test
    /// preconditions. These are presentation evidence, not public life routes.
    pub fn presentation_evidence(&mut self, kind: &str) -> Result<String, String> {
        if self.state.legacy_player.is_none() {
            return Err("Start an evidence player first".into());
        }
        self.state.scene = json!({});
        self.state.pending = None;
        self.state.action_queue.clear();
        let entry = match kind {
            "n1-dynamic" => 22653,
            "n1-expression" => 11026,
            _ => 11019,
        };
        if !kind.starts_with("n1-") {
            self.state.turn_in_progress = true;
            let adult = kind.starts_with("payday-") || kind.starts_with("life-notice-");
            let turn = if kind == "life-notice-10" {
                89
            } else if kind == "life-notice-3" {
                96
            } else {
                4
            };
            let player = self.state.legacy_player.as_mut().unwrap();
            player.set_i16(0x4b64a8, turn)?;
            player.set_i16(0x4b64aa, turn)?;
            self.state.turn = turn as u32;
            if adult {
                player.set_i16(0x4b649e, 6)?;
                let map = self
                    .maps
                    .iter()
                    .find(|map| map.id == "social")
                    .ok_or("Missing adult map")?;
                self.state.map_id = Some(map.id.clone());
                self.state.tile_id = map.tile_index(&map.start);
                self.state.map_facing = map.initial_facing();
                self.load_map_decorations()?;
                self.refresh_spirit_positions(&mut vec![])?;
            }
            if kind.starts_with("payday-") {
                let rank = if kind == "payday-promotion" { 1 } else { 4 };
                let player = self.state.legacy_player.as_mut().unwrap();
                player.set_i16(0x4b64a2, 0)?;
                player.set_i16(0x4b64a6, rank)?;
                for addr in [0x4b6490, 0x4b6492, 0x4b648e, 0x4b6494] {
                    player.set_i16(addr, if kind == "payday-demotion" { 0 } else { 9999 })?;
                }
                self.state.inventory.special.fill(1);
                self.state.career_levels[0] = rank;
                self.state.action_queue.push(TileAction {
                    tile_id: self.state.tile_id.unwrap(),
                    event_id: 0x5dda,
                    landed: true,
                    path: vec![],
                    spirit_checked: true,
                });
            }
        }
        self.state.vm = Some(LegacyVm::new(entry));
        self.advance_vm(&mut vec![]);
        self.sync_player();
        let payload = self.save();
        let response = self.restore(&payload);
        if !response.ok {
            return Err(format!("Evidence validation: {:?}", response.error));
        }
        Ok(payload)
    }
    /// Encode previously validated domain-state evidence using this build's fresh save format.
    /// This accepts no legacy envelope and never bypasses the normal restore validator.
    #[doc(hidden)]
    pub fn save_test_state(&mut self, state: Value) -> Result<String, String> {
        let state: State = serde_json::from_value(state)
            .map_err(|error| format!("Invalid evidence state: {error}"))?;
        let checksum = digest(&serde_json::to_vec(&state).map_err(|error| error.to_string())?);
        let payload = serde_json::to_string(&save::Save {
            format: save::SAVE_FORMAT.into(),
            version: save::SAVE_VERSION,
            content_hash: self.content_hash.clone(),
            checksum,
            state,
        })
        .map_err(|error| error.to_string())?;
        let restored = self.restore(&payload);
        if !restored.ok {
            let error = restored.error.expect("Rejected evidence includes an error");
            return Err(format!("{}: {}", error.code, error.message));
        }
        Ok(self.save())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_encoding_still_uses_atomic_production_restore_validation() {
        let content = json!({"maps":[{"id":"park","start":{"row":0,"column":0},"chips":[{"row":0,"column":0,"eventId":1,"next":[]}]}]});
        let mut session = GameSession::from_content(content).unwrap();
        let saved = session.save();
        let envelope: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            session.save_test_state(envelope["state"].clone()).unwrap(),
            saved
        );
        let mut invalid = envelope["state"].clone();
        invalid["inventory"]["ordinary"] = json!([]);
        assert!(
            session
                .save_test_state(invalid)
                .unwrap_err()
                .starts_with("invalid_save:")
        );
        assert_eq!(session.save(), saved);
    }
}
