//! Map spirits: placement (0x0041e130/0x0041e280), attachment duration
//! (0x00420200), and timed slot effects (0x0041f5c0).
use crate::{character::LegacyPlayer,engine::Coordinate,rng::ReplayRng};
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct Spirits {pub positions:[Coordinate;5],pub active:Option<usize>,pub expires_turn:i32}

impl Default for Spirits {
    fn default()->Self{Self{positions:std::array::from_fn(|_|Coordinate{row:-1,column:-1}),active:None,expires_turn:0}}
}

impl Spirits {
    pub const DURATIONS:[i32;5]=[8,15,10,6,6];

    pub fn initialize(pool:&[Coordinate],player:&Coordinate,rng:&mut ReplayRng)->Result<Self,String>{
        let mut spirits=Self::default();
        if pool.iter().filter(|point|*point!=player).map(|point|(point.row,point.column)).collect::<std::collections::BTreeSet<_>>().len()<5{return Err("Original spirit pool has fewer than five available positions".into());}
        for index in 0..5{spirits.reposition(index,pool,player,rng)?;}
        Ok(spirits)
    }

    pub fn reposition(&mut self,index:usize,pool:&[Coordinate],player:&Coordinate,rng:&mut ReplayRng)->Result<(),String>{
        if index>=5 || !pool.iter().any(|point|point!=player && self.positions.iter().enumerate().all(|(other,position)|other==index || position!=point)){
            return Err("Original spirit position pool has no unoccupied tile".into());
        }
        loop{
            let point=&pool[rng.next_rand()as usize%pool.len()];
            if point!=player && self.positions.iter().enumerate().all(|(other,position)|other==index || position!=point){self.positions[index]=point.clone();return Ok(());}
        }
    }

    pub fn change_map(&mut self,pool:&[Coordinate],player:&Coordinate,rng:&mut ReplayRng)->Result<(),String>{
        for index in 0..5{if self.active!=Some(index){self.reposition(index,pool,player,rng)?;}}
        Ok(())
    }

    pub fn encounter(&self,player:&Coordinate)->Option<usize>{self.positions.iter().enumerate().find(|(index,position)|self.active!=Some(*index) && *position==player).map(|(index,_)|index)}

    pub fn attach(&mut self,index:usize,total_turn:i16,pool:&[Coordinate],player:&Coordinate,rng:&mut ReplayRng)->Result<(),String>{
        if index>=5{return Err("Original spirit identifier is out of range".into());}
        if let Some(previous)=self.active{self.reposition(previous,pool,player,rng)?;}
        self.active=Some(index);self.expires_turn=i32::from(total_turn)+Self::DURATIONS[index];Ok(())
    }

    pub fn expire(&mut self,total_turn:i16,pool:&[Coordinate],player:&Coordinate,rng:&mut ReplayRng)->Result<Option<usize>,String>{
        if let Some(index)=self.active{if self.expires_turn<=i32::from(total_turn){self.reposition(index,pool,player,rng)?;self.active=None;return Ok(Some(index));}}
        Ok(None)
    }
}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum SpiritLotteryPhase{Spinning,Stopping,Result,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct SpiritDigit {pub value:i32,pub interval:u32,pub last_tick:u32,pub initial_frame:bool,pub frame_count:i32}
impl SpiritDigit {
    fn new(value:i32,interval:u32,frame_count:i32)->Self{Self{value,interval,last_tick:0,initial_frame:true,frame_count}}
    /// FUN_0045a990 updates at most once per original frame, with a strict >.
    pub fn tick(&mut self,now:u32){
        if now.wrapping_sub(self.last_tick)>self.interval{
            self.last_tick=now;
            if self.initial_frame{self.initial_frame=false;}else{self.value=(self.value+1)%self.frame_count;}
        }
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct SpiritLottery {
    pub spirit:usize,pub positive_money:bool,pub phase:SpiritLotteryPhase,pub clock_ms:u32,pub result_at:u32,
    pub digits:[SpiritDigit;4],pub attribute:SpiritDigit,pub stopping_digit:i8,pub changes:i16,pub delay:i32,pub leading_range:[i32;2],
}
impl SpiritLottery {
    pub fn new(spirit:usize,positive_money:bool,rng:&mut ReplayRng)->Result<Self,String>{
        if !(2..=4).contains(&spirit){return Err("This original spirit has no slot effect".into());}
        Ok(Self{spirit,positive_money,phase:SpiritLotteryPhase::Spinning,clock_ms:0,result_at:0,
            digits:std::array::from_fn(|_|SpiritDigit::new((rng.next_rand()%10)as i32,40,10)),attribute:SpiritDigit::new(0,100,4),
            stopping_digit:3,changes:0,delay:0,leading_range:[0,0]})
    }
    pub fn stop(&mut self,rng:&mut ReplayRng)->Result<(),String>{
        if self.phase!=SpiritLotteryPhase::Spinning{return Err("Spirit slots have already stopped".into());}
        let roll=rng.next_rand()%1000;
        self.leading_range=match roll{0..=499=>[1,2],500..=799=>[3,4],800..=899=>[5,5],900..=949=>[6,9],_=>[0,0]};
        self.phase=SpiritLotteryPhase::Stopping;Ok(())
    }
    pub fn tick(&mut self,elapsed_ms:u32)->Result<(),String>{
        if self.phase==SpiritLotteryPhase::Finished{return Err("Spirit slots have finished".into());}
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        if self.phase==SpiritLotteryPhase::Result{
            if self.clock_ms.wrapping_sub(self.result_at)>=1000{self.phase=SpiritLotteryPhase::Finished;}
            return Ok(());
        }
        let first=if self.spirit==2{0}else{1};
        if self.phase==SpiritLotteryPhase::Spinning{
            if self.spirit!=2{self.attribute.tick(self.clock_ms);}
            for digit in &mut self.digits[first..]{digit.tick(self.clock_ms);}
        }else{
            for index in first..=self.stopping_digit as usize{
                let digit=&mut self.digits[index];let previous=digit.value;digit.tick(self.clock_ms);
                if index==self.stopping_digit as usize && digit.value!=previous{
                    self.changes=self.changes.wrapping_add(1);self.delay+=i32::from(self.changes)*if self.spirit==2{10}else{5};digit.interval=self.delay as u32;
                }
            }
            let complete=if self.spirit!=2 && self.stopping_digit==1{
                (self.leading_range[0]..=self.leading_range[1]).contains(&self.digits[1].value)
            }else if self.changes>3{
                self.changes=0;self.delay=0;self.stopping_digit-=1;self.stopping_digit<first as i8
            }else{false};
            if complete{self.phase=SpiritLotteryPhase::Result;self.result_at=self.clock_ms;}
        }
        Ok(())
    }
    pub fn amount(&self)->i32{
        if self.spirit==2{self.digits.iter().fold(0,|value,digit|value*10+digit.value)}else{self.digits[1..].iter().fold(0,|value,digit|value*10+digit.value)}
    }
    pub fn apply(&self,player:&mut LegacyPlayer)->Result<(),String>{
        if self.phase!=SpiritLotteryPhase::Finished{return Err("Spirit slot result has not completed".into());}
        if self.spirit==2{
            let amount=self.amount()*if self.positive_money{1}else{-1};
            player.set_i32(0x4b648a,player.i32(0x4b648a)?.wrapping_add(amount).clamp(0,99_999_999))?;
        }else{
            let address=*[0x4b6492,0x4b6490,0x4b6494,0x4b648e].get(self.attribute.value as usize).ok_or("Invalid original spirit attribute")?;
            let current=player.i16(address)?;let amount=self.amount()as i16;
            let result=if self.spirit==3{current.wrapping_add(amount).min(9999)}else{current.wrapping_sub(amount).max(0)};
            player.set_i16(address,result)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn spirits_match_original_x86_placement_digit_frames_and_numeric_results(){
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-spirits.json")).unwrap();
        let maps:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/maps.json")).unwrap();
        for case in vectors["placement"].as_array().unwrap(){
            let map=maps["maps"].as_array().unwrap().iter().find(|map|map["id"]==case["map"]).unwrap();
            let pool:Vec<Coordinate>=serde_json::from_value(map["coordinateIndex"].clone()).unwrap();let player:Coordinate=serde_json::from_value(map["start"].clone()).unwrap();
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());let mut spirits=Spirits::initialize(&pool,&player,&mut rng).unwrap();
            assert_eq!(serde_json::to_value(&spirits.positions).unwrap(),case["initial"]);assert_eq!(rng.draws,case["draws"].as_u64().unwrap());assert_eq!(u64::from(rng.state),case["rngState"].as_u64().unwrap());
            for respawn in case["respawns"].as_array().unwrap(){
                spirits.reposition(respawn["index"].as_u64().unwrap()as usize,&pool,&player,&mut rng).unwrap();
                assert_eq!(serde_json::to_value(&spirits.positions).unwrap(),respawn["positions"]);assert_eq!(rng.draws,respawn["draws"].as_u64().unwrap());assert_eq!(u64::from(rng.state),respawn["rngState"].as_u64().unwrap());
            }
        }
        for case in vectors["digits"].as_array().unwrap(){
            let mut digit=SpiritDigit::new(3,case["interval"].as_u64().unwrap()as u32,case["count"].as_i64().unwrap()as i32);
            for frame in case["frames"].as_array().unwrap(){
                digit.tick(frame["time"].as_u64().unwrap()as u32);
                assert_eq!(i64::from(digit.value),frame["value"].as_i64().unwrap());assert_eq!(u64::from(digit.last_tick),frame["lastTick"].as_u64().unwrap());assert_eq!(digit.initial_frame,frame["initialFrame"].as_bool().unwrap());
            }
        }
        for case in vectors["rewards"].as_array().unwrap(){
            let spirit=case["spirit"].as_u64().unwrap()as usize;let attribute=case["attribute"].as_u64().unwrap()as usize;
            let mut game=SpiritLottery::new(spirit,case["positive"].as_bool().unwrap(),&mut ReplayRng::new(1)).unwrap();game.phase=SpiritLotteryPhase::Finished;game.attribute.value=attribute as i32;
            for(digit,value)in game.digits.iter_mut().zip(case["digits"].as_array().unwrap()){digit.value=value.as_i64().unwrap()as i32;}
            let address=if spirit==2{0x4b648a}else{[0x4b6492,0x4b6490,0x4b6494,0x4b648e][attribute]};let mut player=LegacyPlayer::initialize();let before=case["before"].as_i64().unwrap();
            if spirit==2{player.set_i32(address,before as i32).unwrap();}else{player.set_i16(address,before as i16).unwrap();}game.apply(&mut player).unwrap();
            let actual=if spirit==2{i64::from(player.i32(address).unwrap())}else{i64::from(player.i16(address).unwrap())};assert_eq!(actual,case["after"].as_i64().unwrap(),"{case}");
        }
    }
    #[test]
    fn spirit_placement_and_expiry_reject_occupied_tiles_and_preserve_draws(){
        let pool=(0..8).map(|column|Coordinate{row:1,column}).collect::<Vec<_>>();let player=pool[0].clone();let mut rng=ReplayRng::new(42);
        let mut spirits=Spirits::initialize(&pool,&player,&mut rng).unwrap();
        assert_eq!(spirits.positions.iter().map(|point|point.column).collect::<std::collections::BTreeSet<_>>().len(),5);assert!(spirits.positions.iter().all(|point|point!=&player));
        let encounter=spirits.positions[2].clone();spirits.attach(2,10,&pool,&encounter,&mut rng).unwrap();assert_eq!(spirits.expires_turn,20);
        let draws=rng.draws;assert_eq!(spirits.expire(19,&pool,&encounter,&mut rng).unwrap(),None);assert_eq!(rng.draws,draws);
        assert_eq!(spirits.expire(20,&pool,&encounter,&mut rng).unwrap(),Some(2));assert_ne!(spirits.positions[2],encounter);
    }
    #[test]
    fn spirit_slots_save_and_resume_keep_the_next_gameplay_random_value(){
        for spirit in 2..=4{
            let mut rng=ReplayRng::new(7);let mut game=SpiritLottery::new(spirit,true,&mut rng).unwrap();assert_eq!(rng.draws,4);
            for _ in 0..20{game.tick(17).unwrap();}game.stop(&mut rng).unwrap();assert_eq!(rng.draws,5);
            let mut restored:SpiritLottery=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
            for _ in 0..5000{if game.phase==SpiritLotteryPhase::Finished{break;}game.tick(17).unwrap();restored.tick(17).unwrap();}
            assert_eq!(game,restored);assert_eq!(game.phase,SpiritLotteryPhase::Finished);assert_eq!(rng.draws,5);
            if spirit!=2{assert!((game.leading_range[0]..=game.leading_range[1]).contains(&game.digits[1].value));}
        }
    }
}
