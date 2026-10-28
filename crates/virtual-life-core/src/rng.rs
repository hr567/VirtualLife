use serde::{Deserialize, Serialize};

/// Original MSVC runtime random stream, recovered from N2Game.exe at 0x0045dec1.
/// The exact 32-bit wrap and 15-bit result preserve the original modulo behavior.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayRng { pub state: u32, pub draws: u64 }

impl ReplayRng {
    pub fn new(seed: u64) -> Self { Self { state: seed as u32, draws: 0 } }
    pub fn next_rand(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        self.draws += 1;
        (self.state >> 16) & 0x7fff
    }
    /// Original inclusive helper at 0x0040b9b0. Equal endpoints consume no draw.
    pub fn inclusive(&mut self, low:i32, high:i32)->Result<i32,String> {
        if low==high {return Ok(low);}
        let width=high.checked_sub(low).and_then(|difference|difference.checked_add(1)).filter(|width|*width>0).ok_or("Invalid legacy random range")?;
        Ok(self.next_rand() as i32 % width + low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_restores_the_next_draw() {
        let mut first = ReplayRng::new(1999);
        first.next_rand();
        let mut restored: ReplayRng = serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
        for _ in 0..100 { assert_eq!(first.next_rand(), restored.next_rand()); }
    }
    #[test]
    fn original_msvc_sequence_and_equal_endpoint_behavior() {
        let mut rng=ReplayRng::new(1);
        assert_eq!(rng.inclusive(3,3).unwrap(),3);
        assert_eq!(rng.draws,0);
        assert_eq!((0..5).map(|_|rng.next_rand()).collect::<Vec<_>>(),[41,18467,6334,26500,19169]);
    }
    #[test]
    fn matches_original_x86_execution_vectors() {
        let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/original-core.json")).unwrap();
        for case in vectors["random"].as_array().unwrap() {
            let mut rng=ReplayRng::new(case["seed"].as_u64().unwrap());
            for expected in case["values"].as_array().unwrap(){assert_eq!(u64::from(rng.next_rand()),expected.as_u64().unwrap());}
            assert_eq!(u64::from(rng.state),case["finalState"].as_u64().unwrap());
        }
    }
}
