//! Fire mini-game (native entry 0x00451690). Decorative animation draws share
//! the original CRT RNG and therefore remain in the deterministic core.
use crate::{bonus::BonusAnimation,rng::ReplayRng};
use serde::{Deserialize,Serialize};
fn is_zero_u8(value:&u8)->bool{*value==0}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum FirePhase {Ready,Playing,Finished}
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum FireInput {Left,Right,ReleaseLeft,ReleaseRight}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FireTrack {pub frame:u32,pub last_tick:u32,pub first:bool}
impl Default for FireTrack {fn default()->Self {Self {frame:0,last_tick:0,first:true}}}
impl FireTrack {
    fn tick(&mut self,now:u32,count:u32,period:u32) {
        if now.wrapping_sub(self.last_tick)>period {
            self.last_tick=now;
            if self.first {self.first=false;}else{self.frame=(self.frame+1)%count;}
        }
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FireGame {
    pub phase:FirePhase,
    pub clock_ms:u32,
    pub failed:bool,
    pub finished:bool,
    pub duration:u32,
    pub gauge:i32,
    pub started_at:u32,
    pub elapsed:u32,
    pub won_at_elapsed:u32,
    pub fade_at:u32,
    pub hand:bool,
    pub mouse_armed:bool,
    pub body_state:u32,
    pub body_rest:u32,
    pub body_frame:u32,
    pub body_at:u32,
    pub sparks_state:u32,
    pub sparks_frame:u32,
    pub sparks_at:u32,
    pub sparks_pending:bool,
    pub actor_type:u32,
    pub actor_x:i32,
    pub actor_y:i32,
    pub won_x:i32,
    pub won_y:i32,
    pub actor_step:u32,
    pub actor_at:u32,
    pub actor_hold:bool,
    pub win_stage:u32,
    pub lightning_active:bool,
    pub lightning_frame:u32,
    pub lightning_at:u32,
    #[serde(default,skip_serializing_if="is_zero_u8")]
    pub lightning_sound:u8,
    pub fire_flash:bool,
    pub flash_at:u32,
    pub flash_delay:u32,
    pub shake_active:bool,
    pub shake_at:u32,
    pub shake_frame:u32,
    pub shake_offset:i32,
    pub help_state:u32,
    pub help_at:u32,
    pub help_x:i32,
    pub help_y:i32,
    pub win_banner:bool,
    pub banner_at:u32,
    pub loss_banner:bool,
    pub loss_frame:u32,
    pub loss_at:u32,
    pub loss_animation:bool,
    pub intro:bool,
    pub intro_at:u32,
    pub random_overlay:bool,
    pub random_at:u32,
    pub reward_started:bool,
    pub reward_percent:i32,
    pub tracks:[FireTrack;6],
    pub starting_charm:i16,
    pub charm:i16,
    pub reward:Option<BonusAnimation>,
    pub reward_skip:bool,
}

const ACTOR_MOTION:[[i32;3];5]=[[0,0,1000],[0,-1,100],[0,-2,100],[0,-3,200],[0,1,50]];
const SHAKE_Y:[i32;5]=[4,-3,2,-1,1];

impl FireGame {
    pub fn new(charm:i16)->Self {
        Self {phase:FirePhase::Ready,clock_ms:0,failed:false,finished:false,duration:20500,gauge:0,
            started_at:0,elapsed:0,won_at_elapsed:0,fade_at:0,hand:false,mouse_armed:true,
            body_state:0,body_rest:0,body_frame:0,body_at:0,sparks_state:0,sparks_frame:0,sparks_at:0,sparks_pending:false,
            actor_type:0,actor_x:290,actor_y:341,won_x:0,won_y:0,actor_step:0,actor_at:0,actor_hold:false,
            win_stage:0,lightning_active:false,lightning_frame:0,lightning_at:0,lightning_sound:0,fire_flash:false,flash_at:0,flash_delay:0,
            shake_active:false,shake_at:0,shake_frame:0,shake_offset:0,help_state:0,help_at:0,help_x:0,help_y:0,
            win_banner:false,banner_at:0,loss_banner:false,loss_frame:0,loss_at:0,loss_animation:false,
            intro:true,intro_at:0,random_overlay:false,random_at:0,reward_started:false,reward_percent:0,
            tracks:std::array::from_fn(|_|FireTrack::default()),starting_charm:charm,charm,reward:None,reward_skip:false}
    }
    pub fn start(&mut self)->Result<(),String> {
        if self.phase!=FirePhase::Ready {return Err("Fire game has already started".into());}
        self.phase=FirePhase::Playing;self.started_at=self.clock_ms;self.actor_at=self.clock_ms;self.intro_at=self.clock_ms;Ok(())
    }
    /// The original key dispatcher checks press-left, press-right, release-left,
    /// release-right in that order and handles one event per render iteration.
    pub fn input(&mut self,input:FireInput)->Result<(),String> {
        if self.phase!=FirePhase::Playing {return Err("Fire game is not playing".into());}
        match input {
            FireInput::Left if !self.hand=>{self.hand=true;self.body_rest=1;self.spark();}
            FireInput::Right if self.hand=>{self.hand=false;self.body_rest=2;self.spark();}
            FireInput::ReleaseLeft if self.body_rest==1=>self.body_rest=0,
            FireInput::ReleaseRight if self.body_rest==2=>self.body_rest=0,
            _=>{}
        }
        Ok(())
    }
    fn spark(&mut self) {if self.gauge<1000 {self.sparks_pending=true;self.gauge=self.gauge.wrapping_add(10);}}
    pub fn body_sprite_frame(&self)->u32 {
        match self.body_state {0=>0,1=>2,2=>1,3=>3+self.body_frame%2,4=>5+self.body_frame%2,5=>7+self.body_frame.min(3),_=>0}
    }
    pub fn sparks_sprite_frame(&self)->Option<u32> {
        match self.sparks_state {1=>Some([4,3][self.sparks_frame.min(1) as usize]),2=>Some(self.sparks_frame%2),3=>Some(2),_=>None}
    }
    pub fn remaining_seconds(&self)->u32 {
        if self.failed {0}else{self.duration.wrapping_sub(if self.win_stage==0 {self.elapsed}else{self.won_at_elapsed})/1000%100}
    }
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng)->Result<(),String> {
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        if self.phase!=FirePhase::Playing {return Ok(());}
        let now=self.clock_ms;
        self.elapsed=now.wrapping_sub(self.started_at);
        if self.win_stage==0 && !self.failed && self.duration<self.elapsed {
            self.body_state=4;self.body_frame=0;self.body_at=now;self.actor_hold=true;self.actor_at=now;self.failed=true;
        }
        if self.win_stage==0 && self.elapsed%10==0 {self.gauge=self.gauge.wrapping_sub(5).max(0);}
        match self.body_state {
            0..=2=>{
                if self.elapsed%2000<1991 || rng.next_rand()&1==0 {self.body_state=self.body_rest;self.body_frame=0;}
                else {self.body_state=3;self.body_frame=0;self.body_at=now;}
            }
            3 if now.wrapping_sub(self.body_at)>25=>{
                self.body_at=now;self.body_frame+=1;
                if self.body_frame>15 {self.body_state=self.body_rest;self.body_frame=0;}
            }
            4 if now.wrapping_sub(self.body_at)>30=>{self.body_at=now;self.body_frame=(self.body_frame+1)%2;}
            5 if now.wrapping_sub(self.body_at)>[1000,250,100,100][self.body_frame.min(3) as usize]=>{
                self.body_at=now;self.body_frame+=1;if self.body_frame>3 {self.body_frame=2;}
            }
            _=>{}
        }
        if self.win_stage==0 && !self.failed && self.gauge>999 {
            self.win_stage=1;self.reward_percent=if self.elapsed<12501 {10}else if self.elapsed<15501 {5}else{3};
            self.won_at_elapsed=self.elapsed;self.won_x=self.actor_x;self.won_y=self.actor_y;
            self.fire_flash=false;self.flash_at=now;self.flash_delay=60;self.actor_hold=true;self.actor_at=now;
        }
        if self.win_stage==1 {
            if self.tracks[0].frame>=7 {
                self.win_stage=2;self.body_state=5;self.body_frame=0;self.body_at=now;self.banner_at=now;
                self.tracks[1].tick(now,8,80);
            } else {self.tracks[0].tick(now,8,80);}
        } else if self.win_stage==2 {self.tracks[1].tick(now,8,80);}
        match self.sparks_state {
            0 if self.sparks_pending=>{self.sparks_state=1;self.sparks_frame=0;self.sparks_at=now;self.sparks_pending=false;}
            1 if now.wrapping_sub(self.sparks_at)>50=>{
                self.sparks_at=now;self.sparks_frame+=1;
                if self.sparks_frame>1 {self.sparks_state=2;self.sparks_frame=0;}
            }
            2 if now.wrapping_sub(self.sparks_at)>50=>{
                self.sparks_at=now;self.sparks_frame+=1;
                if self.sparks_frame>1 {
                    self.sparks_frame=0;
                    if self.sparks_pending {self.sparks_pending=false;}else{self.sparks_state=3;}
                }
            }
            3 if now.wrapping_sub(self.sparks_at)>50=>{self.sparks_at=now;self.sparks_frame+=1;if self.sparks_frame!=0 {self.sparks_state=0;}}
            _=>{}
        }
        if self.actor_hold {
            if now.wrapping_sub(self.actor_at)>1000 {
                self.actor_at=now;self.actor_hold=false;
                if self.failed {self.actor_type=1;self.fade_at=now;}
            }
        } else if !self.failed {
            if self.win_stage==0 {
                if !self.shake_active && self.actor_step==4 {self.shake_active=true;self.shake_frame=0;self.shake_at=now;}
                if now.wrapping_sub(self.actor_at)>ACTOR_MOTION[self.actor_step as usize][2] as u32 {
                    self.actor_at=now;self.actor_step=(self.actor_step+1)%5;
                    let offset=ACTOR_MOTION[self.actor_step as usize];
                    self.actor_x=(-20.0*f64::from(self.elapsed)/f64::from(self.duration)+f64::from(290+offset[0])).trunc() as i32;
                    self.actor_y=(-78.0*f64::from(self.elapsed)/f64::from(self.duration)+f64::from(341+offset[1])).trunc() as i32;
                }
            } else {
                let elapsed=f64::from(self.elapsed.wrapping_sub(self.won_at_elapsed).wrapping_sub(1000));
                let duration=f64::from(self.won_at_elapsed.wrapping_mul(1500)/self.duration);
                self.actor_x=(elapsed*f64::from(290-self.won_x)/duration+f64::from(self.won_x)).trunc() as i32;
                self.actor_y=(elapsed*f64::from(341-self.won_y)/duration+f64::from(self.won_y)).trunc() as i32;
            }
        }
        if !self.lightning_active {
            if self.elapsed%1500>1490 && rng.next_rand()&1!=0 {
                self.lightning_active=true;self.lightning_frame=0;
                self.lightning_sound=if rng.next_rand()&1==0 {6}else{5};
            }
        } else if now.wrapping_sub(self.lightning_at)>[80,50,80,0][self.lightning_frame.min(3) as usize] {
            self.lightning_frame+=1;self.lightning_at=now;if self.lightning_frame>3 {self.lightning_active=false;}
        }
        if self.win_stage!=0 {
            if !self.fire_flash {
                if now.wrapping_sub(self.flash_at)>self.flash_delay {self.fire_flash=true;self.flash_at=now;self.flash_delay=rng.next_rand()%40+120;}
            } else if now.wrapping_sub(self.flash_at)>self.flash_delay {self.fire_flash=false;self.flash_at=now;self.flash_delay=60;}
        }
        if self.shake_active && now.wrapping_sub(self.shake_at)>30 {
            self.shake_frame+=1;
            if self.shake_frame<5 {self.shake_at=now;self.shake_offset=SHAKE_Y[self.shake_frame as usize];}
            else {self.shake_active=false;self.shake_offset=0;}
        }
        if self.help_state==1 {
            self.help_x=133;self.help_y=(now.wrapping_sub(self.help_at)*414/250) as i32-348;
            if self.help_y>=66 {self.help_state=3;}
        } else if self.help_state==2 {
            self.help_x=133;self.help_y=66-(now.wrapping_sub(self.help_at)*414/250) as i32;
            if self.help_y<=-348 {self.help_state=0;}
        }
        if self.win_stage==2 {
            if !self.win_banner {
                if now.wrapping_sub(self.banner_at)>2000 {self.win_banner=true;self.banner_at=now;}
            } else if !self.reward_started {
                self.tracks[3].tick(now,5,100);
                if now.wrapping_sub(self.banner_at)>3000 {
                    let current=i32::from(self.charm);
                    let delta=(current*self.reward_percent/100).min(9999-current);
                    self.reward=Some(BonusAnimation::new(current,delta));
                    self.charm=self.charm.wrapping_add(delta as i16);self.reward_started=true;
                }
            }
        }
        if self.loss_animation {
            if self.tracks[2].frame<11 {self.tracks[2].tick(now,12,40);}
            else {self.loss_animation=false;self.loss_banner=true;self.loss_frame=0;self.loss_at=now;}
        }
        if self.loss_banner && now.wrapping_sub(self.loss_at)>30 {
            self.loss_at=now;self.loss_frame+=1;if self.loss_frame>32 {self.finished=true;}
        }
        if self.intro {
            if now.wrapping_sub(self.intro_at)<3001 {self.tracks[4].tick(now,2,100);}else{self.intro=false;}
        }
        if self.random_overlay {
            if now.wrapping_sub(self.random_at)<2501 {self.tracks[5].tick(now,2,50);}else{self.random_overlay=false;}
        } else if !self.intro && self.elapsed%1000>990 && rng.next_rand()&1!=0 {self.random_at=now;self.random_overlay=true;}
        // Native renderer performs these state changes after update, preserving
        // the same RNG order as the tick above.
        if self.reward_started {
            let active=self.reward.as_mut().ok_or("Fire bonus is missing")?.tick(now,rng,self.reward_skip);
            self.reward_skip=false;
            if !active {self.finished=true;}
        }
        if self.actor_type==1 && !self.loss_animation && !self.loss_banner && now.wrapping_sub(self.fade_at)>=500 {self.loss_animation=true;}
        if self.finished {self.phase=FirePhase::Finished;}
        Ok(())
    }
    pub fn final_charm(&self)->Result<i16,String> {
        if self.phase!=FirePhase::Finished {return Err("Fire game is not finished".into());}Ok(self.charm)
    }
    pub fn finish_reward_animation(&mut self)->Result<(),String> {
        if !self.reward_started {return Err("Fire reward has not started".into());}self.reward_skip=true;Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fire_frames_match_original_x86_including_decorative_rng() {
        let data:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-fire.json")).unwrap();
        for (case_index,case) in data["cases"].as_array().unwrap().iter().enumerate() {
            let mut value=serde_json::to_value(FireGame::new(1000)).unwrap();
            for (key,item) in case["initial"].as_object().unwrap() {value[key]=item.clone();}
            let mut game:FireGame=serde_json::from_value(value).unwrap();game.clock_ms=case["start"].as_u64().unwrap() as u32;game.start().unwrap();
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            for row in case["frames"].as_array().unwrap() {
                if let Some(action)=row["action"].as_str() {game.input(serde_json::from_value(serde_json::Value::String(action.into())).unwrap()).unwrap();}
                let now=row["time"].as_u64().unwrap() as u32;game.tick(now.wrapping_sub(game.clock_ms),&mut rng).unwrap();
                let actual=serde_json::to_value(&game).unwrap();
                for (name,expected) in data["fields"].as_array().unwrap().iter().zip(row["after"].as_array().unwrap()) {
                    let key=name.as_str().unwrap();
                    if key=="rngState" {assert_eq!(u64::from(rng.state),expected.as_u64().unwrap(),"case {case_index} at {now}");continue;}
                    if key=="rngDraws" {assert_eq!(rng.draws,expected.as_u64().unwrap(),"case {case_index} at {now}");continue;}
                    if key=="bonus" {
                        if let Some(bonus)=&game.reward {
                            let fields=[bonus.value,bonus.remaining_delta,bonus.value_last_tick as i32,i32::from(bonus.value_phase),bonus.increment,bonus.increment_last_tick as i32,i32::from(bonus.increment_phase),bonus.step_interval as i32,i32::from(bonus.digit_count)];
                            assert_eq!(serde_json::to_value(fields).unwrap(),*expected,"case {case_index} at {now} bonus");
                        }else{assert!(expected.is_null());}
                        continue;
                    }
                    if key=="tracks" {
                        for (track,expected) in game.tracks.iter().zip(expected.as_array().unwrap()) {
                            assert_eq!(u64::from(track.frame),expected["frame"].as_u64().unwrap(),"case {case_index} at {now} track frame");
                            assert_eq!(u64::from(track.last_tick),expected["lastTick"].as_u64().unwrap());
                            assert_eq!(i64::from(track.first),expected["first"].as_i64().unwrap());
                        }
                        continue;
                    }
                    if let Some(boolean)=actual[key].as_bool() {assert_eq!(i64::from(boolean),expected.as_i64().unwrap(),"case {case_index} at {now}: {key}");}
                    else {assert_eq!(&actual[key],expected,"case {case_index} at {now}: {key}");}
                }
            }
        }
    }

    #[test]
    fn alternate_inputs_and_save_resume_keep_result_and_rng() {
        let mut game=FireGame::new(9900);game.start().unwrap();let mut rng=ReplayRng::new(42);
        game.input(FireInput::Right).unwrap();assert_eq!(game.gauge,0);
        game.input(FireInput::Left).unwrap();game.input(FireInput::Left).unwrap();assert_eq!(game.gauge,10);
        game.input(FireInput::Right).unwrap();assert_eq!(game.gauge,20);
        for index in 0..120 {
            game.input(if index%2==0 {FireInput::Left}else{FireInput::Right}).unwrap();game.tick(16,&mut rng).unwrap();
        }
        assert_ne!(game.win_stage,0);
        let mut copy:FireGame=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();let mut copy_rng=rng.clone();
        for _ in 0..1200 {
            game.tick(16,&mut rng).unwrap();copy.tick(16,&mut copy_rng).unwrap();
            if game.phase==FirePhase::Finished {break;}
        }
        assert_eq!(game,copy);assert_eq!(rng,copy_rng);assert_eq!(game.final_charm().unwrap(),9999);
    }
}
