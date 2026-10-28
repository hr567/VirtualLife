//! The original narrated ending (00452260) and staff scroll (00452de0).
//! Clocks advance once per rendered frame; they never catch up skipped frames.
use serde::{Deserialize,Serialize};
use crate::ending_summary::EndingSummary;

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum EndingPhase{Summary,Movie,Credits,EndImage,Reincarnation,Finished}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct EndingFlow{
 pub victory:bool,pub phase:EndingPhase,#[serde(skip_serializing_if="Option::is_none")]pub summary:Option<EndingSummary>,
 #[serde(skip_serializing_if="Option::is_none")]pub movie:Option<EndingMovie>,
 #[serde(skip_serializing_if="Option::is_none")]pub credits:Option<EndingCredits>,
}
impl EndingFlow{
 pub fn new(victory:bool,summary:Option<EndingSummary>,rules:&EpilogueRules)->Self{
  let phase=if summary.is_some(){EndingPhase::Summary}else{EndingPhase::Movie};
  let movie=if summary.is_none(){Some(EndingMovie::new(victory,rules))}else{None};
  Self{victory,phase,summary,movie,credits:None}
 }
 pub fn tick(&mut self,elapsed_ms:u32,rules:&EpilogueRules){match self.phase{
  EndingPhase::Summary=>{let summary=self.summary.as_mut().unwrap();summary.tick(elapsed_ms);if summary.finished{self.summary=None;self.movie=Some(EndingMovie::new(self.victory,rules));self.phase=EndingPhase::Movie;}}
  EndingPhase::Movie=>{let movie=self.movie.as_mut().unwrap();movie.tick(elapsed_ms,rules);if movie.phase==MoviePhase::Finished{self.movie=None;if self.victory{self.credits=Some(EndingCredits::new(&rules.credits));self.phase=EndingPhase::Credits;}else{self.phase=EndingPhase::Finished;}}}
  EndingPhase::Credits=>{let credits=self.credits.as_mut().unwrap();credits.tick(elapsed_ms,&rules.credits);if credits.finished{self.credits=None;self.phase=EndingPhase::EndImage;}}
  _=>{}
 }}
 pub fn action(&mut self,action:&str)->Result<(),String>{match self.phase{
  EndingPhase::Summary=>{let summary=self.summary.as_mut().unwrap();match action{"continue"=>summary.input(true,false,false),"escape"=>summary.input_escape(),"sDown"=>summary.input(false,true,false),"sUp"=>summary.input(false,false,true),_=>return Err("无效的人生回顾操作。".into())}}
  EndingPhase::Movie=>{if action!="escape"{return Err("按 Escape 跳过结局影片。".into());}self.movie.as_mut().unwrap().escape_released=true;}
  EndingPhase::Credits=>{let credits=self.credits.as_mut().unwrap();match action{"escape"=>credits.input_escape(),"sDown"=>credits.input_s(true),"sUp"=>credits.input_s(false),_=>return Err("无效的片尾名单操作。".into())}}
  EndingPhase::EndImage=>{if action!="continue"&&action!="escape"{return Err("请继续查看转世选择。".into());}self.phase=EndingPhase::Reincarnation;}
  _=>return Err("当前结局阶段不接受此操作。".into()),
 }Ok(())}
}

#[derive(Clone,Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct EpilogueRules{pub subtitles:[Vec<Subtitle>;2],pub segments:Vec<Segment>,pub credits:CreditsRules,pub outro_metadata:VideoMetadata}
#[derive(Clone,Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Subtitle{pub text:String,pub segment:usize,pub duration_ms:u32}
#[derive(Clone,Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Segment{pub start_ms:u32,pub end_ms:u32}
#[derive(Clone,Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct VideoMetadata{pub duration_ms:u32}
#[derive(Clone,Debug,Deserialize)]
pub struct CreditsRules{pub rect:[i32;4],pub paragraphs:Vec<CreditsParagraph>}
#[derive(Clone,Debug,Deserialize)]
pub struct CreditsParagraph{pub y:i32}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum MoviePhase{Subtitles,Outro,Finished}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct EndingMovie{
 pub phase:MoviePhase,pub victory:bool,pub clock_ms:u32,pub subtitle_index:usize,
 pub displayed_subtitle:Option<usize>,pub segment:usize,pub video_time_ms:u32,
 pub displayed_video_time_ms:u32,pub narration_started:bool,pub narration_started_ms:u32,
 pub last_subtitle_ms:u32,pub seek_serial:u32,pub escape_released:bool,
}
impl EndingMovie{
 pub fn new(victory:bool,rules:&EpilogueRules)->Self{
  let segment=rules.subtitles[usize::from(victory)][0].segment;
  Self{phase:MoviePhase::Subtitles,victory,clock_ms:0,subtitle_index:0,displayed_subtitle:None,segment,
   video_time_ms:rules.segments[segment].start_ms,displayed_video_time_ms:rules.segments[segment].start_ms,
   narration_started:false,narration_started_ms:0,last_subtitle_ms:0,seek_serial:0,escape_released:false}
 }
 pub fn tick(&mut self,elapsed_ms:u32,rules:&EpilogueRules){
  if self.phase==MoviePhase::Finished{return;}
  self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
  if std::mem::take(&mut self.escape_released){self.phase=MoviePhase::Finished;return;}
  self.video_time_ms=self.video_time_ms.saturating_add(elapsed_ms);self.displayed_video_time_ms=self.video_time_ms;
  match self.phase{
   MoviePhase::Subtitles=>{
    let segment=&rules.segments[self.segment];
    if self.video_time_ms>=segment.end_ms{self.video_time_ms=segment.start_ms;self.seek_serial=self.seek_serial.wrapping_add(1);}
    if !self.narration_started && self.clock_ms>200{
     self.narration_started=true;self.narration_started_ms=self.clock_ms;self.last_subtitle_ms=self.clock_ms;
    }
    let rows=&rules.subtitles[usize::from(self.victory)];
    if let Some(row)=rows.get(self.subtitle_index){
     self.displayed_subtitle=Some(self.subtitle_index);
     if self.clock_ms.wrapping_sub(self.last_subtitle_ms)>row.duration_ms{
      self.last_subtitle_ms=self.clock_ms;self.subtitle_index+=1;
      if let Some(next)=rows.get(self.subtitle_index){if next.segment!=self.segment{
       self.segment=next.segment;self.video_time_ms=rules.segments[self.segment].start_ms;self.seek_serial=self.seek_serial.wrapping_add(1);
      }}
     }
    }else{self.phase=MoviePhase::Outro;self.video_time_ms=0;self.displayed_video_time_ms=0;self.displayed_subtitle=None;self.seek_serial=self.seek_serial.wrapping_add(1);}
   }
   MoviePhase::Outro=>{if self.video_time_ms>=rules.outro_metadata.duration_ms{self.phase=MoviePhase::Finished;}}
   MoviePhase::Finished=>{}
  }
 }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct EndingCredits{
 pub clock_ms:u32,pub offset:i32,pub speed:i32,pub last_scroll_ms:u32,pub fade_started_ms:u32,
 pub fading_in:bool,pub fading_out:bool,pub brightness:u8,pub finished:bool,
 pub escape_released:bool,pub s_pressed:bool,pub s_released:bool,
}
impl EndingCredits{
 pub fn new(rules:&CreditsRules)->Self{Self{clock_ms:0,offset:rules.rect[3]-rules.rect[1],speed:-1,last_scroll_ms:0,fade_started_ms:0,
  fading_in:true,fading_out:false,brightness:0,finished:false,escape_released:false,s_pressed:false,s_released:false}}
 /// 004597a0 replaces the keyboard event buffer each frame; 00459b60/ba0
 /// inspect its final event. A newer S edge supersedes an earlier one.
 pub fn input_s(&mut self,pressed:bool){self.s_pressed=pressed;self.s_released=!pressed;self.escape_released=false;}
 pub fn input_escape(&mut self){self.escape_released=true;self.s_pressed=false;self.s_released=false;}
 pub fn tick(&mut self,elapsed_ms:u32,rules:&CreditsRules){
  if self.finished{return;}
  self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
  if std::mem::take(&mut self.escape_released)&&!self.fading_in&&!self.fading_out{self.fading_out=true;self.fade_started_ms=self.clock_ms;}
  let s_pressed=std::mem::take(&mut self.s_pressed);let s_released=std::mem::take(&mut self.s_released);
  if self.speed==-1{if s_pressed{self.speed=-40;}}else if s_released{self.speed=-1;}
  if self.clock_ms.wrapping_sub(self.last_scroll_ms)>50{
   self.offset=self.offset.wrapping_add(self.speed);
   if !self.fading_out && rules.paragraphs.last().map_or(false,|row|row.y.wrapping_add(self.offset)<80){self.fading_out=true;self.fade_started_ms=self.clock_ms;}
   self.last_scroll_ms=self.clock_ms;
  }
  let elapsed=self.clock_ms.wrapping_sub(self.fade_started_ms);
  if self.fading_in{
   if elapsed<2500{self.brightness=(224*elapsed/2500)as u8;}else{self.brightness=224;self.fading_in=false;}
  }else if self.fading_out{
   if elapsed>2500{self.finished=true;}else{self.brightness=(224*(2500-elapsed)/2500)as u8;}
  }else{self.brightness=224;}
 }
}

#[cfg(test)]
mod tests{
 use super::*;use serde_json::Value;
 fn rules()->EpilogueRules{let root:Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();serde_json::from_value(root["epilogue"].clone()).unwrap()}
 #[test]fn credits_require_key_release_and_single_frame_scroll(){let rules=rules();let mut game=EndingCredits::new(&rules.credits);
  game.escape_released=true;game.tick(2500,&rules.credits);assert!(!game.fading_in);assert!(!game.fading_out);assert_eq!(game.offset,359);
  game.s_pressed=true;game.tick(50,&rules.credits);assert_eq!(game.offset,359);game.tick(1,&rules.credits);assert_eq!(game.offset,319);
  game.input_s(false);game.tick(49,&rules.credits);game.input_escape();game.tick(1,&rules.credits);assert!(game.fading_out);assert_eq!(game.speed,-1);
  game.tick(2500,&rules.credits);assert!(!game.finished);assert_eq!(game.brightness,0);game.tick(1,&rules.credits);assert!(game.finished);
 }
 #[test]fn movie_keeps_caption_for_its_final_frame_and_waits_before_voice(){let rules=rules();let mut game=EndingMovie::new(true,&rules);
  game.tick(200,&rules);assert!(!game.narration_started);game.tick(1,&rules);assert!(game.narration_started);
  game.tick(500,&rules);assert_eq!(game.subtitle_index,0);game.tick(1,&rules);assert_eq!(game.subtitle_index,1);assert_eq!(game.displayed_subtitle,Some(0));
  game.tick(1,&rules);assert_eq!(game.displayed_subtitle,Some(1));game.escape_released=true;game.tick(1,&rules);assert_eq!(game.phase,MoviePhase::Finished);
 }
 #[test]fn credits_rapid_s_edges_keep_only_the_final_event_and_expire_each_frame(){
  let rules=rules();let mut game=EndingCredits::new(&rules.credits);
  game.input_s(true);game.tick(51,&rules.credits);assert_eq!(game.speed,-40);
  game.input_s(false);game.input_s(true);game.input_s(false);
  let save=serde_json::to_string(&game).unwrap();game=serde_json::from_str(&save).unwrap();
  for _ in 0..3{game.tick(51,&rules.credits);assert_eq!(game.speed,-1);assert!(!game.s_pressed&&!game.s_released);}
  game.input_s(true);game.tick(51,&rules.credits);game.input_s(true);game.tick(51,&rules.credits);
  assert!(!game.s_pressed);game.input_s(false);game.tick(51,&rules.credits);game.tick(51,&rules.credits);assert_eq!(game.speed,-1);
  game.input_s(false);game.tick(51,&rules.credits);game.input_s(true);game.tick(51,&rules.credits);game.tick(51,&rules.credits);assert_eq!(game.speed,-40);
  game.tick(2500,&rules.credits);game.input_escape();game.input_s(false);game.tick(1,&rules.credits);assert!(!game.fading_out);assert_eq!(game.speed,-1);
  game.input_s(true);game.input_escape();game.tick(1,&rules.credits);assert!(game.fading_out);assert_eq!(game.speed,-1);
 }
 #[test]fn ending_frames_match_original_x86(){
  let rules=rules();let fixture:Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-epilogue.json")).unwrap();
  for(sequence,row)in fixture["movies"].as_array().unwrap().iter().enumerate(){let mut game=EndingMovie::new(row["victory"].as_bool().unwrap(),&rules);
   for(frame,row)in row["frames"].as_array().unwrap().iter().enumerate(){game.escape_released=row["escape"].as_bool().unwrap();game.tick(row["elapsedMs"].as_u64().unwrap()as u32,&rules);let actual=serde_json::to_value(&game).unwrap();
    for(key,value)in row["after"].as_object().unwrap(){if game.phase==MoviePhase::Finished&&(key=="videoTimeMs"||key=="displayedVideoTimeMs"){continue;}assert_eq!(&actual[key],value,"movie {sequence} frame {frame} {key}");}
   }
  }
  for(sequence,row)in fixture["credits"].as_array().unwrap().iter().enumerate(){let mut game=EndingCredits::new(&rules.credits);
   for(frame,row)in row["frames"].as_array().unwrap().iter().enumerate(){for action in row["actions"].as_array().unwrap(){match action.as_str().unwrap(){"sDown"=>game.input_s(true),"sUp"=>game.input_s(false),"escape"=>game.input_escape(),_=>unreachable!()}}
    game.tick(row["elapsedMs"].as_u64().unwrap()as u32,&rules.credits);let actual=serde_json::to_value(&game).unwrap();
    for(key,value)in row["after"].as_object().unwrap(){if value.is_null(){continue;}assert_eq!(&actual[key],value,"credits {sequence} frame {frame} {key}");}
   }
  }
 }
}
