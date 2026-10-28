//! Native front-surface copy transitions, 0x00457260 (mosaic) and 0x00457470
//! (falling columns). Both consume the shared CRT random stream and ignore input.
//!
//! The original mosaic computes a duration and immediately overwrites it. There
//! is no sleep, presentation call, or input poll between its ordered tile copies.
//! Rain sleeps before each complete column pass. CPU/copy overhead is naturally
//! host dependent; only the original explicit Sleep duration advances this clock.
use crate::rng::ReplayRng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TransitionKind { Rain, Mosaic }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransitionRect { pub x:u16, pub y:u16, pub width:u16, pub height:u16 }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransitionColumn {
    pub speed:u16,
    /// Native stored position, clamped to 479 on completion. The visible height
    /// is 480 for a finished column, otherwise this position.
    pub y:u16,
    pub finished:bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeTransition {
    pub kind:TransitionKind,
    pub duration_seconds:u32,
    /// Column width for rain, square tile size for mosaic.
    pub unit_size:u16,
    pub sleep_ms:u32,
    pub clock_ms:u32,
    pub wait_remaining_ms:u32,
    pub finished:bool,
    pub steps:u32,
    pub columns:Vec<TransitionColumn>,
    /// Mosaic's complete shuffled row-major tile IDs. Construction performs the
    /// native 4 * tile_count swaps, consuming two random values for each swap.
    pub order:Vec<u16>,
    /// Every back-to-front copy issued by the most recent tick, in native order.
    /// A large elapsed tick may contain more than one rain pass.
    pub blits:Vec<TransitionRect>,
}

impl NativeTransition {
    /// Validate serialized transitions before resuming or exposing draw lists.
    pub fn valid_saved_state(&self)->bool {
        match self.kind {
            TransitionKind::Rain=>{
                if ![1,2,4].contains(&self.unit_size) || self.columns.len()!=usize::from(640/self.unit_size)
                    || !self.order.is_empty() || self.steps>48
                    || self.sleep_ms!=((self.duration_seconds as i32).wrapping_mul(1000)/48)as u32
                    || self.wait_remaining_ms>self.sleep_ms{return false;}
                if self.columns.iter().any(|c|!(10..=19).contains(&c.speed)||c.y>479||c.finished&&c.y!=479)
                    || self.finished!=self.columns.iter().all(|c|c.finished){return false;}
                if self.steps==0 && self.columns.iter().any(|c|c.y!=0||c.finished){return false;}
                self.blits.iter().all(|r|r.width==self.unit_size&&r.x%self.unit_size==0
                    &&r.x<=640-self.unit_size&&(1..=19).contains(&r.height)&&r.y<480&&r.y+r.height<=480)
            },
            TransitionKind::Mosaic=>{
                if ![4,8,16,32].contains(&self.unit_size)||!self.columns.is_empty()||self.sleep_ms!=0
                    ||self.wait_remaining_ms!=0||self.clock_ms!=0||self.steps>1||self.finished!=(self.steps==1){return false;}
                let count=usize::from((640/self.unit_size)*(480/self.unit_size));
                if self.order.len()!=count{return false;}
                let mut seen=vec![false;count];
                for &index in &self.order{let index=usize::from(index);if index>=count||seen[index]{return false;}seen[index]=true;}
                if self.blits.is_empty(){return true;}
                let columns=640/self.unit_size;
                self.finished&&self.blits.len()==count&&self.blits.iter().zip(&self.order).all(|(r,&index)|
                    *r==TransitionRect{x:index%columns*self.unit_size,y:index/columns*self.unit_size,width:self.unit_size,height:self.unit_size})
            },
        }
    }

    pub fn rain(seconds:u32,column_width:u16,rng:&mut ReplayRng)->Self {
        let width=match column_width {1|2|4=>column_width,_=>2};
        // 457521..457545: signed, wrapping multiply followed by signed /48;
        // Sleep receives the resulting 32-bit value unchanged.
        let sleep_ms=((seconds as i32).wrapping_mul(1000)/48)as u32;
        let columns=(0..640/width).map(|_|TransitionColumn{
            speed:(rng.next_rand()%10+10)as u16,y:0,finished:false,
        }).collect();
        Self{kind:TransitionKind::Rain,duration_seconds:seconds,unit_size:width,sleep_ms,
            clock_ms:0,wait_remaining_ms:sleep_ms,finished:false,steps:0,columns,
            order:Vec::new(),blits:Vec::new()}
    }

    pub fn mosaic(seconds:u32,tile_size:u16,rng:&mut ReplayRng)->Self {
        let size=match tile_size {4|8|16|32=>tile_size,_=>16};
        let count=(640/size)*(480/size);
        let mut order:Vec<u16>=(0..count).collect();
        for _ in 0..u32::from(count)*4 {
            let a=rng.next_rand()as usize%usize::from(count);
            let b=rng.next_rand()as usize%usize::from(count);
            order.swap(a,b);
        }
        Self{kind:TransitionKind::Mosaic,duration_seconds:seconds,unit_size:size,sleep_ms:0,
            clock_ms:0,wait_remaining_ms:0,finished:false,steps:0,columns:Vec::new(),
            order,blits:Vec::new()}
    }

    /// Advance explicit native sleeps and copies. Returns the elapsed time left
    /// after completion, so a caller can continue a following transition in the
    /// same update. No key/mouse input can skip either native routine.
    pub fn tick(&mut self,elapsed_ms:u32,rng:&mut ReplayRng)->u32 {
        self.blits.clear();
        if self.finished{return elapsed_ms;}
        if self.kind==TransitionKind::Mosaic {
            let columns=640/self.unit_size;
            self.blits.extend(self.order.iter().map(|&index|TransitionRect{
                x:index%columns*self.unit_size,y:index/columns*self.unit_size,
                width:self.unit_size,height:self.unit_size,
            }));
            self.steps=1;self.finished=true;return elapsed_ms;
        }
        let mut remaining=elapsed_ms;
        loop {
            let consumed=remaining.min(self.wait_remaining_ms);
            remaining-=consumed;self.wait_remaining_ms-=consumed;
            self.clock_ms=self.clock_ms.wrapping_add(consumed);
            if self.wait_remaining_ms!=0{return 0;}
            self.steps+=1;
            for(index,column)in self.columns.iter_mut().enumerate(){
                if column.finished{continue;}
                let next=column.y+column.speed;
                let bottom=next.min(480);
                self.blits.push(TransitionRect{x:index as u16*self.unit_size,y:column.y,
                    width:self.unit_size,height:bottom-column.y});
                column.finished=next>479;
                column.y=if column.finished{479}else{next};
            }
            self.finished=self.columns.iter().all(|column|column.finished);
            // 457628..457644 resamples ALL columns, including finished columns
            // and the final all-finished pass, before examining the exit flag.
            for column in &mut self.columns{column.speed=(rng.next_rand()%10+10)as u16;}
            if self.finished{return remaining;}
            self.wait_remaining_ms=self.sleep_ms;
            if remaining==0 && self.sleep_ms!=0{return 0;}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn hash_words(values:impl IntoIterator<Item=u32>)->String {
        let mut hash=0xcbf29ce484222325u64;
        for value in values{for byte in value.to_le_bytes(){hash=(hash^u64::from(byte)).wrapping_mul(0x100000001b3);}}
        format!("{hash:016x}")
    }
    fn column_hash(value:&NativeTransition)->String {
        hash_words(value.columns.iter().flat_map(|c|[u32::from(c.speed),u32::from(c.y),u32::from(c.finished)]))
    }
    fn blit_hash(value:&NativeTransition)->String {
        hash_words(value.blits.iter().flat_map(|r|[u32::from(r.x),u32::from(r.y),u32::from(r.width),u32::from(r.height)]))
    }
    fn adler32(bytes:&[u8])->u32 {
        let(mut a,mut b)=(1u32,0u32);
        for chunk in bytes.chunks(5552){for &byte in chunk{a+=u32::from(byte);b+=a;}a%=65521;b%=65521;}
        (b<<16)|a
    }
    fn pixels(seed:u32,source:bool)->Vec<u8>{
        (0u32..480).flat_map(|y|(0u32..640).map(move|x|if source{x.wrapping_mul(7).wrapping_add(y*13).wrapping_add(seed)as u8}else{(x^y^seed^0xa5)as u8})).collect()
    }
    fn copy_blits(value:&NativeTransition,front:&mut[u8],source:&[u8]) {
        for r in &value.blits{for y in r.y..r.y+r.height{let start=usize::from(y)*640+usize::from(r.x);let end=start+usize::from(r.width);front[start..end].copy_from_slice(&source[start..end]);}}
    }
    fn check_state(value:&NativeTransition,rng:&ReplayRng,frame:&Value,front:&[u8],draws:bool){
        assert_eq!(u64::from(value.clock_ms),frame["time"].as_u64().unwrap());
        assert_eq!(u64::from(rng.state),frame["rngState"].as_u64().unwrap());
        assert_eq!(rng.draws,frame["rngDraws"].as_u64().unwrap());
        assert_eq!(u64::from(adler32(front)),frame["frontAdler32"].as_u64().unwrap());
        if frame.get("columnsHash").is_some(){assert_eq!(column_hash(value),frame["columnsHash"].as_str().unwrap());}
        if draws{assert_eq!(blit_hash(value),frame["blitsHash"].as_str().unwrap());assert_eq!(value.blits.len()as u64,frame["blitCount"].as_u64().unwrap());}
        assert!(value.valid_saved_state());
    }

    #[test]
    fn transitions_match_complete_original_x86_functions_and_pixel_copies(){
        let vectors:Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-transitions.json")).unwrap();
        for case in vectors["cases"].as_array().unwrap(){
            let seed=case["seed"].as_u64().unwrap()as u32;
            let seconds=case["seconds"].as_u64().unwrap()as u32;
            let unit=case["unit"].as_u64().unwrap()as u16;
            let mut rng=ReplayRng::new(u64::from(seed));
            let rain=case["kind"]=="rain";
            let mut value=if rain{NativeTransition::rain(seconds,unit,&mut rng)}else{NativeTransition::mosaic(seconds,unit,&mut rng)};
            let mut front=pixels(seed,false);let source=pixels(seed,true);
            check_state(&value,&rng,&case["initial"],&front,true);
            assert_eq!(u64::from(value.unit_size),case["normalizedUnit"].as_u64().unwrap());
            if rain && value.sleep_ms!=0 {
                for frame in case["frames"].as_array().unwrap(){
                    assert_eq!(value.tick(value.sleep_ms,&mut rng),0);
                    copy_blits(&value,&mut front,&source);check_state(&value,&rng,frame,&front,true);
                }
            }else{
                if !rain{assert_eq!(serde_json::to_value(&value.order).unwrap(),case["order"]);}
                assert_eq!(value.tick(0,&mut rng),0);copy_blits(&value,&mut front,&source);
                let last=case["frames"].as_array().unwrap().last().unwrap();
                check_state(&value,&rng,last,&front,!rain);
                assert_eq!(blit_hash(&value),case["allBlitsHash"].as_str().unwrap());
                assert_eq!(value.blits.len()as u64,case["allBlitCount"].as_u64().unwrap());
            }
            assert!(value.finished,"{case}");assert_eq!(front,source);
            assert_eq!(value.steps as usize,case["frames"].as_array().unwrap().len());
            assert_eq!(u64::from(rng.state),case["finalRngState"].as_u64().unwrap());
            assert_eq!(rng.draws,case["finalRngDraws"].as_u64().unwrap());
        }
    }

    #[test]
    fn transition_save_resume_preserves_partial_sleeps_and_oversized_updates(){
        let mut rng=ReplayRng::new(42);let mut value=NativeTransition::rain(1,2,&mut rng);
        assert_eq!(rng.draws,320);assert_eq!(value.tick(7,&mut rng),0);
        assert_eq!(value.wait_remaining_ms,13);assert!(value.blits.is_empty());
        let mut saved:NativeTransition=serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        let mut saved_rng:ReplayRng=serde_json::from_str(&serde_json::to_string(&rng).unwrap()).unwrap();
        for delta in [0,12,1,16,4,63,1000]{
            assert_eq!(value.tick(delta,&mut rng),saved.tick(delta,&mut saved_rng));assert_eq!(value,saved);assert_eq!(rng,saved_rng);
        }
        assert!(value.finished);assert_eq!(value.clock_ms,760);assert_eq!(value.steps,38);
        let old_rng=rng.clone();assert_eq!(value.tick(500,&mut rng),500);assert_eq!(rng,old_rng);assert!(value.blits.is_empty());
        let mut large_rng=ReplayRng::new(42);let mut large=NativeTransition::rain(1,2,&mut large_rng);
        assert_eq!(large.tick(10_000,&mut large_rng),9240);large.blits.clear();
        assert_eq!(large,value);assert_eq!(large_rng,rng);
        let mut corrupt=large.clone();corrupt.columns[0].speed=0;assert!(!corrupt.valid_saved_state());
        let mut tiles=NativeTransition::mosaic(2,16,&mut rng);assert!(tiles.valid_saved_state());
        tiles.order[1]=tiles.order[0];assert!(!tiles.valid_saved_state());
    }
}
