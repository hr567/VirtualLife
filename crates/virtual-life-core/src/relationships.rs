//! Relationship-house choices recovered from native entry 0x00438480 and its
//! three submenus. The event program still owns the ensuing date/marriage story.
use std::collections::BTreeMap;
use crate::rng::ReplayRng;
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct RelationshipQuestion {pub prompt:String,pub answers:[String;3]}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct RelationshipPerson {
    pub id:u8,pub player_sex:u8,pub cost:i16,pub charm_required:i16,pub romance_required:i16,
    pub selected_portrait:String,pub portrait:String,pub introduction:String,
    pub questions:[RelationshipQuestion;2],
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct RelationshipRules {
    pub people:Vec<RelationshipPerson>,pub event_bases:[[i32;4];2],
    pub messages:BTreeMap<String,String>,pub requirements_failed:String,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct RelationshipState {
    pub sex:u8,pub stage:i16,pub money:i32,pub charm:i16,pub romance:i16,
    /// Original byte arrays at 0x47ad8c and 0x47ad9c. Only the first four
    /// contiguous contacts are presented; old new-target slots are retained.
    pub contacts:[i8;8],pub new_targets:[i8;8],pub dating_target:i8,pub spouse:i8,pub gift_level:u8,
}

impl RelationshipState {
    pub fn contact_count(&self)->usize {self.contacts[..4].iter().position(|&id|id==-1).unwrap_or(4)}
    fn validate(&self)->Result<(),String> {
        if self.sex>1 || !(1..=4).contains(&self.gift_level) ||
            self.contacts.iter().chain(self.new_targets.iter()).chain([&self.dating_target,&self.spouse]).any(|&id|!(-1..8).contains(&id)) {
            return Err("Invalid original relationship state".into());
        }
        let active=&self.contacts[..self.contact_count()];
        for (index,id) in active.iter().enumerate() {if active[..index].contains(id) {return Err("Duplicate original relationship contact".into());}}
        Ok(())
    }
}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum RelationshipPhase {Main,Meet,Question,Notice,Dates,DateConfirm,Marriages,MarriageConfirm,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct RelationshipHouse {
    pub state:RelationshipState,pub phase:RelationshipPhase,pub people:Vec<RelationshipPerson>,
    pub offers:[u8;4],pub marriage_candidates:Vec<u8>,pub hovered_offer:Option<usize>,
    pub selected_person:Option<u8>,pub answer_positions:[usize;2],pub question_index:usize,
    pub correct:u8,pub attempts:u8,pub message:String,pub notice_kind:u8,
    pub clock_ms:u32,pub notice_at:u32,pub return_kind:u8,
    pub event_bases:[i32;4],pub messages:BTreeMap<String,String>,pub requirements_failed:String,
}

/// FUN_004379f0 (also inlined in 0x00437a50): rejection sampling draws four
/// distinct people absent from the first four contiguous existing contacts.
pub fn draw_offers(state:&RelationshipState,rng:&mut ReplayRng)->Result<[u8;4],String> {
    state.validate()?;
    let contacts=&state.contacts[..state.contact_count()];
    let mut offers=[0u8;4];
    for index in 0..4 {
        loop {
            let candidate=(rng.next_rand()%8) as u8;
            if !contacts.contains(&(candidate as i8)) && !offers[..index].contains(&candidate) {offers[index]=candidate;break;}
        }
    }
    Ok(offers)
}

impl RelationshipHouse {
    pub fn new(rules:&RelationshipRules,state:RelationshipState,mission:bool,rng:&mut ReplayRng)->Result<Self,String> {
        state.validate()?;
        let people:Vec<_>=rules.people.iter().filter(|person|person.player_sex==state.sex).cloned().collect();
        if people.len()!=8 || people.iter().enumerate().any(|(index,person)|usize::from(person.id)!=index) {
            return Err("Original relationship catalog must contain eight ordered people per sex".into());
        }
        let married=state.spouse>=0;
        let offers=if married {[0;4]}else{draw_offers(&state,rng)?};
        let marriage_candidates=state.contacts[..state.contact_count()].iter().filter_map(|&id|
            (people[id as usize].romance_required<=state.romance).then_some(id as u8)).collect();
        let message_key=if mission {"mission"} else if state.stage<2 {"child"}else if state.stage<3 {"junior"}else{"welcome"};
        let message=rules.messages.get(message_key).cloned().unwrap_or_default();
        let event_bases=rules.event_bases[usize::from(state.sex)];
        Ok(Self {state,phase:if married {RelationshipPhase::Finished}else{RelationshipPhase::Main},people,offers,marriage_candidates,
            hovered_offer:None,selected_person:None,answer_positions:[0;2],question_index:0,correct:0,attempts:0,message,
            notice_kind:0,clock_ms:0,notice_at:0,return_kind:if married {4}else{0},event_bases,
            messages:rules.messages.clone(),requirements_failed:rules.requirements_failed.clone()})
    }

    fn text(&mut self,key:&str) {self.message=self.messages.get(key).cloned().unwrap_or_default();}
    fn main_action(&self)->Result<(),String> {
        if self.phase!=RelationshipPhase::Main || self.state.stage<3 {return Err("The relationship activity is unavailable".into());}
        Ok(())
    }
    pub fn meet(&mut self)->Result<(),String> {
        self.main_action()?;
        if self.state.contact_count()>=4 {return Err("The original relationship contact limit is four".into());}
        self.phase=RelationshipPhase::Meet;self.text("meet");Ok(())
    }
    pub fn dates(&mut self)->Result<(),String> {
        self.main_action()?;
        if self.state.contact_count()==0 {return Err("There are no known relationship contacts".into());}
        self.phase=RelationshipPhase::Dates;self.text("date");Ok(())
    }
    pub fn marriages(&mut self)->Result<(),String> {
        self.main_action()?;
        if self.marriage_candidates.is_empty() {return Err("No contact meets the original romance requirement".into());}
        self.phase=RelationshipPhase::Marriages;self.text("marriage");Ok(())
    }

    /// One original rendering iteration. Hovering an available introduction
    /// redraws two answer-position RNG values on EVERY frame, not just entry.
    pub fn tick(&mut self,elapsed_ms:u32,hovered_offer:Option<usize>,rng:&mut ReplayRng)->Result<(),String> {
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        if self.phase==RelationshipPhase::Main && self.state.stage<3 && self.clock_ms>=10000 {self.close();}
        if self.phase==RelationshipPhase::Notice {
            let elapsed=self.clock_ms.wrapping_sub(self.notice_at);
            if (self.notice_kind==1 && elapsed>10000) || (self.notice_kind==2 && elapsed>=10000) {self.advance_notice()?;}
        }
        if self.phase==RelationshipPhase::Meet {
            self.hovered_offer=hovered_offer;
            if let Some(index)=hovered_offer {self.hover(index,rng)?;}
        }
        Ok(())
    }

    pub fn hover(&mut self,index:usize,rng:&mut ReplayRng)->Result<(),String> {
        if self.phase!=RelationshipPhase::Meet {return Err("Relationship introduction is not active".into());}
        let person=*self.offers.get(index).ok_or("Relationship offer index is out of range")?;
        self.hovered_offer=Some(index);self.message=self.people[person as usize].introduction.clone();
        self.answer_positions=[(rng.next_rand()%3) as usize,(rng.next_rand()%3) as usize];
        self.question_index=0;self.correct=0;Ok(())
    }

    /// Call after this frame's hover update, as in the original input loop.
    pub fn select_offer(&mut self,index:usize)->Result<(),String> {
        if self.phase!=RelationshipPhase::Meet || self.hovered_offer!=Some(index) {return Err("Hover the original candidate before selecting it".into());}
        if self.state.contact_count()>=4 {self.text("full");self.show_notice(1);return Ok(());}
        let person=*self.offers.get(index).ok_or("Relationship offer index is out of range")?;
        let rule=&self.people[person as usize];
        self.selected_person=Some(person);
        if self.state.charm<rule.charm_required || self.state.money<i32::from(rule.cost) {
            self.message=self.requirements_failed.clone();self.selected_person=None;return Ok(());
        }
        self.phase=RelationshipPhase::Question;self.question_index=0;self.correct=0;
        self.message=rule.questions[0].prompt.clone();Ok(())
    }

    pub fn question(&self)->Result<RelationshipQuestion,String> {
        if self.phase!=RelationshipPhase::Question {return Err("No relationship question is active".into());}
        let person=self.selected_person.ok_or("Relationship candidate is missing")?;
        let question=&self.people[person as usize].questions[self.question_index];
        let mut wrong=1;
        let answers=std::array::from_fn(|index| {
            let source=if index==self.answer_positions[self.question_index] {0}else{let answer=wrong;wrong+=1;answer};
            question.answers[source].clone()
        });
        Ok(RelationshipQuestion {prompt:question.prompt.clone(),answers})
    }

    pub fn answer(&mut self,choice:usize,rng:&mut ReplayRng)->Result<(),String> {
        if self.phase!=RelationshipPhase::Question || choice>=3 {return Err("Invalid relationship answer".into());}
        if choice==self.answer_positions[self.question_index] {self.correct+=1;}
        self.question_index+=1;
        if self.question_index<2 {
            self.message=self.people[self.selected_person.unwrap() as usize].questions[self.question_index].prompt.clone();return Ok(());
        }
        self.settle_answers(rng)
    }

    fn settle_answers(&mut self,rng:&mut ReplayRng)->Result<(),String> {
        let person=self.selected_person.ok_or("Relationship candidate is missing")?;
        if self.correct>0 {
            let slot=self.state.contact_count();
            if slot>=4 || usize::from(self.attempts)>=8 {return Err("Original relationship array is full".into());}
            self.state.new_targets[usize::from(self.attempts)]=person as i8;
            self.state.contacts[slot]=person as i8;
            self.state.money=self.state.money.wrapping_sub(i32::from(self.people[person as usize].cost));
            self.return_kind=1;self.attempts+=1;self.text("success");
            if self.attempts<self.state.gift_level {
                self.offers=draw_offers(&self.state,rng)?;self.phase=RelationshipPhase::Meet;
            } else {self.show_notice(2);}
        } else {
            self.attempts+=1;self.text("rejection");self.show_notice(if self.attempts<self.state.gift_level {1}else{2});
        }
        self.selected_person=None;self.hovered_offer=None;self.question_index=0;self.correct=0;
        Ok(())
    }

    fn show_notice(&mut self,kind:u8) {self.notice_kind=kind;self.notice_at=self.clock_ms;self.phase=RelationshipPhase::Notice;}
    pub fn advance_notice(&mut self)->Result<(),String> {
        if self.phase!=RelationshipPhase::Notice {return Err("No relationship result is awaiting acknowledgement".into());}
        if self.notice_kind==2 {self.close();}else{self.phase=RelationshipPhase::Meet;self.notice_kind=0;}
        Ok(())
    }

    pub fn select_date(&mut self,index:usize)->Result<(),String> {
        if !matches!(self.phase,RelationshipPhase::Dates|RelationshipPhase::DateConfirm) || index>=self.state.contact_count() {return Err("Invalid dating contact".into());}
        self.state.dating_target=self.state.contacts[index];self.selected_person=Some(self.state.dating_target as u8);
        self.phase=RelationshipPhase::DateConfirm;self.text("date");Ok(())
    }
    pub fn select_marriage(&mut self,index:usize)->Result<(),String> {
        if !matches!(self.phase,RelationshipPhase::Marriages|RelationshipPhase::MarriageConfirm) {return Err("Marriage selection is not active".into());}
        let person=*self.marriage_candidates.get(index).ok_or("Invalid marriage contact")?;
        // The native selection writes the spouse byte immediately. Exiting
        // without pressing the explicit No button leaves that byte selected.
        self.state.spouse=person as i8;self.selected_person=Some(person);
        self.phase=RelationshipPhase::MarriageConfirm;self.text("confirmMarriage");Ok(())
    }
    pub fn reject_selection(&mut self)->Result<(),String> {
        match self.phase {
            RelationshipPhase::DateConfirm=>{self.state.dating_target=-1;self.phase=RelationshipPhase::Dates;self.text("date");}
            RelationshipPhase::MarriageConfirm=>{self.state.spouse=-1;self.phase=RelationshipPhase::Marriages;self.text("marriage");}
            _=>return Err("No relationship selection to reject".into()),
        }
        self.selected_person=None;Ok(())
    }
    pub fn confirm_selection(&mut self)->Result<(),String> {
        self.return_kind=match self.phase {RelationshipPhase::DateConfirm=>2,RelationshipPhase::MarriageConfirm=>3,_=>return Err("No relationship selection to confirm".into())};
        self.close();Ok(())
    }
    pub fn close(&mut self) {self.phase=RelationshipPhase::Finished;}
    pub fn event_nodes(&self)->Result<Vec<i32>,String> {
        if self.phase!=RelationshipPhase::Finished {return Err("Relationship activity has not finished".into());}
        Ok(match self.return_kind {
            0=>Vec::new(),
            1=>self.state.new_targets.iter().filter(|&&id|id!=-1).map(|&id|self.event_bases[0]+i32::from(id)).collect(),
            2=>vec![self.event_bases[1]+i32::from(self.state.dating_target)],
            3=>vec![self.event_bases[2]+i32::from(self.state.spouse)],
            4=>vec![self.event_bases[3]+i32::from(self.state.spouse)],
            _=>return Err("Invalid original relationship return kind".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules()->RelationshipRules {
        let catalog:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        serde_json::from_value(catalog["relationships"].clone()).unwrap()
    }
    fn state()->RelationshipState {RelationshipState{sex:0,stage:6,money:30000,charm:9999,romance:90,contacts:[-1;8],new_targets:[-1;8],dating_target:-1,spouse:-1,gift_level:1}}
    #[test]
    fn relationship_candidate_rejection_sampling_matches_original_x86() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-relationships.json")).unwrap();
        for case in vectors["candidates"].as_array().unwrap() {
            let mut state=state();for(index,id)in case["contacts"].as_array().unwrap().iter().enumerate(){state.contacts[index]=id.as_i64().unwrap() as i8;}
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            assert_eq!(serde_json::to_value(draw_offers(&state,&mut rng).unwrap()).unwrap(),case["offers"]);
            assert_eq!(u64::from(rng.state),case["rngState"].as_u64().unwrap());assert_eq!(rng.draws,case["rngDraws"].as_u64().unwrap());
        }
    }
    #[test]
    fn relationship_settlement_matches_original_x86_and_retains_old_new_targets() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-relationships.json")).unwrap();
        for case in vectors["settlements"].as_array().unwrap() {
            let mut state=state();state.sex=case["sex"].as_u64().unwrap() as u8;state.gift_level=case["giftLevel"].as_u64().unwrap() as u8;
            state.new_targets=[7,6,-1,-1,-1,-1,-1,-1];
            for(index,id)in case["contacts"].as_array().unwrap().iter().enumerate(){state.contacts[index]=id.as_i64().unwrap() as i8;}
            let mut house=RelationshipHouse::new(&rules(),state,false,&mut ReplayRng::new(0)).unwrap();
            house.offers=[0,1,2,3];house.phase=RelationshipPhase::Question;house.selected_person=Some(case["candidate"].as_u64().unwrap() as u8);
            house.attempts=case["attempt"].as_u64().unwrap() as u8;house.question_index=1;house.correct=u8::from(case["firstCorrect"].as_bool().unwrap());house.answer_positions=[2,1];
            let mut rng=ReplayRng::new(42);house.answer(usize::from(case["secondCorrect"].as_bool().unwrap()),&mut rng).unwrap();
            assert_eq!(house.state.money,case["money"].as_i64().unwrap() as i32);
            assert_eq!(serde_json::to_value(house.state.contacts).unwrap(),case["afterContacts"]);
            assert_eq!(serde_json::to_value(house.state.new_targets).unwrap(),case["newTargets"]);
            assert_eq!(house.attempts,case["attempts"].as_u64().unwrap() as u8);assert_eq!(house.notice_kind,case["noticeKind"].as_u64().unwrap() as u8);
            assert_eq!(house.return_kind,case["returnKind"].as_u64().unwrap() as u8);assert_eq!(serde_json::to_value(house.offers).unwrap(),case["offers"]);
            assert_eq!(u64::from(rng.state),case["rngState"].as_u64().unwrap());assert_eq!(rng.draws,case["rngDraws"].as_u64().unwrap());
        }
    }
    #[test]
    fn relationship_hover_save_and_married_branch_preserve_original_rng_boundaries() {
        let mut rng=ReplayRng::new(42);let mut house=RelationshipHouse::new(&rules(),state(),false,&mut rng).unwrap();house.meet().unwrap();
        let draws=rng.draws;house.tick(16,Some(0),&mut rng).unwrap();house.tick(16,Some(0),&mut rng).unwrap();assert_eq!(rng.draws,draws+4);
        house.select_offer(0).unwrap();let mut restored:RelationshipHouse=serde_json::from_str(&serde_json::to_string(&house).unwrap()).unwrap();let mut restored_rng=rng.clone();
        for _ in 0..2 {let answer=house.answer_positions[house.question_index];house.answer(answer,&mut rng).unwrap();restored.answer(answer,&mut restored_rng).unwrap();}
        assert_eq!(house,restored);assert_eq!(rng,restored_rng);house.advance_notice().unwrap();assert_eq!(house.event_nodes().unwrap().len(),1);
        let mut married=state();married.spouse=5;let before=rng.clone();let house=RelationshipHouse::new(&rules(),married,false,&mut rng).unwrap();assert_eq!(rng,before);assert_eq!(house.event_nodes().unwrap(),[4055]);
    }
    #[test]
    fn relationship_selection_exit_and_explicit_no_preserve_native_spouse_behavior() {
        let mut state=state();state.contacts[0]=3;
        let mut house=RelationshipHouse::new(&rules(),state,false,&mut ReplayRng::new(1)).unwrap();house.marriages().unwrap();house.select_marriage(0).unwrap();
        house.reject_selection().unwrap();assert_eq!(house.state.spouse,-1);house.select_marriage(0).unwrap();house.close();assert_eq!(house.state.spouse,3);assert!(house.event_nodes().unwrap().is_empty());
    }
}
