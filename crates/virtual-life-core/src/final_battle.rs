//! Final encounter, 0x00443b40. Cards, valuation and attacks keep the original
//! signed-word arithmetic, CRT draws and one-update-per-render-frame timing.
use crate::rng::ReplayRng;
use serde::{Deserialize,Serialize};
use serde_json::Value;

#[derive(Clone,Debug,Deserialize,Serialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct FinalAnimation {pub asset_path:String,pub frame_count:u32,pub frame_duration_ms:u32}
#[derive(Clone,Debug,Deserialize,Serialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct FinalBattleRules {
    pub career_hp_multipliers:Vec<i32>,pub career_power:Vec<i16>,pub enemy_stats:Vec<[i16;4]>,
    pub card_kinds:Vec<[i16;6]>,pub combo_multipliers:Vec<i16>,pub item_prices:Vec<i32>,
    pub enemy_animations:Vec<FinalAnimation>,pub player_animations:Vec<FinalAnimation>,
    pub intro_animations:Vec<FinalAnimation>,pub resources_by_address:Value,#[serde(default)]pub runtime_objects:Value,
    pub card_positions:Vec<[i16;2]>,pub shop_card_positions:Vec<[i16;2]>,
    pub shop_texts:std::collections::BTreeMap<String,String>,#[serde(default)]pub intro_texts:Vec<String>,
    #[serde(default)]pub opening_duration_ms:u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules()->FinalBattleRules{let value:Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();serde_json::from_value(value["finalBattle"].clone()).unwrap()}
    fn vectors()->Value{serde_json::from_str(include_str!("../../../tests/fixtures/original-final-battle.json")).unwrap()}
    fn basic(rules:&FinalBattleRules)->FinalBattleGame{FinalBattleGame::new(FinalBattleInput{sex:0,career:-1,life_level:1,equipped_count:0,valuation:[0;14]},rules).unwrap()}
    fn cards(value:&Value)->Vec<FinalCard>{value.as_array().unwrap().iter().enumerate().map(|(index,card)|FinalCard{kind:card["kind"].as_u64().unwrap()as u8,attack:card["attack"].as_i64().unwrap()as i32,defense:card["defense"].as_i64().unwrap()as i32,selected:card["selected"].as_bool().unwrap(),x:card["x"].as_i64().unwrap_or(29+65*index as i64)as i16}).collect()}
    #[test]
    fn initialization_combinations_and_ten_items_match_original_x86(){
        let rules=rules();let vectors=vectors();
        for row in vectors["initialization"].as_array().unwrap(){let game=FinalBattleGame::new(FinalBattleInput{sex:0,career:row["career"].as_i64().unwrap()as i16,life_level:row["lifeLevel"].as_i64().unwrap()as i16,equipped_count:row["equippedCount"].as_i64().unwrap()as i16,valuation:[0;14]},&rules).unwrap();assert_eq!(i64::from(game.hp),row["hp"].as_i64().unwrap());assert_eq!(i64::from(game.power),row["power"].as_i64().unwrap());}
        for row in vectors["combinations"].as_array().unwrap(){let mut game=basic(&rules);game.cards=cards(&row["cards"]);game.power=row["power"].as_i64().unwrap()as i16;game.shield=row["shield"].as_u64().unwrap()as u8;game.recompute_power(&rules);assert_eq!(i64::from(game.attack),row["attack"].as_i64().unwrap());assert_eq!(i64::from(game.defense),row["defense"].as_i64().unwrap());assert_eq!(u64::from(game.shield),row["afterShield"].as_u64().unwrap());}
        for row in vectors["items"].as_array().unwrap(){let mut game=basic(&rules);game.cards=cards(&row["cards"]);let used=game.apply_item(row["item"].as_u64().unwrap()as u8);assert_eq!(used,row["used"].as_bool().unwrap());assert_eq!(game.selection_changed,row["selectionChanged"].as_bool().unwrap());assert_eq!(serde_json::to_value(&game.cards).unwrap(),row["after"]);}
    }
    #[test]
    fn combat_loop_matches_original_x86_frames_and_rng(){
        let rules=rules();let vectors=vectors();
        for(case,row)in vectors["sequences"].as_array().unwrap().iter().enumerate(){
            let mut game=basic(&rules);game.phase=FinalBattlePhase::Playing;game.hp=30000;game.displayed_hp=30000;game.power=20;
            let mut value=serde_json::to_value(&game).unwrap();for(key,initial)in row["initial"].as_object().unwrap(){value[key]=initial.clone();}game=serde_json::from_value(value).unwrap();
            let mut rng=ReplayRng::new(row["seed"].as_u64().unwrap());
            for(frame,input)in row["frames"].as_array().unwrap().iter().enumerate(){
                if let Some(action)=input["action"].as_str(){game.action(action,input["index"].as_u64().map(|n|n as usize),&rules).unwrap_or_else(|error|panic!("case {case} frame {frame}: {error}"));}
                game.tick(input["elapsedMs"].as_u64().unwrap()as u32,&rules,&mut rng).unwrap();
                let actual=serde_json::to_value(&game).unwrap();
                for(key,expected)in input["after"].as_object().unwrap(){if key=="rngState"{assert_eq!(u64::from(rng.state),expected.as_u64().unwrap(),"case {case} frame {frame} rng");}else if key=="rngDraws"{assert_eq!(rng.draws,expected.as_u64().unwrap(),"case {case} frame {frame} draws");}else{assert_eq!(&actual[key],expected,"case {case} frame {frame} field {key}");}}
            }
        }
    }
}
impl FinalBattleRules {
    fn resource(&self,address:&str)->Result<FinalAnimation,String>{serde_json::from_value(self.resources_by_address[address].clone()).map_err(|_|format!("Missing final-battle animation {address}"))}
    pub fn validate(&self)->Result<(),String>{
        if self.career_hp_multipliers.len()!=26 || self.career_power.len()!=26 || self.enemy_stats.len()!=10 || self.card_kinds.len()!=4 || self.combo_multipliers.len()!=6 || self.item_prices.len()!=10 || self.enemy_animations.len()!=25 || self.player_animations.len()!=13 || self.intro_animations.len()!=2 {return Err("Incomplete original final-battle tables".into());}
        if self.enemy_stats.iter().any(|row|row[1]<=0||row[3]<=0)||self.card_kinds.iter().any(|row|row[1]<row[0]||row[3]<row[2]){return Err("Invalid original final-battle random ranges".into());} Ok(())
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FinalBattleInput {
    #[serde(default)]pub sex:usize,pub career:i16,pub life_level:i16,pub equipped_count:i16,
    /// The fourteen rows from 0x00441860, before converting the first twelve to
    /// money. Rows 12 and 13 instead grant the special gold cards.
    pub valuation:[i32;14],
}
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum FinalBattlePhase {Intro,Shop,Opening,Playing,Finished}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FinalTrack {pub frame:u32,pub last_tick:u32,pub first:bool}
impl Default for FinalTrack {fn default()->Self{Self{frame:0,last_tick:0,first:true}}}
impl FinalTrack {
    fn tick(&mut self,now:u32,count:u32,period:u32){if now.wrapping_sub(self.last_tick)>period {self.last_tick=now;if self.first{self.first=false;}else{self.frame=(self.frame+1)%count;}}}
    fn reset(&mut self){self.frame=0;}
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FinalCard {pub kind:u8,pub attack:i32,pub defense:i32,pub selected:bool,pub x:i16}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FinalShopAnimation {pub item:u8,pub phase:u8,pub x:i16,pub y:i16,pub original_x:i16,pub original_y:i16}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FinalLayer {pub asset_path:String,pub frame:u32,pub x:i32,pub y:i32,pub transparent_index:Option<u8>,pub first_pixel_transparent:bool}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct FinalBattleGame {
    #[serde(default)]pub sex:usize,
    pub phase:FinalBattlePhase,pub clock_ms:u32,pub intro_step:u8,pub intro_started_ms:u32,pub intro_duration_ms:u32,
    pub valuation:[i32;14],pub money:i32,pub hp:i32,pub enemy_hp:i32,pub power:i16,pub items:Vec<u8>,
    pub cards:Vec<FinalCard>,pub cards_moving:bool,pub selected_item:Option<usize>,pub active_item:i8,pub item_used:bool,pub selection_changed:bool,
    pub attack:i16,pub defense:i16,pub enemy_defense:i16,pub shield:u8,pub round:i16,
    /// Original state byte 4e6714: idle 0, heal 1, attack 2, enemy windup 3,
    /// enemy hit 10, victory 11, defeat 12.
    pub combat_state:u8,pub enemy_animation:usize,pub player_animation:usize,
    pub displayed_hp:i32,pub displayed_enemy_hp:i32,pub hp_delta:i16,pub enemy_hp_delta:i16,pub hp_text_y:i16,pub enemy_text_y:i16,
    pub enemy_tracks:Vec<FinalTrack>,pub player_tracks:Vec<FinalTrack>,pub intro_tracks:[FinalTrack;2],pub recovery_track:FinalTrack,pub shield_tracks:[FinalTrack;2],
    pub shop_animation:Option<FinalShopAnimation>,pub shop_panel_y:i32,pub shop_message:String,pub item_scroll:usize,
    pub shop_selected_buy:Option<usize>,pub shop_selected_sell:Option<usize>,
    pub opening_started_ms:u32,pub attack_serial:u32,pub sound_serial:u32,pub sounds:Vec<String>,#[serde(default)]pub sound_stops:Vec<String>,
    #[serde(default)]pub layers:Vec<FinalLayer>,#[serde(default)]pub background_track:FinalTrack,
    #[serde(default="default_right_tracks")]pub right_tracks:Vec<FinalTrack>,#[serde(default="default_left_tracks")]pub left_tracks:Vec<FinalTrack>,
    pub finished:bool,
    #[serde(default)]pub intro_display_step:u8,#[serde(default)]pub intro_message:String,#[serde(default)]pub intro_displayed_message:String,
    #[serde(default)]pub shop_displayed_panel_y:i32,#[serde(default)]pub shop_displayed_item_count:usize,
    #[serde(default)]pub shop_visible_items:Vec<u8>,#[serde(default)]shop_item_pool:Vec<u8>,
    #[serde(default)]pending_shop:Option<(String,Option<usize>)>,
    #[serde(default)]pending_card:Option<usize>,#[serde(default)]pending_use:bool,#[serde(default)]pending_attack:bool,
    #[serde(default)]pending_continue:bool,#[serde(default)]pending_item:Option<usize>,
}
fn default_right_tracks()->Vec<FinalTrack>{vec![FinalTrack::default();9]}
fn default_left_tracks()->Vec<FinalTrack>{vec![FinalTrack::default();10]}
impl FinalBattleGame {
    pub fn new(input:FinalBattleInput,rules:&FinalBattleRules)->Result<Self,String>{
        rules.validate()?;
        let index=usize::try_from(input.career+1).map_err(|_|"Invalid final-battle career")?;
        let power=rules.career_power.get(index).copied().ok_or("Invalid final-battle career")?.wrapping_add(input.equipped_count.wrapping_mul(2));
        let hp=if input.career==-1 {i32::from(input.life_level).wrapping_mul(360).wrapping_add(1640)}else{i32::from(input.life_level).wrapping_mul(18).wrapping_add(82).wrapping_mul(rules.career_hp_multipliers[index]).wrapping_mul(20)/10};
        let free=input.valuation[12].wrapping_add(input.valuation[13]);
        if !(0..=36).contains(&free){return Err("原版最终战的奖励卡片从第37张起会覆写护盾动画对象指针，无法确定原程序的后续行为。进度已保留。".into());}
        Ok(Self{sex:input.sex,phase:FinalBattlePhase::Intro,clock_ms:0,intro_step:0,intro_started_ms:0,intro_duration_ms:10000,
            valuation:input.valuation,money:0,hp,enemy_hp:30000,power,items:vec![],cards:vec![],cards_moving:false,selected_item:None,active_item:-1,item_used:false,selection_changed:false,
            attack:0,defense:0,enemy_defense:0,shield:0,round:0,combat_state:0,enemy_animation:0,player_animation:0,displayed_hp:hp,displayed_enemy_hp:30000,hp_delta:0,enemy_hp_delta:0,hp_text_y:300,enemy_text_y:0,
            enemy_tracks:vec![FinalTrack::default();25],player_tracks:vec![FinalTrack::default();13],intro_tracks:[FinalTrack::default(),FinalTrack::default()],recovery_track:FinalTrack::default(),shield_tracks:[FinalTrack::default(),FinalTrack::default()],
            layers:vec![],background_track:FinalTrack::default(),right_tracks:default_right_tracks(),left_tracks:default_left_tracks(),
            shop_animation:None,shop_panel_y:0,shop_message:rules.shop_texts.get("0x00495c70").cloned().unwrap_or_default(),item_scroll:0,shop_selected_buy:None,shop_selected_sell:None,opening_started_ms:0,attack_serial:0,sound_serial:0,sounds:vec![],sound_stops:vec![],finished:false,pending_card:None,pending_use:false,pending_attack:false,pending_continue:false,pending_item:None,
            intro_display_step:0,intro_message:rules.intro_texts.first().cloned().unwrap_or_default(),intro_displayed_message:String::new(),shop_displayed_panel_y:0,shop_displayed_item_count:0,shop_visible_items:vec![],shop_item_pool:vec![],pending_shop:None})
    }
    pub fn action(&mut self,action:&str,index:Option<usize>,_rules:&FinalBattleRules)->Result<(),String>{
        if self.phase==FinalBattlePhase::Shop && matches!(action,"inspectBuy"|"inspectSell"|"cancelShop"|"buy"|"sell"|"scrollLeft"|"scrollRight") {
            if self.shop_animation.is_some() && !matches!(action,"scrollLeft"|"scrollRight") {return Err("请等待卡片移动结束。".into());}
            if matches!(action,"inspectBuy"|"buy") && index.is_none_or(|id|id>=9) {return Err("请选择出售中的卡片。".into());}
            if matches!(action,"inspectSell"|"sell") && index.is_none_or(|id|id>=self.items.len()||id<self.item_scroll||id>=self.item_scroll+10) {return Err("请选择持有的卡片。".into());}
            self.pending_shop=Some((action.into(),index));return Ok(());
        }
        match action {
            "continue"|"start"=>match self.phase{FinalBattlePhase::Intro|FinalBattlePhase::Shop|FinalBattlePhase::Opening=>self.pending_continue=true,_=>return Err("最终战当前不需要继续。".into())},
            "selectCard"=>{if self.phase!=FinalBattlePhase::Playing||self.combat_state!=0||self.cards_moving{return Err("请等待卡片或攻击动画结束。".into());}let index=index.ok_or("请选择卡片")?;if index>=self.cards.len(){return Err("卡片不存在。".into());}self.pending_card=Some(index);},
            "selectItem"=>{if self.phase!=FinalBattlePhase::Playing||self.item_used{return Err("当前无法选择道具。".into());}let index=index.ok_or("请选择道具")?;if index>=self.items.len(){return Err("道具不存在。".into());}self.pending_item=Some(index);},
            "useItem"=>{if self.phase!=FinalBattlePhase::Playing||self.combat_state!=0||self.item_used||self.selected_item.is_none(){return Err("请先选择可用的道具。".into());}self.pending_use=true;},
            "attack"=>{if self.phase!=FinalBattlePhase::Playing||self.combat_state!=0{return Err("请等待当前攻击结束。".into());}self.pending_attack=true;},
            "scrollLeft"=>{self.item_scroll=self.item_scroll.saturating_sub(1);},
            "scrollRight"=>{let visible=if self.phase==FinalBattlePhase::Shop{10}else{16};if self.item_scroll+visible<self.items.len(){self.item_scroll+=1;}},
            _=>return Err("未知的最终战操作。".into()),
        } Ok(())
    }
    pub fn tick(&mut self,elapsed_ms:u32,rules:&FinalBattleRules,rng:&mut ReplayRng)->Result<(),String>{
        if self.finished{return Ok(());}self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);self.sounds.clear();self.sound_stops.clear();self.layers.clear();
        match self.phase {
            FinalBattlePhase::Intro=>self.tick_intro(rules),
            FinalBattlePhase::Shop=>self.tick_shop(rules),
            FinalBattlePhase::Opening=>{if self.pending_continue||self.clock_ms.wrapping_sub(self.opening_started_ms)>=rules.opening_duration_ms {self.pending_continue=false;self.phase=FinalBattlePhase::Playing;self.item_scroll=0;self.selected_item=None;self.active_item=-1;self.set_item_used(false);}},
            FinalBattlePhase::Playing=>self.tick_combat(rules,rng)?,
            FinalBattlePhase::Finished=>{},
        } Ok(())
    }
    fn tick_intro(&mut self,rules:&FinalBattleRules){
        self.intro_displayed_message.clear();
        if self.intro_step==0 {
            let meta=&rules.intro_animations[0];self.intro_tracks[0].tick(self.clock_ms,meta.frame_count,meta.frame_duration_ms);
            self.object_layer(0x4e6484,self.intro_tracks[0].frame,false,rules);
            if self.intro_tracks[0].frame+1==meta.frame_count{self.intro_step=1;self.intro_started_ms=self.clock_ms;self.intro_duration_ms=20000;}
        }
        self.intro_display_step=self.intro_step;
        if self.intro_step!=0 {
            let meta=&rules.intro_animations[1];self.intro_tracks[1].tick(self.clock_ms,meta.frame_count,meta.frame_duration_ms);
            self.object_layer(0x4e6480,self.intro_tracks[1].frame,false,rules);
            self.intro_displayed_message=self.intro_message.clone();
            // Native uses an unsigned deadline comparison, including at wrap.
            let expired=|game:&Self|game.clock_ms>=game.intro_started_ms.wrapping_add(game.intro_duration_ms);
            if self.intro_step==1 {
                if expired(self){self.intro_step=2;self.intro_started_ms=self.clock_ms;self.intro_duration_ms=10000;self.intro_message=rules.intro_texts.get(1).cloned().unwrap_or_default();}
            }else{
                if self.intro_step==2 && expired(self){self.intro_step=3;self.intro_started_ms=self.clock_ms;self.intro_duration_ms=20000;self.intro_message=rules.intro_texts.get(2).cloned().unwrap_or_default();}
                if self.intro_step==3 && expired(self){self.intro_step=4;self.intro_started_ms=self.clock_ms;self.intro_duration_ms=10000;self.intro_message=rules.intro_texts.get(3).cloned().unwrap_or_default();self.money=0;for value in self.valuation.iter().take(12){self.money=self.money.wrapping_add(*value).min(99_999_999);}}
                if self.intro_step==4 && expired(self){self.intro_step=5;self.intro_started_ms=self.clock_ms;self.intro_duration_ms=20000;self.intro_message=rules.intro_texts.get(3).cloned().unwrap_or_default();}
                if self.intro_step==5 && expired(self){self.phase=FinalBattlePhase::Shop;self.items=vec![9;self.valuation[12].wrapping_add(self.valuation[13])as usize];self.shop_item_pool.clear();}
            }
        }
        if self.pending_continue{self.intro_duration_ms=0;self.pending_continue=false;}
    }
    fn tick_shop(&mut self,rules:&FinalBattleRules){
        if self.shop_item_pool.is_empty(){self.shop_item_pool=self.items.clone();self.shop_item_pool.resize(self.items.len().max(32),255);}
        let command=self.pending_shop.take();let action=command.as_ref().map(|value|value.0.as_str());let index=command.as_ref().and_then(|value|value.1);
        self.shop_displayed_panel_y=self.shop_panel_y;self.shop_displayed_item_count=self.items.len();
        self.shop_panel_y=(self.shop_panel_y+1).min(9);let meta=&rules.intro_animations[1];self.intro_tracks[1].tick(self.clock_ms,meta.frame_count,meta.frame_duration_ms);
        self.object_layer(0x4e6480,self.intro_tracks[1].frame,false,rules);
        if action==Some("scrollLeft") && self.item_scroll>0{self.item_scroll-=1;self.shop_selected_sell=None;}
        if action==Some("scrollRight") && self.item_scroll+10<self.items.len(){self.item_scroll+=1;self.shop_selected_sell=None;}
        let mut moved_position=None;
        if let Some(mut movement)=self.shop_animation.take(){
            if movement.phase==1 {
                self.shop_layer(movement.item,0,movement.x,movement.y,rules);moved_position=Some((movement.item,movement.x,movement.y));
                if movement.x-40<0 {movement.x=0;movement.phase=2;self.money=self.money.wrapping_sub(rules.item_prices[movement.item as usize]);self.shop_item_pool[self.items.len()]=movement.item;self.items.push(movement.item);movement.y=0;}else{movement.x-=40;}
            }
            if movement.phase==2 {
                self.shop_layer(movement.item,0,movement.original_x,movement.y,rules);moved_position=Some((movement.item,movement.original_x,movement.y));
                if movement.y+40<movement.original_y{movement.y+=40;}else{movement.y=movement.original_y;if self.items.len()>10{self.item_scroll=self.items.len()-10;}movement.phase=0;self.shop_selected_buy=None;self.shop_message=rules.shop_texts.get("0x00495ca0").cloned().unwrap_or_default();}
            }
            if movement.phase!=0{self.shop_animation=Some(movement);}
        }
        if self.shop_animation.is_none(){
            if let Some(id)=self.shop_selected_buy{
                if action==Some("cancelShop"){self.shop_selected_buy=None;self.shop_message=rules.shop_texts.get("0x00495ca0").cloned().unwrap_or_default();}
                if action==Some("buy") && self.items.len()<31 {
                    if self.money>rules.item_prices[id]{let position=rules.shop_card_positions[id];self.shop_animation=Some(FinalShopAnimation{item:id as u8,phase:1,x:position[0],y:position[1],original_x:position[0],original_y:position[1]});}
                    else{self.shop_selected_buy=None;self.shop_message="这种装备对你来说太贵啦！".into();}
                }
            }
        }
        if let Some(id)=self.shop_selected_sell{
            self.shop_message=rules.shop_texts.get("0x00495c9c").cloned().unwrap_or_default();
            if action==Some("cancelShop"){self.shop_selected_sell=None;self.shop_message=rules.shop_texts.get("0x00495ca0").cloned().unwrap_or_default();}
            if action==Some("sell") && id<self.items.len(){
                self.money=self.money.wrapping_add(rules.item_prices[self.items[id]as usize]);self.items.remove(id);
                for i in id..self.items.len(){self.shop_item_pool[i]=self.shop_item_pool[i+1];}
                self.shop_selected_sell=None;self.shop_message=rules.shop_texts.get("0x00495ca0").cloned().unwrap_or_default();
            }
        }
        for id in 0..9{
            let position=rules.shop_card_positions[id];let mut drawn=position;
            let moving=self.shop_animation.as_ref().is_some_and(|movement|movement.item as usize==id);
            if moving{if let Some((item,x,y))=moved_position{if item as usize==id{drawn=[x,y];}}}
            let selected=self.shop_animation.is_none() && (self.shop_selected_buy==Some(id)||(action==Some("inspectBuy")&&index==Some(id)));
            if selected{self.shop_selected_buy=Some(id);self.shop_selected_sell=None;self.shop_message=rules.shop_texts.get(&format!("0x{:08x}",0x495c74+id*4)).cloned().unwrap_or_default();}
            self.shop_layer(id as u8,u32::from(selected),drawn[0],drawn[1],rules);
        }
        self.shop_visible_items.clear();
        for slot in 0..self.items.len().min(10){
            let item=self.shop_item_pool.get(self.item_scroll+slot).copied().unwrap_or(255);self.shop_visible_items.push(item);
            let id=self.item_scroll+slot;
            if self.shop_selected_sell==Some(id)||(action==Some("inspectSell")&&index==Some(id)){self.shop_selected_sell=Some(id);self.shop_selected_buy=None;}
        }
        if self.money<2000||self.pending_continue{self.pending_continue=false;self.phase=FinalBattlePhase::Opening;self.opening_started_ms=self.clock_ms;self.shop_message=rules.shop_texts.get("0x00495ca4").cloned().unwrap_or_default();}
    }
    fn shop_layer(&mut self,item:u8,frame:u32,x:i16,y:i16,rules:&FinalBattleRules){
        self.object_layer(0x4e6738+u32::from(item)*4,frame,false,rules);
        if let Some(layer)=self.layers.last_mut(){layer.x=i32::from(x);layer.y=i32::from(y);}
    }
    fn fill_cards(&mut self,rules:&FinalBattleRules,rng:&mut ReplayRng){
        if self.cards.len()<5{self.sound(0x4e67cc,rules);}
        while self.cards.len()<5 {let kind=(rng.next_rand()%4)as u8;let row=rules.card_kinds[kind as usize];let attack=(rng.next_rand()as i32)%(i32::from(row[1])-i32::from(row[0])+1)+i32::from(row[0]);let defense=(rng.next_rand()as i32)%(i32::from(row[3])-i32::from(row[2])+1)+i32::from(row[2]);self.cards.push(FinalCard{kind,attack,defense,selected:false,x:580});}
        self.cards_moving=false;
        for(index,card)in self.cards.iter_mut().enumerate(){let target=rules.card_positions[index][0];if card.x!=target{self.cards_moving=true;card.x=(card.x-80).max(target);break;}}
        if !self.cards_moving{self.stop_sound(0x4e67cc,rules);}
    }
    pub fn recompute_power(&mut self,rules:&FinalBattleRules){
        let mut count=0usize;let(mut attack,mut defense)=(0i16,0i16);
        for card in &self.cards {if card.selected {self.shield=u8::from(card.kind==3);attack=attack.wrapping_add(card.attack as i16);defense=defense.wrapping_add(card.defense as i16);count+=1;}}
        if count>1 {attack=(i32::from(attack)*i32::from(rules.combo_multipliers[count])/10)as i16;defense=(i32::from(defense)*i32::from(rules.combo_multipliers[count])/10)as i16;}
        if self.shield==1&&count>2{self.shield=2;}
        self.attack=(i32::from(attack)*i32::from(self.power)/10)as i16;self.defense=(i32::from(defense)*i32::from(self.power)/10)as i16;
    }
    /// 443880 modifies the 32-bit card fields; 443160 later sums their low words.
    pub fn apply_item(&mut self,item:u8)->bool{
        let selected=self.cards.iter().filter(|card|card.selected).count();
        let allowed=match item{0|1|8|9=>true,2|3=>selected==1,4|5=>selected==2,6|7=>selected>1,_=>false};
        if !allowed{return false;}
        for card in &mut self.cards{match item {
            2|4 if card.selected=>card.attack=card.attack.wrapping_mul(3),
            3|5 if card.selected=>card.defense=card.defense.wrapping_mul(3),
            6 if card.selected=>{card.attack=card.attack.wrapping_add(card.defense);card.defense=0;},
            7 if card.selected=>{card.defense=card.defense.wrapping_add(card.attack);card.attack=0;},
            8=>{card.kind=2;card.selected=true;},
            9=>{card.kind=0;card.attack=120;card.defense=50;card.selected=true;},_=>{}
        }}
        if item>=8{self.selection_changed=true;}self.set_item_used(true);true
    }
    // Native's 32-byte item array is immediately followed by the item-used
    // int32. Bonus cards 33..36 alias that scalar; card 37 would corrupt a
    // heap pointer and is rejected at entry instead of inventing a result.
    fn set_item_used(&mut self,value:bool){self.item_used=value;for(index,item)in self.items.iter_mut().enumerate().skip(32){*item=if index==32{u8::from(value)}else{0};}}
    fn consume_item(&mut self,index:usize){
        if self.items.len()<=32{self.items.remove(index);return;}
        let mut memory=[0u8;36];memory[..self.items.len()].copy_from_slice(&self.items);memory[32..36].copy_from_slice(&(i32::from(self.item_used)).to_le_bytes());
        for offset in index..self.items.len()-1{memory[offset]=memory[offset+1];}
        self.items=memory[..self.items.len()-1].to_vec();self.item_used=i32::from_le_bytes(memory[32..36].try_into().unwrap())==1;
    }
    fn tick_combat(&mut self,rules:&FinalBattleRules,rng:&mut ReplayRng)->Result<(),String>{
        let background=rules.resource("0x004961fc")?;self.background_track.tick(self.clock_ms,background.frame_count,background.frame_duration_ms);self.object_layer(0x4e6558,self.background_track.frame,true,rules);
        self.tick_bars();self.fill_cards(rules,rng);
        if !self.cards_moving&&self.combat_state==0 {if let Some(index)=self.pending_card.take(){self.sound(0x4e67d0,rules);let kind=self.cards[index].kind;for card in &mut self.cards{if card.selected&&card.kind!=kind{card.selected=false;}}self.cards[index].selected=true;self.selection_changed=true;}}
        if self.combat_state==0 {self.recompute_power(rules);}
        if self.pending_use&&self.combat_state==0&&!self.item_used {if let Some(index)=self.selected_item.take(){let item=self.items[index];self.active_item=item as i8;if self.apply_item(item){self.consume_item(index);if self.items.len()<=self.item_scroll+15{self.item_scroll=self.item_scroll.saturating_sub(1);}if item<2{self.sound(0x4e66fc,rules);self.combat_state=1;}}}}
        self.pending_use=false;
        if let Some(index)=self.pending_item.take(){if !self.item_used&&index<self.items.len(){self.selected_item=Some(index);self.active_item=self.items[index]as i8;}}
        if self.pending_attack&&self.combat_state==0&&self.selection_changed {
            self.selection_changed=false;let mut retained=vec![];for(index,card)in self.cards.iter().enumerate(){if !card.selected{let mut card=card.clone();card.x=rules.card_positions[index][0];retained.push(card);}}self.cards=retained;
            if !(self.item_used&&self.active_item<2){self.combat_state=2;self.attack_serial=self.attack_serial.wrapping_add(1);
                if self.item_used{self.sound(0x4e66d0,rules);self.sound(0x4e66dc+self.active_item as u32*4,rules);self.sound(0x4e6634+self.active_item as u32*4,rules);self.player_animation=(self.active_item+3).max(3)as usize;self.enemy_animation=22;self.player_tracks[self.player_animation].reset();
                    // The original resets a different right-hand object than
                    // it subsequently draws (64c0 + animation*4 vs 64b8 + animation*4).
                    let reset=self.player_animation-3;if reset<9{self.right_tracks[reset].reset();}else if reset==9{self.recovery_track.reset();}
                }
                else if self.shield==1||self.shield==2{self.right_tracks[8].reset();self.enemy_animation=0;self.shield_tracks[self.shield as usize-1].reset();}
                else{self.sound(0x4e66cc,rules);self.player_animation=(rng.next_rand()%3)as usize;self.sound(0x4e6700+self.player_animation as u32*4,rules);self.enemy_animation=21;self.player_tracks[self.player_animation].reset();}
            }
        }
        self.pending_attack=false;
        if self.combat_state==0{self.tick_enemy(0,rules);}
        else if self.combat_state==1 {
            let meta=rules.resource("0x004961f8")?;self.recovery_track.tick(self.clock_ms,meta.frame_count,0);self.object_layer(0x4e64f0,self.recovery_track.frame,false,rules);
            if self.recovery_track.frame+1==meta.frame_count{let old=self.hp;self.displayed_hp=old;self.hp=self.hp.wrapping_add(if self.active_item==0{200}else{1050}).min(60000);self.hp_delta=(self.hp as i16).wrapping_sub(old as i16);self.hp_text_y=330;self.sound(0x4e66cc,rules);self.combat_state=0;self.active_item=-1;self.set_item_used(false);}
        }else if self.combat_state==2 {
            self.tick_enemy(self.enemy_animation,rules);
            let done=if self.shield==0 {let index=self.player_animation;let meta=&rules.player_animations[index];self.player_tracks[index].tick(self.clock_ms,meta.frame_count,0);self.object_layer(0x4e6574+index as u32*4,self.player_tracks[index].frame,false,rules);if index>4{self.tick_right(index-5,rules)?;}self.player_tracks[index].frame+1==meta.frame_count}else{let index=self.shield as usize-1;self.sound(0x4e6568+index as u32*4,rules);let meta=rules.resource(if index==0{"0x00495eac"}else{"0x00495eb0"})?;self.shield_tracks[index].tick(self.clock_ms,meta.frame_count,meta.frame_duration_ms);self.object_layer(0x4e6784+index as u32*4,self.shield_tracks[index].frame,false,rules);self.tick_right(8,rules)?;self.shield_tracks[index].frame+1==meta.frame_count};
            if done {self.finish_player_attack(rules,rng);}
        }
        if self.combat_state==3&&self.displayed_enemy_hp==self.enemy_hp {
            self.tick_enemy(self.enemy_animation,rules);
            self.tick_left(self.enemy_animation-1,rules)?;
            if self.enemy_tracks[self.enemy_animation].frame+1==rules.enemy_animations[self.enemy_animation].frame_count{if self.enemy_animation>2{self.sound(0x4e67b8,rules);}self.enemy_animation+=10;self.enemy_tracks[self.enemy_animation].reset();self.sound(0x4e66d4,rules);self.combat_state=10;}
        }
        if self.combat_state==10 {
            if self.enemy_animation!=18{self.tick_enemy(self.enemy_animation,rules);}
            if self.enemy_animation==18||self.enemy_tracks[self.enemy_animation].frame+1==rules.enemy_animations[self.enemy_animation].frame_count{self.finish_enemy_attack(rules,rng);}
        }
        if self.combat_state==11||self.combat_state==12 {
            let index=if self.combat_state==11{self.sound(0x4e67a4,rules);23}else{self.sound(0x4e66d8,rules);24};self.tick_enemy(index,rules);
            if self.enemy_tracks[index].frame+1==rules.enemy_animations[index].frame_count {self.finished=true;self.phase=FinalBattlePhase::Finished;}
        }
        if self.combat_state<11{if self.enemy_hp<1{self.combat_state=11;}else if self.hp<1{self.sound(0x4e671c,rules);self.combat_state=12;}}
        Ok(())
    }
    fn object_layer(&mut self,address:u32,frame:u32,opaque:bool,rules:&FinalBattleRules){let meta=&rules.runtime_objects[format!("0x{address:08x}")];if let Some(asset_path)=meta["assetPath"].as_str(){self.layers.push(FinalLayer{asset_path:asset_path.to_owned(),frame,x:meta["x"].as_i64().unwrap_or(0)as i32,y:meta["y"].as_i64().unwrap_or(0)as i32,transparent_index:if opaque{None}else{meta["transparentIndex"].as_u64().map(|index|index as u8)},first_pixel_transparent:!opaque&&meta["firstPixelTransparent"].as_bool().unwrap_or(false)});}}
    fn sound(&mut self,address:u32,rules:&FinalBattleRules){let object=&rules.runtime_objects[format!("0x{address:08x}")];if let Some(path)=object["sexResources"].get(self.sex).and_then(Value::as_str).or_else(||object["assetPath"].as_str()){self.sounds.push(path.to_owned());self.sound_serial=self.sound_serial.wrapping_add(1);}}
    fn stop_sound(&mut self,address:u32,rules:&FinalBattleRules){let object=&rules.runtime_objects[format!("0x{address:08x}")];if let Some(path)=object["sexResources"].get(self.sex).and_then(Value::as_str).or_else(||object["assetPath"].as_str()){self.sound_stops.push(path.to_owned());self.sound_serial=self.sound_serial.wrapping_add(1);}}
    fn tick_right(&mut self,index:usize,rules:&FinalBattleRules)->Result<(),String>{let meta=rules.resource(&format!("0x{:08x}",0x495e88+index*4))?;self.right_tracks[index].tick(self.clock_ms,meta.frame_count,meta.frame_duration_ms);self.object_layer(0x4e64cc+index as u32*4,self.right_tracks[index].frame,false,rules);Ok(())}
    fn tick_left(&mut self,index:usize,rules:&FinalBattleRules)->Result<(),String>{let meta=rules.resource(&format!("0x{:08x}",0x495e60+index*4))?;self.left_tracks[index].tick(self.clock_ms,meta.frame_count,meta.frame_duration_ms);self.object_layer(0x4e65e4+index as u32*4,self.left_tracks[index].frame,false,rules);Ok(())}
    fn tick_enemy(&mut self,index:usize,rules:&FinalBattleRules){let meta=&rules.enemy_animations[index];self.enemy_tracks[index].tick(self.clock_ms,meta.frame_count,0);self.object_layer(0x4e6664+index as u32*4,self.enemy_tracks[index].frame,false,rules);}
    pub fn finish_player_attack(&mut self,rules:&FinalBattleRules,rng:&mut ReplayRng){
        let old=self.enemy_hp;self.displayed_enemy_hp=old;if self.enemy_defense<self.attack{self.enemy_hp=self.enemy_hp.wrapping_add(i32::from(self.enemy_defense)-i32::from(self.attack));}self.enemy_hp=self.enemy_hp.max(0);self.enemy_hp_delta=(old as i16).wrapping_sub(self.enemy_hp as i16);self.enemy_text_y=30;self.round=self.round.wrapping_add(1);self.set_item_used(false);self.shield=0;self.combat_state=3;
        self.enemy_animation=if self.round%6==0{self.sound(0x4e67c4,rules);(rng.next_rand()%2)as usize+9}else if self.round%2!=0{self.sound(0x4e67bc,rules);(rng.next_rand()%4)as usize+1}else{let result=(rng.next_rand()%4)as usize+5;self.sound(if result==3{0x4e67c0}else{0x4e67c8},rules);result};self.sound(0x4e6608+self.enemy_animation as u32*4,rules);self.enemy_tracks[self.enemy_animation].reset();self.left_tracks[self.enemy_animation-1].reset();self.sound(0x4e6794+self.enemy_animation as u32*4,rules);
    }
    pub fn finish_enemy_attack(&mut self,rules:&FinalBattleRules,rng:&mut ReplayRng){
        let row=rules.enemy_stats[self.enemy_animation-11];let scale=self.items.len()as i32+100;
        let attack=((rng.next_rand()as i32%i32::from(row[1]))*scale/100)as i16;let attack=attack.wrapping_add(row[0]);
        self.displayed_hp=self.hp;if self.defense<attack{self.hp=self.hp.wrapping_add(i32::from(self.defense)-i32::from(attack));}
        self.enemy_defense=(((rng.next_rand()as i32%i32::from(row[3]))*scale/100)as i16).wrapping_add(row[2]);self.hp=self.hp.clamp(0,60000);self.hp_delta=(self.hp as i16).wrapping_sub(self.displayed_hp as i16);self.hp_text_y=330;self.enemy_animation=0;self.enemy_tracks[0].reset();self.combat_state=0;
    }
    fn tick_bars(&mut self){
        if self.hp_text_y>300&&self.hp_delta!=0{self.hp_text_y-=1;}let step=i32::from(self.hp_delta).abs()/20+1;
        if self.hp<self.displayed_hp{self.displayed_hp=(self.displayed_hp-step).max(self.hp);}else if self.hp>self.displayed_hp{self.displayed_hp=(self.displayed_hp+step).min(self.hp);}
        if self.enemy_text_y>0{self.enemy_text_y-=1;}let step=i32::from(self.enemy_hp_delta/20+1);
        if self.enemy_hp<self.displayed_enemy_hp{self.displayed_enemy_hp=(self.displayed_enemy_hp-step).max(self.enemy_hp);}else if self.enemy_hp>self.displayed_enemy_hp{self.displayed_enemy_hp=(self.displayed_enemy_hp+step).min(self.enemy_hp);}
    }
    pub fn valid_saved_state(&self,rules:&FinalBattleRules)->bool{
        self.sex<2&&self.cards.len()<=5&&self.cards.iter().all(|card|card.kind<4)&&self.items.len()<=36&&self.items.iter().all(|&item|item<10)
        &&self.enemy_tracks.len()==25&&self.player_tracks.len()==13&&self.right_tracks.len()==9&&self.left_tracks.len()==10
        &&self.enemy_tracks.iter().zip(&rules.enemy_animations).all(|(track,meta)|track.frame<meta.frame_count)
        &&self.player_tracks.iter().zip(&rules.player_animations).all(|(track,meta)|track.frame<meta.frame_count)
        &&self.enemy_animation<25&&self.player_animation<13&&self.shield<=2&&(-1..=9).contains(&self.active_item)
        &&matches!(self.combat_state,0|1|2|3|10|11|12)&&self.intro_step<=6
        &&(self.combat_state!=3||(1..=10).contains(&self.enemy_animation))&&(self.combat_state!=10||(11..=20).contains(&self.enemy_animation))
        &&self.selected_item.is_none_or(|index|index<self.items.len())&&self.pending_card.is_none_or(|index|index<self.cards.len())
        &&self.shop_selected_buy.is_none_or(|index|index<9)&&self.shop_selected_sell.is_none_or(|index|index<36)
        &&(self.shop_item_pool.is_empty()||self.shop_item_pool.len()>=self.items.len().max(32)&&self.shop_item_pool.len()<=36)&&self.shop_item_pool.iter().all(|&item|item<10||item==255)
        &&self.shop_animation.as_ref().is_none_or(|animation|animation.item<9&&animation.phase<=2)
    }
}
