//! Original fortune.dat questionnaires and their native scoring behavior.
//! In particular, type 2's displayed explanation and applied stat row use
//! different branches, and the last rows overlap thresholds and answers.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FortuneQuestion {
    pub prompt: String,
    pub options: Vec<String>,
    #[serde(default)] pub percentages: Vec<[i16; 4]>,
    #[serde(default)] pub explanations: Vec<String>,
    #[serde(default)] pub scores: Vec<i16>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FortuneRecord {
    pub id: usize,
    pub format: u8,
    pub title: String,
    pub questions: Vec<FortuneQuestion>,
    #[serde(default)] pub percentages: Vec<[i16; 4]>,
    #[serde(default)] pub explanations: Vec<String>,
    #[serde(default)] pub thresholds: Vec<i16>,
    pub runtime_compatible: bool,
    #[serde(default)] pub issues: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FortunePhase { Question, Explanation, Finished, Cancelled }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FortuneGame {
    pub record: FortuneRecord,
    pub phase: FortunePhase,
    pub question_index: usize,
    pub answers: Vec<usize>,
    pub score: i32,
    pub explanation: String,
    /// morality, intelligence, stamina, charm; original signed 16-bit values.
    pub percentages: [i16; 4],
    pub display_outcome: Option<usize>,
    pub statistic_outcome: Option<usize>,
}

impl FortuneGame {
    pub fn new(record: FortuneRecord) -> Result<Self, String> {
        if !record.runtime_compatible { return Err(format!("Fortune {} has unresolved original behavior: {}", record.id, record.issues.join("; "))); }
        if !matches!(record.format, 1 | 2) || record.questions.is_empty() || record.questions.len() > 20 {
            return Err("Invalid original fortune questionnaire".into());
        }
        for question in &record.questions {
            if question.options.is_empty() || question.options.len() > if record.format == 1 { 12 } else { 5 } {
                return Err("Fortune choice count exceeds original storage".into());
            }
            if record.format == 1 && question.percentages.len() < question.options.len() {
                return Err("Fortune answer has no original percentage row".into());
            }
            if record.format == 2 && question.scores.len() < question.options.len() {
                return Err("Fortune answer has no initialized original score".into());
            }
        }
        if record.format == 2 && (!matches!(record.thresholds.len(), 2 | 3) || record.percentages.len() < 3) {
            return Err("Fortune outcome can read uninitialized original memory".into());
        }
        Ok(Self { record, phase: FortunePhase::Question, question_index: 0, answers: Vec::new(),
            score: 0, explanation: String::new(), percentages: [0; 4], display_outcome: None, statistic_outcome: None })
    }

    pub fn answer(&mut self, answer: usize) -> Result<(), String> {
        if self.phase != FortunePhase::Question { return Err("The questionnaire is not accepting an answer".into()); }
        let question = self.record.questions.get(self.question_index).ok_or("Invalid fortune question index")?;
        if answer >= question.options.len() { return Err("Fortune answer is out of range".into()); }
        self.answers.push(answer);
        if self.record.format == 1 {
            for (total, percent) in self.percentages.iter_mut().zip(question.percentages[answer]) {
                *total = total.wrapping_add(percent);
            }
            self.explanation = explanation_line(&question.explanations, answer);
            self.phase = FortunePhase::Explanation;
        } else {
            self.score = self.score.wrapping_add(i32::from(question.scores[answer]));
            if self.answers.len() == self.record.questions.len() {
                self.finish_type_two()?;
                self.phase = FortunePhase::Explanation;
            } else {
                self.question_index += 1;
            }
        }
        Ok(())
    }

    fn finish_type_two(&mut self) -> Result<(), String> {
        // FUN_00435b50 only distinguishes three explanation lines, even when
        // fortune.dat supplies four. Missing lines fall back to the final line.
        let display = if self.score < i32::from(self.record.thresholds[0]) { 0 }
            else if self.score >= i32::from(self.record.thresholds[1]) { 2 } else { 1 };
        self.display_outcome = Some(display);
        self.explanation = explanation_line(&self.record.explanations, display);

        // The original packed struct at 0x004e5ad4 has three percentage rows,
        // followed by threshold count, three thresholds, and twenty answers.
        // Loading a fourth percentage row does NOT create a fourth storage row:
        // threshold loading overwrites it. Preserve this original behavior.
        let mut packed = [0_i16; 36];
        for (index, row) in self.record.percentages.iter().take(3).enumerate() {
            packed[index * 4..index * 4 + 4].copy_from_slice(row);
        }
        packed[12] = self.record.thresholds.len() as i16;
        packed[13..13 + self.record.thresholds.len()].copy_from_slice(&self.record.thresholds);
        for (index, answer) in self.answers.iter().enumerate() { packed[16 + index] = *answer as i16; }

        // FUN_00436450 reuses e5b3c for LAST question's optionCount - 1.
        // FUN_00437094 then indexes thresholds by that value, not by the
        // threshold count, and uses answer 0 as threshold 3 when needed.
        let mut outcome = self.record.questions.last().ok_or("Missing fortune question")?.options.len() - 1;
        while outcome > 0 && self.score < i32::from(packed[13 + outcome]) { outcome -= 1; }
        self.percentages.copy_from_slice(&packed[outcome * 4..outcome * 4 + 4]);
        self.statistic_outcome = Some(outcome);
        Ok(())
    }

    pub fn advance(&mut self) -> Result<(), String> {
        if self.phase != FortunePhase::Explanation { return Err("There is no fortune explanation to advance".into()); }
        if self.answers.len() == self.record.questions.len() {
            self.phase = FortunePhase::Finished;
        } else {
            self.question_index += 1;
            self.explanation.clear();
            self.phase = FortunePhase::Question;
        }
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), String> {
        if matches!(self.phase, FortunePhase::Finished | FortunePhase::Cancelled) { return Err("The fortune questionnaire already ended".into()); }
        self.phase = FortunePhase::Cancelled;
        Ok(())
    }

    /// FUN_00437360 narrows the percentage delta to int16, then clamps the
    /// proposed sum BEFORE animated addition. This differs from other rewards.
    pub fn final_stats(&self, current: [i16; 4]) -> Result<[i16; 4], String> {
        if self.phase != FortunePhase::Finished { return Err("Fortune effects are not ready".into()); }
        Ok(std::array::from_fn(|index| {
            let delta = (i32::from(current[index]) * i32::from(self.percentages[index]) / 100) as i16;
            (i32::from(current[index]) + i32::from(delta)).clamp(0, 9999) as i16
        }))
    }
}

fn explanation_line(lines: &[String], index: usize) -> String {
    lines.get(index).or_else(|| lines.last()).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<FortuneRecord> {
        let value: serde_json::Value = serde_json::from_str(include_str!("../../../assets/catalog/questionnaires.json")).unwrap();
        serde_json::from_value(value["fortune"]["records"].clone()).unwrap()
    }

    fn complete(record: FortuneRecord, answer: usize) -> FortuneGame {
        let mut game = FortuneGame::new(record).unwrap();
        while game.phase != FortunePhase::Finished {
            match game.phase {
                FortunePhase::Question => { let choice = answer.min(game.record.questions[game.question_index].options.len() - 1); game.answer(choice).unwrap(); }
                FortunePhase::Explanation => game.advance().unwrap(),
                _ => unreachable!(),
            }
        }
        game
    }

    #[test]
    fn original_corpus_questionnaires_complete_without_repairing_undefined_records() {
        let records = catalog();
        let blocked: Vec<usize> = records.iter().filter(|record| FortuneGame::new((*record).clone()).is_err()).map(|record| record.id).collect();
        assert_eq!(blocked, [32, 33, 37]);
        for record in records.into_iter().filter(|record| record.runtime_compatible) {
            for answer in 0..5 {
                let game = complete(record.clone(), answer);
                assert_eq!(game.answers.len(), game.record.questions.len());
                assert!(game.final_stats([1000; 4]).unwrap().iter().all(|value| (0..=9999).contains(value)));
            }
        }
    }

    #[test]
    fn original_type_two_threshold_overlap_is_not_repaired() {
        let game = complete(catalog()[24].clone(), 0);
        assert_eq!(game.statistic_outcome, Some(3));
        assert_eq!(game.percentages, [2, 5, 12, 0]);
        assert_eq!(game.final_stats([1000; 4]).unwrap(), [1020, 1050, 1120, 1000]);
        let game = complete(catalog()[18].clone(), 0);
        assert_eq!(game.statistic_outcome, Some(2));
        assert_eq!(game.percentages, game.record.percentages[2]);
    }

    #[test]
    fn questionnaire_save_preserves_answers_and_cancel_never_applies_partial_rewards() {
        let mut game = FortuneGame::new(catalog()[0].clone()).unwrap();
        game.answer(0).unwrap();
        let mut restored: FortuneGame = serde_json::from_str(&serde_json::to_string(&game).unwrap()).unwrap();
        assert_eq!(game, restored);
        restored.cancel().unwrap();
        assert!(restored.final_stats([1000; 4]).is_err());
    }

    #[test]
    fn fortune_matches_original_x86_score_outcome_and_clamp_vectors() {
        let records = catalog();
        let vectors: serde_json::Value = serde_json::from_str(include_str!("../../../tests/fixtures/original-fortune.json")).unwrap();
        for case in vectors["cases"].as_array().unwrap() {
            let game = complete(records[case["recordId"].as_u64().unwrap() as usize].clone(), case["choice"].as_u64().unwrap() as usize);
            assert_eq!(game.score, case["score"].as_i64().unwrap() as i32);
            assert_eq!(serde_json::to_value(&game.answers).unwrap(), case["answers"]);
            assert_eq!(serde_json::to_value(game.percentages).unwrap(), case["percentages"]);
            assert_eq!(serde_json::to_value(game.statistic_outcome).unwrap(), case["outcome"]);
            let before: [i16; 4] = serde_json::from_value(case["before"].clone()).unwrap();
            assert_eq!(serde_json::to_value(game.final_stats(before).unwrap()).unwrap(), case["after"]);
        }
    }
}
