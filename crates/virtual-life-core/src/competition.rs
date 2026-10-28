//! Student competitions (0x004193e0, 0x0044c010): original scoring, clocks,
//! question draws, presentation order and prize settlement.
use crate::{original_rules::{ExamQuestion,SchoolExam},rng::ReplayRng};
use serde::{Deserialize,Serialize};
use crate::competition_run::{RunningContest,RunningPhase,RunningRules};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct CompetitionAnimation{
    pub resource:String,pub x:i32,pub y:i32,pub frame_count:i32,pub interval:u32,
    #[serde(default)]pub frame:i32,#[serde(default)]pub last_tick:u32,
    #[serde(default="initial_frame")]pub initial_frame:bool,
}
fn initial_frame()->bool{true}
impl CompetitionAnimation{
    pub fn tick(&mut self,now:u32){if now.wrapping_sub(self.last_tick)>self.interval{self.last_tick=now;if self.initial_frame{self.initial_frame=false;}else{self.frame=(self.frame+1)%self.frame_count;}}}
    pub fn last(&self)->bool{self.frame==self.frame_count-1}
    pub fn reset(&mut self){self.frame=0;}
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct CompetitionRules{
    pub charm_thresholds:[[i16;3];3],pub stamina_thresholds:[[i16;3];3],pub prizes:[i16;4],
    pub menu:Vec<Vec<String>>,pub beauty_intro:Vec<Vec<String>>,pub running_intro:Vec<Vec<String>>,
    pub beauty_opponents:Vec<Vec<String>>,pub running_opponents:Vec<Vec<String>>,
    pub quiz_text:Vec<String>,pub beauty_text:Vec<String>,pub running_text:Vec<String>,
    pub balloon:CompetitionAnimation,pub opening_animation:CompetitionAnimation,pub beauty_actors:Vec<Vec<Vec<CompetitionAnimation>>>,pub beauty_stage:CompetitionAnimation,
    pub judges:CompetitionAnimation,pub beauty_podium:CompetitionAnimation,pub podium_places:Vec<CompetitionAnimation>,
    pub rank_animations:Vec<CompetitionAnimation>,pub money_animation:CompetitionAnimation,pub quiz_feedback:Vec<CompetitionAnimation>,pub beauty_walk_step:f64,
    pub running:RunningRules,
}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum BeautyPhase{WalkIn,Pose,WalkOut,AfterWalk,Judges,Scores,Podium,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct BeautyContest{
    pub phase:BeautyPhase,pub scores:[[i16;3];4],pub totals:[i16;4],pub winners:[usize;3],pub place:usize,pub prize:i16,
    pub contestant:usize,pub walk_x:f64,pub walk_y:i32,pub phase_since:u32,pub podium_place:usize,
    pub actors:Vec<Vec<CompetitionAnimation>>,pub stage:CompetitionAnimation,pub judges:CompetitionAnimation,
    pub podium:CompetitionAnimation,pub podium_places:Vec<CompetitionAnimation>,pub walk_step:f64,
    pub layers:Vec<CompetitionAnimation>,
}
impl BeautyContest{
    pub fn new(stage:i16,sex:usize,charm:i16,rules:&CompetitionRules,rng:&mut ReplayRng)->Self{
        let row=if(2..=4).contains(&stage){(stage-2)as usize}else{0};
        let place=rules.charm_thresholds[row].iter().filter(|&&threshold|charm>threshold).count();
        let bases=[[60,90,80,70],[70,80,90,60],[80,70,90,60],[90,60,70,80]][place];
        let mut scores=[[0;3];4];for judge in 0..3{for actor in 0..4{scores[actor][judge]=bases[actor]+(rng.next_rand()%10)as i16;}}
        let totals=scores.map(|row|row.iter().sum());let winners=[[1,2,3],[2,1,0],[2,0,1],[0,3,2]][place];
        Self{phase:BeautyPhase::WalkIn,scores,totals,winners,place,prize:if place==0{0}else{rules.prizes[place]},contestant:0,walk_x:435.0,walk_y:0,phase_since:0,podium_place:0,
            actors:rules.beauty_actors[sex].clone(),stage:rules.beauty_stage.clone(),judges:rules.judges.clone(),podium:rules.beauty_podium.clone(),podium_places:rules.podium_places.clone(),walk_step:rules.beauty_walk_step,layers:vec![]}
    }
    pub fn tick(&mut self,now:u32){
        if self.phase!=BeautyPhase::AfterWalk{self.layers.clear();}
        match self.phase{
            BeautyPhase::WalkIn|BeautyPhase::Pose|BeautyPhase::WalkOut=>{
                self.stage.tick(now);let state=match self.phase{BeautyPhase::WalkIn=>0,BeautyPhase::Pose=>1,_=>2};
                let actor=&mut self.actors[self.contestant][state];actor.x=self.walk_x as i32;actor.y=self.walk_y;actor.tick(now);
                self.layers.extend([self.stage.clone(),actor.clone()]);
                match self.phase{
                    BeautyPhase::WalkIn=>{self.walk_x-=self.walk_step;if self.walk_y<110{self.walk_y+=1;}if self.walk_y==110{self.phase=BeautyPhase::Pose;let pose=&mut self.actors[self.contestant][1];pose.x=self.walk_x as i32;pose.y=110;if pose.last(){self.phase=BeautyPhase::WalkOut;let out=&mut self.actors[self.contestant][2];out.x=self.walk_x as i32;out.y=110;}}},
                    BeautyPhase::Pose=>{if actor.last(){self.phase=BeautyPhase::WalkOut;let out=&mut self.actors[self.contestant][2];out.x=self.walk_x as i32;out.y=self.walk_y;}},
                    _=>{self.walk_x+=self.walk_step;if self.walk_y>0{self.walk_y-=1;}if self.walk_y==0{self.phase=BeautyPhase::AfterWalk;}},
                }
            },
            // The nested 0x44a150 loop flips its final frame before returning;
            // the caller then flips once more without advancing any FLC.
            BeautyPhase::AfterWalk=>{self.phase=BeautyPhase::Judges;self.judges.reset();},
            BeautyPhase::Judges=>{self.judges.tick(now);self.layers.push(self.judges.clone());if self.judges.last(){self.phase=BeautyPhase::Scores;self.phase_since=now;}},
            BeautyPhase::Scores=>{if now.wrapping_sub(self.phase_since)>=2000{self.contestant+=1;if self.contestant==4{self.phase=BeautyPhase::Podium;self.phase_since=now;}else{self.phase=BeautyPhase::WalkIn;self.walk_x=435.0;self.walk_y=0;}}else{self.layers.push(self.judges.clone());}},
            BeautyPhase::Podium=>{
                if !self.podium.last(){self.podium.tick(now);}
                self.layers.push(self.podium.clone());for index in 0..self.podium_place{self.layers.push(self.podium_places[2-index].clone());}
                if self.podium_place<3{let entry=&mut self.podium_places[2-self.podium_place];entry.tick(now);self.layers.push(entry.clone());if entry.last(){self.podium_place+=1;}}
                if now.wrapping_sub(self.phase_since)>=10000{self.phase=BeautyPhase::Finished;}
            },
            BeautyPhase::Finished=>{},
        }
    }
}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum CompetitionPhase{Arrival,Opening,Menu,Intro,Beauty,Running,QuizIntro,QuizQuestion,QuizFeedback,QuizResult,RankPrize,MoneyPrize,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct StudentCompetition{
    pub phase:CompetitionPhase,pub clock_ms:u32,pub phase_since:u32,pub stage:i16,pub sex:usize,pub name:String,pub charm:i16,pub stamina:i16,
    pub selected:Option<usize>,pub intro_page:usize,pub place:usize,pub prize:i16,pub prize_paid:bool,
    pub beauty:Option<BeautyContest>,pub exam:Option<SchoolExam>,pub last_correct:bool,pub feedback:Vec<CompetitionAnimation>,
    #[serde(default,skip_serializing_if="Option::is_none")]pub answered_question:Option<ExamQuestion>,
    #[serde(default,skip_serializing_if="Option::is_none")]pub running:Option<RunningContest>,
    pub animation:Option<CompetitionAnimation>,pub rules:CompetitionRules,
}
impl StudentCompetition{
    pub fn new(stage:i16,sex:usize,name:String,charm:i16,stamina:i16,rules:CompetitionRules)->Self{
        Self{phase:CompetitionPhase::Arrival,clock_ms:0,phase_since:0,stage,sex,name,charm,stamina,selected:None,intro_page:0,place:0,prize:0,prize_paid:false,
            beauty:None,exam:None,last_correct:false,feedback:rules.quiz_feedback.clone(),answered_question:None,running:None,animation:Some(rules.balloon.clone()),rules}
    }
    pub fn select(&mut self,index:usize,questions:&[ExamQuestion],rng:&mut ReplayRng)->Result<(),String>{
        if self.phase!=CompetitionPhase::Menu||index>=3{return Err("请选择学生大赛的比赛项目。".into());}
        self.selected=Some(index);self.phase_since=self.clock_ms;self.animation=None;
        if index==2{self.exam=Some(SchoolExam::new(questions,rng)?);self.phase=CompetitionPhase::QuizIntro;}else{self.phase=CompetitionPhase::Intro;self.intro_page=0;}
        Ok(())
    }
    fn next_intro(&mut self,rng:&mut ReplayRng)->Result<(),String>{
        self.intro_page+=1;self.phase_since=self.clock_ms;
        if self.intro_page==6{
            if self.selected==Some(0){self.beauty=Some(BeautyContest::new(self.stage,self.sex,self.charm,&self.rules,rng));self.phase=CompetitionPhase::Beauty;}
            else{self.running=Some(RunningContest::new(self.stage,self.sex,self.stamina,&self.rules,rng,self.clock_ms));self.phase=CompetitionPhase::Running;}
        }Ok(())
    }
    pub fn continue_action(&mut self,rng:&mut ReplayRng)->Result<(),String>{
        match self.phase{
            CompetitionPhase::Intro=>self.next_intro(rng)?,
            CompetitionPhase::QuizIntro=>{self.exam.as_mut().unwrap().started=true;self.phase=CompetitionPhase::QuizQuestion;self.phase_since=self.clock_ms;},
            CompetitionPhase::QuizResult=>self.start_award(),
            _=>return Err("请等待当前比赛动画。".into()),
        }Ok(())
    }
    pub fn answer(&mut self,index:usize,questions:&[ExamQuestion],rng:&mut ReplayRng)->Result<(),String>{
        if self.phase!=CompetitionPhase::QuizQuestion{return Err("当前不在答题阶段。".into());}
        // 0x44aed0 fills the question text before advancing the question bank;
        // 0x44b260 reuses that text through the answer feedback animation.
        let answered_question=self.exam.as_ref().unwrap().current.clone();
        self.last_correct=self.exam.as_mut().unwrap().answer(index,questions,rng)?;
        self.answered_question=Some(answered_question);
        self.feedback[0].reset();self.feedback[1].reset();self.phase=CompetitionPhase::QuizFeedback;Ok(())
    }
    pub fn running_input(&mut self,left:bool,right:bool)->Result<(),String>{
        if self.phase!=CompetitionPhase::Running{return Err("当前不在飞人大赛中。".into());}
        self.running.as_mut().unwrap().input(left,right);Ok(())
    }
    pub fn start_award(&mut self){
        if self.place>0{self.animation=Some(self.rules.rank_animations[self.place-1].clone());self.phase=CompetitionPhase::RankPrize;}
        else{self.start_money();}
    }
    fn start_money(&mut self){
        if self.prize>0{self.animation=Some(self.rules.money_animation.clone());self.phase=CompetitionPhase::MoneyPrize;}
        else{self.animation=None;self.phase=CompetitionPhase::Finished;}
    }
    pub fn take_prize(&mut self)->Option<i32>{if self.phase==CompetitionPhase::MoneyPrize&&!self.prize_paid{self.prize_paid=true;Some(i32::from(self.prize))}else{None}}
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng)->Result<(),String>{
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);let now=self.clock_ms;
        match self.phase{
            CompetitionPhase::Arrival=>{if now.wrapping_sub(self.phase_since)>=100{self.phase_since=now;let animation=self.animation.as_mut().unwrap();animation.frame+=1;if animation.frame==18{self.phase=CompetitionPhase::Opening;self.animation=Some(self.rules.opening_animation.clone());}}},
            CompetitionPhase::Opening=>{let animation=self.animation.as_mut().unwrap();animation.tick(now);if animation.last(){self.phase=CompetitionPhase::Menu;}},
            CompetitionPhase::Intro=>{if now.wrapping_sub(self.phase_since)>=8000{self.next_intro(rng)?;}},
            CompetitionPhase::Beauty=>{let beauty=self.beauty.as_mut().unwrap();beauty.tick(now);if beauty.phase==BeautyPhase::Finished{self.place=beauty.place;self.prize=beauty.prize;self.start_award();}},
            CompetitionPhase::Running=>{let running=self.running.as_mut().unwrap();running.tick(now);if running.phase==RunningPhase::Finished{self.place=running.place;self.prize=running.prize;self.start_award();}},
            CompetitionPhase::QuizIntro=>{if now.wrapping_sub(self.phase_since)>=4000{self.continue_action(rng)?;}},
            CompetitionPhase::QuizFeedback=>{
                let feedback=&mut self.feedback[usize::from(self.last_correct)];feedback.tick(now);
                if feedback.last(){
                    if !self.last_correct{feedback.reset();}
                    let exam=self.exam.as_ref().unwrap();if exam.answered==10{self.place=match exam.score{90.. =>3,80.. =>2,61.. =>1,_=>0};self.prize=if self.place==0{0}else{self.rules.prizes[self.place]};self.phase=CompetitionPhase::QuizResult;self.phase_since=now;}
                    else{self.phase=CompetitionPhase::QuizQuestion;}
                }
            },
            CompetitionPhase::QuizResult=>{if now.wrapping_sub(self.phase_since)>=4000{self.start_award();}},
            CompetitionPhase::RankPrize|CompetitionPhase::MoneyPrize=>{let animation=self.animation.as_mut().unwrap();animation.tick(now);if animation.last(){if self.phase==CompetitionPhase::RankPrize{self.start_money();}else{self.phase=CompetitionPhase::Finished;self.animation=None;}}},
            _=>{},
        }Ok(())
    }
    pub fn text(&self)->String{
        match self.phase{
            CompetitionPhase::Arrival|CompetitionPhase::Opening=>"学生大赛开始了！".into(),
            CompetitionPhase::Menu=>self.rules.menu[(self.stage-2).clamp(0,2)as usize][0].clone(),
            CompetitionPhase::Intro=>{let texts=if self.selected==Some(0){&self.rules.beauty_intro[self.sex]}else{&self.rules.running_intro[self.sex]};match self.intro_page{0=>texts[0].clone(),1=>format!("{}{}{}",texts[1],self.name,texts[2]),page=>texts[page+1].clone()}},
            CompetitionPhase::Beauty=>{let game=self.beauty.as_ref().unwrap();let index=match game.phase{BeautyPhase::WalkIn|BeautyPhase::Pose|BeautyPhase::WalkOut|BeautyPhase::AfterWalk=>game.contestant,BeautyPhase::Judges|BeautyPhase::Scores=>4,_=>6};self.rules.beauty_text[index].clone()},
            CompetitionPhase::QuizIntro=>self.rules.quiz_text[0].clone(),
            CompetitionPhase::QuizQuestion|CompetitionPhase::QuizFeedback=>{let exam=self.exam.as_ref().unwrap();self.rules.quiz_text[match(exam.answered,exam.score){(5,40..)=>3,(5,..30)=>4,(8,..50)=>6,(8,_)=>5,_=>1}].clone()},
            CompetitionPhase::QuizResult=>self.rules.quiz_text[7+self.place].clone(),
            CompetitionPhase::RankPrize|CompetitionPhase::MoneyPrize=>format!("{}，获得奖金 {} 元。",if self.place==0{"比赛结束"}else{["","第三名","第二名","第一名"][self.place]},self.prize),
            CompetitionPhase::Running=>{let index=self.running.as_ref().unwrap().displayed_text;if index<self.rules.running_text.len(){self.rules.running_text[index].clone()}else{self.rules.running.score_pause_text.clone()}},
            CompetitionPhase::Finished=>"比赛结束。".into(),
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn rules()->CompetitionRules{let catalog:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();serde_json::from_value(catalog["competition"].clone()).unwrap()}
    #[test]
    fn beauty_scores_prizes_and_rng_match_original_x86(){
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-competition.json")).unwrap();let rules=rules();
        for row in fixture["beauty"].as_array().unwrap(){
            let mut rng=ReplayRng::new(row["seed"].as_u64().unwrap());let game=BeautyContest::new(row["stage"].as_i64().unwrap()as i16,0,row["charm"].as_i64().unwrap()as i16,&rules,&mut rng);let actual=serde_json::to_value(game).unwrap();
            for key in ["scores","totals","winners","place","prize"]{assert_eq!(actual[key],row[key],"stage {} charm {} {key}",row["stage"],row["charm"]);}
            assert_eq!(u64::from(rng.state),row["rngState"].as_u64().unwrap());assert_eq!(rng.draws,12);
        }
    }
    #[test]
    fn beauty_frames_match_original_x86(){
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-beauty-frames.json")).unwrap();let rules=rules();
        for sequence in fixture["sequences"].as_array().unwrap(){
            let mut rng=ReplayRng::new(42);let mut game=BeautyContest::new(2,sequence["sex"].as_u64().unwrap()as usize,100,&rules,&mut rng);let mut now=0;
            for(index,row)in sequence["frames"].as_array().unwrap().iter().enumerate(){
                now+=row["elapsedMs"].as_u64().unwrap()as u32;game.tick(now);let value=serde_json::to_value(&game).unwrap();let expected=&row["after"];
                for key in ["contestant","podiumPlace"]{assert_eq!(value[key],expected[key],"sex {} step {} frame {index} {key}",sequence["sex"],sequence["step"]);}
                for key in ["stage","judges","podium"]{for(field,wanted)in expected[key].as_object().unwrap(){assert_eq!(&value[key][field],wanted,"sex {} step {} frame {index} {key}.{field}",sequence["sex"],sequence["step"]);}}
                for key in ["podiumPlaces"]{for(actual,expected)in value[key].as_array().unwrap().iter().zip(expected[key].as_array().unwrap()){for(field,wanted)in expected.as_object().unwrap(){assert_eq!(&actual[field],wanted,"sex {} step {} frame {index} {key}.{field}",sequence["sex"],sequence["step"]);}}}
                for(actor,expected)in value["actors"].as_array().unwrap().iter().zip(expected["actors"].as_array().unwrap()){for(actual,expected)in actor.as_array().unwrap().iter().zip(expected.as_array().unwrap()){for(field,wanted)in expected.as_object().unwrap(){assert_eq!(&actual[field],wanted,"sex {} step {} frame {index} actor.{field}",sequence["sex"],sequence["step"]);}}}
            }
            assert_eq!(game.phase,BeautyPhase::Finished);assert_eq!(rng.draws,12);
        }
    }
    #[test]
    fn quiz_prefetches_after_tenth_answer_and_waits_before_paying(){
        let questions:Vec<ExamQuestion>=(0..30).map(|id|ExamQuestion{id,question:format!("{id}"),options:vec!["a".into(),"b".into(),"c".into(),"d".into()]}).collect();let mut rng=ReplayRng::new(42);
        let mut game=StudentCompetition::new(2,0,"测试".into(),100,100,rules());
        for _ in 0..18{game.tick(100,&mut rng).unwrap();}while game.phase==CompetitionPhase::Opening{game.tick(100,&mut rng).unwrap();}assert_eq!(game.phase,CompetitionPhase::Menu);assert_eq!(rng.draws,0);
        game.select(2,&questions,&mut rng).unwrap();game.continue_action(&mut rng).unwrap();
        for answered in 1..=10{
            let question=game.exam.as_ref().unwrap().current.clone();let answer=game.exam.as_ref().unwrap().correct;game.answer(answer,&questions,&mut rng).unwrap();assert_eq!(game.exam.as_ref().unwrap().answered,answered);assert_eq!(game.answered_question,Some(question.clone()));assert_ne!(game.exam.as_ref().unwrap().current.id,question.id);assert!(game.take_prize().is_none());
            for _ in 0..100{if game.phase!=CompetitionPhase::QuizFeedback{break;}game.tick(100,&mut rng).unwrap();}
        }
        assert_eq!(game.exam.as_ref().unwrap().used.len(),11);assert_eq!(game.phase,CompetitionPhase::QuizResult);assert_eq!(game.place,3);assert_eq!(game.prize,10000);
        game.continue_action(&mut rng).unwrap();assert!(game.take_prize().is_none());
        while game.phase==CompetitionPhase::RankPrize{game.tick(100,&mut rng).unwrap();}
        assert_eq!(game.take_prize(),Some(10000));assert_eq!(game.take_prize(),None);
        let copy:StudentCompetition=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();assert_eq!(game,copy);
        while game.phase!=CompetitionPhase::Finished{game.tick(100,&mut rng).unwrap();}assert_eq!(game.take_prize(),None);
    }
}
