//! Rules independently recovered from the original executable. Each function
//! documents its source address so unresolved behavior is not filled by guesses.
use crate::{engine::Map,rng::ReplayRng};
use serde::{Deserialize,Serialize};
use crate::character::LegacyPlayer;

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct EquipmentKnowledge{pub learned:bool,pub level:u8}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct EquipmentSlot{pub id:i16,pub level:i16}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct ExamQuestion{pub id:usize,pub question:String,pub options:Vec<String>}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct SchoolExam{pub used:Vec<usize>,pub current:ExamQuestion,pub correct:usize,pub answered:u8,pub score:u8,pub started:bool}
impl SchoolExam{
    /// FUN_0043eac0 draws a unique question, then swaps answer zero with rand%4.
    fn draw(questions:&[ExamQuestion],used:&mut Vec<usize>,rng:&mut ReplayRng)->Result<(ExamQuestion,usize),String>{
        if questions.len()<=used.len(){return Err("Original exam question bank exhausted".into());}
        let mut index=rng.next_rand()as usize%questions.len();
        while used.contains(&index){index=rng.next_rand()as usize%questions.len();}
        let mut question=questions[index].clone();
        if question.options.len()!=4{return Err("Original exam question must have four answers".into());}
        used.push(index);let correct=rng.next_rand()as usize%4;question.options.swap(0,correct);Ok((question,correct))
    }
    pub fn new(questions:&[ExamQuestion],rng:&mut ReplayRng)->Result<Self,String>{
        let mut used=Vec::new();let(current,correct)=Self::draw(questions,&mut used,rng)?;
        Ok(Self{used,current,correct,answered:0,score:0,started:false})
    }
    pub fn answer(&mut self,index:usize,questions:&[ExamQuestion],rng:&mut ReplayRng)->Result<bool,String>{
        if !self.started || self.answered>=10 || index>=4{return Err("考试答案无效。".into());}
        let correct=index==self.correct;if correct{self.score+=10;}
        // 0x0043ed90 loads the next question even after answer ten.
        let(next,answer)=Self::draw(questions,&mut self.used,rng)?;self.current=next;self.correct=answer;self.answered+=1;
        Ok(correct)
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct CareerPrerequisite{pub id:i16,pub level:i16}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct CareerRule{
    pub id:usize,pub name:String,pub morality:i16,pub intelligence:i16,pub stamina:i16,pub charm:i16,pub money:i32,
    pub degree_required:u8,pub prerequisites:Vec<CareerPrerequisite>,
    pub promotion_increment:i16,pub promotion_item:i8,pub base_salary:i16,
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct PaydayOutcome{pub previous_level:i16,pub level:i16,pub salary:i32,pub spouse_salary:i32,pub total:i32,pub mastered:bool}

/// FUN_00423780 selects promotion/demotion before paying the new rank's salary.
/// Promotion equipment is checked, not consumed; careers 8, 13 and 15 retain
/// the original second-item and sex-specific exceptions.
pub fn payday_rank(player:&LegacyPlayer,career:Option<&CareerRule>,special:&[i16])->Result<i16,String>{
    let rank=player.life().profession_level;
    if [-1,24].contains(&player.life().profession){return Ok(rank);}
    let career=career.ok_or("Missing original career promotion rule")?;
    if i16::try_from(career.id).ok()!=Some(player.life().profession){return Err("Career promotion rule does not match player".into());}
    let requirements=[(0x4b6490,career.morality),(0x4b6492,career.intelligence),(0x4b648e,career.stamina),(0x4b6494,career.charm)];
    let meets=|offset:i16|->Result<bool,String>{
        for(address,base)in requirements{if base!=0 && i32::from(player.i16(address)?)<i32::from(base)+i32::from(offset){return Ok(false);}}
        Ok(true)
    };
    if !meets(rank.wrapping_sub(1).wrapping_mul(career.promotion_increment))?{return Ok(if rank>1{rank-1}else{rank});}
    if rank>=5 || !meets(rank.wrapping_mul(career.promotion_increment))?{return Ok(rank);}
    if rank>1 && player.i16(0x4b64e4)?==0{
        let primary=usize::try_from(career.promotion_item).map_err(|_|"Invalid original promotion item")?;
        let has=|index:usize|special.get(index).copied().unwrap_or(0)>0;
        let available=if has(primary){career.id!=8 || has(primary+1)}else{
            (career.id==13 && player.i16(0x4b6498)?==0 || career.id==15) && has(primary+1)
        };
        if !available{return Ok(rank);}
    }
    Ok(rank.wrapping_add(1))
}

/// FUN_00424210 mode zero, including signed x86 multiplication and doubled
/// landing pay. Skill trigger four runs in the host after this money write.
pub fn collect_payday(player:&mut LegacyPlayer,rank:i16,base_salary:i16,multipliers:&[i32],spouse_salary:i16,landed:bool)->Result<PaydayOutcome,String>{
    let multiplier=*multipliers.get(usize::try_from(rank).map_err(|_|"Invalid salary rank")?).ok_or("Missing original salary multiplier")?;
    let previous_level=player.life().profession_level;
    let salary=multiplier.wrapping_mul(i32::from(base_salary))/10;
    let spouse_salary=i32::from(spouse_salary);
    let scale=if landed{2}else{1};
    let total=salary.wrapping_add(spouse_salary).wrapping_mul(scale);
    let mastered=rank==5 && previous_level==4;
    player.set_i16(0x4b64a6,rank)?;
    if mastered{player.add_i16(0x4b64f2,1)?;}
    player.add_i32(0x4b648a,total)?;
    Ok(PaydayOutcome{previous_level,level:rank,salary:salary.wrapping_mul(scale),spouse_salary:spouse_salary.wrapping_mul(scale),total,mastered})
}
impl CareerRule{
    /// FUN_0042d4b0, including mastered-career exclusion and diploma gates.
    pub fn eligible(&self,player:&LegacyPlayer,levels:&[i16])->Result<bool,String>{
        if self.id>=24 || levels.get(self.id).copied().ok_or("Missing original career history")?==5 || player.life().profession==self.id as i16{return Ok(false);}
        if player.i16(0x4b6490)?<self.morality || player.i16(0x4b6492)?<self.intelligence || player.i16(0x4b648e)?<self.stamina || player.i16(0x4b6494)?<self.charm || player.i32(0x4b648a)?<self.money{return Ok(false);}
        if self.degree_required!=0 && !(self.degree_required==1 && player.i16(0x4b64b8)?==1){return Ok(false);}
        for required in &self.prerequisites{if required.id!=-1 && levels.get(required.id as usize).copied().ok_or("Invalid original career prerequisite")?<required.level{return Ok(false);}}
        Ok(true)
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct StatShopItem {pub resource:String,pub price:i32,pub amount:i32,pub name:String}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct StatShopOffer {pub item:StatShopItem,pub group:usize,pub bought:bool}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct StatShop {pub offers:Vec<StatShopOffer>,pub purchases:u8}

impl StatShop {
    /// FUN_0042b930: two distinct products in each of four attribute groups.
    pub fn generate(groups:&[Vec<StatShopItem>],rng:&mut ReplayRng)->Result<Self,String>{
        if groups.len()<4 || groups[..4].iter().any(|items|items.len()<2){return Err("Missing original attribute-shop catalog".into());}
        let mut offers=Vec::new();
        for(group,items)in groups[..4].iter().enumerate(){
            let first=rng.next_rand()as usize%items.len();
            let mut second=rng.next_rand()as usize%items.len();
            while second==first{second=rng.next_rand()as usize%items.len();}
            for index in [first,second]{offers.push(StatShopOffer{item:items[index].clone(),group,bought:false});}
        }
        Ok(Self{offers,purchases:0})
    }
    /// Every third adult visit replaces two distinct slots with romantic gifts.
    /// Without any original relationship candidate, only the first three
    /// (flower) products are eligible. The eight ordinary draws still occur.
    pub fn generate_for_visit(groups:&[Vec<StatShopItem>],special_stock:bool,has_candidate:bool,rng:&mut ReplayRng)->Result<Self,String>{
        let mut shop=Self::generate(groups,rng)?;
        if special_stock{
            let items=groups.get(4).ok_or("Missing original gift shop catalog")?;
            let count=if has_candidate{items.len()}else{3};
            if count<2 || items.len()<count{return Err("Missing original gift products".into());}
            let first=rng.next_rand()as usize%8;let mut second=rng.next_rand()as usize%8;
            while second==first{second=rng.next_rand()as usize%8;}
            let first_item=rng.next_rand()as usize%count;let mut second_item=rng.next_rand()as usize%count;
            while second_item==first_item{second_item=rng.next_rand()as usize%count;}
            for(slot,item)in[(first,first_item),(second,second_item)]{shop.offers[slot]=StatShopOffer{item:items[item].clone(),group:4,bought:false};}
        }
        Ok(shop)
    }
    /// FUN_0042c620/0042ce40. Three purchases per visit; already-bought products
    /// remain unavailable. Attribute increments are limited before animation.
    pub fn purchase(&mut self,index:usize,player:&mut LegacyPlayer)->Result<Option<u8>,String>{
        if self.purchases>=3{return Err("本次购物已购买三件商品。".into());}
        let offer=self.offers.get_mut(index).ok_or("商品不存在。")?;
        if offer.bought{return Err("此商品已售出。".into());}
        if player.i32(0x4b648a)?<offer.item.price{return Err("持有金不足。".into());}
        let mut gift=None;
        if offer.group==4 && offer.item.amount<=0{gift=Some((-(offer.item.amount as i8)).min(4)as u8);}
        else{
            let address=*[0x4b6494,0x4b6492,0x4b648e,0x4b6490,0x4b6496].get(offer.group).ok_or("Unknown original shop group")?;
            let amount=offer.item.amount as i16;let cap=if offer.group==4{99}else{9999};
            player.set_i16(address,(i32::from(player.i16(address)?)+i32::from(amount)).min(cap)as i16)?;
        }
        player.add_i32(0x4b648a,-offer.item.price)?;
        offer.bought=true;self.purchases+=1;Ok(gift)
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct StudentReward {pub points:u32,pub money:i32}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct Allocation {pub intelligence:u32,pub morality:u32,pub charm:u32,pub stamina:u32}

impl StudentReward {
    /// 0x00420840 / 0x00420340: start-tile reward and explicit attribute allocation.
    pub fn calculate(player:&LegacyPlayer,landed:bool)->Result<Self,String> {
        let stage_turn=player.i16(0x4b64a8)?;
        let battles=i32::from(player.i16(0x4b64bc)?);
        let wins=i32::from(player.i16(0x4b64be)?);
        let win_percent=if battles==0{0}else{wins*100/battles};
        let(mut points,mut money)=(300,2000);
        if stage_turn<15{points+=200;money+=12000;}else if stage_turn<=24{points+=200;money+=5000;}
        if win_percent==100{points+=300;money+=15000;}
        else if win_percent>=60{points+=200;money+=7500;}
        else if win_percent>=30{points+=100;money+=2500;}
        let bonus=if battles>25{(200,16000)}else if battles>20{(200,12500)}else if battles>15{(200,9000)}else if battles>10{(100,6000)}else if battles>6{(100,4000)}else if battles>4{(100,2500)}else{(0,0)};
        points+=bonus.0;money+=bonus.1;
        if landed{points*=2;money*=2;}
        Ok(Self{points,money})
    }
    pub fn default_allocation(&self)->Allocation {let quarter=self.points/4;Allocation{intelligence:quarter,morality:quarter,charm:quarter,stamina:self.points-quarter*3}}
    pub fn apply(&self,player:&mut LegacyPlayer,allocation:&Allocation)->Result<(),String> {
        let values=[allocation.intelligence,allocation.morality,allocation.charm,allocation.stamina];
        let total=values.iter().try_fold(0u32,|sum,value|sum.checked_add(*value)).ok_or("属性点总数溢出。")?;
        if total!=self.points{return Err(format!("请分配全部 {} 点成长点数。",self.points));}
        player.set_i32(0x4b648a,player.i32(0x4b648a)?.wrapping_add(self.money).min(99_999_999))?;
        for(address,value)in[0x4b6492,0x4b6490,0x4b6494,0x4b648e].into_iter().zip(values){
            player.set_i16(address,player.i16(address)?.wrapping_add(value as i16).min(9999))?;
        }
        player.set_i16(0x4b64bc,0)?;player.set_i16(0x4b64be,0)?;
        if player.i16(0x4b64cc)? != -1 && player.i16(0x4b64ce)?==9{player.set_i16(0x4b64d0,1)?;}
        Ok(())
    }
}

/// N2Game.exe 0x00414cb0: a forced result bypasses the random draw.
pub fn roll_dice(rng:&mut ReplayRng,forced:Option<u8>)->Result<u8,String> {
    match forced {
        Some(value @ 1..=8)=>Ok(value),
        Some(_)=>Err("Forced die result must be 1–8".into()),
        None=>Ok((rng.next_rand()%8+1)as u8),
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct Movement {
    pub path:Vec<usize>,
    pub reached_end:bool,
    pub entry_step_pending:bool,
}

/// N2Game.exe 0x004150a0 and map layout reader 0x00416e40.
/// An ordinary step draws even on a single-link tile. The first entry step does
/// not draw. On absent selected links, the original uses the other link.
/// Animation directions, passed-tile effects and map changes belong to the caller.
pub fn plan_movement(map:&Map,start:usize,steps:u8,mut entry_step_pending:bool,rng:&mut ReplayRng)->Result<Movement,String> {
    if steps==0 || steps>8 {return Err("Movement must contain 1–8 die steps".into());}
    let mut current=start; let mut path=Vec::new();
    for _ in 0..steps {
        let tile=map.chips.get(current).ok_or("Invalid movement start")?;
        let preferred=if entry_step_pending {entry_step_pending=false;0}else{(rng.next_rand()%2)as usize};
        let Some(next)=tile.next.get(preferred).or_else(||tile.next.first()) else {
            return Ok(Movement{path,reached_end:true,entry_step_pending:true});
        };
        current=map.tile_index(next).ok_or("Original map links to a missing tile")?;
        path.push(current);
    }
    let reached_end=map.chips[current].next.is_empty();
    Ok(Movement{path,reached_end,entry_step_pending:entry_step_pending||reached_end})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shop_stock_matches_original_x86_draw_order_and_gift_restrictions(){
        let rules:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-shops.json")).unwrap();
        for case in vectors["cases"].as_array().unwrap(){
            let young=case["stage"].as_i64().unwrap()<=3;
            let groups:Vec<Vec<StatShopItem>>=serde_json::from_value(rules["statShops"][if young{"young"}else{"adult"}].clone()).unwrap();
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            let shop=StatShop::generate_for_visit(&groups,!young && case["previousVisits"].as_i64()==Some(2),case["hasCandidate"].as_bool().unwrap(),&mut rng).unwrap();
            for (offer,expected)in shop.offers.iter().zip(case["offers"].as_array().unwrap()){
                let group=expected["group"].as_u64().unwrap()as usize;let index=expected["index"].as_u64().unwrap()as usize;
                assert_eq!(offer.group,group,"{case}");assert_eq!(offer.item,groups[group][index],"{case}");
            }
            assert_eq!(rng.draws,case["draws"].as_u64().unwrap(),"{case}");
        }
    }
    #[test]
    fn shop_flowers_replace_gift_level_and_love_products_cap_at_99(){
        let mut player=LegacyPlayer::initialize();player.set_i32(0x4b648a,100000).unwrap();player.set_i16(0x4b6496,98).unwrap();
        let flower=StatShopItem{resource:"q/aa35.bmp".into(),price:9000,amount:-4,name:"玫瑰".into()};
        let card=StatShopItem{resource:"q/aa38.bmp".into(),price:4000,amount:5,name:"生日贺卡".into()};
        let mut shop=StatShop{offers:vec![StatShopOffer{item:flower,group:4,bought:false},StatShopOffer{item:card,group:4,bought:false}],purchases:0};
        assert_eq!(shop.purchase(0,&mut player).unwrap(),Some(4));assert_eq!(player.life().affection,98);
        assert_eq!(shop.purchase(1,&mut player).unwrap(),None);assert_eq!(player.life().affection,99);assert_eq!(player.stats().money,Some(87000));
        assert!(shop.purchase(0,&mut player).is_err());
    }
    #[test]
    fn payday_promotion_and_salary_match_original_machine_code(){
        let rules:serde_json::Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        let careers:Vec<CareerRule>=serde_json::from_value(rules["careers"].clone()).unwrap();
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-payday.json")).unwrap();
        for case in vectors["promotion"].as_array().unwrap(){
            let mut player=LegacyPlayer::initialize();let career=case["career"].as_i64().unwrap()as i16;
            player.set_i16(0x4b64a2,career).unwrap();player.set_i16(0x4b64a6,case["rank"].as_i64().unwrap()as i16).unwrap();
            player.set_i16(0x4b6498,case["sex"].as_i64().unwrap()as i16).unwrap();player.set_i16(0x4b64e4,case["override"].as_i64().unwrap()as i16).unwrap();
            for(address,value)in [0x4b6490,0x4b6492,0x4b648e,0x4b6494].into_iter().zip(case["stats"].as_array().unwrap()){
                player.set_i16(address,value.as_i64().unwrap()as i16).unwrap();
            }
            let special:Vec<i16>=serde_json::from_value(case["special"].clone()).unwrap();
            let actual=payday_rank(&player,careers.iter().find(|rule|rule.id as i16==career),&special).unwrap();
            assert_eq!(i64::from(actual),case["expectedRank"].as_i64().unwrap(),"{}",case["name"]);
        }
        let multipliers:Vec<i32>=serde_json::from_value(rules["salaryMultipliers"].clone()).unwrap();
        for case in vectors["salary"].as_array().unwrap(){
            let mut player=LegacyPlayer::initialize();let career=case["career"].as_i64().unwrap()as i16;let rank=case["rank"].as_i64().unwrap()as i16;
            player.set_i16(0x4b64a2,career).unwrap();player.set_i16(0x4b64a6,rank).unwrap();player.set_i32(0x4b648a,case["initialMoney"].as_i64().unwrap()as i32).unwrap();
            let base=if career==-1{vectors["unemployedBaseSalary"].as_i64().unwrap()}else{rules["baseSalaries"][career as usize].as_i64().unwrap()}as i16;
            let spouse=case["spouse"].as_i64().unwrap();let sex=case["sex"].as_i64().unwrap();
            let spouse_salary=if spouse==-1{0}else{rules["spouseSalary"][(spouse%8+sex*8)as usize].as_i64().unwrap()as i16};
            collect_payday(&mut player,rank,base,&multipliers,spouse_salary,case["landed"].as_bool().unwrap()).unwrap();
            assert_eq!(i64::from(player.i32(0x4b648a).unwrap()),case["expectedMoney"].as_i64().unwrap(),"{case}");
        }
    }
    fn map()->Map {serde_json::from_str(r#"{"id":"fixture","start":{"row":0,"column":0},"chips":[{"row":0,"column":0,"eventId":0,"next":[{"row":1,"column":1}]},{"row":1,"column":1,"eventId":1,"next":[{"row":2,"column":0},{"row":2,"column":2}]},{"row":2,"column":0,"eventId":2,"next":[]},{"row":2,"column":2,"eventId":3,"next":[]}]}"#).unwrap()}
    #[test]
    fn dice_and_branch_draw_order_matches_original() {
        let mut rng=ReplayRng::new(1);
        let steps=roll_dice(&mut rng,None).unwrap();assert_eq!(steps,2);
        let movement=plan_movement(&map(),0,steps,true,&mut rng).unwrap();
        assert_eq!(movement.path,[1,3]);assert!(movement.reached_end);assert_eq!(rng.draws,2);
    }
    #[test]
    fn forced_die_bypasses_rng_and_single_link_still_draws() {
        let mut rng=ReplayRng::new(1);
        assert_eq!(roll_dice(&mut rng,Some(1)).unwrap(),1);assert_eq!(rng.draws,0);
        let movement=plan_movement(&map(),0,1,false,&mut rng).unwrap();
        assert_eq!(movement.path,[1]);assert_eq!(rng.draws,1);
    }
    #[test]
    fn original_start_reward_requires_allocation_and_resets_battle_counts() {
        let mut player=LegacyPlayer::initialize();let reward=StudentReward::calculate(&player,true).unwrap();
        assert_eq!(reward,StudentReward{points:1000,money:28000});
        let before=player.clone();assert!(reward.apply(&mut player,&Allocation{intelligence:0,morality:0,charm:0,stamina:0}).is_err());assert_eq!(before,player);
        reward.apply(&mut player,&reward.default_allocation()).unwrap();
        assert_eq!(player.stats().intelligence,Some(350));assert_eq!(player.stats().money,Some(38000));
        player.set_i16(0x4b64bc,26).unwrap();player.set_i16(0x4b64be,26).unwrap();
        assert_eq!(StudentReward::calculate(&player,false).unwrap(),StudentReward{points:1000,money:45000});
    }
}
