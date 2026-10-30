//! Offline evidence construction, excluded from production builds.
use super::*;

impl GameSession {
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
