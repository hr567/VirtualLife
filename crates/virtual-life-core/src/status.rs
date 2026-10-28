//! Read-only parameter overview arithmetic and labels from FUN_00401550.
use crate::{character::LegacyPlayer, shops::HousingState,rng::ReplayRng,transition::NativeTransition};
use serde::{Deserialize,Serialize};
use serde_json::{Value,json};

fn integer(table:&Value,key:&str,index:usize)->Result<i32,String>{
    table[key][index].as_i64().map(|v|v as i32).ok_or_else(||format!("Missing original status table {key}[{index}]"))
}
fn string(table:&Value,key:&str,index:usize)->Result<String,String>{
    table[key][index].as_str().map(String::from).ok_or_else(||format!("Missing original status label {key}[{index}]"))
}
/// This is the overview's displayed return, including its original use of the
/// first principal for both unfinished installments. Collection uses a different
/// original routine and must keep its own arithmetic.
pub fn investment_return(stage:u8,rows:&Value)->Result<i32,String>{
    if stage<2{return Ok(0);}
    let progress=usize::from(stage-1);let completed=progress/3;let mut value=0i32;
    let row=|i:usize,j:usize|rows[i][j].as_i64().map(|n|n as i32).ok_or("Invalid overview investment row".to_string());
    for i in 0..completed{value=value.wrapping_add(row(i,0)?.wrapping_add(row(i,1)?).wrapping_add(row(i,2)?).wrapping_mul(row(i,4)?)/100);}
    match progress%3{
        1=>value=value.wrapping_add(row(completed,0)?.wrapping_mul(row(completed,3)?)/100),
        2=>value=value.wrapping_add(row(completed,0)?.wrapping_mul(row(completed,4)?).wrapping_mul(2)/100),
        _=>{},
    }
    Ok(value)
}

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub enum StatusPage {Overview,House,Investment,CareerHistory,Skills}
impl StatusPage {pub fn parse(value:&str)->Result<Self,String>{match value{
    "overview"=>Ok(Self::Overview),"house"=>Ok(Self::House),"investment"=>Ok(Self::Investment),
    "careerHistory"=>Ok(Self::CareerHistory),"skills"=>Ok(Self::Skills),_=>Err("参数页面不存在。".into()),
}}}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct OverviewAnimation {pub clock_ms:u32,pub back_tracks:[crate::inventory_view::InventoryTrack;2]}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="camelCase")]
pub struct StatusPanel {pub page:StatusPage,pub scroll:usize,pub clock_ms:u32,pub back_tracks:[crate::inventory_view::InventoryTrack;2],
    #[serde(default,skip_serializing_if="Option::is_none")]pub transition:Option<NativeTransition>,
    #[serde(default,skip_serializing_if="Option::is_none")]pub overview_animation:Option<OverviewAnimation>}
impl Default for StatusPanel {fn default()->Self{Self{page:StatusPage::Overview,scroll:0,clock_ms:0,
    back_tracks:std::array::from_fn(|_|crate::inventory_view::InventoryTrack{frame:0,last_tick:0,first:true}),transition:None,overview_animation:None}}}
impl StatusPanel {
    pub fn page(&mut self,page:StatusPage,overview:&Value,rng:&mut ReplayRng)->Result<(),String>{
        if self.transition.is_some(){return Err("请等待住宅转场结束。".into());}
        if page==StatusPage::House&&overview["canInspectHouse"]!=true{return Err("尚未购买住宅。".into());}
        if page==StatusPage::Investment&&overview["canInspectInvestment"]!=true{return Err("尚未参与房产投资。".into());}
        if page==StatusPage::Overview{self.close();return Ok(());}
        // FUN_00401550 retains its local_a34 Back object while a child native
        // panel runs. Preserve its frame/first flag and the elapsed tick epoch.
        let overview_animation=if self.page==StatusPage::Overview{Some(OverviewAnimation{clock_ms:self.clock_ms,back_tracks:self.back_tracks.clone()})}
            else{self.overview_animation.take().map(|mut saved|{saved.clock_ms=saved.clock_ms.wrapping_add(self.clock_ms);saved})};
        *self=Self{page,overview_animation,..Self::default()};
        match page{
            StatusPage::House=>self.transition=Some(NativeTransition::rain(1,2,rng)),
            StatusPage::Investment=>{NativeTransition::mosaic(2,16,rng).tick(0,rng);},
            _=>{},
        }
        Ok(())
    }
    pub fn scroll(&mut self,delta:i32,index:Option<usize>,rows:usize)->Result<(),String>{
        let maximum=match self.page{StatusPage::CareerHistory=>rows.saturating_sub(10),StatusPage::Skills=>22,_=>return Err("此参数页面不含可滚动列表。".into())};
        self.scroll=index.unwrap_or_else(||self.scroll.saturating_add_signed(delta as isize)).min(maximum);Ok(())
    }
    /// A nested native panel closes back to the overview; its Back closes the modal.
    pub fn close(&mut self)->bool{
        if self.page==StatusPage::Overview{return true;}
        let elapsed=self.clock_ms;let overview=self.overview_animation.take();*self=Self::default();
        if let Some(saved)=overview{self.clock_ms=saved.clock_ms.wrapping_add(elapsed);self.back_tracks=saved.back_tracks;}
        false
    }
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng){
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        if let Some(transition)=self.transition.as_mut(){
            transition.tick(elapsed_ms,rng);
            if !transition.finished{return;}
            // 43b528 copies the full house, including its newest furniture,
            // after the falling-column preview. Mosaic has no native delay.
            NativeTransition::mosaic(2,16,rng).tick(0,rng);self.transition=None;
        }
        if matches!(self.page,StatusPage::House|StatusPage::Investment)&&self.clock_ms>8000{self.close();return;}
        for(index,track)in self.back_tracks.iter_mut().enumerate(){if self.clock_ms.wrapping_sub(track.last_tick)>114{
            track.last_tick=self.clock_ms;if track.first{track.first=false;}else{track.frame=(track.frame+1)%[6,5][index];}
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_overview_back_tracks_survive_child_panels_and_saved_timeouts(){
        let overview=json!({"canInspectHouse":true,"canInspectInvestment":true});
        for page in [StatusPage::House,StatusPage::Investment,StatusPage::CareerHistory,StatusPage::Skills]{
            let mut rng=ReplayRng::new(42);let mut panel=StatusPanel::default();
            panel.tick(115,&mut rng);panel.tick(115,&mut rng);let expected=panel.back_tracks.clone();assert_eq!(expected[0].frame,1);
            panel.page(page,&overview,&mut rng).unwrap();assert!(panel.back_tracks[0].first);panel.tick(1000,&mut rng);
            let mut restored:StatusPanel=serde_json::from_str(&serde_json::to_string(&panel).unwrap()).unwrap();let mut restored_rng=rng.clone();
            assert!(!restored.close());assert_eq!(restored.page,StatusPage::Overview);assert_eq!(restored.back_tracks,expected);assert_eq!(restored.clock_ms,1230);
            // A native GetTickCount clock keeps advancing inside the child;
            // returning advances one frame on the next draw, without catch-up.
            restored.tick(0,&mut restored_rng);assert_eq!(restored.back_tracks[0].frame,2);assert_eq!(restored.back_tracks[1].frame,2);assert_eq!(restored.back_tracks[0].last_tick,1230);
            if matches!(page,StatusPage::House|StatusPage::Investment){
                panel.tick(7001,&mut rng);assert_eq!(panel.page,StatusPage::Overview);assert_eq!(panel.back_tracks,expected);assert_eq!(panel.clock_ms,8231);
                panel.tick(0,&mut rng);assert_eq!(panel.back_tracks[0].frame,2);
            }
        }
    }
    #[test]
    fn overview_investment_display_matches_original_x86(){
        let fixture:Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-status.json")).unwrap();
        let rules:Value=serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        for case in fixture["investments"].as_array().unwrap(){
            assert_eq!(investment_return(case["stage"].as_u64().unwrap()as u8,&rules["statusRendering"]["investmentRows"]).unwrap(),case["value"].as_i64().unwrap()as i32,"{case}");
        }
    }
}
pub fn overview(player:&LegacyPlayer,housing:&HousingState,spouse:i8,mission_index:Option<i16>,rules:&Value)->Result<Value,String>{
    let table=&rules["statusRendering"];let life=player.life();let sex=player.i16(0x4b6498)?;
    if !(0..=1).contains(&sex){return Err("Invalid overview sex".into());}
    let empty=table["emptyLabel"].as_str().ok_or("Missing overview empty label")?;
    let mut fields=json!({"sex":string(table,"sexLabels",sex as usize)?,"career":empty,"salary":empty,
        "investment":empty,"investmentReturn":empty,"marriage":string(table,"marriageLabels",usize::from(spouse!=-1))?,"spouseSalary":empty});
    if life.profession!=-1&&life.profession_level>0{
        let career=usize::try_from(life.profession).map_err(|_|"Invalid overview career")?;
        let rank=usize::try_from(life.profession_level).map_err(|_|"Invalid overview rank")?;
        fields["career"]=json!(table["careerRankNames"][career][rank-1].as_str().ok_or("Missing overview career rank")?);
        fields["salary"]=json!(integer(table,"salaryBase",career)?.wrapping_mul(integer(table,"salaryMultipliers",rank)?)/10);
    }
    if housing.investment_stage>=2{
        fields["investment"]=json!(string(table,"descriptions",usize::from(housing.investment_stage-1)/3)?);
        fields["investmentReturn"]=json!(investment_return(housing.investment_stage,&table["investmentRows"])?);
    }
    if spouse!=-1{
        let index=i32::from(spouse)%8+i32::from(sex)*8;
        fields["spouseSalary"]=json!(integer(table,"spouseSalary",usize::try_from(index).map_err(|_|"Invalid overview spouse")?)?);
    }
    let mission_count=player.i16(0x4b64d4)?;
    let mission_rank=if player.i16(0x4b64de)?==1{table["initialMissionRankLabel"].as_str().ok_or("Missing initial mission rank")?.to_string()}
        else{string(table,"missionRankLabels",usize::try_from(mission_count/5).map_err(|_|"Invalid mission rank")?)?};
    let mission=match mission_index{Some(index)=>string(table,"descriptions",usize::try_from(index+5).map_err(|_|"Invalid mission description")?)?,None=>empty.to_string()};
    let hp=i32::from(player.i16(0x4b6486)?);let max_hp=i32::from(player.i16(0x4b6484)?);
    if max_hp==0{return Err("Original status health bar has a zero divisor".into());}
    Ok(json!({"fields":fields,"portraitIndex":sex*31+life.stage+if life.profession==-1{0}else{life.profession},
        "hpWidth":hp.wrapping_mul(102)/max_hp,"hpText":format!("{hp}/{max_hp}"),
        "missionCount":mission_count,"missionRank":mission_rank,"mission":mission,
        "hasMission":mission_index.is_some(),"canInspectHouse":housing.style>=0,"canInspectInvestment":housing.investment_stage>=2}))
}

/// Text rows for the read-only native skill and employment-history panels.
pub fn detail_rows(player:&LegacyPlayer,history:&[Value],rules:&Value)->Result<Value,String>{
    let text=&rules["statusRendering"]["skills"];let skills=player.life().skills;
    let prefix=text["triggerPrefix"].as_str().ok_or("Missing native skill prefix")?;
    let attribute_prefix=text["attributePrefix"].as_str().ok_or("Missing native attribute prefix")?;
    let percent=text["percentSuffix"].as_str().ok_or("Missing native percent suffix")?;
    let lines=|row:&Value,amount:i32,percentage:bool|->Result<(String,String),String>{
        let trigger=row["trigger"].as_u64().ok_or("Invalid native skill trigger")?as usize;
        let attribute=row["attribute"].as_u64().ok_or("Invalid native skill attribute")?as usize;
        let first=format!("{prefix}{}",string(text,"triggerNames",trigger)?);
        let second=format!("{attribute_prefix}{}{amount}{}",string(text,"attributeNames",attribute)?,if percentage&&trigger==4{percent}else{""});
        Ok((first,second))
    };
    let mut skill_rows=Vec::new();
    for index in 0..26{
        let row=if index==0{&text["student"]}else{&text["rows"][index]};
        let learned=if index==0{if player.life().profession==-1{skills[25]}else{0}}else{skills[index-1]};
        let mut columns=Vec::new();
        for column in 0..3{
            let amount=integer(row,"values",column)?;
            let absent=index>0&&amount<1;
            let(first,second)=if absent{(rules["statusRendering"]["emptyLabel"].as_str().ok_or("Missing native empty label")?.to_string(),String::new())}else{lines(row,amount,true)?};
            columns.push(json!({"first":first,"second":second,"color":if !absent&&(column as i16)<learned{1}else{255}}));
        }
        skill_rows.push(json!({"name":string(text,"names",index)?,"columns":columns}));
    }
    let mut history_rows=Vec::new();
    for record in history{
        let career=record["career"].as_u64().ok_or("Invalid native career history")?as usize;
        let rank=record["rank"].as_u64().ok_or("Invalid native career history rank")?as usize;
        let row=&rules["skills"][career];
        let amount=integer(row,"levelValues",rank.checked_sub(1).ok_or("Invalid native history rank")?)?;
        let(first,second)=lines(row,amount,false)?;
        history_rows.push(json!({"year":record["year"],"age":record["age"],"name":row["name"],"description":format!("{first}{second}")}));
    }
    Ok(json!({"skills":skill_rows,"careerHistory":history_rows}))
}
