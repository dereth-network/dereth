//! Restore supported vital-cost recipes through an explicit Empyrean content extension.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn number(row: &Value, field: &str) -> Option<i64> {
    row["values"][field].as_str()?.parse().ok()
}

/// Canonical ACE SQL keeps these recipes quarantined. This supplementary script restores only
/// success-player vital costs with known preconditions and no mutation or stat-modification block.
pub fn patch(ledger: &[Value]) -> (Vec<u8>, Vec<Value>, Vec<u32>) {
    let mut mods: BTreeMap<i64, Vec<&Value>> = BTreeMap::new();
    let mut child_mods = BTreeSet::new();
    for row in ledger {
        let table = row["table"].as_str().unwrap_or_default();
        if table == "recipe_mod" {
            if let Some(recipe) = number(row, "recipe_Id") {
                mods.entry(recipe).or_default().push(row);
            }
        } else if table.starts_with("recipe_mods_") {
            child_mods.extend(number(row, "recipe_Mod_Id"));
        }
    }
    let mut accepted = BTreeMap::new();
    for (recipe, rows) in mods {
        let [row] = rows.as_slice() else { continue };
        let Some(id) = number(row, "id") else {
            continue;
        };
        let Some(flags) = number(row, "requires").filter(|f| (0..=7).contains(f)) else {
            continue;
        };
        if child_mods.contains(&id)
            || number(row, "index") != Some(3)
            || number(row, "executes_On_Success") != Some(1)
            || ["data_Id", "instance_Id", "unknown_7", "unknown_9"]
                .iter()
                .any(|f| number(row, f) != Some(0))
        {
            continue;
        }
        let values: Option<Vec<i64>> = ["health", "stamina", "mana"]
            .iter()
            .map(|f| number(row, f).filter(|n| *n <= 0))
            .collect();
        let Some(values) = values else { continue };
        let costs: Option<Vec<u32>> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                if flags & (1 << i) == 0 {
                    Some(0)
                } else {
                    v.checked_neg().and_then(|n| u32::try_from(n).ok())
                }
            })
            .collect();
        if let (Ok(recipe), Some(costs)) = (u32::try_from(recipe), costs) {
            accepted.insert(recipe, costs);
        }
    }
    let mut sql = b"-- Empyrean recipe preconditions. Apply after world.sql; unsupported by stock ACE.\nSET NAMES utf8mb4;\nSET SQL_MODE='STRICT_ALL_TABLES,NO_AUTO_VALUE_ON_ZERO';\nSET FOREIGN_KEY_CHECKS=1;\nCREATE TABLE `recipe_requirements_vital` (`id` int unsigned NOT NULL, `health` int unsigned NOT NULL, `stamina` int unsigned NOT NULL, `mana` int unsigned NOT NULL, PRIMARY KEY (`id`), CONSTRAINT `recipe_vital_parent` FOREIGN KEY (`id`) REFERENCES `recipe` (`id`) ON DELETE CASCADE) ENGINE=InnoDB;\n".to_vec();
    let mut changes = Vec::new();
    // The original ledger follows schema order: recipe parents precede their children.
    for row in ledger {
        let table = row["table"].as_str().unwrap_or_default();
        let key = match table {
            "recipe" => Some("id"),
            "recipe_mod" | "cook_book" => Some("recipe_Id"),
            t if t.starts_with("recipe_requirements_") => Some("recipe_Id"),
            _ => None,
        };
        let recipe = key
            .and_then(|key| number(row, key))
            .and_then(|n| u32::try_from(n).ok());
        if !recipe.is_some_and(|id| accepted.contains_key(&id)) {
            continue;
        }
        let Some(fields) = row["values"].as_object() else {
            continue;
        };
        let fields: Vec<_> = fields
            .iter()
            .filter(|(name, _)| {
                table != "recipe_mod" || !matches!(name.as_str(), "index" | "requires")
            })
            .collect();
        let columns = fields
            .iter()
            .map(|(name, _)| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(",");
        sql.extend_from_slice(format!("INSERT INTO `{table}` ({columns}) VALUES (").as_bytes());
        for (i, (name, value)) in fields.iter().enumerate() {
            if i > 0 {
                sql.push(b',');
            }
            if let Some(value) = value.as_str() {
                if row["quoted_columns"].as_array().is_some_and(|columns| {
                    columns
                        .iter()
                        .any(|column| column.as_str() == Some(name.as_str()))
                }) {
                    sql.extend_from_slice(&super::convert::literal(value.as_bytes()));
                } else {
                    sql.extend_from_slice(value.as_bytes());
                }
            } else {
                sql.extend_from_slice(b"NULL");
            }
        }
        sql.extend_from_slice(b");\n");
        changes.push(json!({"table":table,"action":"restore_for_empyrean",
            "recipe_id":recipe,"source_row":row["source_row"],
            "reason":"success-player vital costs with explicit current-vital preconditions",
            "values":row["values"]}));
    }
    for (recipe, cost) in &accepted {
        sql.extend_from_slice(
            format!(
                "INSERT INTO `recipe_requirements_vital` VALUES ({recipe},{},{},{});\n",
                cost[0], cost[1], cost[2]
            )
            .as_bytes(),
        );
    }
    (sql, changes, accepted.into_keys().collect())
}
