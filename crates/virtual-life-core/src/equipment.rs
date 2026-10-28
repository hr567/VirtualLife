//! Equipment purchase and research in FUN_0043d130 (native opcode 27).
use crate::{original_rules::{EquipmentKnowledge, EquipmentSlot}, rng::ReplayRng};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct EquipmentDefinition {
    pub id: usize,
    pub name: String,
    pub price: i32,
    pub icon: String,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EquipmentOutcome { Purchased, Upgraded, InsufficientMoney, CapacityReached, MaximumLevel }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentWorkshop {
    pub offered_id: usize,
    pub force_purchase: bool,
    pub mode: Option<String>,
    pub outcome: Option<EquipmentOutcome>,
}

impl EquipmentWorkshop {
    pub fn new(known: &[EquipmentKnowledge], carried: &[EquipmentSlot], first_visit: bool, rng: &mut ReplayRng) -> Result<Self, String> {
        check_storage(known, carried)?;
        if known.iter().all(|item| item.learned) { return Err("Original equipment selector would loop forever when all eighteen items are owned".into()); }
        // The original selects an unowned offer before the player chooses
        // purchase/research, even when they immediately cancel.
        let mut offered_id = rng.next_rand() as usize % 18;
        while known[offered_id].learned { offered_id = rng.next_rand() as usize % 18; }
        let force_purchase = first_visit || carried.iter().all(|item| item.id == -1);
        Ok(Self { offered_id, force_purchase, mode: if force_purchase { Some("purchase".into()) } else { None }, outcome: None })
    }

    pub fn set_mode(&mut self, mode: &str) -> Result<(), String> {
        if self.outcome.is_some() { return Err("The equipment visit has already completed".into()); }
        if !matches!(mode, "purchase" | "research") || (self.force_purchase && mode != "purchase") {
            return Err("Invalid equipment workshop mode".into());
        }
        self.mode = Some(mode.into());
        Ok(())
    }

    pub fn purchase(&mut self, definition: &EquipmentDefinition, known: &mut [EquipmentKnowledge], carried: &[EquipmentSlot], first_visit: &mut bool, money: &mut i32) -> Result<EquipmentOutcome, String> {
        check_storage(known, carried)?;
        if self.mode.as_deref() != Some("purchase") || self.outcome.is_some() || definition.id != self.offered_id || definition.price < 0 {
            return Err("Invalid equipment purchase".into());
        }
        if known[definition.id].learned { return Err("Offered equipment is already owned".into()); }
        let outcome = if *money < definition.price { EquipmentOutcome::InsufficientMoney } else {
            // 0x0043d8c1 clears first-visit BEFORE testing the eight-item limit.
            *first_visit = false;
            if known.iter().filter(|item| item.learned).count() >= 8 { EquipmentOutcome::CapacityReached } else {
                *money -= definition.price;
                known[definition.id].learned = true;
                // No carried-slot insertion here: new equipment is retrieved
                // separately through native opcode 17's equipment tab.
                EquipmentOutcome::Purchased
            }
        };
        self.outcome = Some(outcome);
        Ok(outcome)
    }

    pub fn research(&mut self, definition: &EquipmentDefinition, known: &mut [EquipmentKnowledge], carried: &mut [EquipmentSlot], money: &mut i32) -> Result<EquipmentOutcome, String> {
        check_storage(known, carried)?;
        if self.mode.as_deref() != Some("research") || self.outcome.is_some() || definition.id >= 18 || definition.price < 0 || !carried.iter().any(|item| item.id == definition.id as i16) {
            return Err("Invalid equipment research selection".into());
        }
        let outcome = if known[definition.id].level >= 4 { EquipmentOutcome::MaximumLevel }
        else if *money < definition.price / 2 { EquipmentOutcome::InsufficientMoney }
        else {
            *money -= definition.price / 2;
            known[definition.id].level += 1;
            // This is intentionally >0, not >=0. Original equipment id zero
            // is excluded from this count. Full inventories also skip updating
            // their existing carried level even though research was charged.
            let count = carried.iter().filter(|item| item.id > 0).count();
            if count < 8 {
                let index = carried.iter().position(|item| item.id == definition.id as i16).unwrap_or(count);
                carried[index] = EquipmentSlot { id: definition.id as i16, level: i16::from(known[definition.id].level) };
            }
            EquipmentOutcome::Upgraded
        };
        self.outcome = Some(outcome);
        Ok(outcome)
    }
}

fn check_storage(known: &[EquipmentKnowledge], carried: &[EquipmentSlot]) -> Result<(), String> {
    if known.len() != 18 || carried.len() != 8 || carried.iter().any(|slot| !(-1..18).contains(&slot.id)) {
        return Err("Original equipment storage requires eighteen ownership records and eight fixed slots".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inventory() -> (Vec<EquipmentKnowledge>, Vec<EquipmentSlot>) {
        (vec![EquipmentKnowledge { learned: false, level: 0 }; 18], vec![EquipmentSlot { id: -1, level: 0 }; 8])
    }
    fn definition(id: usize) -> EquipmentDefinition {
        EquipmentDefinition { id, name: "Test".into(), price: 2000, icon: String::new(), description: String::new() }
    }

    #[test]
    fn purchase_has_distinct_ownership_and_carried_state() {
        let (mut known, carried) = inventory();
        let mut visit = EquipmentWorkshop::new(&known, &carried, true, &mut ReplayRng::new(1)).unwrap();
        let mut first = true;
        let mut money = 2000;
        let definition = definition(visit.offered_id);
        assert_eq!(visit.purchase(&definition, &mut known, &carried, &mut first, &mut money).unwrap(), EquipmentOutcome::Purchased);
        assert_eq!(money, 0);
        assert!(!first);
        assert!(known[definition.id].learned);
        assert!(carried.iter().all(|slot| slot.id == -1));
    }

    #[test]
    fn affordable_capacity_rejection_still_clears_first_visit() {
        let (mut known, carried) = inventory();
        for item in &mut known[..8] { item.learned = true; }
        let mut visit = EquipmentWorkshop::new(&known, &carried, true, &mut ReplayRng::new(1)).unwrap();
        let mut first = true;
        let mut money = 2000;
        let definition = definition(visit.offered_id);
        assert_eq!(visit.purchase(&definition, &mut known, &carried, &mut first, &mut money).unwrap(), EquipmentOutcome::CapacityReached);
        assert!(!first);
        assert_eq!(money, 2000);
    }

    #[test]
    fn research_preserves_original_id_zero_and_full_inventory_behavior() {
        let (mut known, mut carried) = inventory();
        for (index, slot) in carried.iter_mut().enumerate() { *slot = EquipmentSlot { id: (index + 1) as i16, level: 0 }; known[index + 1].learned = true; }
        let mut visit = EquipmentWorkshop::new(&known, &carried, false, &mut ReplayRng::new(1)).unwrap();
        visit.set_mode("research").unwrap();
        let mut money = 2000;
        assert_eq!(visit.research(&definition(1), &mut known, &mut carried, &mut money).unwrap(), EquipmentOutcome::Upgraded);
        assert_eq!(known[1].level, 1);
        assert_eq!(carried[0].level, 0); // Original full-slot bug.
        carried[7].id = 0;
        let mut visit = EquipmentWorkshop::new(&known, &carried, false, &mut ReplayRng::new(1)).unwrap();
        visit.set_mode("research").unwrap();
        assert_eq!(visit.research(&definition(1), &mut known, &mut carried, &mut money).unwrap(), EquipmentOutcome::Upgraded);
        assert_eq!(carried[0].level, 2);
        assert_eq!(money, 0);
    }

    #[test]
    fn equipment_matches_original_x86_purchase_and_research_vectors() {
        let catalog: serde_json::Value = serde_json::from_str(include_str!("../../../assets/catalog/rules.json")).unwrap();
        let definitions: Vec<EquipmentDefinition> = serde_json::from_value(catalog["equipment"].clone()).unwrap();
        let vectors: serde_json::Value = serde_json::from_str(include_str!("../../../tests/fixtures/original-equipment.json")).unwrap();
        for case in vectors["cases"].as_array().unwrap() {
            let id = case["id"].as_u64().unwrap() as usize;
            let mode = case["mode"].as_str().unwrap();
            let mut known: Vec<EquipmentKnowledge> = serde_json::from_value(case["before"]["known"].clone()).unwrap();
            let mut carried: Vec<EquipmentSlot> = serde_json::from_value(case["before"]["carried"].clone()).unwrap();
            let mut first = case["before"]["first"].as_bool().unwrap();
            let mut money = case["before"]["money"].as_i64().unwrap() as i32;
            let mut visit = EquipmentWorkshop { offered_id: id, force_purchase: false, mode: Some(mode.into()), outcome: None };
            if mode == "purchase" { visit.purchase(&definitions[id], &mut known, &carried, &mut first, &mut money).unwrap(); }
            else { visit.research(&definitions[id], &mut known, &mut carried, &mut money).unwrap(); }
            assert_eq!(serde_json::json!({"money": money, "known": known, "carried": carried, "first": first}), case["after"], "equipment {id} {mode}");
        }
    }
}
