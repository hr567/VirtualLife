//! Native fist/target game (0x0040e1e0). Timed animation advances at most once
//! per original render-loop iteration, including after a delayed frame.
use crate::{bonus::BonusAnimation,rng::ReplayRng};
use serde::{Deserialize,Serialize};

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum FistPhase {Ready,Playing,Help,ResultPause,Won,Lost,LostShake,LostWait,Reward,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FistGame {
    pub phase:FistPhase,
    pub clock_ms:u32,
    pub player_x:i16,
    pub player_state:i32,
    pub strike_frame:i16,
    pub strike_last_tick:u32,
    pub target_x:i16,
    /// 0/1 normal left/right, 2/3 happy, 4/5 angry, 6 stunned.
    pub target_state:i32,
    pub resume_state:i32,
    pub target_speed:i16,
    pub target_frame:i16,
    pub target_last_tick:u32,
    pub target_interval:u32,
    pub combo:i16,
    pub misses:i16,
    /// The original adds a packed combo/miss DWORD; display and awards use
    /// only its signed low word. Preserve the full DWORD for save fidelity.
    pub score:i32,
    pub remaining_seconds:i32,
    pub second_last_tick:u32,
    pub left:bool,
    pub right:bool,
    pub fire:bool,
    pub phase_last_tick:u32,
    pub win_frame:u8,
    pub loss_y:i32,
    pub loss_velocity:i32,
    pub loss_shake_frame:usize,
    pub starting_morality:i16,
    pub reward:Option<BonusAnimation>,
    pub reward_skip:bool,
}

impl Default for FistGame {fn default()->Self {Self::new(0)}}

impl FistGame {
    pub fn new(morality:i16)->Self {
        Self {phase:FistPhase::Ready,clock_ms:0,player_x:258,player_state:0,
            strike_frame:-1,strike_last_tick:0,target_x:21,target_state:1,resume_state:0,
            target_speed:4,target_frame:-1,target_last_tick:0,target_interval:50,
            combo:0,misses:0,score:0,remaining_seconds:60,second_last_tick:0,
            left:false,right:false,fire:false,phase_last_tick:0,win_frame:0,
            loss_y:-75,loss_velocity:32,loss_shake_frame:0,starting_morality:morality,
            reward:None,reward_skip:false}
    }

    pub fn start(&mut self)->Result<(),String> {
        if !matches!(self.phase,FistPhase::Ready|FistPhase::Help) {return Err("Fist game help is not open".into());}
        self.phase=FistPhase::Playing;Ok(())
    }

    pub fn show_help(&mut self)->Result<(),String> {
        if self.phase!=FistPhase::Playing {return Err("Fist game is not playing".into());}
        self.phase=FistPhase::Help;self.left=false;self.right=false;self.fire=false;Ok(())
    }

    pub fn input(&mut self,left:bool,right:bool,fire:bool) {self.left=left;self.right=right;self.fire|=fire;}
    pub fn display_score(&self)->i16 {self.score as i16}
    pub fn win_visible(&self)->bool {self.phase==FistPhase::Won && self.win_frame%2==0}
    pub fn loss_shift(&self)->i32 {if self.phase==FistPhase::LostShake {loss_shake_frames().get(self.loss_shake_frame).copied().unwrap_or(-2)}else if self.phase==FistPhase::LostWait {-2}else{0}}

    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng)->Result<(),String> {
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        let now=self.clock_ms;
        match self.phase {
            FistPhase::Playing=>{
                self.play_frame(now);
                if self.remaining_seconds as i16==0 {self.phase=FistPhase::ResultPause;self.phase_last_tick=now;self.left=false;self.right=false;}
            }
            FistPhase::ResultPause if now.wrapping_sub(self.phase_last_tick)>=1000=>{
                self.phase=if self.display_score()<20 {FistPhase::Lost}else{FistPhase::Won};self.phase_last_tick=now;
            }
            FistPhase::Won if now.wrapping_sub(self.phase_last_tick)>=if self.win_frame%2==0 {200}else{100}=>{
                self.win_frame+=1;self.phase_last_tick=now;
                if self.win_frame==10 {
                    let delta=self.reward_delta(self.starting_morality);
                    self.reward=Some(BonusAnimation::new(i32::from(self.starting_morality),i32::from(delta)));
                    self.phase=FistPhase::Reward;
                }
            }
            FistPhase::Lost if now.wrapping_sub(self.phase_last_tick)>=10=>{
                let next=self.loss_y+self.loss_velocity;
                self.loss_velocity=(self.loss_velocity-2).max(8);
                self.phase_last_tick=now;
                if next>202 {self.phase=FistPhase::LostShake;}else{self.loss_y=next;}
            }
            FistPhase::LostShake=>{
                self.loss_shake_frame+=1;
                if self.loss_shake_frame>=loss_shake_frames().len() {self.phase=FistPhase::LostWait;self.phase_last_tick=now;}
            }
            FistPhase::LostWait if now.wrapping_sub(self.phase_last_tick)>=1000=>self.phase=FistPhase::Finished,
            _=>{}
        }
        if self.phase==FistPhase::Reward {
            let active=self.reward.as_mut().ok_or("Fist reward is missing")?.tick(now,rng,self.reward_skip);
            self.reward_skip=false;
            if !active {self.phase=FistPhase::Finished;}
        }
        self.fire=false;
        Ok(())
    }

    fn play_frame(&mut self,now:u32) {
        if self.left && (0..=2).contains(&self.player_state) {self.player_state=i32::from(i32::from(self.player_x)-2>=141);}
        if self.right && (0..=2).contains(&self.player_state) {self.player_state=if i32::from(self.player_x)+2<=411 {2}else{0};}
        if self.fire && (0..=2).contains(&self.player_state) && self.target_state!=6 {self.player_state=3;}
        match self.player_state {
            1=>{self.player_x=self.player_x.wrapping_sub(2);self.player_state=0;}
            2=>{self.player_x=self.player_x.wrapping_add(2);self.player_state=0;}
            3 if (now.wrapping_sub(self.strike_last_tick) as i32)>150=>{
                self.strike_frame=self.strike_frame.wrapping_add(1);
                if self.strike_frame>=8 {self.strike_frame=-1;self.player_state=0;}
                self.strike_last_tick=now;
            }
            _=>{}
        }
        if self.strike_frame==7 && self.player_state==3 && self.target_state!=6 {
            if i32::from(self.player_x)+38<i32::from(self.target_x)+102 && i32::from(self.target_x)+48<i32::from(self.player_x)+66 {
                self.combo=self.combo.wrapping_add(1);
                let packed=i32::from(self.combo as u16)|(i32::from(self.misses)<<16);
                self.score=self.score.wrapping_add(packed);
                self.target_speed=self.target_speed.wrapping_add(2).min(24);
                self.resume_state=self.target_state;self.target_state=6;self.target_frame=-1;
                self.misses=0;
            } else {
                self.target_speed=self.target_speed.wrapping_sub(1).max(2);
                self.misses=self.misses.wrapping_add(1);self.combo=0;
            }
            self.player_state=0;self.strike_frame=-1;
        }
        if (now.wrapping_sub(self.target_last_tick) as i32)>self.target_interval as i32 {
            let left=i32::from(self.target_x)-i32::from(self.target_speed)<21;
            let right=i32::from(self.target_x)+i32::from(self.target_speed)>471;
            match self.target_state {
                0=>{
                    if left {self.target_state=if self.misses>=2 {3}else if self.combo>=2 {5}else{1};}
                    else {self.target_x=self.target_x.wrapping_sub(self.target_speed);if self.misses>=2 {self.target_state=2;}else if self.combo>=2 {self.target_state=4;}}
                }
                1=>{
                    if right {self.target_state=if self.misses>=2 {2}else if self.combo>=2 {4}else{0};}
                    else {self.target_x=self.target_x.wrapping_add(self.target_speed);if self.misses>=2 {self.target_state=3;}else if self.combo>=2 {self.target_state=5;}}
                }
                2=>{
                    if left {self.target_state=if self.combo>0 {1}else{3};}
                    else {self.target_x=self.target_x.wrapping_sub(self.target_speed);if self.combo>0 {self.target_state=0;}}
                }
                3=>{
                    if right {self.target_state=if self.combo>0 {0}else{2};}
                    else {self.target_x=self.target_x.wrapping_add(self.target_speed);if self.combo>0 {self.target_state=1;}}
                }
                4=>{
                    if left {self.target_state=if self.misses>0 {1}else{5};}
                    else {self.target_x=self.target_x.wrapping_sub(self.target_speed);if self.misses>0 {self.target_state=0;}}
                }
                5=>{
                    if right {self.target_state=if self.misses>0 {0}else{4};}
                    else {self.target_x=self.target_x.wrapping_add(self.target_speed);if self.misses>0 {self.target_state=1;}}
                }
                6=>self.target_interval=100,
                _=>{}
            }
            self.target_frame=self.target_frame.wrapping_add(1);
            if self.target_frame>=4 {
                self.target_frame=0;
                if self.target_state==6 {self.target_state=self.resume_state;self.target_interval=50;}
            }
            self.target_last_tick=now;
        }
        if (now.wrapping_sub(self.second_last_tick) as i32)>=1000 {self.remaining_seconds=self.remaining_seconds.wrapping_sub(1);self.second_last_tick=now;}
    }

    fn reward_delta(&self,current:i16)->i16 {
        let score=self.display_score();
        let percent=if score>=40 {10}else if score>=30 {5}else if score>=20 {3}else{0};
        let delta=(i32::from(current)*percent/100) as i16;
        if i32::from(current)+i32::from(delta)>9999 {9999i16.wrapping_sub(current)}else{delta}
    }
    pub fn final_morality(&self,current:i16)->Result<i16,String> {
        if !matches!(self.phase,FistPhase::Reward|FistPhase::Finished) {return Err("Fist result is not ready".into());}
        Ok(current.wrapping_add(self.reward_delta(current)).min(9999))
    }
    pub fn finish_reward_animation(&mut self)->Result<(),String> {
        if self.phase!=FistPhase::Reward {return Err("Fist reward is not showing".into());}
        self.reward_skip=true;Ok(())
    }
}

/// FUN_004107f0(8,12,25); the unused third argument does not insert a delay.
pub fn loss_shake_frames()->Vec<i32> {
    let mut frames=Vec::new();
    for bounce in 0..8 {
        let initial=12-bounce*12/8;
        frames.push(0);
        let mut y=initial;
        let mut velocity=initial-2;
        while y>0 {frames.push(-y);y+=velocity;velocity-=2;}
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fist_frames_match_original_x86() {
        let data:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-fist.json")).unwrap();
        for (index,sequence) in data["sequences"].as_array().unwrap().iter().enumerate() {
            let mut value=serde_json::to_value(FistGame::new(1000)).unwrap();
            for (key,item) in sequence["initial"].as_object().unwrap() {value[key]=item.clone();}
            let mut game:FistGame=serde_json::from_value(value).unwrap();game.start().unwrap();
            let mut rng=ReplayRng::new(42);
            for row in sequence["frames"].as_array().unwrap() {
                game.input(row["left"].as_bool().unwrap_or_else(||row["left"].as_i64().unwrap_or(0)!=0),row["right"].as_bool().unwrap_or_else(||row["right"].as_i64().unwrap_or(0)!=0),row["fire"].as_bool().unwrap_or_else(||row["fire"].as_i64().unwrap_or(0)!=0));
                let now=row["time"].as_u64().unwrap() as u32;
                game.tick(now.wrapping_sub(game.clock_ms),&mut rng).unwrap();
                let actual=serde_json::to_value(&game).unwrap();
                for (key,expected) in row["after"].as_object().unwrap() {assert_eq!(&actual[key],expected,"sequence {index}, time {now}, {key}");}
                assert_eq!(rng.draws,0);
            }
        }
    }

    #[test]
    fn save_resume_preserves_inputs_and_reward_rng() {
        let mut game=FistGame::new(9876);game.phase=FistPhase::Won;game.score=40;game.win_frame=9;
        let mut rng=ReplayRng::new(1999);game.tick(101,&mut rng).unwrap();
        assert_eq!(game.phase,FistPhase::Reward);
        let mut copy:FistGame=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();let mut copy_rng=rng.clone();
        for _ in 0..1000 {game.tick(16,&mut rng).unwrap();copy.tick(16,&mut copy_rng).unwrap();if game.phase==FistPhase::Finished {break;}}
        assert_eq!(game,copy);assert_eq!(rng,copy_rng);assert_eq!(game.final_morality(9876).unwrap(),9999);
    }

    #[test]
    fn loss_waits_through_original_drop_shake_and_final_pause() {
        let mut game=FistGame::new(1000);game.start().unwrap();game.remaining_seconds=1;
        let mut rng=ReplayRng::new(1);game.tick(1000,&mut rng).unwrap();assert_eq!(game.phase,FistPhase::ResultPause);
        game.tick(1000,&mut rng).unwrap();assert_eq!(game.phase,FistPhase::Lost);
        for _ in 0..16 {game.tick(10,&mut rng).unwrap();}
        assert_eq!(game.phase,FistPhase::LostShake);assert_eq!(game.loss_y,201);
        for _ in loss_shake_frames() {game.tick(16,&mut rng).unwrap();}
        assert_eq!(game.phase,FistPhase::LostWait);game.tick(1000,&mut rng).unwrap();
        assert_eq!(game.phase,FistPhase::Finished);assert_eq!(game.final_morality(1000).unwrap(),1000);
    }

    #[test]
    fn reward_percentages_and_clamps_match_original_x86() {
        let data:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-fist.json")).unwrap();
        for row in data["rewards"].as_array().unwrap() {
            let before=row["before"].as_i64().unwrap() as i16;
            let mut game=FistGame::new(before);game.phase=FistPhase::Finished;game.score=row["score"].as_i64().unwrap() as i32;
            assert_eq!(i64::from(game.reward_delta(before)),row["delta"].as_i64().unwrap());
            assert_eq!(i64::from(game.final_morality(before).unwrap()),row["after"].as_i64().unwrap());
        }
    }
}
