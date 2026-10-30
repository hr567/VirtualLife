//! Original domain state; excludes all Bevy render and browser handles.
use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Coordinate {
    pub row: i32,
    pub column: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub row: i32,
    pub column: i32,
    pub event_id: i64,
    #[serde(default)]
    pub next: Vec<Coordinate>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Map {
    pub id: String,
    pub start: Coordinate,
    pub chips: Vec<Tile>,
    #[serde(default)]
    pub coordinate_index: Vec<Coordinate>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Map {
    pub(super) fn decoration_count(&self) -> usize {
        self.extra
            .get("legacySections")
            .and_then(|sections| sections["General"]["SpiritCount"]["signed"][0].as_u64())
            .unwrap_or(0) as usize
    }
    /// 0x417550 draws once for every Spirit_n, including repeating animations.
    pub(super) fn decoration_delays(&self, rng: &mut ReplayRng) -> Vec<u32> {
        (0..self.decoration_count())
            .map(|_| (rng.next_rand() % 6 + 5) * 1000)
            .collect()
    }
    /// 0x00417ae0 / direction table 0x004768cc.
    pub(super) fn facing_between(&self, from: usize, to: usize) -> Option<u8> {
        let from = self.chips.get(from)?;
        let to = self.chips.get(to)?;
        // The source table stores column delta first, then row delta.
        match (to.column - from.column, to.row - from.row) {
            (-1, 1) => Some(0),
            (-1, -1) => Some(1),
            (1, -1) => Some(2),
            (1, 1) => Some(3),
            _ => None,
        }
    }
    pub(super) fn initial_facing(&self) -> Option<u8> {
        let start = self.tile_index(&self.start)?;
        let next = self.chips.get(start)?.next.first()?;
        self.facing_between(start, self.tile_index(next)?)
    }
    pub fn tile_index(&self, coordinate: &Coordinate) -> Option<usize> {
        self.chips
            .iter()
            .position(|tile| tile.row == coordinate.row && tile.column == coordinate.column)
    }
    /// Follows explicit source-map links using caller-supplied branch selections.
    /// This does not invent the original random branching or dice distribution.
    pub fn trace_path(&self, start: usize, branches: &[usize]) -> Result<Vec<usize>, String> {
        let mut current = start;
        let mut path = vec![start];
        for branch in branches {
            let tile = self.chips.get(current).ok_or("Tile index out of range")?;
            let coordinate = tile.next.get(*branch).ok_or("Branch index out of range")?;
            current = self
                .tile_index(coordinate)
                .ok_or("Map link does not point to a tile")?;
            path.push(current);
        }
        Ok(path)
    }
}

#[derive(Deserialize)]
pub(super) struct Content {
    #[serde(default)]
    pub(super) maps: Vec<Map>,
    #[serde(default)]
    pub(super) events: Option<Value>,
    #[serde(default)]
    pub(super) rules: Option<Value>,
    #[serde(default)]
    pub(super) questionnaires: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub sex: String,
    pub birth_month: u8,
    pub birth_day: u8,
    pub blood_type: String,
}

impl Profile {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 64 {
            return Err("姓名应为 1–64 个字符。".into());
        }
        if !["male", "female"].contains(&self.sex.as_str()) {
            return Err("性别无效。".into());
        }
        if !["A", "B", "AB", "O"].contains(&self.blood_type.as_str()) {
            return Err("血型无效。".into());
        }
        // 0042225c changes month without clamping day, and 00422556's
        // confirmation copies the selection without calendar normalization.
        // A month change can therefore retain a displayed February 31.
        if !(1..=12).contains(&self.birth_month) || !(1..=31).contains(&self.birth_day) {
            return Err("生日日期无效。".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub money: Option<i64>,
    pub hp: Option<i64>,
    pub max_hp: Option<i64>,
    pub intelligence: Option<i64>,
    pub morality: Option<i64>,
    pub stamina: Option<i64>,
    pub charm: Option<i64>,
    pub experience: Option<i64>,
    pub age: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct State {
    pub(super) schema_version: u32,
    pub(super) revision: u64,
    pub(super) last_seq: u64,
    pub(super) phase: String,
    pub(super) map_id: Option<String>,
    pub(super) tile_id: Option<usize>,
    pub(super) turn: u32,
    pub(super) player: Option<Profile>,
    pub(super) stats: Stats,
    pub(super) blockers: Vec<String>,
    pub(super) message: String,
    pub(super) rng: ReplayRng,
    pub(super) legacy_player: Option<LegacyPlayer>,
    pub(super) entry_step_pending: bool,
    pub(super) vm: Option<LegacyVm>,
    pub(super) pending: Option<Value>,
    pub(super) scene: Value,
    pub(super) turn_in_progress: bool,
    pub(super) action_queue: Vec<TileAction>,
    pub(super) pending_allocation: Option<StudentReward>,
    pub(super) active_condition: Option<ActiveCondition>,
    pub(super) host_activity: Option<HostActivity>,
    pub(super) shop_visits: u8,
    pub(super) career_levels: Vec<i16>,
    pub(super) career_history: Vec<Value>,
    pub(super) active_mission: Option<ActiveMission>,
    pub(super) inventory: Inventory,
    pub(super) queued_die: Option<u8>,
    pub(super) travel_origin: Option<MapPosition>,
    pub(super) study_origin: Option<MapPosition>,
    pub(super) spouse_index: i8,
    pub(super) promotion_years: Vec<i16>,
    pub(super) relationship_candidates: Vec<i8>,
    pub(super) relationship_new_targets: [i8; 8],
    pub(super) dating_target: i8,
    pub(super) relationship_years: [Option<i16>; 3],
    pub(super) gift_level: u8,
    pub(super) fortune_visits: u8,
    pub(super) known_equipment: Vec<EquipmentKnowledge>,
    pub(super) carried_equipment: Vec<EquipmentSlot>,
    pub(super) first_equipment_visit: bool,
    pub(super) housing: HousingState,
    pub(super) spirits: Spirits,
    pub(super) last_roll_steps: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) gas_target_state: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) map_facing: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) life_history: Option<[Vec<i32>; 16]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) map_decorations: Option<MapDecorations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) battle_persistent: Option<BattlePersistent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MapDecorations {
    pub map_id: String,
    pub load_serial: u32,
    pub delays_ms: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MapPosition {
    pub map_id: String,
    pub tile_id: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Inventory {
    pub ordinary: Vec<i16>,
    pub special: Vec<i16>,
}
impl Default for Inventory {
    fn default() -> Self {
        let mut ordinary = vec![0; 13];
        ordinary[4] = 1;
        ordinary[5] = 1;
        Self {
            ordinary,
            special: vec![0; 27],
        }
    }
}
impl Inventory {
    pub(super) fn read(&self, address: u32) -> Option<i32> {
        if (0x4b9ef8..0x4b9f12).contains(&address) && address % 2 == 0 {
            return self
                .ordinary
                .get((address - 0x4b9ef8) as usize / 2)
                .copied()
                .map(i32::from);
        }
        if (0x4b9f14..0x4b9f4a).contains(&address) && address % 2 == 0 {
            return self
                .special
                .get((address - 0x4b9f14) as usize / 2)
                .copied()
                .map(i32::from);
        }
        None
    }
    pub(super) fn write(&mut self, address: u32, value: i16) -> bool {
        if (0x4b9ef8..0x4b9f12).contains(&address) && address % 2 == 0 {
            self.ordinary[(address - 0x4b9ef8) as usize / 2] = value;
            return true;
        }
        if (0x4b9f14..0x4b9f4a).contains(&address) && address % 2 == 0 {
            self.special[(address - 0x4b9f14) as usize / 2] = value;
            return true;
        }
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActiveMission {
    pub index: i16,
    pub words: Vec<i16>,
    pub multiplier: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum NoticeContinuation {
    Native,
    AcceptMission { mission: ActiveMission },
    ResolveMission { success: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum StatusContinuation {
    NativeShop {
        shop: NativeShop,
        bonus: Option<(u32, i16)>,
    },
    NativeReturn,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum HostActivity {
    StatShop {
        shop: StatShop,
    },
    Career {
        careers: Vec<CareerRule>,
        entering_adulthood: bool,
    },
    Notice {
        pages: Vec<i32>,
        page: usize,
        continuation: NoticeContinuation,
    },
    SchoolExam {
        exam: SchoolExam,
    },
    Tracking {
        game: TrackingGame,
    },
    Hammer {
        game: HammerGame,
    },
    Fist {
        game: FistGame,
    },
    Gas {
        game: GasGame,
    },
    Fire {
        game: FireGame,
    },
    Firebee {
        game: FirebeeGame,
    },
    Fortune {
        game: FortuneGame,
    },
    EquipmentWorkshop {
        workshop: EquipmentWorkshop,
        definitions: Vec<EquipmentDefinition>,
    },
    NativeShop {
        shop: NativeShop,
    },
    Housing {
        catalog: HousingCatalog,
        result: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        preview_investment: bool,
    },
    Ending {
        flow: EndingFlow,
    },
    Relationship {
        house: RelationshipHouse,
    },
    SpiritEncounter {
        spirit: usize,
    },
    SpiritLottery {
        game: SpiritLottery,
    },
    Competition {
        game: StudentCompetition,
        turn_outcome: TurnOutcome,
    },
    Battle {
        game: BattleGame,
    },
    FinalBattle {
        game: FinalBattleGame,
    },
    Inventory {
        panel: InventoryPanel,
    },
    Status {
        panel: StatusPanel,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<StatusContinuation>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct TileAction {
    pub tile_id: usize,
    pub event_id: i32,
    pub landed: bool,
    pub path: Vec<usize>,
    pub spirit_checked: bool,
}
