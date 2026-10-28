//! Firebee shooter (0x00445df0), including original frame-based motion, shared
//! RNG consumption, projectile compaction, and the first-boss-hit exit quirk.
use crate::{bonus::BonusAnimation,rng::ReplayRng};
use serde::{Deserialize,Serialize};

pub const ROW_X:[i16;7]=[100,136,166,198,228,260,290];
const INTERVAL:[u32;4]=[300,150,100,80];

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum FirebeePhase {Ready,Playing,Help,Reward,Finished}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FirebeeTrack {pub x:i32,pub y:i32,pub frame:u32,pub last_tick:u32,pub first:bool}
impl Default for FirebeeTrack {fn default()->Self {Self{x:0,y:0,frame:0,last_tick:0,first:true}}}
impl FirebeeTrack {
    fn tick(&mut self,now:u32,count:u32,period:u32){
        if now.wrapping_sub(self.last_tick)>period {
            self.last_tick=now;
            if self.first {self.first=false;}else{self.frame=(self.frame+1)%count;}
        }
    }
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FirebeeEnemyDraw {pub row:u8,pub x:i32,pub y:i32,pub frame:u32}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct FirebeeGame {
    pub phase:FirebeePhase,pub clock_ms:u32,pub help_at:u32,
    pub wave:u8,pub direction:u8,pub player_state:u8,pub player_x:i32,
    pub failed:bool,pub finished:bool,pub shot_at:u32,pub descent_at:u32,pub score:i16,
    pub row_y:[i16;16],pub health:Vec<i8>,pub enemies:Vec<FirebeeTrack>,
    pub enemy_shots:Vec<[i16;2]>,pub player_shots:Vec<[i16;2]>,pub explosions:Vec<FirebeeTrack>,pub death:FirebeeTrack,
    pub drawn_enemies:Vec<FirebeeEnemyDraw>,pub drawn_enemy_shots:Vec<[i16;2]>,
    pub drawn_player_shots:Vec<[i16;2]>,pub drawn_explosions:Vec<FirebeeTrack>,
    pub sounds:Vec<u8>,pub starting_money:i32,pub reward:Option<BonusAnimation>,pub reward_skip:bool,
    enemy_storage:Vec<[i16;2]>,player_storage:Vec<[i16;2]>,explosion_storage:Vec<FirebeeTrack>,
    left:bool,right:bool,fire:bool,
}

impl FirebeeGame {
    pub fn new(money:i32)->Self {
        let mut row_y=std::array::from_fn(|i|238-56*i as i16);row_y[15]-=100;
        let mut health=vec![0;112];
        for row in 0..16 {for col in 0..Self::columns(row) {health[row*7+col]=if row==14{8}else if row==15{100}else{row as i8+1};}}
        let mut enemies=vec![FirebeeTrack::default();16];enemies[14].x=290;enemies[15].x=250;
        Self {phase:FirebeePhase::Ready,clock_ms:0,help_at:0,wave:6,direction:0,player_state:0,player_x:285,
            failed:false,finished:false,shot_at:0,descent_at:0,score:0,row_y,health,enemies,
            enemy_shots:Vec::new(),player_shots:Vec::new(),explosions:Vec::new(),death:FirebeeTrack::default(),
            drawn_enemies:Vec::new(),drawn_enemy_shots:Vec::new(),drawn_player_shots:Vec::new(),drawn_explosions:Vec::new(),
            sounds:Vec::new(),starting_money:money,reward:None,reward_skip:false,
            enemy_storage:vec![[0;2];105],player_storage:vec![[0;2];101],explosion_storage:vec![FirebeeTrack::default();100],
            left:false,right:false,fire:false}
    }
    fn columns(row:usize)->usize {(7-row/2).max(1)}
    pub fn start(&mut self)->bool {
        if matches!(self.phase,FirebeePhase::Ready|FirebeePhase::Help){self.phase=FirebeePhase::Playing;true}else{false}
    }
    pub fn show_help(&mut self)->bool {
        if self.phase==FirebeePhase::Playing {self.help_at=self.clock_ms;self.phase=FirebeePhase::Help;true}else{false}
    }
    pub fn input(&mut self,left:bool,right:bool,fire:bool){self.left=left;self.right=right;self.fire=fire;}
    pub fn finish_reward_animation(&mut self){if self.phase==FirebeePhase::Reward {self.reward_skip=true;}}
    pub fn final_money(&self)->Option<i32>{
        (self.phase==FirebeePhase::Finished).then(||self.starting_money.wrapping_add(i32::from(self.score)*11).min(99_999_999))
    }
    fn explode(&mut self,count:&mut usize,x:i32,y:i32){
        if *count<self.explosion_storage.len(){
            let sprite=&mut self.explosion_storage[*count];sprite.x=x;sprite.y=y;sprite.frame=0;*count+=1;
        }
    }
    fn compact(storage:&mut [[i16;2]],index:usize,count:usize,include_tail:bool){
        for i in index..count-usize::from(!include_tail){storage[i]=storage[i+1];}
    }
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng){
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);let now=self.clock_ms;
        self.sounds.clear();
        if matches!(self.phase,FirebeePhase::Ready|FirebeePhase::Help){
            if now.wrapping_sub(self.help_at)>=20_000 {self.start();}else{return;}
        }
        if self.phase==FirebeePhase::Reward {
            let skip=std::mem::take(&mut self.reward_skip);
            if self.reward.as_mut().is_none_or(|reward|!reward.tick(now,rng,skip)){self.phase=FirebeePhase::Finished;}
            return;
        }
        if self.phase!=FirebeePhase::Playing{return;}
        let mut enemy_count=self.enemy_shots.len();let mut player_count=self.player_shots.len();let mut explosion_count=self.explosions.len();
        self.enemy_storage[..enemy_count].copy_from_slice(&self.enemy_shots);
        self.player_storage[..player_count].copy_from_slice(&self.player_shots);
        self.explosion_storage[..explosion_count].clone_from_slice(&self.explosions);
        self.drawn_enemies.clear();self.drawn_enemy_shots.clear();self.drawn_player_shots.clear();self.drawn_explosions.clear();
        let mut column_cursor=0;
        for row in usize::from(self.wave)-6..usize::from(self.wave) {
            self.enemies[row].tick(now,24,71);
            if row<14 {
                for col in 0..Self::columns(row) {
                    if self.health[row*7+col]>0 {
                        let x=i32::from(ROW_X[row/2])+col as i32*62;
                        self.enemies[row].x=x;self.enemies[row].y=i32::from(self.row_y[row]);
                        self.drawn_enemies.push(FirebeeEnemyDraw{row:row as u8,x,y:i32::from(self.row_y[row]),frame:self.enemies[row].frame});
                        if rng.next_rand()%9>=8 && enemy_count<99 && now>=self.shot_at.wrapping_add(300) {
                            self.enemy_storage[enemy_count]=[(x+25) as i16,self.row_y[row]+20];enemy_count+=1;self.shot_at=now;
                        }
                    }
                }
                column_cursor=Self::columns(row);
            }else if self.health[row*7]>0 {
                let enemy=&mut self.enemies[row];
                if enemy.x<350 && self.direction==0 && row>14 {enemy.x+=2;}else{self.direction=1;}
                if enemy.x>200 && self.direction==1 && row>14 {enemy.x-=2;}else{self.direction=0;}
                enemy.y=i32::from(self.row_y[row]);
                self.drawn_enemies.push(FirebeeEnemyDraw{row:row as u8,x:enemy.x,y:enemy.y,frame:enemy.frame});
                if rng.next_rand()%9>=8 && enemy_count<99 && now>=self.shot_at.wrapping_add(500) {
                    if row==14 && enemy.y>40 {
                        self.sounds.push(3);self.enemy_storage[enemy_count]=[(enemy.x+column_cursor as i32*40+25) as i16,(enemy.y+20) as i16];enemy_count+=1;self.shot_at=now;
                    }else if enemy.y>10 {
                        self.sounds.push(3);
                        for col in 0..5 {self.enemy_storage[enemy_count]=[(enemy.x+25+col*40) as i16,(enemy.y+20) as i16];enemy_count+=1;}
                        column_cursor=5;self.shot_at=now;
                    }
                }
            }
        }
        let mut index=0;
        while index<enemy_count {
            if i32::from(self.enemy_storage[index][1])+6<360 {self.enemy_storage[index][1]+=6;}
            else{Self::compact(&mut self.enemy_storage,index,enemy_count,false);enemy_count-=1;}
            self.drawn_enemy_shots.push(self.enemy_storage[index]);index+=1;
        }
        index=0;
        while index<enemy_count {
            if let Some(hit)=(0..player_count).find(|&hit|(i32::from(self.enemy_storage[index][0])-i32::from(self.player_storage[hit][0])).abs()<10 && (i32::from(self.enemy_storage[index][1])-i32::from(self.player_storage[hit][1])).abs()<10) {
                Self::compact(&mut self.enemy_storage,index,enemy_count,true);enemy_count-=1;
                if explosion_count<99 {self.explode(&mut explosion_count,i32::from(self.player_storage[hit][0])-25,i32::from(self.player_storage[hit][1]));}
                Self::compact(&mut self.player_storage,hit,player_count,true);player_count-=1;
            }
            index+=1;
        }
        for point in &self.enemy_storage[..enemy_count] {
            if point[1]>350 && self.player_x+20<i32::from(point[0]) && i32::from(point[0])<self.player_x+60 {self.failed=true;self.sounds.push(7);}
        }
        let first=usize::from(self.wave)-6;
        if now>=self.descent_at.wrapping_add(INTERVAL[first/3]) {
            for y in &mut self.row_y {*y=y.wrapping_add(1);}
            self.descent_at=now;
            if i32::from(self.row_y[first])+30>=350 {
                if (0..Self::columns(first)).any(|col|self.health[first*7+col]>0) {self.failed=true;self.sounds.push(7);}
                else if self.wave<16 {self.wave+=1;}
            }
        }
        self.player_state=0;
        if !self.failed {
            if self.left {self.player_state=1;self.player_x=(self.player_x-2).max(103);}
            else if self.right {self.player_state=2;self.player_x=if self.player_x+82<536 {self.player_x+2}else{456};}
        }
        if std::mem::take(&mut self.fire) && !self.failed && player_count<100 {
            self.sounds.push(4);self.player_storage[player_count]=[(self.player_x+30) as i16,320];player_count+=1;
        }
        index=0;
        while index<player_count {
            for row in usize::from(self.wave)-6..usize::from(self.wave) {
                let point=self.player_storage[index];
                if row==15 && self.wave==16 && self.enemies[15].y>20 {
                    let enemy=&self.enemies[15];
                    if enemy.y<i32::from(point[1]) && i32::from(point[1])<enemy.y+30 && enemy.x+30<i32::from(point[0]) && i32::from(point[0])<enemy.x+90 && self.health[105]>0 {
                        self.health[105]-=1;if self.health[105]==0{self.score=self.score.wrapping_add(160);}
                        self.explode(&mut explosion_count,i32::from(point[0])-30,i32::from(point[1])-20);
                        Self::compact(&mut self.player_storage,index,player_count,false);player_count-=1;self.player_storage[player_count]=[0,0];
                        self.sounds.push(5);self.finished=true;break;
                    }
                }else {
                    let columns=Self::columns(row);let mut overlap=false;
                    if self.row_y[row]<point[1] && i32::from(point[1])<i32::from(self.row_y[row])+30 {
                        for col in 0..columns {
                            let x=i32::from(ROW_X[7-columns])+col as i32*62;
                            if x+10<i32::from(point[0]) && i32::from(point[0])<x+52 {
                                overlap=true;
                                if self.health[row*7+col]>0 {
                                    self.health[row*7+col]-=1;
                                    if self.health[row*7+col]==0{self.score=self.score.wrapping_add((row as i16+1)*10);}
                                    self.explode(&mut explosion_count,i32::from(point[0])-30,i32::from(point[1])-20);
                                    Self::compact(&mut self.player_storage,index,player_count,false);player_count-=1;self.player_storage[player_count]=[0,0];
                                    self.sounds.push(6);
                                }
                                break;
                            }
                        }
                    }
                    if overlap {break;}
                }
            }
            index+=1;
        }
        index=0;
        while index<player_count {
            if i32::from(self.player_storage[index][1])-8<86 {Self::compact(&mut self.player_storage,index,player_count,false);player_count-=1;}
            else{self.player_storage[index][1]-=8;}
            self.drawn_player_shots.push(self.player_storage[index]);index+=1;
        }
        index=0;
        while index<explosion_count {
            self.explosion_storage[index].tick(now,10,114);
            self.drawn_explosions.push(self.explosion_storage[index].clone());
            if self.explosion_storage[index].frame==9 {
                // Original moves only x/y/frame; each destination keeps its timer.
                for next in index..explosion_count-1 {
                    self.explosion_storage[next].x=self.explosion_storage[next+1].x;
                    self.explosion_storage[next].y=self.explosion_storage[next+1].y;
                    self.explosion_storage[next].frame=self.explosion_storage[next+1].frame;
                }
                explosion_count-=1;self.player_storage[player_count]=[0,0];
            }
            index+=1;
        }
        if self.failed {
            self.death.x=self.player_x;self.death.y=350;self.death.tick(now,20,71);
            if self.death.frame==19{self.finished=true;}
        }
        self.enemy_shots=self.enemy_storage[..enemy_count].to_vec();self.player_shots=self.player_storage[..player_count].to_vec();self.explosions=self.explosion_storage[..explosion_count].to_vec();
        if self.finished {
            if self.score!=0 {
                // The bonus function receives a signed short even though final
                // settlement uses the full signed-score * 11 integer product.
                self.reward=Some(BonusAnimation::new(self.starting_money,i32::from(self.score.wrapping_mul(11))));self.phase=FirebeePhase::Reward;
            }else{self.phase=FirebeePhase::Finished;}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(t:&FirebeeTrack)->serde_json::Value {serde_json::json!([t.x,t.y,t.frame,t.last_tick,u8::from(t.first)])}
    #[test]
    fn original_x86_complete_frame_parity(){
        let data:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-firebee.json")).unwrap();
        for (case_index,case) in data["cases"].as_array().unwrap().iter().enumerate(){
            let mut game=FirebeeGame::new(1000);let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            for (key,value) in case["initial"].as_object().unwrap(){match key.as_str(){
                "wave"=>game.wave=value.as_u64().unwrap() as u8,"playerX"=>game.player_x=value.as_i64().unwrap() as i32,
                "rowY"=>game.row_y=serde_json::from_value(value.clone()).unwrap(),"health"=>game.health=serde_json::from_value(value.clone()).unwrap(),
                "playerShots"=>game.player_shots=serde_json::from_value(value.clone()).unwrap(),"enemyShots"=>game.enemy_shots=serde_json::from_value(value.clone()).unwrap(),
                "score"=>game.score=value.as_i64().unwrap() as i16,_=>panic!("Unknown initial {key}"),
            }}
            game.start();
            for (frame_index,frame) in case["frames"].as_array().unwrap().iter().enumerate(){
                game.input(frame["left"].as_bool().unwrap_or_else(||frame["left"].as_i64()==Some(1)),frame["right"].as_bool().unwrap_or_else(||frame["right"].as_i64()==Some(1)),frame["fire"].as_bool().unwrap_or(false));
                game.tick(frame["time"].as_u64().unwrap() as u32-game.clock_ms,&mut rng);
                let values=serde_json::json!({"wave":game.wave,"direction":game.direction,"playerState":game.player_state,"playerX":game.player_x,
                    "failed":u8::from(game.failed),"finished":u8::from(game.finished),"shotAt":game.shot_at,"descentAt":game.descent_at,"score":game.score,
                    "rowY":game.row_y,"health":game.health,"enemyShots":game.enemy_shots,"playerShots":game.player_shots,
                    "enemies":game.enemies.iter().map(track).collect::<Vec<_>>(),"explosions":game.explosions.iter().map(track).collect::<Vec<_>>(),"death":track(&game.death),"rngState":rng.state,"rngDraws":rng.draws,
                    "drawnEnemies":game.drawn_enemies.iter().map(|enemy|serde_json::json!([enemy.row,enemy.x,enemy.y,enemy.frame])).collect::<Vec<_>>(),
                    "drawnEnemyShots":game.drawn_enemy_shots,"drawnPlayerShots":game.drawn_player_shots,"drawnExplosions":game.drawn_explosions.iter().map(track).collect::<Vec<_>>(),"sounds":game.sounds});
                for (field_index,field) in data["fields"].as_array().unwrap().iter().enumerate(){let key=field.as_str().unwrap();assert_eq!(values[key],frame["after"][field_index],"case {case_index} frame {frame_index} {key}");}
            }
        }
        for case in data["rewards"].as_array().unwrap(){
            let mut game=FirebeeGame::new(case["before"].as_i64().unwrap() as i32);game.score=case["score"].as_i64().unwrap() as i16;game.phase=FirebeePhase::Finished;
            assert_eq!(game.final_money(),Some(case["after"].as_i64().unwrap() as i32));
            assert_eq!(i32::from(game.score.wrapping_mul(11)),case["animationDelta"].as_i64().unwrap() as i32);
        }
    }
    #[test]
    fn save_resume_help_and_money_settlement(){
        let mut game=FirebeeGame::new(99_999_950);assert_eq!(game.final_money(),None);game.start();
        let mut rng=ReplayRng::new(1999);game.input(false,true,true);game.tick(16,&mut rng);
        let mut restored:FirebeeGame=serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();let mut rng2=rng.clone();
        for _ in 0..30 {game.tick(16,&mut rng);restored.tick(16,&mut rng2);assert_eq!(game,restored);assert_eq!(rng,rng2);}
        assert!(game.show_help());let before=game.row_y;game.tick(1000,&mut rng);assert_eq!(game.row_y,before);assert!(game.start());
        game.score=10;game.phase=FirebeePhase::Finished;assert_eq!(game.final_money(),Some(99_999_999));
    }
}
