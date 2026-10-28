//! Daily inventory actions from FUN_00404630 / 00405c60 / 00406a30.
use crate::original_rules::{EquipmentKnowledge, EquipmentSlot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemDefinition {
    pub id: usize,
    pub name: String,
    pub animation: String,
    #[serde(default)]
    pub frame_count: u32,
    #[serde(default)]
    pub frame_duration_ms: u32,
    pub descriptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healing_percent: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct InventoryRules {
    pub ordinary: Vec<ItemDefinition>,
    pub special: Vec<ItemDefinition>,
    pub equipment: Vec<ItemDefinition>,
}

fn counted_item(id: usize, counts: &[i16], expected: usize) -> Result<(), String> {
    if counts.len() != expected || id >= expected || counts[id] <= 0 {
        return Err("The selected item is not in the inventory".into());
    }
    Ok(())
}

/// Every ordinary item consumes one copy even when HP is already full.
pub fn use_ordinary(id: usize, counts: &mut [i16], hp: &mut i16, max_hp: i16, rules: &InventoryRules) -> Result<(), String> {
    counted_item(id, counts, 13)?;
    let percent = rules.ordinary.iter().find(|item| item.id == id).and_then(|item| item.healing_percent)
        .ok_or("The original healing percentage is missing")?;
    let healing = percent.wrapping_mul(i32::from(max_hp)) / 100;
    counts[id] -= 1;
    // The original adds to the signed 16-bit HP word before its upper clamp.
    // It deliberately has no lower clamp here, including after overflow.
    *hp = hp.wrapping_add(healing as i16).min(max_hp);
    Ok(())
}

pub fn drop_ordinary(id: usize, counts: &mut [i16]) -> Result<(), String> {
    counted_item(id, counts, 13)?;
    counts[id] -= 1;
    Ok(())
}

pub fn drop_special(id: usize, counts: &mut [i16]) -> Result<(), String> {
    counted_item(id, counts, 27)?;
    counts[id] -= 1;
    Ok(())
}

fn equipment_slot(id: usize, carried: &[EquipmentSlot], equipped: &[i16]) -> Result<usize, String> {
    if id >= 18 || carried.len() != 8 || equipped.len() != 6 ||
        equipped.iter().any(|&value| !(-1..18).contains(&value)) {
        return Err("The equipment inventory is invalid".into());
    }
    carried.iter().position(|item| item.id == id as i16).ok_or_else(|| "The selected equipment is not carried".into())
}

pub fn wear_equipment(id: usize, carried: &[EquipmentSlot], equipped: &mut [i16]) -> Result<(), String> {
    equipment_slot(id, carried, equipped)?;
    equipped[id / 3] = id as i16;
    Ok(())
}

pub fn unequip_equipment(id: usize, carried: &[EquipmentSlot], equipped: &mut [i16]) -> Result<(), String> {
    equipment_slot(id, carried, equipped)?;
    if equipped[id / 3] == id as i16 { equipped[id / 3] = -1; }
    Ok(())
}

/// Call only after the original confirmation dialog has been accepted.
pub fn drop_equipment(id: usize, known: &mut [EquipmentKnowledge], carried: &mut [EquipmentSlot], equipped: &mut [i16]) -> Result<(), String> {
    if known.len() != 18 { return Err("The equipment ownership records are incomplete".into()); }
    let slot = equipment_slot(id, carried, equipped)?;
    if equipped[id / 3] == id as i16 { equipped[id / 3] = -1; }
    // Native code removes the first matching slot and leaves a hole. Even if a
    // duplicate remains, dropping clears this design's ownership and research.
    carried[slot] = EquipmentSlot { id: -1, level: 0 };
    known[id] = EquipmentKnowledge { learned: false, level: 0 };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn daily_inventory_actions_match_original_x86() {
        let rules: InventoryRules = serde_json::from_value(serde_json::from_str::<Value>(include_str!("../../../assets/catalog/rules.json")).unwrap()["inventory"].clone()).unwrap();
        let fixture: Value = serde_json::from_str(include_str!("../../../tests/fixtures/original-inventory.json")).unwrap();
        for (index, case) in fixture["items"].as_array().unwrap().iter().enumerate() {
            let id = case["id"].as_u64().unwrap() as usize;
            let mut counts: Vec<i16> = serde_json::from_value(case["before"]["counts"].clone()).unwrap();
            let mut hp = case["before"]["hp"].as_i64().unwrap() as i16;
            let max_hp = case["before"]["maxHp"].as_i64().unwrap() as i16;
            match case["action"].as_str().unwrap() {
                "use" => use_ordinary(id, &mut counts, &mut hp, max_hp, &rules).unwrap(),
                "dropOrdinary" => drop_ordinary(id, &mut counts).unwrap(),
                "dropSpecial" => drop_special(id, &mut counts).unwrap(),
                _ => panic!(),
            }
            assert_eq!(json!({"counts": counts, "hp": hp, "maxHp": max_hp}), case["after"], "item vector {index}");
        }
        for (index, case) in fixture["equipment"].as_array().unwrap().iter().enumerate() {
            let id = case["id"].as_u64().unwrap() as usize;
            let mut carried: Vec<EquipmentSlot> = serde_json::from_value(case["before"]["carried"].clone()).unwrap();
            let mut known: Vec<EquipmentKnowledge> = serde_json::from_value(case["before"]["known"].clone()).unwrap();
            let mut equipped: Vec<i16> = serde_json::from_value(case["before"]["equipped"].clone()).unwrap();
            match case["action"].as_str().unwrap() {
                "wear" => wear_equipment(id, &carried, &mut equipped).unwrap(),
                "unequip" => unequip_equipment(id, &carried, &mut equipped).unwrap(),
                "drop" => drop_equipment(id, &mut known, &mut carried, &mut equipped).unwrap(),
                _ => panic!(),
            }
            assert_eq!(json!({"carried": carried, "known": known, "equipped": equipped}), case["after"], "equipment vector {index}");
        }
    }

    #[test]
    fn invalid_inventory_actions_have_no_partial_effects() {
        let mut carried = vec![EquipmentSlot { id: -1, level: 0 }; 8];
        let mut known = vec![EquipmentKnowledge { learned: false, level: 0 }; 18];
        let mut equipped = vec![-1; 6];
        let before = (carried.clone(), known.clone(), equipped.clone());
        assert!(wear_equipment(0, &carried, &mut equipped).is_err());
        assert!(drop_equipment(18, &mut known, &mut carried, &mut equipped).is_err());
        assert_eq!((carried, known, equipped), before);
        let mut counts = vec![0; 13];
        assert!(drop_ordinary(0, &mut counts).is_err());
        assert!(drop_ordinary(13, &mut counts).is_err());
        assert!(counts.iter().all(|&count| count == 0));
    }
}
