//! Native 15 card combat: original packed arithmetic, persistent opponents,
//! real-time cooldowns, animation-driven attacks, and reward settlement.
use crate::{bonus::BonusAnimation,competition::CompetitionAnimation,original_rules::EquipmentSlot,rng::ReplayRng};
use serde::{Deserialize,Serialize};
use std::collections::BTreeMap;

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct BattleRules {
    pub player_cooldown:Vec<i16>,pub career_power:Vec<i16>,pub base_cards:Vec<Vec<i16>>,pub card_positions:Vec<Vec<i16>>,
    pub enemy_cooldown:Vec<Vec<i16>>,pub enemy_hp:Vec<Vec<i16>>,pub modifiers:Vec<Vec<i16>>,pub combos:Vec<Vec<Vec<f64>>>,
    pub rewards:Vec<Vec<i16>>,pub enemy_power:Vec<Vec<i16>>,pub equipment:Vec<Vec<i16>>,
    pub enemy_names:Vec<String>,pub enemy_descriptions:Vec<String>,pub intro_text:Vec<Vec<String>>,pub timeout_text:Vec<String>,
    pub win_text:String,pub mastered_win_text:String,pub mastered_intro_text:String,pub loss_text:String,
    #[serde(default)]pub rendering:serde_json::Value,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct BattleInput {
    pub hp:i16,pub max_hp:i16,pub experience:i16,pub money:i32,pub sex:usize,pub stage:i16,pub career:i16,pub career_level:i16,pub life_level:i16,
    pub stamina:i16,pub intelligence:i16,pub morality:i16,pub charm:i16,pub mission_kind:i16,pub mission_target:i16,pub god:i16,pub traveling:bool,
    pub invulnerable:bool,pub equipped:Vec<i16>,pub carried:Vec<EquipmentSlot>,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct BattleCard {pub kind:i8,pub attack:i32,pub defense:i32}
impl Default for BattleCard {fn default()->Self{Self{kind:0,attack:0,defense:0}}}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct BattleEnemy {pub hp:i16,pub respawn_level:i16,pub encounters:i16,pub wins:i16,pub rank:i16}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct BattlePersistent {pub enemies:Vec<BattleEnemy>,pub cards:Vec<BattleCard>,pub visits:u8,#[serde(default="empty_equipment")]pub equipped_ids:Vec<i16>}
fn empty_equipment()->Vec<i16>{vec![-1;6]}
impl BattlePersistent {
    pub fn new(rules:&BattleRules)->Self{Self{enemies:rules.enemy_hp.iter().map(|hp|BattleEnemy{hp:hp[0],respawn_level:0,encounters:0,wins:0,rank:0}).collect(),cards:vec![],visits:0,equipped_ids:empty_equipment()}}
}
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum BattlePhase {Opening,Intro,Playing,Result,Reward,Finished}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct BattleGame {
    pub phase:BattlePhase,pub clock_ms:u32,pub phase_since:u32,pub input:BattleInput,pub persistent:BattlePersistent,pub rules:BattleRules,
    pub opening_left_x:i16,pub opening_right_x:i16,pub show_opening:bool,
    pub enemy:usize,pub background:usize,pub player_class:i16,pub mission_started:bool,pub hp:i16,pub max_hp:i16,pub enemy_hp:i16,pub enemy_max_hp:i16,
    pub money:i32,pub experience:i32,pub card_capacity:usize,pub cards:Vec<BattleCard>,pub card_count:usize,pub selected:Vec<bool>,pub card_x:Vec<i16>,pub dealing:bool,
    pub player_cooldown:i16,pub enemy_cooldown:i16,pub player_tenths:i16,pub enemy_tenths:i16,pub player_seconds:i16,pub enemy_seconds:i16,
    pub last_tenth:u32,pub last_second:u32,pub remaining:i16,pub attack_power:i16,pub defense_power:i16,pub enemy_attack:i16,pub enemy_defense:i16,
    pub bank_player_attack:i16,pub bank_player_defense:i16,pub bank_enemy_attack:i16,pub bank_enemy_defense:i16,
    pub attack_phase:u8,pub player_attacking:bool,pub enemy_attacking:bool,pub player_actor:usize,pub enemy_actor:usize,pub enemy_effect:usize,
    pub combo_count:usize,pub attack_kind:usize,pub repeat_count:i8,pub attack_repeats:i8,pub reflection:i8,pub strong_attack:bool,pub strong_enemy:bool,pub god_effect:usize,
    pub used_cards:Vec<BattleCard>,pub used_selected:Vec<bool>,pub player_portrait_timers:Vec<i16>,pub enemy_portrait_timers:Vec<i16>,pub loss_y:i16,
    pub won:bool,pub lost:bool,pub result_started:bool,pub return_code:Option<u8>,pub text:String,
    pub displayed_hp:i32,pub displayed_enemy_hp:i32,pub player_damage:i16,pub enemy_damage:i16,
    pub player_damage_frames:i16,pub enemy_damage_frames:i16,pub shown_player_damage:bool,pub shown_enemy_damage:bool,pub hovered_card:Option<usize>,pub palette_resource:String,
    pub tracks:BTreeMap<String,CompetitionAnimation>,pub layers:Vec<CompetitionAnimation>,pub sounds:Vec<String>,pub sound_stops:Vec<String>,
    pub bonus:Option<BonusAnimation>,pub bonus_kind:u8,pub reward_queue:Vec<(u8,i32,i32)>,
    #[serde(default)]pub help:bool,#[serde(default)]pub help_page:usize,#[serde(default)]pub help_since:u32,#[serde(default)]pub pending_select:Option<usize>,#[serde(default)]pub pending_attack:bool,#[serde(default)]pub pending_skip:bool,
}
impl BattleGame {
    pub fn new(input:BattleInput,mut persistent:BattlePersistent,rules:&BattleRules,rng:&mut ReplayRng)->Result<Self,String>{
        let original_persistent=persistent.clone();let original_rng=rng.clone();
        if input.sex>1 || !(0..=7).contains(&input.stage)|| !(-1..25).contains(&input.career)|| !(1..=50).contains(&input.life_level)
            || input.equipped.len()!=6|| input.carried.len()!=8|| input.equipped.iter().any(|&id|!(-1..18).contains(&id))
            || input.carried.iter().any(|item|!(-1..18).contains(&item.id)||!(0..=4).contains(&item.level))
            || persistent.enemies.len()!=15||persistent.cards.len()>5||persistent.enemies.iter().any(|enemy|!(0..=9).contains(&enemy.rank)||!(0..=9).contains(&enemy.respawn_level)){
            return Err("Invalid original battle state".into());
        }
        let player_class=if input.career==-1{if input.stage<6{input.stage+25}else{30}}else{input.career};
        let selected_enemy=if input.stage==2{0}else if (3..=5).contains(&input.stage){rng.next_rand()as usize%5+1}else{rng.next_rand()as usize%9+6};
        let background=rng.next_rand()as usize%9;
        let mut player_cooldown=rules.player_cooldown[((input.life_level-1)/10)as usize];
        // Native computes the enemy's cooldown before applying a quest override.
        let mut enemy_cooldown=rules.enemy_cooldown[selected_enemy][(persistent.enemies[selected_enemy].rank as usize).min(4)];
        let enemy=if input.mission_kind==2&&input.mission_target>1{(input.mission_target-2)as usize}else{selected_enemy};
        if enemy>=15{return Err("Original battle mission target is outside the fifteen enemies".into());}
        let record=&mut persistent.enemies[enemy];let enemy_max_hp=rules.enemy_hp[enemy][(record.rank as usize).min(4)];
        if record.hp<1{if record.respawn_level<9{record.respawn_level+=1;record.hp=rules.enemy_hp[enemy][(record.respawn_level as usize).min(4)];}else{record.hp=0;}}
        let enemy_hp=record.hp.min(enemy_max_hp);
        let card_capacity=match input.stage{0=>2,1|2=>3,3=>4,_=>5};
        persistent.cards.truncate(card_capacity);let card_count=persistent.cards.len();let mut cards=persistent.cards.clone();cards.resize(5,BattleCard::default());
        let mut attack_repeats=1;
        for&id in &input.equipped{if id<0{continue;}let level=input.carried.iter().find(|slot|slot.id==id).map_or(0,|slot|slot.level)as usize;let table=&rules.equipment[id as usize];
            match id{0..=2=>enemy_cooldown=enemy_cooldown.wrapping_add(table[level]),5=>attack_repeats=table[level]as i8,9..=11=>player_cooldown=player_cooldown.wrapping_add(table[level]).max(0),_=>{}}
        }
        let rank=record.rank as usize;let text=if rank==9{rules.mastered_intro_text.clone()}else{rules.intro_text[enemy][rank.min(4)*2+usize::from(record.encounters!=record.rank*2)].clone()};
        let won=rank==9;let money=input.money+if won{5000}else{0};
        let mut result=Self{phase:if input.hp==0{BattlePhase::Finished}else{BattlePhase::Opening},clock_ms:0,phase_since:0,opening_left_x:-412,opening_right_x:640,show_opening:input.hp!=0,hp:input.hp,max_hp:input.max_hp,money,experience:i32::from(input.experience),
            enemy,background,player_class,mission_started:input.mission_kind==2,enemy_hp,enemy_max_hp,input,persistent,rules:rules.clone(),card_capacity,cards,card_count,selected:vec![false;5],card_x:vec![580;5],dealing:false,
            player_cooldown,enemy_cooldown,player_tenths:player_cooldown*10,enemy_tenths:0,player_seconds:player_cooldown,enemy_seconds:0,last_tenth:0,last_second:0,remaining:80,
            attack_power:0,defense_power:0,enemy_attack:0,enemy_defense:0,bank_player_attack:0,bank_player_defense:0,bank_enemy_attack:0,bank_enemy_defense:0,attack_phase:0,player_attacking:false,enemy_attacking:false,
            player_actor:0,enemy_actor:0,enemy_effect:0,combo_count:0,attack_kind:0,repeat_count:0,attack_repeats,reflection:-1,strong_attack:false,strong_enemy:false,god_effect:0,used_cards:vec![BattleCard::default();5],used_selected:vec![false;5],player_portrait_timers:vec![0,1,2,3],enemy_portrait_timers:vec![0,1,2,3],loss_y:0,won,lost:false,result_started:false,
            return_code:None,text,displayed_hp:0,displayed_enemy_hp:i32::from(enemy_hp),player_damage:0,enemy_damage:0,player_damage_frames:0,enemy_damage_frames:0,shown_player_damage:false,shown_enemy_damage:false,hovered_card:None,palette_resource:String::new(),tracks:BTreeMap::new(),layers:vec![],sounds:vec![],sound_stops:vec![],bonus:None,bonus_kind:0,reward_queue:vec![],help:false,help_page:0,help_since:0,pending_select:None,pending_attack:false,pending_skip:false};
        result.displayed_hp=i32::from(result.hp);if result.input.hp==0{result.return_code=Some(2);result.money=result.input.money;result.won=false;result.mission_started=false;result.persistent=original_persistent;*rng=original_rng;}result.load_tracks()?;Ok(result)
    }
    fn level(&self,id:i16)->usize{self.input.carried.iter().find(|item|item.id==id).map_or(0,|item|item.level)as usize}
    pub fn refill(&mut self,rng:&mut ReplayRng){
        for index in self.card_count..self.card_capacity{
            let mut kind=(rng.next_rand()%5)as i8;if kind>3{kind=(rng.next_rand()%11)as i8+4;}
            let mut card=BattleCard{kind,attack:0,defense:0};
            if kind<4{let row=&self.rules.base_cards[kind as usize];let base_attack=rng.next_rand()as i32%(i32::from(row[1]-row[0])+1)+i32::from(row[0]);
                let base_defense=rng.next_rand()as i32%(i32::from(row[3]-row[2])+1)+i32::from(row[2]);
                let scale=|base:i32|if self.input.career==-1{(i32::from(self.input.life_level)*3+100)*base/100}else{
                    ((i32::from(self.rules.career_power[self.input.career as usize])*i32::from(self.input.career_level+9)*base/10)*i32::from(self.input.life_level+100)/100)/100};
                card.attack=scale(base_attack)+(i32::from(self.input.intelligence)*base_attack)/10000+(i32::from(self.input.stamina)*base_attack)/10000;
                card.defense=scale(base_defense)+(i32::from(self.input.charm)*base_defense)/10000+(i32::from(self.input.morality)*base_defense)/10000;
            }self.cards[index]=card;
        }self.card_count=self.card_capacity;
    }
    fn select_now(&mut self,index:usize){
        if self.player_attacking||self.dealing||index>=self.card_count{return;}self.sounds.push("card".into());if self.selected[index]{self.selected[index]=false;return;}
        let kind=self.cards[index].kind;if kind<4{for i in 0..self.card_count{if self.cards[i].kind<4&&self.cards[i].kind!=kind{self.selected[i]=false;}}}self.selected[index]=true;
    }
    pub fn select_card(&mut self,index:usize)->Result<(),String>{if self.phase!=BattlePhase::Playing||self.player_attacking||self.dealing||index>=self.card_count{return Err("Battle card cannot be selected now".into());}self.pending_select=Some(index);Ok(())}
    pub fn hover_card(&mut self,index:Option<usize>)->Result<(),String>{if index.is_some_and(|index|index>=self.card_count){return Err("Unknown battle card".into());}self.hovered_card=index;Ok(())}
    pub fn attack(&mut self)->Result<(),String>{if !self.can_attack(){return Err("Original battle attack is not ready".into());}self.pending_attack=true;Ok(())}
    pub fn can_attack(&self)->bool{self.phase==BattlePhase::Playing&&!self.help&&!self.result_started&&!self.won&&!self.lost&&!self.player_attacking&&!self.enemy_attacking&&!self.dealing&&self.selected.iter().any(|&v|v)&&self.player_cooldown<self.player_tenths/10&&self.player_cooldown<=self.player_seconds}
    pub fn calculate_player(&mut self,commit:bool){
        let mut bases=0;let mut last_base=0;let mut modifier=0;let mut add_attack=0i16;let mut add_defense=0i16;let mut attack=0i16;let mut defense=0i16;
        for index in 0..self.card_capacity{if self.selected[index]{let card=&self.cards[index];if card.kind<4{bases+=1;last_base=card.kind as usize;}else{modifier=card.kind;add_attack=add_attack.wrapping_add(self.rules.modifiers[card.kind as usize][0]);add_defense=add_defense.wrapping_add(self.rules.modifiers[card.kind as usize][1]);}attack=attack.wrapping_add(card.attack as i16);defense=defense.wrapping_add(card.defense as i16);}}
        if bases>1{let combo=&self.rules.combos[last_base][bases-2];if combo[0]!=0.0{attack=(f64::from(attack)*combo[0]).trunc()as i32 as i16;}if combo[1]!=0.0{defense=(f64::from(defense)*combo[1]).trunc()as i32 as i16;}}
        if modifier!=0{attack=attack.wrapping_add((i32::from(attack)*i32::from(add_attack)/100)as i16);defense=defense.wrapping_add((i32::from(defense)*i32::from(add_defense)/100)as i16);}
        if commit{for&id in &self.input.equipped{if id<0{continue;}let level=self.level(id);let row=&self.rules.equipment[id as usize];match id{5=>attack=attack.wrapping_add((i32::from(row[6+level])*i32::from(attack)/100)as i16),12..=14=>attack=attack.wrapping_add((i32::from(row[level])*i32::from(attack)/100)as i16),15..=17=>defense=defense.wrapping_add((i32::from(row[level])*i32::from(defense)/100)as i16),_=>{}}}
            self.bank_player_attack=self.bank_player_attack.wrapping_add(attack);self.bank_player_defense=self.bank_player_defense.wrapping_add(defense);
        }
        self.attack_power=attack;self.defense_power=defense;self.reflection=if last_base==3{i8::from(self.selected.iter().filter(|&&v|v).count()<self.card_capacity/2+1)}else{-1};
    }
    pub fn calculate_enemy(&mut self,rng:&mut ReplayRng){let rank=i32::from(self.persistent.enemies[self.enemy].rank);let row=&self.rules.enemy_power[self.enemy];
        let attack=(rng.next_rand()%30)as i16+row[0];self.enemy_attack=attack.wrapping_add((rank*i32::from(attack)*2/3)as i16);
        let defense=(rng.next_rand()%30)as i16+row[1];self.enemy_defense=defense.wrapping_add((rank*i32::from(defense)*2/3)as i16);
        self.bank_enemy_attack=self.bank_enemy_attack.wrapping_add(self.enemy_attack);self.bank_enemy_defense=self.bank_enemy_defense.wrapping_add(self.enemy_defense);self.strong_enemy=i32::from(row[0])+30<=i32::from(self.enemy_attack);
    }
    pub fn damage_player(&mut self){let before=self.hp;if self.bank_player_defense<self.bank_enemy_attack{if !self.input.invulnerable{self.hp=self.hp.wrapping_add(self.bank_player_defense.wrapping_sub(self.bank_enemy_attack));}self.player_damage_frames=60;}self.hp=self.hp.max(0);self.bank_player_defense=0;self.bank_enemy_attack=0;self.player_damage=before.wrapping_sub(self.hp);if self.hp==0{self.lost=true;}}
    pub fn damage_enemy(&mut self){let before=self.enemy_hp;if self.bank_enemy_defense<self.bank_player_attack{self.enemy_hp=self.enemy_hp.wrapping_add(self.bank_enemy_defense.wrapping_sub(self.bank_player_attack));self.enemy_damage_frames=60;}self.enemy_hp=self.enemy_hp.max(0);self.bank_enemy_defense=0;self.bank_player_attack=0;self.enemy_damage=before.wrapping_sub(self.enemy_hp);if self.enemy_hp==0{self.won=true;}}
    fn apply_reward_equipment(&mut self){let base=&self.rules.rewards[self.enemy];let exp=i32::from(base[2]);let money=i32::from(base[3]);for&id in &self.input.equipped{if id<0{continue;}let row=&self.rules.equipment[id as usize];let level=self.level(id);match id{0..=2|9..=11=>self.experience+=i32::from(row[level+6])*exp/100,3|4=>{self.money+=money*i32::from(row[level])/100;self.experience+=exp*i32::from(row[level+6])/100;},6..=8=>{self.experience+=exp*i32::from(row[level])/100;self.money+=money*i32::from(row[level+6])/100;},_=>{}}}}
    fn load_tracks(&mut self)->Result<(),String>{
        let r=&self.rules.rendering;let class=self.player_class as usize;let sex=self.input.sex;let enemy=self.enemy;
        let mut rows=vec![];
        for i in 0..5{rows.extend([(format!("player{i}"),r["actors"][sex][class][i].clone()),(format!("enemy{i}"),r["enemyActors"][enemy][i].clone()),(format!("element{i}"),r["elementEffects"][i].clone()),(format!("combo{}",i+1),r["cardCombo"][i].clone()),(format!("hover{i}"),r["playerCardFlips"][i].clone()),(format!("selected{i}"),r["enemyCardFlips"][i].clone())]);}
        rows.push(("strong".into(),r["playerStrongEffects"][sex][class].clone()));
        for i in 0..2{rows.extend([(format!("enemyEffect{i}"),r["enemyStrongEffects"][enemy][i].clone()),(format!("shield{i}"),r["shields"][i].clone()),(format!("ready{i}"),r["healthFlares"][i].clone())]);}
        for i in 0..3{rows.push((format!("god{i}"),r["godEffects"][i].clone()));}for i in 0..10{rows.push((format!("help{i}"),r["effectOverlays"][i].clone()));}
        for(name,value)in rows{self.tracks.insert(name,serde_json::from_value(value).map_err(|error|format!("Original battle animation metadata: {error}"))?);}Ok(())
    }
    fn tick_track(&mut self,name:&str){self.tracks.get_mut(name).unwrap().tick(self.clock_ms);}
    fn layer(&mut self,name:&str){self.layers.push(self.tracks[name].clone());}
    fn draw_track(&mut self,name:&str){self.tick_track(name);self.layer(name);}
    fn last(&self,name:&str)->bool{self.tracks[name].last()}
    fn reset(&mut self,name:&str){self.tracks.get_mut(name).unwrap().reset();}
    fn effect_palette(&mut self,name:&str){self.palette_resource=self.tracks[name].resource.clone();}
    pub fn start(&mut self)->Result<(),String>{
        if self.help{self.help_page+=1;self.help_since=self.clock_ms;self.reset(&format!("help{}",self.help_page));if self.help_page>=9{self.help=false;self.phase_since=self.clock_ms;}return Ok(());}
        if self.phase!=BattlePhase::Intro{return Err("Original battle intro has already ended".into());}
        self.effect_palette("strong");if self.won{self.begin_rewards();}else{self.phase=BattlePhase::Playing;self.text.clear();}Ok(())
    }
    pub fn show_help(&mut self)->Result<(),String>{if !matches!(self.phase,BattlePhase::Playing|BattlePhase::Result){return Err("Original battle help is unavailable now".into());}self.help=true;self.help_page=0;self.help_since=self.clock_ms;Ok(())}
    pub fn skip_reward(&mut self){self.pending_skip=true;}
    fn begin_player_attack(&mut self){
        self.displayed_hp=i32::from(self.hp);self.repeat_count=self.attack_repeats;self.combo_count=self.selected.iter().filter(|&&v|v).count();
        self.attack_kind=4;for i in 0..self.card_count{if self.selected[i]&&self.cards[i].kind<4{self.attack_kind=self.cards[i].kind as usize;}}
        self.reset(&format!("combo{}",self.combo_count));self.attack_phase=0;self.player_attacking=true;self.calculate_player(true);
        self.used_cards=self.cards.clone();self.used_selected=self.selected.clone();let mut remaining=vec![];let mut positions=vec![];
        for i in 0..self.card_count{if !self.selected[i]{remaining.push(self.cards[i].clone());positions.push(self.rules.card_positions[i][0]);}}
        self.card_count=remaining.len();for(i,card)in remaining.into_iter().enumerate(){self.cards[i]=card;}self.card_x.fill(580);for(i,x)in positions.into_iter().enumerate(){self.card_x[i]=x;}
        self.selected.fill(false);self.player_portrait_timers=vec![0,1,2,3];self.sounds.push("showCards".into());
    }
    fn after_player_damage(&mut self,rng:&mut ReplayRng){
        self.player_seconds=0;self.player_tenths=0;self.repeat_count=self.repeat_count.wrapping_sub(1);
        if self.repeat_count>0{self.bank_player_attack=self.bank_player_attack.wrapping_add(self.attack_power);self.bank_player_defense=self.bank_player_defense.wrapping_add(self.defense_power);self.attack_phase=1;self.player_actor=1;self.reset("player1");return;}
        self.damage_enemy();
        if self.input.god==1&&!self.input.traveling&&rng.next_rand()%2==1{self.attack_phase=9;self.god_effect=(rng.next_rand()%3)as usize;let name=format!("god{}",self.god_effect);self.reset(&name);self.effect_palette(&name);}
        else{self.attack_phase=0;self.player_attacking=false;}
    }
    fn animate_attacks(&mut self,rng:&mut ReplayRng){
        if self.player_attacking{
            if self.attack_phase==0{let name=format!("combo{}",self.combo_count);self.sounds.push(format!("combo{}",self.combo_count));self.draw_track(&name);
                if self.last(&name){self.sound_stops.push(name);self.strong_attack=(self.card_capacity/2+1)*20<=self.attack_power.max(0)as usize;self.player_actor=if self.strong_attack{2}else{1};self.attack_phase=1;if self.reflection<0{self.sounds.push(if self.strong_attack{"playerStrong"}else{"playerWeak"}.into());}self.reset(&format!("player{}",self.player_actor));}
            }
            if self.attack_phase==1{
                if self.strong_attack{for timer in &mut self.player_portrait_timers{*timer=(*timer-1).max(0);}}
                if self.last(&format!("player{}",self.player_actor)){
                    self.player_actor=0;self.attack_phase=2;
                    if self.reflection<0{self.sounds.push("enemyDefence".into());if !self.strong_attack{let name=format!("element{}",self.attack_kind);self.reset(&name);self.effect_palette(&name);}else if !(self.input.sex==0&&self.player_class==18){self.reset("strong");self.effect_palette("strong");}}
                    else{self.sounds.push("shield".into());let name=format!("shield{}",self.reflection);self.reset(&name);self.effect_palette(&name);}
                }
            }else if self.attack_phase==2{
                if self.reflection<0{
                    if !self.strong_attack{let name=format!("element{}",self.attack_kind);self.draw_track(&name);if self.last(&name){self.after_player_damage(rng);}}
                    else if self.last("strong")||(self.input.sex==0&&self.player_class==18){self.after_player_damage(rng);}
                    else{self.draw_track("strong");}
                }else{
                    let name=format!("shield{}",self.reflection);self.draw_track(&name);if self.reflection==1{self.draw_track(&format!("player{}",self.player_actor));}self.sounds.push(format!("shieldBounce{}",self.reflection));
                    if self.last(&name){self.player_seconds=0;self.player_tenths=0;self.repeat_count=0;self.damage_enemy();self.attack_phase=0;self.player_attacking=false;}
                }
            }else if self.attack_phase==9{
                let name=format!("god{}",self.god_effect);self.draw_track(&name);if self.last(&name){self.attack_phase=0;self.player_attacking=false;let before=self.enemy_hp;self.enemy_hp=self.enemy_hp.wrapping_sub(match self.god_effect{0=>self.enemy_max_hp/4,1=>self.enemy_max_hp/2,_=>self.enemy_max_hp}).max(0);self.enemy_damage=before.wrapping_sub(self.enemy_hp);self.enemy_damage_frames=60;if self.enemy_hp==0{self.won=true;}}
            }
        }
        if self.enemy_attacking{
            if self.attack_phase==3{if self.last(&format!("enemy{}",self.enemy_actor)){self.sounds.push("playerHit".into());self.enemy_effect=usize::from(self.strong_enemy);self.attack_phase=4;self.enemy_actor=0;let name=format!("enemyEffect{}",self.enemy_effect);self.reset(&name);self.effect_palette(&name);}}
            else if self.attack_phase==4{let name=format!("enemyEffect{}",self.enemy_effect);self.draw_track(&name);if self.enemy_effect==1{for timer in &mut self.enemy_portrait_timers{*timer=(*timer-1).max(0);}}
                if self.last(&name){self.attack_phase=0;self.enemy_attacking=false;self.enemy_seconds=0;self.enemy_tenths=0;self.damage_player();}
            }
        }
    }
    fn actor_layers(&mut self){
        let player=format!("player{}",self.player_actor);let enemy=format!("enemy{}",self.enemy_actor);
        {let track=self.tracks.get_mut(&player).unwrap();if self.attack_phase==4{track.x=(track.x+1).min(474);track.y=(track.y+1).min(195);}else{track.x=428;track.y=149;}}
        if !(self.attack_phase==2&&self.reflection==1){self.draw_track(&player);}
        {let track=self.tracks.get_mut(&enemy).unwrap();if self.attack_phase==2&&self.reflection==-1{track.x=(track.x-1).max(38);if track.y>1{track.y-=1;}}else{track.x=84;track.y=46;}}
        self.draw_track(&enemy);
    }
    fn update_bars(&mut self){
        self.shown_player_damage=self.player_damage_frames>0;self.shown_enemy_damage=self.enemy_damage_frames>0;if self.shown_player_damage{self.player_damage_frames-=1;}if self.shown_enemy_damage{self.enemy_damage_frames-=1;}
        if self.displayed_hp>i32::from(self.hp){self.displayed_hp=(self.displayed_hp-i32::from((self.player_damage/20+1)as i8)).max(i32::from(self.hp));}
        if self.displayed_enemy_hp>i32::from(self.enemy_hp){self.displayed_enemy_hp=(self.displayed_enemy_hp-i32::from((self.enemy_damage/20+1)as i8)).max(i32::from(self.enemy_hp));}
        if i32::from(self.player_tenths)>=i32::from(self.player_cooldown)*10{if self.attack_phase==0&&!self.player_attacking{self.sounds.push("cooldownReady".into());}else{self.sound_stops.push("cooldownReady".into());}self.draw_track("ready1");}else{self.sound_stops.push("cooldownReady".into());}
        if self.enemy_tenths>=self.enemy_cooldown.wrapping_mul(10){self.draw_track("ready0");}
    }
    fn deal_cards(&mut self){
        self.dealing=false;
        for i in 0..self.card_count{let target=self.rules.card_positions[i][0];if self.card_x[i]!=target{self.dealing=true;self.card_x[i]=(self.card_x[i]-80).max(target);break;}}
    }
    fn begin_result(&mut self){
        if self.result_started{return;}
        if self.remaining==0&&!self.player_attacking&&!self.enemy_attacking&&self.enemy_hp!=0{
            self.experience+=i32::from(self.rules.rewards[self.enemy][2]);self.money+=i32::from(self.rules.rewards[self.enemy][3]);self.apply_reward_equipment();self.text=self.rules.timeout_text[self.enemy].clone();self.result_started=true;
        }
        if self.won&&!self.result_started&&self.displayed_enemy_hp==i32::from(self.enemy_hp){
            self.sounds.push("enemyDamage".into());
            self.reset(&format!("enemy{}",self.enemy_actor));self.enemy_actor=4;
            let row=&self.rules.rewards[self.enemy];self.experience+=i32::from(row[1])*i32::from(row[2])/100+i32::from(row[2]);self.money+=i32::from(row[0])*i32::from(row[3])/100+i32::from(row[3]);self.apply_reward_equipment();
            let record=&mut self.persistent.enemies[self.enemy];record.wins=record.wins.wrapping_add(1);record.rank=(record.rank+1).min(9);record.encounters=record.rank<<1;
            self.text=if record.rank==9{self.rules.mastered_win_text.clone()}else{self.rules.win_text.clone()};self.result_started=true;
        }
        if self.lost&&!self.result_started&&self.displayed_hp==i32::from(self.hp){self.sounds.push("playerLoss".into());self.player_actor=4;self.reset("player4");self.loss_y=0;self.text=self.rules.loss_text.clone();self.result_started=true;}
        if self.result_started{self.phase=BattlePhase::Result;self.phase_since=self.clock_ms;}
    }
    fn begin_rewards(&mut self){
        self.experience=self.experience.min(9999);self.money=self.money.min(99_999_999);self.reward_queue.clear();
        if self.money!=self.input.money{self.reward_queue.push((0,self.input.money,self.money-self.input.money));}
        if self.experience!=i32::from(self.input.experience){self.reward_queue.push((1,i32::from(self.input.experience),self.experience-i32::from(self.input.experience)));}
        if self.lost&&self.remaining!=0&&self.money!=0{self.reward_queue.push((2,self.money,-(self.money/2)));}
        self.phase=BattlePhase::Reward;self.next_reward();
    }
    fn next_reward(&mut self){
        if self.reward_queue.is_empty(){self.finish();return;}let(kind,value,delta)=self.reward_queue.remove(0);self.bonus_kind=kind;self.bonus=Some(BonusAnimation::new(value,delta));
    }
    fn finish(&mut self){
        self.hp=self.hp.min(self.max_hp);let record=&mut self.persistent.enemies[self.enemy];record.hp=self.enemy_hp.min(self.rules.enemy_hp[self.enemy][4]);record.encounters=record.encounters.wrapping_add(1);
        self.persistent.visits=self.persistent.visits.min(4)+1;self.persistent.cards=self.cards[..self.card_count].to_vec();
        self.return_code=Some(if self.won{1}else if self.lost&&self.remaining!=0{2}else{0});if self.lost&&self.remaining==0&&!self.won{self.hp=1;}
        // 0x433550 only animates the defeat penalty. Native never writes it
        // back to 0x4b648a; the retained money is deliberately unchanged.
        self.phase=BattlePhase::Finished;self.bonus=None;
    }
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng){
        if self.phase==BattlePhase::Finished{return;}self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);self.layers.clear();self.sounds.clear();self.sound_stops.clear();
        self.show_opening=false;
        if self.phase==BattlePhase::Opening{self.show_opening=true;self.opening_left_x=(self.opening_left_x+16).min(0);self.opening_right_x=(self.opening_right_x-16).max(228);if self.opening_left_x==0&&self.opening_right_x==228{self.phase=BattlePhase::Intro;self.phase_since=self.clock_ms;self.help=self.persistent.visits==0;self.help_since=self.clock_ms;}return;}
        if self.help{let name=format!("help{}",self.help_page);if !self.last(&name){self.tick_track(&name);}self.layer(&name);if self.clock_ms.wrapping_sub(self.help_since)>=10000{let _=self.start();}return;}
        if self.phase==BattlePhase::Intro{self.actor_layers();if self.clock_ms.wrapping_sub(self.phase_since)>10000{let _=self.start();}return;}
        if self.phase==BattlePhase::Reward{let skip=std::mem::take(&mut self.pending_skip);if let Some(bonus)=&mut self.bonus{if !bonus.tick(self.clock_ms,rng,skip){self.next_reward();}}return;}
        if !self.result_started{if self.clock_ms>=self.last_tenth.wrapping_add(100){self.player_tenths=self.player_tenths.wrapping_add(1);self.enemy_tenths=self.enemy_tenths.wrapping_add(1);self.last_tenth=self.clock_ms;}if self.clock_ms>=self.last_second.wrapping_add(1000){self.player_seconds=self.player_seconds.wrapping_add(1);self.enemy_seconds=self.enemy_seconds.wrapping_add(1);if self.remaining>0{self.remaining-=1;}self.last_second=self.clock_ms;}}
        self.update_bars();self.actor_layers();
        if !self.result_started{
            self.refill(rng);self.deal_cards();if let Some(index)=self.pending_select.take(){self.select_now(index);}for i in 0..self.card_count{if self.selected[i]{self.draw_track(&format!("selected{i}"));}}
            if !self.dealing{if let Some(index)=self.hovered_card{if index<self.card_count{self.draw_track(&format!("hover{index}"));}}}
            self.animate_attacks(rng);if !self.player_attacking{self.calculate_player(false);}
        }
        let attack=std::mem::take(&mut self.pending_attack);if attack&&self.can_attack(){self.begin_player_attack();}
        if self.enemy_cooldown<=self.enemy_tenths/10&&self.enemy_cooldown<=self.enemy_seconds&&!self.enemy_attacking&&!self.player_attacking&&!self.won&&!self.lost&&self.remaining>0{
            self.enemy_portrait_timers=vec![0,1,2,3];self.calculate_enemy(rng);if self.strong_enemy{self.sounds.push("enemyStrong".into());}self.enemy_actor=usize::from(self.strong_enemy)+1;self.reset(&format!("enemy{}",self.enemy_actor));self.enemy_attacking=true;self.attack_phase=3;
        }
        if self.result_started{
            if self.won{if self.last(&format!("enemy{}",self.enemy_actor)){self.begin_rewards();}}
            else if self.lost&&self.remaining!=0{self.loss_y=(self.loss_y+10).min(200);if self.last(&format!("player{}",self.player_actor)){self.begin_rewards();}}
            else if self.clock_ms>=self.phase_since.wrapping_add(4000){self.begin_rewards();}
        }else{self.begin_result();}
    }
}

#[cfg(test)]mod tests{
    use super::*;
    fn rules()->BattleRules{serde_json::from_value(serde_json::from_str::<serde_json::Value>(include_str!("../../../assets/catalog/rules.json")).unwrap()["battle"].clone()).unwrap()}
    fn compare(game:&BattleGame,rng:&ReplayRng,expected:&serde_json::Value,fields:&[&str]){let value=serde_json::to_value(game).unwrap();for&field in fields{assert_eq!(value[field],expected[field],"{field}; enemy {}",game.enemy);}assert_eq!(u64::from(rng.state),expected["rngState"].as_u64().unwrap());assert_eq!(rng.draws,expected["rngDraws"].as_u64().unwrap());}
    #[test]fn numeric_battle_matches_original_x86(){
        let rules=rules();let all:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-battle.json")).unwrap();
        for case in all["initial"].as_array().unwrap(){let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());let mut game=BattleGame::new(serde_json::from_value(case["input"].clone()).unwrap(),serde_json::from_value(case["persistent"].clone()).unwrap(),&rules,&mut rng).unwrap();
            compare(&game,&rng,&case["initial"],&["enemy","background","playerClass","hp","maxHp","enemyHp","enemyMaxHp","experience","cardCapacity","cardCount","playerCooldown","enemyCooldown","playerTenths","enemyTenths","playerSeconds","enemySeconds","remaining","attackRepeats"]);
            game.refill(&mut rng);assert_eq!(serde_json::to_value(&game.cards[..game.card_count]).unwrap(),case["cards"]);assert_eq!(u64::from(rng.state),case["rngState"].as_u64().unwrap());assert_eq!(rng.draws,case["rngDraws"].as_u64().unwrap());
        }
        for case in all["combos"].as_array().unwrap(){let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());let mut game=BattleGame::new(serde_json::from_value(case["input"].clone()).unwrap(),BattlePersistent::new(&rules),&rules,&mut rng).unwrap();game.cards=serde_json::from_value(case["cards"].clone()).unwrap();game.selected=serde_json::from_value(case["selected"].clone()).unwrap();
            for(name,commit)in[("preview",false),("committed",true)]{game.calculate_player(commit);compare(&game,&rng,&case[name],&["attackPower","defensePower","bankPlayerAttack","bankPlayerDefense","reflection"]);}game.calculate_enemy(&mut rng);compare(&game,&rng,&case["enemy"],&["enemyAttack","enemyDefense","bankEnemyAttack","bankEnemyDefense"]);
        }
        for case in all["damage"].as_array().unwrap(){let inp:BattleInput=serde_json::from_value(all["initial"][0]["input"].clone()).unwrap();let mut rng=ReplayRng::new(42);let mut game=BattleGame::new(inp,BattlePersistent::new(&rules),&rules,&mut rng).unwrap();game.hp=case["hp"].as_i64().unwrap()as i16;game.enemy_hp=case["enemyHp"].as_i64().unwrap()as i16;game.input.invulnerable=case["invulnerable"].as_bool().unwrap();
            let bank:Vec<i16>=serde_json::from_value(case["bank"].clone()).unwrap();game.bank_player_attack=bank[0];game.bank_player_defense=bank[1];game.bank_enemy_attack=bank[2];game.bank_enemy_defense=bank[3];game.damage_player();game.damage_enemy();let value=serde_json::to_value(&game).unwrap();for field in ["hp","enemyHp","playerDamage","enemyDamage","bankPlayerAttack","bankPlayerDefense","bankEnemyAttack","bankEnemyDefense"]{assert_eq!(value[field],case["after"][field],"damage {field}");}assert_eq!(game.won,case["won"].as_bool().unwrap());assert_eq!(game.lost,case["lost"].as_bool().unwrap());
        }
    }
    #[test]fn battle_loop_matches_original_x86_frames(){
        let rules=rules();let all:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-battle.json")).unwrap();
        for(case_index,case)in all["sequences"].as_array().unwrap().iter().enumerate(){let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());let mut game=BattleGame::new(serde_json::from_value(case["input"].clone()).unwrap(),serde_json::from_value(case["persistent"].clone()).unwrap(),&rules,&mut rng).unwrap();game.help=false;game.phase=BattlePhase::Intro;game.start().unwrap();game.rules.rendering=serde_json::Value::Null;let mut expected_tracks=serde_json::Map::new();
            for frame in case["frames"].as_array().unwrap(){let time=frame["time"].as_u64().unwrap()as u32;game.pending_select=frame["select"].as_u64().map(|v|v as usize);game.pending_attack=frame["attack"].as_bool().unwrap();game.tick(time-game.clock_ms,&mut rng);let actual=serde_json::to_value(&game).unwrap();let mut expected=if frame["after"].is_array(){serde_json::Value::Object(all["frameFields"].as_array().unwrap().iter().zip(frame["after"].as_array().unwrap()).map(|(name,value)|(name.as_str().unwrap().to_string(),value.clone())).collect())}else{frame["after"].clone()};
                if expected["tracks"].is_array(){for change in expected["tracks"].as_array().unwrap(){expected_tracks.insert(all["trackNames"][change[0].as_u64().unwrap()as usize].as_str().unwrap().into(),change[1].clone());}expected["tracks"]=serde_json::Value::Object(expected_tracks.clone());for layer in expected["layers"].as_array_mut().unwrap(){layer[0]=all["trackNames"][layer[0].as_u64().unwrap()as usize].clone();}}
                for(name,value)in expected.as_object().unwrap(){if ["cards","tracks","layers","rngState","rngDraws"].contains(&name.as_str()){continue;}let expected=if actual[name].is_boolean()&&value.is_number(){serde_json::json!(value.as_i64().unwrap()!=0)}else{value.clone()};assert_eq!(actual[name],expected,"case {case_index}, time {time}, {name}");}
                assert_eq!(serde_json::to_value(&game.cards[..game.card_count]).unwrap(),expected["cards"],"case {case_index}, time {time}, cards");
                assert_eq!(u64::from(rng.state),expected["rngState"].as_u64().unwrap(),"case {case_index}, time {time}, RNG state");assert_eq!(rng.draws,expected["rngDraws"].as_u64().unwrap(),"case {case_index}, time {time}, RNG draws");
                for(name,track)in expected["tracks"].as_object().unwrap(){let t=&game.tracks[name];assert_eq!(serde_json::json!([t.frame,t.last_tick,i32::from(t.initial_frame),t.x,t.y]),*track,"case {case_index}, time {time}, track {name}");}
                let names:Vec<serde_json::Value>=game.layers.iter().map(|layer|{let name=game.tracks.iter().find(|(_,track)|track.resource==layer.resource&&track.x==layer.x&&track.y==layer.y).map(|(name,_)|name.clone()).unwrap_or_else(||"moved".into());serde_json::json!([name,layer.frame,layer.x,layer.y])}).collect();
                // Two native tracks can share an identical resource; compare
                // actual resource/frame/position instead of arbitrary map keys.
                let wanted:Vec<serde_json::Value>=expected["layers"].as_array().unwrap().iter().map(|entry|serde_json::json!([game.tracks[entry[0].as_str().unwrap()].resource,entry[1],entry[2],entry[3]])).collect();
                let got:Vec<serde_json::Value>=game.layers.iter().map(|layer|serde_json::json!([layer.resource,layer.frame,layer.x,layer.y])).collect();assert_eq!(got,wanted,"case {case_index}, time {time}, layers {names:?}");
            }
            if case["finished"].as_bool().unwrap(){assert!(matches!(game.phase,BattlePhase::Reward|BattlePhase::Finished),"case {case_index} did not end");assert_eq!(i64::from(game.money),case["settled"]["money"].as_i64().unwrap());assert_eq!(i64::from(game.experience),case["settled"]["experience"].as_i64().unwrap());}
        }
    }
    #[test]fn battle_entry_and_restore_preserve_native_boundaries(){
        let rules=rules();let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-battle.json")).unwrap();let input:BattleInput=serde_json::from_value(fixture["initial"][0]["input"].clone()).unwrap();
        let mut empty=input.clone();empty.hp=0;empty.mission_kind=2;empty.mission_target=3;let persistent=BattlePersistent::new(&rules);let mut rng=ReplayRng::new(42);let before=rng.clone();let game=BattleGame::new(empty,persistent.clone(),&rules,&mut rng).unwrap();assert_eq!(game.return_code,Some(2));assert_eq!(game.persistent,persistent);assert_eq!(rng,before);assert!(!game.mission_started);assert_eq!(game.money,input.money);
        let mut game=BattleGame::new(input,persistent,&rules,&mut rng).unwrap();for i in 0..26{game.tick(16,&mut rng);assert_eq!(game.show_opening,true);assert_eq!(game.opening_left_x,(-412+(i+1)*16).min(0));}assert_eq!(game.phase,BattlePhase::Intro);assert!(game.help);for _ in 0..9{game.start().unwrap();}assert!(!game.help);game.start().unwrap();assert_eq!(game.phase,BattlePhase::Playing);
        for _ in 0..200{if !game.dealing&&!game.player_attacking&&game.card_count>0&&!game.selected[0]{game.pending_select=Some(0);}game.pending_attack=true;game.tick(100,&mut rng);if game.player_attacking&&game.attack_phase==2{break;}}
        assert!(game.player_attacking);let mut restored:BattleGame=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();let mut restored_rng=rng.clone();for _ in 0..300{game.tick(71,&mut rng);restored.tick(71,&mut restored_rng);assert_eq!(game,restored);assert_eq!(rng,restored_rng);}
    }
}
