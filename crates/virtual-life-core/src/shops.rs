//! Original item shop (0x004399b0/0x0043a7b0) and housing (0x0042a550).
use crate::{character::LegacyPlayer,equipment::EquipmentDefinition,original_rules::{EquipmentKnowledge,EquipmentSlot},rng::ReplayRng};
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct NativeItem{pub index:usize,pub inventory_id:i16,pub price:i32,pub description:String,pub resource:String}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct Furniture{pub index:usize,pub price:i32,pub description:String,pub resource:String,pub bonus_amount:i16,pub bonus_attribute:i16}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct ItemShopCatalog{pub ordinary_young:Vec<NativeItem>,pub ordinary_adult:Vec<NativeItem>,pub special_young:Vec<NativeItem>,pub special_adult:Vec<NativeItem>,pub furniture:Vec<Furniture>}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct HouseStyle{pub id:usize,pub description:String,pub price:i32,pub bonuses:[i16;4]}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct HousingCatalog{pub styles:Vec<HouseStyle>,pub investment_rows:Vec<[i32;5]>,pub stage_descriptions:Vec<String>}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct HousingState{pub style:i8,pub furniture:i16,pub investment_stage:u8,pub purchase_years:Vec<i16>}
impl Default for HousingState{fn default()->Self{Self{style:-1,furniture:0,investment_stage:0,purchase_years:vec![]}}}
impl HousingState{
    pub fn buy(&mut self,style:&HouseStyle,player:&mut LegacyPlayer)->Result<(),String>{
        if self.investment_stage!=0 || self.style!=-1 || style.id>3{return Err("住宅购买状态无效。".into());}
        // The original comparison is strict: an exact-price balance fails.
        if player.i32(0x4b648a)?<=style.price{return Err("持有金必须高于房价。".into());}
        player.add_i32(0x4b648a,-style.price)?;
        self.style=style.id as i8;self.investment_stage=1;self.purchase_years.push(player.life().year);
        for(address,bonus)in [0x4b6490,0x4b6492,0x4b648e,0x4b6494].into_iter().zip(style.bonuses){
            player.set_i16(address,(i32::from(player.i16(address)?)+i32::from(bonus)).min(9999)as i16)?;
        }
        Ok(())
    }
    pub fn next_investment(&self,catalog:&HousingCatalog)->Result<i32,String>{
        if !(1..13).contains(&self.investment_stage){return Err("当前没有新的房产投资。".into());}
        let index=usize::from(self.investment_stage-1);
        Ok(catalog.investment_rows.get(index/3).ok_or("Missing original investment row")?[index%3])
    }
    pub fn invest(&mut self,catalog:&HousingCatalog,player:&mut LegacyPlayer)->Result<i32,String>{
        let price=self.next_investment(catalog)?;
        if player.i32(0x4b648a)?<=price{return Err("持有金必须高于本期投资额。".into());}
        player.add_i32(0x4b648a,-price)?;
        if self.investment_stage>1 && (self.investment_stage-1)%3==0{self.purchase_years.push(player.life().year);}
        self.investment_stage+=1;Ok(price)
    }
    pub fn income(&self,catalog:&HousingCatalog)->Result<i32,String>{
        if !(2..=13).contains(&self.investment_stage){return Err("当前没有可以领取的房产收益。".into());}
        let progress=usize::from(self.investment_stage-1);let completed=progress/3;let mut income=0i32;
        for row in catalog.investment_rows.iter().take(completed){income=income.wrapping_add(row[0].wrapping_add(row[1]).wrapping_add(row[2]).wrapping_mul(row[4])/100);}
        if progress%3!=0{
            let row=catalog.investment_rows.get(completed).ok_or("Missing original investment row")?;
            let partial=if progress%3==1{row[0].wrapping_mul(row[3])/100}else{row[4].wrapping_mul(row[0]).wrapping_mul(2)/100};
            income=income.wrapping_add(partial);
        }
        Ok(income)
    }
    pub fn collect_income(&self,catalog:&HousingCatalog,player:&mut LegacyPlayer)->Result<i32,String>{
        let income=self.income(catalog)?;player.add_i32(0x4b648a,income)?;Ok(income)
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct NativeShopOffer{
    /// 0 furniture, 1 equipment retrieval, 2 ordinary items, 3 career items.
    pub category:u8,pub catalog_index:usize,pub inventory_id:i16,pub price:i32,pub description:String,pub resource:String,pub purchased:bool,
    pub bonus_attribute:Option<i16>,pub bonus_amount:i16,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct NativeShop{pub offers:Vec<NativeShopOffer>}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="camelCase")]
pub struct NativePurchase{pub category:u8,pub inventory_id:i16,pub price:i32,pub received:bool}

impl NativeShop{
    pub fn generate(catalog:&ItemShopCatalog,player:&LegacyPlayer,housing:&HousingState,known:&[EquipmentKnowledge],carried:&[EquipmentSlot],definitions:&[EquipmentDefinition],rng:&mut ReplayRng)->Result<Self,String>{
        let young=player.life().stage<4;
        let ordinary=if young{&catalog.ordinary_young}else{&catalog.ordinary_adult};
        let special=if young{&catalog.special_young}else{&catalog.special_adult};
        let ordinary_width=ordinary.len()/3;let special_width=special.len()/3;
        if ordinary_width==0 || special_width==0{return Err("Missing original item-shop products".into());}
        let mut ordinary_indices=[0usize;3];let mut special_indices=[0usize;3];
        for slot in 0..3{
            ordinary_indices[slot]=rng.next_rand()as usize%ordinary_width+ordinary_width*slot;
            let mut index=rng.next_rand()as usize%special_width+special_width*slot;
            if index==14 && player.i16(0x4b6498)?==0{index=15;}
            if index==15 && player.i16(0x4b6498)?==1{index=14;}
            special_indices[slot]=index;
        }
        let career=player.life().profession;
        match career{
            -1|24=>{},0..=7=>special_indices[0]=career as usize,
            8=>{special_indices[0]=8;special_indices[1]=9;},9..=12=>special_indices[0]=career as usize+1,
            13=>special_indices[0]=if player.i16(0x4b6498)?==1{14}else{15},
            14=>special_indices[0]=16,15=>{special_indices[0]=17;special_indices[1]=18;},16..=23=>special_indices[0]=career as usize+3,
            _=>return Err("Invalid original item-shop career".into()),
        }
        let mut offers=Vec::new();
        if housing.style>=0 && (housing.style<3 && housing.furniture<=14 || housing.style==3 && housing.furniture<14){
            let index=housing.style as usize*15+housing.furniture as usize;
            let item=catalog.furniture.get(index).ok_or("Missing original furniture offer")?;
            offers.push(NativeShopOffer{category:0,catalog_index:index,inventory_id:-1,price:item.price,description:item.description.clone(),resource:item.resource.clone(),purchased:false,bonus_attribute:Some(item.bonus_attribute),bonus_amount:item.bonus_amount});
        }
        for (id,item)in known.iter().enumerate(){
            if item.learned && item.level==0 && !carried.iter().any(|slot|slot.id==id as i16){
                let definition=definitions.iter().find(|definition|definition.id==id).ok_or("Missing original carried-equipment definition")?;
                offers.push(NativeShopOffer{category:1,catalog_index:id,inventory_id:id as i16,price:0,description:format!("领取{}\n{}",definition.name,definition.description),resource:definition.icon.clone(),purchased:false,bonus_attribute:None,bonus_amount:0});
            }
        }
        for(category,items,indices)in[(2,ordinary,ordinary_indices),(3,special,special_indices)]{
            for index in indices{
                let item=items.get(index).ok_or("Original item-shop index exceeds its product table")?;
                offers.push(NativeShopOffer{category,catalog_index:index,inventory_id:item.inventory_id,price:item.price,description:item.description.clone(),resource:item.resource.clone(),purchased:false,bonus_attribute:None,bonus_amount:0});
            }
        }
        Ok(Self{offers})
    }
    pub fn purchase(&mut self,index:usize,player:&mut LegacyPlayer,housing:&mut HousingState,ordinary:&mut [i16],special:&mut [i16],known:&[EquipmentKnowledge],carried:&mut [EquipmentSlot])->Result<NativePurchase,String>{
        let offer=self.offers.get_mut(index).ok_or("此商品不存在。")?;
        if offer.purchased{return Err("此商品已售出。".into());}
        if player.i32(0x4b648a)?<offer.price{return Err("持有金不足。".into());}
        let mut received=true;
        match offer.category{
            0=>{
                if housing.style<0 || housing.style as usize*15+housing.furniture as usize!=offer.catalog_index{return Err("家具顺序与住宅状态不符。".into());}
                housing.furniture+=1;
                // Unrecognized final table words are ignored by the original
                // switch, including its last Chinese-style furniture entry.
                if let Some(attribute @ 0..=3)=offer.bonus_attribute{
                    let address=[0x4b6490,0x4b6492,0x4b648e,0x4b6494][attribute as usize];
                    player.set_i16(address,(i32::from(player.i16(address)?)+i32::from(offer.bonus_amount)).min(9999)as i16)?;
                }
            }
            1=>{
                if let Some(slot)=carried.iter_mut().find(|slot|slot.id==-1){
                    *slot=EquipmentSlot{id:offer.inventory_id,level:i16::from(known.get(offer.inventory_id as usize).ok_or("Missing equipment ownership")?.level)};
                }else{received=false;}
            }
            2|3=>{
                let inventory=if offer.category==2{ordinary}else{special};
                if inventory.iter().map(|amount|i32::from(*amount)).sum::<i32>()>=8{return Err("这类道具的携带数量已达八件。".into());}
                let quantity=inventory.get_mut(offer.inventory_id as usize).ok_or("Invalid original inventory item")?;*quantity=quantity.wrapping_add(1);
            }
            _=>return Err("Unknown original item-shop category".into()),
        }
        player.add_i32(0x4b648a,-offer.price)?;offer.purchased=true;
        Ok(NativePurchase{category:offer.category,inventory_id:offer.inventory_id,price:offer.price,received})
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn rules()->serde_json::Value{serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap()}
    #[test]
    fn item_shop_and_property_income_match_original_machine_code(){
        let rules=rules();let catalog:ItemShopCatalog=serde_json::from_value(rules["itemShop"].clone()).unwrap();
        let definitions:Vec<EquipmentDefinition>=serde_json::from_value(rules["equipment"].clone()).unwrap();
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-native-shops.json")).unwrap();
        for case in vectors["stock"].as_array().unwrap(){
            let mut player=LegacyPlayer::initialize();player.set_i16(0x4b649e,case["stage"].as_i64().unwrap()as i16).unwrap();player.set_i16(0x4b64a2,case["career"].as_i64().unwrap()as i16).unwrap();player.set_i16(0x4b6498,case["sex"].as_i64().unwrap()as i16).unwrap();
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            let shop=NativeShop::generate(&catalog,&player,&HousingState::default(),&vec![EquipmentKnowledge{learned:false,level:0};18],&vec![EquipmentSlot{id:-1,level:0};8],&definitions,&mut rng).unwrap();
            assert_eq!(shop.offers.iter().filter(|offer|offer.category==2).map(|offer|offer.catalog_index).collect::<Vec<_>>(),serde_json::from_value::<Vec<usize>>(case["ordinary"].clone()).unwrap(),"{case}");
            assert_eq!(shop.offers.iter().filter(|offer|offer.category==3).map(|offer|offer.catalog_index).collect::<Vec<_>>(),serde_json::from_value::<Vec<usize>>(case["special"].clone()).unwrap(),"{case}");
            assert_eq!(rng.draws,case["draws"].as_u64().unwrap());
        }
        let catalog:HousingCatalog=serde_json::from_value(rules["housing"].clone()).unwrap();
        for case in vectors["income"].as_array().unwrap(){
            let mut player=LegacyPlayer::initialize();player.set_i32(0x4b648a,case["initialMoney"].as_i64().unwrap()as i32).unwrap();
            let housing=HousingState{style:0,furniture:0,investment_stage:case["stage"].as_u64().unwrap()as u8,purchase_years:vec![]};
            housing.collect_income(&catalog,&mut player).unwrap();
            assert_eq!(i64::from(player.i32(0x4b648a).unwrap()),case["expectedMoney"].as_i64().unwrap(),"{case}");
        }
    }
    #[test]
    fn housing_preserves_exact_price_rejection_and_repeated_rental_income(){
        let catalog:HousingCatalog=serde_json::from_value(rules()["housing"].clone()).unwrap();let mut housing=HousingState::default();let mut player=LegacyPlayer::initialize();
        player.set_i32(0x4b648a,catalog.styles[0].price).unwrap();assert!(housing.buy(&catalog.styles[0],&mut player).is_err());
        player.add_i32(0x4b648a,1).unwrap();housing.buy(&catalog.styles[0],&mut player).unwrap();assert_eq!(player.stats().money,Some(1));
        let price=housing.next_investment(&catalog).unwrap();player.set_i32(0x4b648a,price+1).unwrap();housing.invest(&catalog,&mut player).unwrap();
        assert_eq!(housing.investment_stage,2);let income=housing.collect_income(&catalog,&mut player).unwrap();housing.collect_income(&catalog,&mut player).unwrap();assert_eq!(player.stats().money,Some(1+i64::from(income)*2));
    }
    #[test]
    fn item_purchase_rejects_capacity_and_equipment_retrieval_uses_free_fixed_slot(){
        let rules=rules();let catalog:ItemShopCatalog=serde_json::from_value(rules["itemShop"].clone()).unwrap();let definitions:Vec<EquipmentDefinition>=serde_json::from_value(rules["equipment"].clone()).unwrap();
        let mut known=vec![EquipmentKnowledge{learned:false,level:0};18];known[0].learned=true;let mut carried=vec![EquipmentSlot{id:-1,level:0};8];let mut player=LegacyPlayer::initialize();player.set_i32(0x4b648a,1_000_000).unwrap();
        let mut housing=HousingState::default();let mut ordinary=vec![0;13];ordinary[0]=8;let mut special=vec![0;27];
        let mut shop=NativeShop::generate(&catalog,&player,&housing,&known,&carried,&definitions,&mut ReplayRng::new(1)).unwrap();
        let equipment=shop.offers.iter().position(|offer|offer.category==1).unwrap();let item=shop.offers.iter().position(|offer|offer.category==2).unwrap();let initial=player.clone();
        assert!(shop.purchase(item,&mut player,&mut housing,&mut ordinary,&mut special,&known,&mut carried).is_err());assert_eq!(player,initial);
        assert!(shop.purchase(equipment,&mut player,&mut housing,&mut ordinary,&mut special,&known,&mut carried).unwrap().received);assert_eq!(carried[0],EquipmentSlot{id:0,level:0});assert_eq!(player,initial);
    }
}
