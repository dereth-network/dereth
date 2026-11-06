//! Treasure and emote-spell references, settled before projection.
//!
//! A creature names its wielded treasure by type (DID 32) and its death treasure by type (DID 35);
//! an emote action names a spell. The source's own server looks a death type up in the wielded
//! lists first, so a death type that names a wielded list drops that list's items into the corpse.
//! ACE's schema has no such fallback, so the projection writes those items as the creature's
//! treasure create list. A reference to nothing is quarantined only when it has been reviewed;
//! any other dangling reference refuses the conversion.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use empyrean_content::error::ImportError;
use empyrean_content::import::mysqldump::{DumpSink, Row, TableDef};
use serde_json::{json, Value as Json};

use super::convert::{fail, number};

pub const WIELDED_TREASURE: i128 = 32;
pub const DEATH_TREASURE: i128 = 35;
/// ACE's `DestinationType.ContainTreasure`: into the container, as rolled treasure.
const CONTAIN_TREASURE: i128 = 9;
const TREASURE: i128 = 8;

/// Creatures whose wielded treasure type the source does not define: (creature, type). The
/// source's own server equips them with nothing from it.
pub const WIELDED_ABSENT: &[(i128, i128)] = &[
    (227, 220),
    (619, 250),
    (1404, 250),
    (1484, 300),
    (1608, 71),
    (1610, 78),
];
pub const WIELDED_ABSENT_REASON: &str =
    "wielded treasure type absent from the source, whose own server equips nothing from it";

/// Emote actions casting a spell the source does not define: (emote owner, spell).
pub const EMOTE_SPELLS_ABSENT: &[(i128, i128)] = &[(2568, 2017), (4201, 2648)];
pub const EMOTE_SPELL_ABSENT_REASON: &str = "spell absent from the source";

pub const DEATH_LIST_REASON: &str =
    "death type names a wielded treasure list; its items are written as the treasure create list";

#[derive(Debug, Clone)]
struct WieldedEntry {
    class: i128,
    stack: i128,
    probability: String,
    simple: bool,
}

/// The reference rows of the source, read in one pass.
#[derive(Debug, Default)]
pub struct Index {
    death_types: BTreeSet<i128>,
    wielded: BTreeMap<i128, Vec<WieldedEntry>>,
    /// (object, DID type, value).
    treasure_refs: Vec<(i128, i128, i128)>,
    /// emote id -> owner.
    emote_owner: BTreeMap<i128, i128>,
    /// (emote id, spell).
    emote_spells: Vec<(i128, i128)>,
    spells: BTreeSet<i128>,
    create_list_max: i128,
    /// Objects whose create list already rolls treasure.
    rolled: BTreeSet<i128>,
}

/// What the settled references do to the projection.
#[derive(Debug, Default)]
pub struct Settled {
    /// (object, DID type, value) -> reason.
    pub treasure: BTreeMap<(i128, i128, i128), &'static str>,
    /// (owner, spell) pairs whose actions are quarantined.
    pub emote_spells: BTreeSet<(i128, i128)>,
    pub emote_owner: BTreeMap<i128, i128>,
    /// The create-list rows written for death types naming wielded lists.
    pub restored: Vec<Restored>,
}

#[derive(Debug, Clone)]
pub struct Restored {
    pub id: i128,
    pub object: i128,
    pub class: i128,
    pub stack: i128,
    pub probability: String,
    pub treasure_type: i128,
}

fn text(row: &Row<'_>, name: &str) -> Result<String, ImportError> {
    let i = row
        .columns()
        .iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .ok_or_else(|| fail(format!("{}.{} missing", row.table, name)))?;
    let b = row.values()[i]
        .bytes()
        .ok_or_else(|| fail(format!("{}.{} is null", row.table, name)))?;
    String::from_utf8(b.to_vec()).map_err(|e| fail(e.to_string()))
}

fn flag(row: &Row<'_>, name: &str) -> Result<bool, ImportError> {
    let t = text(row, name)?;
    Ok(match t.to_ascii_lowercase().as_str() {
        "1" | "true" | "b'1'" | "\u{1}" => true,
        "0" | "false" | "b'0'" | "" | "\0" => false,
        _ => return Err(fail(format!("{}.{name} is not a flag: {t:?}", row.table))),
    })
}

impl DumpSink for Index {
    fn create_table(&mut self, _: &TableDef) -> Result<(), ImportError> {
        Ok(())
    }
    fn row(&mut self, row: &Row<'_>) -> Result<(), ImportError> {
        match row.table {
            "treasure_death" => {
                self.death_types.insert(number(row, "treasure_Type")?);
            }
            "treasure_wielded" => {
                // A list rolled as one set: its first entry starts the set and nothing nests.
                let simple = !flag(row, "has_Sub_Set")? && !flag(row, "continues_Previous_Set")?;
                let entries = self
                    .wielded
                    .entry(number(row, "treasure_Type")?)
                    .or_default();
                let simple = simple && (entries.is_empty() == flag(row, "set_Start")?);
                entries.push(WieldedEntry {
                    class: number(row, "weenie_Class_Id")?,
                    stack: number(row, "stack_Size")?,
                    probability: text(row, "probability")?,
                    simple,
                });
            }
            "weenie_properties_d_i_d" => {
                let kind = number(row, "type")?;
                if kind == WIELDED_TREASURE || kind == DEATH_TREASURE {
                    self.treasure_refs.push((
                        number(row, "object_Id")?,
                        kind,
                        number(row, "value")?,
                    ));
                }
            }
            "weenie_properties_emote" => {
                self.emote_owner
                    .insert(number(row, "id")?, number(row, "object_Id")?);
            }
            "weenie_properties_emote_action" => {
                let i = row
                    .columns()
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case("spell_Id"));
                if let Some(spell) = i.and_then(|i| row.values()[i].bytes()) {
                    let spell: i128 = std::str::from_utf8(spell)
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| fail("emote spell is not an integer"))?;
                    if spell != 0 {
                        self.emote_spells.push((number(row, "emote_Id")?, spell));
                    }
                }
            }
            "spell" => {
                self.spells.insert(number(row, "id")?);
            }
            "weenie_properties_create_list" => {
                self.create_list_max = self.create_list_max.max(number(row, "id")?);
                if number(row, "destination_Type")? & TREASURE != 0 && text(row, "shade")? != "0" {
                    self.rolled.insert(number(row, "object_Id")?);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Index {
    /// Refuse projected content in which any treasure or emote-spell reference names nothing.
    /// `death_types` are the death profiles the Empyrean extension carries.
    pub fn verify(&self, death_types: &BTreeSet<i128>) -> Result<(), ImportError> {
        let mut dangling = Vec::new();
        for &(object, kind, value) in &self.treasure_refs {
            let known = value == 0
                || if kind == WIELDED_TREASURE {
                    self.wielded.contains_key(&value)
                } else {
                    death_types.contains(&value)
                };
            if !known {
                dangling.push(format!("weenie {object} DID {kind} -> {value}"));
            }
        }
        for &(emote, spell) in &self.emote_spells {
            if !self.spells.contains(&spell) {
                let owner = self.emote_owner.get(&emote).copied().unwrap_or(0);
                dangling.push(format!("weenie {owner} emote spell {spell}"));
            }
        }
        if dangling.is_empty() {
            Ok(())
        } else {
            Err(fail(format!(
                "projected content has dangling references: {}",
                dangling.join(", ")
            )))
        }
    }

    /// Settle every treasure and emote-spell reference: resolved, restored, reviewed absent, or
    /// refused.
    pub fn settle(&self) -> Result<Settled, ImportError> {
        let mut out = Settled {
            emote_owner: self.emote_owner.clone(),
            ..Settled::default()
        };
        let reviewed: BTreeSet<_> = WIELDED_ABSENT.iter().copied().collect();
        let mut next = self.create_list_max;
        for &(object, kind, value) in &self.treasure_refs {
            if value == 0 {
                continue;
            }
            if kind == WIELDED_TREASURE {
                if self.wielded.contains_key(&value) {
                    continue;
                }
                if !reviewed.contains(&(object, value)) {
                    return Err(fail(format!(
                        "weenie {object} names wielded treasure {value}, which the source does not define"
                    )));
                }
                out.treasure
                    .insert((object, kind, value), WIELDED_ABSENT_REASON);
                continue;
            }
            if self.death_types.contains(&value) {
                continue;
            }
            let Some(list) = self.wielded.get(&value) else {
                return Err(fail(format!(
                    "weenie {object} names death treasure {value}, which the source does not define"
                )));
            };
            if !list.iter().all(|e| e.simple) || self.rolled.contains(&object) {
                return Err(fail(format!(
                    "weenie {object}'s death treasure {value} is a wielded list the create list cannot express"
                )));
            }
            out.treasure
                .insert((object, kind, value), DEATH_LIST_REASON);
            for e in list {
                next += 1;
                out.restored.push(Restored {
                    id: next,
                    object,
                    class: e.class,
                    stack: e.stack,
                    probability: e.probability.clone(),
                    treasure_type: value,
                });
            }
        }
        let reviewed: BTreeSet<_> = EMOTE_SPELLS_ABSENT.iter().copied().collect();
        for &(emote, spell) in &self.emote_spells {
            if self.spells.contains(&spell) {
                continue;
            }
            let owner = self.emote_owner.get(&emote).copied().unwrap_or(0);
            if !reviewed.contains(&(owner, spell)) {
                return Err(fail(format!(
                    "weenie {owner}'s emote casts spell {spell}, which the source does not define"
                )));
            }
            out.emote_spells.insert((owner, spell));
        }
        Ok(out)
    }
}

impl Settled {
    /// The quarantine reason for a row, if the settled references remove it.
    pub fn quarantine(&self, row: &Row<'_>) -> Result<Option<&'static str>, ImportError> {
        match row.table {
            "weenie_properties_d_i_d" => {
                let kind = number(row, "type")?;
                if kind != WIELDED_TREASURE && kind != DEATH_TREASURE {
                    return Ok(None);
                }
                let key = (number(row, "object_Id")?, kind, number(row, "value")?);
                Ok(self.treasure.get(&key).copied())
            }
            "weenie_properties_emote_action" => {
                let i = row
                    .columns()
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case("spell_Id"));
                let Some(spell) = i.and_then(|i| row.values()[i].bytes()) else {
                    return Ok(None);
                };
                let spell: i128 = std::str::from_utf8(spell)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let owner = self
                    .emote_owner
                    .get(&number(row, "emote_Id")?)
                    .copied()
                    .unwrap_or(0);
                Ok(self
                    .emote_spells
                    .contains(&(owner, spell))
                    .then_some(EMOTE_SPELL_ABSENT_REASON))
            }
            _ => Ok(None),
        }
    }

    /// Write the restored create-list rows and their ledger records.
    pub fn write(&self, out: &mut impl Write, ledger: &mut Vec<Json>) -> Result<(), ImportError> {
        if self.restored.is_empty() {
            return Ok(());
        }
        let io = |e: std::io::Error| fail(e.to_string());
        out.write_all(b"INSERT INTO `weenie_properties_create_list` (`id`,`object_Id`,`destination_Type`,`weenie_Class_Id`,`stack_Size`,`palette`,`shade`,`try_To_Bond`) VALUES\n")
            .map_err(io)?;
        for (i, r) in self.restored.iter().enumerate() {
            let sep = if i + 1 == self.restored.len() {
                ";"
            } else {
                ","
            };
            writeln!(
                out,
                "({},{},{CONTAIN_TREASURE},{},{},0,{},0){sep}",
                r.id, r.object, r.class, r.stack, r.probability
            )
            .map_err(io)?;
            ledger.push(json!({"table":"weenie_properties_create_list","action":"restore",
                "reason":DEATH_LIST_REASON,"treasure_type":r.treasure_type,
                "values":{"id":r.id.to_string(),"object_Id":r.object.to_string(),
                    "destination_Type":CONTAIN_TREASURE.to_string(),"weenie_Class_Id":r.class.to_string(),
                    "stack_Size":r.stack.to_string(),"palette":"0","shade":r.probability,"try_To_Bond":"0"}}));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
