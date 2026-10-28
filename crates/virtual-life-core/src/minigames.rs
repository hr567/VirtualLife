//! Native minigame state recovered from N2Game.exe. Frame updates are explicit
//! so the browser owns presentation timing while Rust owns choices and rewards.
use crate::rng::ReplayRng;
use crate::bonus::BonusAnimation;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrackingPhase { Reveal, Memorize, Shuffle, Settle, Choose, RoundResult, Reward, Finished }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrackingObject {
    pub x: i32,
    pub y: i32,
    /// Original index into the ten gbu animations at 0x0047aa04.
    pub sprite: u8,
    pub vertical_direction: i32,
    pub settled: bool,
    pub destination: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrackingGame {
    pub phase: TrackingPhase,
    pub round: u8,
    pub correct: u8,
    pub outcomes: Vec<bool>,
    pub objects: [TrackingObject; 5],
    pub revealed_target: usize,
    pub target: usize,
    pub selected: Option<usize>,
    pub phase_elapsed_ms: u32,
    pub idle_elapsed_ms: u32,
    pub idle_animation: Option<(usize, u8)>,
    pub exit_direction: u8,
    pub exit_vertical_direction: i32,
    pub clock_ms: u32,
    pub starting_intelligence: i16,
    pub reward: Option<BonusAnimation>,
    pub reward_skip: bool,
}

impl TrackingGame {
    /// FUN_00429350's six stage pairs at 0x0047aa74 are all [3, 1].
    pub const ROUNDS: u8 = 3;
    pub const SPRITES: [&'static str; 10] = [
        "gbs/gbu01.flc", "gbs/gbu02.flc", "gbs/gbu09.flc", "gbs/gbu03.flc",
        "gbs/gbu10.flc", "gbs/gbu07.flc", "gbs/gbu08.flc", "gbs/gbu04.flc",
        "gbs/gbu05.flc", "gbs/gbu06.flc",
    ];

    pub fn new(rng: &mut ReplayRng) -> Self {
        let target = rng.next_rand() as usize % 5; // FUN_00427f50
        Self {
            phase: TrackingPhase::Reveal, round: 1, correct: 0, outcomes: Vec::new(),
            objects: std::array::from_fn(|index| TrackingObject {
                x: 460 + index as i32 * 70, y: 220, sprite: 1,
                vertical_direction: 0, settled: false, destination: index,
            }),
            revealed_target: target, target, selected: None,
            phase_elapsed_ms: 0, idle_elapsed_ms: 0, idle_animation: None,
            exit_direction: 2, exit_vertical_direction: 0,
            clock_ms: 0, starting_intelligence: 0, reward: None, reward_skip: false,
        }
    }

    pub fn with_intelligence(rng: &mut ReplayRng,current:i16)->Self {
        let mut game=Self::new(rng);game.starting_intelligence=current;game
    }

    /// One original rendering-loop iteration. `elapsed_ms` advances the clock,
    /// while movement remains per-frame, as in FUN_004282a0.
    pub fn tick(&mut self, elapsed_ms: u32, rng: &mut ReplayRng) -> Result<(), String> {
        self.clock_ms=self.clock_ms.wrapping_add(elapsed_ms);
        self.phase_elapsed_ms = self.phase_elapsed_ms.checked_add(elapsed_ms).ok_or("Minigame clock overflow")?;
        match self.phase {
            TrackingPhase::Reveal => {
                for object in &mut self.objects { object.x -= 1; }
                if self.objects[0].x < 141 {
                    for (index, object) in self.objects.iter_mut().enumerate() {
                        object.x = 140 + index as i32 * 70;
                        object.sprite = 0;
                    }
                    self.phase = TrackingPhase::Memorize;
                    self.phase_elapsed_ms = 0;
                }
            }
            TrackingPhase::Shuffle => {
                for object in &mut self.objects {
                    if object.sprite == 3 {
                        object.x -= 6;
                        if object.x < 1 { object.sprite = 4; object.x += 6; }
                    } else {
                        object.x += 6;
                        if object.x > 639 { object.sprite = 3; object.x -= 6; }
                    }
                    if object.vertical_direction == -1 && object.y < 1 { object.vertical_direction = 1; }
                    else if object.vertical_direction == 1 && object.y > 479 { object.vertical_direction = -1; }
                    object.y += object.vertical_direction * 6;
                }
                if self.phase_elapsed_ms >= 4000 {
                    self.phase = TrackingPhase::Settle;
                    self.phase_elapsed_ms = 0;
                    for object in &mut self.objects { object.settled = false; }
                }
            }
            TrackingPhase::Settle => {
                for object in &mut self.objects {
                    if object.settled { continue; }
                    let x = 140 + object.destination as i32 * 70;
                    object.sprite = if object.x < x { 4 } else { 3 };
                    object.x = approach(object.x, x, 6);
                    object.y = approach(object.y, 220, 6);
                    if object.x == x && object.y == 220 { object.settled = true; object.sprite = 0; }
                }
                if self.objects.iter().all(|object| object.settled) {
                    // The original renders destination slots, not identity order,
                    // after the movement animation has completed.
                    self.objects.sort_by_key(|object| object.destination);
                    self.phase = TrackingPhase::Choose;
                    self.phase_elapsed_ms = 0;
                    self.idle_elapsed_ms = 0;
                    // FUN_00428a80 consumes these before accepting an answer.
                    self.exit_direction = (rng.next_rand() % 2 + 2) as u8;
                    self.exit_vertical_direction = (rng.next_rand() % 3) as i32 - 1;
                }
            }
            TrackingPhase::Memorize | TrackingPhase::Choose => {
                self.idle_elapsed_ms = self.idle_elapsed_ms.checked_add(elapsed_ms).ok_or("Minigame clock overflow")?;
                let threshold = if self.phase == TrackingPhase::Memorize { 7000 } else { 6000 };
                if self.idle_elapsed_ms >= threshold {
                    self.finish_idle_animation();
                    let mut index = rng.next_rand() as usize % 4;
                    let mut sprite = (rng.next_rand() % 5 + 5) as u8;
                    if sprite >= 8 { index = rng.next_rand() as usize % 4; sprite = 8; }
                    self.objects[index].sprite = sprite;
                    if sprite == 8 { self.objects[index + 1].sprite = 9; }
                    self.idle_animation = Some((index, sprite));
                    self.idle_elapsed_ms = 0;
                }
            }
            TrackingPhase::Reward => {
                let active=self.reward.as_mut().ok_or("Tracking reward animation is missing")?.tick(self.clock_ms,rng,self.reward_skip);
                self.reward_skip=false;
                if !active {self.phase=TrackingPhase::Finished;}
            }
            TrackingPhase::RoundResult | TrackingPhase::Finished => {}
        }
        Ok(())
    }

    /// The renderer calls this when the original idle FLC reaches its last frame.
    pub fn finish_idle_animation(&mut self) {
        if let Some((index, sprite)) = self.idle_animation.take() {
            self.objects[index].sprite = 0;
            if sprite == 8 { self.objects[index + 1].sprite = 0; }
        }
    }

    /// Click after the initial reveal, as in FUN_00428810. Shuffling consumes
    /// one destination draw followed by direction pairs for all five objects.
    pub fn start_shuffle(&mut self, rng: &mut ReplayRng) -> Result<(), String> {
        if self.phase != TrackingPhase::Memorize { return Err("The target reveal has not finished".into()); }
        self.finish_idle_animation();
        self.target = rng.next_rand() as usize % 5;
        for (index, object) in self.objects.iter_mut().enumerate() {
            object.destination = if index == self.revealed_target { self.target }
                else if index == self.target { self.revealed_target } else { index };
            object.sprite = (rng.next_rand() % 2 + 3) as u8;
            object.vertical_direction = (rng.next_rand() % 3) as i32 - 1;
            object.settled = false;
        }
        self.phase = TrackingPhase::Shuffle;
        self.phase_elapsed_ms = 0;
        self.idle_elapsed_ms = 0;
        Ok(())
    }

    /// An idle-animation click only cancels that animation in the original;
    /// a second click selects the object. False means no answer was submitted.
    pub fn choose(&mut self, index: usize) -> Result<bool, String> {
        if self.phase != TrackingPhase::Choose { return Err("The minigame is not accepting an answer".into()); }
        if index >= 5 { return Err("Tracking object index is out of range".into()); }
        if self.idle_animation.is_some() { self.finish_idle_animation(); return Ok(false); }
        self.selected = Some(index);
        let correct = index == self.target;
        self.correct += u8::from(correct);
        self.outcomes.push(correct);
        self.phase = TrackingPhase::RoundResult;
        self.phase_elapsed_ms = 0;
        Ok(true)
    }

    pub fn next_round(&mut self, rng: &mut ReplayRng) -> Result<(), String> {
        if self.phase != TrackingPhase::RoundResult { return Err("The current round is not complete".into()); }
        if self.round == Self::ROUNDS {
            if self.correct==0 {self.phase=TrackingPhase::Finished;}
            else {
                self.phase=TrackingPhase::Reward;
                let delta=self.final_intelligence(self.starting_intelligence)?.wrapping_sub(self.starting_intelligence);
                self.reward=Some(BonusAnimation::new(i32::from(self.starting_intelligence),i32::from(delta)));
                self.reward.as_mut().unwrap().tick(self.clock_ms,rng,false);
            }
            return Ok(());
        }
        let next = Self::new(rng);
        self.round += 1;
        self.phase = next.phase;
        self.objects = next.objects;
        self.revealed_target = next.revealed_target;
        self.target = next.target;
        self.selected = None;
        self.phase_elapsed_ms = 0;
        self.idle_elapsed_ms = 0;
        self.idle_animation = None;
        Ok(())
    }

    pub fn finish_reward_animation(&mut self)->Result<(),String> {
        if self.phase!=TrackingPhase::Reward {return Err("Tracking reward animation has not started".into());}
        self.reward_skip=true;Ok(())
    }

    /// FUN_004295e3..0042961c: the new stat is stored as int16 BEFORE clamping.
    pub fn final_intelligence(&self, current: i16) -> Result<i16, String> {
        if !matches!(self.phase,TrackingPhase::Reward|TrackingPhase::Finished) { return Err("Minigame reward is not ready".into()); }
        let percent = match self.correct { 0 => return Ok(current), 1 => 3, 2 => 5, 3 => 10, _ => return Err("Invalid tracking score".into()) };
        Ok(current.wrapping_add((i32::from(current) * percent / 100) as i16).min(9999))
    }
}

fn approach(value: i32, destination: i32, step: i32) -> i32 {
    if value < destination { (value + step).min(destination) }
    else { (value - step).max(destination) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reach_memorize(game: &mut TrackingGame, rng: &mut ReplayRng) {
        for _ in 0..320 { game.tick(16, rng).unwrap(); }
        assert_eq!(game.phase, TrackingPhase::Memorize);
    }

    fn reach_choose(game: &mut TrackingGame, rng: &mut ReplayRng) {
        game.start_shuffle(rng).unwrap();
        for _ in 0..400 { if game.phase == TrackingPhase::Choose { break; } game.tick(16, rng).unwrap(); }
        assert_eq!(game.phase, TrackingPhase::Choose);
    }

    #[test]
    fn tracking_three_rounds_and_saved_mid_shuffle_resume() {
        let mut rng = ReplayRng::new(1999);
        let mut game = TrackingGame::new(&mut rng);
        reach_memorize(&mut game, &mut rng);
        game.start_shuffle(&mut rng).unwrap();
        for _ in 0..20 { game.tick(16, &mut rng).unwrap(); }
        let mut restored: TrackingGame = serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
        let mut restored_rng = rng.clone();
        for _ in 0..330 { game.tick(16, &mut rng).unwrap(); restored.tick(16, &mut restored_rng).unwrap(); }
        assert_eq!(game, restored);
        assert_eq!(rng, restored_rng);
        for round in 1..=3 {
            if round > 1 { reach_memorize(&mut game, &mut rng); reach_choose(&mut game, &mut rng); }
            assert_eq!(game.phase, TrackingPhase::Choose);
            assert!(game.choose(game.target).unwrap());
            game.next_round(&mut rng).unwrap();
        }
        assert_eq!(game.final_intelligence(9990).unwrap(), 9999);
        assert_eq!(game.final_intelligence(1000).unwrap(), 1100);
        assert_eq!(game.outcomes, [true, true, true]);
        assert_eq!(rng.draws, 42);
    }

    #[test]
    fn tracking_idle_click_cancels_animation_without_scoring() {
        let mut rng = ReplayRng::new(1);
        let mut game = TrackingGame::new(&mut rng);
        reach_memorize(&mut game, &mut rng);
        reach_choose(&mut game, &mut rng);
        game.tick(6000, &mut rng).unwrap();
        assert!(game.idle_animation.is_some());
        assert!(!game.choose(game.target).unwrap());
        assert!(game.outcomes.is_empty());
        assert!(game.choose(game.target).unwrap());
        assert_eq!(game.correct, 1);
    }

    #[test]
    fn tracking_matches_original_x86_shuffle_and_reward_vectors() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!("../../../tests/fixtures/original-minigames.json")).unwrap();
        for case in vectors["shuffles"].as_array().unwrap() {
            let mut rng = ReplayRng::new(case["seed"].as_u64().unwrap());
            let mut game = TrackingGame::new(&mut ReplayRng::new(0));
            game.phase = TrackingPhase::Memorize;
            game.revealed_target = case["revealed"].as_u64().unwrap() as usize;
            game.start_shuffle(&mut rng).unwrap();
            assert_eq!(game.target, case["target"].as_u64().unwrap() as usize);
            assert_eq!(rng.state, case["rngState"].as_u64().unwrap() as u32);
            assert_eq!(rng.draws, 11);
            for (index, object) in game.objects.iter().enumerate() {
                assert_eq!(object.destination, case["destinations"][index].as_u64().unwrap() as usize);
                assert_eq!(object.sprite, case["sprites"][index].as_u64().unwrap() as u8);
                assert_eq!(object.vertical_direction, case["verticalDirections"][index].as_i64().unwrap() as i32);
            }
        }
        for case in vectors["rewards"].as_array().unwrap() {
            let mut game = TrackingGame::new(&mut ReplayRng::new(0));
            game.phase = TrackingPhase::Finished;
            game.correct = case["correct"].as_u64().unwrap() as u8;
            assert_eq!(game.final_intelligence(case["before"].as_i64().unwrap() as i16).unwrap(), case["after"].as_i64().unwrap() as i16);
        }
    }
}
