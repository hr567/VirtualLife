//! Original packed player fields, recovered from 0x004144c0 and verified by
//! executing that original routine in Unicorn (tests/fixtures/original-core.json).
use serde::{Deserialize,Serialize};
use crate::engine::Stats;

const BASE:u32=0x004b6478;
// Include the 26 persistent skill words at 0x004b6508..0x004b653a.
const SIZE:usize=0xc4;

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct LegacyPlayer { memory:Vec<u8> }

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct Life {
    pub stage:i16,pub stage_turn:i16,pub year:i16,pub life_level:i16,
    pub profession:i16,pub profession_level:i16,pub affection:i16,
    pub incarnation:i16,pub skills:Vec<i16>,
    pub traveling:bool,
}

#[derive(Clone,Debug,Default,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct TurnOutcome {pub levels_gained:i16,pub recovered_from_zero_hp:bool,pub aged:bool,pub stage_turn:i16,pub total_turn:i16}

impl LegacyPlayer {
    pub fn initialize()->Self {
        let mut player=Self{memory:vec![0;SIZE]};
        for (address,value) in [
            (0x6484,100),(0x6486,100),(0x648e,100),(0x6490,100),(0x6492,100),(0x6494,100),
            (0x649a,12),(0x649c,2016),(0x649e,2),(0x64a0,2),(0x64a2,-1),(0x64a4,1),
            (0x64c0,1),(0x64ca,1500),(0x64cc,-1),(0x64ce,-1),(0x64d6,-1),
            (0x64d8,39),(0x64de,1),(0x64e8,1),
        ]{player.set_i16(0x004b0000+address,value).unwrap();}
        player.set_i32(0x004b648a,10000).unwrap();
        player
    }
    pub fn valid(&self)->bool {self.memory.len()==SIZE}
    pub fn bytes(&self)->&[u8] {&self.memory}
    fn offset(&self,address:u32,size:usize)->Result<usize,String> {
        let offset=address.checked_sub(BASE).ok_or("Player address below record")? as usize;
        if offset+size>self.memory.len(){Err(format!("Unknown player address 0x{address:08x}"))}else{Ok(offset)}
    }
    pub fn i16(&self,address:u32)->Result<i16,String>{let p=self.offset(address,2)?;Ok(i16::from_le_bytes(self.memory[p..p+2].try_into().unwrap()))}
    pub fn i32(&self,address:u32)->Result<i32,String>{let p=self.offset(address,4)?;Ok(i32::from_le_bytes(self.memory[p..p+4].try_into().unwrap()))}
    pub fn set_i16(&mut self,address:u32,value:i16)->Result<(),String>{let p=self.offset(address,2)?;self.memory[p..p+2].copy_from_slice(&value.to_le_bytes());Ok(())}
    pub fn set_i32(&mut self,address:u32,value:i32)->Result<(),String>{let p=self.offset(address,4)?;self.memory[p..p+4].copy_from_slice(&value.to_le_bytes());Ok(())}
    pub fn add_i16(&mut self,address:u32,delta:i16)->Result<(),String>{self.set_i16(address,self.i16(address)?.wrapping_add(delta))}
    pub fn add_i32(&mut self,address:u32,delta:i32)->Result<(),String>{self.set_i32(address,self.i32(address)?.wrapping_add(delta))}
    pub fn stats(&self)->Stats {
        Stats{money:Some(self.i32(0x4b648a).unwrap()as i64),hp:Some(self.i16(0x4b6486).unwrap()as i64),max_hp:Some(self.i16(0x4b6484).unwrap()as i64),
            intelligence:Some(self.i16(0x4b6492).unwrap()as i64),morality:Some(self.i16(0x4b6490).unwrap()as i64),stamina:Some(self.i16(0x4b648e).unwrap()as i64),
            charm:Some(self.i16(0x4b6494).unwrap()as i64),experience:Some(self.i16(0x4b6488).unwrap()as i64),age:Some(self.i16(0x4b649a).unwrap()as i64)}
    }
    pub fn life(&self)->Life {Life{
        stage:self.i16(0x4b649e).unwrap(),stage_turn:self.i16(0x4b64a8).unwrap(),year:self.i16(0x4b649c).unwrap(),
        life_level:self.i16(0x4b64a4).unwrap(),profession:self.i16(0x4b64a2).unwrap(),profession_level:self.i16(0x4b64a6).unwrap(),affection:self.i16(0x4b6496).unwrap(),
        incarnation:self.i16(0x4b64c0).unwrap(),skills:(0..26).map(|i|self.i16(0x4b6508+i*2).unwrap()).collect(),
        traveling:self.i16(0x4b64ac).unwrap()==1,
    }}
    /// FUN_00425270: the original clamps after event and skill changes.
    pub fn clamp_stats(&mut self)->Result<(),String>{
        self.set_i16(0x4b6486,self.i16(0x4b6486)?.min(self.i16(0x4b6484)?).max(0))?;
        self.set_i32(0x4b648a,self.i32(0x4b648a)?.clamp(0,99_999_999))?;
        for address in [0x4b6488,0x4b648e,0x4b6490,0x4b6492,0x4b6494]{self.set_i16(address,self.i16(address)?.clamp(0,9999))?;}
        self.set_i16(0x4b6496,self.i16(0x4b6496)?.clamp(0,99))?;
        Ok(())
    }
    /// FUN_004159a0 epilogue / FUN_00419590: retain the highest acquired rank.
    pub fn update_skills(&mut self)->Result<(),String>{
        let stage=self.i16(0x4b649e)?;
        if (2..=4).contains(&stage){self.set_i16(0x4b653a,stage-1)?;}
        let career=self.i16(0x4b64a2)?;
        if (0..25).contains(&career){
            let rank=match self.i16(0x4b64a6)?{1|2=>1,3|4=>2,5=>3,_=>0};
            let address=0x4b6508+career as u32*2;
            self.set_i16(address,self.i16(address)?.max(rank))?;
        }
        Ok(())
    }
    /// Packed player part of FUN_0041b0c0. Inventory and career-history resets
    /// live in their owning systems; fully mastered skill words survive.
    pub fn reincarnate(&mut self)->Result<(),String>{
        let generation=self.i16(0x4b64c0)?;
        let experience=i32::from(self.i16(0x4b6488)?);
        let level=((experience/10+400)/200)as i16;
        self.add_i16(0x4b64c0,1)?;
        self.set_i16(0x4b6496,0)?;
        self.set_i32(0x4b648a,i32::from(generation)*50_000)?;
        self.set_i16(0x4b6488,(experience/10)as i16+200)?;
        self.set_i16(0x4b64a4,level)?;
        self.set_i16(0x4b6484,level.wrapping_mul(18).wrapping_add(82))?;
        for address in [0x4b6492,0x4b6490,0x4b6494,0x4b648e]{
            self.set_i16(address,(self.i16(address)?/10).wrapping_add(generation.wrapping_mul(200)))?;
        }
        self.set_i16(0x4b6486,self.i16(0x4b6484)?)?;self.clamp_stats()?;
        for(address,value)in[(0x64a2,-1),(0x64a6,0),(0x649e,2),(0x64a0,2),(0x649c,2016),(0x649a,12),
            (0x64a8,0),(0x64aa,0),(0x64ac,0),(0x64b0,0),(0x64b4,0),(0x64b8,0),(0x64bc,0),(0x64be,0),
            (0x64c2,0),(0x64c4,0),(0x64ca,1500),(0x64cc,-1),(0x64ce,-1),(0x64d0,0),(0x64e2,0),(0x64e4,0),
            (0x64d4,0),(0x64d8,39),(0x64da,0),(0x64de,1),(0x64ec,0),(0x64f0,0),(0x64f2,0),(0x64e8,1)]{
            self.set_i16(0x4b0000+address,value)?;
        }
        // These original flags occupy four bytes despite the adjacent packed
        // short fields. Clear both halves, including on imported saves.
        for(address,value)in[(0x64ac,0),(0x64b0,0),(0x64b4,0),(0x64b8,0),(0x64d0,0),(0x64da,0),(0x64de,1),(0x64e8,1),(0x64ec,0)]{
            self.set_i32(0x4b0000+address,value)?;
        }
        for index in 0..26{let address=0x4b6508+index*2;if self.i16(address)?<3{self.set_i16(address,0)?;}}
        Ok(())
    }
    /// Numeric part of 0x004159a0, after event completion and before stage hooks.
    /// Travel turns do not age the character. Stage transitions and competitions
    /// are separate effects: this method never silently skips their handlers.
    pub fn settle_turn_numbers(&mut self)->Result<TurnOutcome,String> {
        let mut outcome=TurnOutcome::default();
        let experience=self.i16(0x4b6488)?.min(9999);self.set_i16(0x4b6488,experience)?;
        let old_level=self.i16(0x4b64a4)?;
        let new_level=(experience/200+1).min(50);
        if new_level>old_level {
            outcome.levels_gained=new_level-old_level;
            self.add_i16(0x4b6484,outcome.levels_gained*18)?;self.set_i16(0x4b64a4,new_level)?;
            if self.i16(0x4b6486)?>0 {self.set_i16(0x4b6486,self.i16(0x4b6484)?)?;}
        }
        let traveling=self.i16(0x4b64ac)?==1;
        if self.i16(0x4b6486)?<1 {
            self.set_i32(0x4b648a,self.i32(0x4b648a)?/2)?;
            self.set_i16(0x4b6486,self.i16(0x4b6484)?)?;
            outcome.recovered_from_zero_hp=true;
        }
        if !traveling {
            self.add_i16(0x4b64a8,1)?;self.add_i16(0x4b64aa,1)?;
            let stage=self.i16(0x4b649e)?;
            let turns_per_year=if stage==6 {self.i16(0x4b64e6)?}else{5};
            if turns_per_year<=0{return Err("Original adult turns-per-year is missing".into());}
            if self.i16(0x4b64a8)?%turns_per_year==0 {
                self.add_i16(0x4b649a,1)?;self.add_i16(0x4b649c,1)?;outcome.aged=true;
            }
        }
        outcome.stage_turn=self.i16(0x4b64a8)?;outcome.total_turn=self.i16(0x4b64aa)?;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_matches_executed_original_machine_code() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-core.json")).unwrap();
        let player=LegacyPlayer::initialize();
        for (address,expected) in vectors["initialPlayer"]["words"].as_object().unwrap() {
            let address=u32::from_str_radix(address.trim_start_matches("0x"),16).unwrap();
            assert_eq!(i64::from(player.i16(address).unwrap()),expected.as_i64().unwrap(),"{address:x}");
        }
    }
    #[test]
    fn numeric_turn_progression_levels_and_recovery() {
        let mut player=LegacyPlayer::initialize();player.set_i16(0x4b6488,450).unwrap();player.set_i16(0x4b6486,0).unwrap();
        let outcome=player.settle_turn_numbers().unwrap();
        assert_eq!(outcome.levels_gained,2);assert_eq!(player.stats().max_hp,Some(136));assert_eq!(player.stats().hp,Some(136));assert_eq!(player.stats().money,Some(5000));
        for _ in 0..4 {player.settle_turn_numbers().unwrap();}
        assert_eq!(player.stats().age,Some(13));assert_eq!(player.life().year,2017);
    }
    #[test]
    fn reincarnation_preserves_only_mastered_skills_and_scales_previous_life(){
        let mut player=LegacyPlayer::initialize();player.set_i16(0x4b6488,1000).unwrap();player.set_i16(0x4b6492,6000).unwrap();
        player.set_i16(0x4b6508,3).unwrap();player.set_i16(0x4b650a,2).unwrap();player.set_i16(0x4b64a2,8).unwrap();
        player.reincarnate().unwrap();
        assert_eq!(player.stats().money,Some(50_000));assert_eq!(player.stats().experience,Some(300));
        assert_eq!(player.stats().intelligence,Some(800));assert_eq!(player.stats().max_hp,Some(118));
        assert_eq!(player.life().incarnation,2);assert_eq!(player.life().skills[0],3);assert_eq!(player.life().skills[1],0);
        assert_eq!(player.life().profession,-1);assert_eq!(player.life().stage,2);
    }
    #[test]
    fn reincarnation_matches_original_machine_code_for_every_packed_byte(){
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-reincarnation.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap(){
            let mut player=LegacyPlayer{memory:case["initialPlayer"].as_array().unwrap().iter().map(|byte|byte.as_u64().unwrap()as u8).collect()};
            player.reincarnate().unwrap();
            let expected=case["expectedPlayer"].as_array().unwrap().iter().map(|byte|byte.as_u64().unwrap()as u8).collect::<Vec<_>>();
            let mismatches=player.bytes().iter().zip(expected.iter()).enumerate().filter(|(_, (actual,expected))|actual!=expected).map(|(index,(actual,expected))|format!("{:x}: {actual} != {expected}",BASE+index as u32)).collect::<Vec<_>>();
            assert!(mismatches.is_empty(),"generation={} {}",case["generation"],mismatches.join(", "));
        }
    }
}
