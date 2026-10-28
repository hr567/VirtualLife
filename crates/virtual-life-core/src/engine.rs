//! Ordered state transitions and versioned saves. Gameplay is enabled only as
//! original initialization and transition rules are recovered and verified.
use crate::rng::ReplayRng;
use crate::character::{LegacyPlayer,TurnOutcome};
use crate::competition::{StudentCompetition,CompetitionRules,CompetitionPhase};
use crate::ending_summary::{EndingSummary,EndingSummaryRules};
use crate::battle::{BattleGame,BattleInput,BattlePersistent,BattlePhase,BattleRules};
use crate::final_battle::{FinalBattleGame,FinalBattleInput,FinalBattleRules};
use crate::inventory::{self,InventoryRules};
use crate::inventory_view::{InventoryPanel,InventoryTab};
use crate::status::{StatusPanel,StatusPage};
use crate::epilogue::{EndingFlow,EndingPhase,EpilogueRules};
use crate::legacy_vm::{LegacyProgram,LegacyVm,VmEffect,VmPending,VmInput,DialogueSpeaker,ActiveCondition,ConditionOutcome,select_ordinary_event,SkillDefinition,skill_effects,select_mission,split_dialogue};
use crate::original_rules::{roll_dice,plan_movement,StudentReward,Allocation,StatShop,StatShopItem,CareerRule,SchoolExam,ExamQuestion,payday_rank,collect_payday,EquipmentKnowledge,EquipmentSlot};
use crate::minigames::{TrackingGame,TrackingPhase};
use crate::hammer::{HammerGame,HammerPhase};
use crate::fist::{FistGame,FistPhase};
use crate::gas::{GasGame,GasPhase};
use crate::fire::{FireGame,FirePhase,FireInput};
use crate::firebee::{FirebeeGame,FirebeePhase};
use crate::fortune::{FortuneGame,FortuneRecord,FortunePhase};
use crate::equipment::{EquipmentDefinition,EquipmentWorkshop,EquipmentOutcome};
use crate::shops::{NativeShop,ItemShopCatalog,HousingState,HousingCatalog};
use crate::relationships::{RelationshipHouse,RelationshipPhase,RelationshipRules,RelationshipState};
use crate::spirits::{Spirits,SpiritLottery,SpiritLotteryPhase};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SAVE_VERSION: u32 = 1;
const INITIAL_BLOCKERS: &[&str] = &[
    "fortune_records_32_33_37_uninitialized_original_memory",
    "final_bonus_cards_37_and_above_overwrite_original_heap_pointers",
    "original_event_nonadvancing_opcode_zero",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Coordinate { pub row: i32, pub column: i32 }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub row: i32,
    pub column: i32,
    pub event_id: i64,
    #[serde(default)] pub next: Vec<Coordinate>,
    #[serde(flatten)] pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Map {
    pub id: String,
    pub start: Coordinate,
    pub chips: Vec<Tile>,
    #[serde(default)] pub coordinate_index:Vec<Coordinate>,
    #[serde(flatten)] pub extra: BTreeMap<String, Value>,
}

impl Map {
    fn decoration_count(&self)->usize{self.extra.get("legacySections").and_then(|sections|sections["General"]["SpiritCount"]["signed"][0].as_u64()).unwrap_or(0)as usize}
    /// 0x417550 draws once for every Spirit_n, including repeating animations.
    fn decoration_delays(&self,rng:&mut ReplayRng)->Vec<u32>{(0..self.decoration_count()).map(|_|(rng.next_rand()%6+5)*1000).collect()}
    /// 0x00417ae0 / direction table 0x004768cc.
    fn facing_between(&self,from:usize,to:usize)->Option<u8>{
        let from=self.chips.get(from)?;let to=self.chips.get(to)?;
        // The source table stores column delta first, then row delta.
        match(to.column-from.column,to.row-from.row){(-1,1)=>Some(0),(-1,-1)=>Some(1),(1,-1)=>Some(2),(1,1)=>Some(3),_=>None}
    }
    fn initial_facing(&self)->Option<u8>{let start=self.tile_index(&self.start)?;let next=self.chips.get(start)?.next.first()?;self.facing_between(start,self.tile_index(next)?)}
    pub fn tile_index(&self, coordinate: &Coordinate) -> Option<usize> {
        self.chips.iter().position(|tile| tile.row == coordinate.row && tile.column == coordinate.column)
    }
    /// Follows explicit source-map links using caller-supplied branch selections.
    /// This does not invent the original random branching or dice distribution.
    pub fn trace_path(&self, start: usize, branches: &[usize]) -> Result<Vec<usize>, String> {
        let mut current = start;
        let mut path = vec![start];
        for branch in branches {
            let tile = self.chips.get(current).ok_or("Tile index out of range")?;
            let coordinate = tile.next.get(*branch).ok_or("Branch index out of range")?;
            current = self.tile_index(coordinate).ok_or("Map link does not point to a tile")?;
            path.push(current);
        }
        Ok(path)
    }
}

#[derive(Deserialize)]
struct Content {
    #[serde(default)] maps: Vec<Map>,
    #[serde(default)] events: Option<Value>,
    #[serde(default)] rules: Option<Value>,
    #[serde(default)] questionnaires: Option<Value>,
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
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.chars().count() > 64 { return Err("姓名应为 1–64 个字符。".into()); }
        if !["male","female"].contains(&self.sex.as_str()) { return Err("性别无效。".into()); }
        if !["A","B","AB","O"].contains(&self.blood_type.as_str()) { return Err("血型无效。".into()); }
        let days = match self.birth_month { 1|3|5|7|8|10|12 => 31, 4|6|9|11 => 30, 2 => 29, _ => 0 };
        if self.birth_day == 0 || self.birth_day > days { return Err("生日日期无效。".into()); }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub money: Option<i64>, pub hp: Option<i64>, pub max_hp: Option<i64>,
    pub intelligence: Option<i64>, pub morality: Option<i64>, pub stamina: Option<i64>,
    pub charm: Option<i64>, pub experience: Option<i64>, pub age: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct State {
    schema_version: u32,
    revision: u64,
    last_seq: u64,
    phase: String,
    map_id: Option<String>,
    tile_id: Option<usize>,
    turn: u32,
    player: Option<Profile>,
    stats: Stats,
    blockers: Vec<String>,
    message: String,
    rng: ReplayRng,
    legacy_player: Option<LegacyPlayer>,
    entry_step_pending: bool,
    vm: Option<LegacyVm>,
    pending: Option<Value>,
    scene: Value,
    turn_in_progress: bool,
    action_queue: Vec<TileAction>,
    pending_allocation:Option<StudentReward>,
    active_condition:Option<ActiveCondition>,
    host_activity:Option<HostActivity>,
    shop_visits:u8,
    career_levels:Vec<i16>,
    career_history:Vec<Value>,
    active_mission:Option<ActiveMission>,
    inventory:Inventory,
    queued_die:Option<u8>,
    travel_origin:Option<MapPosition>,
    study_origin:Option<MapPosition>,
    spouse_index:i8,
    promotion_years:Vec<i16>,
    relationship_candidates:Vec<i8>,
    relationship_new_targets:[i8;8],
    dating_target:i8,
    relationship_years:[Option<i16>;3],
    gift_level:u8,
    fortune_visits:u8,
    known_equipment:Vec<EquipmentKnowledge>,
    carried_equipment:Vec<EquipmentSlot>,
    first_equipment_visit:bool,
    housing:HousingState,
    spirits:Spirits,
    last_roll_steps:u8,
    #[serde(default,skip_serializing_if="Option::is_none")]
    gas_target_state:Option<usize>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    map_facing:Option<u8>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    life_history:Option<[Vec<i32>;16]>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    map_decorations:Option<MapDecorations>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    battle_persistent:Option<BattlePersistent>,
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
struct MapDecorations{map_id:String,load_serial:u32,delays_ms:Vec<u32>}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
struct MapPosition{map_id:String,tile_id:usize,#[serde(default,skip_serializing_if="Option::is_none")]facing:Option<u8>}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
struct Inventory{ordinary:Vec<i16>,special:Vec<i16>}
impl Default for Inventory{fn default()->Self{let mut ordinary=vec![0;13];ordinary[4]=1;ordinary[5]=1;Self{ordinary,special:vec![0;27]}}}
impl Inventory{
    fn read(&self,address:u32)->Option<i32>{
        if (0x4b9ef8..0x4b9f12).contains(&address) && address%2==0{return self.ordinary.get((address-0x4b9ef8)as usize/2).copied().map(i32::from);}
        if (0x4b9f14..0x4b9f4a).contains(&address) && address%2==0{return self.special.get((address-0x4b9f14)as usize/2).copied().map(i32::from);}
        None
    }
    fn write(&mut self,address:u32,value:i16)->bool{
        if (0x4b9ef8..0x4b9f12).contains(&address) && address%2==0{self.ordinary[(address-0x4b9ef8)as usize/2]=value;return true;}
        if (0x4b9f14..0x4b9f4a).contains(&address) && address%2==0{self.special[(address-0x4b9f14)as usize/2]=value;return true;}
        false
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
struct ActiveMission{index:i16,words:Vec<i16>,multiplier:i32}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(tag="kind",rename_all="camelCase")]
enum NoticeContinuation{Native,AcceptMission{mission:ActiveMission},ResolveMission{success:bool}}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(tag="kind",rename_all="camelCase")]
enum StatusContinuation{NativeShop{shop:NativeShop,bonus:Option<(u32,i16)>},NativeReturn}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(tag="kind",rename_all="camelCase")]
enum HostActivity {StatShop{shop:StatShop},Career{careers:Vec<CareerRule>,entering_adulthood:bool},Notice{pages:Vec<i32>,page:usize,continuation:NoticeContinuation},SchoolExam{exam:SchoolExam},Tracking{game:TrackingGame},Hammer{game:HammerGame},Fist{game:FistGame},Gas{game:GasGame},Fire{game:FireGame},Firebee{game:FirebeeGame},Fortune{game:FortuneGame},EquipmentWorkshop{workshop:EquipmentWorkshop,definitions:Vec<EquipmentDefinition>},NativeShop{shop:NativeShop},Housing{catalog:HousingCatalog,result:Option<String>,#[serde(default,skip_serializing_if="std::ops::Not::not")]preview_investment:bool},Ending{flow:EndingFlow},Relationship{house:RelationshipHouse},SpiritEncounter{spirit:usize},SpiritLottery{game:SpiritLottery},Competition{game:StudentCompetition,turn_outcome:TurnOutcome},Battle{game:BattleGame},FinalBattle{game:FinalBattleGame},Inventory{panel:InventoryPanel},Status{panel:StatusPanel,#[serde(default,skip_serializing_if="Option::is_none")]continuation:Option<StatusContinuation>}}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
struct TileAction { tile_id:usize,event_id:i32,landed:bool,path:Vec<usize>,spirit_checked:bool }

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Command {
    #[serde(rename_all = "camelCase")] ExploreMap { seq: u64, map_id: String },
    #[serde(rename_all = "camelCase")] InspectTile { seq: u64, tile_id: usize },
    #[serde(rename_all = "camelCase")] FollowLink { seq: u64, branch: usize },
    #[serde(rename_all = "camelCase")] NewGame { seq: u64, profile: Profile, seed: u64 },
    RollDice { seq: u64,value:Option<u8> },
    AdvanceEvent { seq:u64 },
    ChooseOption { seq:u64,index:usize },
    AllocateStats { seq:u64,allocations:Allocation },
    #[serde(rename_all="camelCase")] MinigameTick{seq:u64,elapsed_ms:u32},
    MinigameAction{seq:u64,action:String,index:Option<usize>,x:Option<i32>,y:Option<i32>,left:Option<bool>,right:Option<bool>,fire:Option<bool>},
    #[serde(rename_all="camelCase")] RelationshipTick{seq:u64,elapsed_ms:u32,hover_index:Option<usize>},
    RelationshipAction{seq:u64,action:String,index:Option<usize>},
    #[serde(rename_all="camelCase")] CompetitionTick{seq:u64,elapsed_ms:u32},
    CompetitionAction{seq:u64,action:String,index:Option<usize>},
    #[serde(rename_all="camelCase")] EndingTick{seq:u64,elapsed_ms:u32},
    EndingAction{seq:u64,action:String,index:Option<usize>},
    #[serde(rename_all="camelCase")] BattleTick{seq:u64,elapsed_ms:u32},
    BattleAction{seq:u64,action:String,index:Option<usize>},
    OpenInventory{seq:u64,tab:Option<String>},
    InventoryAction{seq:u64,action:String,index:Option<usize>,tab:Option<String>},
    #[serde(rename_all="camelCase")] InventoryTick{seq:u64,elapsed_ms:u32},
    OpenStatus{seq:u64},
    StatusAction{seq:u64,action:String,page:Option<String>,delta:Option<i32>,index:Option<usize>},
    #[serde(rename_all="camelCase")] StatusTick{seq:u64,elapsed_ms:u32},
}

impl Command {
    fn seq(&self) -> u64 { match self { Self::ExploreMap{seq,..}|Self::InspectTile{seq,..}|Self::FollowLink{seq,..}|Self::NewGame{seq,..}|Self::RollDice{seq,..}|Self::AdvanceEvent{seq}|Self::ChooseOption{seq,..}|Self::AllocateStats{seq,..}|Self::MinigameTick{seq,..}|Self::MinigameAction{seq,..}|Self::RelationshipTick{seq,..}|Self::RelationshipAction{seq,..}|Self::CompetitionTick{seq,..}|Self::CompetitionAction{seq,..}|Self::EndingTick{seq,..}|Self::EndingAction{seq,..}|Self::BattleTick{seq,..}|Self::BattleAction{seq,..}|Self::OpenInventory{seq,..}|Self::InventoryAction{seq,..}|Self::InventoryTick{seq,..}|Self::OpenStatus{seq,..}|Self::StatusAction{seq,..}|Self::StatusTick{seq,..} => *seq } }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Save { format_version: u32, content_hash: String, checksum: String, state: State }

pub struct Runtime { maps: Vec<Map>, content_hash: String, state: State,program:Option<LegacyProgram>,rules:Option<Value>,questionnaires:Option<Value> }

impl Runtime {
    pub fn new(content_json: &str) -> Result<Self, String> {
        // JavaScript also serializes 1.0 as 1 and -0 as 0. Catalog identity
        // follows semantic numbers, independent of native/browser JSON spelling.
        let mut canonical:Value=serde_json::from_str(content_json).map_err(|e|format!("Invalid content catalog: {e}"))?;
        canonicalize_catalog_numbers(&mut canonical);
        let content_hash=digest(canonical.to_string().as_bytes());
        let content: Content = serde_json::from_value(canonical).map_err(|e| format!("Invalid content catalog: {e}"))?;
        let mut ids = BTreeSet::new();
        for map in &content.maps {
            if map.id.is_empty() || !ids.insert(&map.id) { return Err("Duplicate or empty map identifier".into()); }
            let mut coordinates = BTreeSet::new();
            for tile in &map.chips {
                if !coordinates.insert((tile.row, tile.column)) { return Err(format!("Duplicate tile in map {}", map.id)); }
            }
            if map.tile_index(&map.start).is_none() { return Err(format!("Map {} start does not identify a tile", map.id)); }
        }
        let program=content.events.as_ref().map(|value|LegacyProgram::from_json(&value.to_string())).transpose()?;
        Ok(Self { maps: content.maps, program,rules:content.rules,questionnaires:content.questionnaires,content_hash, state: State {
            schema_version: SAVE_VERSION, revision: 0, last_seq: 0, phase: "ready".into(), map_id: None, tile_id: None,
            turn: 0, player: None, stats: Stats::default(), blockers: INITIAL_BLOCKERS.iter().map(|s| (*s).into()).collect(),
            message: "原版资源已载入；完整游戏规则仍在核验。".into(), rng: ReplayRng::new(0),
            legacy_player:None,entry_step_pending:true,vm:None,pending:None,scene:json!({}),turn_in_progress:false,action_queue:vec![],pending_allocation:None,active_condition:None,host_activity:None,shop_visits:0,career_levels:vec![1;25],career_history:vec![],active_mission:None,inventory:Inventory::default(),queued_die:None,travel_origin:None,study_origin:None,spouse_index:-1,promotion_years:vec![],relationship_candidates:vec![-1;8],relationship_new_targets:[-1;8],dating_target:-1,relationship_years:[None;3],gift_level:1,fortune_visits:0,known_equipment:vec![EquipmentKnowledge{learned:false,level:0};18],carried_equipment:vec![EquipmentSlot{id:-1,level:0};8],first_equipment_visit:true,housing:HousingState::default(),spirits:Spirits::default(),last_roll_steps:0,gas_target_state:None,map_facing:None,life_history:None,map_decorations:None,battle_persistent:None,
        } })
    }

    fn current_map(&self) -> Option<&Map> { self.state.map_id.as_ref().and_then(|id| self.maps.iter().find(|map| &map.id == id)) }
    fn snapshot_value(&self) -> Value {
        let mut snapshot = serde_json::to_value(&self.state).unwrap();
        for field in ["rng","legacyPlayer","entryStepPending","vm","turnInProgress","actionQueue","pendingAllocation","hostActivity"] {snapshot.as_object_mut().unwrap().remove(field);}
        snapshot["life"]=self.state.legacy_player.as_ref().map(|player|serde_json::to_value(player.life()).unwrap()).unwrap_or(Value::Null);
        let status_open=matches!(self.state.host_activity,Some(HostActivity::Status{..}));
        snapshot["statusOverview"]=if status_open{self.status_overview().unwrap_or(Value::Null)}else{Value::Null};
        snapshot["statusDetails"]=if status_open{self.state.legacy_player.as_ref().zip(self.rules.as_ref()).and_then(|(player,rules)|crate::status::detail_rows(player,&self.state.career_history,rules).ok()).unwrap_or(Value::Null)}else{Value::Null};
        snapshot["selectedTile"] = self.current_map().and_then(|map| self.state.tile_id.and_then(|id| map.chips.get(id))).map(|tile| serde_json::to_value(tile).unwrap()).unwrap_or(Value::Null);
        snapshot
    }
    pub fn snapshot_json(&self) -> String { self.snapshot_value().to_string() }
    fn response(&self, error: Option<(&str, String)>, events: Vec<Value>) -> String {
        let mut response = json!({"ok":error.is_none(), "snapshot":self.snapshot_value(), "events":events});
        if let Some((code,message)) = error { response["error"] = json!({"code":code,"message":message}); }
        response.to_string()
    }

    pub fn dispatch(&mut self, command_json: &str) -> String {
        let command: Command = match serde_json::from_str(command_json) {
            Ok(command) => command,
            Err(e) => return self.response(Some(("invalid_command",e.to_string())),vec![]),
        };
        let expected = match self.state.last_seq.checked_add(1) { Some(seq) => seq, None => return self.response(Some(("sequence_exhausted","命令序号已用尽。".into())),vec![]) };
        if command.seq() != expected { return self.response(Some(("out_of_order",format!("Expected sequence {expected}, got {}",command.seq()))),vec![]); }
        // Valid, ordered commands are consumed even when a gameplay precondition fails.
        self.state.last_seq = expected;
        match command {
            Command::ExploreMap { map_id, .. } => {
                if self.state.legacy_player.is_some() {return self.response(Some(("game_active","正在进行的人生不能直接切换浏览地图。".into())),vec![]);}
                let Some(map) = self.maps.iter().find(|map| map.id == map_id) else { return self.response(Some(("unknown_map",format!("Unknown map {map_id}"))),vec![]); };
                let tile_id = map.tile_index(&map.start).unwrap();
                self.state.map_id = Some(map_id.clone()); self.state.tile_id = Some(tile_id);
                self.state.phase = "exploring".into();
                self.state.message = "正在浏览原版地图与格子数据；这不是已完成的游戏流程。".into();
                self.state.revision += 1;
                self.response(None,vec![json!({"type":"mapSelected","mapId":map_id,"tileId":tile_id})])
            }
            Command::InspectTile { tile_id, .. } => {
                if self.state.phase!="exploring" {return self.response(Some(("wrong_phase","只有地图浏览可直接选择格子。".into())),vec![]);}
                let Some(map) = self.current_map() else { return self.response(Some(("no_map","尚未选择地图。".into())),vec![]); };
                if tile_id >= map.chips.len() { return self.response(Some(("unknown_tile","地图格不存在。".into())),vec![]); }
                self.state.tile_id = Some(tile_id); self.state.revision += 1;
                self.response(None,vec![json!({"type":"tileSelected","tileId":tile_id})])
            }
            Command::FollowLink { branch, .. } => {
                if self.state.phase != "exploring" { return self.response(Some(("wrong_phase","只有地图浏览可直接选择连接路径。".into())),vec![]); }
                let Some(map) = self.current_map() else { return self.response(Some(("no_map","尚未选择地图。".into())),vec![]); };
                let path = match map.trace_path(self.state.tile_id.unwrap(), &[branch]) { Ok(path) => path, Err(e) => return self.response(Some(("invalid_link",e)),vec![]) };
                self.state.tile_id = path.last().copied(); self.state.revision += 1;
                self.response(None,vec![json!({"type":"pathTraced","tiles":path})])
            }
            Command::NewGame { profile, seed, .. } => {
                if let Err(e) = profile.validate() { return self.response(Some(("invalid_profile",e)),vec![]); }
                if self.program.is_none(){return self.response(Some(("missing_events","缺少原版事件程序，无法开始游戏。".into())),vec![]);}
                let Some(map)=self.maps.iter().find(|map|map.id=="park")else{return self.response(Some(("missing_map","缺少原版公园地图。".into())),vec![]);};
                let tile_id=map.tile_index(&map.start).unwrap();
                let map_facing=map.initial_facing();
                let mut rng=ReplayRng::new(seed);let decoration_delays=map.decoration_delays(&mut rng);
                let spirits=match Spirits::initialize(&map.coordinate_index,&map.start,&mut rng){Ok(spirits)=>spirits,Err(error)=>return self.response(Some(("spirit_positions",error)),vec![])};
                let mut player=LegacyPlayer::initialize();
                player.set_i16(0x4b6498,i16::from(profile.sex=="female")).unwrap();
                // The main menu sets this default at 0x00420db0 before initialization.
                player.set_i16(0x4b64e6,5).unwrap();
                self.state.stats=player.stats();self.state.legacy_player=Some(player);
                self.state.player=Some(profile);self.state.rng=rng;self.state.map_id=Some("park".into());self.state.tile_id=Some(tile_id);self.state.map_facing=map_facing;
                self.state.map_decorations=Some(MapDecorations{map_id:"park".into(),load_serial:1,delays_ms:decoration_delays});
                self.state.turn=0;self.state.entry_step_pending=true;self.state.vm=None;self.state.pending=None;self.state.scene=json!({});self.state.action_queue.clear();self.state.turn_in_progress=false;
                self.state.pending_allocation=None;
                self.state.active_condition=None;
                self.state.host_activity=None;self.state.shop_visits=0;
                self.state.career_levels=vec![1;25];self.state.career_history.clear();
                self.state.active_mission=None;self.state.inventory=Inventory::default();
                self.state.queued_die=None;
                self.state.travel_origin=None;
                self.state.study_origin=None;
                self.state.spouse_index=-1;self.state.promotion_years.clear();
                self.state.relationship_candidates=vec![-1;8];self.state.gift_level=1;self.state.fortune_visits=0;
                self.state.relationship_new_targets=[-1;8];self.state.dating_target=-1;self.state.relationship_years=[None;3];
                self.state.known_equipment=vec![EquipmentKnowledge{learned:false,level:0};18];self.state.carried_equipment=vec![EquipmentSlot{id:-1,level:0};8];self.state.first_equipment_visit=true;
                self.state.housing=HousingState::default();
                self.state.battle_persistent=None;
                self.state.life_history=Some(Self::initial_life_history());
                self.state.spirits=spirits;self.state.last_roll_steps=0;
                self.state.phase="playing".into();self.state.message="新的人生从这里开始。".into();self.state.revision+=1;
                self.response(None,vec![json!({"type":"newGameStarted","mapId":"park","tileId":tile_id})])
            }
            Command::RollDice { value,.. } => match self.roll(value){Ok(events)=>self.response(None,events),Err(e)=>self.response(Some(("cannot_roll",e)),vec![])},
            Command::AdvanceEvent { .. }=>self.resume_event(VmInput::Continue),
            Command::ChooseOption { index,.. }=>self.resume_event(VmInput::Choose{index}),
            Command::MinigameTick{elapsed_ms,..}=>self.resume_minigame(Some(elapsed_ms),None,None,None,None),
            Command::MinigameAction{action,index,x,y,left,right,fire,..}=>self.resume_minigame(None,Some(action.as_str()),index,x.zip(y),Some((left.unwrap_or(false),right.unwrap_or(false),fire.unwrap_or(false)))),
            Command::RelationshipTick{elapsed_ms,hover_index,..}=>self.resume_relationship(Some(elapsed_ms),None,hover_index),
            Command::RelationshipAction{action,index,..}=>self.resume_relationship(None,Some(action.as_str()),index),
            Command::CompetitionTick{elapsed_ms,..}=>self.resume_competition(Some(elapsed_ms),None,None),
            Command::CompetitionAction{action,index,..}=>self.resume_competition(None,Some(action.as_str()),index),
            Command::EndingTick{elapsed_ms,..}=>self.resume_ending(Some(elapsed_ms),None,None),
            Command::EndingAction{action,index,..}=>self.resume_ending(None,Some(action.as_str()),index),
            Command::BattleTick{elapsed_ms,..}=>self.resume_battle(Some(elapsed_ms),None,None),
            Command::BattleAction{action,index,..}=>self.resume_battle(None,Some(action.as_str()),index),
            Command::OpenInventory{tab,..}=>self.open_inventory(tab.as_deref()),
            Command::InventoryAction{action,index,tab,..}=>self.resume_inventory(None,Some(action.as_str()),index,tab.as_deref()),
            Command::InventoryTick{elapsed_ms,..}=>self.resume_inventory(Some(elapsed_ms),None,None,None),
            Command::OpenStatus{..}=>self.open_status(),
            Command::StatusAction{action,page,delta,index,..}=>self.resume_status(None,Some(action.as_str()),page.as_deref(),delta.unwrap_or(0),index),
            Command::StatusTick{elapsed_ms,..}=>self.resume_status(Some(elapsed_ms),None,None,0,None),
            Command::AllocateStats{allocations,..}=>{
                let Some(reward)=self.state.pending_allocation.clone()else{return self.response(Some(("wrong_phase","当前没有待分配的成长点数。".into())),vec![]);};
                if let Err(error)=reward.apply(self.state.legacy_player.as_mut().unwrap(),&allocations){return self.response(Some(("invalid_allocation",error)),vec![]);}
                self.state.pending_allocation=None;self.state.pending=None;self.state.phase="playing".into();self.state.revision+=1;self.sync_player();
                let mut events=vec![json!({"type":"statsAllocated","allocations":allocations,"money":reward.money})];self.run_actions(&mut events);self.response(None,events)
            }
        }
    }

    fn sync_player(&mut self) {
        if let Some(player)=&self.state.legacy_player {
            self.state.stats=player.stats();self.state.turn=player.i16(0x4b64aa).unwrap().max(0)as u32;
        }
    }

    fn trigger_skills(&mut self,trigger:i16,events:&mut Vec<Value>)->Result<(),String>{
        let Some(rules)=self.rules.as_ref().and_then(|rules|rules.get("skills"))else{return Err("缺少原版技能规则表。".into());};
        let rules:Vec<SkillDefinition>=serde_json::from_value(rules.clone()).map_err(|error|error.to_string())?;
        let player=self.state.legacy_player.as_mut().ok_or("No player")?;
        let levels=std::array::from_fn(|i|player.i16(0x4b6508+i as u32*2).unwrap());
        let effects=skill_effects(trigger,&rules,&levels,&|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)})?;
        for write in effects.writes {if write.address==0x4b648a{player.set_i32(write.address,write.value)?;}else{player.set_i16(write.address,write.value as i16)?;}}
        for effect in effects.activations{events.push(json!({"type":"skillApplied","effect":effect}));}
        self.sync_player();Ok(())
    }

    fn select_event(&mut self,events:&mut Vec<Value>)->Result<i32,String>{
        let player=self.state.legacy_player.as_ref().ok_or("No player")?;
        let rules=self.rules.as_ref().ok_or("Missing original event tables")?;
        let selected=select_ordinary_event(
            &|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)},
            &|address|rules["tableSegments"].as_array()?.iter().find_map(|segment|{
                let base=u32::from_str_radix(segment["baseAddress"].as_str()?.trim_start_matches("0x"),16).ok()?;
                let offset=address.checked_sub(base)?;
                if offset%2!=0{return None;}
                segment["values"].as_array()?.get(offset as usize/2)?.as_i64().map(|value|value as i16)
            }),&mut ||self.state.rng.next_rand())?;
        let player=self.state.legacy_player.as_mut().unwrap();
        for write in selected.writes{player.set_i16(write.address,write.value as i16)?;}
        self.trigger_skills(selected.skill_trigger,events)?;
        Ok(selected.next_node)
    }

    fn block(&mut self,code:&str,message:String,details:Value) {
        self.sync_player();
        self.state.phase="blocked".into();self.state.message=message.clone();
        self.state.pending=Some(json!({"kind":"blocked","code":code,"text":message,"details":details}));
    }

    fn roll(&mut self,forced:Option<u8>)->Result<Vec<Value>,String> {
        if self.state.phase!="playing" || self.state.turn_in_progress{return Err("请先完成当前事件。".into());}
        if forced.is_some() && (self.state.spirits.active!=Some(0) || self.state.legacy_player.as_ref().unwrap().life().traveling){return Err("只有当前骰子神的能力可以指定点数。".into());}
        let map=self.current_map().ok_or("没有正在进行的地图。")?;
        let start=self.state.tile_id.ok_or("没有角色位置。")?;
        let mut rng=self.state.rng.clone();
        let rolled=roll_dice(&mut rng,forced)?;
        let movement=plan_movement(map,start,rolled,self.state.entry_step_pending,&mut rng)?;
        let mut queue=Vec::new();
        let mut segment=Vec::new();
        for (index,tile_id) in movement.path.iter().copied().enumerate(){
            segment.push(tile_id);
            let event_id=map.chips[tile_id].event_id as i32;
            let landed=index+1==movement.path.len();
            if landed || [0x5dc4,0x5dcc,0x5ddb,0x5de7,0x5dda].contains(&event_id){queue.push(TileAction{tile_id,event_id,landed,path:std::mem::take(&mut segment),spirit_checked:false});}
        }
        let mut events=vec![json!({"type":"diceRolled","rolled":rolled})];
        self.state.rng=rng;self.state.entry_step_pending=movement.entry_step_pending;
        self.state.last_roll_steps=movement.path.len()as u8;
        self.state.action_queue=queue;self.state.turn_in_progress=true;self.state.pending=None;self.state.scene=json!({});
        self.state.queued_die=Some(rolled);
        if movement.reached_end{
            let player=self.state.legacy_player.as_mut().unwrap();
            player.set_i16(if player.life().traveling{0x4b64b0}else{0x4b64b4},1).unwrap();
        }
        self.state.revision+=1;
        self.run_actions(&mut events);
        Ok(events)
    }

    fn resume_event(&mut self,input:VmInput)->String {
        if self.state.phase!="event" {return self.response(Some(("wrong_phase","当前没有可继续的事件。".into())),vec![]);}
        if self.state.host_activity.is_some(){return self.resume_host_activity(input);}
        let Some(vm)=&mut self.state.vm else{return self.response(Some(("missing_event","当前事件状态不存在。".into())),vec![]);};
        if let Err(error)=vm.resume(input){return self.response(Some(("invalid_event_input",error)),vec![]);}
        self.state.pending=None;self.state.revision+=1;
        let mut events=Vec::new();self.advance_vm(&mut events);
        self.response(None,events)
    }

    fn begin_stat_shop(&mut self)->Result<(),String>{
        let player=self.state.legacy_player.as_ref().ok_or("No player")?;
        let young=player.life().stage<=3;
        let groups=self.rules.as_ref().and_then(|rules|rules.get("statShops")).and_then(|shops|shops.get(if young{"young"}else{"adult"})).ok_or("缺少原版商店商品表。")?;
        let groups:Vec<Vec<StatShopItem>>=serde_json::from_value(groups.clone()).map_err(|error|error.to_string())?;
        self.state.shop_visits=(self.state.shop_visits+1)%3;
        let shop=StatShop::generate_for_visit(&groups,!young && self.state.shop_visits==0,self.state.relationship_candidates.iter().any(|id|*id!=-1),&mut self.state.rng)?;
        let player=self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)?!=-1 && player.i16(0x4b64ce)?==4{player.set_i16(0x4b64d0,1)?;}
        self.state.host_activity=Some(HostActivity::StatShop{shop});self.refresh_host_activity();Ok(())
    }

    fn begin_career(&mut self,entering_adulthood:bool)->Result<(),String>{
        self.state.legacy_player.as_mut().unwrap().update_skills()?;
        let careers:Vec<CareerRule>=serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("careers")).ok_or("缺少原版职业规则。")?.clone()).map_err(|error|error.to_string())?;
        self.state.host_activity=Some(HostActivity::Career{careers,entering_adulthood});self.refresh_host_activity();Ok(())
    }

    fn begin_item_shop(&mut self)->Result<(),String>{
        let rules=self.rules.as_ref().ok_or("Missing original item shop tables")?;
        let catalog:ItemShopCatalog=serde_json::from_value(rules["itemShop"].clone()).map_err(|error|error.to_string())?;
        let equipment:Vec<EquipmentDefinition>=serde_json::from_value(rules["equipment"].clone()).map_err(|error|error.to_string())?;
        let player=self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)?!=-1 && player.i16(0x4b64ce)?==5{player.set_i16(0x4b64d0,1)?;}
        let shop=NativeShop::generate(&catalog,player,&self.state.housing,&self.state.known_equipment,&self.state.carried_equipment,&equipment,&mut self.state.rng)?;
        self.state.host_activity=Some(HostActivity::NativeShop{shop});self.refresh_host_activity();Ok(())
    }

    fn begin_housing(&mut self)->Result<(),String>{
        let catalog:HousingCatalog=serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("housing")).ok_or("Missing original housing tables")?.clone()).map_err(|error|error.to_string())?;
        let player=self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)?!=-1 && player.i16(0x4b64ce)?==6{player.set_i16(0x4b64d0,1)?;}
        self.state.host_activity=Some(HostActivity::Housing{catalog,result:None,preview_investment:false});self.refresh_host_activity();Ok(())
    }

    fn choose_career(&mut self,career:i16)->Result<(),String>{
        let player=self.state.legacy_player.as_mut().unwrap();
        let old=player.life().profession;
        player.set_i16(0x4b64a2,career)?;player.set_i16(0x4b64a6,*self.state.career_levels.get(career as usize).ok_or("Invalid career")?)?;
        player.update_skills()?;
        if old!=career && self.state.career_history.len()<98{
            self.state.career_history.push(json!({"year":player.life().year,"career":career,"rank":player.i16(0x4b6508+career as u32*2)?}));
            if old!=-1{self.state.life_history.get_or_insert_with(Self::initial_life_history)[6].push(i32::from(player.life().year));}
        }
        Ok(())
    }

    /// FUN_00419d20: update the stage before selecting the original map table.
    fn transition_stage(&mut self,stage:i16,events:&mut Vec<Value>)->Result<(),String>{
        let data=self.rules.as_ref().and_then(|rules|rules["stages"].as_array()).and_then(|stages|stages.iter().find(|item|item["id"].as_i64()==Some(i64::from(stage)))).ok_or("Missing original stage table")?;
        let path=data["mapPath"].as_str().ok_or("Missing stage map")?;
        let id=path.rsplit('/').next().ok_or("Invalid stage map path")?.to_owned();
        let map=self.maps.iter().find(|map|map.id==id).ok_or("Missing stage map resources")?;
        let tile_id=map.tile_index(&map.start).unwrap();
        let player=self.state.legacy_player.as_mut().unwrap();let previous=player.life().stage;
        if let Some(index)=match stage{4=>Some(4),5 if previous==3=>Some(3),6 if previous!=6=>Some(5),_=>None}{self.state.life_history.get_or_insert_with(Self::initial_life_history)[index].push(i32::from(player.life().year));}
        player.set_i16(0x4b64a8,0)?;player.set_i16(0x4b64a0,previous)?;player.set_i16(0x4b649e,stage)?;
        if stage!=4 || previous!=3 {self.state.tile_id=Some(tile_id);self.state.entry_step_pending=true;self.state.map_facing=map.initial_facing();}
        self.state.map_id=Some(id.to_owned());self.load_map_decorations()?;self.state.scene=json!({});self.sync_player();
        events.push(json!({"type":"stageChanged","previous":previous,"stage":stage,"mapId":id,"tileId":self.state.tile_id}));
        if stage==6 && previous!=6{self.begin_career(true)?;}
        Ok(())
    }

    fn enter_travel(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        let ids=self.rules.as_ref().and_then(|rules|rules["travelMaps"].as_array()).ok_or("Missing original travel map table")?;
        if ids.len()!=4{return Err("Original travel table must have four maps".into());}
        let index=self.state.rng.next_rand()as usize%4;
        let path=ids[index].as_str().or_else(||ids[index]["mapPath"].as_str()).ok_or("Missing travel path")?;
        let id=path.rsplit('/').next().ok_or("Invalid travel path")?;
        let id=id.to_owned();let map=self.maps.iter().find(|map|map.id==id).ok_or("Missing travel map")?;
        self.state.travel_origin=Some(MapPosition{map_id:self.state.map_id.clone().unwrap(),tile_id:self.state.tile_id.unwrap(),facing:self.state.map_facing});
        self.state.map_id=Some(id.to_owned());self.state.tile_id=map.tile_index(&map.start);self.state.entry_step_pending=true;self.state.map_facing=map.initial_facing();
        self.load_map_decorations()?;
        let player=self.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b64ac,1)?;player.set_i16(0x4b64b0,0)?;
        events.push(json!({"type":"travelStarted","mapId":id,"tileId":self.state.tile_id}));self.sync_player();Ok(())
    }

    fn return_from_travel(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        let origin=self.state.travel_origin.take().ok_or("Missing original travel return position")?;
        self.state.map_id=Some(origin.map_id.clone());self.state.tile_id=Some(origin.tile_id);self.state.entry_step_pending=true;self.state.map_facing=origin.facing;
        self.load_map_decorations()?;
        let player=self.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b64ac,0)?;player.set_i16(0x4b64b0,0)?;
        events.push(json!({"type":"travelEnded","mapId":origin.map_id,"tileId":origin.tile_id}));self.sync_player();Ok(())
    }

    fn load_map_decorations(&mut self)->Result<(),String>{
        let id=self.state.map_id.as_ref().ok_or("Missing decoration map")?.clone();
        let map=self.maps.iter().find(|map|map.id==id).ok_or("Missing decoration map")?;let delays_ms=map.decoration_delays(&mut self.state.rng);
        let load_serial=self.state.map_decorations.as_ref().map_or(1,|state|state.load_serial.wrapping_add(1));
        self.state.map_decorations=Some(MapDecorations{map_id:id,load_serial,delays_ms});Ok(())
    }

    fn spirit_map_context(&self)->Result<(Vec<Coordinate>,Coordinate),String>{
        let map=self.current_map().ok_or("Missing original spirit map")?;let tile=map.chips.get(self.state.tile_id.ok_or("Missing player tile")?).ok_or("Invalid player tile")?;
        Ok((map.coordinate_index.clone(),Coordinate{row:tile.row,column:tile.column}))
    }

    fn refresh_spirit_positions(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        let(pool,position)=self.spirit_map_context()?;self.state.spirits.change_map(&pool,&position,&mut self.state.rng)?;
        events.push(json!({"type":"spiritsRepositioned","positions":self.state.spirits.positions}));Ok(())
    }

    fn begin_spirit_lottery(&mut self)->Result<(),String>{
        let spirit=self.state.spirits.active.ok_or("Missing active spirit")?;
        let game=SpiritLottery::new(spirit,self.state.last_roll_steps%2==1,&mut self.state.rng)?;
        self.state.host_activity=Some(HostActivity::SpiritLottery{game});self.refresh_host_activity();Ok(())
    }

    /// FUN_004159a0 checks spirits after movement and before the landed event.
    fn prepare_spirit_event(&mut self,events:&mut Vec<Value>)->Result<bool,String>{
        let life=self.state.legacy_player.as_ref().unwrap().life();if life.traveling || life.stage==2{return Ok(false);}
        let(pool,position)=self.spirit_map_context()?;
        if let Some(spirit)=self.state.spirits.encounter(&position){
            let player=self.state.legacy_player.as_mut().unwrap();
            self.state.spirits.attach(spirit,player.i16(0x4b64aa)?,&pool,&position,&mut self.state.rng)?;player.set_i16(0x4b64d6,spirit as i16)?;
            events.push(json!({"type":"spiritAttached","spirit":spirit,"expiresTurn":self.state.spirits.expires_turn}));
            self.state.host_activity=Some(HostActivity::SpiritEncounter{spirit});self.refresh_host_activity();return Ok(true);
        }
        if self.state.spirits.active.is_some_and(|spirit|spirit>=2){self.begin_spirit_lottery()?;return Ok(true);}
        Ok(false)
    }

    fn resume_spirit_encounter(&mut self,input:VmInput)->String{
        if !matches!(input,VmInput::Continue|VmInput::Choose{index:0}){return self.response(Some(("spirit_input","请继续阅读神灵事件。".into())),vec![]);}
        let Some(HostActivity::SpiritEncounter{spirit})=self.state.host_activity.take()else{unreachable!()};
        self.state.pending=None;self.state.scene=json!({});self.state.revision+=1;let mut events=Vec::new();
        if spirit>=2{if let Err(error)=self.begin_spirit_lottery(){self.block("spirit_lottery",error,Value::Null);}}
        else{self.state.phase="playing".into();self.run_actions(&mut events);}
        self.response(None,events)
    }

    fn resume_spirit_lottery(&mut self,elapsed:Option<u32>,action:Option<&str>)->String{
        let Some(HostActivity::SpiritLottery{game})=self.state.host_activity.as_mut()else{unreachable!()};
        let result=if let Some(elapsed)=elapsed{game.tick(elapsed)}else if action==Some("stop"){game.stop(&mut self.state.rng)}else{Err("请选择停止转盘。".into())};
        if let Err(error)=result{return self.response(Some(("spirit_input",error)),vec![]);}
        self.state.revision+=1;let mut events=Vec::new();
        if game.phase==SpiritLotteryPhase::Finished{
            if let Err(error)=game.apply(self.state.legacy_player.as_mut().unwrap()){self.block("spirit_result",error,Value::Null);return self.response(None,events);}
            events.push(json!({"type":"spiritEffect","spirit":game.spirit,"amount":game.amount(),"positiveMoney":game.positive_money,"attribute":game.attribute.value}));
            self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});self.sync_player();self.state.phase="playing".into();self.run_actions(&mut events);
        }else{self.refresh_host_activity();}
        self.response(None,events)
    }

    fn show_notice(&mut self,pages:Vec<i32>,continuation:NoticeContinuation)->Result<(),String>{
        if pages.is_empty(){return Err("Original notice has no pages".into());}
        for id in &pages{if self.program.as_ref().unwrap().dialogue(*id).is_none(){return Err(format!("Missing original dialogue {id}"));}}
        self.state.host_activity=Some(HostActivity::Notice{pages,page:0,continuation});self.refresh_host_activity();Ok(())
    }

    fn exam_questions(&self)->Result<Vec<ExamQuestion>,String>{
        let records=self.questionnaires.as_ref().and_then(|data|data["highTest"].get("records")).ok_or("缺少原版高考题库。")?;
        serde_json::from_value(records.clone()).map_err(|error|error.to_string())
    }

    fn begin_school_exam(&mut self)->Result<(),String>{
        let questions=self.exam_questions()?;let exam=SchoolExam::new(&questions,&mut self.state.rng)?;
        self.state.host_activity=Some(HostActivity::SchoolExam{exam});self.refresh_host_activity();Ok(())
    }

    fn begin_competition(&mut self,turn_outcome:TurnOutcome)->Result<(),String>{
        let data=self.rules.as_ref().and_then(|rules|rules.get("competition")).ok_or("缺少原版学生大赛规则。")?;
        let rules:CompetitionRules=serde_json::from_value(data.clone()).map_err(|error|error.to_string())?;
        let player=self.state.legacy_player.as_ref().unwrap();
        let game=StudentCompetition::new(player.life().stage,player.i16(0x4b6498)?as usize,self.state.player.as_ref().unwrap().name.clone(),player.i16(0x4b6494)?,player.i16(0x4b648e)?,rules);
        self.state.host_activity=Some(HostActivity::Competition{game,turn_outcome});self.refresh_host_activity();Ok(())
    }

    fn resume_competition(&mut self,elapsed:Option<u32>,action:Option<&str>,index:Option<usize>)->String{
        if elapsed.is_some_and(|ms|ms>60000){return self.response(Some(("competition_time","比赛时间步长无效。".into())),vec![]);}
        let questions=if matches!(action,Some("choose"|"answer")){match self.exam_questions(){Ok(questions)=>questions,Err(error)=>return self.response(Some(("competition_questions",error)),vec![])}}else{Vec::new()};
        let Some(HostActivity::Competition{game,turn_outcome})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_competition","当前没有学生大赛。".into())),vec![]);};
        let result=if let Some(ms)=elapsed{game.tick(ms,&mut self.state.rng)}else{match action{
            Some("continue")=>game.continue_action(&mut self.state.rng),
            Some("choose")=>index.ok_or("请选择比赛项目。".into()).and_then(|index|game.select(index,&questions,&mut self.state.rng)),
            Some("answer")=>index.ok_or("请选择答案。".into()).and_then(|index|game.answer(index,&questions,&mut self.state.rng)),
            Some("left")=>game.running_input(true,false),
            Some("right")=>game.running_input(false,true),
            _=>Err("比赛操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("competition_input",error)),vec![]);}
        let mut events=Vec::new();let finished=game.phase==CompetitionPhase::Finished;let outcome=turn_outcome.clone();
        if let Some(prize)=game.take_prize(){let player=self.state.legacy_player.as_mut().unwrap();player.set_i32(0x4b648a,player.i32(0x4b648a).unwrap().wrapping_add(prize)).unwrap();events.push(json!({"type":"competitionPrize","amount":prize,"place":game.place}));}
        if finished{events.push(json!({"type":"competitionCompleted","selected":game.selected,"place":game.place,"prize":game.prize}));self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});self.finish_turn_transitions(outcome,&mut events);}
        else{self.refresh_host_activity();}
        self.state.revision+=1;self.sync_player();self.response(None,events)
    }

    fn collect_salary(&mut self,landed:bool,events:&mut Vec<Value>)->Result<(),String>{
        let rules=self.rules.as_ref().ok_or("Missing original salary tables")?;
        let player=self.state.legacy_player.as_ref().ok_or("Missing player")?;
        let career=player.life().profession;
        let definition=rules["careers"].as_array().and_then(|items|items.iter().find(|item|item["id"].as_i64()==Some(i64::from(career)))).map(|item|serde_json::from_value::<CareerRule>(item.clone())).transpose().map_err(|error|error.to_string())?;
        let rank=payday_rank(player,definition.as_ref(),&self.state.inventory.special)?;
        let base=if career==-1{rules["unemployedBaseSalary"].as_i64().ok_or("Missing original unemployed salary")?}else{rules["baseSalaries"].get(career as usize).and_then(Value::as_i64).ok_or("Missing original base salary")?}as i16;
        let multipliers:Vec<i32>=serde_json::from_value(rules["salaryMultipliers"].clone()).map_err(|error|error.to_string())?;
        let spouse=if self.state.spouse_index==-1{0}else{
            let index=i32::from(self.state.spouse_index)%8+i32::from(player.i16(0x4b6498)?)*8;
            rules["spouseSalary"].get(usize::try_from(index).map_err(|_|"Invalid original spouse index")?).and_then(Value::as_i64).ok_or("Missing original spouse salary")? as i16
        };
        if rank>player.life().profession_level{self.state.promotion_years.push(player.life().year);}
        let outcome=collect_payday(self.state.legacy_player.as_mut().unwrap(),rank,base,&multipliers,spouse,landed)?;
        if career>=0{*self.state.career_levels.get_mut(career as usize).ok_or("Invalid original career history")?=rank;}
        events.push(json!({"type":"payday","career":career,"landed":landed,"outcome":outcome}));
        self.trigger_skills(4,events)?;self.sync_player();Ok(())
    }

    fn begin_minigame(&mut self)->Result<(),String>{
        let variant=self.state.rng.next_rand()%6;
        let player=self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64ce)?==11{
            let target=player.i16(0x4b64cc)?;
            if !(1..=6).contains(&target) || target==[3,4,5,2,6,1][variant as usize]{player.set_i16(0x4b64d0,1)?;}
        }
        self.state.host_activity=Some(match variant{
            0=>{let mut game=GasGame::new(player.i16(0x4b648e)?);game.target_state=self.state.gas_target_state.unwrap_or(0);HostActivity::Gas{game}},
            1=>HostActivity::Tracking{game:TrackingGame::with_intelligence(&mut self.state.rng,player.i16(0x4b6492)?)},
            2=>HostActivity::Hammer{game:HammerGame::with_morality(player.i16(0x4b6490)?)},
            3=>HostActivity::Fire{game:FireGame::new(player.i16(0x4b6494)?)},
            4=>HostActivity::Firebee{game:FirebeeGame::new(player.i32(0x4b648a)?)},
            5=>HostActivity::Fist{game:FistGame::new(player.i16(0x4b6490)?)},
            _=>return Err(format!("原版小游戏 {variant} 的规则与输入模块仍在还原。")),
        });self.refresh_host_activity();Ok(())
    }

    fn begin_fortune(&mut self)->Result<(),String>{
        let records:Vec<FortuneRecord>=serde_json::from_value(self.questionnaires.as_ref().and_then(|data|data["fortune"].get("records")).ok_or("Missing original fortune questionnaires")?.clone()).map_err(|error|error.to_string())?;
        if records.len()!=50{return Err("Original fortune questionnaire count must be 50".into());}
        let index=self.state.rng.next_rand()as usize%records.len();
        let player=self.state.legacy_player.as_mut().unwrap();
        if player.i16(0x4b64cc)?>0 && player.i16(0x4b64ce)?==13{player.set_i16(0x4b64d0,1)?;}
        let game=FortuneGame::new(records[index].clone())?;
        self.state.host_activity=Some(HostActivity::Fortune{game});self.refresh_host_activity();Ok(())
    }

    fn begin_relationship(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        let rules:RelationshipRules=serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("relationships")).ok_or("Missing original relationship tables")?.clone()).map_err(|error|error.to_string())?;
        let player=self.state.legacy_player.as_mut().unwrap();let target=player.i16(0x4b64cc)?;let mission=player.i16(0x4b64ce)?==12;
        if target!=-1 && mission{player.set_i32(0x4b64d0,1)?;}
        let state=RelationshipState{sex:player.i16(0x4b6498)?as u8,stage:player.life().stage,money:player.i32(0x4b648a)?,charm:player.i16(0x4b6494)?,romance:player.i16(0x4b6496)?,
            contacts:self.state.relationship_candidates.as_slice().try_into().map_err(|_|"Invalid original contact storage")?,new_targets:self.state.relationship_new_targets,
            dating_target:self.state.dating_target,spouse:self.state.spouse_index,gift_level:self.state.gift_level};
        let house=RelationshipHouse::new(&rules,state,target>0 && mission,&mut self.state.rng)?;
        let finished=house.phase==RelationshipPhase::Finished;self.state.host_activity=Some(HostActivity::Relationship{house});
        if finished{self.finish_relationship(events)?;}else{self.refresh_host_activity();}Ok(())
    }

    fn sync_relationship(&mut self)->Result<(),String>{
        let Some(HostActivity::Relationship{house})=self.state.host_activity.as_ref()else{return Err("Missing relationship activity".into());};
        let player=self.state.legacy_player.as_mut().unwrap();player.set_i32(0x4b648a,house.state.money)?;player.set_i16(0x4b6494,house.state.charm)?;player.set_i16(0x4b6496,house.state.romance)?;
        self.state.relationship_candidates=house.state.contacts.to_vec();self.state.relationship_new_targets=house.state.new_targets;
        self.state.dating_target=house.state.dating_target;self.state.spouse_index=house.state.spouse;self.state.gift_level=house.state.gift_level;
        self.sync_player();Ok(())
    }

    fn finish_relationship(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        self.sync_relationship()?;
        let Some(HostActivity::Relationship{house})=self.state.host_activity.as_ref()else{unreachable!()};
        let nodes=house.event_nodes()?;let kind=house.return_kind;
        if (1..=3).contains(&kind){let year=&mut self.state.relationship_years[usize::from(kind-1)];if year.is_none(){*year=Some(self.state.legacy_player.as_ref().unwrap().life().year);}}
        events.push(json!({"type":"relationshipCompleted","returnKind":kind,"eventNodes":nodes}));
        self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});
        self.state.vm.as_mut().ok_or("Missing relationship continuation")?.resume_relationship(nodes,kind)?;
        self.advance_vm(events);Ok(())
    }

    fn resume_relationship(&mut self,elapsed:Option<u32>,action:Option<&str>,index:Option<usize>)->String{
        if self.state.phase!="event"{return self.response(Some(("wrong_phase","当前没有正在进行的感情事件。".into())),vec![]);}
        let Some(HostActivity::Relationship{house})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_relationship","当前没有正在进行的感情事件。".into())),vec![]);};
        let missing=||"请选择一个人物或答案。".to_string();
        let result=if let Some(elapsed)=elapsed{house.tick(elapsed,index,&mut self.state.rng)}else{match action{
            Some("meet")=>house.meet(),Some("dates")=>house.dates(),Some("marriages")=>house.marriages(),
            Some("selectOffer")=>index.ok_or_else(missing).and_then(|index|{house.hover(index,&mut self.state.rng)?;house.select_offer(index)}),
            Some("answer")=>index.ok_or_else(missing).and_then(|index|house.answer(index,&mut self.state.rng)),
            Some("advanceNotice")=>house.advance_notice(),
            Some("selectDate")=>index.ok_or_else(missing).and_then(|index|house.select_date(index)),
            Some("selectMarriage")=>index.ok_or_else(missing).and_then(|index|house.select_marriage(index)),
            Some("reject")=>house.reject_selection(),Some("confirm")=>house.confirm_selection(),Some("close")=>{house.close();Ok(())},
            _=>Err("感情事件操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("relationship_input",error)),vec![]);}
        let finished=house.phase==RelationshipPhase::Finished;let mut events=Vec::new();self.state.revision+=1;
        if let Err(error)=self.sync_relationship(){self.block("relationship_state",error,Value::Null);}
        else if finished{if let Err(error)=self.finish_relationship(&mut events){self.block("relationship_continuation",error,Value::Null);}}
        else{self.refresh_host_activity();}
        self.response(None,events)
    }

    fn begin_equipment_workshop(&mut self)->Result<(),String>{
        let definitions:Vec<EquipmentDefinition>=serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("equipment")).ok_or("Missing original equipment workshop catalog")?.clone()).map_err(|error|error.to_string())?;
        let workshop=EquipmentWorkshop::new(&self.state.known_equipment,&self.state.carried_equipment,self.state.first_equipment_visit,&mut self.state.rng)?;
        self.state.host_activity=Some(HostActivity::EquipmentWorkshop{workshop,definitions});self.refresh_host_activity();Ok(())
    }

    fn resume_equipment_workshop(&mut self,input:VmInput)->String{
        let Some(HostActivity::EquipmentWorkshop{workshop,definitions})=self.state.host_activity.as_mut()else{unreachable!()};
        let mut finished=false;let mut events=Vec::new();
        let result=if workshop.outcome.is_some(){
            if matches!(input,VmInput::Continue|VmInput::Choose{index:0}){finished=true;Ok(())}else{Err("请继续阅读装备结算。".into())}
        }else if let VmInput::Choose{index}=input{
            match workshop.mode.as_deref(){
                None=>match index{0=>workshop.set_mode("purchase"),1=>workshop.set_mode("research"),2=>{finished=true;Ok(())},_=>Err("请选择购买、研究或离开。".into())},
                Some("purchase")=>match index{
                    0=>{
                        let definition=definitions.iter().find(|definition|definition.id==workshop.offered_id).cloned().ok_or("Missing original equipment offer");
                        match definition{Err(error)=>Err(error.into()),Ok(definition)=>{
                            let player=self.state.legacy_player.as_mut().unwrap();let mut money=player.i32(0x4b648a).unwrap();
                            let outcome=workshop.purchase(&definition,&mut self.state.known_equipment,&self.state.carried_equipment,&mut self.state.first_equipment_visit,&mut money);
                            match outcome{Err(error)=>Err(error),Ok(outcome)=>{player.set_i32(0x4b648a,money).unwrap();events.push(json!({"type":"equipmentPurchase","id":definition.id,"outcome":outcome}));Ok(())}}
                        }}
                    },
                    1=>{finished=true;Ok(())},_=>Err("请选择购买或离开。".into()),
                },
                Some("research")=>{
                    if index==18{finished=true;Ok(())}else{
                        let definition=definitions.iter().find(|definition|definition.id==index).cloned().ok_or("请选择携带中的装备。");
                        match definition{Err(error)=>Err(error.into()),Ok(definition)=>{
                            let player=self.state.legacy_player.as_mut().unwrap();let mut money=player.i32(0x4b648a).unwrap();
                            match workshop.research(&definition,&mut self.state.known_equipment,&mut self.state.carried_equipment,&mut money){
                                Err(error)=>Err(error),Ok(outcome)=>{player.set_i32(0x4b648a,money).unwrap();events.push(json!({"type":"equipmentResearch","id":definition.id,"outcome":outcome}));Ok(())}
                            }
                        }}
                    }
                }
                _=>Err("装备商店状态无效。".into()),
            }
        }else{Err("请选择装备操作。".into())};
        if let Err(error)=result{return self.response(Some(("equipment_input",error)),vec![]);}
        self.state.revision+=1;self.sync_player();
        if finished{
            self.state.host_activity=None;self.state.pending=None;
            if let Err(error)=self.state.vm.as_mut().ok_or("Missing equipment continuation").and_then(|vm|vm.resume(VmInput::HostReturned{next_node:None}).map_err(|_|"Invalid equipment continuation")){self.block("equipment_continuation",error.into(),Value::Null);}else{self.advance_vm(&mut events);}
        }else{self.refresh_host_activity();}
        self.response(None,events)
    }

    fn resume_fortune(&mut self,input:VmInput)->String{
        let Some(HostActivity::Fortune{game})=self.state.host_activity.as_mut()else{unreachable!()};
        let result=match(game.phase,input){
            (FortunePhase::Question,VmInput::Choose{index})=>{
                let count=game.record.questions[game.question_index].options.len();
                if index==count{game.cancel()}else{game.answer(index)}
            }
            (FortunePhase::Explanation,VmInput::Continue)|(FortunePhase::Explanation,VmInput::Choose{index:0})=>game.advance(),
            (FortunePhase::Explanation,VmInput::Choose{index:1})=>game.cancel(),
            _=>Err("请选择当前测验的答案或继续阅读解说。".into()),
        };
        if let Err(error)=result{return self.response(Some(("fortune_input",error)),vec![]);}
        self.state.revision+=1;
        if matches!(game.phase,FortunePhase::Finished|FortunePhase::Cancelled){
            let cancelled=game.phase==FortunePhase::Cancelled;let record_id=game.record.id;let percentages=game.percentages;
            if !cancelled{
                let player=self.state.legacy_player.as_mut().unwrap();let addresses=[0x4b6490,0x4b6492,0x4b648e,0x4b6494];
                let current=addresses.map(|address|player.i16(address).unwrap());
                for(address,value)in addresses.into_iter().zip(game.final_stats(current).unwrap()){player.set_i16(address,value).unwrap();}
            }
            self.state.fortune_visits=(self.state.fortune_visits+1).min(11);self.state.host_activity=None;self.state.pending=None;
            let mut events=vec![json!({"type":"fortuneCompleted","recordId":record_id,"cancelled":cancelled,"percentages":percentages})];
            if let Err(error)=self.state.vm.as_mut().ok_or("Missing fortune continuation").and_then(|vm|vm.resume(VmInput::HostReturned{next_node:None}).map_err(|_|"Invalid fortune continuation")){
                self.block("fortune_continuation",error.into(),Value::Null);
            }else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.refresh_host_activity();self.response(None,vec![])
    }

    fn resume_minigame(&mut self,elapsed:Option<u32>,action:Option<&str>,index:Option<usize>,point:Option<(i32,i32)>,keys:Option<(bool,bool,bool)>)->String{
        if self.state.phase!="event"{return self.response(Some(("wrong_phase","当前没有正在进行的小游戏。".into())),vec![]);}
        if matches!(self.state.host_activity,Some(HostActivity::SpiritLottery{..})){self.resume_spirit_lottery(elapsed,action)}
        else if matches!(self.state.host_activity,Some(HostActivity::Hammer{..})){self.resume_hammer(elapsed,action,index,point)}
        else if matches!(self.state.host_activity,Some(HostActivity::Fist{..})){self.resume_fist(elapsed,action,keys)}
        else if matches!(self.state.host_activity,Some(HostActivity::Gas{..})){self.resume_gas(elapsed,action)}
        else if matches!(self.state.host_activity,Some(HostActivity::Fire{..})){self.resume_fire(elapsed,action)}
        else if matches!(self.state.host_activity,Some(HostActivity::Firebee{..})){self.resume_firebee(elapsed,action,keys)}
        else{self.resume_tracking(elapsed,action,index)}
    }

    fn resume_firebee(&mut self,elapsed:Option<u32>,action:Option<&str>,keys:Option<(bool,bool,bool)>)->String{
        let Some(HostActivity::Firebee{game})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_minigame","当前没有射击游戏。".into())),vec![]);};
        let valid=if let Some(elapsed)=elapsed{game.tick(elapsed,&mut self.state.rng);true}else{match action{
            Some("start")=>game.start(),Some("help")=>game.show_help(),
            Some("input")=>{if game.phase!=FirebeePhase::Playing{false}else{let(left,right,fire)=keys.unwrap_or((false,false,false));game.input(left,right,fire);true}},
            Some("finishRewardAnimation")=>{let valid=game.phase==FirebeePhase::Reward;game.finish_reward_animation();valid},
            _=>false,
        }};
        if !valid{return self.response(Some(("minigame_input","当前射击游戏无法执行这项操作。".into())),vec![]);}
        self.state.revision+=1;
        if game.phase==FirebeePhase::Finished{
            let money=game.final_money().unwrap();let score=game.score;let failed=game.failed;let delta=money.wrapping_sub(game.starting_money);self.state.legacy_player.as_mut().unwrap().set_i32(0x4b648a,money).unwrap();
            self.state.host_activity=None;self.state.pending=None;let mut events=vec![json!({"type":"minigameCompleted","game":"firebee","failed":failed,"score":score,"moneyDelta":delta})];
            if let Err(error)=self.trigger_skills(2,&mut events).and_then(|_|self.state.vm.as_mut().ok_or("Missing minigame continuation")?.resume(VmInput::HostReturned{next_node:None})){self.block("minigame_continuation",error,Value::Null);}else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.refresh_host_activity();self.response(None,vec![])
    }

    fn resume_fire(&mut self,elapsed:Option<u32>,action:Option<&str>)->String{
        let Some(HostActivity::Fire{game})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_minigame","当前没有生火游戏。".into())),vec![]);};
        let result=if let Some(elapsed)=elapsed{game.tick(elapsed,&mut self.state.rng)}else{match action{
            Some("start")=>game.start(),Some("left")=>game.input(FireInput::Left),Some("right")=>game.input(FireInput::Right),Some("releaseLeft")=>game.input(FireInput::ReleaseLeft),Some("releaseRight")=>game.input(FireInput::ReleaseRight),Some("finishRewardAnimation")=>game.finish_reward_animation(),_=>Err("小游戏操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("minigame_input",error)),vec![]);}
        self.state.revision+=1;
        if game.reward_started{self.state.legacy_player.as_mut().unwrap().set_i16(0x4b6494,game.charm).unwrap();}
        if game.phase==FirePhase::Finished{
            let charm=game.final_charm().unwrap();let delta=charm.wrapping_sub(game.starting_charm);let failed=game.failed;self.state.legacy_player.as_mut().unwrap().set_i16(0x4b6494,charm).unwrap();
            self.state.host_activity=None;self.state.pending=None;let mut events=vec![json!({"type":"minigameCompleted","game":"fire","failed":failed,"charmDelta":delta})];
            if let Err(error)=self.trigger_skills(2,&mut events).and_then(|_|self.state.vm.as_mut().ok_or("Missing minigame continuation")?.resume(VmInput::HostReturned{next_node:None})){self.block("minigame_continuation",error,Value::Null);}else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.sync_player();self.refresh_host_activity();self.response(None,vec![])
    }

    fn resume_gas(&mut self,elapsed:Option<u32>,action:Option<&str>)->String{
        let Some(HostActivity::Gas{game})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_minigame","当前没有狗熊捕鱼游戏。".into())),vec![]);};
        let result=if let Some(elapsed)=elapsed{game.tick(elapsed,&mut self.state.rng)}else{match action{
            Some("start")=>game.start(),Some("help")=>game.show_help(),Some("throw")=>game.throw(),Some("finishRewardAnimation")=>game.finish_reward_animation(),_=>Err("小游戏操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("minigame_input",error)),vec![]);}
        self.state.revision+=1;
        if game.phase==GasPhase::Finished{
            let successes=game.successes;self.state.gas_target_state=Some(game.target_state);
            let player=self.state.legacy_player.as_mut().unwrap();let old=player.i16(0x4b648e).unwrap();let stamina=game.final_stamina(old).unwrap();player.set_i16(0x4b648e,stamina).unwrap();
            self.state.host_activity=None;self.state.pending=None;let mut events=vec![json!({"type":"minigameCompleted","game":"gas","successes":successes,"staminaDelta":stamina-old})];
            if let Err(error)=self.trigger_skills(2,&mut events).and_then(|_|self.state.vm.as_mut().ok_or("Missing minigame continuation")?.resume(VmInput::HostReturned{next_node:None})){self.block("minigame_continuation",error,Value::Null);}else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.refresh_host_activity();self.response(None,vec![])
    }

    fn resume_fist(&mut self,elapsed:Option<u32>,action:Option<&str>,keys:Option<(bool,bool,bool)>)->String{
        let Some(HostActivity::Fist{game})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_minigame","当前没有拳击游戏。".into())),vec![]);};
        let result=if let Some(elapsed)=elapsed{game.tick(elapsed,&mut self.state.rng)}else{match action{
            Some("start")=>game.start(),Some("help")=>game.show_help(),
            Some("input")=>{if game.phase!=FistPhase::Playing{Err("请先开始拳击游戏。".into())}else{let(left,right,fire)=keys.unwrap_or((false,false,false));game.input(left,right,fire);Ok(())}},
            Some("finishRewardAnimation")=>game.finish_reward_animation(),
            _=>Err("小游戏操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("minigame_input",error)),vec![]);}
        self.state.revision+=1;
        if game.phase==FistPhase::Finished{
            let score=game.display_score();let player=self.state.legacy_player.as_mut().unwrap();let old=player.i16(0x4b6490).unwrap();
            let morality=game.final_morality(old).unwrap();player.set_i16(0x4b6490,morality).unwrap();
            self.state.host_activity=None;self.state.pending=None;
            let mut events=vec![json!({"type":"minigameCompleted","game":"fist","score":score,"moralityDelta":morality-old})];
            if let Err(error)=self.trigger_skills(2,&mut events).and_then(|_|self.state.vm.as_mut().ok_or("Missing minigame continuation")?.resume(VmInput::HostReturned{next_node:None})){
                self.block("minigame_continuation",error,Value::Null);
            }else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.refresh_host_activity();self.response(None,vec![])
    }

    fn resume_hammer(&mut self,elapsed:Option<u32>,action:Option<&str>,index:Option<usize>,point:Option<(i32,i32)>)->String{
        if self.state.phase!="event"{return self.response(Some(("wrong_phase","当前没有正在进行的小游戏。".into())),vec![]);}
        let Some(HostActivity::Hammer{game})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_minigame","当前没有槌击游戏。".into())),vec![]);};
        let result=if let Some(elapsed)=elapsed{game.tick(elapsed,&mut self.state.rng)}else{match action{
            Some("start")=>game.start(),
            Some("strikeIndex")=>index.ok_or_else(||"请选择一个目标。".to_string()).and_then(|index|game.strike_index(index).map(|_|())),
            Some("strike")=>point.ok_or_else(||"缺少槌击位置。".to_string()).and_then(|(x,y)|game.strike(x,y).map(|_|())),
            Some("finishRewardAnimation")=>game.finish_reward_animation(),
            _=>Err("小游戏操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("minigame_input",error)),vec![]);}
        self.state.revision+=1;
        if game.phase==HammerPhase::Finished{
            let score=game.score;
            let player=self.state.legacy_player.as_mut().unwrap();let old=player.i16(0x4b6490).unwrap();
            let morality=game.final_morality(old).unwrap();player.set_i16(0x4b6490,morality).unwrap();
            self.state.host_activity=None;self.state.pending=None;
            let mut events=vec![json!({"type":"minigameCompleted","game":"hammer","score":score,"moralityDelta":morality-old})];
            if let Err(error)=self.trigger_skills(2,&mut events).and_then(|_|self.state.vm.as_mut().ok_or("Missing minigame continuation")?.resume(VmInput::HostReturned{next_node:None})){
                self.block("minigame_continuation",error,Value::Null);
            }else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.refresh_host_activity();self.response(None,vec![])
    }

    fn resume_tracking(&mut self,elapsed:Option<u32>,action:Option<&str>,index:Option<usize>)->String{
        if self.state.phase!="event"{return self.response(Some(("wrong_phase","当前没有正在进行的小游戏。".into())),vec![]);}
        let Some(HostActivity::Tracking{game})=self.state.host_activity.as_mut()else{return self.response(Some(("wrong_minigame","当前没有五物追踪游戏。".into())),vec![]);};
        let result=if let Some(elapsed)=elapsed{game.tick(elapsed,&mut self.state.rng)}else{match action{
            Some("startShuffle")=>game.start_shuffle(&mut self.state.rng),
            Some("choose")=>index.ok_or_else(||"请选择一个目标。".to_string()).and_then(|index|game.choose(index).map(|_|())),
            Some("nextRound")=>game.next_round(&mut self.state.rng),
            Some("finishIdleAnimation")=>{game.finish_idle_animation();Ok(())},
            Some("finishRewardAnimation")=>game.finish_reward_animation(),
            _=>Err("小游戏操作无效。".into()),
        }};
        if let Err(error)=result{return self.response(Some(("minigame_input",error)),vec![]);}
        self.state.revision+=1;
        if game.phase==TrackingPhase::Finished{
            let correct=game.correct;
            let player=self.state.legacy_player.as_mut().unwrap();let old=player.i16(0x4b6492).unwrap();
            let intelligence=game.final_intelligence(old).unwrap();player.set_i16(0x4b6492,intelligence).unwrap();
            self.state.host_activity=None;self.state.pending=None;
            let mut events=vec![json!({"type":"minigameCompleted","game":"tracking","correct":correct,"intelligenceDelta":intelligence-old})];
            if let Err(error)=self.trigger_skills(2,&mut events).and_then(|_|self.state.vm.as_mut().ok_or("Missing minigame continuation")?.resume(VmInput::HostReturned{next_node:None})){
                self.block("minigame_continuation",error,Value::Null);
            }else{self.advance_vm(&mut events);}
            self.sync_player();return self.response(None,events);
        }
        self.refresh_host_activity();self.response(None,vec![])
    }

    fn complete_school_exam(&mut self,result:u8,events:&mut Vec<Value>)->Result<(),String>{
        let stage=self.state.legacy_player.as_ref().unwrap().life().stage;
        if stage==3 && result>0{self.state.study_origin=Some(MapPosition{map_id:self.state.map_id.clone().unwrap(),tile_id:self.state.tile_id.unwrap(),facing:self.state.map_facing});}
        if result==2{self.state.legacy_player.as_mut().unwrap().set_i16(0x4b64b8,1)?;}
        let next=match result{0=>6,1=>5,2=>4,_=>return Err("Invalid original exam result".into())};
        self.transition_stage(next,events)?;
        if next==4{if let Some(origin)=&self.state.study_origin{self.state.map_id=Some(origin.map_id.clone());self.state.tile_id=Some(origin.tile_id);self.state.entry_step_pending=true;self.state.map_facing=origin.facing;}}
        if self.state.host_activity.is_none(){self.refresh_spirit_positions(events)?;self.state.legacy_player.as_mut().unwrap().update_skills()?;self.sync_player();self.state.phase="playing".into();}
        Ok(())
    }

    fn status_overview(&self)->Result<Value,String>{crate::status::overview(self.state.legacy_player.as_ref().ok_or("Missing status player")?,&self.state.housing,self.state.spouse_index,self.state.active_mission.as_ref().map(|mission|mission.index),self.rules.as_ref().ok_or("Missing status tables")?)}
    fn open_status(&mut self)->String{
        if self.state.phase!="playing"||self.state.host_activity.is_some()||self.state.pending.is_some(){return self.response(Some(("wrong_phase","请先完成当前事件。".into())),vec![]);}
        if let Err(error)=self.status_overview(){return self.response(Some(("status_data",error)),vec![]);}
        self.state.host_activity=Some(HostActivity::Status{panel:StatusPanel::default(),continuation:None});self.state.revision+=1;self.refresh_host_activity();self.response(None,vec![])
    }
    fn begin_property_preview(&mut self,page:StatusPage,continuation:StatusContinuation)->Result<(),String>{
        let overview=self.status_overview()?;let mut panel=StatusPanel::default();
        panel.page(page,&overview,&mut self.state.rng)?;
        self.state.host_activity=Some(HostActivity::Status{panel,continuation:Some(continuation)});self.refresh_host_activity();Ok(())
    }
    fn resume_status(&mut self,elapsed_ms:Option<u32>,action:Option<&str>,page:Option<&str>,delta:i32,index:Option<usize>)->String{
        let Some(HostActivity::Status{mut panel,continuation})=self.state.host_activity.clone()else{return self.response(Some(("wrong_phase","当前没有打开参数页面。".into())),vec![]);};
        let mut closed=false;let result:Result<(),String>=(||{
            if let Some(elapsed)=elapsed_ms{panel.tick(elapsed,&mut self.state.rng);closed=continuation.is_some()&&panel.page==StatusPage::Overview;return Ok(());}
            if panel.transition.is_some(){return Err("请等待住宅转场结束。".into());}
            match action{
                Some("close")=>{closed=panel.close()||continuation.is_some();},
                Some("page")=>{
                    if continuation.is_some(){return Err("请先关闭当前房产预览。".into());}
                    let overview=self.status_overview()?;panel.page(StatusPage::parse(page.ok_or("请选择参数页面。")?)?,&overview,&mut self.state.rng)?;
                },
                Some("scroll")=>{panel.scroll(delta,index,self.state.career_history.len())?;},
                _=>return Err("未知的参数页面操作。".into())
            }Ok(())
        })();
        if let Err(error)=result{return self.response(Some(("status_input",error)),vec![]);}
        let mut events=Vec::new();self.state.revision+=1;
        if closed{
            self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});
            match continuation{
                Some(StatusContinuation::NativeShop{shop,bonus})=>{
                    if let Some((address,value))=bonus{self.state.legacy_player.as_mut().unwrap().set_i16(address,value).unwrap();self.sync_player();}
                    self.state.host_activity=Some(HostActivity::NativeShop{shop});self.refresh_host_activity();
                },
                Some(StatusContinuation::NativeReturn)=>{
                    if let Some(vm)=self.state.vm.as_mut(){if let Err(error)=vm.resume(VmInput::HostReturned{next_node:None}){self.block("native_resume",error,Value::Null);}else{self.advance_vm(&mut events);}}
                    else{self.state.phase="playing".into();}
                },
                None=>self.state.phase="playing".into(),
            }
        }else{self.state.host_activity=Some(HostActivity::Status{panel,continuation});self.refresh_host_activity();}
        self.response(None,events)
    }
    fn inventory_rules(&self)->Result<InventoryRules,String>{serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("inventory")).ok_or("Missing original inventory tables")?.clone()).map_err(|error|error.to_string())}
    fn open_inventory(&mut self,tab:Option<&str>)->String{
        if self.state.phase!="playing"||self.state.host_activity.is_some()||self.state.pending.is_some(){return self.response(Some(("wrong_phase","请先完成当前事件。".into())),vec![]);}
        let tab=match InventoryTab::parse(tab.unwrap_or("ordinary")){Ok(tab)=>tab,Err(error)=>return self.response(Some(("inventory_tab",error)),vec![])};
        if let Err(error)=self.inventory_rules(){return self.response(Some(("inventory_rules",error)),vec![]);}
        if self.state.battle_persistent.is_none(){let rules=match self.battle_rules(){Ok(rules)=>rules,Err(error)=>return self.response(Some(("inventory_rules",error)),vec![])};self.state.battle_persistent=Some(BattlePersistent::new(&rules));}
        let panel=InventoryPanel::new(tab,&self.state.inventory.ordinary,&self.state.inventory.special,&self.state.carried_equipment,&self.state.battle_persistent.as_ref().unwrap().equipped_ids);
        self.state.host_activity=Some(HostActivity::Inventory{panel});self.state.revision+=1;self.refresh_host_activity();self.response(None,vec![])
    }
    fn resume_inventory(&mut self,elapsed_ms:Option<u32>,action:Option<&str>,index:Option<usize>,tab:Option<&str>)->String{
        let Some(HostActivity::Inventory{mut panel})=self.state.host_activity.clone()else{return self.response(Some(("wrong_phase","当前没有打开物品栏。".into())),vec![]);};
        let rules=match self.inventory_rules(){Ok(rules)=>rules,Err(error)=>return self.response(Some(("inventory_rules",error)),vec![])};
        if panel.confirm_drop&&elapsed_ms.is_none()&&!matches!(action,Some("confirmDrop"|"cancelDrop")){return self.response(Some(("inventory_confirm","请先回答是否丢弃这件物品。".into())),vec![]);}
        let mut events=vec![];let mut rebuild=false;
        let result:Result<(),String>=(||{
            if let Some(elapsed)=elapsed_ms{panel.tick(elapsed,&rules);return Ok(());}
            match action{
                Some("close")=>{},
                Some("tab")=>{let tab=InventoryTab::parse(tab.ok_or("请选择物品分类。")?)?;let clock=panel.clock_ms;panel=InventoryPanel::new(tab,&self.state.inventory.ordinary,&self.state.inventory.special,&self.state.carried_equipment,&self.state.battle_persistent.as_ref().ok_or("Missing equipment state")?.equipped_ids);panel.clock_ms=clock;},
                Some("select")=>{let selected=index.filter(|index|*index<panel.entries.len()).ok_or("这件物品不存在。")?;panel.selected_index=Some(selected);},
                Some("drop")=>{panel.selected_id()?;panel.confirm_drop=true;},
                Some("cancelDrop")=>{if !panel.confirm_drop{return Err("当前没有待确认的丢弃操作。".into());}panel.confirm_drop=false;},
                Some("confirmDrop")=>{
                    if !panel.confirm_drop{return Err("请先选择要丢弃的物品。".into());}let id=panel.selected_id()?;
                    match panel.tab{InventoryTab::Ordinary=>inventory::drop_ordinary(id,&mut self.state.inventory.ordinary)?,InventoryTab::Special=>inventory::drop_special(id,&mut self.state.inventory.special)?,InventoryTab::Equipment=>inventory::drop_equipment(id,&mut self.state.known_equipment,&mut self.state.carried_equipment,&mut self.state.battle_persistent.as_mut().ok_or("Missing equipment state")?.equipped_ids)?,}
                    panel.confirm_drop=false;panel.preview_origin=crate::inventory_view::PreviewOrigin{x:if panel.tab==InventoryTab::Equipment{312}else{302},y:47};rebuild=true;events.push(json!({"type":"inventoryChanged","action":"drop","tab":panel.tab,"id":id}));
                },
                Some("use")=>{if panel.tab!=InventoryTab::Ordinary{return Err("这种物品不能直接使用。".into());}let id=panel.selected_id()?;let player=self.state.legacy_player.as_mut().ok_or("Missing inventory player")?;let mut hp=player.i16(0x4b6486)?;let max_hp=player.i16(0x4b6484)?;inventory::use_ordinary(id,&mut self.state.inventory.ordinary,&mut hp,max_hp,&rules)?;player.set_i16(0x4b6486,hp)?;panel.preview_origin=crate::inventory_view::PreviewOrigin{x:294,y:40};rebuild=true;events.push(json!({"type":"inventoryChanged","action":"use","id":id,"hp":hp}));},
                Some("wear"|"unequip")=>{if panel.tab!=InventoryTab::Equipment{return Err("请先选择装备。".into());}let id=panel.selected_id()?;let equipped=&mut self.state.battle_persistent.as_mut().ok_or("Missing equipment state")?.equipped_ids;
                    if action==Some("wear"){inventory::wear_equipment(id,&self.state.carried_equipment,equipped)?;}else{inventory::unequip_equipment(id,&self.state.carried_equipment,equipped)?;}
                    for entry in &mut panel.entries{entry.equipped=equipped.get(entry.id/3)==Some(&(entry.id as i16));}events.push(json!({"type":"inventoryChanged","action":action,"id":id}));
                },
                _=>return Err("未知的物品栏操作。".into()),
            }Ok(())
        })();
        if let Err(error)=result{return self.response(Some(("inventory_input",error)),vec![]);}
        if rebuild{panel.rebuild(&self.state.inventory.ordinary,&self.state.inventory.special,&self.state.carried_equipment,&self.state.battle_persistent.as_ref().unwrap().equipped_ids);}
        self.state.revision+=1;self.sync_player();
        if action==Some("close"){self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});self.state.phase="playing".into();}else{self.state.host_activity=Some(HostActivity::Inventory{panel});self.refresh_host_activity();}
        self.response(None,events)
    }
    fn battle_rules(&self)->Result<BattleRules,String>{serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("battle")).ok_or("Missing original combat tables")?.clone()).map_err(|error|error.to_string())}
    fn final_battle_rules(&self)->Result<FinalBattleRules,String>{serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("finalBattle")).ok_or("Missing original final-combat tables")?.clone()).map_err(|error|error.to_string())}

    fn begin_battle(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        let rules=self.battle_rules()?;let persistent=self.state.battle_persistent.clone().unwrap_or_else(||BattlePersistent::new(&rules));
        let player=self.state.legacy_player.as_ref().ok_or("Missing combat player")?;let life=player.life();
        let input=BattleInput{hp:player.i16(0x4b6486)?,max_hp:player.i16(0x4b6484)?,experience:player.i16(0x4b6488)?,money:player.i32(0x4b648a)?,sex:player.i16(0x4b6498)?as usize,stage:life.stage,career:life.profession,career_level:life.profession_level,life_level:life.life_level,
            stamina:player.i16(0x4b648e)?,intelligence:player.i16(0x4b6492)?,morality:player.i16(0x4b6490)?,charm:player.i16(0x4b6494)?,mission_kind:player.i16(0x4b64ce)?,mission_target:player.i16(0x4b64cc)?,god:player.i16(0x4b64d6)?,traveling:life.traveling,invulnerable:false,equipped:persistent.equipped_ids.clone(),carried:self.state.carried_equipment.clone()};
        let game=BattleGame::new(input,persistent,&rules,&mut self.state.rng)?;
        if game.input.hp>0&&game.mission_started{self.state.legacy_player.as_mut().unwrap().set_i16(0x4b64d0,1)?;}
        if game.phase==BattlePhase::Finished {self.finish_battle(game,events)?;}else{self.state.host_activity=Some(HostActivity::Battle{game});self.refresh_host_activity();}Ok(())
    }
    fn finish_battle(&mut self,game:BattleGame,events:&mut Vec<Value>)->Result<(),String>{
        let result=game.return_code.ok_or("Original combat has not returned")?;
        let player=self.state.legacy_player.as_mut().ok_or("Missing combat player")?;
        player.set_i16(0x4b6486,game.hp)?;player.set_i16(0x4b6488,game.experience as i16)?;player.set_i32(0x4b648a,game.money)?;
        player.add_i16(0x4b64bc,1)?;if result==1{player.add_i16(0x4b64be,1)?;player.add_i16(0x4b64f0,1)?;}
        self.state.battle_persistent=Some(game.persistent);self.state.host_activity=None;self.state.pending=None;
        events.push(json!({"type":"battleCompleted","enemy":game.enemy,"result":result,"hp":game.hp,"money":game.money,"experience":game.experience}));
        if game.hp>0{self.trigger_skills(0,events)?;}
        self.state.vm.as_mut().ok_or("Missing combat event continuation")?.resume(VmInput::HostReturned{next_node:None})?;self.sync_player();self.advance_vm(events);Ok(())
    }
    /// The fourteen native liquidation rows (441860). All products and sums
    /// wrap at their original signed 32-bit width before division.
    fn final_battle_input(&self)->Result<FinalBattleInput,String>{
        let player=self.state.legacy_player.as_ref().ok_or("Missing final combat player")?;let life=player.life();let rules=self.rules.as_ref().ok_or("Missing original rules")?;let final_rules=&rules["finalBattle"];
        let housing:HousingCatalog=serde_json::from_value(rules["housing"].clone()).map_err(|error|error.to_string())?;
        let salary=if life.profession<0{0}else{rules["baseSalaries"].get(life.profession as usize).and_then(Value::as_i64).ok_or("Missing final career salary")?as i32};
        let mut home=0i32;if self.state.housing.style>=0{let style=self.state.housing.style as usize;home=housing.styles.get(style).ok_or("Invalid final house style")?.price;
            for index in 0..self.state.housing.furniture.max(0)as usize{home=home.wrapping_add(final_rules["furnitureValues"][style][index].as_i64().ok_or("Missing final furniture value")?as i32);}}
        let progress=self.state.housing.investment_stage.saturating_sub(1)as usize;let mut investments=0i32;
        for row in housing.investment_rows.iter().take(progress/3){for value in &row[..3]{investments=investments.wrapping_add(*value);}}
        if progress%3>0{let row=housing.investment_rows.get(progress/3).ok_or("Invalid final investment stage")?;for value in row.iter().take(progress%3){investments=investments.wrapping_add(*value);}}
        let spouse=if self.state.spouse_index<0{0}else{final_rules["spouseValues"][self.state.spouse_index as usize].as_i64().ok_or("Missing final spouse value")?as i32};
        let wins=self.state.battle_persistent.as_ref().map_or(0,|state|state.enemies.iter().fold(0i32,|sum,enemy|sum.wrapping_add(i32::from(enemy.wins))));
        let valuation=[player.i32(0x4b648a)?,salary.wrapping_mul(3),home.wrapping_mul(2)/10,investments.wrapping_mul(2)/10,spouse/10,
            i32::from(life.profession_level)*1000,i32::from(life.life_level)*80,i32::from(player.i16(0x4b6492)?)*10,i32::from(player.i16(0x4b6490)?)*10,i32::from(player.i16(0x4b6494)?)*10,i32::from(player.i16(0x4b648e)?)*10,wins.wrapping_mul(500),
            i32::from(player.i16(0x4b64f2)?),i32::from(player.i16(0x4b64e2)?)/2+i32::from(player.i16(0x4b64d4)?)/2];
        Ok(FinalBattleInput{sex:player.i16(0x4b6498)?as usize,career:life.profession,life_level:life.life_level,equipped_count:self.state.battle_persistent.as_ref().map_or(0,|state|state.equipped_ids.iter().filter(|&&id|id!=-1).count()as i16),valuation})
    }
    fn begin_final_battle(&mut self)->Result<(),String>{
        let rules=self.final_battle_rules()?;self.state.legacy_player.as_mut().ok_or("Missing final combat player")?.update_skills()?;
        let game=FinalBattleGame::new(self.final_battle_input()?,&rules)?;self.state.legacy_player.as_mut().unwrap().set_i32(0x4b648a,game.money)?;
        self.state.host_activity=Some(HostActivity::FinalBattle{game});self.sync_player();self.refresh_host_activity();Ok(())
    }
    fn resume_battle(&mut self,elapsed_ms:Option<u32>,action:Option<&str>,index:Option<usize>)->String{
        let Some(mut activity)=self.state.host_activity.clone()else{return self.response(Some(("wrong_phase","当前没有正在进行的战斗。".into())),vec![]);};
        let result=match &mut activity{
            HostActivity::Battle{game}=>{if let Some(elapsed)=elapsed_ms{game.tick(elapsed,&mut self.state.rng);Ok(())}else{match action{
                Some("continue"|"start")=>game.start(),Some("selectCard")=>index.ok_or("请选择卡片。".into()).and_then(|index|game.select_card(index)),Some("hoverCard")=>game.hover_card(index),Some("attack")=>game.attack(),Some("help")=>game.show_help(),Some("finishRewardAnimation")=>{game.skip_reward();Ok(())},_=>Err("未知的战斗操作。".into()),
            }}},
            HostActivity::FinalBattle{game}=>match self.final_battle_rules(){Ok(rules)=>{if let Some(elapsed)=elapsed_ms{game.tick(elapsed,&rules,&mut self.state.rng)}else{game.action(action.unwrap_or(""),index,&rules)}},Err(error)=>Err(error)},
            _=>Err("当前没有正在进行的战斗。".into()),
        };
        if let Err(error)=result{return self.response(Some(("battle_input",error)),vec![]);}
        let mut events=vec![];self.state.revision+=1;
        match activity{
            HostActivity::Battle{game}if game.phase==BattlePhase::Finished=>{if let Err(error)=self.finish_battle(game,&mut events){self.block("battle_continuation",error,Value::Null);}},
            HostActivity::FinalBattle{game}=>{self.state.legacy_player.as_mut().unwrap().set_i32(0x4b648a,game.money).unwrap();if game.finished{if let Err(error)=self.complete_final_battle(game.hp>0,&mut events){self.block("final_battle_continuation",error,Value::Null);}}else{self.state.host_activity=Some(HostActivity::FinalBattle{game});self.refresh_host_activity();}},
            activity=>{self.state.host_activity=Some(activity);self.refresh_host_activity();},
        }
        self.sync_player();self.response(None,events)
    }

    /// Continuation after FUN_00443b40. Defeat reincarnates after the ending;
    /// victory adds the original FUN_0041c040 yes/no choice.
    pub fn complete_final_battle(&mut self,victory:bool,events:&mut Vec<Value>)->Result<(),String>{
        self.state.legacy_player.as_mut().ok_or("Missing final-battle player")?.update_skills()?;
        let rules:EndingSummaryRules=serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("endingSummary")).ok_or("Missing original ending-summary tables")?.clone()).map_err(|error|error.to_string())?;
        let player=self.state.legacy_player.as_ref().unwrap();
        let stage=player.life().stage;let career=player.i16(0x4b64a2)?;
        // The native constructor returns false outside stages 5/6, or when an
        // unemployed player's edm00/eum00 resources cannot be loaded. Its caller
        // proceeds directly to the movie, rather than blocking the ending.
        let summary=if stage==5||(stage==6&&(0..26).contains(&career)){Some(EndingSummary::new(victory,player.i16(0x4b6498)?as usize,stage,career,&self.ending_history(),&rules)?)}else{None};
        let epilogue:EpilogueRules=serde_json::from_value(self.rules.as_ref().and_then(|rules|rules.get("epilogue")).ok_or("Missing original epilogue tables")?.clone()).map_err(|error|error.to_string())?;
        self.state.vm=None;self.state.pending_allocation=None;self.state.action_queue.clear();self.state.turn_in_progress=false;
        self.state.host_activity=Some(HostActivity::Ending{flow:EndingFlow::new(victory,summary,&epilogue)});
        self.sync_player();
        events.push(json!({"type":"lifeCompleted","victory":victory,"stats":self.state.stats,"life":self.state.legacy_player.as_ref().unwrap().life()}));
        self.refresh_host_activity();Ok(())
    }

    /// FUN_00414830 / FUN_0041b0c0 initialize the two school-history entries.
    fn initial_life_history()->[Vec<i32>;16]{let mut history=std::array::from_fn(|_|Vec::new());history[1].push(2016);history[2].push(2019);history}
    /// FUN_004159a0 records these values before entering the final battle.
    pub fn prepare_final_history(&mut self)->Result<(),String>{
        let player=self.state.legacy_player.as_ref().ok_or("Missing final-battle player")?;
        let history=self.state.life_history.get_or_insert_with(Self::initial_life_history);
        history[14].push(i32::from(player.i16(0x4b64f0)?).min(99));history[15].push(i32::from(player.life().year));Ok(())
    }
    fn ending_history(&self)->[Vec<i32>;16]{
        let mut history=self.state.life_history.clone().unwrap_or_else(Self::initial_life_history);
        history[7]=self.state.promotion_years.iter().copied().map(i32::from).collect();history[8]=self.state.housing.purchase_years.iter().copied().map(i32::from).collect();
        for(index,year)in [11,12,10].into_iter().zip(self.state.relationship_years){history[index]=year.into_iter().map(i32::from).collect();}history
    }

    /// State outside the packed character that FUN_0041b0c0 resets. Rank-five
    /// careers and fully mastered skills survive; belongings and history do not.
    fn reincarnate(&mut self,events:&mut Vec<Value>)->Result<(),String>{
        self.state.legacy_player.as_mut().ok_or("Missing reincarnation player")?.reincarnate()?;
        for rank in &mut self.state.career_levels{if *rank<5{*rank=1;}}
        self.state.inventory=Inventory::default();self.state.career_history.clear();self.state.promotion_years.clear();
        self.state.shop_visits=0;self.state.spouse_index=-1;self.state.relationship_candidates.fill(-1);self.state.gift_level=1;self.state.fortune_visits=0;
        self.state.relationship_new_targets.fill(-1);self.state.dating_target=-1;self.state.relationship_years=[None;3];
        self.state.known_equipment.fill(EquipmentKnowledge{learned:false,level:0});self.state.carried_equipment.fill(EquipmentSlot{id:-1,level:0});self.state.first_equipment_visit=true;
        self.state.housing=HousingState::default();self.state.active_mission=None;self.state.active_condition=None;
        self.state.battle_persistent=None;
        self.state.life_history=Some(Self::initial_life_history());
        self.state.pending_allocation=None;self.state.host_activity=None;self.state.pending=None;self.state.vm=None;
        self.state.travel_origin=None;self.state.study_origin=None;self.state.action_queue.clear();self.state.queued_die=None;self.state.turn_in_progress=false;
        self.state.spirits.active=None;self.state.spirits.expires_turn=0;self.state.last_roll_steps=0;
        self.transition_stage(2,events)?;
        self.refresh_spirit_positions(events)?;
        self.state.legacy_player.as_mut().unwrap().set_i16(0x4b64d6,-1)?;
        self.state.legacy_player.as_mut().unwrap().update_skills()?;self.sync_player();
        self.state.phase="playing".into();self.state.message="新的人生开始了。".into();
        events.push(json!({"type":"reincarnated","incarnation":self.state.legacy_player.as_ref().unwrap().life().incarnation}));Ok(())
    }

    fn resume_ending(&mut self,elapsed_ms:Option<u32>,action:Option<&str>,index:Option<usize>)->String{
        let Some(HostActivity::Ending{mut flow})=self.state.host_activity.clone()else{return self.response(Some(("wrong_phase","当前没有正在播放的人生结局。".into())),vec![]);};
        let mut events=Vec::new();
        if flow.phase==EndingPhase::Reincarnation && action==Some("choose"){
            match index{Some(0)=>{if let Err(error)=self.reincarnate(&mut events){self.block("reincarnation",error,Value::Null);}},Some(1)=>{
                self.state.host_activity=None;self.state.pending=None;self.state.phase="completed".into();self.state.message="这段人生已经圆满结束。".into();
                events.push(json!({"type":"gameCompleted"}));
            },_=>return self.response(Some(("ending_input","请选择继续新的人生或结束游戏。".into())),vec![])}
        }else{
            if let Some(elapsed_ms)=elapsed_ms{
                let rules=self.rules.as_ref().and_then(|rules|rules.get("epilogue")).ok_or("Missing original epilogue tables").and_then(|value|serde_json::from_value::<EpilogueRules>(value.clone()).map_err(|_|"Invalid original epilogue tables"));
                let rules=match rules{Ok(rules)=>rules,Err(error)=>return self.response(Some(("ending_rules",error.into())),vec![])};
                flow.tick(elapsed_ms,&rules);
            }else if let Some(action)=action{if let Err(error)=flow.action(action){return self.response(Some(("ending_input",error)),vec![]);}}
            if flow.phase==EndingPhase::Finished{if let Err(error)=self.reincarnate(&mut events){self.block("reincarnation",error,Value::Null);}}
            else{self.state.host_activity=Some(HostActivity::Ending{flow});self.refresh_host_activity();}
        }
        self.state.revision+=1;self.response(None,events)
    }

    fn begin_mission(&mut self)->Result<(),String>{
        if let Some(mission)=&mut self.state.active_mission{
            mission.multiplier=mission.multiplier.wrapping_add(1);
            self.state.scene=json!({"backgroundResource":"install/bmp/money.bmp"});
            return self.show_notice(vec![24999],NoticeContinuation::Native);
        }
        let player=self.state.legacy_player.as_mut().unwrap();
        let selected=select_mission(&|address|if address==0x4b64da{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)},&mut ||self.state.rng.next_rand())?;
        for write in selected.writes{player.set_i16(write.address,write.value as i16)?;}
        let Some(index)=selected.mission_index else{return self.show_notice(vec![25014],NoticeContinuation::Native);};
        let record=self.rules.as_ref().and_then(|rules|rules["missions"].as_array()).and_then(|missions|missions.get(index as usize)).ok_or("Missing original mission table")?;
        let words:Vec<i16>=serde_json::from_value(record["words"].clone()).map_err(|error|error.to_string())?;
        if words.len()!=11{return Err("Original mission must contain eleven words".into());}
        let mut pages=Vec::new();
        if player.i16(0x4b64de)?==1{player.set_i16(0x4b64de,0)?;pages.extend([25000,25001,25002]);}
        else if player.i16(0x4b64d4)?>0 && player.i16(0x4b64d4)?%5==0 && player.i32(0x4b64ec)?==1{
            pages.push(i32::from(player.i16(0x4b64d4)?)/5+25002);player.set_i32(0x4b64ec,0)?;
        }
        pages.extend((0..words[3].max(1)).map(|offset|i32::from(words[0])+i32::from(offset)));
        self.state.scene=json!({"backgroundResource":self.program.as_ref().unwrap().background_resource(i32::from(words[9])),"patternResource":"install/bmp/missionbg.bmp"});
        self.show_notice(pages,NoticeContinuation::AcceptMission{mission:ActiveMission{index,words,multiplier:1}})
    }

    fn resolve_mission(&mut self,success:bool,events:&mut Vec<Value>)->Result<(),String>{
        let mission=self.state.active_mission.as_ref().ok_or("Condition has no original mission record")?;
        let player=self.state.legacy_player.as_ref().unwrap();let inventory=&self.state.inventory;
        let effects=self.program.as_ref().unwrap().mission_result_effects(i32::from(mission.words[if success{7}else{8}]),mission.multiplier,&|address|inventory.read(address).or_else(||if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)}))?;
        let player=self.state.legacy_player.as_mut().unwrap();
        for write in &effects.writes{
            if !self.state.inventory.write(write.address,write.value as i16){if write.address==0x4b648a{player.set_i32(write.address,write.value)?;}else{player.set_i16(write.address,write.value as i16)?;}}
        }
        if success{player.add_i16(0x4b64d4,1)?;}
        player.set_i32(0x4b64ec,i32::from(success))?;player.set_i32(0x4b64d0,0)?;player.set_i16(0x4b64cc,-1)?;player.set_i16(0x4b64ce,-1)?;
        events.push(json!({"type":"missionResolved","missionIndex":mission.index,"success":success,"effects":effects}));
        self.state.active_mission=None;self.state.active_condition=None;self.sync_player();Ok(())
    }

    fn refresh_host_activity(&mut self){
        match self.state.host_activity.as_ref(){
            Some(HostActivity::StatShop{shop})=>{
                let mut options=shop.offers.iter().enumerate().map(|(index,offer)|{
                    let benefit=if offer.group==4 && offer.item.amount<=0{"鲜花礼物".to_string()}else{format!("{} +{}",["魅力","智力","体能","道德","爱情"][offer.group],offer.item.amount)};
                    json!({"index":index,"text":format!("{} · {}元 · {}{}",offer.item.name,offer.item.price,benefit,if offer.bought{"（已购买）"}else{""}),"disabled":offer.bought||shop.purchases>=3})
                }).collect::<Vec<_>>();
                options.push(json!({"index":shop.offers.len(),"text":"离开商店"}));
                self.state.pending=Some(json!({"kind":"choice","text":format!("属性商店 · 本次已购买 {}/3 件",shop.purchases),"options":options}));
                self.state.scene=json!({"backgroundResource":"pm0g/pm0g01.bmp"});self.state.phase="event".into();
            }
            Some(HostActivity::Career{careers,..})=>{
                let player=self.state.legacy_player.as_ref().unwrap();
                let mut options=careers.iter().map(|career|json!({"index":career.id,"text":career.name,"disabled":!career.eligible(player,&self.state.career_levels).unwrap_or(false)})).collect::<Vec<_>>();
                options.push(json!({"index":24,"text":"离开人才市场"}));
                self.state.pending=Some(json!({"kind":"choice","text":"选择职业","options":options}));self.state.phase="event".into();
            }
            Some(HostActivity::Notice{pages,page,..})=>{
                let text=split_dialogue(self.program.as_ref().unwrap().dialogue(pages[*page]).unwrap());
                self.state.pending=Some(json!({"kind":"dialogue","text":text.text,"dialogueId":pages[*page],"speaker":Value::Null}));self.state.phase="event".into();
            }
            Some(HostActivity::SchoolExam{exam})=>{
                if !exam.started{
                    self.state.pending=Some(json!({"kind":"choice","text":"高中毕业考试：共十题，每题十分。是否参加？","options":[{"index":0,"text":"参加考试"},{"index":1,"text":"放弃考试，进入社会"}]}));
                }else{
                    self.state.pending=Some(json!({"kind":"choice","text":format!("第 {}/10 题 · {} 分\n{}",exam.answered+1,exam.score,exam.current.question),"options":exam.current.options.iter().enumerate().map(|(index,text)|json!({"index":index,"text":text})).collect::<Vec<_>>()}));
                }
                self.state.phase="event".into();
            }
            Some(HostActivity::Tracking{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"tracking","state":game,"text":"记住目标，追踪它移动后的位置。"}));
                self.state.phase="event".into();
            }
            Some(HostActivity::Hammer{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"hammer","state":game,"text":"击中普通目标或铠甲目标，避开友好目标。"}));
                self.state.phase="event".into();
            }
            Some(HostActivity::Fist{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"fist","state":game,"text":"左右移动，按空格出拳；连续命中可以获得更多分数。"}));
                self.state.phase="event".into();
            }
            Some(HostActivity::Gas{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"gas","state":game,"text":"狗熊捕鱼：看准大鱼的位置，按空格或回车捕鱼。共有三次机会。"}));
                self.state.phase="event".into();
            }
            Some(HostActivity::Firebee{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"firebee","state":game,"text":"用左右方向键移动，按空格射击。按 F1 查看说明。"}));self.state.phase="event".into();
            }
            Some(HostActivity::Fire{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"fire","state":game,"text":"交替按左右键，赶在时间耗尽前生起火。"}));
                self.state.phase="event".into();
            }
            Some(HostActivity::Ending{flow})=>{
                self.state.pending=Some(json!({"kind":"ending","state":flow,"text":"人生回顾"}));self.state.phase="event".into();self.state.scene=json!({});
            }
            Some(HostActivity::Battle{game})=>{self.state.pending=Some(json!({"kind":"battle","game":"ordinary","state":game,"text":game.text}));self.state.phase="event".into();self.state.scene=json!({});}
            Some(HostActivity::FinalBattle{game})=>{self.state.pending=Some(json!({"kind":"battle","game":"final","state":game,"text":"最终战斗"}));self.state.phase="event".into();self.state.scene=json!({});}
            Some(HostActivity::Inventory{panel})=>{self.state.pending=Some(json!({"kind":"inventory","state":panel,"ordinary":self.state.inventory.ordinary,"special":self.state.inventory.special,"carried":self.state.carried_equipment,"equipped":self.state.battle_persistent.as_ref().map(|persistent|&persistent.equipped_ids),"rules":self.rules.as_ref().and_then(|rules|rules.get("inventory"))}));self.state.phase="event".into();self.state.scene=json!({});}
            Some(HostActivity::Status{panel,continuation})=>{self.state.pending=Some(json!({"kind":"status","state":panel,"overview":self.status_overview().ok(),"housing":self.state.housing,"returnTo":match continuation{Some(StatusContinuation::NativeShop{..})=>"shop",Some(StatusContinuation::NativeReturn)=>"housing",None=>"overview"}}));self.state.phase="event".into();self.state.scene=json!({});}
            Some(HostActivity::Relationship{house})=>{
                self.state.pending=Some(json!({"kind":"relationship","state":house,"question":house.question().ok(),"text":house.message}));
                self.state.scene=json!({});self.state.phase="event".into();
            }
            Some(HostActivity::SpiritEncounter{spirit})=>{
                self.state.pending=Some(json!({"kind":"choice","text":"神灵将与你同行。","options":[{"index":0,"text":"继续"}],"spirit":spirit}));self.state.phase="event".into();
                self.state.scene=json!({"backgroundResource":format!("cd/data/spirit/sen_{:02}.bmp",[3,2,1,4,5][*spirit])});
            }
            Some(HostActivity::SpiritLottery{game})=>{
                self.state.pending=Some(json!({"kind":"minigame","game":"spiritSlots","state":game,"text":"停止转盘，查看神灵的影响。"}));self.state.phase="event".into();self.state.scene=json!({});
            }
            Some(HostActivity::Fortune{game})=>{
                let (text,options)=if game.phase==FortunePhase::Question{
                    let question=&game.record.questions[game.question_index];
                    let mut options=question.options.iter().enumerate().map(|(index,text)|json!({"index":index,"text":text})).collect::<Vec<_>>();
                    options.push(json!({"index":options.len(),"text":"离开测验"}));
                    (format!("{} · {}/{}\n{}",game.record.title,game.question_index+1,game.record.questions.len(),question.prompt),options)
                }else{(game.explanation.clone(),vec![json!({"index":0,"text":"继续"}),json!({"index":1,"text":"离开测验"})])};
                self.state.pending=Some(json!({"kind":"choice","text":text,"options":options}));self.state.phase="event".into();
            }
            Some(HostActivity::EquipmentWorkshop{workshop,definitions})=>{
                let(text,options)=if let Some(outcome)=workshop.outcome{
                    (match outcome{EquipmentOutcome::Purchased=>"装备购买成功，请到道具商店领取后携带。",EquipmentOutcome::Upgraded=>"装备研究完成。",EquipmentOutcome::InsufficientMoney=>"持有金不足。",EquipmentOutcome::CapacityReached=>"拥有的战斗装备已达八件。",EquipmentOutcome::MaximumLevel=>"这件装备已达到最高等级。"}.to_owned(),vec![json!({"index":0,"text":"继续"})])
                }else{match workshop.mode.as_deref(){
                    Some("purchase")=>{
                        let item=definitions.iter().find(|item|item.id==workshop.offered_id).unwrap();
                        (format!("{} · {}元\n{}",item.name,item.price,item.description),vec![json!({"index":0,"text":"购买"}),json!({"index":1,"text":"离开"})])
                    }
                    Some("research")=>{
                        let mut options=self.state.carried_equipment.iter().filter(|slot|slot.id>=0).filter_map(|slot|definitions.iter().find(|item|item.id==slot.id as usize).map(|item|json!({"index":item.id,"text":format!("{} · 研究费用 {}元",item.name,item.price/2)}))).collect::<Vec<_>>();
                        options.push(json!({"index":18,"text":"离开"}));("选择要研究的装备".into(),options)
                    }
                    _=>("装备购买与研究".into(),vec![json!({"index":0,"text":"购买装备"}),json!({"index":1,"text":"研究装备"}),json!({"index":2,"text":"离开"})]),
                }};
                self.state.pending=Some(json!({"kind":"choice","text":text,"options":options}));self.state.phase="event".into();
            }
            Some(HostActivity::NativeShop{shop})=>{
                let mut options=shop.offers.iter().enumerate().filter(|(_,offer)|!offer.purchased).map(|(index,offer)|json!({"index":index,"text":format!("{} · {}元\n{}",["家具","战斗装备领取","普通道具","职业道具"][offer.category as usize],offer.price,offer.description)})).collect::<Vec<_>>();
                options.push(json!({"index":shop.offers.len(),"text":"离开道具商店"}));
                self.state.pending=Some(json!({"kind":"choice","text":"道具商店","options":options}));self.state.phase="event".into();
            }
            Some(HostActivity::Housing{catalog,result,..})=>{
                let(text,options)=if let Some(result)=result{(result.clone(),vec![json!({"index":0,"text":"继续"})])}
                else if self.state.housing.investment_stage==0{
                    let mut options=catalog.styles.iter().map(|style|json!({"index":style.id,"text":format!("{}元 · {}",style.price,style.description)})).collect::<Vec<_>>();
                    options.push(json!({"index":4,"text":"离开"}));("选择住宅".into(),options)
                }else{
                    let mut options=Vec::new();
                    if let Ok(price)=self.state.housing.next_investment(catalog){options.push(json!({"index":5,"text":format!("投资 {}元",price)}));}
                    if let Ok(income)=self.state.housing.income(catalog){options.push(json!({"index":6,"text":format!("领取房产收益 {}元",income)}));}
                    options.push(json!({"index":7,"text":"离开"}));
                    (catalog.stage_descriptions.get(usize::from(self.state.housing.investment_stage-1)).cloned().unwrap_or_default(),options)
                };
                self.state.pending=Some(json!({"kind":"choice","text":text,"options":options}));self.state.phase="event".into();
            }
            Some(HostActivity::Competition{game,..})=>{
                let choices:Vec<Value>=match game.phase{
                    CompetitionPhase::Menu=>["造型大赛","飞人大赛","智力大赛"].iter().enumerate().map(|(index,text)|json!({"index":index,"text":text})).collect(),
                    CompetitionPhase::Intro|CompetitionPhase::QuizIntro|CompetitionPhase::QuizResult=>vec![json!({"index":0,"text":"继续"})],
                    CompetitionPhase::QuizQuestion=>game.exam.as_ref().unwrap().current.options.iter().enumerate().map(|(index,text)|json!({"index":index,"text":text})).collect(),
                    _=>vec![],
                };
                self.state.pending=Some(json!({"kind":"competition","state":game,"text":game.text(),"choices":choices}));self.state.phase="event".into();
            },
            None=>{}
        }
    }

    fn resume_host_activity(&mut self,input:VmInput)->String{
        if matches!(self.state.host_activity,Some(HostActivity::Battle{..}|HostActivity::FinalBattle{..})){return self.resume_battle(None,Some("continue"),None);}
        if let Some(HostActivity::Competition{game,..})=&self.state.host_activity{
            let action=match game.phase{CompetitionPhase::Menu=>"choose",CompetitionPhase::QuizQuestion=>"answer",_=>"continue"};
            return self.resume_competition(None,Some(action),match input{VmInput::Choose{index}=>Some(index),_=>None});
        }
        if matches!(self.state.host_activity,Some(HostActivity::SpiritEncounter{..})){return self.resume_spirit_encounter(input);}
        if let Some(HostActivity::Ending{flow})=&self.state.host_activity{
            let(action,index)=match input{VmInput::Continue=>("continue",None),VmInput::Choose{index}if flow.phase==EndingPhase::Reincarnation=>("choose",Some(index)),VmInput::Choose{index:0}=>("continue",None),_=>return self.response(Some(("ending_input","无效的结局操作。".into())),vec![])};
            return self.resume_ending(None,Some(action),index);
        }
        if matches!(self.state.host_activity,Some(HostActivity::EquipmentWorkshop{..})){return self.resume_equipment_workshop(input);}
        if matches!(self.state.host_activity,Some(HostActivity::Fortune{..})){return self.resume_fortune(input);}
        if matches!(self.state.host_activity,Some(HostActivity::SchoolExam{..})){
            let VmInput::Choose{index}=input else{return self.response(Some(("invalid_exam_input","请选择考试答案。".into())),vec![]);};
            let questions=match self.exam_questions(){Ok(questions)=>questions,Err(error)=>return self.response(Some(("exam_catalog",error)),vec![])};
            let Some(HostActivity::SchoolExam{exam})=self.state.host_activity.as_mut()else{unreachable!()};
            let mut result=None;let mut events=Vec::new();
            if !exam.started{
                match index{0=>exam.started=true,1=>result=Some(0),_=>return self.response(Some(("invalid_exam_input","选择无效。".into())),vec![])}
            }else{
                match exam.answer(index,&questions,&mut self.state.rng){Ok(correct)=>events.push(json!({"type":"examAnswer","correct":correct,"score":exam.score,"answered":exam.answered})),Err(error)=>return self.response(Some(("exam_answer",error)),vec![])}
                if exam.answered==10{result=Some(if exam.score>=60{2}else{1});events.push(json!({"type":"examCompleted","score":exam.score,"passed":exam.score>=60}));}
            }
            self.state.revision+=1;
            if let Some(result)=result{
                self.state.host_activity=None;self.state.pending=None;
                if let Err(error)=self.complete_school_exam(result,&mut events){self.block("exam_transition",error,Value::Null);}
            }else{self.refresh_host_activity();}
            return self.response(None,events);
        }
        if let Some(HostActivity::Notice{pages,page,continuation})=self.state.host_activity.as_mut(){
            if !matches!(input,VmInput::Continue){return self.response(Some(("invalid_event_input","请继续阅读当前事件。".into())),vec![]);}
            *page+=1;self.state.revision+=1;
            if *page<pages.len(){self.refresh_host_activity();return self.response(None,vec![]);}
            let continuation=continuation.clone();self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});
            let mut events=Vec::new();
            match continuation{
                NoticeContinuation::AcceptMission{mission}=>{
                    let setup=match self.program.as_ref().unwrap().condition_setup(i32::from(mission.words[6])){Ok(setup)=>setup,Err(error)=>{self.block("mission_condition",error,Value::Null);return self.response(None,events);}};
                    let player=self.state.legacy_player.as_mut().unwrap();
                    let active=ActiveCondition::from_setup(&setup,&|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)});
                    match active{Ok(active)=>self.state.active_condition=Some(active),Err(error)=>{self.block("mission_condition",error,Value::Null);return self.response(None,events);}}
                    if let Some(target)=setup.mission{player.set_i16(0x4b64ce,target.kind as i16).unwrap();player.set_i16(0x4b64cc,target.target as i16).unwrap();player.set_i32(0x4b64d0,0).unwrap();}
                    events.push(json!({"type":"missionAccepted","index":mission.index}));self.state.active_mission=Some(mission);
                }
                NoticeContinuation::ResolveMission{success}=>{
                    if let Err(error)=self.resolve_mission(success,&mut events){self.block("mission_result",error,Value::Null);return self.response(None,events);}
                    self.state.phase="playing".into();self.finish_turn(&mut events);return self.response(None,events);
                }
                NoticeContinuation::Native=>{}
            }
            if let Err(error)=self.state.vm.as_mut().unwrap().resume(VmInput::HostReturned{next_node:None}){self.block("native_resume",error,Value::Null);}else{self.advance_vm(&mut events);}
            return self.response(None,events);
        }
        let VmInput::Choose{index}=input else{return self.response(Some(("invalid_event_input","请选择商品或离开。".into())),vec![]);};
        let mut events=Vec::new();
        let mut selected_career=None;
        let mut entering_adulthood=false;
        let mut property_preview=None;
        let finished=match self.state.host_activity.as_mut().unwrap(){
            HostActivity::NativeShop{shop}=>{
                if index==shop.offers.len(){true}else{
                    let bonus_address=shop.offers.get(index).filter(|offer|offer.category==0).and_then(|offer|offer.bonus_attribute).filter(|attribute|(0..=3).contains(attribute)).map(|attribute|[0x4b6490,0x4b6492,0x4b648e,0x4b6494][attribute as usize]);
                    let old_bonus=bonus_address.map(|address|(address,self.state.legacy_player.as_ref().unwrap().i16(address).unwrap()));
                    match shop.purchase(index,self.state.legacy_player.as_mut().unwrap(),&mut self.state.housing,&mut self.state.inventory.ordinary,&mut self.state.inventory.special,&self.state.known_equipment,&mut self.state.carried_equipment){
                        Ok(purchase)=>{
                            if purchase.category==0{
                                // 43ac82 opens the house before applying its furniture reward.
                                let bonus=old_bonus.map(|(address,old)|{let player=self.state.legacy_player.as_mut().unwrap();let value=player.i16(address).unwrap();player.set_i16(address,old).unwrap();(address,value)});
                                property_preview=Some((StatusPage::House,StatusContinuation::NativeShop{shop:shop.clone(),bonus}));
                            }
                            events.push(json!({"type":"nativeItemPurchased","purchase":purchase}));
                        },Err(error)=>return self.response(Some(("cannot_purchase",error)),vec![]),
                    }
                    false
                }
            }
            HostActivity::Housing{catalog,result,preview_investment}=>{
                if result.is_some(){if index==0{
                    if *preview_investment{property_preview=Some((StatusPage::Investment,StatusContinuation::NativeReturn));}true
                }else{return self.response(Some(("housing_input","请继续阅读房产结算。".into())),vec![]);}}
                else if self.state.housing.investment_stage==0{
                    if index==4{true}else{
                        let Some(style)=catalog.styles.iter().find(|style|style.id==index)else{return self.response(Some(("housing_input","住宅不存在。".into())),vec![]);};
                        if let Err(error)=self.state.housing.buy(style,self.state.legacy_player.as_mut().unwrap()){return self.response(Some(("housing_purchase",error)),vec![]);}
                        *result=Some("住宅购买成功。".into());events.push(json!({"type":"housePurchased","style":index,"price":style.price}));false
                    }
                }else{match index{
                    5=>{match self.state.housing.invest(catalog,self.state.legacy_player.as_mut().unwrap()){
                        Ok(price)=>{*preview_investment=true;*result=Some(format!("本期房产投资完成，支付 {price} 元。"));events.push(json!({"type":"propertyInvestment","price":price,"stage":self.state.housing.investment_stage}));},
                        Err(error)=>return self.response(Some(("housing_investment",error)),vec![]),
                    }false},
                    6=>{match self.state.housing.collect_income(catalog,self.state.legacy_player.as_mut().unwrap()){
                        Ok(income)=>{*result=Some(format!("已领取房产收益 {income} 元。"));events.push(json!({"type":"propertyIncome","amount":income}));},
                        Err(error)=>return self.response(Some(("housing_income",error)),vec![]),
                    }false},
                    7=>true,_=>return self.response(Some(("housing_input","房产操作无效。".into())),vec![]),
                }}
            }
            HostActivity::StatShop{shop}=>{
                if index==shop.offers.len(){true}else{
                    match shop.purchase(index,self.state.legacy_player.as_mut().unwrap()){Ok(Some(gift))=>self.state.gift_level=gift,Ok(None)=>{},Err(error)=>return self.response(Some(("cannot_purchase",error)),vec![])}
                    events.push(json!({"type":"itemPurchased","offer":shop.offers[index]}));false
                }
            }
            HostActivity::Career{careers,entering_adulthood:entering}=>{
                entering_adulthood=*entering;
                if index==24{true}else{
                    let Some(career)=careers.iter().find(|career|career.id==index)else{return self.response(Some(("invalid_career","职业不存在。".into())),vec![]);};
                    if !career.eligible(self.state.legacy_player.as_ref().unwrap(),&self.state.career_levels).unwrap_or(false){return self.response(Some(("career_requirements","尚未满足这个职业的条件。".into())),vec![]);}
                    selected_career=Some(index as i16);true
                }
            }
            HostActivity::Notice{..}=>unreachable!(),
            HostActivity::SchoolExam{..}=>unreachable!(),
            HostActivity::Tracking{..}|HostActivity::Hammer{..}|HostActivity::Fist{..}|HostActivity::Gas{..}|HostActivity::Fire{..}|HostActivity::Firebee{..}=>return self.response(Some(("minigame_input","请使用小游戏中的操作。".into())),vec![]),
            HostActivity::Fortune{..}=>unreachable!(),
            HostActivity::EquipmentWorkshop{..}=>unreachable!(),
            HostActivity::Ending{..}=>unreachable!(),
            HostActivity::Relationship{..}=>return self.response(Some(("relationship_input","请使用感情事件中的操作。".into())),vec![]),
            HostActivity::SpiritEncounter{..}=>unreachable!(),
            HostActivity::SpiritLottery{..}=>return self.response(Some(("spirit_input","请使用转盘中的操作。".into())),vec![]),
            HostActivity::Competition{..}|HostActivity::Battle{..}|HostActivity::FinalBattle{..}=>unreachable!(),
            HostActivity::Inventory{..}=>return self.response(Some(("inventory_input","请使用物品栏中的操作。".into())),vec![]),
            HostActivity::Status{..}=>return self.response(Some(("status_input","请使用参数页面中的操作。".into())),vec![]),
        };
        if let Some(career)=selected_career{if let Err(error)=self.choose_career(career){return self.response(Some(("career_change",error)),vec![]);}events.push(json!({"type":"careerChanged","career":career}));}
        if entering_adulthood && self.state.legacy_player.as_ref().unwrap().life().profession==-1{self.choose_career(4).unwrap();}
        self.sync_player();self.state.revision+=1;
        if let Some((page,continuation))=property_preview{
            if let Err(error)=self.begin_property_preview(page,continuation){self.block("property_preview",error,Value::Null);}
            return self.response(None,events);
        }
        if finished{
            self.state.host_activity=None;self.state.pending=None;self.state.scene=json!({});
            if entering_adulthood{if let Err(error)=self.refresh_spirit_positions(&mut events){self.block("spirit_positions",error,Value::Null);return self.response(None,events);}}
            if let Some(vm)=self.state.vm.as_mut(){if let Err(error)=vm.resume(VmInput::HostReturned{next_node:None}){self.block("native_resume",error,Value::Null);}else{self.advance_vm(&mut events);}}
            else{self.state.phase="playing".into();}
        }else{self.refresh_host_activity();}
        self.response(None,events)
    }

    fn run_actions(&mut self,events:&mut Vec<Value>) {
        while !self.state.action_queue.is_empty() && self.state.phase!="blocked" {
            let mut action=self.state.action_queue.remove(0);
            if !action.spirit_checked{
                let from_facing=self.state.map_facing;let map=self.current_map().unwrap();let mut from=self.state.tile_id.unwrap();let mut facing=from_facing;
                for &to in &action.path{if let Some(direction)=map.facing_between(from,to){facing=Some(direction);}from=to;}
                self.state.map_facing=facing;
                events.push(json!({"type":"moved","fromTileId":self.state.tile_id,"tiles":action.path,"rolled":self.state.queued_die.take(),"fromFacing":from_facing,"facing":facing}));self.state.tile_id=Some(action.tile_id);
            }
            if action.landed && !action.spirit_checked{
                action.spirit_checked=true;
                match self.prepare_spirit_event(events){
                    Ok(true)=>{action.path.clear();self.state.action_queue.insert(0,action);return;},
                    Ok(false)=>{},Err(error)=>{self.block("spirit_event",error,Value::Null);return;},
                }
            }
            events.push(json!({"type":"tileTriggered","tileId":action.tile_id,"eventId":action.event_id,"landed":action.landed}));
            match action.event_id {
                0x5dc4|0x5dcc|0x5ddb|0x5de7=>{
                    match StudentReward::calculate(self.state.legacy_player.as_ref().unwrap(),action.landed){
                        Ok(reward)=>{self.state.pending=Some(json!({"kind":"allocation","text":"分配成长点数","points":reward.points,"money":reward.money,"defaults":reward.default_allocation()}));self.state.pending_allocation=Some(reward);self.state.phase="event".into();}
                        Err(error)=>self.block("student_reward",error,Value::Null),
                    }
                    return;
                }
                0x5dda=>{
                    if let Err(error)=self.collect_salary(action.landed,events){self.block("payday_handler",error,json!(action));return;}
                }
                0x5dcd|0x5ddc=>{
                    let player=self.state.legacy_player.as_mut().unwrap();
                    if player.i16(0x4b64cc).unwrap()!=-1 && player.i16(0x4b64ce).unwrap()==10{player.set_i16(0x4b64d0,1).unwrap();}
                    player.set_i16(0x4b64b0,1).unwrap();
                }
                0x5dec|0x5df6|0x5e01|0x5e0b|0x5e15=>{},
                id=>{
                    self.state.vm=Some(LegacyVm::new(id));self.state.phase="event".into();self.advance_vm(events);return;
                }
            }
        }
        if self.state.phase!="blocked" {self.finish_turn(events);}
    }

    fn advance_vm(&mut self,events:&mut Vec<Value>) {
        let Some(program)=&self.program else{self.block("missing_events","缺少原版事件数据。".into(),Value::Null);return;};
        let Some(vm)=&mut self.state.vm else{return;};
        let batch=match vm.advance(program,&mut ||self.state.rng.next_rand()){
            Ok(batch)=>batch,Err(error)=>{self.block("event_decode",error,Value::Null);return;}
        };
        for effect in batch.effects {
            events.push(serde_json::to_value(&effect).unwrap());
            match effect {
                VmEffect::Pattern{resource_id,..}=>{self.state.scene["patternResource"]=json!(resource_id);self.state.scene["backgroundResource"]=Value::Null;},
                VmEffect::Background{resource_id,..}=>{self.state.scene["backgroundResource"]=json!(resource_id);},
                VmEffect::Portraits{left_id,right_id,..}=>{
                    self.state.scene["leftPortraitId"]=json!(left_id);self.state.scene["rightPortraitId"]=json!(right_id);
                    self.state.scene["leftPortraitResource"]=json!(self.program.as_ref().unwrap().portrait_resource(left_id));
                    self.state.scene["rightPortraitResource"]=json!(self.program.as_ref().unwrap().portrait_resource(right_id));
                }
                VmEffect::Dialogue{dialogue_id,content,left_id,right_id}=>{
                    let speaker=content.speaker.map(|speaker|match speaker{DialogueSpeaker::Player=>self.state.player.as_ref().map(|p|p.name.clone()).unwrap_or_default(),DialogueSpeaker::Named{name}=>name});
                    self.state.pending=Some(json!({"kind":"dialogue","text":content.text,"speaker":speaker,"dialogueId":dialogue_id,"leftPortraitId":left_id,"rightPortraitId":right_id}));
                    self.state.message="".into();
                }
                VmEffect::ClearPortraits{left,right}=>{if left{self.state.scene["leftPortraitResource"]=Value::Null;}if right{self.state.scene["rightPortraitResource"]=Value::Null;}},
                VmEffect::Choice{options,..}=>{self.state.pending=Some(json!({"kind":"choice","text":"请选择","options":options}));},
                VmEffect::ApplyResult{result}=>{
                    let player=self.state.legacy_player.as_mut().unwrap();
                    for delta in result.changes {
                        let current=if delta.wrapping_bits==32{player.i32(delta.address)}else{player.i16(delta.address).map(i32::from)};
                        let changed=current.and_then(|current|if delta.wrapping_bits==32{player.set_i32(delta.address,delta.apply(current))}else{player.set_i16(delta.address,delta.apply(current)as i16)});
                        if let Err(error)=changed{self.block("result_field",error,json!({"address":delta.address}));return;}
                    }
                    if let Some(career)=result.unlock_career {
                        if player.life().profession!=career{
                            player.set_i16(0x4b64a2,career).unwrap();player.set_i16(0x4b64a6,5).unwrap();player.set_i32(0x4b6538,3).unwrap();
                            if self.state.career_history.len()<98{self.state.career_history.push(json!({"year":player.life().year,"career":career,"rank":3}));}
                            events.push(json!({"type":"careerChanged","career":career}));
                        }
                    }
                }
                VmEffect::ConfigureCondition{condition}=>{
                    let player=self.state.legacy_player.as_mut().unwrap();
                    let active=ActiveCondition::from_setup(&condition,&|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)});
                    if let Some(mission)=&condition.mission {
                        player.set_i16(0x4b64ce,mission.kind as i16).unwrap();player.set_i16(0x4b64cc,mission.target as i16).unwrap();player.set_i16(0x4b64d0,0).unwrap();
                    }
                    match active{Ok(active)=>self.state.active_condition=Some(active),Err(error)=>{self.block("condition_handler",error,json!(condition));return;}}
                }
                VmEffect::SetValue{address,value}=>{
                    if let Err(error)=self.state.legacy_player.as_mut().unwrap().set_i16(address,value as i16){self.block("player_field",error,json!({"address":address}));return;}
                }
                VmEffect::NativeCall{call}=>{
                    if call.opcode==15{if let Err(error)=self.begin_battle(events){self.block("battle_handler",error,json!(call));}return;}
                    if call.opcode==17{
                        if let Err(error)=self.begin_item_shop(){self.block("item_shop",error,json!(call));}return;
                    }
                    if call.opcode==18{
                        if let Err(error)=self.begin_housing(){self.block("housing",error,json!(call));}return;
                    }
                    if call.opcode==27{
                        if let Err(error)=self.begin_equipment_workshop(){self.block("equipment_handler",error,json!(call));}return;
                    }
                    if call.opcode==25{
                        if let Err(error)=self.begin_fortune(){self.block("fortune_handler",error,json!(call));}return;
                    }
                    if call.opcode==24{
                        if let Err(error)=self.begin_relationship(events){self.block("relationship_handler",error,json!(call));}return;
                    }
                    if call.opcode==22{
                        let player=self.state.legacy_player.as_mut().unwrap();
                        if player.i16(0x4b64cc).unwrap()>0 && player.i16(0x4b64ce).unwrap()==10{player.set_i16(0x4b64d0,1).unwrap();}
                        player.set_i32(0x4b64b0,1).unwrap();
                        if let Err(error)=self.state.vm.as_mut().unwrap().resume(VmInput::HostReturned{next_node:None}){self.block("native_resume",error,json!(call));return;}
                        self.advance_vm(events);return;
                    }
                    if call.opcode==23{
                        if let Err(error)=self.begin_minigame(){self.block("minigame_handler",error,json!(call));}return;
                    }
                    if call.opcode==26{
                        if let Err(error)=self.begin_mission(){self.block("mission_selection",error,json!(call));}return;
                    }
                    if call.opcode==19{
                        if let Err(error)=self.begin_career(false){self.block("career_selection",error,json!(call));}return;
                    }
                    if call.opcode==16{
                        if let Err(error)=self.begin_stat_shop(){self.block("stat_shop",error,json!(call));}return;
                    }
                    if call.opcode==28{
                        let next=match self.select_event(events){Ok(next)=>next,Err(error)=>{self.block("ordinary_event",error,json!(call));return;}};
                        if let Err(error)=self.state.vm.as_mut().unwrap().resume(VmInput::HostReturned{next_node:Some(next)}){self.block("native_resume",error,json!(call));return;}
                        self.advance_vm(events);return;
                    }
                    self.block("native_event_handler","此事件需要尚未还原的原版玩法模块，进度已保留。".into(),json!(call));return;
                },
                VmEffect::Finished=>{},
            }
        }
        self.sync_player();
        if batch.finished {
            self.state.vm=None;self.state.pending=None;self.state.scene=json!({});self.state.phase="playing".into();self.run_actions(events);
        }else if matches!(batch.pending,Some(VmPending::Dialogue{..}|VmPending::Choice{..})){self.state.phase="event".into();}
    }

    fn finish_turn(&mut self,events:&mut Vec<Value>) {
        if !self.state.turn_in_progress{return;}
        if let Some(condition)=&self.state.active_condition {
            let player=self.state.legacy_player.as_ref().unwrap();
            match condition.evaluate(&|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)}) {
                Ok(ConditionOutcome::Waiting)=>{},
                Ok(outcome)=>{
                    let success=outcome==ConditionOutcome::Succeeded;
                    let Some(mission)=self.state.active_mission.as_ref()else{self.block("condition_resolution","此条件缺少原版任务索引，无法确定结算表。".into(),json!({"outcome":outcome,"conditionId":condition.setup.condition_id}));return;};
                    let mode=if success{1}else{2};
                    let pages=(0..mission.words[3+mode].max(1)).map(|offset|i32::from(mission.words[mode])+i32::from(offset)).collect();
                    self.state.scene=json!({"backgroundResource":self.program.as_ref().unwrap().background_resource(i32::from(mission.words[9]))});
                    if let Err(error)=self.show_notice(pages,NoticeContinuation::ResolveMission{success}){self.block("mission_dialogue",error,Value::Null);}return;
                }
                Err(error)=>{self.block("condition_state",error,Value::Null);return;}
            }
        }
        let life=self.state.legacy_player.as_ref().unwrap().life();
        if !life.traveling && life.stage!=2{
            let(pool,position)=match self.spirit_map_context(){Ok(context)=>context,Err(error)=>{self.block("spirit_positions",error,Value::Null);return;}};
            let total_turn=self.state.legacy_player.as_ref().unwrap().i16(0x4b64aa).unwrap();
            match self.state.spirits.expire(total_turn,&pool,&position,&mut self.state.rng){
                Ok(Some(spirit))=>{self.state.legacy_player.as_mut().unwrap().set_i16(0x4b64d6,-1).unwrap();events.push(json!({"type":"spiritDeparted","spirit":spirit}));},
                Ok(None)=>{},Err(error)=>{self.block("spirit_departure",error,Value::Null);return;},
            }
        }
        let player=self.state.legacy_player.as_mut().unwrap();
        let outcome=match player.settle_turn_numbers(){Ok(outcome)=>outcome,Err(error)=>{self.block("turn_numbers",error,Value::Null);return;}};
        let stage=player.life().stage;
        let traveling=player.life().traveling;
        self.state.turn_in_progress=false;self.sync_player();events.push(json!({"type":"turnCompleted","outcome":outcome}));
        if traveling{
            if self.state.legacy_player.as_ref().unwrap().i16(0x4b64b0).unwrap()==1 || outcome.recovered_from_zero_hp{
                if let Err(error)=self.return_from_travel(events){self.block("travel_return",error,Value::Null);return;}
            }
            self.state.legacy_player.as_mut().unwrap().update_skills().unwrap();self.sync_player();self.state.phase="playing".into();return;
        }
        if outcome.aged && (2..=4).contains(&stage){
            // FUN_0041ba90 only shows a sex/stage/year specific picture and sound.
            let sex=self.state.legacy_player.as_ref().unwrap().i16(0x4b6498).unwrap();
            let prefix=match(stage,sex){(2,0)=>"jm",(2,_)=>"jm0",(3,0)=>"sm",(3,_)=>"sm0",(4,0)=>"un",_=>"un0"};
            events.push(json!({"type":"birthday","resource":format!("install/bmp/{prefix}{:02}.bmp",outcome.stage_turn/5),"age":self.state.stats.age}));
        }
        let limit=self.rules.as_ref().and_then(|rules|rules["stages"].as_array()).and_then(|stages|stages.iter().find(|item|item["id"].as_i64()==Some(i64::from(stage)))).and_then(|item|item["stageTurns"].as_i64());
        if (2..=4).contains(&stage) && limit.is_some_and(|limit|i64::from(outcome.stage_turn)==limit/2){
            if let Err(error)=self.begin_competition(outcome){self.block("student_competition",error,json!({"stage":stage}));}return;
        }
        self.finish_turn_transitions(outcome,events);
    }

    fn finish_turn_transitions(&mut self,outcome:TurnOutcome,events:&mut Vec<Value>){
        let stage=self.state.legacy_player.as_ref().unwrap().life().stage;
        let limit=self.rules.as_ref().and_then(|rules|rules["stages"].as_array()).and_then(|stages|stages.iter().find(|item|item["id"].as_i64()==Some(i64::from(stage)))).and_then(|item|item["stageTurns"].as_i64());
        if self.state.legacy_player.as_ref().unwrap().i16(0x4b64b0).unwrap()==1{
            if let Err(error)=self.enter_travel(events){self.block("travel_transition",error,Value::Null);return;}
            self.state.legacy_player.as_mut().unwrap().update_skills().unwrap();self.sync_player();self.state.phase="playing".into();return;
        }
        if let Some(limit)=limit {
            if i64::from(outcome.stage_turn)>=limit && limit>0{
                self.state.legacy_player.as_mut().unwrap().set_i16(0x4b64bc,0).unwrap();self.state.legacy_player.as_mut().unwrap().set_i16(0x4b64be,0).unwrap();
                match stage{
                    0|1|2|4=>{
                        if let Err(error)=self.transition_stage(if stage==4{6}else{stage+1},events){self.block("stage_transition",error,json!({"stage":stage}));return;}
                        if self.state.host_activity.is_some(){return;}
                        if let Err(error)=self.refresh_spirit_positions(events){self.block("spirit_positions",error,Value::Null);return;}
                    }
                    3=>{if let Err(error)=self.begin_school_exam(){self.block("graduation_exam",error,Value::Null);}return;}
                    _=>{if let Err(error)=self.prepare_final_history().and_then(|_|self.begin_final_battle()){self.block("final_battle",error,json!({"stage":stage}));}return;}
                }
            }
        }
        if self.state.legacy_player.as_ref().unwrap().i16(0x4b64b4).unwrap()==1{
            let player=self.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b64b4,0).unwrap();player.set_i16(0x4b64bc,0).unwrap();player.set_i16(0x4b64be,0).unwrap();
            if let Err(error)=self.begin_school_exam(){self.block("graduation_exam",error,Value::Null);}return;
        }
        self.state.legacy_player.as_mut().unwrap().update_skills().unwrap();self.sync_player();
        self.state.phase="playing".into();self.state.message="可以继续掷骰子。".into();
    }

    pub fn serialize(&self) -> String {
        let state = serde_json::to_vec(&self.state).unwrap();
        serde_json::to_string(&Save { format_version:SAVE_VERSION,content_hash:self.content_hash.clone(),checksum:digest(&state),state:self.state.clone() }).unwrap()
    }

    pub fn restore(&mut self, save_json: &str) -> String {
        let save: Save = match serde_json::from_str(save_json) { Ok(save) => save, Err(e) => return self.response(Some(("invalid_save",e.to_string())),vec![]) };
        if save.format_version != SAVE_VERSION || save.state.schema_version != SAVE_VERSION { return self.response(Some(("save_version","不支持此存档版本。".into())),vec![]); }
        if save.content_hash != self.content_hash { return self.response(Some(("content_mismatch","存档与当前原版资源清单不匹配。".into())),vec![]); }
        if save.checksum != digest(&serde_json::to_vec(&save.state).unwrap()) { return self.response(Some(("save_checksum","存档校验失败。".into())),vec![]); }
        if !["ready","exploring","playing","event","blocked","completed"].contains(&save.state.phase.as_str()) { return self.response(Some(("invalid_save","此存档状态无法识别。".into())),vec![]); }
        if save.state.battle_persistent.as_ref().is_some_and(|persistent|!valid_battle_persistent(persistent)) || !self.valid_saved_activity(&save.state){
            return self.response(Some(("invalid_save","存档的战斗或物品栏记录不完整。".into())),vec![]);
        }
        if save.state.inventory.ordinary.len()!=13 || save.state.inventory.special.len()!=27 || save.state.career_levels.len()!=25 ||
            save.state.gas_target_state.is_some_and(|state|state>3) || save.state.map_facing.is_some_and(|facing|facing>3) ||
            save.state.relationship_candidates.len()!=8 || save.state.known_equipment.len()!=18 || save.state.carried_equipment.len()!=8 ||
            save.state.carried_equipment.iter().any(|slot|!(-1..18).contains(&slot.id) || !(0..=4).contains(&slot.level)) ||
            save.state.known_equipment.iter().any(|item|item.level>4) || save.state.active_mission.as_ref().is_some_and(|mission|mission.words.len()!=11){
            return self.response(Some(("invalid_save","存档的道具、职业或任务记录不完整。".into())),vec![]);
        }
        if !(-1..=3).contains(&save.state.housing.style) || !(0..=15).contains(&save.state.housing.furniture) || save.state.housing.investment_stage>13 ||
            save.state.housing.style==3 && save.state.housing.furniture>14 ||
            save.state.housing.style==-1 && (save.state.housing.furniture!=0 || save.state.housing.investment_stage!=0) ||
            save.state.housing.style>=0 && save.state.housing.investment_stage==0{
            return self.response(Some(("invalid_save","存档的住宅状态无效。".into())),vec![]);
        }
        match (&save.state.map_id,save.state.tile_id) {
            (Some(id),Some(tile)) if self.maps.iter().any(|map| &map.id == id && tile < map.chips.len()) => {},
            (None,None) if save.state.phase == "ready" => {},
            _ => return self.response(Some(("invalid_save","存档地图或坐标无效。".into())),vec![]),
        }
        if let Some(player)=&save.state.legacy_player {
            if !player.valid() || save.state.player.as_ref().is_none_or(|profile|profile.validate().is_err()) || !["playing","event","blocked","completed"].contains(&save.state.phase.as_str()) {
                return self.response(Some(("invalid_save","存档角色资料无效。".into())),vec![]);
            }
            if save.state.stats!=player.stats() || save.state.turn!=player.i16(0x4b64aa).unwrap().max(0)as u32 {
                return self.response(Some(("invalid_save","存档参数与角色状态不一致。".into())),vec![]);
            }
            if save.state.phase=="event" && ((save.state.vm.is_none() && save.state.pending_allocation.is_none() && save.state.host_activity.is_none()) || save.state.pending.is_none()) {
                return self.response(Some(("invalid_save","存档事件状态不完整。".into())),vec![]);
            }
        }else if save.state.turn != 0 || save.state.player.is_some() || save.state.stats != Stats::default() || !["ready","exploring"].contains(&save.state.phase.as_str()) {
            return self.response(Some(("invalid_save","存档缺少原版角色状态。".into())),vec![]);
        }
        self.state = save.state;
        self.response(None,vec![json!({"type":"stateRestored"})])
    }
    fn valid_saved_activity(&self,state:&State)->bool{
        match &state.host_activity{
            Some(HostActivity::Status{panel,continuation})=>{
                let valid_transition=panel.transition.as_ref().is_none_or(|transition|panel.page==StatusPage::House&&transition.kind==crate::transition::TransitionKind::Rain&&transition.duration_seconds==1&&transition.unit_size==2&&!transition.finished&&transition.valid_saved_state());
                let valid_continuation=match continuation{
                    Some(StatusContinuation::NativeShop{bonus,..})=>panel.page==StatusPage::House&&bonus.is_none_or(|(address,_)|[0x4b6490,0x4b6492,0x4b648e,0x4b6494].contains(&address)),
                    Some(StatusContinuation::NativeReturn)=>panel.page==StatusPage::Investment,
                    None=>true,
                };
                let valid_overview=panel.overview_animation.as_ref().is_none_or(|saved|panel.page!=StatusPage::Overview&&saved.back_tracks[0].frame<6&&saved.back_tracks[1].frame<5);
                valid_transition&&valid_continuation&&valid_overview&&panel.back_tracks[0].frame<6&&panel.back_tracks[1].frame<5&&match panel.page{StatusPage::CareerHistory=>panel.scroll<=state.career_history.len().saturating_sub(10),StatusPage::Skills=>panel.scroll<=22,_=>panel.scroll==0}
            },
            Some(HostActivity::Inventory{panel})=>{
                let Some(persistent)=&state.battle_persistent else{return false;};let Ok(rules)=self.inventory_rules()else{return false;};
                let expected=InventoryPanel::new(panel.tab,&state.inventory.ordinary,&state.inventory.special,&state.carried_equipment,&persistent.equipped_ids);
                let definitions=match panel.tab{InventoryTab::Ordinary=>&rules.ordinary,InventoryTab::Special=>&rules.special,InventoryTab::Equipment=>&rules.equipment};
                panel.entries.len()==expected.entries.len()&&panel.entries.iter().zip(&expected.entries).all(|(actual,expected)|actual.id==expected.id&&actual.slot==expected.slot&&actual.equipped==expected.equipped&&definitions.get(actual.id).is_some_and(|definition|actual.frame<definition.frame_count))
                    &&panel.selected_index.is_none_or(|index|index<panel.entries.len())&&(!panel.confirm_drop||panel.selected_index.is_some())&&panel.back_tracks[0].frame<6&&panel.back_tracks[1].frame<5
            },
            Some(HostActivity::FinalBattle{game})=>self.final_battle_rules().is_ok_and(|rules|game.valid_saved_state(&rules)),
            Some(HostActivity::Battle{game})=>{
                let Ok(rules)=self.battle_rules()else{return false;};let input=&game.input;
                if game.rules!=rules||!valid_battle_persistent(&game.persistent)||input.sex>1||!(-1..25).contains(&input.career)||!(0..=7).contains(&input.stage)||input.carried.len()!=8||input.equipped.len()!=6||input.equipped.iter().any(|&id|!(-1..18).contains(&id))
                    ||input.carried.iter().any(|item|!(-1..18).contains(&item.id)||!(0..=4).contains(&item.level))||game.enemy>=15||game.background>=9||game.player_class<0||game.player_class>30
                    ||game.cards.len()!=5||game.selected.len()!=5||game.card_x.len()!=5||game.used_cards.len()!=5||game.used_selected.len()!=5||game.card_count>game.card_capacity||!(2..=5).contains(&game.card_capacity)
                    ||game.cards.iter().chain(&game.used_cards).any(|card|!(0..15).contains(&card.kind))||game.player_portrait_timers.len()!=4||game.enemy_portrait_timers.len()!=4||game.player_actor>4||game.enemy_actor>4||game.enemy_effect>1||game.god_effect>2||game.combo_count>5||game.attack_kind>4||game.help_page>9
                    ||game.pending_select.is_some_and(|index|index>=game.card_count)||game.hovered_card.is_some_and(|index|index>=5){return false;}
                let mut names=vec!["strong".to_owned()];for index in 0..5{for prefix in ["player","enemy","element","hover","selected"]{names.push(format!("{prefix}{index}"));}names.push(format!("combo{}",index+1));}
                for index in 0..2{for prefix in ["enemyEffect","shield","ready"]{names.push(format!("{prefix}{index}"));}}for index in 0..3{names.push(format!("god{index}"));}for index in 0..10{names.push(format!("help{index}"));}
                names.iter().all(|name|game.tracks.get(name).is_some_and(|track|track.frame_count>0&&track.frame>=0&&track.frame<track.frame_count))
            },
            _=>true,
        }
    }
}

fn valid_battle_persistent(persistent:&BattlePersistent)->bool{
    persistent.enemies.len()==15&&persistent.enemies.iter().all(|enemy|(0..=9).contains(&enemy.rank)&&(0..=9).contains(&enemy.respawn_level))&&persistent.cards.len()<=5&&persistent.cards.iter().all(|card|(0..15).contains(&card.kind))
        &&persistent.equipped_ids.len()==6&&persistent.equipped_ids.iter().all(|&id|(-1..18).contains(&id))
}

fn canonicalize_catalog_numbers(value:&mut Value){
    match value{
        Value::Array(items)=>for item in items{canonicalize_catalog_numbers(item);},
        Value::Object(fields)=>for item in fields.values_mut(){canonicalize_catalog_numbers(item);},
        Value::Number(number) if number.is_f64()=>{
            let float=number.as_f64().unwrap();
            // Beyond Number.MAX_SAFE_INTEGER JavaScript cannot preserve every
            // integer. Keep those representations distinct instead of rounding.
            if float.fract()==0.0&&float.abs()<=9_007_199_254_740_991.0{*number=(float as i64).into();}
        },
        _=>{},
    }
}

fn digest(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes { hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3); }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONTENT:&str=r#"{"maps":[{"id":"test","start":{"row":0,"column":0},"chips":[{"row":0,"column":0,"eventId":1,"next":[{"row":1,"column":1}]},{"row":1,"column":1,"eventId":2,"next":[{"row":0,"column":0}]}]}]}"#;
    #[test]
    fn save_catalog_identity_ignores_json_format_but_rejects_changed_data(){
        let original=Runtime::new(CONTENT).unwrap();let save=original.serialize();
        let reordered=r#"{ "maps": [ { "chips": [{ "next": [{"column":1,"row":1}], "eventId":1,"column":0,"row":0 }, {"next":[{"column":0,"row":0}],"eventId":2,"column":1,"row":1}], "start":{"column":0,"row":0}, "id":"test" } ] }"#;
        let mut same=Runtime::new(reordered).unwrap();assert_eq!(original.content_hash,same.content_hash);
        assert_eq!(serde_json::from_str::<Value>(&same.restore(&save)).unwrap()["ok"],true);
        let mut changed=Runtime::new(&reordered.replace("\"eventId\":2","\"eventId\":3")).unwrap();
        let rejected:Value=serde_json::from_str(&changed.restore(&save)).unwrap();assert_eq!(rejected["ok"],false);assert_eq!(rejected["error"]["code"],"content_mismatch");
    }
    #[test]
    fn save_catalog_identity_normalizes_nested_javascript_numbers_and_round_trips(){
        let catalog=|rules:&str|format!("{},\"rules\":{rules}}}",&CONTENT[..CONTENT.len()-1]);
        let native=catalog(r#"{"nested":{"integer":1.0,"zero":-0.0,"array":[0.0,-2.0,1e3,0.25,-0.75,1e-7,{"maximum":9007199254740991.0,"minimum":-9007199254740991.0}]}}"#);
        let browser=catalog(r#"{"nested":{"array":[0,-2,1000,0.25,-0.75,0.0000001,{"minimum":-9007199254740991,"maximum":9007199254740991}],"zero":0,"integer":1}}"#);
        let mut native=Runtime::new(&native).unwrap();let mut browser_runtime=Runtime::new(&browser).unwrap();
        assert_eq!(native.content_hash,browser_runtime.content_hash);let original=native.serialize();
        assert_eq!(serde_json::from_str::<Value>(&browser_runtime.restore(&original)).unwrap()["ok"],true);
        let browser_save=browser_runtime.serialize();assert_eq!(original,browser_save);
        assert_eq!(serde_json::from_str::<Value>(&native.restore(&browser_save)).unwrap()["ok"],true);assert_eq!(original,native.serialize());
        let changed=browser.replace("0.25","0.2500000000000001");let mut changed=Runtime::new(&changed).unwrap();
        assert_ne!(native.content_hash,changed.content_hash);assert_eq!(serde_json::from_str::<Value>(&changed.restore(&original)).unwrap()["error"]["code"],"content_mismatch");
        for number in ["9007199254740992.0","-9007199254740992.0"]{
            let mut value:Value=serde_json::from_str(number).unwrap();canonicalize_catalog_numbers(&mut value);assert!(value.as_number().unwrap().is_f64());
        }
    }
    fn dispatch(runtime:&mut Runtime, command:Value)->Value { serde_json::from_str(&runtime.dispatch(&command.to_string())).unwrap() }
    fn real_runtime()->Runtime{
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let read=|name:&str|->Value{serde_json::from_str(&std::fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap()).unwrap()};
        let mut content=read("maps");for name in ["events","rules","questionnaires"]{content[name]=read(name);}
        let mut runtime=Runtime::new(&content.to_string()).unwrap();
        assert_eq!(dispatch(&mut runtime,json!({"seq":1,"type":"newGame","seed":1,"profile":{"name":"测试","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}}))["ok"],true);
        runtime
    }
    fn next(runtime:&mut Runtime,kind:&str,index:usize)->Value{dispatch(runtime,json!({"seq":runtime.state.last_seq+1,"type":kind,"index":index}))}
    fn relationship_action(runtime:&mut Runtime,action:&str,index:usize)->Value{dispatch(runtime,json!({"seq":runtime.state.last_seq+1,"type":"relationshipAction","action":action,"index":index}))}
    #[test]
    fn status_pages_restore_scroll_and_return_without_changing_life(){
        let mut runtime=real_runtime();let run=|runtime:&mut Runtime,action:&str,page:Option<&str>,index:Option<usize>|{let seq=runtime.state.last_seq+1;dispatch(runtime,json!({"seq":seq,"type":"statusAction","action":action,"page":page,"index":index}))};
        assert!(runtime.snapshot_value()["statusOverview"].is_null());assert!(runtime.snapshot_value()["statusDetails"].is_null());
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"openStatus"}))["ok"],true);assert_eq!(run(&mut runtime,"page",Some("house"),None)["ok"],false);assert_eq!(run(&mut runtime,"close",None,None)["ok"],true);
        runtime.state.housing=HousingState{style:0,furniture:2,investment_stage:5,purchase_years:vec![2025]};runtime.state.spouse_index=3;
        runtime.state.career_history=(0..12).map(|index|json!({"year":2022+index,"career":index,"rank":1+index%3})).collect();
        let player=runtime.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b649e,6).unwrap();player.set_i16(0x4b649c,2040).unwrap();player.set_i16(0x4b64a2,1).unwrap();player.set_i16(0x4b64a6,3).unwrap();player.set_i16(0x4b6508,1).unwrap();player.set_i16(0x4b650a,2).unwrap();player.set_i16(0x4b650c,3).unwrap();player.set_i16(0x4b653a,3).unwrap();runtime.sync_player();
        let before=(runtime.state.legacy_player.clone(),runtime.state.rng.clone(),runtime.state.turn);let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"openStatus"}))["ok"],true);
        assert_eq!(runtime.snapshot_value()["statusOverview"]["canInspectHouse"],true);assert_eq!(runtime.snapshot_value()["statusDetails"]["careerHistory"].as_array().unwrap().len(),12);
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("status.vlsave.json"),runtime.serialize()).unwrap();}
        let mut expected_rng=runtime.state.rng.clone();let mut rain=crate::transition::NativeTransition::rain(1,2,&mut expected_rng);rain.tick(1000,&mut expected_rng);assert!(rain.finished);
        for _ in 0..2{crate::transition::NativeTransition::mosaic(2,16,&mut expected_rng).tick(0,&mut expected_rng);}
        for page in ["house","investment"]{
            assert_eq!(run(&mut runtime,"page",Some(page),None)["ok"],true);
            if page=="house"{
                let rng=runtime.state.rng.clone();assert_eq!(run(&mut runtime,"close",None,None)["ok"],false);assert_eq!(run(&mut runtime,"page",Some("investment"),None)["ok"],false);assert_eq!(runtime.state.rng,rng);
                let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusTick","elapsedMs":27}))["ok"],true);
                let state=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);
                let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusTick","elapsedMs":973}))["ok"],true);
            }
            assert_eq!(run(&mut runtime,"close",None,None)["ok"],true);assert_eq!(runtime.snapshot_value()["pending"]["state"]["page"],"overview");
        }
        assert_eq!(run(&mut runtime,"page",Some("careerHistory"),None)["ok"],true);assert_eq!(run(&mut runtime,"scroll",None,Some(99))["ok"],true);assert_eq!(runtime.snapshot_value()["pending"]["state"]["scroll"],2);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusTick","elapsedMs":115}))["ok"],true);let saved_state=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,saved_state);
        assert_eq!(run(&mut runtime,"page",Some("skills"),None)["ok"],true);assert_eq!(run(&mut runtime,"scroll",None,Some(99))["ok"],true);assert_eq!(runtime.snapshot_value()["pending"]["state"]["scroll"],22);assert_eq!(run(&mut runtime,"close",None,None)["ok"],true);assert_eq!(run(&mut runtime,"close",None,None)["ok"],true);assert_eq!(runtime.state.phase,"playing");assert_eq!((runtime.state.legacy_player.clone(),runtime.state.turn),(before.0,before.2));assert_eq!(runtime.state.rng,expected_rng);
        assert!(runtime.snapshot_value()["statusOverview"].is_null());assert!(runtime.snapshot_value()["statusDetails"].is_null());
    }
    #[test]
    fn property_purchase_previews_restore_and_resume_the_original_caller(){
        let mut runtime=real_runtime();runtime.state.housing=HousingState{style:0,furniture:0,investment_stage:1,purchase_years:vec![2016]};
        runtime.state.legacy_player.as_mut().unwrap().set_i32(0x4b648a,10_000_000).unwrap();runtime.sync_player();
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,17,30001,0,0,0]},{"words":[30001,18,30002,0,0,0]},{"words":[30002,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        let Some(HostActivity::NativeShop{shop})=runtime.state.host_activity.as_ref()else{panic!()};
        let offer=shop.offers.iter().position(|offer|offer.category==0).unwrap();let item=shop.offers[offer].clone();
        let attribute=[0x4b6490,0x4b6492,0x4b648e,0x4b6494][item.bonus_attribute.unwrap()as usize];
        let old=runtime.state.legacy_player.as_ref().unwrap().i16(attribute).unwrap();let old_money=runtime.state.stats.money.unwrap();let mut expected_rng=runtime.state.rng.clone();
        assert_eq!(next(&mut runtime,"chooseOption",offer)["ok"],true);assert_eq!(runtime.state.housing.furniture,1);assert_eq!(runtime.state.stats.money,Some(old_money-i64::from(item.price)));
        assert_eq!(runtime.state.legacy_player.as_ref().unwrap().i16(attribute).unwrap(),old);assert_eq!(runtime.snapshot_value()["pending"]["returnTo"],"shop");
        let rng=runtime.state.rng.clone();let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusAction","action":"close"}))["ok"],false);assert_eq!(runtime.state.rng,rng);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusTick","elapsedMs":35}))["ok"],true);
        let state=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusTick","elapsedMs":965}))["ok"],true);
        let mut rain=crate::transition::NativeTransition::rain(1,2,&mut expected_rng);rain.tick(1000,&mut expected_rng);crate::transition::NativeTransition::mosaic(2,16,&mut expected_rng).tick(0,&mut expected_rng);assert_eq!(runtime.state.rng,expected_rng);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusAction","action":"close"}))["ok"],true);
        assert_eq!(runtime.state.legacy_player.as_ref().unwrap().i16(attribute).unwrap(),(i32::from(old)+i32::from(item.bonus_amount)).min(9999)as i16);
        let Some(HostActivity::NativeShop{shop})=runtime.state.host_activity.as_ref()else{panic!()};assert!(shop.offers[offer].purchased);let exit=shop.offers.len();
        assert_eq!(next(&mut runtime,"chooseOption",exit)["ok"],true);assert!(matches!(runtime.state.host_activity,Some(HostActivity::Housing{..})));
        assert_eq!(next(&mut runtime,"chooseOption",5)["ok"],true);assert_eq!(runtime.state.rng,expected_rng);
        let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);
        assert_eq!(next(&mut runtime,"chooseOption",0)["ok"],true);crate::transition::NativeTransition::mosaic(2,16,&mut expected_rng).tick(0,&mut expected_rng);
        assert_eq!(runtime.state.rng,expected_rng);assert_eq!(runtime.snapshot_value()["pending"]["returnTo"],"housing");
        let state=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"statusTick","elapsedMs":8001}))["ok"],true);
        assert!(runtime.state.vm.is_none());assert!(runtime.state.host_activity.is_none());assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.rng,expected_rng);
    }
    #[test]
    fn daily_inventory_commands_preserve_equipment_preview_and_confirmed_mutations(){
        let mut runtime=real_runtime();runtime.state.inventory.ordinary[0]=2;runtime.state.inventory.ordinary[2]=1;runtime.state.inventory.special[0]=2;
        runtime.state.carried_equipment[0]=EquipmentSlot{id:0,level:2};runtime.state.carried_equipment[3]=EquipmentSlot{id:3,level:3};
        runtime.state.known_equipment[0]=EquipmentKnowledge{learned:true,level:2};runtime.state.known_equipment[3]=EquipmentKnowledge{learned:true,level:3};
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b6486,30).unwrap();runtime.sync_player();let rng=runtime.state.rng.clone();let turn=runtime.state.turn;
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"openInventory"}))["ok"],true);
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("inventory.vlsave.json"),runtime.serialize()).unwrap();}
        let run=|runtime:&mut Runtime,action:&str,index:Option<usize>,tab:Option<&str>|{let seq=runtime.state.last_seq+1;dispatch(runtime,json!({"seq":seq,"type":"inventoryAction","action":action,"index":index,"tab":tab}))};
        for elapsed in [101,101]{let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"inventoryTick","elapsedMs":elapsed}))["ok"],true);}
        let state=runtime.snapshot_value();assert_eq!(state["pending"]["state"]["entries"][0]["frame"],1);assert_eq!(state["pending"]["state"]["entries"][1]["frame"],0);
        assert_eq!(run(&mut runtime,"use",None,None)["ok"],true);assert_eq!(runtime.state.inventory.ordinary[0],1);assert!(runtime.state.legacy_player.as_ref().unwrap().i16(0x4b6486).unwrap()>30);assert_eq!(runtime.snapshot_value()["pending"]["state"]["previewOrigin"],json!({"x":294,"y":40}));assert_eq!(runtime.snapshot_value()["pending"]["state"]["entries"][0]["frame"],0);
        assert_eq!(run(&mut runtime,"drop",None,None)["ok"],true);let save=runtime.serialize();let before=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,before);
        assert_eq!(run(&mut runtime,"use",None,None)["ok"],false);assert_eq!(runtime.state.inventory.ordinary[0],1);assert_eq!(run(&mut runtime,"cancelDrop",None,None)["ok"],true);assert_eq!(runtime.state.inventory.ordinary[0],1);
        assert_eq!(run(&mut runtime,"tab",None,Some("equipment"))["ok"],true);assert_eq!(runtime.snapshot_value()["pending"]["state"]["previewOrigin"]["x"],312);
        assert_eq!(run(&mut runtime,"wear",None,None)["ok"],true);assert_eq!(runtime.state.battle_persistent.as_ref().unwrap().equipped_ids[0],0);
        assert_eq!(run(&mut runtime,"unequip",None,None)["ok"],true);assert_eq!(runtime.state.battle_persistent.as_ref().unwrap().equipped_ids[0],-1);assert_eq!(run(&mut runtime,"wear",None,None)["ok"],true);
        assert_eq!(run(&mut runtime,"drop",None,None)["ok"],true);assert_eq!(run(&mut runtime,"confirmDrop",None,None)["ok"],true);assert_eq!(runtime.state.battle_persistent.as_ref().unwrap().equipped_ids[0],-1);assert_eq!(runtime.state.carried_equipment[0].id,-1);assert_eq!(runtime.state.carried_equipment[3].id,3);assert!(!runtime.state.known_equipment[0].learned);assert_eq!(runtime.snapshot_value()["pending"]["state"]["entries"][0]["id"],3);
        assert_eq!(run(&mut runtime,"tab",None,Some("special"))["ok"],true);assert_eq!(run(&mut runtime,"use",None,None)["ok"],false);assert_eq!(run(&mut runtime,"drop",None,None)["ok"],true);assert_eq!(run(&mut runtime,"confirmDrop",None,None)["ok"],true);assert_eq!(runtime.state.inventory.special[0],1);
        assert_eq!(run(&mut runtime,"close",None,None)["ok"],true);assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.turn,turn);assert_eq!(runtime.state.rng,rng);
    }
    #[test]
    fn saved_battle_and_inventory_indices_are_validated_before_restore(){
        let mut runtime=real_runtime();runtime.state.inventory.ordinary[0]=1;assert_eq!(serde_json::from_str::<Value>(&runtime.open_inventory(None)).unwrap()["ok"],true);
        for shape in 0..3{let mut malformed:Save=serde_json::from_str(&runtime.serialize()).unwrap();match shape{
            0=>malformed.state.battle_persistent.as_mut().unwrap().equipped_ids.clear(),
            1=>{let Some(HostActivity::Inventory{panel})=malformed.state.host_activity.as_mut()else{panic!()};panel.entries[0].id=13;},
            _=>{let Some(HostActivity::Inventory{panel})=malformed.state.host_activity.as_mut()else{panic!()};panel.selected_index=Some(99);},
        }malformed.checksum=digest(&serde_json::to_vec(&malformed.state).unwrap());let before=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&serde_json::to_string(&malformed).unwrap())).unwrap()["error"]["code"],"invalid_save");assert_eq!(runtime.state,before);}
        runtime.state.host_activity=None;runtime.state.pending=None;runtime.state.phase="playing".into();runtime.begin_final_battle().unwrap();
        let mut malformed:Save=serde_json::from_str(&runtime.serialize()).unwrap();let Some(HostActivity::FinalBattle{game})=malformed.state.host_activity.as_mut()else{panic!()};game.enemy_tracks.clear();malformed.checksum=digest(&serde_json::to_vec(&malformed.state).unwrap());let before=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&serde_json::to_string(&malformed).unwrap())).unwrap()["error"]["code"],"invalid_save");assert_eq!(runtime.state,before);
    }
    #[test]
    fn native_battle_dispatches_restores_and_commits_result_once(){
        let mut runtime=real_runtime();let node=(0..32768).find(|id|runtime.program.as_ref().unwrap().node_words(*id).is_some_and(|words|words[1]==15)).unwrap();
        runtime.state.vm=Some(LegacyVm::new(node));runtime.advance_vm(&mut vec![]);
        assert_eq!(runtime.snapshot_value()["pending"]["game"],"ordinary");assert!(matches!(runtime.state.host_activity,Some(HostActivity::Battle{..})));
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("battle.vlsave.json"),runtime.serialize()).unwrap();}
        let before=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,before);
        let player=runtime.state.legacy_player.as_ref().unwrap();let battles=player.i16(0x4b64bc).unwrap();let wins=player.i16(0x4b64be).unwrap();let total=player.i16(0x4b64f0).unwrap();
        let Some(HostActivity::Battle{game})=runtime.state.host_activity.as_mut()else{panic!()};game.phase=BattlePhase::Finished;game.return_code=Some(1);game.hp=80;game.money=12345;game.experience=321;game.persistent.visits=1;
        assert_eq!(dispatch(&mut runtime,json!({"seq":before.last_seq+1,"type":"battleTick","elapsedMs":17}))["ok"],true);
        let player=runtime.state.legacy_player.as_ref().unwrap();assert_eq!(player.i16(0x4b64bc).unwrap(),battles+1);assert_eq!(player.i16(0x4b64be).unwrap(),wins+1);assert_eq!(player.i16(0x4b64f0).unwrap(),total+1);assert_eq!(runtime.state.battle_persistent.as_ref().unwrap().visits,1);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleTick","elapsedMs":17}))["ok"],false);assert_eq!(runtime.state.legacy_player.as_ref().unwrap().i16(0x4b64f0).unwrap(),total+1);
        runtime.reincarnate(&mut vec![]).unwrap();assert!(runtime.state.battle_persistent.is_none());
    }
    #[test]
    fn final_battle_valuation_matches_original_x86_and_enters_the_existing_ending(){
        let mut runtime=real_runtime();let battle_rules=runtime.battle_rules().unwrap();let vectors:Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-final-battle.json")).unwrap();
        for row in vectors["valuations"].as_array().unwrap(){
            for field in row["playerFields"].as_array().unwrap(){let address=field[0].as_u64().unwrap()as u32;let value=field[1].as_i64().unwrap();if field[2]==4{runtime.state.legacy_player.as_mut().unwrap().set_i32(address,value as i32).unwrap();}else{runtime.state.legacy_player.as_mut().unwrap().set_i16(address,value as i16).unwrap();}}
            runtime.state.housing=HousingState{style:row["style"].as_i64().unwrap()as i8,furniture:row["furniture"].as_i64().unwrap()as i16,investment_stage:row["investmentStage"].as_u64().unwrap()as u8,purchase_years:vec![]};runtime.state.spouse_index=row["spouse"].as_i64().unwrap()as i8;
            let mut persistent=BattlePersistent::new(&battle_rules);for(enemy,wins)in persistent.enemies.iter_mut().zip(row["wins"].as_array().unwrap()){enemy.wins=wins.as_i64().unwrap()as i16;}runtime.state.battle_persistent=Some(persistent);
            assert_eq!(serde_json::to_value(runtime.final_battle_input().unwrap().valuation).unwrap(),row["valuation"]);
        }
        let mut runtime=real_runtime();let player=runtime.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b649e,6).unwrap();player.set_i16(0x4b64a2,1).unwrap();player.set_i16(0x4b64a4,30).unwrap();player.set_i16(0x4b64a6,5).unwrap();player.set_i16(0x4b64f2,3).unwrap();player.set_i32(0x4b648a,500000).unwrap();
        runtime.prepare_final_history().unwrap();runtime.begin_final_battle().unwrap();assert_eq!(runtime.snapshot_value()["pending"]["game"],"final");assert_eq!(runtime.state.stats.money,Some(0));
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();let initial=runtime.serialize();std::fs::write(directory.join("final-battle.vlsave.json"),&initial).unwrap();
            for _ in 0..1000{let phase=runtime.snapshot_value()["pending"]["state"]["phase"].as_str().unwrap().to_owned();if phase=="shop"{break;}let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleAction","action":"continue"}))["ok"],true);let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleTick","elapsedMs":101}))["ok"],true);}
            assert_eq!(runtime.snapshot_value()["pending"]["state"]["phase"],"shop");std::fs::write(directory.join("final-shop.vlsave.json"),runtime.serialize()).unwrap();
            for _ in 0..2{let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleAction","action":"continue"}))["ok"],true);let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleTick","elapsedMs":101}))["ok"],true);}
            for _ in 0..50{let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleTick","elapsedMs":17}))["ok"],true);}
            assert_eq!(runtime.snapshot_value()["pending"]["state"]["phase"],"playing");std::fs::write(directory.join("final-combat.vlsave.json"),runtime.serialize()).unwrap();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&initial)).unwrap()["ok"],true);
        }
        let before=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,before);
        let Some(HostActivity::FinalBattle{game})=runtime.state.host_activity.as_mut()else{panic!()};game.finished=true;game.hp=0;game.money=22222;
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"battleTick","elapsedMs":17}))["ok"],true);assert_eq!(runtime.state.stats.money,Some(22222));assert_eq!(runtime.snapshot_value()["pending"]["kind"],"ending");let Some(HostActivity::Ending{flow})=&runtime.state.host_activity else{panic!()};assert!(!flow.victory);
    }
    #[test]
    fn map_load_decoration_draws_precede_gods_and_match_original_x86(){
        let cases:Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-map-load.json")).unwrap();let mut runtime=real_runtime();
        for case in cases["cases"].as_array().unwrap(){
            let map=runtime.maps.iter().find(|map|map.id==case["map"].as_str().unwrap()).unwrap();let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            assert_eq!(serde_json::to_value(map.decoration_delays(&mut rng)).unwrap(),case["delaysMs"]);let spirits=Spirits::initialize(&map.coordinate_index,&map.start,&mut rng).unwrap();
            assert_eq!(serde_json::to_value(spirits.positions).unwrap(),case["positions"]);assert_eq!(rng.draws,case["rngDraws"].as_u64().unwrap());assert_eq!(u64::from(rng.state),case["rngState"].as_u64().unwrap());
            if case["map"]=="park" && case["seed"]==1{assert_eq!(runtime.state.rng,rng);assert_eq!(serde_json::to_value(&runtime.state.map_decorations.as_ref().unwrap().delays_ms).unwrap(),case["delaysMs"]);}
        }
        let before=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,before);
        runtime.transition_stage(3,&mut Vec::new()).unwrap();let before=runtime.state.rng.draws;let serial=runtime.state.map_decorations.as_ref().unwrap().load_serial;
        runtime.transition_stage(4,&mut Vec::new()).unwrap();assert_eq!(runtime.state.rng.draws-before,4);assert_eq!(runtime.state.map_decorations.as_ref().unwrap().load_serial,serial+1);
        let before=runtime.state.rng.draws;runtime.enter_travel(&mut Vec::new()).unwrap();assert_eq!(runtime.state.rng.draws-before,1+runtime.current_map().unwrap().decoration_count()as u64);
        let before=runtime.state.rng.draws;runtime.return_from_travel(&mut Vec::new()).unwrap();assert_eq!(runtime.state.rng.draws-before,runtime.current_map().unwrap().decoration_count()as u64);
    }
    #[test]
    fn ending_history_records_only_actual_transitions_and_resets_on_reincarnation(){
        let mut runtime=real_runtime();assert_eq!(runtime.ending_history()[1],[2016]);assert_eq!(runtime.ending_history()[2],[2019]);
        runtime.transition_stage(3,&mut Vec::new()).unwrap();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649c,2019).unwrap();runtime.complete_school_exam(1,&mut Vec::new()).unwrap();
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649c,2020).unwrap();runtime.complete_school_exam(1,&mut Vec::new()).unwrap();assert_eq!(runtime.ending_history()[3],[2019]);
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649c,2021).unwrap();runtime.complete_school_exam(2,&mut Vec::new()).unwrap();assert_eq!(runtime.ending_history()[4],[2021]);
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649c,2022).unwrap();runtime.transition_stage(6,&mut Vec::new()).unwrap();assert_eq!(runtime.ending_history()[5],[2022]);
        runtime.choose_career(4).unwrap();assert!(runtime.ending_history()[6].is_empty());runtime.choose_career(5).unwrap();runtime.choose_career(5).unwrap();assert_eq!(runtime.ending_history()[6],[2022]);
        runtime.state.promotion_years=vec![2023,2024];runtime.state.housing.purchase_years=vec![2025];runtime.state.relationship_years=[Some(2026),Some(2027),Some(2028)];
        let player=runtime.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b64f0,123).unwrap();player.set_i16(0x4b649c,2060).unwrap();runtime.prepare_final_history().unwrap();let history=runtime.ending_history();
        for(index,expected)in [(7,vec![2023,2024]),(8,vec![2025]),(10,vec![2028]),(11,vec![2026]),(12,vec![2027]),(14,vec![99]),(15,vec![2060])]{assert_eq!(history[index],expected);}
        let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.ending_history(),history);
        runtime.reincarnate(&mut Vec::new()).unwrap();assert_eq!(runtime.ending_history(),Runtime::initial_life_history());
    }
    #[test]
    fn original_map_facing_uses_column_then_row_and_restores_travel_heading(){
        let mut runtime=real_runtime();
        for(id,expected)in[("acient",1),("business",2),("desert",2),("lost",2),("park",0),("play",1),("social",3),("study",0)]{
            assert_eq!(runtime.maps.iter().find(|map|map.id==id).unwrap().initial_facing(),Some(expected),"{id}");
        }
        runtime.state.map_facing=Some(3);let tile=runtime.state.tile_id;runtime.enter_travel(&mut Vec::new()).unwrap();assert_eq!(runtime.state.travel_origin.as_ref().unwrap().facing,Some(3));
        let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);runtime.return_from_travel(&mut Vec::new()).unwrap();assert_eq!(runtime.state.map_facing,Some(3));assert_eq!(runtime.state.tile_id,tile);
        runtime.transition_stage(3,&mut Vec::new()).unwrap();runtime.state.map_facing=Some(2);runtime.transition_stage(4,&mut Vec::new()).unwrap();assert_eq!(runtime.state.map_facing,Some(2));
    }
    #[test]
    fn student_competition_saves_settles_once_and_resumes_queued_travel(){
        for traveling in [false,true]{
            let mut runtime=real_runtime();let player=runtime.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b64a8,6).unwrap();player.set_i16(0x4b64aa,6).unwrap();if traveling{player.set_i16(0x4b64b0,1).unwrap();}
            runtime.state.turn_in_progress=true;let mut events=Vec::new();runtime.finish_turn(&mut events);
            assert_eq!(runtime.state.turn,7);assert_eq!(events.iter().filter(|event|event["type"]=="turnCompleted").count(),1);assert!(!runtime.state.legacy_player.as_ref().unwrap().life().traveling);
            assert!(matches!(runtime.state.host_activity,Some(HostActivity::Competition{..})));assert!(!runtime.state.turn_in_progress);
            for _ in 0..200{if matches!(runtime.state.host_activity,Some(HostActivity::Competition{game:StudentCompetition{phase:CompetitionPhase::Menu,..},..})){break;}let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"competitionTick","seq":seq,"elapsedMs":100}))["ok"],true);}
            if !traveling&&std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures/competition.vlsave.json");std::fs::write(path,runtime.serialize()).unwrap();}
            let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"competitionAction","seq":seq,"action":"choose","index":2}))["ok"],true);
            let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"competitionAction","seq":seq,"action":"continue"}))["ok"],true);
            let mut restored=false;let mut prize_events=0;let mut result_events=0;
            for _ in 0..3000{
                let Some(HostActivity::Competition{game,..})=runtime.state.host_activity.as_ref()else{break;};
                let phase=game.phase;let answer=game.exam.as_ref().map(|exam|exam.correct).unwrap_or(0);
                if phase==CompetitionPhase::QuizFeedback&&!restored{let save=runtime.serialize();let state=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);restored=true;}
                let seq=runtime.state.last_seq+1;
                let response=if phase==CompetitionPhase::QuizQuestion{dispatch(&mut runtime,json!({"type":"competitionAction","seq":seq,"action":"answer","index":answer}))}else{dispatch(&mut runtime,json!({"type":"competitionTick","seq":seq,"elapsedMs":100}))};
                assert_eq!(response["ok"],true);prize_events+=response["events"].as_array().unwrap().iter().filter(|event|event["type"]=="competitionPrize").count();result_events+=response["events"].as_array().unwrap().iter().filter(|event|event["type"]=="competitionCompleted").count();
                assert!(!response["events"].as_array().unwrap().iter().any(|event|event["type"]=="turnCompleted"));
            }
            assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.turn,7);assert_eq!(runtime.state.stats.money,Some(20000));assert_eq!(prize_events,1);assert_eq!(result_events,1);
            assert_eq!(runtime.state.legacy_player.as_ref().unwrap().life().traveling,traveling);assert!(restored);
        }
    }
    #[test]
    fn running_competition_commands_restore_inputs_and_pay_only_once(){
        let mut runtime=real_runtime();let player=runtime.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b64a8,6).unwrap();player.set_i16(0x4b64aa,6).unwrap();
        runtime.state.turn_in_progress=true;runtime.finish_turn(&mut Vec::new());
        for _ in 0..200{if matches!(runtime.state.host_activity,Some(HostActivity::Competition{game:StudentCompetition{phase:CompetitionPhase::Menu,..},..})){break;}let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"competitionTick","seq":seq,"elapsedMs":100}))["ok"],true);}
        let draws=runtime.state.rng.draws;let action=|runtime:&mut Runtime,action:&str|{let seq=runtime.state.last_seq+1;dispatch(runtime,json!({"type":"competitionAction","seq":seq,"action":action}))};
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"competitionAction","seq":seq,"action":"choose","index":1}))["ok"],true);
        for _ in 0..6{assert_eq!(action(&mut runtime,"continue")["ok"],true);}assert_eq!(runtime.state.rng.draws,draws+3);
        assert_eq!(action(&mut runtime,"left")["ok"],true);let save=runtime.serialize();let before=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,before);
        let initial_money=runtime.state.stats.money.unwrap();let mut prizes=Vec::new();let mut completed=0;
        for frame in 0..3000{
            let Some(HostActivity::Competition{game,..})=runtime.state.host_activity.as_ref()else{break;};
            if game.phase==CompetitionPhase::Running{assert_eq!(action(&mut runtime,if frame%2==0{"right"}else{"left"})["ok"],true);}
            let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"type":"competitionTick","seq":seq,"elapsedMs":100}));assert_eq!(response["ok"],true);
            for event in response["events"].as_array().unwrap(){match event["type"].as_str(){Some("competitionPrize")=>prizes.push(event["amount"].as_i64().unwrap()),Some("competitionCompleted")=>completed+=1,Some("turnCompleted")=>panic!("competition settled the turn twice"),_=>{}}}
        }
        assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.turn,7);assert_eq!(prizes.len(),1);assert_eq!(completed,1);assert_eq!(runtime.state.stats.money,Some(initial_money+prizes[0]));assert_eq!(runtime.state.rng.draws,draws+3);
        let money=runtime.state.stats.money;assert_eq!(action(&mut runtime,"left")["ok"],false);assert_eq!(runtime.state.stats.money,money);
    }
    #[test]
    fn relationship_real_catalog_entry_produces_a_restorable_house(){
        let mut runtime=real_runtime();runtime.transition_stage(3,&mut Vec::new()).unwrap();runtime.refresh_spirit_positions(&mut Vec::new()).unwrap();
        let player=runtime.state.legacy_player.as_mut().unwrap();player.set_i16(0x4b6494,9999).unwrap();player.set_i16(0x4b6496,99).unwrap();player.set_i32(0x4b648a,1_000_000).unwrap();runtime.state.gift_level=2;runtime.sync_player();
        let node=(0..32768).find(|id|runtime.program.as_ref().unwrap().node_words(*id).is_some_and(|words|words[1]==24)).unwrap();
        runtime.state.vm=Some(LegacyVm::new(node));runtime.advance_vm(&mut Vec::new());assert_eq!(runtime.state.pending.as_ref().unwrap()["kind"],"relationship");
        let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){
            let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("relationship.vlsave.json"),save).unwrap();
        }
    }
    #[test]
    fn spirit_encounter_pauses_landed_tile_and_saved_slots_finish_before_turn_settlement(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649e,3).unwrap();let tile=runtime.state.tile_id.unwrap();let(_,position)=runtime.spirit_map_context().unwrap();
        runtime.state.spirits.positions[2]=position.clone();runtime.state.last_roll_steps=1;runtime.state.turn_in_progress=true;
        runtime.state.action_queue=vec![TileAction{tile_id:tile,event_id:0x5dec,landed:true,path:vec![tile],spirit_checked:false}];
        let mut events=Vec::new();runtime.run_actions(&mut events);assert!(matches!(runtime.state.host_activity,Some(HostActivity::SpiritEncounter{spirit:2})));assert!(!events.iter().any(|event|event["type"]=="tileTriggered"));
        assert_eq!(runtime.state.spirits.active,Some(2));assert_eq!(runtime.state.spirits.expires_turn,10);let money=runtime.state.stats.money.unwrap();let draws=runtime.state.rng.draws;
        next(&mut runtime,"chooseOption",0);assert_eq!(runtime.state.rng.draws,draws+4);
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){
            let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("spirit-slots.vlsave.json"),runtime.serialize()).unwrap();
            let mut controlled=real_runtime();controlled.transition_stage(3,&mut Vec::new()).unwrap();controlled.refresh_spirit_positions(&mut Vec::new()).unwrap();
            controlled.state.spirits.active=Some(0);controlled.state.spirits.expires_turn=8;controlled.state.legacy_player.as_mut().unwrap().set_i16(0x4b64d6,0).unwrap();controlled.sync_player();
            let save=controlled.serialize();assert_eq!(serde_json::from_str::<Value>(&controlled.restore(&save)).unwrap()["ok"],true);std::fs::write(directory.join("controlled-dice.vlsave.json"),save).unwrap();
        }
        let seq=runtime.state.last_seq+1;dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":"stop"}));assert_eq!(runtime.state.rng.draws,draws+5);
        let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);
        let mut triggered=0;for _ in 0..5000{if runtime.state.host_activity.is_none(){break;}let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"seq":seq,"type":"minigameTick","elapsedMs":17}));assert_eq!(response["ok"],true);triggered+=response["events"].as_array().unwrap().iter().filter(|event|event["type"]=="tileTriggered").count();}
        assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.turn,1);assert_eq!(triggered,1);assert!(runtime.state.stats.money.unwrap()>=money);
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b64aa,10).unwrap();runtime.state.turn_in_progress=true;runtime.finish_turn(&mut Vec::new());
        assert_eq!(runtime.state.spirits.active,None);assert_eq!(runtime.state.legacy_player.as_ref().unwrap().i16(0x4b64d6).unwrap(),-1);assert_ne!(runtime.state.spirits.positions[2],position);
    }
    #[test]
    fn relationship_meeting_saves_answers_pays_once_and_runs_all_native_followups(){
        let mut runtime=real_runtime();let player=runtime.state.legacy_player.as_mut().unwrap();
        player.set_i16(0x4b649e,3).unwrap();player.set_i16(0x4b6494,9999).unwrap();player.set_i32(0x4b648a,1_000_000).unwrap();runtime.sync_player();
        let mut nodes=vec![json!({"words":[30000,24,30001,0,0,0]}),json!({"words":[30001,12,0,0,0,0]})];for id in 4000..4008{nodes.push(json!({"words":[id,12,0,0,0,0]}));}
        runtime.program=Some(LegacyProgram::from_json(&json!({"program":{"nodes":nodes}}).to_string()).unwrap());
        runtime.state.relationship_new_targets[1]=7;runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        assert_eq!(runtime.state.pending.as_ref().unwrap()["kind"],"relationship");assert_eq!(relationship_action(&mut runtime,"meet",0)["ok"],true);
        let draws=runtime.state.rng.draws;let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"relationshipTick","elapsedMs":16,"hoverIndex":0}))["ok"],true);assert_eq!(runtime.state.rng.draws,draws+2);
        assert_eq!(relationship_action(&mut runtime,"selectOffer",0)["ok"],true);assert_eq!(runtime.state.rng.draws,draws+4);
        let Some(HostActivity::Relationship{house})=runtime.state.host_activity.as_ref()else{panic!()};let person=house.selected_person.unwrap();let cost=i64::from(house.people[person as usize].cost);let answer=house.answer_positions[0];
        assert_eq!(relationship_action(&mut runtime,"answer",answer)["ok"],true);let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
        let Some(HostActivity::Relationship{house})=runtime.state.host_activity.as_ref()else{panic!()};let wrong=(house.answer_positions[1]+1)%3;
        assert_eq!(relationship_action(&mut runtime,"answer",wrong)["ok"],true);assert_eq!(runtime.state.stats.money,Some(1_000_000-cost));assert_eq!(runtime.state.relationship_candidates[0],person as i8);
        let response=relationship_action(&mut runtime,"advanceNotice",0);assert_eq!(response["ok"],true);
        let completed=response["events"].as_array().unwrap().iter().find(|event|event["type"]=="relationshipCompleted").unwrap();assert_eq!(completed["eventNodes"],json!([4000+i32::from(person),4007]));
        assert_eq!(runtime.state.relationship_years[0],Some(2016));assert_eq!(runtime.state.phase,"playing");assert!(runtime.state.vm.is_none());
    }
    #[test]
    fn relationship_married_branch_skips_house_rng_and_marriage_exit_preserves_selection(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649e,6).unwrap();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b6496,99).unwrap();runtime.sync_player();
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,24,30001,0,0,0]},{"words":[30001,12,0,0,0,0]},{"words":[4050,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.relationship_candidates[0]=0;runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        assert_eq!(relationship_action(&mut runtime,"marriages",0)["ok"],true);assert_eq!(relationship_action(&mut runtime,"selectMarriage",0)["ok"],true);assert_eq!(runtime.state.spouse_index,0);
        assert_eq!(relationship_action(&mut runtime,"close",0)["ok"],true);assert_eq!(runtime.state.spouse_index,0);assert_eq!(runtime.state.relationship_years[2],None);
        let draws=runtime.state.rng.draws;runtime.state.vm=Some(LegacyVm::new(30000));let mut events=Vec::new();runtime.advance_vm(&mut events);
        assert_eq!(runtime.state.rng.draws,draws);assert!(runtime.state.host_activity.is_none());assert_eq!(runtime.state.phase,"playing");
        assert!(events.iter().any(|event|event["type"]=="relationshipCompleted" && event["returnKind"]==4 && event["eventNodes"]==json!([4050])));
    }
    #[test]
    fn native_travel_opcode_marks_mission_and_enters_the_original_travel_map(){
        let mut runtime=real_runtime();let original_map=runtime.state.map_id.clone();
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b64cc,999).unwrap();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b64ce,10).unwrap();
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,22,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.state.turn_in_progress=true;let mut events=Vec::new();runtime.advance_vm(&mut events);
        let player=runtime.state.legacy_player.as_ref().unwrap();assert!(player.life().traveling);assert_eq!(player.i16(0x4b64d0).unwrap(),1);
        assert_eq!(runtime.state.travel_origin.as_ref().unwrap().map_id,original_map.unwrap());assert!(events.iter().any(|event|event["type"]=="travelStarted"));
    }
    #[test]
    fn ending_skips_unavailable_original_career_art(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649e,6).unwrap();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b64a2,-1).unwrap();
        runtime.prepare_final_history().unwrap();runtime.complete_final_battle(false,&mut Vec::new()).unwrap();
        let Some(HostActivity::Ending{flow})=&runtime.state.host_activity else{panic!()};assert_eq!(flow.phase,EndingPhase::Movie);assert!(flow.summary.is_none());
        let before=runtime.state.rng.clone();let response=dispatch(&mut runtime,json!({"seq":2,"type":"endingTick","elapsedMs":201}));assert_eq!(response["ok"],true);assert_eq!(runtime.state.rng,before);
        let save=runtime.serialize();let expected=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,expected);
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649e,5).unwrap();runtime.complete_final_battle(true,&mut Vec::new()).unwrap();
        let Some(HostActivity::Ending{flow})=&runtime.state.host_activity else{panic!()};assert_eq!(flow.phase,EndingPhase::Summary);assert_eq!(flow.summary.as_ref().unwrap().career,25);
    }
    #[test]
    fn ending_defeat_reincarnates_and_victory_can_complete_or_continue(){
        fn finish_presentation(runtime:&mut Runtime){
            for _ in 0..100{
                let Some(HostActivity::Ending{flow})=&runtime.state.host_activity else{return;};
                let action=match flow.phase{EndingPhase::Reincarnation=>return,EndingPhase::Summary|EndingPhase::EndImage=>"continue",_=>"escape"};
                let result=dispatch(runtime,json!({"seq":runtime.state.last_seq+1,"type":"endingAction","action":action}));assert_eq!(result["ok"],true,"{result}");
                let save=runtime.serialize();let expected=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,expected);
                let result=dispatch(runtime,json!({"seq":runtime.state.last_seq+1,"type":"endingTick","elapsedMs":1000}));assert_eq!(result["ok"],true,"{result}");
            }panic!("Ending presentation did not finish");
        }
        let mut runtime=real_runtime();
        let player=runtime.state.legacy_player.as_mut().unwrap();
        player.set_i16(0x4b649e,6).unwrap();player.set_i16(0x4b64a2,1).unwrap();player.set_i16(0x4b64a6,5).unwrap();player.set_i16(0x4b6488,1000).unwrap();
        runtime.state.career_levels[1]=5;runtime.state.career_levels[2]=4;runtime.state.career_history.push(json!({"career":1}));runtime.state.promotion_years.push(2050);
        runtime.state.inventory.special[0]=5;runtime.state.inventory.ordinary[0]=5;runtime.state.spouse_index=3;runtime.state.relationship_candidates[0]=2;
        runtime.state.gift_level=4;runtime.state.fortune_visits=11;runtime.state.shop_visits=2;runtime.state.housing.style=1;runtime.state.housing.investment_stage=12;
        runtime.state.known_equipment[0]=EquipmentKnowledge{learned:true,level:2};runtime.state.carried_equipment[0]=EquipmentSlot{id:0,level:2};runtime.state.first_equipment_visit=false;
        runtime.prepare_final_history().unwrap();runtime.complete_final_battle(false,&mut Vec::new()).unwrap();
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("ending-loss.vlsave.json"),runtime.serialize()).unwrap();}
        finish_presentation(&mut runtime);
        assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.legacy_player.as_ref().unwrap().life().stage,2);assert_eq!(runtime.state.legacy_player.as_ref().unwrap().life().incarnation,2);
        assert_eq!(runtime.state.legacy_player.as_ref().unwrap().life().skills[1],3);assert_eq!(runtime.state.career_levels[1],5);assert_eq!(runtime.state.career_levels[2],1);
        assert_eq!(runtime.state.inventory,Inventory::default());assert!(runtime.state.career_history.is_empty());assert!(runtime.state.promotion_years.is_empty());
        assert_eq!(runtime.state.spouse_index,-1);assert!(runtime.state.relationship_candidates.iter().all(|id|*id==-1));assert_eq!(runtime.state.gift_level,1);assert_eq!(runtime.state.fortune_visits,0);
        assert_eq!(runtime.state.housing,HousingState::default());assert!(runtime.state.known_equipment.iter().all(|item|!item.learned && item.level==0));assert!(runtime.state.carried_equipment.iter().all(|item|item.id==-1));assert!(runtime.state.first_equipment_visit);
        runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b649e,6).unwrap();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b64a2,1).unwrap();runtime.prepare_final_history().unwrap();runtime.complete_final_battle(true,&mut Vec::new()).unwrap();
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("ending-win.vlsave.json"),runtime.serialize()).unwrap();}
        finish_presentation(&mut runtime);
        let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
        assert_eq!(next(&mut runtime,"chooseOption",1)["ok"],true);assert_eq!(runtime.state.phase,"completed");
        let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
        assert_eq!(next(&mut runtime,"rollDice",0)["error"]["code"],"cannot_roll");
        runtime.complete_final_battle(true,&mut Vec::new()).unwrap();finish_presentation(&mut runtime);next(&mut runtime,"chooseOption",0);
        assert_eq!(runtime.state.phase,"playing");assert_eq!(runtime.state.legacy_player.as_ref().unwrap().life().incarnation,3);
    }
    #[test]
    fn fire_commands_commit_charm_when_reward_starts_and_preserve_saved_animation(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b6494,1000).unwrap();runtime.sync_player();let seed=(0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==3).unwrap();
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){
            let mut fixture=real_runtime();fixture.state.rng=ReplayRng::new(seed);let node=(0..32768).find(|id|fixture.program.as_ref().unwrap().node_words(*id).is_some_and(|words|words[1]==23)).unwrap();
            fixture.state.vm=Some(LegacyVm::new(node));fixture.advance_vm(&mut Vec::new());assert_eq!(fixture.state.pending.as_ref().unwrap()["game"],"fire");
            let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("fire.vlsave.json"),fixture.serialize()).unwrap();
        }
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());runtime.state.rng=ReplayRng::new(seed);runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":"start"}))["ok"],true);let mut rewarded=false;let mut completed=false;
        for _ in 0..2000{
            let Some(HostActivity::Fire{game})=runtime.state.host_activity.as_ref()else{panic!()};
            if game.win_stage==0{let action=if game.hand{"right"}else{"left"};let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":action}))["ok"],true);}
            let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"seq":seq,"type":"minigameTick","elapsedMs":17}));assert_eq!(response["ok"],true);
            if response["snapshot"]["pending"]["state"]["rewardStarted"]==true && !rewarded{
                assert_eq!(runtime.state.stats.charm,Some(1100));let save=runtime.serialize();let state=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);rewarded=true;
            }
            if let Some(event)=response["events"].as_array().unwrap().iter().find(|event|event["type"]=="minigameCompleted" && event["game"]=="fire"){assert_eq!(event["charmDelta"],100);assert_eq!(event["failed"],false);completed=true;break;}
        }
        assert!(rewarded && completed);assert!(runtime.state.host_activity.is_none());assert!(runtime.state.vm.is_none());assert_eq!(runtime.state.phase,"playing");
    }
    #[test]
    fn gas_commands_restore_during_a_throw_and_resume_after_three_successes(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b648e,1000).unwrap();runtime.sync_player();
        let seed=(0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==0).unwrap();
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){
            let mut fixture=real_runtime();fixture.state.rng=ReplayRng::new(seed);let node=(0..32768).find(|id|fixture.program.as_ref().unwrap().node_words(*id).is_some_and(|words|words[1]==23)).unwrap();
            fixture.state.vm=Some(LegacyVm::new(node));fixture.advance_vm(&mut Vec::new());assert_eq!(fixture.state.pending.as_ref().unwrap()["game"],"gas");
            let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("gas.vlsave.json"),fixture.serialize()).unwrap();
        }
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.rng=ReplayRng::new(seed);runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        assert_eq!(runtime.state.pending.as_ref().unwrap()["game"],"gas");let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":"start"}))["ok"],true);
        let mut saved=false;let mut completed=false;
        for _ in 0..3000{
            let Some(HostActivity::Gas{game})=runtime.state.host_activity.as_ref()else{panic!()};
            if game.phase==GasPhase::Hit && !saved{let state=runtime.state.clone();let save=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);saved=true;}
            let Some(HostActivity::Gas{game})=runtime.state.host_activity.as_ref()else{panic!()};
            if game.phase==GasPhase::Playing && !game.ending_pending && game.player_state!=1 && (261..=280).contains(&game.targets[game.target_state].x){let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":"throw"}))["ok"],true);}
            let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"seq":seq,"type":"minigameTick","elapsedMs":17}));assert_eq!(response["ok"],true);
            if let Some(event)=response["events"].as_array().unwrap().iter().find(|event|event["type"]=="minigameCompleted" && event["game"]=="gas"){assert_eq!(event["successes"],3);assert_eq!(event["staminaDelta"],100);completed=true;break;}
        }
        assert!(saved && completed);assert!(runtime.state.host_activity.is_none());assert!(runtime.state.vm.is_none());assert_eq!(runtime.state.phase,"playing");assert!(runtime.state.gas_target_state.is_some());
    }
    #[test]
    fn firebee_commands_save_inputs_pause_and_pay_after_reward(){
        let mut runtime=real_runtime();let seed=(0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==4).unwrap();
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){let mut fixture=real_runtime();fixture.state.rng=ReplayRng::new(seed);let node=(0..32768).find(|id|fixture.program.as_ref().unwrap().node_words(*id).is_some_and(|words|words[1]==23)).unwrap();fixture.state.vm=Some(LegacyVm::new(node));fixture.advance_vm(&mut Vec::new());let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("firebee.vlsave.json"),fixture.serialize()).unwrap();}
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());runtime.state.rng=ReplayRng::new(seed);runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());assert_eq!(runtime.state.pending.as_ref().unwrap()["game"],"firebee");
        let action=|runtime:&mut Runtime,value:Value|{let mut value=value;value["type"]=json!("minigameAction");value["seq"]=json!(runtime.state.last_seq+1);dispatch(runtime,value)};
        assert_eq!(action(&mut runtime,json!({"action":"start"}))["ok"],true);assert_eq!(action(&mut runtime,json!({"action":"input","left":true,"fire":true}))["ok"],true);
        let save=runtime.serialize();let state=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,state);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":400}))["ok"],true);
        assert_eq!(action(&mut runtime,json!({"action":"help"}))["ok"],true);let Some(HostActivity::Firebee{game})=runtime.state.host_activity.as_ref()else{panic!()};let player_x=game.player_x;let row_y=game.row_y;
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":5000}))["ok"],true);let Some(HostActivity::Firebee{game})=runtime.state.host_activity.as_ref()else{panic!()};assert_eq!(game.player_x,player_x);assert_eq!(game.row_y,row_y);
        assert_eq!(action(&mut runtime,json!({"action":"start"}))["ok"],true);let Some(HostActivity::Firebee{game})=runtime.state.host_activity.as_mut()else{panic!()};game.finished=true;game.score=12;
        let mut completed=false;let mut reward_seen=false;
        for _ in 0..1000{let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":100}));assert_eq!(response["ok"],true);if response["snapshot"]["pending"]["state"]["phase"]=="reward"{reward_seen=true;assert_eq!(runtime.state.stats.money,Some(10000));}if let Some(event)=response["events"].as_array().unwrap().iter().find(|event|event["type"]=="minigameCompleted" && event["game"]=="firebee"){assert_eq!(event["moneyDelta"],132);completed=true;break;}}
        assert!(reward_seen&&completed);assert_eq!(runtime.state.stats.money,Some(10132));assert!(runtime.state.vm.is_none());assert_eq!(runtime.state.phase,"playing");
    }
    #[test]
    fn fist_commands_save_held_inputs_and_settle_only_after_original_reward(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i16(0x4b6490,1000).unwrap();runtime.sync_player();
        if std::env::var_os("VIRTUAL_LIFE_EXPORT_FIXTURES").is_some(){
            let mut fixture=real_runtime();fixture.state.rng=ReplayRng::new((0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==5).unwrap());
            let node=(0..32768).find(|id|fixture.program.as_ref().unwrap().node_words(*id).is_some_and(|words|words[1]==23)).unwrap();
            fixture.state.vm=Some(LegacyVm::new(node));fixture.advance_vm(&mut Vec::new());assert_eq!(fixture.state.pending.as_ref().unwrap()["game"],"fist");
            let directory=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/test-fixtures");std::fs::create_dir_all(&directory).unwrap();std::fs::write(directory.join("fist.vlsave.json"),fixture.serialize()).unwrap();
        }
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.rng=ReplayRng::new((0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==5).unwrap());
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());assert_eq!(runtime.state.pending.as_ref().unwrap()["game"],"fist");
        let action=|runtime:&mut Runtime,value:Value|{let mut value=value;value["type"]=json!("minigameAction");value["seq"]=json!(runtime.state.last_seq+1);dispatch(runtime,value)};
        assert_eq!(action(&mut runtime,json!({"action":"start"}))["ok"],true);
        assert_eq!(action(&mut runtime,json!({"action":"input","left":true}))["ok"],true);
        let saved=runtime.serialize();let state=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);assert_eq!(runtime.state,state);
        let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":16}));assert_eq!(response["snapshot"]["pending"]["state"]["playerX"],256);
        assert_eq!(action(&mut runtime,json!({"action":"help"}))["ok"],true);
        let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":5000}));assert_eq!(response["snapshot"]["pending"]["state"]["remainingSeconds"],60);
        assert_eq!(action(&mut runtime,json!({"action":"start"}))["ok"],true);
        let Some(HostActivity::Fist{game})=runtime.state.host_activity.as_mut()else{panic!()};game.remaining_seconds=1;game.score=20;
        let mut completed=false;let mut reward_seen=false;
        for _ in 0..1000{
            let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":101}));assert_eq!(response["ok"],true);
            if response["snapshot"]["pending"]["state"]["phase"]=="reward"{reward_seen=true;assert_eq!(runtime.state.stats.morality,Some(1000));}
            if let Some(event)=response["events"].as_array().unwrap().iter().find(|event|event["type"]=="minigameCompleted" && event["game"]=="fist"){
                assert_eq!(event["moralityDelta"],30);completed=true;break;
            }
        }
        assert!(reward_seen && completed);assert!(runtime.state.host_activity.is_none());assert!(runtime.state.vm.is_none());assert_eq!(runtime.state.phase,"playing");
    }
    #[test]
    fn hammer_commands_preserve_a_saved_game_and_resume_the_native_event(){
        let mut runtime=real_runtime();
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.rng=ReplayRng::new((0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==2).unwrap());
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        assert_eq!(runtime.state.pending.as_ref().unwrap()["game"],"hammer");
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":"start"}))["ok"],true);
        let Some(HostActivity::Hammer{game})=runtime.state.host_activity.as_mut()else{panic!()};game.targets[0].phase=2;
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameAction","action":"strikeIndex","index":0}))["snapshot"]["pending"]["state"]["score"],1);
        let saved=runtime.serialize();let expected=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);assert_eq!(runtime.state,expected);
        let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"seq":seq,"type":"minigameTick","elapsedMs":45501}))["snapshot"]["pending"]["state"]["phase"],"lost");
        let mut completed=false;
        for _ in 0..33{let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"seq":seq,"type":"minigameTick","elapsedMs":31}));assert_eq!(response["ok"],true);completed|=response["events"].as_array().unwrap().iter().any(|event|event["type"]=="minigameCompleted" && event["game"]=="hammer");}
        assert!(completed);assert!(runtime.state.host_activity.is_none());assert!(runtime.state.vm.is_none());assert_eq!(runtime.state.phase,"playing");
    }
    #[test]
    fn original_equipment_purchase_then_pickup_preserves_separate_ownership(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i32(0x4b648a,1_000_000).unwrap();runtime.sync_player();
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,27,30001,0,0,0]},{"words":[30001,17,30002,0,0,0]},{"words":[30002,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        let Some(HostActivity::EquipmentWorkshop{workshop,..})=runtime.state.host_activity.as_ref()else{panic!()};let id=workshop.offered_id;
        assert_eq!(next(&mut runtime,"chooseOption",0)["ok"],true);assert!(runtime.state.known_equipment[id].learned);assert!(runtime.state.carried_equipment.iter().all(|slot|slot.id==-1));
        let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
        assert_eq!(next(&mut runtime,"chooseOption",0)["ok"],true);
        let Some(HostActivity::NativeShop{shop})=runtime.state.host_activity.as_ref()else{panic!()};let offer=shop.offers.iter().position(|offer|offer.category==1 && offer.inventory_id==id as i16).unwrap();
        let previous=runtime.state.stats.money;assert_eq!(next(&mut runtime,"chooseOption",offer)["ok"],true);assert_eq!(runtime.state.stats.money,previous);assert_eq!(runtime.state.carried_equipment[0].id,id as i16);
    }
    #[test]
    fn original_fortune_cancel_has_no_stat_effect_and_counts_the_visit(){
        let mut runtime=real_runtime();runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,25,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
        runtime.state.rng=ReplayRng::new((0..100).find(|seed|ReplayRng::new(*seed).next_rand()%50==0).unwrap());
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        let Some(HostActivity::Fortune{game})=runtime.state.host_activity.as_ref()else{panic!()};let cancel=game.record.questions[0].options.len();
        let previous=runtime.state.stats.clone();let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
        let result=next(&mut runtime,"chooseOption",cancel);assert_eq!(result["snapshot"]["phase"],"playing");assert_eq!(runtime.state.stats,previous);assert_eq!(runtime.state.fortune_visits,1);
    }
    #[test]
    fn tracking_commands_restore_mid_shuffle_and_resume_the_original_event(){
        let mut runtime=real_runtime();
        runtime.program=Some(LegacyProgram::from_json(r#"{"program":{"nodes":[{"words":[30000,23,30001,0,0,0]},{"words":[30001,12,0,0,0,0]}]}}"#).unwrap());
        let seed=(0..100).find(|seed|ReplayRng::new(*seed).next_rand()%6==1).unwrap();runtime.state.rng=ReplayRng::new(seed);
        runtime.state.vm=Some(LegacyVm::new(30000));runtime.advance_vm(&mut Vec::new());
        assert_eq!(runtime.state.pending.as_ref().unwrap()["kind"],"minigame");
        let initial=runtime.state.stats.intelligence.unwrap();
        for round in 1..=3{
            let mut ticks=0;
            loop{
                let Some(HostActivity::Tracking{game})=runtime.state.host_activity.as_ref()else{panic!()};
                if game.phase==TrackingPhase::Memorize{break;}
                let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":16}))["ok"],true);
                ticks+=1;assert!(ticks<400);
            }
            let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameAction","seq":seq,"action":"startShuffle"}))["ok"],true);
            let save=runtime.serialize();let before=runtime.state.clone();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&save)).unwrap()["ok"],true);assert_eq!(runtime.state,before);
            ticks=0;
            let answer=loop{
                let Some(HostActivity::Tracking{game})=runtime.state.host_activity.as_ref()else{panic!()};
                if game.phase==TrackingPhase::Choose{break game.target;}
                let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":16}))["ok"],true);
                ticks+=1;assert!(ticks<500);
            };
            let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameAction","seq":seq,"action":"choose","index":answer}))["ok"],true);
            let seq=runtime.state.last_seq+1;let response=dispatch(&mut runtime,json!({"type":"minigameAction","seq":seq,"action":"nextRound"}));assert_eq!(response["ok"],true);
            if round<3{assert_eq!(response["snapshot"]["pending"]["state"]["round"],round+1);}else{assert_eq!(response["snapshot"]["pending"]["state"]["phase"],"reward");}
        }
        let draws=runtime.state.rng.draws;
        for _ in 0..500{if runtime.state.host_activity.is_none(){break;}let seq=runtime.state.last_seq+1;assert_eq!(dispatch(&mut runtime,json!({"type":"minigameTick","seq":seq,"elapsedMs":17}))["ok"],true);}
        assert!(runtime.state.rng.draws>draws);assert_eq!(runtime.state.phase,"playing");
        assert_eq!(runtime.state.stats.intelligence,Some(initial+initial/10));assert!(runtime.state.vm.is_none());
    }
    #[test]
    fn original_attribute_shop_enforces_payment_capacity_and_saved_sold_state(){
        let mut runtime=real_runtime();runtime.state.legacy_player.as_mut().unwrap().set_i32(0x4b648a,100_000).unwrap();runtime.sync_player();
        runtime.begin_stat_shop().unwrap();
        let before=runtime.state.stats.clone();
        let Some(HostActivity::StatShop{shop})=runtime.state.host_activity.as_ref()else{panic!()};
        let offer=shop.offers[0].clone();
        assert_eq!(next(&mut runtime,"chooseOption",0)["ok"],true);
        assert_eq!(runtime.state.stats.money,Some(before.money.unwrap()-i64::from(offer.item.price)));
        assert_eq!(runtime.state.stats.charm,Some(before.charm.unwrap()+i64::from(offer.item.amount)));
        assert_eq!(next(&mut runtime,"chooseOption",0)["error"]["code"],"cannot_purchase");
        assert_eq!(next(&mut runtime,"chooseOption",2)["ok"],true);assert_eq!(next(&mut runtime,"chooseOption",4)["ok"],true);
        assert_eq!(next(&mut runtime,"chooseOption",6)["error"]["code"],"cannot_purchase");
        let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
    }
    #[test]
    fn original_graduation_exam_passes_to_university_and_preserves_extra_question_draw(){
        let mut runtime=real_runtime();runtime.transition_stage(3,&mut Vec::new()).unwrap();runtime.begin_school_exam().unwrap();
        assert_eq!(next(&mut runtime,"chooseOption",0)["ok"],true);
        for _ in 0..9{
            let Some(HostActivity::SchoolExam{exam})=runtime.state.host_activity.as_ref()else{panic!()};let answer=exam.correct;
            assert_eq!(next(&mut runtime,"chooseOption",answer)["ok"],true);
            let saved=runtime.serialize();assert_eq!(serde_json::from_str::<Value>(&runtime.restore(&saved)).unwrap()["ok"],true);
        }
        let Some(HostActivity::SchoolExam{exam})=runtime.state.host_activity.as_ref()else{panic!()};let answer=exam.correct;
        let previous_draws=runtime.state.rng.draws;let result=next(&mut runtime,"chooseOption",answer);
        assert_eq!(result["snapshot"]["life"]["stage"],4);assert_eq!(result["snapshot"]["mapId"],"study");
        assert_eq!(runtime.state.legacy_player.as_ref().unwrap().i16(0x4b64b8).unwrap(),1);
        assert!(runtime.state.rng.draws>=previous_draws+2);
    }
    #[test]
    fn ordered_commands_and_explicit_graph_edges() {
        let mut runtime=Runtime::new(CONTENT).unwrap();
        assert_eq!(dispatch(&mut runtime,json!({"seq":2,"type":"exploreMap","mapId":"test"}))["error"]["code"],"out_of_order");
        assert_eq!(dispatch(&mut runtime,json!({"seq":1,"type":"exploreMap","mapId":"test"}))["snapshot"]["tileId"],0);
        assert_eq!(dispatch(&mut runtime,json!({"seq":2,"type":"followLink","branch":0}))["snapshot"]["tileId"],1);
        assert_eq!(dispatch(&mut runtime,json!({"seq":3,"type":"followLink","branch":1}))["error"]["code"],"invalid_link");
        assert_eq!(runtime.state.turn,0);
    }
    #[test]
    fn save_round_trip_and_corruption_are_atomic() {
        let mut original=Runtime::new(CONTENT).unwrap();
        dispatch(&mut original,json!({"seq":1,"type":"exploreMap","mapId":"test"}));
        let save=original.serialize(); let mut restored=Runtime::new(CONTENT).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&restored.restore(&save)).unwrap()["ok"],true);
        assert_eq!(original.state,restored.state);
        let before=restored.state.clone(); let broken=save.replace("\"turn\":0","\"turn\":7");
        assert_eq!(serde_json::from_str::<Value>(&restored.restore(&broken)).unwrap()["error"]["code"],"save_checksum");
        assert_eq!(before,restored.state);
        for shape in 0..3{
            let mut malformed:Save=serde_json::from_str(&save).unwrap();
            match shape{0=>malformed.state.inventory.ordinary.clear(),1=>malformed.state.carried_equipment[0].id=18,_=>malformed.state.housing.style=4}
            malformed.checksum=digest(&serde_json::to_vec(&malformed.state).unwrap());
            assert_eq!(serde_json::from_str::<Value>(&restored.restore(&serde_json::to_string(&malformed).unwrap())).unwrap()["error"]["code"],"invalid_save");
            assert_eq!(before,restored.state);
        }
    }
    #[test]
    fn unsupported_gameplay_does_not_invent_state() {
        let mut runtime=Runtime::new(CONTENT).unwrap();
        let result=dispatch(&mut runtime,json!({"seq":1,"type":"newGame","seed":1,"profile":{"name":"测试","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}}));
        assert_eq!(result["error"]["code"],"missing_events");
        assert_eq!(runtime.state.stats,Stats::default()); assert!(runtime.state.player.is_none());
        assert_eq!(runtime.state.rng.draws,0);
    }
}
