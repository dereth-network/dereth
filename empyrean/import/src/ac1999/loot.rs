//! Compile the reviewed source loot inputs into an explicit Empyrean extension.

use empyrean_content::loot::{self, Category, GroupedTreasureDeath, SlotGroup, SourceLootPolicy};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

fn read(root: &Path, path: &str, hashes: &mut BTreeMap<String, String>) -> Result<Value, String> {
    let bytes = std::fs::read(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() > loot::MAX_POLICY_BYTES {
        return Err(format!("{path}: input size limit"));
    }
    hashes.insert(path.into(), super::source::hash(&bytes));
    serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))
}

pub fn policy(root: &Path, revision: &str) -> Result<SourceLootPolicy, String> {
    let mut hashes = BTreeMap::new();
    let profile = read(root, "profile.json", &mut hashes)?;
    let mut tiers = BTreeMap::new();
    for n in 1..=5 {
        tiers.insert(
            n.to_string(),
            read(root, &format!("Tiers/tier-{n}.json"), &mut hashes)?,
        );
    }
    let mut pools = BTreeMap::new();
    for name in loot::POOLS {
        let mut value = read(root, &format!("Pools/{name}.json"), &mut hashes)?;
        if *name == "gems" {
            value = json!([value]);
        }
        pools.insert(*name, value);
    }
    let mut mutations = BTreeMap::new();
    for name in ["armor", "melee-weapons", "missile-weapons"] {
        let value = read(root, &format!("Mutations/{name}.json"), &mut hashes)?;
        let values = value.as_object().ok_or("mutation file must be an object")?;
        for (key, value) in values {
            if mutations.insert(key.clone(), value.clone()).is_some() {
                return Err(format!("duplicate mutation family {key}"));
            }
        }
    }
    let mut spells = BTreeMap::new();
    for name in loot::SPELL_POOLS {
        spells.insert(
            *name,
            read(root, &format!("Spells/{name}.json"), &mut hashes)?,
        );
    }
    let material_values = read(root, "material-values.json", &mut hashes)?;
    let material_colors = read(root, "material-colors.json", &mut hashes)?;
    let bytes=serde_json::to_vec(&json!({"id":1,"schema":1,"algorithm":loot::ALGORITHM,
        "algorithm_revision":loot::SOURCE_REVISION,"revision":revision,"files_sha256":hashes,"profile":profile,"tiers":tiers,"pools":pools,
        "mutations":mutations,"spells":spells,"material_values":material_values,"material_colors":material_colors})).map_err(|e|e.to_string())?;
    let mut policy = SourceLootPolicy::from_json(&bytes)?;
    // Source file/category joins are exact. Reject unused families instead of dropping fields.
    for (family, bands) in &policy.mutations {
        if family == "armor" {
            continue;
        }
        let matching: Vec<&mut Category> = policy
            .pools
            .values_mut()
            .flat_map(|c| c.iter_mut())
            .filter(|c| c.category == *family)
            .collect();
        if matching.len() != 1 {
            return Err(format!("mutation family {family} has no unique category"));
        }
        for c in matching {
            if !c.mutation_bands.is_empty() {
                return Err(format!("two band definitions for {family}"));
            }
            c.mutation_bands = bands.clone();
        }
    }
    for categories in policy.pools.values_mut() {
        for c in categories {
            for e in &mut c.entries {
                if e.heritage.is_none() {
                    e.heritage.clone_from(&c.heritage);
                }
            }
        }
    }
    policy.validate()?;
    Ok(policy)
}

fn number(row: &Value, name: &str) -> Result<u32, String> {
    row["values"][name]
        .as_str()
        .ok_or_else(|| format!("missing treasure field {name}"))?
        .parse()
        .map_err(|e| format!("{name}: {e}"))
}

pub fn patch(
    root: &Path,
    revision: &str,
    ledger: &[Value],
) -> Result<(Vec<u8>, Vec<Value>, SourceLootPolicy), String> {
    let policy = policy(root, revision)?;
    let mut sql=b"-- Versioned source loot. Requires Empyrean; stock ACE cannot execute these profiles.\nCREATE TABLE `empyrean_grouped_treasure` (`id` int unsigned NOT NULL, `profile` mediumtext NOT NULL, PRIMARY KEY (`id`)) ENGINE=InnoDB;\nCREATE TABLE `empyrean_loot_policy` (`id` int unsigned NOT NULL, `profile` mediumtext NOT NULL, PRIMARY KEY (`id`)) ENGINE=InnoDB;\n".to_vec();
    let mut changes = Vec::new();
    let prefixes = [
        "weapon",
        "armor",
        "item",
        "magic_Item",
        "consumable",
        "valuable",
    ];
    for row in ledger.iter().filter(|r| r["table"] == "treasure_death") {
        let mut groups = [SlotGroup::default(); 6];
        for (g, p) in groups.iter_mut().zip(prefixes) {
            *g = SlotGroup {
                chance: number(row, &format!("{p}_Chance"))?,
                min: number(row, &format!("{p}_Min_Amount"))?,
                max: number(row, &format!("{p}_Max_Amount"))?,
            };
        }
        let quality = row["values"]["loot_Quality_Mod"]
            .as_str()
            .ok_or("missing loot quality")?
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        let r = GroupedTreasureDeath {
            id: number(row, "id")?,
            treasure_type: number(row, "treasure_Type")?,
            tier: number(row, "tier")?,
            quality,
            groups,
        };
        r.validate()?;
        let bytes = serde_json::to_vec(&r).map_err(|e| e.to_string())?;
        insert(
            &mut sql,
            "empyrean_grouped_treasure",
            r.treasure_type,
            &bytes,
        );
        changes.push(json!({"table":"treasure_death","action":"restore_for_empyrean","treasure_type":r.treasure_type,"source_row":row["source_row"],"algorithm":loot::ALGORITHM}));
    }
    insert(
        &mut sql,
        "empyrean_loot_policy",
        1,
        &serde_json::to_vec(&policy).map_err(|e| e.to_string())?,
    );
    Ok((sql, changes, policy))
}
fn insert(sql: &mut Vec<u8>, table: &str, id: u32, json: &[u8]) {
    sql.extend_from_slice(
        format!("INSERT INTO `{table}` (`id`,`profile`) VALUES ({id},").as_bytes(),
    );
    sql.extend_from_slice(&super::convert::literal(json));
    sql.extend_from_slice(b");\n");
}

/// SQL template and spell-payload closure; DAT resources remain an operator-local preflight.
pub fn validate_references(policy: &SourceLootPolicy, pack: &[u8]) -> Result<(), String> {
    use empyrean_content::{pack::Pack, WorldDatabaseWithEntityCache};
    let db = WorldDatabaseWithEntityCache::new(
        Pack::from_bytes(pack.to_vec()).map_err(|e| e.to_string())?,
    );
    for categories in policy.pools.values() {
        for c in categories {
            for e in &c.entries {
                for id in std::iter::once(&e.wcid).chain(&e.elemental_variants) {
                    if db.get_cached_weenie(*id).is_none() {
                        return Err(format!(
                            "loot {} references missing template {id}",
                            c.category
                        ));
                    }
                }
            }
        }
    }
    for pool in policy.spells.values() {
        for family in pool {
            for id in &family.spells {
                if *id != 0 && db.get_cached_spell(*id).is_none() {
                    return Err(format!(
                        "loot spell family {} references missing spell {id}",
                        family.spell_name
                    ));
                }
            }
        }
    }
    Ok(())
}
