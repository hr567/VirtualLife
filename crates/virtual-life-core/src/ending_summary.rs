//! Original life-history scroll and career portrait, 0044d010/0044dd70.
//! Native uninitialized history slots use the same zeroed-allocation baseline
//! as the executable oracle; no claim is made about an old process's heap.
use crate::competition::CompetitionAnimation;
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct EndingSummaryRules {
    pub header:String,pub paragraphs:Vec<Vec<String>>,pub conclusions:Vec<Vec<String>>,
    pub encoded_header:Vec<u8>,pub encoded_paragraphs:Vec<Vec<Vec<u8>>>,pub encoded_conclusions:Vec<Vec<Vec<u8>>>,
    pub gender_replacements:Vec<Vec<String>>,pub encoded_gender_replacements:Vec<Vec<Vec<u8>>>,
    pub text_x:i32,pub initial_y:i32,pub clip:[i32;4],pub line_height:i32,
    pub backgrounds:Vec<Vec<String>>,pub portraits:Vec<Vec<String>>,pub actors:Vec<CompetitionAnimation>,
    pub palettes:Vec<Vec<Vec<[u8;4]>>>,
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct SummaryParagraph {
    /// Human-readable text, before the original byte-based line breaks.
    pub text:String,
    /// Original GBK bytes with LF inserted every 32 bytes, even within a glyph.
    pub bytes:Vec<u8>,pub x:i32,pub y:i32,pub height:i32,
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct EndingSummary {
    pub finished:bool,pub clock_ms:u32,pub sex:usize,pub career:usize,pub victory:bool,
    pub background:String,pub portrait:String,pub actor:CompetitionAnimation,pub paragraphs:Vec<SummaryParagraph>,
    pub clip:[i32;4],pub scroll_y:i32,pub actor_y:i32,pub show_text:bool,pub show_actor:bool,
    pub scrolling:bool,pub actor_entering:bool,pub waiting:bool,pub graying:bool,pub gray_pause:bool,
    pub fading_in:bool,pub flashing:bool,pub flash_dirty:bool,pub flash_alternate:bool,
    pub speed:i32,pub last_scroll_ms:u32,pub last_flash_ms:u32,pub fade_started_ms:u32,
    pub original_palette:Vec<[u8;4]>,pub palette:Vec<[u8;4]>,pub base_palette:Vec<[u8;4]>,pub alternate_palette:Vec<[u8;4]>,
    pub allocated_paragraphs:usize,pub end_bottom:i32,pub skip:bool,pub s_down:bool,pub s_up:bool,
    #[serde(default,skip_serializing_if="is_false")]
    pub escape_released:bool,
    initial_y:i32,
}
fn is_false(value:&bool)->bool{!*value}

fn substitute_bytes(template:&[u8],value:i32)->Vec<u8>{
    let replacement=value.to_string();let mut result=Vec::new();let mut i=0;
    while i<template.len(){
        if template[i..].starts_with(b"%d"){result.extend_from_slice(replacement.as_bytes());i+=2;}
        else{result.push(template[i]);i+=1;}
    }
    result
}

fn gender_bytes(bytes:&mut [u8],pairs:&[Vec<Vec<u8>>]){
    // 0044da3b advances by one byte, including over the trail byte of a glyph.
    for i in 0..bytes.len().saturating_sub(1){
        for pair in pairs{if pair.len()==2&&pair[0].len()==2&&pair[1].len()==2&&bytes[i..i+2]==pair[0]{
            bytes[i..i+2].copy_from_slice(&pair[1]);break;
        }}
    }
}

impl EndingSummary {
    pub fn new(victory:bool,sex:usize,stage:i16,career:i16,history:&[Vec<i32>;16],rules:&EndingSummaryRules)->Result<Self,String>{
        let career=match stage{5=>25,6 if (0..26).contains(&career)=>career as usize,_=>return Err("Original ending summary requires adult or working stage and a valid career".into())};
        if sex>1||rules.paragraphs.len()!=16||rules.encoded_paragraphs.len()!=16{return Err("Invalid ending summary rules or character sex".into());}
        let mut paragraphs=Vec::new();let mut y=0;
        let add=|text:String,bytes:Vec<u8>,y:i32,paragraphs:&mut Vec<SummaryParagraph>|->i32{
            let height=bytes.len().div_ceil(32)as i32*rules.line_height;
            let mut wrapped=Vec::with_capacity(bytes.len()+bytes.len()/32);
            for(i,byte)in bytes.into_iter().enumerate(){if i>0&&i%32==0{wrapped.push(b'\n');}wrapped.push(byte);}
            paragraphs.push(SummaryParagraph{text,bytes:wrapped,x:rules.text_x,y,height});y+height+14
        };
        y=add(rules.header.clone(),rules.encoded_header.clone(),y,&mut paragraphs);
        for(category,values)in history.iter().enumerate(){
            for(index,&value)in values.iter().enumerate(){
                let row=index.min(rules.paragraphs[category].len().checked_sub(1).ok_or("Missing history paragraph template")?);
                let mut text=rules.paragraphs[category][row].replace("%d",&value.to_string());
                let mut bytes=substitute_bytes(&rules.encoded_paragraphs[category][row],value);
                if sex==1{
                    for pair in &rules.gender_replacements{text=text.replace(&pair[0],&pair[1]);}
                    gender_bytes(&mut bytes,&rules.encoded_gender_replacements);
                }
                y=add(text,bytes,y,&mut paragraphs);
                if category==7{break;}
            }
        }
        y+=28;
        add(rules.conclusions[usize::from(victory)][career].clone(),rules.encoded_conclusions[usize::from(victory)][career].clone(),y,&mut paragraphs);
        let allocated_paragraphs=history.iter().enumerate().map(|(i,v)|if i==7{1}else{v.len()}).sum::<usize>()+2;
        let end_bottom=if paragraphs.len()==allocated_paragraphs{let last=paragraphs.last().unwrap();last.y+last.height}else{0};
        let base_palette=rules.palettes.get(sex).and_then(|r|r.get(career)).filter(|p|p.len()==256).ok_or("Missing ending palette")?.clone();
        let mut alternate_palette=base_palette.clone();for color in &mut alternate_palette{color[..3].fill(0);}
        Ok(Self{finished:false,clock_ms:0,sex,career,victory,background:rules.backgrounds[sex][career].clone(),portrait:rules.portraits[sex][career].clone(),
            actor:rules.actors[sex].clone(),paragraphs,clip:rules.clip,scroll_y:rules.initial_y,actor_y:rules.initial_y,
            show_text:true,show_actor:false,scrolling:true,actor_entering:false,waiting:false,graying:false,gray_pause:false,
            fading_in:true,flashing:false,flash_dirty:false,flash_alternate:false,speed:-1,last_scroll_ms:0,last_flash_ms:0,fade_started_ms:0,
            original_palette:base_palette.clone(),palette:alternate_palette.clone(),base_palette,alternate_palette,allocated_paragraphs,end_bottom,
            skip:false,s_down:false,s_up:false,escape_released:false,initial_y:rules.initial_y})
    }

    /// Mouse skipping is independent of keyboard input. S uses down/up edges;
    /// Escape key-up goes through input_escape to replace the prior key event.
    pub fn input(&mut self,skip:bool,s_down:bool,s_up:bool){
        self.skip|=skip;
        // Native input polling replaces its buffer, and the S helpers inspect
        // only the final keyboard event rather than accumulating edge flags.
        if s_down||s_up{self.s_down=s_down&&!s_up;self.s_up=s_up;self.escape_released=false;}
    }
    pub fn input_escape(&mut self){self.escape_released=true;self.s_down=false;self.s_up=false;}

    pub fn tick(&mut self,elapsed_ms:u32){
        if self.finished{return;}
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);let now=self.clock_ms;
        let mouse_skip=std::mem::take(&mut self.skip);let escape=std::mem::take(&mut self.escape_released);
        if (mouse_skip||escape)&&!self.graying&&!self.gray_pause{
            self.scrolling=false;self.flashing=false;self.graying=true;self.last_scroll_ms=now;
        }
        let s_down=std::mem::take(&mut self.s_down);let s_up=std::mem::take(&mut self.s_up);
        if self.speed==-1{if s_down{self.speed=-40;}}
        else if s_up{self.speed=-1;}
        if self.show_text{
            let bottom=self.end_bottom.wrapping_add(self.scroll_y);
            if !self.actor_entering&&bottom.wrapping_add(120)<443{self.actor_entering=true;self.actor_y=self.initial_y;}
            if bottom<19{self.show_text=false;}
        }
        if self.actor_entering{
            self.actor.tick(now);
            if self.scrolling&&self.actor_y<141{
                self.actor_entering=false;self.scrolling=false;self.waiting=true;self.last_scroll_ms=now;
            }
        }
        if self.scrolling&&now.wrapping_sub(self.last_scroll_ms)>40{
            self.last_scroll_ms=now;
            if self.show_text{self.scroll_y=self.scroll_y.wrapping_add(self.speed);}
            if self.actor_entering{self.actor_y=self.actor_y.wrapping_add(self.speed);}
        }
        if !self.fading_in{
            if self.flashing&&now.wrapping_sub(self.last_flash_ms)>150{
                self.last_flash_ms=now;self.flash_alternate=!self.flash_alternate;self.flash_dirty=true;
            }
        }else{
            let elapsed=now.wrapping_sub(self.fade_started_ms);
            if elapsed<1800{
                for(base,alternate)in self.base_palette.iter().zip(&mut self.alternate_palette){
                    for channel in 0..3{alternate[channel]=(u32::from(base[channel])*elapsed/1800)as u8;}
                }
            }else{
                for(index,(base,alternate))in self.base_palette.iter().zip(&mut self.alternate_palette).enumerate(){
                    *alternate=*base;if(206..=245).contains(&index){for channel in 0..3{alternate[channel]=(u32::from(base[channel])*247/255)as u8;}}
                }
                self.last_flash_ms=now;self.fading_in=false;self.flashing=true;
            }
        }
        if self.waiting{
            self.actor.tick(now);
            if now.wrapping_sub(self.last_scroll_ms)>1800{
                self.flashing=false;self.waiting=false;self.graying=true;self.last_scroll_ms=now;
            }
        }
        if self.graying{
            self.actor.tick(now);let elapsed=now.wrapping_sub(self.last_scroll_ms);
            if elapsed<3000{
                // Native x87 multiplies by the exact float32 constant at 0046b310.
                let factor=f64::from(f32::from_bits(0x39aec33e));
                for(base,alternate)in self.base_palette.iter().zip(&mut self.alternate_palette){
                    let gray=(u32::from(base[0])+u32::from(base[1])+u32::from(base[2]))/3;
                    for channel in 0..3{alternate[channel]=((gray as f64-f64::from(base[channel]))*f64::from(elapsed)*factor+f64::from(base[channel]))as u8;}
                }
            }else{
                for color in &mut self.base_palette{let gray=((u32::from(color[0])+u32::from(color[1])+u32::from(color[2]))/3)as u8;color[..3].fill(gray);}
                self.palette.clone_from(&self.base_palette);self.graying=false;self.gray_pause=true;self.last_scroll_ms=now;
            }
        }
        if self.gray_pause{
            self.actor.tick(now);
            if now.wrapping_sub(self.last_scroll_ms)>1500{self.gray_pause=false;self.finished=true;self.last_scroll_ms=now;}
        }
        self.actor.y=self.actor_y;
        self.show_actor=self.actor_entering||self.waiting||((self.graying||self.gray_pause||self.finished)&&self.actor_y<443);
        // 0044e7a0 does not render the update that first returns false.
        if self.finished{return;}
        if self.fading_in{self.palette.clone_from(&self.alternate_palette);}
        else if self.flashing&&self.flash_dirty{
            self.palette.clone_from(if self.flash_alternate{&self.alternate_palette}else{&self.base_palette});self.flash_dirty=false;
        }
        if self.graying{self.palette.clone_from(&self.alternate_palette);}
    }
}

#[cfg(test)]
mod tests{
    use super::*;use serde_json::Value;
    fn rules()->EndingSummaryRules{let raw:Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();serde_json::from_value(raw["endingSummary"].clone()).unwrap()}
    fn history()->[Vec<i32>;16]{let mut h=std::array::from_fn(|_|Vec::new());h[1]=vec![2016];h[2]=vec![2019];h[7]=vec![2030,2034];h[14]=vec![15];h[15]=vec![2060];h}
    #[test]fn original_history_layout_gender_and_first_promotion(){
        let rules=rules();let mut h=history();h[11]=vec![2023];
        let state=EndingSummary::new(true,1,5,-1,&h,&rules).unwrap();assert_eq!(state.career,25);
        assert_eq!(state.paragraphs.len(),h.iter().map(Vec::len).sum::<usize>()+1);
        assert!(state.paragraphs.iter().any(|p|p.text.contains("男孩")));assert!(!state.paragraphs.iter().any(|p|p.text.contains("2034")));
        for p in &state.paragraphs{assert_eq!(p.bytes.split(|&b|b==10).next().unwrap().len().min(32),p.bytes.len().min(32));}
        h[7].clear();let empty=EndingSummary::new(false,0,6,3,&h,&rules).unwrap();assert_eq!(empty.allocated_paragraphs,empty.paragraphs.len()+1);assert_eq!(empty.end_bottom,0);
    }
    #[test]fn saved_skip_fade_and_acceleration_keep_original_clocks(){
        let rules=rules();let mut game=EndingSummary::new(false,0,6,2,&history(),&rules).unwrap();game.input(false,true,false);game.tick(40);assert_eq!(game.scroll_y,463);
        game.tick(1);assert_eq!(game.scroll_y,423);game.input(false,false,true);game.tick(41);assert_eq!(game.scroll_y,422);
        game.input(true,false,false);let json=serde_json::to_string(&game).unwrap();let mut restored:EndingSummary=serde_json::from_str(&json).unwrap();
        for elapsed in [1,1799,1200,1,1500,1]{game.tick(elapsed);restored.tick(elapsed);assert_eq!(restored,game);}
        assert!(game.finished);assert!(game.palette.iter().all(|c|c[0]==c[1]&&c[1]==c[2]));
    }
    #[test]fn rapid_s_release_press_release_does_not_leave_acceleration_queued(){
        let rules=rules();let mut game=EndingSummary::new(false,0,6,2,&history(),&rules).unwrap();
        game.input(false,true,false);game.tick(41);assert_eq!(game.speed,-40);
        game.input(false,false,true);game.input(false,true,false);game.input(false,false,true);
        let save=serde_json::to_string(&game).unwrap();game=serde_json::from_str(&save).unwrap();
        for _ in 0..3{game.tick(41);assert_eq!(game.speed,-1);assert!(!game.s_down&&!game.s_up);}
        game.input(false,true,false);game.tick(41);game.input(false,true,false);game.tick(41);assert!(!game.s_down);
        game.input(false,false,true);game.tick(41);game.tick(41);assert_eq!(game.speed,-1);
        game.input_escape();game.input(false,true,false);game.tick(41);assert!(!game.graying);assert_eq!(game.speed,-40);
        game.input(true,false,false);game.input(false,false,true);game.tick(41);assert!(game.graying);assert_eq!(game.speed,-1);
    }
    fn palette_hash(palette:&[[u8;4]])->u32{palette.iter().flatten().fold(2166136261u32,|hash,&byte|(hash^u32::from(byte)).wrapping_mul(16777619))}
    fn check_layout(case:&Value,rules:&EndingSummaryRules)->EndingSummary{
        let history:[Vec<i32>;16]=serde_json::from_value(case["history"].clone()).unwrap();
        let game=EndingSummary::new(case["victory"].as_bool().unwrap(),case["sex"].as_u64().unwrap()as usize,6,case["career"].as_i64().unwrap()as i16,&history,rules).unwrap();
        let expected=case["paragraphs"].as_array().unwrap();assert_eq!(game.allocated_paragraphs,expected.len());
        for(index,paragraph)in game.paragraphs.iter().enumerate(){
            assert_eq!(serde_json::json!({"bytes":paragraph.bytes,"x":paragraph.x,"y":paragraph.y,"height":paragraph.height}),expected[index],"summary paragraph {index}, sex {}, career {}, victory {}",game.sex,game.career,game.victory);
        }
        if game.paragraphs.len()<expected.len(){assert_eq!(expected.last().unwrap(),&serde_json::json!({"bytes":[],"x":0,"y":0,"height":0}));}
        game
    }
    #[test]fn history_and_frames_match_original_x86(){
        let fixture:Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-ending-summary.json")).unwrap();let rules=rules();
        for case in fixture["layouts"].as_array().unwrap(){check_layout(case,&rules);}
        for(case_index,case)in fixture["cases"].as_array().unwrap().iter().enumerate(){
            let mut game=check_layout(case,&rules);
            for(frame_index,frame)in case["frames"].as_array().unwrap().iter().enumerate(){
                game.input(frame["skip"].as_bool().unwrap(),frame["sDown"].as_bool().unwrap(),frame["sUp"].as_bool().unwrap());game.tick(case["step"].as_u64().unwrap()as u32);
                let actual=serde_json::json!({"scrollY":game.scroll_y,"actorY":game.actor_y,"lastScrollMs":game.last_scroll_ms,"speed":game.speed,"lastFlashMs":game.last_flash_ms,"fadeStartedMs":game.fade_started_ms,
                    "scrolling":game.scrolling,"showText":game.show_text,"actorEntering":game.actor_entering,"flashing":game.flashing,"flashDirty":game.flash_dirty,"flashAlternate":game.flash_alternate,
                    "waiting":game.waiting,"graying":game.graying,"grayPause":game.gray_pause,"finished":game.finished,"fadingIn":game.fading_in,
                    "actor":[game.actor.frame,i64::from(game.actor.last_tick),i64::from(game.actor.initial_frame)],"paletteHash":palette_hash(&game.palette),
                    "basePaletteHash":palette_hash(&game.base_palette),"alternatePaletteHash":palette_hash(&game.alternate_palette)});
                for(field_index,field)in fixture["fields"].as_array().unwrap().iter().enumerate(){
                    let field=field.as_str().unwrap();assert_eq!(actual[field],frame["after"][field_index],"original summary case {case_index} frame {frame_index} {field}");
                }
            }
        }
    }
}
