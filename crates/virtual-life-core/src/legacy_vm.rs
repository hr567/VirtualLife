//! Interpreter for the original N1 Group event program.
//!
//! Recovered from N2Game.exe FUN_0040c2e0 and its resource/rendering helpers.
//! The original loader scans for the first matching signed node ID; duplicate
//! IDs are intentionally retained with first-match semantics. Native gameplay
//! calls yield explicit host requests rather than being silently skipped.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
struct WordRecord { words: Vec<u16> }
#[derive(Clone, Debug, Default, Deserialize)]
struct WordTable { #[serde(default)] records: Vec<WordRecord> }
#[derive(Clone, Debug, Deserialize)]
struct TextRecord { id: u16, #[serde(default)] text: String }
#[derive(Clone, Debug, Default, Deserialize)]
struct TextTable { #[serde(default)] records: Vec<TextRecord> }
#[derive(Clone, Debug, Default, Deserialize)]
struct ProgramTable { #[serde(default)] nodes: Vec<WordRecord> }
#[derive(Clone, Debug, Deserialize)]
struct EventCatalog {
    program: ProgramTable,
    #[serde(default)] dialogues: TextTable,
    #[serde(default)] portraits: TextTable,
    #[serde(default)] backgrounds: TextTable,
    #[serde(default)] abc: WordTable,
    #[serde(default)] conditions: WordTable,
    #[serde(default)] results: WordTable,
}

#[derive(Clone, Debug)]
pub struct LegacyProgram {
    nodes: BTreeMap<i32, [u16; 6]>,
    dialogues: BTreeMap<i32, String>,
    portraits: BTreeMap<i32, String>,
    backgrounds: BTreeMap<i32, String>,
    choices: BTreeMap<i32, [u16; 5]>,
    conditions: BTreeMap<i32, Vec<u16>>,
    results: BTreeMap<i32, Vec<u16>>,
    pub source_node_count: usize,
    pub duplicate_node_count: usize,
}

fn signed(word: u16) -> i32 { word as i16 as i32 }
fn int32(words: &[u16], index: usize) -> i32 {
    (u32::from(words[index]) | (u32::from(words[index + 1]) << 16)) as i32
}

impl LegacyProgram {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let catalog: EventCatalog = serde_json::from_str(json).map_err(|e| format!("Invalid original event catalog: {e}"))?;
        let source_node_count = catalog.program.nodes.len();
        let mut nodes = BTreeMap::new();
        for record in catalog.program.nodes {
            let words: [u16; 6] = record.words.try_into().map_err(|_| "N1 node must contain six uint16 words")?;
            nodes.entry(signed(words[0])).or_insert(words);
        }
        let mut choices = BTreeMap::new();
        for record in catalog.abc.records {
            let words: [u16; 5] = record.words.try_into().map_err(|_| "ABC choice record must contain five uint16 words")?;
            choices.entry(signed(words[0])).or_insert(words);
        }
        fn text_index(table: TextTable) -> BTreeMap<i32, String> {
            let mut result = BTreeMap::new();
            for record in table.records { result.entry(signed(record.id)).or_insert(record.text); }
            result
        }
        fn words_index(table: WordTable, width: usize) -> Result<BTreeMap<i32, Vec<u16>>, String> {
            let mut result = BTreeMap::new();
            for record in table.records {
                if record.words.len() != width { return Err(format!("Expected a {width}-word legacy table record")); }
                result.entry(signed(record.words[0])).or_insert(record.words);
            }
            Ok(result)
        }
        Ok(Self {
            duplicate_node_count: source_node_count - nodes.len(), source_node_count,
            nodes, choices, dialogues: text_index(catalog.dialogues), portraits: text_index(catalog.portraits), backgrounds: text_index(catalog.backgrounds),
            conditions: words_index(catalog.conditions, 29)?, results: words_index(catalog.results, 59)?,
        })
    }

    pub fn has_node(&self, id: i32) -> bool { self.nodes.contains_key(&id) }
    pub fn node_words(&self, id: i32) -> Option<[u16; 6]> { self.nodes.get(&id).copied() }
    pub fn dialogue(&self, id: i32) -> Option<&str> { self.dialogues.get(&id).map(String::as_str) }
    pub fn background_resource(&self, id: i32) -> Option<String> {
        self.backgrounds.get(&id).map(|name| format!("install/bmp/{}", name.replace('\\', "/").to_lowercase()))
    }
    pub fn portrait_resource(&self, id: i32) -> Option<String> {
        self.portraits.get(&id).map(|name| format!("install/bmp/{}", name.replace('\\', "/").to_lowercase()))
    }
    pub fn result_delta(&self, id: i32) -> Result<ResultDelta, String> {
        let words = self.results.get(&id).ok_or_else(|| format!("Original result record {id} is missing"))?;
        Ok(ResultDelta::from_words(id, words))
    }
    pub fn condition_setup(&self, id: i32) -> Result<ConditionSetup, String> {
        let words = self.conditions.get(&id).ok_or_else(|| format!("Original condition record {id} is missing"))?;
        Ok(ConditionSetup::from_words(id, words))
    }
    /// FUN_0040bc30 is the mission reward handler, distinct from opcode 12.
    /// Pass 1 when the task is inactive; only positive rewards are multiplied.
    pub fn mission_result_effects(&self, id: i32, multiplier: i32, read_memory: &dyn Fn(u32) -> Option<i32>) -> Result<MissionResultEffects, String> {
        let words = self.results.get(&id).ok_or_else(|| format!("Original mission result record {id} is missing"))?;
        MissionResultEffects::from_words(id, words, multiplier, read_memory)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VariableDelta {
    pub address: u32,
    pub name: String,
    pub delta: i32,
    pub wrapping_bits: u8,
    pub minimum: i32,
    pub maximum: i32,
}

impl VariableDelta {
    /// Original code stores to its int16/int32 field before calling its clamp.
    pub fn apply(&self, current: i32) -> i32 {
        let value = if self.wrapping_bits == 16 {
            (current as i16).wrapping_add(self.delta as i16) as i32
        } else { current.wrapping_add(self.delta) };
        value.clamp(self.minimum, self.maximum)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResultDelta {
    pub result_id: i32,
    pub changes: Vec<VariableDelta>,
    /// FUN_0040bb10: result word 15 == 21 unlocks career 24 unless already there.
    pub unlock_career: Option<i16>,
    pub original_words: Vec<u16>,
}

impl ResultDelta {
    fn from_words(id: i32, words: &[u16]) -> Self {
        let mut changes = Vec::new();
        let money = int32(words, 7);
        if money != -4096 { changes.push(VariableDelta { address: 0x004b648a, name: "money".into(), delta: money, wrapping_bits: 32, minimum: 0, maximum: 99_999_999 }); }
        for (index, address, name) in [(9, 0x004b6492, "intelligence"), (10, 0x004b6494, "charm"), (11, 0x004b6490, "morality"), (12, 0x004b648e, "stamina")] {
            let delta = signed(words[index]);
            if delta != -4096 { changes.push(VariableDelta { address, name: name.into(), delta: delta * 10, wrapping_bits: 16, minimum: 0, maximum: 9999 }); }
        }
        let romance = int32(words, 31);
        if romance != -4096 { changes.push(VariableDelta { address: 0x004b6496, name: "romance".into(), delta: romance as i16 as i32, wrapping_bits: 16, minimum: 0, maximum: 99 }); }
        Self { result_id: id, changes, unlock_career: (signed(words[15]) == 21).then_some(24), original_words: words.to_vec() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConditionTarget { pub base_address: u32, pub delta: i32 }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissionSetup { pub kind: i32, pub target: i32, pub progress: i32 }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConditionSetup {
    pub condition_id: i32,
    pub deadline_base_address: u32,
    pub deadline_delta: i32,
    pub stat_target: Option<ConditionTarget>,
    pub mission: Option<MissionSetup>,
    pub original_words: Vec<u16>,
}
impl ConditionSetup {
    fn from_words(id: i32, words: &[u16]) -> Self {
        // Preserve the original ordered early returns in FUN_0040b9e0.
        let target = [(0x004b6492, signed(words[2])), (0x004b6490, signed(words[3])), (0x004b648a, int32(words, 5)), (0x004b648e, signed(words[7])), (0x004b6494, signed(words[8]))]
            .into_iter().find(|(_, delta)| *delta > 0).map(|(base_address, delta)| ConditionTarget { base_address, delta });
        let kind = signed(words[24]);
        let mission = if target.is_none() && kind > 0 {
            Some(MissionSetup { kind, target: match kind { 2 => signed(words[22]), 11 => signed(words[25]), _ => 999 }, progress: 0 })
        } else { None };
        Self { condition_id: id, deadline_base_address: 0x004b64aa, deadline_delta: signed(words[23]) + 1, stat_target: target, mission, original_words: words.to_vec() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveCondition {
    pub setup: ConditionSetup,
    pub deadline: i32,
    pub stat_threshold: Option<i32>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConditionOutcome { Waiting, Succeeded, Failed }

impl ActiveCondition {
    /// Capture targets when FUN_0040b9e0 accepts the mission, not on each turn.
    pub fn from_setup(setup: &ConditionSetup, read_memory: &dyn Fn(u32) -> Option<i32>) -> Result<Self, String> {
        let turn = read_memory(setup.deadline_base_address).ok_or("Original total-turn field 0x004b64aa is unavailable")? as i16 as i32;
        let stat_threshold = if let Some(target) = &setup.stat_target {
            Some(read_memory(target.base_address).ok_or_else(|| format!("Original task stat 0x{:08x} is unavailable", target.base_address))?.wrapping_add(target.delta))
        } else { None };
        if stat_threshold.is_none() && setup.mission.is_none() { return Err("Original task has neither an established stat target nor mission kind".into()); }
        Ok(Self { setup: setup.clone(), deadline: turn + setup.deadline_delta, stat_threshold })
    }

    /// Call after the landed event and before incrementing the original turn.
    /// FUN_00419110 deliberately casts money to signed int16 before comparing.
    pub fn evaluate(&self, read_memory: &dyn Fn(u32) -> Option<i32>) -> Result<ConditionOutcome, String> {
        let turn = read_memory(0x004b64aa).ok_or("Original total-turn field is unavailable")? as i16 as i32;
        let successful = if self.setup.mission.is_some() {
            read_memory(0x004b64d0).ok_or("Original mission-progress flag is unavailable")? == 1
        } else {
            let target = self.setup.stat_target.as_ref().ok_or("Missing original stat target")?;
            let value = read_memory(target.base_address).ok_or("Original task stat is unavailable")? as i16 as i32;
            value >= self.stat_threshold.ok_or("Missing captured task threshold")?
        };
        Ok(if successful { ConditionOutcome::Succeeded } else if turn + 1 < self.deadline { ConditionOutcome::Waiting } else { ConditionOutcome::Failed })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWrite { pub address: u32, pub value: i32 }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissionResultEffects {
    pub result_id: i32,
    pub writes: Vec<MemoryWrite>,
    pub amplified_positive_reward: bool,
    pub lost_level: bool,
    pub item_id: Option<i16>,
    pub item_quantity: i16,
}

impl MissionResultEffects {
    fn from_words(id: i32, original: &[u16], multiplier: i32, read: &dyn Fn(u32) -> Option<i32>) -> Result<Self, String> {
        fn get(read: &dyn Fn(u32) -> Option<i32>, address: u32) -> Result<i32, String> {
            read(address).ok_or_else(|| format!("Original mission result field 0x{address:08x} is unavailable"))
        }
        let mut words=original.to_vec();
        let mut amplified_positive_reward=false;
        if multiplier > 1 {
            for index in [7,31] {
                let value=int32(&words,index);
                if value > 0 {
                    let value=value.wrapping_mul(multiplier) as u32;
                    words[index]=value as u16;words[index+1]=(value>>16) as u16;
                    amplified_positive_reward=true;
                }
            }
            for index in 9..=12 {
                if signed(words[index]) > 0 {
                    words[index]=(words[index] as i16).wrapping_mul(multiplier as i16) as u16;
                    amplified_positive_reward=true;
                }
            }
        }
        let result=ResultDelta::from_words(id,&words);
        let mut values=BTreeMap::new();
        for field in [0x004b6484,0x004b6486,0x004b6488,0x004b648a,0x004b648e,0x004b6490,0x004b6492,0x004b6494,0x004b6496] {
            values.insert(field,get(read,field)?);
        }
        for change in result.changes { values.insert(change.address,change.apply(values[&change.address])); }
        let lost_level=signed(words[5]) != -4096 && get(read,0x004b64a4)? > 1;
        if lost_level {
            values.insert(0x004b64a4,(get(read,0x004b64a4)? as i16).wrapping_sub(1) as i32);
            values.insert(0x004b6484,(values[&0x004b6484] as i16).wrapping_sub(18) as i32);
            values.insert(0x004b6488,(values[&0x004b6488] as i16).wrapping_sub(200) as i32);
        }
        values.insert(0x004b6486,values[&0x004b6486].min(values[&0x004b6484]).max(0));
        for (field,maximum) in [(0x004b6488,9999),(0x004b648a,99_999_999),(0x004b648e,9999),
            (0x004b6490,9999),(0x004b6492,9999),(0x004b6494,9999),(0x004b6496,99)] {
            values.insert(field,values[&field].clamp(0,maximum));
        }
        let item_kind=signed(words[15]);
        let mut item_id=None;let mut item_quantity=0;
        if item_kind > 0 {
            const IDS:[i16;26]=[-1,999,9,5,2,1,-1,6,4,0,3,7,8,10,11,12,100,101,106,110,120,121,122,123,126,1000];
            let id=*IDS.get(item_kind as usize).ok_or("Original mission item kind is outside its table")?;
            if id < 0 { return Err("Original mission item kind points outside the supported inventory".into()); }
            item_id=Some(id);
            let quantity=words[14] as i16;
            let (address,amount)=match id {
                999 => (0x004b64e2,quantity),
                1000 => (0x004b64e4,quantity),
                _ => {
                    let (base,count)=if id < 100 {(0x004b9ef8,13)}else{(0x004b9f14,27)};
                    let mut total=0_i16;
                    for index in 0..count { total=total.wrapping_add(get(read,base+index*2)? as i16); }
                    let capacity=8-i32::from(total);
                    let amount=i32::from(quantity).min(capacity) as i16;
                    (if id < 100 {0x004b9ef8+id as u32*2}else{0x004b9e4c+id as u32*2},amount)
                }
            };
            item_quantity=amount;
            values.insert(address,(get(read,address)? as i16).wrapping_add(amount) as i32);
        }
        let writes=values.into_iter().filter(|(address,value)|read(*address)!=Some(*value))
            .map(|(address,value)|MemoryWrite{address,value}).collect();
        Ok(Self{result_id:id,writes,amplified_positive_reward,lost_level,item_id,item_quantity})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventSelection {
    pub next_node: i32,
    pub writes: Vec<MemoryWrite>,
    /// The caller applies FUN_0041c2a0(1) after these writes, before the node.
    pub skill_trigger: i16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissionSelection { pub mission_index: Option<i16>, pub writes: Vec<MemoryWrite> }

/// FUN_00418040, called by opcode 26 only when no mission is active. Receiving
/// the task sets its multiplier to 1; a later visit while active increments it.
pub fn select_mission(read: &dyn Fn(u32) -> Option<i32>, next_random: &mut dyn FnMut() -> u32) -> Result<MissionSelection, String> {
    let completed=read(0x004b64d4).ok_or("Original completed-mission count is unavailable")? as i16;
    if completed >= 50 {
        return Ok(MissionSelection{mission_index:None,writes:if completed>50{vec![MemoryWrite{address:0x004b64d4,value:50}]}else{vec![]}});
    }
    let stage=read(0x004b649e).ok_or("Original life stage is unavailable")? as i16;
    let mut writes=Vec::new();
    let mission_index=if (0..6).contains(&stage) {
        (next_random()%12) as i16
    } else if stage==6 {
        // 0x004b64da is a packed int32 flag, spanning words 64da and 64dc.
        let draw=next_random();
        let exhausted=read(0x004b64da).ok_or("Original adult-mission completion flag is unavailable")?==1;
        if draw%100<20 || exhausted { (next_random()%27+12) as i16 }
        else {
            let index=read(0x004b64d8).ok_or("Original next adult mission is unavailable")? as i16;
            let next=index.wrapping_add(1);
            writes.push(MemoryWrite{address:0x004b64d8,value:next as i32});
            if next>48 {
                writes.push(MemoryWrite{address:0x004b64da,value:1});
                writes.push(MemoryWrite{address:0x004b64dc,value:0});
            }
            index
        }
    } else { return Err(format!("Original mission selection does not establish a task for stage {stage}")); };
    Ok(MissionSelection{mission_index:Some(mission_index),writes})
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDefinition {
    pub id: usize,
    pub trigger: i32,
    pub attribute: i32,
    pub level_values: [i16; 4],
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivation {
    pub skill_id: usize,
    pub level: i16,
    pub name: String,
    pub address: u32,
    pub amount: i16,
    pub percentage: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillEffects { pub activations: Vec<SkillActivation>, pub writes: Vec<MemoryWrite> }

/// Numeric effects of FUN_0041c2a0, in original skill-index order. A clamp of
/// every tracked stat follows each activation. Skill 25 is student-only.
pub fn skill_effects(
    trigger: i16,
    skills: &[SkillDefinition],
    levels: &[i16; 26],
    read_memory: &dyn Fn(u32) -> Option<i32>,
) -> Result<SkillEffects, String> {
    let career = read_memory(0x004b64a2).ok_or("Original career field is unavailable")? as i16;
    let mut values = BTreeMap::new();
    let mut activations = Vec::new();
    let mut ordered = skills.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|skill| skill.id);
    for skill in ordered {
        let level = *levels.get(skill.id).ok_or("Invalid original skill ID")?;
        if level <= 0 || skill.trigger != i32::from(trigger) || (skill.id == 25 && career != -1) { continue; }
        let amount = *skill.level_values.get((level - 1) as usize).ok_or("Original skill level is outside the exported table")?;
        let address = *[0x004b6486, 0x004b6488, 0x004b648a, 0x004b6492, 0x004b6490, 0x004b6494, 0x004b648e]
            .get(skill.attribute as usize).ok_or("Original skill attribute is unavailable")?;
        for field in [0x004b6484, 0x004b6486, 0x004b6488, 0x004b648a, 0x004b648e, 0x004b6490, 0x004b6492, 0x004b6494, 0x004b6496] {
            if let std::collections::btree_map::Entry::Vacant(entry) = values.entry(field) {
                entry.insert(read_memory(field).ok_or_else(|| format!("Original skill stat 0x{field:08x} is unavailable"))?);
            }
        }
        let current = values[&address];
        let percentage = trigger == 4;
        let next = if address == 0x004b648a {
            if percentage {
                // 0x41c77d: FILD amount; FIMUL money; FMUL qword 0.01;
                // FIADD money; __ftol. Legal table amounts are nonnegative.
                ((i64::from(current) * (100 + i64::from(amount))) / 100) as i32
            } else { current.wrapping_add(i32::from(amount)) }
        } else {
            let current = current as i16;
            let delta = if percentage { (i32::from(current) * i32::from(amount) / 100) as i16 } else { amount };
            current.wrapping_add(delta) as i32
        };
        values.insert(address, next);
        let hp = values[&0x004b6486].min(values[&0x004b6484]).max(0);
        values.insert(0x004b6486, hp);
        for (field, maximum) in [(0x004b6488,9999),(0x004b648a,99_999_999),(0x004b648e,9999),
            (0x004b6490,9999),(0x004b6492,9999),(0x004b6494,9999),(0x004b6496,99)] {
            values.insert(field, values[&field].clamp(0, maximum));
        }
        activations.push(SkillActivation { skill_id: skill.id, level, name: skill.name.clone(), address, amount, percentage });
    }
    let writes = values.into_iter().filter(|(address, value)| read_memory(*address) != Some(*value))
        .map(|(address, value)| MemoryWrite { address, value }).collect();
    Ok(SkillEffects { activations, writes })
}

/// Opcode 28 in FUN_0040c2e0. All reads are signed player/table fields, except
/// money, which the host reads as int32. Table addresses are preserved exactly.
/// This retains every original RNG draw, including the draw before career 24.
pub fn select_ordinary_event(
    read_memory: &dyn Fn(u32) -> Option<i32>,
    read_table_i16: &dyn Fn(u32) -> Option<i16>,
    next_random: &mut dyn FnMut() -> u32,
) -> Result<EventSelection, String> {
    fn field(read: &dyn Fn(u32) -> Option<i32>, address: u32) -> Result<i32, String> {
        read(address).ok_or_else(|| format!("Original event field 0x{address:08x} is unavailable"))
    }
    fn range(read: &dyn Fn(u32) -> Option<i16>, address: i32) -> Result<(i32, i32), String> {
        let address = u32::try_from(address).map_err(|_| "Original event table address underflow")?;
        Ok((read(address).ok_or_else(|| format!("Original event table 0x{address:08x} is unavailable"))? as i32,
            read(address + 2).ok_or_else(|| format!("Original event table 0x{:08x} is unavailable", address + 2))? as i32))
    }
    let career = field(read_memory, 0x004b64a2)? as i16 as i32;
    let complete = field(read_memory, 0x004b64c2)? as i16 == 1;
    let mut writes = Vec::new();
    let use_random = if complete || career == 24 {
        // This draw is deliberately consumed even when career == 24.
        next_random() % 10 < 6 && career != 24
    } else {
        let mut story = field(read_memory, 0x004b64ca)? as i16;
        if story == 1500 {
            return Ok(EventSelection { next_node: 1500,
                writes: vec![MemoryWrite { address: 0x004b64ca, value: 1501 }], skill_trigger: 1 });
        }
        if next_random() % 10 < 4 || (story == 1510 && field(read_memory, 0x004b648a)? < 500_001) {
            next_random() % 10 < 6
        } else {
            let accepted = field(read_memory, 0x004b64c6)? as i16;
            if story == 1502 && accepted != 1 { story = 1507; }
            let next = story.wrapping_add(1);
            writes.push(MemoryWrite { address: 0x004b64ca, value: next as i32 });
            if (next >= 1507 && accepted == 1) || (next > 1511 && accepted == 0) {
                writes.push(MemoryWrite { address: 0x004b64c2, value: 1 });
            }
            return Ok(EventSelection { next_node: story as i32, writes, skill_trigger: 1 });
        }
    };
    let stage = field(read_memory, 0x004b649e)? as i16 as i32;
    let (start, end) = if use_random {
        if next_random() % 10 < 5 { (0x5e25, 0x5e3f) }
        else { range(read_table_i16, 0x004738fc + stage * 4)? }
    } else {
        let sex = field(read_memory, 0x004b6498)? as i16 as i32;
        if career == -1 {
            range(read_table_i16, 0x00477578 + (sex * 31 + stage) * 20)?
        } else {
            let level = field(read_memory, 0x004b64a6)? as i16 as i32;
            range(read_table_i16, 0x00477574 + ((sex * 31 + stage + career) * 5 + level) * 4)?
        }
    };
    let next_node = if start == end { start } else {
        let width = end.checked_sub(start).and_then(|n| n.checked_add(1)).filter(|n| *n > 0)
            .ok_or("Invalid original ordinary-event range")?;
        start + (next_random() % width as u32) as i32
    };
    Ok(EventSelection { next_node, writes, skill_trigger: 1 })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DialogueSpeaker { Player, Named { name: String } }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DialogueText { pub speaker: Option<DialogueSpeaker>, pub text: String, pub raw_text: String }

/// FUN_00409d70 recognizes the full-width ／ and ０ bytes in the GBK text.
pub fn split_dialogue(raw: &str) -> DialogueText {
    let (speaker, text) = match raw.strip_prefix('／').and_then(|rest| rest.split_once('／')) {
        Some(("０", text)) => (Some(DialogueSpeaker::Player), text),
        Some((name, text)) => (Some(DialogueSpeaker::Named { name: name.into() }), text),
        None => (None, raw),
    };
    DialogueText { speaker, text: text.into(), raw_text: raw.into() }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption { pub index: usize, pub node_id: i32, pub dialogue_id: i32, pub text: String, pub next_node: i32 }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeCall { pub node_id: i32, pub opcode: u16, pub args: [i32; 3], pub original_next: i32, pub function_address: Option<u32> }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VmEffect {
    #[serde(rename_all = "camelCase")]
    Pattern { pattern_id: i32, scroll_mode: i32, resource_id: String },
    #[serde(rename_all = "camelCase")]
    Background { background_id: i32, mode: i32, resource_id: String },
    #[serde(rename_all = "camelCase")]
    Portraits { transition: i32, left_id: i32, right_id: i32 },
    #[serde(rename_all = "camelCase")]
    Dialogue { dialogue_id: i32, content: DialogueText, left_id: i32, right_id: i32 },
    #[serde(rename_all = "camelCase")]
    ClearPortraits { left: bool, right: bool },
    #[serde(rename_all = "camelCase")]
    Choice { group_id: i32, options: Vec<ChoiceOption> },
    ApplyResult { result: ResultDelta },
    ConfigureCondition { condition: ConditionSetup },
    SetValue { address: u32, value: i32 },
    NativeCall { call: NativeCall },
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NativeContinuation { Next { node: i32 }, HostChosen, End }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VmPending {
    #[serde(rename_all = "camelCase")]
    Dialogue { next_node: i32 },
    #[serde(rename_all = "camelCase")]
    Choice { group_id: i32, options: Vec<ChoiceOption> },
    Native { call: NativeCall, continuation: NativeContinuation },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VmInput {
    Continue,
    Choose { index: usize },
    #[serde(rename_all = "camelCase")]
    HostReturned { next_node: Option<i32> },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VmBatch { pub effects: Vec<VmEffect>, pub pending: Option<VmPending>, pub finished: bool }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyVm {
    pub program_counter: i32,
    pub pending: Option<VmPending>,
    pub finished: bool,
    pub instructions_executed: u64,
    #[serde(default)] queued_effects: Vec<VmEffect>,
    #[serde(default)] relationship_nodes: Vec<i32>,
}

impl LegacyVm {
    pub fn new(start_node: i32) -> Self { Self { program_counter: start_node, pending: None, finished: false, instructions_executed: 0, queued_effects: vec![], relationship_nodes: vec![] } }
    fn finish(&mut self, effects: &mut Vec<VmEffect>) { self.finished = true; self.pending = None; effects.push(VmEffect::Finished); }

    pub fn resume(&mut self, input: VmInput) -> Result<(), String> {
        let pending = self.pending.clone().ok_or("The original event is not waiting for input")?;
        match (pending, input) {
            (VmPending::Dialogue { next_node }, VmInput::Continue) => self.program_counter = next_node,
            (VmPending::Choice { group_id, options }, VmInput::Choose { index }) => {
                let option = options.get(index).ok_or("Choice index is out of range")?;
                self.program_counter = option.next_node;
                if group_id == 120 { self.queued_effects.push(VmEffect::SetValue { address: 0x004b64c6, value: i32::from(index == 0) }); }
            }
            (VmPending::Native { continuation, .. }, VmInput::HostReturned { next_node }) => match continuation {
                NativeContinuation::Next { node } => self.program_counter = next_node.unwrap_or(node),
                NativeContinuation::HostChosen => self.program_counter = next_node.ok_or("This native event must return an original next-node ID")?,
                NativeContinuation::End => { self.finished = true; self.queued_effects.push(VmEffect::Finished); }
            },
            _ => return Err("Input does not match the original event's pending operation".into()),
        }
        self.pending = None;
        Ok(())
    }

    /// Native opcode 24 can schedule several new-contact stories. In the
    /// original dispatcher only opcode 12 advances that list; a condition,
    /// missing node or another terminating native operation ends it instead.
    /// Only return kind 1 replaces the original local contact-story list;
    /// dating/marriage/cancel inside a story preserve an existing remainder.
    pub fn resume_relationship(&mut self,event_nodes:Vec<i32>,return_kind:u8)->Result<(),String> {
        let Some(VmPending::Native {call,..})=&self.pending else {return Err("The original event is not waiting for the relationship house".into());};
        if call.opcode!=24 || return_kind>4 || event_nodes.len()>8 || event_nodes.iter().any(|&node|node<=0) ||
            (return_kind==0 && !event_nodes.is_empty()) || (return_kind>0 && event_nodes.is_empty()) ||
            (return_kind>1 && event_nodes.len()!=1) {
            return Err("Invalid original relationship continuation".into());
        }
        self.program_counter=event_nodes.first().copied().unwrap_or(call.original_next);
        if return_kind==1 {self.relationship_nodes=event_nodes[1..].to_vec();}
        self.pending=None;
        Ok(())
    }

    pub fn advance(&mut self, program: &LegacyProgram, next_random: &mut dyn FnMut() -> u32) -> Result<VmBatch, String> {
        let mut effects = std::mem::take(&mut self.queued_effects);
        if self.pending.is_some() || self.finished { return Ok(VmBatch { effects, pending: self.pending.clone(), finished: self.finished }); }
        for _ in 0..1024 {
            let Some(words) = program.node_words(self.program_counter) else { self.finish(&mut effects); break; };
            let opcode = words[1];
            let next = signed(words[2]);
            let [a, b, c] = [signed(words[3]), signed(words[4]), signed(words[5])];
            self.instructions_executed += 1;
            match opcode {
                1 => {
                    if !(1..=6).contains(&a) { return Err(format!("Invalid original background pattern {a}")); }
                    effects.push(VmEffect::Pattern { pattern_id: a, scroll_mode: b, resource_id: format!("install/bmp/bug{a:02}.bmp") });
                    self.program_counter = next;
                }
                2 => {
                    let resource_id = program.background_resource(a).ok_or_else(|| format!("Original background {a} is missing"))?;
                    effects.push(VmEffect::Background { background_id: a, mode: b, resource_id });
                    self.program_counter = next;
                }
                3 => { effects.push(VmEffect::Portraits { transition: a, left_id: b, right_id: c }); self.program_counter = next; }
                4 => {
                    let raw = program.dialogue(a).ok_or_else(|| format!("Original dialogue {a} is missing"))?;
                    effects.push(VmEffect::Dialogue { dialogue_id: a, content: split_dialogue(raw), left_id: b, right_id: c });
                    self.pending = Some(VmPending::Dialogue { next_node: next });
                    break;
                }
                5 | 8 | 10 | 11 | 13 | 20 | 21 => self.program_counter = next,
                6 => { effects.push(VmEffect::ClearPortraits { left: b != 0, right: c != 0 }); self.program_counter = next; }
                7 => {
                    let group = program.choices.get(&c).ok_or_else(|| format!("Original choice group {c} is missing"))?;
                    let mut options = Vec::new();
                    for choice_node in group[1..].iter().copied().map(signed).filter(|id| *id > 0) {
                        let node = program.node_words(choice_node).ok_or_else(|| format!("Original choice node {choice_node} is missing"))?;
                        let dialogue_id = signed(node[3]);
                        let text = program.dialogue(dialogue_id).ok_or_else(|| format!("Original choice dialogue {dialogue_id} is missing"))?.to_owned();
                        options.push(ChoiceOption { index: options.len(), node_id: choice_node, dialogue_id, text, next_node: signed(node[2]) });
                    }
                    if options.is_empty() { return Err(format!("Original choice group {c} has no positive node references")); }
                    effects.push(VmEffect::Choice { group_id: c, options: options.clone() });
                    self.pending = Some(VmPending::Choice { group_id: c, options });
                    break;
                }
                9 => { effects.push(VmEffect::ConfigureCondition { condition: program.condition_setup(a)? }); self.finish(&mut effects); break; }
                12 => {
                    if a > 0 { effects.push(VmEffect::ApplyResult { result: program.result_delta(a)? }); }
                    effects.push(VmEffect::ClearPortraits {left:true,right:true});
                    if self.relationship_nodes.is_empty() {self.finish(&mut effects);break;}
                    self.program_counter=self.relationship_nodes.remove(0);
                }
                14 => {
                    self.program_counter = if b == 0 || a == b { a } else {
                        let width = b.checked_sub(a).and_then(|n| n.checked_add(1)).filter(|n| *n > 0).ok_or("Invalid original random-node range")?;
                        (next_random() % width as u32) as i32 + a
                    };
                }
                15..=19 | 22..=28 => {
                    let function_address = match opcode { 15 => Some(0x00433680), 16 => Some(0x0042c970), 17 => Some(0x0043b540), 18 => Some(0x0042a550), 19 => Some(0x00419590), 24 => Some(0x00438480), 25 => Some(0x00436b80), 27 => Some(0x0043d130), _ => None };
                    let call = NativeCall { node_id: self.program_counter, opcode, args: [a, b, c], original_next: next, function_address };
                    let continuation = match opcode { 24 | 28 => NativeContinuation::HostChosen, 26 => NativeContinuation::End, _ => NativeContinuation::Next { node: next } };
                    effects.push(VmEffect::NativeCall { call: call.clone() });
                    self.pending = Some(VmPending::Native { call, continuation });
                    break;
                }
                // The native default branch reloads the same node without
                // changing its ID. Report this original infinite loop instead
                // of hanging the browser or guessing a corrected next node.
                _ => return Err(format!("原版事件节点 {} 的操作码 {opcode} 会反复读取自身，无法继续。进度已保留。", self.program_counter)),
            }
        }
        if self.pending.is_none() && !self.finished { return Err("Original event exceeded the 1024-instruction resume limit".into()); }
        Ok(VmBatch { effects, pending: self.pending.clone(), finished: self.finished })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn program(nodes: Vec<[u16; 6]>) -> LegacyProgram {
        let words = nodes.into_iter().map(|words| json!({"words":words})).collect::<Vec<_>>();
        LegacyProgram::from_json(&json!({"program":{"nodes":words},"dialogues":{"records":[{"id":1,"text":"／０／你好"},{"id":2,"text":"选择甲"},{"id":3,"text":"选择乙"}]},"backgrounds":{"records":[{"id":9,"text":"bg222.bmp"}]},"abc":{"records":[{"words":[120,10,11,65535,65535]}]}}).to_string()).unwrap()
    }

    #[test]
    fn original_first_duplicate_node_wins_and_dialogue_waits() {
        let p = program(vec![[1,11,2,0,0,0],[2,4,3,1,1001,0],[2,0,0,0,0,0],[3,12,0,0,0,0]]);
        assert_eq!(p.duplicate_node_count,1);
        let mut vm=LegacyVm::new(1);
        let batch=vm.advance(&p,&mut || panic!("no randomness expected")).unwrap();
        assert!(matches!(&batch.effects[0],VmEffect::Dialogue{content,..} if content.speaker==Some(DialogueSpeaker::Player)&&content.text=="你好"));
        assert!(!batch.finished);
        vm.resume(VmInput::Continue).unwrap();
        assert!(vm.advance(&p,&mut || 0).unwrap().finished);
    }

    #[test]
    fn original_opcode_zero_loops_are_reported_without_repairing_the_catalog() {
        let p=LegacyProgram::from_json(include_str!("../../../assets/catalog/events.json")).unwrap();
        let observed:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-dispatch-boundaries.json")).unwrap();
        for case in observed["cases"].as_array().unwrap() {
            let entry=case["entry"].as_i64().unwrap()as i32;
            let loop_node=case["loopNode"].as_i64().unwrap()as i32;
            let mut vm=LegacyVm::new(entry);
            let error=loop {
                match vm.advance(&p,&mut ||panic!("opcode-zero path must not consume randomness")) {
                    Err(error)=>break error,
                    Ok(batch)=>{
                        assert!(matches!(batch.pending,Some(VmPending::Dialogue{..})));
                        vm.resume(VmInput::Continue).unwrap();
                    }
                }
            };
            assert_eq!(vm.program_counter,loop_node);
            assert_eq!(p.node_words(loop_node).unwrap()[1],0);
            assert_eq!(vm.instructions_executed,case["loads"].as_array().unwrap().len()as u64-7);
            assert!(error.contains(&format!("节点 {loop_node} 的操作码 0")));
            assert!(!vm.finished);
        }
        // ABC2026's second answer reaches the malformed original record;
        // its apparent intended next node 26289 is stored in argument 1.
        let mut vm=LegacyVm::new(26272);
        vm.advance(&p,&mut ||0).unwrap();vm.resume(VmInput::Choose{index:1}).unwrap();
        assert_eq!(vm.program_counter,26283);
        assert_eq!(p.node_words(26288).unwrap(),[26288,3,0,26289,1,607]);
    }

    #[test]
    fn selection_jumps_to_option_next_and_preserves_original_group120_side_effect() {
        let p=program(vec![[1,7,0,0,0,120],[10,4,20,2,0,0],[11,4,21,3,0,0],[20,12,0,0,0,0],[21,12,0,0,0,0]]);
        let mut vm=LegacyVm::new(1);
        let batch=vm.advance(&p,&mut || 0).unwrap();
        assert!(matches!(&batch.pending,Some(VmPending::Choice{options,..}) if options.len()==2&&options[1].next_node==21));
        assert!(vm.resume(VmInput::Choose{index:2}).is_err());
        vm.resume(VmInput::Choose{index:1}).unwrap();
        let batch=vm.advance(&p,&mut || 0).unwrap();
        assert!(batch.effects.contains(&VmEffect::SetValue{address:0x004b64c6,value:0}));
        assert!(batch.finished);
    }

    #[test]
    fn inclusive_random_branch_consumes_one_draw_except_singleton_ranges() {
        let p=program(vec![[1,14,0,10,12,0],[10,12,0,0,0,0],[11,12,0,0,0,0],[12,12,0,0,0,0]]);
        let mut vm=LegacyVm::new(1);let mut calls=0;
        vm.advance(&p,&mut || {calls+=1;5}).unwrap();
        assert_eq!(vm.program_counter,12);assert_eq!(calls,1);
        let p=program(vec![[1,14,0,10,10,0],[10,12,0,0,0,0]]);
        LegacyVm::new(1).advance(&p,&mut || panic!("singleton must not advance RNG")).unwrap();
    }

    #[test]
    fn native_gameplay_waits_for_an_explicit_host_result() {
        let p=program(vec![[1,24,2,0,0,0],[2,12,0,0,0,0]]);let mut vm=LegacyVm::new(1);
        assert!(!vm.advance(&p,&mut || 0).unwrap().finished);
        assert!(vm.resume(VmInput::HostReturned{next_node:None}).is_err());
        vm.resume(VmInput::HostReturned{next_node:Some(2)}).unwrap();
        assert!(vm.advance(&p,&mut || 0).unwrap().finished);
    }

    #[test]
    fn relationship_story_queue_roundtrips_and_advances_only_after_result_nodes() {
        let p=program(vec![[1,24,2,0,0,0],[2,12,0,0,0,0],
            [10,4,11,1,0,0],[11,12,0,0,0,0],[20,4,21,2,0,0],[21,12,0,0,0,0],[30,4,31,3,0,0],[31,12,0,0,0,0]]);
        let mut vm=LegacyVm::new(1);vm.advance(&p,&mut ||0).unwrap();
        vm.resume_relationship(vec![10,20,30],1).unwrap();
        for expected in 1..=3 {
            let batch=vm.advance(&p,&mut ||0).unwrap();assert!(!batch.finished);
            assert!(batch.effects.iter().any(|effect|matches!(effect,VmEffect::Dialogue {dialogue_id,..} if *dialogue_id==expected)));
            vm=serde_json::from_str(&serde_json::to_string(&vm).unwrap()).unwrap();vm.resume(VmInput::Continue).unwrap();
        }
        let batch=vm.advance(&p,&mut ||0).unwrap();assert!(batch.finished);
        assert_eq!(batch.effects.iter().filter(|effect|matches!(effect,VmEffect::Finished)).count(),1);
    }

    #[test]
    fn relationship_cancel_uses_original_next_and_nested_dates_keep_remaining_stories() {
        let p=program(vec![[1,24,2,0,0,0],[2,12,0,0,0,0],[10,24,11,0,0,0],[11,12,0,0,0,0],
            [20,4,21,2,0,0],[21,12,0,0,0,0],[30,4,31,3,0,0],[31,12,0,0,0,0]]);
        let mut vm=LegacyVm::new(1);vm.advance(&p,&mut ||0).unwrap();vm.resume_relationship(vec![],0).unwrap();assert!(vm.advance(&p,&mut ||0).unwrap().finished);
        let mut vm=LegacyVm::new(1);vm.advance(&p,&mut ||0).unwrap();vm.resume_relationship(vec![10,20],1).unwrap();
        vm.advance(&p,&mut ||0).unwrap();vm.resume_relationship(vec![30],2).unwrap();vm.advance(&p,&mut ||0).unwrap();
        vm.resume(VmInput::Continue).unwrap();let batch=vm.advance(&p,&mut ||0).unwrap();
        assert!(batch.effects.iter().any(|effect|matches!(effect,VmEffect::Dialogue {dialogue_id:2,..})));assert!(!batch.finished);
    }

    #[test]
    fn result_sentinels_stat_scaling_and_store_then_clamp_match_executable() {
        let mut words=vec![0xf000;59];words[0]=1;words[8]=0xffff;words[32]=0xffff;words[15]=0;
        assert!(ResultDelta::from_words(1,&words).changes.is_empty());
        words[9]=5;words[10]=(-3_i16) as u16;words[7]=1000;words[8]=0;words[31]=2;words[32]=0;words[15]=21;
        let result=ResultDelta::from_words(1,&words);
        assert_eq!(result.unlock_career,Some(24));
        assert_eq!(result.changes.iter().find(|d|d.address==0x004b6492).unwrap().delta,50);
        assert_eq!(result.changes.iter().find(|d|d.address==0x004b6494).unwrap().apply(20),0);
        assert_eq!(VariableDelta{address:0,name:String::new(),delta:50,wrapping_bits:16,minimum:0,maximum:9999}.apply(32760),0);
    }

    #[test]
    fn real_catalog_first_event_runs_to_original_dialogue_and_roundtrips_pending_state() {
        let data=include_str!("../../../assets/catalog/events.json");
        let p=LegacyProgram::from_json(data).unwrap();assert_eq!(p.source_node_count,13913);assert_eq!(p.duplicate_node_count,44);
        let mut vm=LegacyVm::new(31);let batch=vm.advance(&p,&mut || 0).unwrap();
        assert!(batch.effects.iter().any(|effect|matches!(effect,VmEffect::Background{background_id:9,..})));
        assert!(matches!(batch.pending,Some(VmPending::Dialogue{..})));
        let restored:LegacyVm=serde_json::from_str(&serde_json::to_string(&vm).unwrap()).unwrap();assert_eq!(restored,vm);
    }

    #[test]
    fn condition_captures_target_and_succeeds_on_the_deadline_turn() {
        let mut words=vec![0u16;29];words[2]=30;words[3]=50;words[23]=2;
        let setup=ConditionSetup::from_words(1,&words);
        assert_eq!(setup.stat_target,Some(ConditionTarget{base_address:0x4b6492,delta:30}));
        let mut memory=BTreeMap::from([(0x4b64aa,10),(0x4b6492,100)]);
        let condition=ActiveCondition::from_setup(&setup,&|address|memory.get(&address).copied()).unwrap();
        assert_eq!(condition.deadline,13);assert_eq!(condition.stat_threshold,Some(130));
        assert_eq!(condition.evaluate(&|address|memory.get(&address).copied()).unwrap(),ConditionOutcome::Waiting);
        memory.insert(0x4b64aa,12);
        assert_eq!(condition.evaluate(&|address|memory.get(&address).copied()).unwrap(),ConditionOutcome::Failed);
        memory.insert(0x4b6492,130);
        assert_eq!(condition.evaluate(&|address|memory.get(&address).copied()).unwrap(),ConditionOutcome::Succeeded);
    }

    #[test]
    fn money_condition_preserves_original_signed_int16_comparison() {
        let mut words=vec![0u16;29];words[5]=100;words[23]=3;
        let setup=ConditionSetup::from_words(1,&words);
        let memory=BTreeMap::from([(0x4b64aa,10),(0x4b648a,40_000)]);
        let condition=ActiveCondition::from_setup(&setup,&|address|memory.get(&address).copied()).unwrap();
        assert_eq!(condition.stat_threshold,Some(40_100));
        let updated=BTreeMap::from([(0x4b64aa,13),(0x4b648a,40_100)]);
        assert_eq!(condition.evaluate(&|address|updated.get(&address).copied()).unwrap(),ConditionOutcome::Failed);
    }

    #[test]
    fn mission_condition_reads_progress_and_uses_kind_specific_target() {
        let mut words=vec![0u16;29];words[24]=11;words[25]=6;words[23]=3;
        let setup=ConditionSetup::from_words(1,&words);
        assert_eq!(setup.mission,Some(MissionSetup{kind:11,target:6,progress:0}));
        let memory=BTreeMap::from([(0x4b64aa,10),(0x4b64d0,0)]);
        let condition=ActiveCondition::from_setup(&setup,&|address|memory.get(&address).copied()).unwrap();
        assert_eq!(condition.evaluate(&|address|memory.get(&address).copied()).unwrap(),ConditionOutcome::Waiting);
        assert_eq!(condition.evaluate(&|address|if address==0x4b64d0{Some(1)}else{memory.get(&address).copied()}).unwrap(),ConditionOutcome::Succeeded);
    }

    fn event_memory()->BTreeMap<u32,i32> {
        BTreeMap::from([(0x4b64a2,-1),(0x4b64c2,0),(0x4b64ca,1500),(0x4b64c6,0),
            (0x4b649e,2),(0x4b6498,0),(0x4b64a6,0),(0x4b648a,10_000)])
    }

    #[test]
    fn ordinary_event_intro_and_rejection_preserve_story_writes() {
        let mut memory=event_memory();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|_|panic!("no table needed"),&mut ||panic!("intro must not draw")).unwrap();
        assert_eq!(event.next_node,1500);assert_eq!(event.writes,vec![MemoryWrite{address:0x4b64ca,value:1501}]);
        memory.insert(0x4b64ca,1502);
        let mut draws=0;
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|_|panic!("no table needed"),&mut ||{draws+=1;9}).unwrap();
        assert_eq!(draws,1);assert_eq!(event.next_node,1507);assert_eq!(event.writes,vec![MemoryWrite{address:0x4b64ca,value:1508}]);
        memory.insert(0x4b64ca,1511);
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|_|None,&mut ||9).unwrap();
        assert_eq!(event.next_node,1511);assert!(event.writes.contains(&MemoryWrite{address:0x4b64c2,value:1}));
    }

    #[test]
    fn ordinary_event_random_paths_preserve_draw_count_and_exact_table_addresses() {
        let mut memory=event_memory();memory.insert(0x4b64c2,1);
        let mut random=[0,0,26].into_iter();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|_|panic!("common pool needs no table"),&mut ||random.next().unwrap()).unwrap();
        assert_eq!(event.next_node,0x5e3f);assert_eq!(random.next(),None);
        let table=BTreeMap::from([(0x473904,50_i16),(0x473906,52),(0x4775a0,80),(0x4775a2,80)]);
        let mut random=[0,9,1].into_iter();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|address|table.get(&address).copied(),&mut ||random.next().unwrap()).unwrap();
        assert_eq!(event.next_node,51);assert_eq!(random.next(),None);
        let mut random=[9].into_iter();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|address|table.get(&address).copied(),&mut ||random.next().unwrap()).unwrap();
        assert_eq!(event.next_node,80);assert_eq!(random.next(),None);
    }

    #[test]
    fn career24_still_draws_before_forcing_the_career_event_table() {
        let mut memory=event_memory();memory.insert(0x4b64a2,24);memory.insert(0x4b64a6,1);
        let table=BTreeMap::from([(0x477780,900_i16),(0x477782,901)]);
        let mut random=[0,1].into_iter();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|address|table.get(&address).copied(),&mut ||random.next().unwrap()).unwrap();
        assert_eq!(event.next_node,901);assert_eq!(random.next(),None);
    }

    #[test]
    fn unaffordable_story1510_reenters_the_random_pool_without_advancing_story() {
        let mut memory=event_memory();memory.insert(0x4b64ca,1510);memory.insert(0x4b648a,500_000);
        let mut random=[9,0,0,3].into_iter();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|_|None,&mut ||random.next().unwrap()).unwrap();
        assert_eq!(event.next_node,0x5e28);assert!(event.writes.is_empty());assert_eq!(random.next(),None);
        memory.insert(0x4b648a,500_001);
        let mut random=[9].into_iter();
        let event=select_ordinary_event(&|address|memory.get(&address).copied(),&|_|None,&mut ||random.next().unwrap()).unwrap();
        assert_eq!(event.next_node,1510);assert_eq!(random.next(),None);
    }

    #[test]
    fn skill_trigger_filters_levels_and_preserves_student_only_bonus() {
        let catalog:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        let skills:Vec<SkillDefinition>=serde_json::from_value(catalog["skills"].clone()).unwrap();
        assert_eq!(skills.len(),26);
        let player=crate::character::LegacyPlayer::initialize();
        let read=|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)};
        let mut levels=[0;26];levels[2]=2;levels[3]=1;
        let effects=skill_effects(1,&skills,&levels,&read).unwrap();
        assert_eq!(effects.activations.iter().map(|a|a.skill_id).collect::<Vec<_>>(),[2,3]);
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b648a,value:12_000}));
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b648e,value:160}));
        assert!(skill_effects(2,&skills,&levels,&read).unwrap().activations.is_empty());
        levels=[0;26];levels[25]=3;
        let student_skill=&skills[25];
        assert_eq!(skill_effects(student_skill.trigger as i16,&skills,&levels,&read).unwrap().activations.len(),1);
        assert!(skill_effects(student_skill.trigger as i16,&skills,&levels,&|a|if a==0x4b64a2{Some(4)}else{read(a)}).unwrap().activations.is_empty());
    }

    #[test]
    fn skill_percentage_and_store_then_clamp_match_original_short_fields() {
        let mut player=crate::character::LegacyPlayer::initialize();player.set_i16(0x4b6486,79).unwrap();
        let read=|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)};
        let skills=vec![SkillDefinition{id:0,trigger:4,attribute:0,level_values:[10,20,40,0],name:"fixture".into()}];
        let mut levels=[0;26];levels[0]=1;
        assert_eq!(skill_effects(4,&skills,&levels,&read).unwrap().writes,vec![MemoryWrite{address:0x4b6486,value:86}]);
        let skills=vec![SkillDefinition{id:0,trigger:1,attribute:3,level_values:[100,0,0,0],name:"fixture".into()}];
        assert_eq!(skill_effects(1,&skills,&levels,&|a|if a==0x4b6492{Some(32760)}else{read(a)}).unwrap().writes,
            vec![MemoryWrite{address:0x4b6492,value:0}]);
    }

    fn empty_result()->Vec<u16> {
        let mut words=vec![0xf000;59];words[8]=0xffff;words[32]=0xffff;words[15]=0;words
    }

    #[test]
    fn mission_positive_multiplier_does_not_scale_penalties_or_unlock_careers() {
        let player=crate::character::LegacyPlayer::initialize();
        let read=|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)};
        let mut words=empty_result();words[7]=100;words[8]=0;words[9]=5;words[10]=(-2_i16) as u16;
        let effects=MissionResultEffects::from_words(1,&words,3,&read).unwrap();
        assert!(effects.amplified_positive_reward);assert!(!effects.lost_level);
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b648a,value:10_300}));
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b6492,value:250}));
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b6494,value:80}));
    }

    #[test]
    fn mission_level_penalty_reduces_max_hp_and_clamps_current_hp() {
        let mut player=crate::character::LegacyPlayer::initialize();player.set_i16(0x4b64a4,3).unwrap();player.set_i16(0x4b6488,450).unwrap();
        let read=|address|if address==0x4b648a{player.i32(address).ok()}else{player.i16(address).ok().map(i32::from)};
        let mut words=empty_result();words[5]=0;
        let effects=MissionResultEffects::from_words(1,&words,1,&read).unwrap();
        assert!(effects.lost_level);
        for (address,value) in [(0x4b64a4,2),(0x4b6484,82),(0x4b6486,82),(0x4b6488,250)] {
            assert!(effects.writes.contains(&MemoryWrite{address,value}));
        }
    }

    #[test]
    fn mission_inventory_awards_use_original_mapping_and_capacity() {
        let player=crate::character::LegacyPlayer::initialize();
        let read=|address|if address==0x4b648a{player.i32(address).ok()}else if (0x4b9ef8..0x4b9f4a).contains(&address){Some(if address==0x4b9efa{7}else{0})}else{player.i16(address).ok().map(i32::from)};
        let mut words=empty_result();words[14]=3;words[15]=2;
        let effects=MissionResultEffects::from_words(1,&words,2,&read).unwrap();
        assert_eq!(effects.item_id,Some(9));assert_eq!(effects.item_quantity,1);
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b9f0a,value:1}));
        words[15]=1;
        let effects=MissionResultEffects::from_words(1,&words,2,&read).unwrap();
        assert_eq!(effects.item_id,Some(999));assert_eq!(effects.item_quantity,3);
        assert!(effects.writes.contains(&MemoryWrite{address:0x4b64e2,value:3}));
    }

    #[test]
    fn mission_selection_preserves_stage_pools_adult_sequence_and_draws() {
        let mut memory=BTreeMap::from([(0x4b649e,2),(0x4b64d4,0),(0x4b64da,0),(0x4b64d8,39)]);
        let selected=select_mission(&|a|memory.get(&a).copied(),&mut ||13).unwrap();
        assert_eq!(selected.mission_index,Some(1));assert!(selected.writes.is_empty());
        memory.insert(0x4b649e,6);let mut draws=[99].into_iter();
        let selected=select_mission(&|a|memory.get(&a).copied(),&mut ||draws.next().unwrap()).unwrap();
        assert_eq!(selected.mission_index,Some(39));assert_eq!(selected.writes,vec![MemoryWrite{address:0x4b64d8,value:40}]);
        memory.insert(0x4b64d8,48);
        let selected=select_mission(&|a|memory.get(&a).copied(),&mut ||99).unwrap();
        assert_eq!(selected.mission_index,Some(48));assert!(selected.writes.contains(&MemoryWrite{address:0x4b64da,value:1}));
        memory.insert(0x4b64da,1);let mut draws=[99,26].into_iter();
        let selected=select_mission(&|a|memory.get(&a).copied(),&mut ||draws.next().unwrap()).unwrap();
        assert_eq!(selected.mission_index,Some(38));assert!(selected.writes.is_empty());assert_eq!(draws.next(),None);
        memory.insert(0x4b64d4,51);
        let selected=select_mission(&|a|memory.get(&a).copied(),&mut ||panic!("completed missions do not draw")).unwrap();
        assert_eq!(selected.mission_index,None);assert_eq!(selected.writes,vec![MemoryWrite{address:0x4b64d4,value:50}]);
    }

    fn fixture_memory(value:&serde_json::Value)->BTreeMap<u32,i32> {
        value.as_object().unwrap().iter().map(|(address,value)|
            (u32::from_str_radix(address.trim_start_matches("0x"),16).unwrap(),value.as_i64().unwrap() as i32)).collect()
    }

    #[test]
    fn ordinary_event_selection_matches_80_original_x86_execution_cases() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-vm.json")).unwrap();
        let catalog:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        let mut table=BTreeMap::new();
        for segment in catalog["tableSegments"].as_array().unwrap() {
            let base=u32::from_str_radix(segment["baseAddress"].as_str().unwrap().trim_start_matches("0x"),16).unwrap();
            for (index,value) in segment["values"].as_array().unwrap().iter().enumerate() {
                table.insert(base+index as u32*2,value.as_i64().unwrap() as i16);
            }
        }
        let cases=vectors["selectors"].as_array().unwrap();assert_eq!(cases.len(),80);
        for case in cases {
            let mut memory=fixture_memory(&case["fields"]);
            let mut rng=crate::rng::ReplayRng::new(case["seed"].as_u64().unwrap());
            let result=select_ordinary_event(&|a|memory.get(&a).copied(),&|a|table.get(&a).copied(),&mut ||rng.next_rand()).unwrap();
            assert_eq!(result.next_node,case["nextNode"].as_i64().unwrap() as i32,"{} seed {}",case["name"],case["seed"]);
            assert_eq!(rng.draws,case["rngDraws"].as_u64().unwrap(),"{} seed {}",case["name"],case["seed"]);
            assert_eq!(rng.state,case["rngState"].as_u64().unwrap() as u32);
            for write in result.writes {memory.insert(write.address,write.value);}
            for (address,value) in fixture_memory(&case["after"]) {assert_eq!(memory[&address],value);}
        }
    }

    #[test]
    fn mission_results_match_original_x86_execution_including_inventory_and_overflows() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-vm.json")).unwrap();
        let cases=vectors["missionResults"].as_array().unwrap();assert_eq!(cases.len(),7);
        for case in cases {
            let mut memory=fixture_memory(&case["fields"]);
            let words:Vec<u16>=serde_json::from_value(case["words"].clone()).unwrap();
            let result=MissionResultEffects::from_words(1,&words,case["multiplier"].as_i64().unwrap() as i32,&|a|memory.get(&a).copied()).unwrap();
            assert_eq!(result.amplified_positive_reward,case["amplified"].as_bool().unwrap(),"{}",case["name"]);
            assert_eq!(result.lost_level,case["lostLevel"].as_bool().unwrap(),"{}",case["name"]);
            for write in result.writes {memory.insert(write.address,write.value);}
            assert_eq!(memory,fixture_memory(&case["after"]),"{}",case["name"]);
        }
    }
}
