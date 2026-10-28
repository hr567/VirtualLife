//! The three native item panels share the modal selection and preview clocks.
use crate::inventory::InventoryRules;
use crate::original_rules::EquipmentSlot;
use serde::{Deserialize,Serialize};

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum InventoryTab {Ordinary,Special,Equipment}
impl InventoryTab {
    pub fn parse(value:&str)->Result<Self,String>{match value{"ordinary"=>Ok(Self::Ordinary),"special"=>Ok(Self::Special),"equipment"=>Ok(Self::Equipment),_=>Err("物品分类不存在。".into())}}
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct InventoryEntry {pub id:usize,pub slot:Option<usize>,pub equipped:bool,pub frame:u32,pub last_tick:u32,pub first:bool}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct InventoryTrack {pub frame:u32,pub last_tick:u32,pub first:bool}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct PreviewOrigin {pub x:i32,pub y:i32}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct InventoryPanel {pub tab:InventoryTab,pub selected_index:Option<usize>,pub confirm_drop:bool,pub clock_ms:u32,pub entries:Vec<InventoryEntry>,pub preview_origin:PreviewOrigin,pub back_tracks:[InventoryTrack;2]}
impl InventoryPanel {
    pub fn new(tab:InventoryTab,ordinary:&[i16],special:&[i16],carried:&[EquipmentSlot],equipped:&[i16])->Self{
        let mut state=Self{tab,selected_index:Some(0),confirm_drop:false,clock_ms:0,entries:vec![],preview_origin:PreviewOrigin{x:if tab==InventoryTab::Equipment{312}else{302},y:47},back_tracks:[InventoryTrack{frame:0,last_tick:0,first:true},InventoryTrack{frame:0,last_tick:0,first:true}]};state.rebuild(ordinary,special,carried,equipped);state
    }
    pub fn rebuild(&mut self,ordinary:&[i16],special:&[i16],carried:&[EquipmentSlot],equipped:&[i16]){
        self.entries.clear();
        if self.tab==InventoryTab::Equipment{for(slot,item)in carried.iter().enumerate(){if item.id>=0{let id=item.id as usize;self.entries.push(InventoryEntry{id,slot:Some(slot),equipped:equipped.get(id/3)==Some(&item.id),frame:0,last_tick:0,first:true});}}}
        else{let counts=if self.tab==InventoryTab::Ordinary{ordinary}else{special};for(id,count)in counts.iter().enumerate(){for _ in 0..(*count).max(0){self.entries.push(InventoryEntry{id,slot:None,equipped:false,frame:0,last_tick:0,first:true});}}}
        self.selected_index=if self.entries.is_empty(){None}else{Some(self.selected_index.unwrap_or(0).min(self.entries.len()-1))};
    }
    pub fn selected_id(&self)->Result<usize,String>{self.selected_index.and_then(|index|self.entries.get(index)).map(|entry|entry.id).ok_or_else(||"请先选择一件物品。".into())}
    pub fn tick(&mut self,elapsed_ms:u32,rules:&InventoryRules){
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);if self.confirm_drop{return;}
        for(index,track)in self.back_tracks.iter_mut().enumerate(){if self.clock_ms.wrapping_sub(track.last_tick)>114{track.last_tick=self.clock_ms;if track.first{track.first=false;}else{track.frame=(track.frame+1)%[6,5][index];}}}
        let Some(entry)=self.selected_index.and_then(|index|self.entries.get_mut(index))else{return;};
        let definitions=match self.tab{InventoryTab::Ordinary=>&rules.ordinary,InventoryTab::Special=>&rules.special,InventoryTab::Equipment=>&rules.equipment};
        if self.clock_ms.wrapping_sub(entry.last_tick)>100{entry.last_tick=self.clock_ms;if entry.first{entry.first=false;}else{entry.frame=(entry.frame+1)%definitions[entry.id].frame_count.max(1);}}
    }
}
