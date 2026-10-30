//! Ordered state transitions and versioned saves. Gameplay is enabled only as
//! original initialization and transition rules are recovered and verified.
use crate::battle::{BattleGame, BattleInput, BattlePersistent, BattlePhase, BattleRules};
use crate::character::{LegacyPlayer, TurnOutcome};
use crate::competition::{CompetitionPhase, CompetitionRules, StudentCompetition};
use crate::ending_summary::{EndingSummary, EndingSummaryRules};
use crate::epilogue::{EndingFlow, EndingPhase, EpilogueRules};
use crate::equipment::{EquipmentDefinition, EquipmentOutcome, EquipmentWorkshop};
use crate::final_battle::{FinalBattleGame, FinalBattleInput, FinalBattleRules};
use crate::fire::{FireGame, FireInput, FirePhase};
use crate::firebee::{FirebeeGame, FirebeePhase};
use crate::fist::{FistGame, FistPhase};
use crate::fortune::{FortuneGame, FortunePhase, FortuneRecord};
use crate::gas::{GasGame, GasPhase};
use crate::hammer::{HammerGame, HammerPhase};
use crate::inventory::{self, InventoryRules};
use crate::inventory_view::{InventoryPanel, InventoryTab};
use crate::legacy_vm::{
    ActiveCondition, ConditionOutcome, DialogueSpeaker, LegacyProgram, LegacyVm, SkillDefinition,
    VmEffect, VmInput, VmPending, select_mission, select_ordinary_event, skill_effects,
    split_dialogue,
};
use crate::minigames::{TrackingGame, TrackingPhase};
use crate::original_rules::{
    Allocation, CareerRule, EquipmentKnowledge, EquipmentSlot, ExamQuestion, SchoolExam, StatShop,
    StatShopItem, StudentReward, collect_payday, payday_rank, plan_movement, roll_dice,
};
use crate::relationships::{
    RelationshipHouse, RelationshipPhase, RelationshipRules, RelationshipState,
};
use crate::rng::ReplayRng;
use crate::shops::{HousingCatalog, HousingState, ItemShopCatalog, NativeShop};
use crate::spirits::{SpiritLottery, SpiritLotteryPhase, Spirits};
use crate::status::{StatusPage, StatusPanel};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const INITIAL_BLOCKERS: &[&str] = &[
    "fortune_records_32_33_37_uninitialized_original_memory",
    "final_bonus_cards_37_and_above_overwrite_original_heap_pointers",
    "original_event_nonadvancing_opcode_zero",
];

mod activities;
mod command;
mod effects;
mod host;
mod native_view;
mod save;
mod session;
mod state;
#[cfg(feature = "test-support")]
mod test_support;
#[cfg(test)]
mod typed_tests;
pub use command::GameCommand;
pub use effects::{GameEffect, GameEffectKind, GameError, GameResponse, GameSnapshot};
#[cfg(test)]
use save::Save;
use save::{SAVE_VERSION, canonicalize_catalog_numbers, digest};
pub use session::FrameStep;
pub use state::{
    ActiveMission, Coordinate, Inventory, Map, MapDecorations, MapPosition, Profile, Stats, Tile,
};
use state::{Content, HostActivity, NoticeContinuation, State, StatusContinuation, TileAction};

pub struct GameSession {
    maps: Vec<Map>,
    content_hash: String,
    state: State,
    program: Option<LegacyProgram>,
    rules: Option<Value>,
    questionnaires: Option<Value>,
}

/// JSON boundary retained for evidence probes; browser runtime uses GameSession directly.
pub struct Runtime(GameSession);
impl std::ops::Deref for Runtime {
    type Target = GameSession;
    fn deref(&self) -> &GameSession {
        &self.0
    }
}
impl std::ops::DerefMut for Runtime {
    fn deref_mut(&mut self) -> &mut GameSession {
        &mut self.0
    }
}
impl Runtime {
    pub fn new(content_json: &str) -> Result<Self, String> {
        GameSession::from_json(content_json).map(Self)
    }
    pub fn dispatch(&mut self, command_json: &str) -> String {
        let response = match serde_json::from_str(command_json) {
            Ok(command) => self.0.dispatch(command),
            Err(error) => self
                .0
                .response(Some(("invalid_command", error.to_string())), vec![]),
        };
        serde_json::to_string(&response).unwrap()
    }
    #[cfg(test)]
    fn snapshot_value(&self) -> Value {
        serde_json::to_value(self.0.snapshot()).unwrap()
    }
    pub fn snapshot_json(&self) -> String {
        serde_json::to_string(&self.0.snapshot()).unwrap()
    }
    pub fn serialize(&self) -> String {
        self.0.save()
    }
    pub fn restore(&mut self, save_json: &str) -> String {
        serde_json::to_string(&self.0.restore(save_json)).unwrap()
    }
    #[cfg(test)]
    fn initial_life_history() -> [Vec<i32>; 16] {
        GameSession::initial_life_history()
    }
}

#[cfg(test)]
#[path = "engine/evidence_tests.rs"]
mod tests;
