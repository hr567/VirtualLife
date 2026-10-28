//! Original student running/flying contest, native 0x004495a0.
//! Scores count rendered frames after the launch animation, not elapsed time.
use crate::{competition::{CompetitionAnimation,CompetitionRules},rng::ReplayRng};
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct RunningRules {
    pub score_pause_text:String,
    pub actors:Vec<Vec<Vec<CompetitionAnimation>>>,pub track:CompetitionAnimation,
    pub launch:CompetitionAnimation,pub flight:CompetitionAnimation,pub hand:CompetitionAnimation,
    pub host:CompetitionAnimation,pub podium:CompetitionAnimation,pub podium_places:Vec<CompetitionAnimation>,
}
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum RunningPhase {Preparing,Running,Finishing,ResultPause,Podium,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct RunningContest {
    pub phase:RunningPhase,pub contestant:usize,pub rank:usize,pub place:usize,pub prize:i16,
    pub times:[i16;4],pub counter:i32,pub interval:i32,pub opponent_intervals:[i16;3],
    pub hand:u8,pub phase_since:u32,pub podium_place:usize,pub winners:[usize;3],
    pub actors:Vec<Vec<CompetitionAnimation>>,pub track:CompetitionAnimation,
    pub launch:CompetitionAnimation,pub flight:CompetitionAnimation,pub hand_animation:CompetitionAnimation,
    pub host:CompetitionAnimation,pub podium:CompetitionAnimation,pub podium_places:Vec<CompetitionAnimation>,
    pub layers:Vec<CompetitionAnimation>,pub sounds:Vec<String>,
    pub displayed_contestant:usize,pub displayed_times:[i16;4],pub show_score_panels:bool,pub cleared_frame:bool,
    pub displayed_text:usize,pub text_index:usize,
    prizes:[i16;4],left:bool,right:bool,
}

impl RunningContest {
    pub fn new(stage:i16,sex:usize,stamina:i16,rules:&CompetitionRules,rng:&mut ReplayRng,now:u32)->Self {
        let opponent_intervals=[10+(rng.next_rand()%5)as i16,30+(rng.next_rand()%5)as i16,35+(rng.next_rand()%5)as i16];
        let row=if(2..=4).contains(&stage){(stage-2)as usize}else{0};
        let level=rules.stamina_thresholds[row].iter().filter(|&&threshold|stamina>=threshold).count();
        let running=&rules.running;
        let mut launch=running.launch.clone();launch.tick(now);
        let mut track=running.track.clone();track.tick(now);
        Self {phase:RunningPhase::Preparing,contestant:0,rank:0,place:0,prize:0,times:[0;4],counter:0,
            interval:[80,60,40,20][level],opponent_intervals,hand:10,phase_since:now,podium_place:1,winners:[0,1,2],
            actors:running.actors[sex].clone(),track,launch,flight:running.flight.clone(),hand_animation:running.hand.clone(),
            host:running.host.clone(),podium:running.podium.clone(),podium_places:running.podium_places.clone(),
            layers:Vec::new(),sounds:Vec::new(),displayed_contestant:0,displayed_times:[0;4],show_score_panels:true,
            cleared_frame:false,displayed_text:0,text_index:0,prizes:rules.prizes,left:false,right:false}
    }
    /// Both flags are key-down edges; held keys must not repeat automatically.
    pub fn input(&mut self,left:bool,right:bool){self.left|=left;self.right|=right;}
    pub fn tick(&mut self,now:u32){
        if self.phase==RunningPhase::Finished{return;}
        let left=std::mem::take(&mut self.left);let right=std::mem::take(&mut self.right);
        self.layers.clear();self.sounds.clear();self.cleared_frame=false;
        self.displayed_contestant=self.contestant;self.displayed_times=self.times;self.displayed_text=self.text_index;
        self.show_score_panels=self.phase!=RunningPhase::Podium;
        self.host.tick(now);self.layers.push(self.host.clone());
        if self.show_score_panels{self.layers.push(self.track.clone());}
        match self.phase {
            RunningPhase::Preparing=>{
                let actor=&mut self.actors[self.contestant][0];actor.tick(now);self.layers.push(actor.clone());
                if actor.frame==actor.frame_count/2{self.sounds.push("launch".into());}
                if actor.last(){self.counter=0;self.phase=RunningPhase::Running;self.run_frame(now,left,right);}
            },
            RunningPhase::Running=>self.run_frame(now,left,right),
            RunningPhase::Finishing=>{
                let actor=&mut self.actors[self.contestant][2];actor.tick(now);actor.interval=self.interval as i16 as u32;self.layers.push(actor.clone());
                if actor.last(){
                    self.times[self.contestant]=self.counter as i16;self.contestant+=1;
                    if self.contestant==4{
                        self.sounds.push("result".into());self.phase_since=now;self.phase=RunningPhase::ResultPause;
                        self.rank=if self.times[0]<=self.times[1]{0}else if self.times[0]<=self.times[2]{1}else if self.times[0]<=self.times[3]{2}else{3};
                        self.place=3-self.rank;self.prize=self.prizes[self.place];
                        let mut order=vec![1,2,3];order.insert(self.rank,0);self.winners.copy_from_slice(&order[..3]);self.text_index=5;
                    }else{
                        self.interval=i32::from(self.opponent_intervals[self.contestant-1]);self.track.reset();self.flight.reset();
                        self.phase=RunningPhase::Preparing;self.text_index=self.contestant;
                    }
                }
            },
            RunningPhase::ResultPause=>{
                if now>=self.phase_since.wrapping_add(2000){
                    self.phase=RunningPhase::Podium;self.text_index=4;self.cleared_frame=true;self.layers.clear();self.podium_frame(now);
                }
            },
            RunningPhase::Podium=>self.podium_frame(now),
            RunningPhase::Finished=>{},
        }
    }
    fn run_frame(&mut self,now:u32,left:bool,right:bool){
        if self.track.last(){self.phase=RunningPhase::Finishing;}
        if (self.interval as i16)<1000 {
            self.track.interval=(i32::from(self.interval as i16)*2)as u32;self.track.tick(now);
            let actor=&mut self.actors[self.contestant][1];actor.interval=(i32::from(self.interval as i16)/2)as u32;actor.tick(now);
        }
        self.layers.push(self.actors[self.contestant][1].clone());
        if self.contestant==0 {
            if left && matches!(self.hand,10|1){self.hand_animation.frame=0;self.hand=0;self.interval=(self.interval-100).max(0);}
            if right && matches!(self.hand,10|0){self.hand_animation.frame=1;self.hand=1;self.interval=(self.interval-100).max(0);}
            self.layers.push(self.hand_animation.clone());
        }
        if self.launch.last(){self.counter=(self.counter+1).min(999);self.flight.tick(now);self.layers.push(self.flight.clone());}
        else{self.launch.tick(now);self.layers.push(self.launch.clone());}
        if self.phase==RunningPhase::Running && self.contestant==0 && (self.interval as i16)<999{self.interval+=1;}
    }
    fn podium_frame(&mut self,now:u32){
        if !self.podium.last(){self.podium.tick(now);}self.layers.push(self.podium.clone());
        for index in 0..self.podium_place-1{self.layers.push(self.podium_places[2-index].clone());}
        if self.podium_place<4{
            let place=&mut self.podium_places[3-self.podium_place];place.tick(now);self.layers.push(place.clone());
            if place.last(){self.podium_place+=1;}
        }
        if now>=self.phase_since.wrapping_add(10000){self.phase=RunningPhase::Finished;}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules()->CompetitionRules{let value:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();serde_json::from_value(value["competition"].clone()).unwrap()}
    fn track(animation:&CompetitionAnimation)->serde_json::Value {serde_json::json!([animation.frame,animation.last_tick,u8::from(animation.initial_frame),animation.interval])}
    #[test]
    fn complete_running_contests_match_original_x86(){
        let data:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-running.json")).unwrap();let rules=rules();
        for (case_index,case)in data["cases"].as_array().unwrap().iter().enumerate(){
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            let mut game=RunningContest::new(case["stage"].as_i64().unwrap()as i16,case["sex"].as_u64().unwrap()as usize,case["stamina"].as_i64().unwrap()as i16,&rules,&mut rng,case["start"].as_u64().unwrap()as u32);
            for(frame_index,frame)in case["frames"].as_array().unwrap().iter().enumerate(){
                game.input(frame["left"].as_bool().unwrap(),frame["right"].as_bool().unwrap());game.tick(frame["time"].as_u64().unwrap()as u32);
                let mut tracks=serde_json::Map::new();let mut names=std::collections::HashMap::new();
                for(actor_index,actor)in game.actors.iter().enumerate(){for(phase,animation)in actor.iter().enumerate(){let name=format!("actor{actor_index}_{phase}");tracks.insert(name.clone(),track(animation));names.insert(animation.resource.as_str(),name);}}
                for(name,animation)in [("track",&game.track),("launch",&game.launch),("flight",&game.flight),("handAnimation",&game.hand_animation),("host",&game.host),("podium",&game.podium),("podium0",&game.podium_places[0]),("podium1",&game.podium_places[1]),("podium2",&game.podium_places[2])]{tracks.insert(name.into(),track(animation));names.insert(animation.resource.as_str(),name.into());}
                let phase=match game.phase{RunningPhase::Preparing=>0,RunningPhase::Running=>1,RunningPhase::Finishing=>2,RunningPhase::ResultPause=>3,_=>4};
                let value=serde_json::json!({"phase":phase,"contestant":game.contestant,"rank":game.rank,"hand":game.hand,"podiumPlace":game.podium_place,"interval":game.interval,
                    "counter":game.counter,"phaseSince":game.phase_since,"finished":u8::from(game.phase==RunningPhase::Finished),"times":game.times,"opponentIntervals":game.opponent_intervals,
                    "tracks":tracks,"layers":game.layers.iter().map(|animation|serde_json::json!([names[animation.resource.as_str()],animation.frame])).collect::<Vec<_>>(),"sounds":game.sounds,"rngState":rng.state,"rngDraws":rng.draws});
                for(field_index,field)in data["fields"].as_array().unwrap().iter().enumerate(){let key=field.as_str().unwrap();assert_eq!(value[key],frame["after"][field_index],"case{case_index} frame{frame_index} {key}");}
            }
        }
    }
    #[test]
    fn save_preserves_inputs_and_fourth_place_original_prize(){
        let mut rng=ReplayRng::new(42);let mut game=RunningContest::new(3,0,300,&rules(),&mut rng,0);game.input(true,false);
        let mut restored:RunningContest=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
        for now in (16..5000).step_by(16){game.tick(now);restored.tick(now);assert_eq!(game,restored);}
        game.contestant=3;game.phase=RunningPhase::Finishing;game.actors[3][2].frame=game.actors[3][2].frame_count-1;game.actors[3][2].last_tick=5000;
        game.times=[999,100,200,0];game.counter=300;game.tick(5000);assert_eq!(game.rank,3);assert_eq!(game.place,0);assert_eq!(game.prize,850);
        let mut queued=RunningContest::new(3,0,300,&rules(),&mut rng,0);queued.phase=RunningPhase::Running;queued.interval=250;
        queued.input(true,false);queued.input(false,true);queued.tick(16);
        assert_eq!((queued.hand,queued.interval),(1,51));queued.tick(32);assert_eq!((queued.hand,queued.interval),(1,52));
    }
}
