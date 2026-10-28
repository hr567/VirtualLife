//! Native statistic reward animation (0x00453ab0 / 0x00453b00). Its displayed
//! spinning digits consume the shared CRT RNG, so presentation is stateful.
use crate::rng::ReplayRng;
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct BonusAnimation {
    pub value:i32,
    pub remaining_delta:i32,
    pub value_last_tick:u32,
    pub value_phase:u8,
    pub increment:i32,
    pub increment_last_tick:u32,
    pub increment_phase:u8,
    pub step_interval:u32,
    pub digit_count:u8,
    pub value_units:Option<u8>,
    pub increment_units:Option<u8>,
    pub visible:bool,
    pub finished:bool,
}

impl BonusAnimation {
    pub fn new(value:i32,delta:i32)->Self {
        let mut maximum=value.max(delta).max(value.wrapping_add(delta));
        let mut digit_count=0;
        loop {maximum/=10;digit_count+=1;if maximum==0 || digit_count==10 {break;}}
        Self {value,remaining_delta:delta,value_last_tick:0,value_phase:0,increment:delta,
            increment_last_tick:0,increment_phase:0,step_interval:10,digit_count,
            value_units:None,increment_units:None,visible:false,finished:false}
    }

    /// One original update-and-render call. `skip` models Escape/right-click;
    /// it takes effect only during the main number's counting phase.
    pub fn tick(&mut self,now:u32,rng:&mut ReplayRng,skip:bool)->bool {
        if self.finished {return false;}
        match self.value_phase {
            0=>{self.value_last_tick=now;self.value_phase=1;return true;}
            1=>{if now.wrapping_sub(self.value_last_tick)>1600 {self.value_last_tick=now;self.value_phase=2;}}
            2=>{
                if self.remaining_delta!=0 && !skip {
                    if now.wrapping_sub(self.value_last_tick)>self.step_interval {
                        self.value_last_tick=now;
                        let step=self.remaining_delta.clamp(-10,10);
                        self.value=self.value.wrapping_add(step);
                        self.remaining_delta=self.remaining_delta.wrapping_sub(step);
                        if step<0 {self.value=self.value.max(0);}
                    }
                } else {
                    if self.remaining_delta!=0 {
                        self.increment=0;
                        let delta=self.remaining_delta;
                        if delta>0 {self.value=self.value.wrapping_add(delta);}
                        else if self.value>0 {self.value=self.value.wrapping_add(delta).max(0);}
                        // Original post-decrement/increment loops leave -1/+1.
                        self.remaining_delta=if delta>0 {-1}else{1};
                    }
                    self.value_last_tick=now;self.value_phase=3;
                }
            }
            3=>{if now.wrapping_sub(self.value_last_tick)>1500 {self.finished=true;return false;}}
            _=>unreachable!("Invalid native bonus phase"),
        }
        match self.increment_phase {
            0=>{self.increment_last_tick=now;self.increment_phase=1;return true;}
            1=>{if now.wrapping_sub(self.increment_last_tick)>1400 {self.increment_last_tick=now;self.increment_phase=2;}}
            2=>{
                if self.increment==0 {self.increment_last_tick=now;self.increment_phase=3;}
                else if now.wrapping_sub(self.increment_last_tick)>self.step_interval {
                    self.increment_last_tick=now;self.increment-=self.increment.clamp(-10,10);
                }
            }
            3=>{}
            _=>unreachable!("Invalid native bonus phase"),
        }
        self.visible=true;
        self.value_units=if self.value_phase==2 {Some((rng.next_rand()%10) as u8)}else{None};
        self.increment_units=if self.increment_phase==2 {Some((rng.next_rand()%10) as u8)}else{None};
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reward_animation_matches_1529_original_x86_frames_including_skip_rng() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-bonus.json")).unwrap();
        for case in vectors["cases"].as_array().unwrap() {
            let mut animation=BonusAnimation::new(case["before"].as_i64().unwrap() as i32,case["delta"].as_i64().unwrap() as i32);
            let mut rng=ReplayRng::new(42);
            for frame in case["frames"].as_array().unwrap() {
                let now=frame["time"].as_u64().unwrap() as u32;
                let skip=case["skipAt"].as_u64().is_some_and(|time|u64::from(now)>=time);
                let active=animation.tick(now,&mut rng,skip);
                assert_eq!(active,frame["active"].as_bool().unwrap());
                let fields=[animation.value,animation.remaining_delta,animation.value_last_tick as i32,i32::from(animation.value_phase),animation.increment,animation.increment_last_tick as i32,i32::from(animation.increment_phase),animation.step_interval as i32,i32::from(animation.digit_count)];
                assert_eq!(serde_json::to_value(fields).unwrap(),frame["fields"],"delta {} at {now}",case["delta"]);
                assert_eq!(u64::from(rng.state),frame["rngState"].as_u64().unwrap());
                assert_eq!(rng.draws,frame["rngDraws"].as_u64().unwrap());
                if active && animation.visible {
                    let units:Vec<u8>=animation.value_units.into_iter().chain(animation.increment_units).collect();
                    assert_eq!(serde_json::to_value(units).unwrap(),frame["units"]);
                }
            }
        }
    }
}
