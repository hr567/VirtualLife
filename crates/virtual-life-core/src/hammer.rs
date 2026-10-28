//! Nine-target hammer game, native entry 0x00456af0. Each tick represents one
//! original render-loop iteration; spawn attempts are per frame, not per second.
use crate::rng::ReplayRng;
use crate::bonus::BonusAnimation;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="camelCase")]
pub enum HammerPhase { Ready, Playing, Won, Lost, Reward, Finished }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="camelCase")]
pub struct HammerTarget {
    pub x: i32,
    pub y: i32,
    /// 0 ordinary (+1), 1 armored (three hits, +5), 2 friendly (-3).
    pub kind: i32,
    /// Original states: 0 hidden, 1 rising, 2 exposed, 3 sinking, 4 struck.
    pub phase: i32,
    pub hits: i32,
    pub frame: i32,
    pub last_tick: u32,
    /// Source row in the five-frame, vertically stacked mouse bitmap.
    pub sprite_y: i32,
    pub impact_until: Option<u32>,
    pub impact_position: [i32;2],
    pub score_frame: Option<u8>,
    pub score_last_tick: u32,
}

impl HammerTarget {
    fn new(x: i32, y: i32) -> Self {
        Self { x,y,kind:0,phase:0,hits:0,frame:0,last_tick:0,sprite_y:0,impact_until:None,
            impact_position:[0;2],score_frame:None,score_last_tick:0 }
    }

    /// FUN_00454910 / FUN_00454c30, including strict timer boundaries.
    pub fn tick(&mut self, now: u32, rng: &mut ReplayRng) {
        let elapsed = now.wrapping_sub(self.last_tick);
        match self.phase {
            0 => {
                if elapsed > 999 && rng.next_rand() % 100 == 0 {
                    if let Some(kind)=match rng.next_rand() % 10 { 1..=3 => Some(0), 4..=5 => Some(1), 6 => Some(2), _ => None } {
                        self.kind=kind;self.phase=1; self.frame=0; self.hits=0; self.sprite_y=0; self.last_tick=now;
                    }
                }
            }
            1 if elapsed > if self.frame == 0 {100} else {50} => {
                self.frame+=1;
                if self.frame==2 {self.phase=2;self.frame=0;self.sprite_y=170;} else {self.sprite_y=85;}
                self.last_tick=now;
            }
            2 if elapsed > 1000 => {self.phase=3;self.frame=0;self.sprite_y=255;self.last_tick=now;}
            3 if elapsed > 100 => {self.phase=0;self.frame+=1;self.last_tick=now;}
            4 if elapsed > if self.frame == 0 {50} else {1000} => {
                self.frame+=1;
                if self.frame==2 {self.phase=3;self.frame=0;self.sprite_y=255;} else {self.sprite_y=340;}
                self.last_tick=now;
            }
            _ => {}
        }
        if self.impact_until.is_some_and(|until| now.wrapping_sub(until) < 0x80000000 && now != until) {
            self.impact_until=None;
        }
        if let Some(frame)=self.score_frame {
            if now.wrapping_sub(self.score_last_tick)>50 {
                self.score_frame=if frame+1<12 {Some(frame+1)} else {None};
                self.score_last_tick=now;
            }
        }
    }

    /// FUN_004547a0. The hammer's contact rectangle may intersect more than one
    /// target, so the game applies every returned score before clamping at zero.
    fn hit(&mut self, rectangle: [i32;4], now: u32) -> i32 {
        if self.phase != 2 {return 0;}
        let target=[self.x+33,self.y+21,self.x+86,self.y+59];
        if target[0].max(rectangle[0]) >= target[2].min(rectangle[2]) ||
            target[1].max(rectangle[1]) >= target[3].min(rectangle[3]) {return 0;}
        self.hits+=1;
        let score=match (self.kind,self.hits) {(0,1)=>1,(1,3)=>5,(2,1)=>-3,_=>0};
        if score != 0 {
            self.phase=4;self.frame=0;self.sprite_y=255;self.last_tick=now;
            self.score_frame=Some(0);self.score_last_tick=now;
        }
        self.impact_until=Some(now.wrapping_add(150));
        self.impact_position=[rectangle[0]-10,rectangle[1]];
        score
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="camelCase")]
pub struct HammerGame {
    pub phase: HammerPhase,
    pub targets: [HammerTarget;9],
    pub score: i32,
    pub clock_ms: u32,
    pub started_at: u32,
    pub result_at: u32,
    pub remaining_seconds: u32,
    pub loss_frame: u8,
    pub loss_last_tick: u32,
    pub pointer: [i32;2],
    pub starting_morality: i16,
    pub reward: Option<BonusAnimation>,
    pub reward_skip: bool,
}

impl Default for HammerGame {fn default()->Self {Self::new()}}

impl HammerGame {
    pub const DURATION_MS:u32=45_500;
    pub const PASS_SCORE:i32=80;
    pub const POSITIONS:[[i32;2];9]=[[129,273],[266,275],[403,275],[104,203],[242,203],[379,203],[137,133],[266,133],[394,134]];
    pub const LOSS_Y:[i32;34]=[-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-100,-50,0,50,120,200,200,120,80,120,200,200,120,200,200];

    pub fn new()->Self {
        Self {phase:HammerPhase::Ready,targets:Self::POSITIONS.map(|[x,y]|HammerTarget::new(x,y)),
            score:0,clock_ms:0,started_at:0,result_at:0,remaining_seconds:45,loss_frame:0,loss_last_tick:0,pointer:[102,59],
            starting_morality:0,reward:None,reward_skip:false}
    }

    pub fn with_morality(current:i16)->Self {let mut game=Self::new();game.starting_morality=current;game}

    /// Ready-screen time matters: native target timers start during loading,
    /// before the user starts the 45.5-second gameplay timer.
    pub fn start(&mut self)->Result<(),String> {
        if self.phase!=HammerPhase::Ready {return Err("Hammer game already started".into());}
        self.started_at=self.clock_ms;self.phase=HammerPhase::Playing;Ok(())
    }

    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng)->Result<(),String> {
        if self.phase==HammerPhase::Finished {return Err("Hammer game has finished".into());}
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        if self.phase==HammerPhase::Ready {return Ok(());}
        if self.phase==HammerPhase::Playing {
            let elapsed=self.clock_ms.wrapping_sub(self.started_at);
            let remaining=Self::DURATION_MS.wrapping_sub(elapsed)/1000%100;
            self.remaining_seconds=self.remaining_seconds.min(remaining);
            if elapsed>Self::DURATION_MS {
                self.phase=if self.score>=Self::PASS_SCORE {HammerPhase::Won}else{HammerPhase::Lost};
                self.result_at=self.clock_ms;self.loss_last_tick=self.clock_ms;
            }
        } else {self.remaining_seconds=0;}
        for target in &mut self.targets {target.tick(self.clock_ms,rng);}
        if self.phase==HammerPhase::Won && self.clock_ms.wrapping_sub(self.result_at)>3000 {
            self.phase=HammerPhase::Reward;
            let final_value=self.final_morality(self.starting_morality)?;
            self.reward=Some(BonusAnimation::new(i32::from(self.starting_morality),i32::from(final_value)-i32::from(self.starting_morality)));
        }
        if self.phase==HammerPhase::Lost && self.clock_ms.wrapping_sub(self.loss_last_tick)>30 {
            self.loss_last_tick=self.clock_ms;self.loss_frame+=1;
            if self.loss_frame>32 {self.phase=HammerPhase::Finished;}
        }
        if self.phase==HammerPhase::Reward {
            let active=self.reward.as_mut().ok_or("Hammer reward animation is missing")?.tick(self.clock_ms,rng,self.reward_skip);
            self.reward_skip=false;
            if !active {self.phase=HammerPhase::Finished;}
        }
        Ok(())
    }

    /// A complete press/release input. Pointer coordinates are the top-left of
    /// the original 68x68 hammer and are clamped to the 433x348 playfield.
    pub fn strike(&mut self,x:i32,y:i32)->Result<i32,String> {
        if self.phase!=HammerPhase::Playing {return Err("Hammer game is not accepting strikes".into());}
        self.pointer=[x.clamp(102,467),y.clamp(59,339)];
        let [x,y]=self.pointer;
        let rectangle=[x,y+50,x+35,y+67];
        let mut delta=0;
        for target in &mut self.targets {delta+=target.hit(rectangle,self.clock_ms);}
        self.score=(self.score+delta).max(0);
        Ok(delta)
    }

    /// Original numeric-key targeting at target origin +(38,-28).
    pub fn strike_index(&mut self,index:usize)->Result<i32,String> {
        let [x,y]=*Self::POSITIONS.get(index).ok_or("Hammer target index is out of range")?;
        self.strike(x+38,y-28)
    }

    pub fn finish_reward_animation(&mut self)->Result<(),String> {
        if self.phase!=HammerPhase::Reward {return Err("Hammer reward animation has not started".into());}
        self.reward_skip=true;Ok(())
    }

    pub fn final_morality(&self,current:i16)->Result<i16,String> {
        if !matches!(self.phase,HammerPhase::Reward|HammerPhase::Finished) {return Err("Hammer game result is not ready".into());}
        let percent=if self.score>=150 {10}else if self.score>=120 {5}else if self.score>=80 {3}else{0};
        Ok((i32::from(current)+i32::from(current)*percent/100).min(9999) as i16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hammer_target_frames_and_rng_match_original_x86() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-hammer.json")).unwrap();
        for case in vectors["sequences"].as_array().unwrap() {
            let mut target=HammerTarget::new(0,0);
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            for tick in case["ticks"].as_array().unwrap() {
                target.tick(tick["time"].as_u64().unwrap() as u32,&mut rng);
                let observed=serde_json::to_value(&target).unwrap();
                for field in ["kind","phase","hits","frame","lastTick","spriteY"] {assert_eq!(observed[field],tick[field],"seed {} time {} field {field}",case["seed"],tick["time"]);}
                assert_eq!(u64::from(rng.state),tick["rngState"].as_u64().unwrap());
            }
        }
    }

    #[test]
    fn hammer_hits_match_original_x86_for_all_target_types() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-hammer.json")).unwrap();
        for case in vectors["hits"].as_array().unwrap() {
            let mut target=HammerTarget::new(100,100);
            target.kind=case["kind"].as_i64().unwrap() as i32;target.phase=case["phase"].as_i64().unwrap() as i32;target.hits=case["hits"].as_i64().unwrap() as i32;
            let x=case["x"].as_i64().unwrap() as i32;let y=case["y"].as_i64().unwrap() as i32;
            assert_eq!(target.hit([x,y,x+35,y+17],100),case["score"].as_i64().unwrap() as i32);
            let observed=serde_json::to_value(target).unwrap();
            for field in ["kind","phase","hits","frame","lastTick","spriteY"] {assert_eq!(observed[field],case["after"][field],"case {case} field {field}");}
        }
    }

    #[test]
    fn hammer_deadline_save_resume_failure_and_reward_thresholds() {
        let mut game=HammerGame::new();let mut rng=ReplayRng::new(42);
        game.tick(1200,&mut rng).unwrap();assert_eq!(rng.draws,0);game.start().unwrap();
        game.tick(45_500,&mut rng).unwrap();assert_eq!(game.phase,HammerPhase::Playing);
        let mut restored:HammerGame=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();let mut restored_rng=rng.clone();
        for _ in 0..35 {game.tick(31,&mut rng).unwrap_or(());restored.tick(31,&mut restored_rng).unwrap_or(());}
        assert_eq!(game,restored);assert_eq!(rng,restored_rng);assert_eq!(game.phase,HammerPhase::Finished);assert_eq!(game.final_morality(1000).unwrap(),1000);
        for (score,expected) in [(79,1000),(80,1030),(119,1030),(120,1050),(149,1050),(150,1100)] {
            game.score=score;assert_eq!(game.final_morality(1000).unwrap(),expected);
        }
    }

    #[test]
    fn hammer_rewards_match_original_x86() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-hammer.json")).unwrap();
        for case in vectors["rewards"].as_array().unwrap() {
            let mut game=HammerGame::new();game.phase=HammerPhase::Reward;game.score=case["score"].as_i64().unwrap() as i32;
            assert_eq!(game.final_morality(case["before"].as_i64().unwrap() as i16).unwrap(),case["after"].as_i64().unwrap() as i16,"{case}");
        }
    }
}
