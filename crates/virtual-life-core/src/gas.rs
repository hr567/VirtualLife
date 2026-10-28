//! Native timing game (0x004265a0). Its movement and countdown are driven by
//! rendered frames, while the original FLC objects advance on wall-clock time.
use crate::{bonus::BonusAnimation,rng::ReplayRng};
use serde::{Deserialize,Serialize};

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum GasPhase{Ready,Playing,Help,Hit,Miss,Won,WonWait,Lost,LostFall,LostShake,Reward,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct GasAnimation{
    pub resource:String,pub x:i32,pub y:i32,pub frame:i32,pub frame_count:i32,pub interval:u32,pub last_tick:u32,pub initial_frame:bool,
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn gas_game_frames_match_original_x86(){
        let data:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-gas.json")).unwrap();
        for sequence in data["sequences"].as_array().unwrap(){
            let mut game=GasGame::new(1000);game.start().unwrap();let mut rng=ReplayRng::new(sequence["seed"].as_u64().unwrap());
            for(index,row)in sequence["frames"].as_array().unwrap().iter().enumerate(){
                if row["fire"]==true{game.throw().unwrap();}game.tick(row["elapsedMs"].as_u64().unwrap()as u32,&mut rng).unwrap();
                let value=serde_json::to_value(&game).unwrap();let expected=&row["after"];
                for key in ["attempts","successes","results","targetState","targetSpeed","playerState","countdown","statePending"]{assert_eq!(value[key],expected[key],"{} frame {index} {key}",sequence["name"]);}
                for key in ["targets","animations"]{for(actual,expected)in value[key].as_array().unwrap().iter().zip(expected[key].as_array().unwrap()){
                    for(field,expected)in expected.as_object().unwrap(){assert_eq!(&actual[field],expected,"{} frame {index} {} {field}",sequence["name"],actual["resource"]);}
                }}
                assert_eq!(rng.draws,expected["draws"].as_u64().unwrap(),"{} frame {index} rng draws",sequence["name"]);assert_eq!(u64::from(rng.state),expected["rngState"].as_u64().unwrap());
            }
        }
        for case in data["rewards"].as_array().unwrap(){
            let mut game=GasGame::new(case["before"].as_i64().unwrap()as i16);game.successes=case["successes"].as_u64().unwrap()as usize;game.phase=GasPhase::Finished;
            assert_eq!(i64::from(game.final_stamina(game.starting_stamina).unwrap()),case["after"].as_i64().unwrap());
        }
    }

    #[test]
    fn gas_game_result_animations_and_reward_resume_without_extra_game_rng(){
        for successes in [0,1,2,3]{
            let mut game=GasGame::new(9900);game.successes=successes;game.results=std::array::from_fn(|index|index<successes);game.attempts=3;game.phase=GasPhase::Playing;game.ending_pending=true;
            let mut rng=ReplayRng::new(42);game.tick(16,&mut rng).unwrap();let mut copy:GasGame=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();let mut copy_rng=rng.clone();
            for _ in 0..2000{if game.phase==GasPhase::Finished{break;}game.tick(17,&mut rng).unwrap();copy.tick(17,&mut copy_rng).unwrap();assert_eq!(game,copy);assert_eq!(rng,copy_rng);}
            assert_eq!(game.phase,GasPhase::Finished);assert_eq!(game.final_stamina(9900).unwrap(),if successes==0{9900}else{9999});
            if successes==0{assert_eq!(rng.draws,0);}else{assert!(rng.draws>0);}
        }
    }
}
impl GasAnimation{
    fn new(resource:&str,x:i32,y:i32,count:i32,interval:u32)->Self{Self{resource:format!("install/gas/{resource}.flc"),x,y,frame:0,frame_count:count,interval,last_tick:0,initial_frame:true}}
    fn tick(&mut self,now:u32){if now.wrapping_sub(self.last_tick)>self.interval{self.last_tick=now;if self.initial_frame{self.initial_frame=false;}else{self.frame=(self.frame+1)%self.frame_count;}}}
    fn reset(&mut self){self.frame=0;}
    fn last(&self)->bool{self.frame==self.frame_count-1}
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct GasGame{
    pub phase:GasPhase,pub clock_ms:u32,pub phase_last_tick:u32,
    pub attempts:usize,pub successes:usize,pub results:[bool;3],
    pub target_state:usize,pub target_speed:i32,pub targets:[GasAnimation;4],
    pub player_state:usize,pub countdown:i32,pub state_pending:bool,
    /// idle, yawn, mouth, smoke, hit, miss, victory, defeat, falling, fish0..2.
    pub animations:Vec<GasAnimation>,pub fire:bool,pub ending_pending:bool,
    pub fall_y:f64,pub fall_velocity:f64,pub shake_frame:usize,
    pub starting_stamina:i16,pub reward:Option<BonusAnimation>,pub reward_skip:bool,
}
impl GasGame{
    pub fn new(stamina:i16)->Self{
        Self{phase:GasPhase::Ready,clock_ms:0,phase_last_tick:0,attempts:0,successes:0,results:[false;3],target_state:0,target_speed:3,
            targets:[GasAnimation::new("gan05",434,344,8,214),GasAnimation::new("gan01",434,344,4,171),GasAnimation::new("gan06",434,344,2,71),GasAnimation::new("gan04",434,344,2,71)],
            player_state:0,countdown:200,state_pending:false,animations:vec![
                GasAnimation::new("gau01",222,188,1,71),GasAnimation::new("gau06",180,188,28,142),
                GasAnimation::new("gau07",199,249,2,785),GasAnimation::new("gau09",224,195,46,71),
                GasAnimation::new("gau04",185,180,20,71),GasAnimation::new("gau05",173,166,16,71),
                GasAnimation::new("gas03",192,137,5,71),GasAnimation::new("gau10",180,164,23,71),
                GasAnimation::new("gau08",225,276,2,71),GasAnimation::new("fish",213,417,4,71),
                GasAnimation::new("fish",289,417,4,71),GasAnimation::new("fish",365,417,4,71)],
            fire:false,ending_pending:false,fall_y:0.0,fall_velocity:0.2,shake_frame:0,starting_stamina:stamina,reward:None,reward_skip:false}
    }
    pub fn start(&mut self)->Result<(),String>{if !matches!(self.phase,GasPhase::Ready|GasPhase::Help){return Err("Gas instructions are not open".into());}self.phase=GasPhase::Playing;Ok(())}
    pub fn show_help(&mut self)->Result<(),String>{if self.phase!=GasPhase::Playing{return Err("Gas game is not playing".into());}self.phase=GasPhase::Help;self.phase_last_tick=self.clock_ms;self.fire=false;Ok(())}
    pub fn throw(&mut self)->Result<(),String>{if self.phase!=GasPhase::Playing{return Err("Wait for the current gas game animation".into());}self.fire=true;Ok(())}
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng)->Result<(),String>{
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);let now=self.clock_ms;
        match self.phase{
            GasPhase::Ready|GasPhase::Help=>{self.tick_fish();if now.wrapping_sub(self.phase_last_tick)>=12000{self.phase=GasPhase::Playing;}},
            GasPhase::Playing=>{
                if self.ending_pending{self.begin_ending();self.tick_ending();}
                else{self.play_frame(rng);}
            }
            GasPhase::Hit|GasPhase::Miss=>{
                let index=if self.phase==GasPhase::Hit{4}else{5};
                if self.animations[index].last(){self.finish_attempt(rng);self.main_tail();}else{self.attempt_frame();}
            }
            GasPhase::Won|GasPhase::WonWait|GasPhase::Lost|GasPhase::LostFall|GasPhase::LostShake=>self.tick_ending(),
            GasPhase::Reward=>{
                if !self.reward.as_mut().ok_or("Gas reward is missing")?.tick(now,rng,self.reward_skip){self.phase=GasPhase::Finished;}
                self.reward_skip=false;
            },
            GasPhase::Finished=>return Err("Gas game has finished".into()),
        }
        self.fire=false;Ok(())
    }
    fn switch_target(&mut self,index:usize,speed:i32){
        let old=self.target_state;self.targets[index].x=self.targets[old].x;self.targets[index].y=self.targets[old].y;self.target_state=index;self.target_speed=speed;
    }
    fn play_frame(&mut self,rng:&mut ReplayRng){
        self.countdown-=1;if self.countdown<0{self.state_pending=true;}
        let roll=rng.next_rand()%10;
        if self.target_state==2 && self.targets[2].last(){self.switch_target(3,10);}
        else if roll<2 && self.target_state!=0 && self.target_state!=3 && self.targets[self.target_state].last(){self.switch_target(0,(rng.next_rand()%2)as i32+1);}
        else if roll<9{
            if self.target_state!=1 && self.target_state!=3 && self.targets[self.target_state].last(){self.switch_target(1,(rng.next_rand()%3)as i32+2);}
        }else if self.target_state!=2 && self.target_state!=3 && self.targets[self.target_state].x>200 && self.targets[self.target_state].last(){self.switch_target(2,3);self.targets[2].interval=71*4;}
        if self.animations[1].last(){self.state_pending=true;self.animations[1].reset();}
        if self.state_pending{
            match self.player_state{
                0=>{self.countdown=9999;self.player_state=1;},
                1=>{self.countdown=rng.next_rand()as i32*100%9+800;self.player_state=2;},
                _=>{self.countdown=rng.next_rand()as i32*100%6+300;self.player_state=0;},
            }self.state_pending=false;
        }
        match self.player_state{0=>self.animations[0].tick(self.clock_ms),1=>self.animations[1].tick(self.clock_ms),_=>{self.animations[2].tick(self.clock_ms);self.animations[3].tick(self.clock_ms);}}
        if self.fire{
            if self.player_state==0 || self.player_state==2{
                let hit=(256..285).contains(&self.targets[self.target_state].x);
                self.phase=if hit{GasPhase::Hit}else{GasPhase::Miss};self.animations[if hit{4}else{5}].reset();self.attempt_frame();return;
            }
            self.reset_player(rng);
        }
        self.main_tail();
    }
    fn move_target(&mut self){
        self.targets[self.target_state].x-=self.target_speed;
        if self.targets[self.target_state].x<22{self.target_state=0;self.target_speed=2;self.targets[0].x=534;}
        self.targets[self.target_state].tick(self.clock_ms);
    }
    fn attempt_frame(&mut self){
        let hit=self.phase==GasPhase::Hit;let index=if hit{4}else{5};self.animations[index].tick(self.clock_ms);
        if !hit || self.animations[index].frame<6{self.move_target();}
        else{self.target_state=0;self.target_speed=2;self.targets[0].x=534;}
        self.tick_fish();
    }
    fn reset_player(&mut self,rng:&mut ReplayRng){self.player_state=0;for animation in &mut self.animations[..4]{animation.reset();}self.countdown=rng.next_rand()as i32*100%6+300;}
    fn finish_attempt(&mut self,rng:&mut ReplayRng){
        if self.phase==GasPhase::Hit{self.results[self.attempts]=true;self.successes+=1;}
        self.attempts+=1;self.reset_player(rng);self.phase=GasPhase::Playing;
    }
    fn tick_fish(&mut self){for index in 0..3{if self.results[index]{self.animations[9+index].tick(self.clock_ms);}}}
    fn main_tail(&mut self){self.move_target();self.tick_fish();if self.attempts>=3{self.ending_pending=true;}}
    fn begin_ending(&mut self){self.phase=if self.successes>=1{GasPhase::Won}else{GasPhase::Lost};self.phase_last_tick=self.clock_ms;}
    fn tick_ending(&mut self){
        let elapsed=self.clock_ms.wrapping_sub(self.phase_last_tick);
        match self.phase{
            GasPhase::Won=>{
                if elapsed>=4000{self.phase=GasPhase::WonWait;self.phase_last_tick=self.clock_ms;}
                else{self.animations[0].tick(self.clock_ms);if elapsed<3000{self.animations[6].tick(self.clock_ms);}self.tick_fish();}
            }
            GasPhase::WonWait=>if elapsed>=1000{self.begin_reward();},
            GasPhase::Lost=>{
                if self.animations[7].last(){self.phase=GasPhase::LostFall;self.fall_frame();}
                else{self.move_target();self.animations[7].tick(self.clock_ms);self.tick_fish();}
            }
            GasPhase::LostFall=>{
                if self.fall_y>=300.0{self.phase=GasPhase::LostShake;self.phase_last_tick=self.clock_ms;}
                else{self.fall_frame();}
            }
            GasPhase::LostShake=>if elapsed>=100{self.shake_frame+=1;self.phase_last_tick=self.clock_ms;if self.shake_frame>=16{self.begin_reward();}},
            _=>{}
        }
    }
    fn fall_frame(&mut self){self.fall_velocity+=0.2;self.fall_y+=self.fall_velocity;self.move_target();self.animations[8].tick(self.clock_ms);self.tick_fish();}
    fn reward_delta(&self,current:i16)->i16{
        if self.successes==0{return 0;}
        let percent=match self.successes{1=>3,2=>5,3=>10,_=>0};let result=current.wrapping_add((i32::from(current)*percent/100)as i16).min(9999);result.wrapping_sub(current)
    }
    fn begin_reward(&mut self){if self.successes==0{self.phase=GasPhase::Finished;}else{self.reward=Some(BonusAnimation::new(i32::from(self.starting_stamina),i32::from(self.reward_delta(self.starting_stamina))));self.phase=GasPhase::Reward;}}
    pub fn final_stamina(&self,current:i16)->Result<i16,String>{if self.phase!=GasPhase::Finished{return Err("Gas result is not finished".into());}Ok(current.wrapping_add(self.reward_delta(current)))}
    pub fn finish_reward_animation(&mut self)->Result<(),String>{if self.phase!=GasPhase::Reward{return Err("Gas reward is not showing".into());}self.reward_skip=true;Ok(())}
}
