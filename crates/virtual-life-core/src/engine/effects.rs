//! Typed snapshots and effects shared by Bevy presentation and headless validation.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub schema_version: u32,
    pub revision: u64,
    pub last_seq: u64,
    pub phase: String,
    pub map_id: Option<String>,
    pub tile_id: Option<usize>,
    pub turn: u32,
    pub player: Option<Profile>,
    pub stats: Stats,
    pub blockers: Vec<String>,
    pub message: String,
    pub pending: Option<Value>,
    /// Read-only native panel data. Commands retain their original choice or
    /// allocation semantics. Never part of saved State.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_view: Option<Value>,
    pub scene: Value,
    pub active_condition: Option<ActiveCondition>,
    pub shop_visits: u8,
    pub career_levels: Vec<i16>,
    pub career_history: Vec<Value>,
    pub active_mission: Option<ActiveMission>,
    pub inventory: Inventory,
    pub queued_die: Option<u8>,
    pub travel_origin: Option<MapPosition>,
    pub study_origin: Option<MapPosition>,
    pub spouse_index: i8,
    pub promotion_years: Vec<i16>,
    pub relationship_candidates: Vec<i8>,
    pub relationship_new_targets: [i8; 8],
    pub dating_target: i8,
    pub relationship_years: [Option<i16>; 3],
    pub gift_level: u8,
    pub fortune_visits: u8,
    pub known_equipment: Vec<EquipmentKnowledge>,
    pub carried_equipment: Vec<EquipmentSlot>,
    pub first_equipment_visit: bool,
    pub housing: HousingState,
    pub spirits: Spirits,
    pub last_roll_steps: u8,
    pub gas_target_state: Option<usize>,
    pub map_facing: Option<u8>,
    pub life_history: Option<[Vec<i32>; 16]>,
    pub map_decorations: Option<MapDecorations>,
    pub battle_persistent: Option<BattlePersistent>,
    pub life: Option<crate::character::Life>,
    pub status_overview: Option<Value>,
    pub status_details: Option<Value>,
    pub selected_tile: Option<Tile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GameEffectKind {
    #[serde(rename = "pattern")]
    Pattern,
    #[serde(rename = "background")]
    Background,
    #[serde(rename = "portraits")]
    Portraits,
    #[serde(rename = "dialogue")]
    Dialogue,
    #[serde(rename = "clearPortraits")]
    ClearPortraits,
    #[serde(rename = "choice")]
    Choice,
    #[serde(rename = "applyResult")]
    ApplyResult,
    #[serde(rename = "configureCondition")]
    ConfigureCondition,
    #[serde(rename = "setValue")]
    SetValue,
    #[serde(rename = "nativeCall")]
    NativeCall,
    #[serde(rename = "finished")]
    Finished,
    #[serde(rename = "mapSelected")]
    MapSelected,
    #[serde(rename = "tileSelected")]
    TileSelected,
    #[serde(rename = "pathTraced")]
    PathTraced,
    #[serde(rename = "newGameStarted")]
    NewGameStarted,
    #[serde(rename = "statsAllocated")]
    StatsAllocated,
    #[serde(rename = "skillApplied")]
    SkillApplied,
    #[serde(rename = "diceRolled")]
    DiceRolled,
    #[serde(rename = "stageChanged")]
    StageChanged,
    #[serde(rename = "travelStarted")]
    TravelStarted,
    #[serde(rename = "travelEnded")]
    TravelEnded,
    #[serde(rename = "spiritsRepositioned")]
    SpiritsRepositioned,
    #[serde(rename = "spiritAttached")]
    SpiritAttached,
    #[serde(rename = "spiritEffect")]
    SpiritEffect,
    #[serde(rename = "competitionPrize")]
    CompetitionPrize,
    #[serde(rename = "competitionCompleted")]
    CompetitionCompleted,
    #[serde(rename = "payday")]
    Payday,
    #[serde(rename = "relationshipCompleted")]
    RelationshipCompleted,
    #[serde(rename = "equipmentPurchase")]
    EquipmentPurchase,
    #[serde(rename = "equipmentResearch")]
    EquipmentResearch,
    #[serde(rename = "fortuneCompleted")]
    FortuneCompleted,
    #[serde(rename = "minigameCompleted")]
    MinigameCompleted,
    #[serde(rename = "inventoryChanged")]
    InventoryChanged,
    #[serde(rename = "battleCompleted")]
    BattleCompleted,
    #[serde(rename = "lifeCompleted")]
    LifeCompleted,
    #[serde(rename = "reincarnated")]
    Reincarnated,
    #[serde(rename = "gameCompleted")]
    GameCompleted,
    #[serde(rename = "missionResolved")]
    MissionResolved,
    #[serde(rename = "examAnswer")]
    ExamAnswer,
    #[serde(rename = "examCompleted")]
    ExamCompleted,
    #[serde(rename = "missionAccepted")]
    MissionAccepted,
    #[serde(rename = "nativeItemPurchased")]
    NativeItemPurchased,
    #[serde(rename = "housePurchased")]
    HousePurchased,
    #[serde(rename = "propertyInvestment")]
    PropertyInvestment,
    #[serde(rename = "propertyIncome")]
    PropertyIncome,
    #[serde(rename = "itemPurchased")]
    ItemPurchased,
    #[serde(rename = "careerChanged")]
    CareerChanged,
    #[serde(rename = "moved")]
    Moved,
    #[serde(rename = "tileTriggered")]
    TileTriggered,
    #[serde(rename = "spiritDeparted")]
    SpiritDeparted,
    #[serde(rename = "turnCompleted")]
    TurnCompleted,
    #[serde(rename = "birthday")]
    Birthday,
    #[serde(rename = "lifeNotice")]
    LifeNotice,
    #[serde(rename = "stateRestored")]
    StateRestored,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GameEffect {
    #[serde(rename = "type")]
    pub kind: GameEffectKind,
    #[serde(flatten)]
    pub data: serde_json::Map<String, Value>,
}
impl GameEffect {
    pub(super) fn from_value(value: Value) -> Self {
        serde_json::from_value(value).expect("Original rule emitted an unknown effect")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameResponse {
    pub ok: bool,
    pub snapshot: GameSnapshot,
    pub events: Vec<GameEffect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GameError>,
}
