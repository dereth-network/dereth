//! The checks of a pack's server content: what its SQL names must be there.
//!
//! - **Types:** each property's value is of its table's type, and each property id is one the
//!   server's schema has for that type (when the schema is known).
//! - **Classes:** every class an emote action, a generator profile or a create-list row names is a
//!   pack's weenie (`{{wcid:...}}`) or a class of the world database (when it is known).
//! - **Quests** (warnings, as the server stamps an unregistered quest too): every quest a quest
//!   action names is registered, in a pack's `quest` rows or the world database's; and a quest a
//!   pack registers is used. An emote set's `quest` is the label a quest or test action answers
//!   to, not a quest, and is not looked up.
//! - **Spells:** every spell a spell book or an action names is in the files' spell table.
//!
//! Each finding names the file and the line its row starts on. The checks read rows, not
//! weenies, so they run as well over a whole world database as over a pack's files.

use std::collections::BTreeSet;
use std::path::Path;

use crate::content::{unquote, PropertyKind, Schema};
use crate::error::Loc;
use crate::sql_ids::{self, Lines};

/// The emote actions whose message names a quest: the quest, fellowship quest and quest-bit
/// actions, the object's own as well as the player's.
pub const QUEST_ACTIONS: [u32; 27] = [
    20, 21, 22, 30, 31, 32, 33, 58, 60, 61, 70, 79, 80, 81, 82, 83, 84, 85, 86, 102, 103, 104, 105,
    106, 107, 108, 109,
];

/// What the content may name.
#[derive(Clone, Debug, Default)]
pub struct Known {
    /// The packs' weenies, by their symbols in full (`pack:name`).
    pub pack_classes: BTreeSet<String>,
    /// The world database's classes, when it is known.
    pub world_classes: Option<BTreeSet<u32>>,
    /// Every registered quest, its name in lower case.
    pub quests: BTreeSet<String>,
    /// Whether the world database's quests are among them (else a quest is checked against the
    /// packs' alone only when there are no others to look in).
    pub world_quests: bool,
    /// The spell table's spells, when the files are known.
    pub spells: Option<BTreeSet<u32>>,
    /// The server's names.
    pub schema: Option<Schema>,
}

/// One finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub at: Loc,
    pub text: String,
    pub warning: bool,
}

/// A quest name as registered: what comes before `@` (a quest action may stamp a quest with a
/// suffix of its own), in lower case.
#[must_use]
pub fn quest_key(name: &str) -> String {
    name.split('@').next().unwrap_or("").trim().to_lowercase()
}

/// The names of the quests `text` registers (its `quest` rows).
#[must_use]
pub fn quests(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    sql_ids::rows(text, |row| {
        if row.table == "quest" {
            if let Some(n) = row.get("name") {
                out.push(unquote(n));
            }
        }
    });
    out
}

/// The checks over `text`, the SQL file `path` of pack `pack` (whose tokens name its own
/// symbols); the quests it uses are added to `used`.
pub fn check(
    path: &Path,
    pack: &str,
    text: &str,
    known: &Known,
    used: &mut BTreeSet<String>,
    out: &mut Vec<Finding>,
) {
    let mut lines = Lines::new(text);
    let mut warnings = Vec::new();
    let mut fault = |at: usize, text: String, lines: &mut Lines<'_>| {
        out.push(Finding {
            at: Loc::new(path, lines.at(at)),
            text,
            warning: false,
        });
    };
    let class_known = |v: &str| -> Option<bool> {
        let v = v.trim();
        if v.eq_ignore_ascii_case("NULL") {
            return Some(true);
        }
        if let Some(inner) = v.strip_prefix("{{").and_then(|s| s.strip_suffix("}}")) {
            let sym = inner.split_once(':').map_or("", |(_, s)| s);
            return Some(
                crate::symbol::qualify(pack, sym.trim())
                    .is_ok_and(|q| known.pack_classes.contains(&q)),
            );
        }
        let n = sql_ids::number(v)?;
        if n == 0 {
            return Some(true);
        }
        known.world_classes.as_ref().map(|w| w.contains(&n))
    };
    let quest_known = |name: &str| -> bool {
        let k = quest_key(name);
        k.is_empty() || known.quests.contains(&k)
    };
    let spell_known = |v: &str| -> Option<bool> {
        let n = sql_ids::number(v.trim())?;
        if n == 0 {
            return Some(true);
        }
        known.spells.as_ref().map(|s| s.contains(&n))
    };
    sql_ids::rows(text, |row| {
        let table = row.table;
        if let Some(kind) = PropertyKind::of_table(table) {
            let (Some(ty), Some(v)) = (row.get("type"), row.get("value")) else {
                return;
            };
            let id = sql_ids::number(ty);
            if kind.value(v).is_none() && !v.trim().starts_with("{{") {
                fault(
                    row.at,
                    format!("{table}: property {ty}'s value {v} is not of its type"),
                    &mut lines,
                );
            }
            if let (Some(s), Some(id)) = (&known.schema, id) {
                if s.property(kind, id).is_none() {
                    fault(
                        row.at,
                        format!("{table}: property {id} is not one the server has for this type"),
                        &mut lines,
                    );
                }
            }
            return;
        }
        let class = |col: &str,
                     what: &str,
                     lines: &mut Lines<'_>,
                     fault: &mut dyn FnMut(usize, String, &mut Lines<'_>)| {
            if let Some(v) = row.get(col) {
                if class_known(v) == Some(false) {
                    fault(
                        row.at,
                        format!("{table}: {what} names class {}, which no pack defines and the world does not have", v.trim()),
                        lines,
                    );
                }
            }
        };
        match table {
            "weenie_properties_emote_action" => {
                class("weenie_Class_Id", "an action", &mut lines, &mut fault);
                if let Some(v) = row.get("spell_Id") {
                    if spell_known(v) == Some(false) {
                        fault(
                            row.at,
                            format!("{table}: an action names spell {}, which the spell table does not have", v.trim()),
                            &mut lines,
                        );
                    }
                }
                let ty = row.get("type").and_then(sql_ids::number);
                if ty.is_some_and(|t| QUEST_ACTIONS.contains(&t)) {
                    if let Some(m) = row.get("message") {
                        let name = unquote(m);
                        used.insert(quest_key(&name));
                        if !quest_known(&name) {
                            warnings.push(Finding {
                                at: Loc::new(path, lines.at(row.at)),
                                text: format!(
                                    "{table}: an action names quest {name:?}, which is not registered"
                                ),
                                warning: true,
                            });
                        }
                    }
                }
            }
            "weenie_properties_generator" => {
                class(
                    "weenie_Class_Id",
                    "a generator profile",
                    &mut lines,
                    &mut fault,
                );
            }
            "weenie_properties_create_list" => {
                class(
                    "weenie_Class_Id",
                    "a create-list row",
                    &mut lines,
                    &mut fault,
                );
            }
            "weenie_properties_spell_book" => {
                if let Some(v) = row.get("spell") {
                    if spell_known(v) == Some(false) {
                        fault(
                            row.at,
                            format!("{table}: spell {} is not in the spell table", v.trim()),
                            &mut lines,
                        );
                    }
                }
            }
            _ => {}
        }
    });
    out.extend(warnings);
}

/// The warnings for the quests pack files register (`registered`, each with where) that no
/// content uses (`used`).
#[must_use]
pub fn unused_quests(registered: &[(Loc, String)], used: &BTreeSet<String>) -> Vec<Finding> {
    registered
        .iter()
        .filter(|(_, n)| !used.contains(&quest_key(n)))
        .map(|(at, n)| Finding {
            at: at.clone(),
            text: format!("quest {n:?} is registered and nothing uses it"),
            warning: true,
        })
        .collect()
}

/// The spells the files' spell table has.
#[must_use]
pub fn spells(set: &crate::dats::DatSet) -> BTreeSet<u32> {
    use dereth_assets::Decode;
    let id = dereth_primitives::DataId(0x0E00_000E);
    set.read_portal(id)
        .and_then(|b| dereth_assets::tables::SpellTable::decode_payload(id, &b).ok())
        .map(|t| t.spells.into_keys().collect())
        .unwrap_or_default()
}

/// The checks over every SQL file of `packs`, as compiled into `c` over the files `base`: what
/// `known` gives (the world database's classes and quests, the server's names), and the packs'
/// own weenies and quests. Their quests are warned of when nothing uses them; another quest is
/// looked up only when the world database's are known.
#[must_use]
pub fn check_packs(
    packs: &[crate::source::Pack],
    c: &crate::compile::Compiled,
    base: &crate::dats::DatSet,
    known: &Known,
) -> Vec<Finding> {
    let mut known = known.clone();
    known.pack_classes.extend(
        c.lock
            .of(crate::symbol::Kind::Wcid)
            .map(|(s, _)| s.to_owned()),
    );
    if known.spells.is_none() {
        known.spells = Some(spells(base));
    }
    let mut registered: Vec<(Loc, String)> = Vec::new();
    for p in packs {
        for f in &p.sql {
            let mut lines = Lines::new(&f.text);
            sql_ids::rows(&f.text, |row| {
                if row.table == "quest" {
                    if let Some(n) = row.get("name") {
                        registered.push((Loc::new(&f.path, lines.at(row.at)), unquote(n)));
                    }
                }
            });
        }
    }
    known
        .quests
        .extend(registered.iter().map(|(_, n)| quest_key(n)));
    let mut used = BTreeSet::new();
    let mut out = Vec::new();
    for p in packs {
        for f in &p.sql {
            check(&f.path, &p.name, &f.text, &known, &mut used, &mut out);
        }
    }
    // Without the world database's quests, a quest not registered here may be one of its.
    if !known.world_quests {
        out.retain(|x| !(x.warning && x.text.contains("which is not registered")));
    }
    out.extend(unused_quests(&registered, &used));
    out
}
