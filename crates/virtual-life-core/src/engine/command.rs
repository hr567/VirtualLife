//! Ordered game inputs.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameCommand {
    #[serde(rename_all = "camelCase")]
    ExploreMap {
        seq: u64,
        map_id: String,
    },
    #[serde(rename_all = "camelCase")]
    InspectTile {
        seq: u64,
        tile_id: usize,
    },
    #[serde(rename_all = "camelCase")]
    FollowLink {
        seq: u64,
        branch: usize,
    },
    #[serde(rename_all = "camelCase")]
    NewGame {
        seq: u64,
        profile: Profile,
        seed: u64,
    },
    RollDice {
        seq: u64,
        value: Option<u8>,
    },
    AdvanceEvent {
        seq: u64,
    },
    ChooseOption {
        seq: u64,
        index: usize,
    },
    AllocateStats {
        seq: u64,
        allocations: Allocation,
    },
    #[serde(rename_all = "camelCase")]
    MinigameTick {
        seq: u64,
        elapsed_ms: u32,
    },
    MinigameAction {
        seq: u64,
        action: String,
        index: Option<usize>,
        x: Option<i32>,
        y: Option<i32>,
        left: Option<bool>,
        right: Option<bool>,
        fire: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    RelationshipTick {
        seq: u64,
        elapsed_ms: u32,
        hover_index: Option<usize>,
    },
    RelationshipAction {
        seq: u64,
        action: String,
        index: Option<usize>,
    },
    #[serde(rename_all = "camelCase")]
    CompetitionTick {
        seq: u64,
        elapsed_ms: u32,
    },
    CompetitionAction {
        seq: u64,
        action: String,
        index: Option<usize>,
    },
    #[serde(rename_all = "camelCase")]
    EndingTick {
        seq: u64,
        elapsed_ms: u32,
    },
    EndingAction {
        seq: u64,
        action: String,
        index: Option<usize>,
    },
    #[serde(rename_all = "camelCase")]
    BattleTick {
        seq: u64,
        elapsed_ms: u32,
    },
    BattleAction {
        seq: u64,
        action: String,
        index: Option<usize>,
    },
    OpenInventory {
        seq: u64,
        tab: Option<String>,
    },
    InventoryAction {
        seq: u64,
        action: String,
        index: Option<usize>,
        tab: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    InventoryTick {
        seq: u64,
        elapsed_ms: u32,
    },
    OpenStatus {
        seq: u64,
    },
    StatusAction {
        seq: u64,
        action: String,
        page: Option<String>,
        delta: Option<i32>,
        index: Option<usize>,
    },
    #[serde(rename_all = "camelCase")]
    StatusTick {
        seq: u64,
        elapsed_ms: u32,
    },
}

impl GameCommand {
    pub fn seq(&self) -> u64 {
        match self {
            Self::ExploreMap { seq, .. }
            | Self::InspectTile { seq, .. }
            | Self::FollowLink { seq, .. }
            | Self::NewGame { seq, .. }
            | Self::RollDice { seq, .. }
            | Self::AdvanceEvent { seq }
            | Self::ChooseOption { seq, .. }
            | Self::AllocateStats { seq, .. }
            | Self::MinigameTick { seq, .. }
            | Self::MinigameAction { seq, .. }
            | Self::RelationshipTick { seq, .. }
            | Self::RelationshipAction { seq, .. }
            | Self::CompetitionTick { seq, .. }
            | Self::CompetitionAction { seq, .. }
            | Self::EndingTick { seq, .. }
            | Self::EndingAction { seq, .. }
            | Self::BattleTick { seq, .. }
            | Self::BattleAction { seq, .. }
            | Self::OpenInventory { seq, .. }
            | Self::InventoryAction { seq, .. }
            | Self::InventoryTick { seq, .. }
            | Self::OpenStatus { seq, .. }
            | Self::StatusAction { seq, .. }
            | Self::StatusTick { seq, .. } => *seq,
        }
    }
}

impl GameCommand {
    /// Set the external command sequence; UI callers normally use GameSession::send.
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        match &mut self {
            Self::ExploreMap { seq, .. }
            | Self::InspectTile { seq, .. }
            | Self::FollowLink { seq, .. }
            | Self::NewGame { seq, .. }
            | Self::RollDice { seq, .. }
            | Self::AdvanceEvent { seq, .. }
            | Self::ChooseOption { seq, .. }
            | Self::AllocateStats { seq, .. }
            | Self::MinigameTick { seq, .. }
            | Self::MinigameAction { seq, .. }
            | Self::RelationshipTick { seq, .. }
            | Self::RelationshipAction { seq, .. }
            | Self::CompetitionTick { seq, .. }
            | Self::CompetitionAction { seq, .. }
            | Self::EndingTick { seq, .. }
            | Self::EndingAction { seq, .. }
            | Self::BattleTick { seq, .. }
            | Self::BattleAction { seq, .. }
            | Self::OpenInventory { seq, .. }
            | Self::InventoryAction { seq, .. }
            | Self::InventoryTick { seq, .. }
            | Self::OpenStatus { seq, .. }
            | Self::StatusAction { seq, .. }
            | Self::StatusTick { seq, .. } => *seq = sequence,
        }
        self
    }
}
