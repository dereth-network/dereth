//! The checks of a pack's server content: what its SQL names must be there.
//!
//! - **Types:** each property's value is of its table's type, and each property id is one the
//!   server's schema has for that type (when the schema is known).
//! - **Classes:** every class an emote action, a generator profile or a create-list row names is a
//!   pack's weenie (`{{wcid:...}}`) or a class of the world database (when it is known). A
//!   generator's treasure entry names a loot table instead, as a death treasure (data id 35) and a
//!   wielded treasure (data id 32) do: one the packs and the world do not have is a warning (when
//!   the world's are known), as the server makes nothing of it.
//! - **Quests** (warnings, as the server stamps an unregistered quest too): every quest a quest
//!   action names is registered, in a pack's `quest` rows or the world database's; and a quest a
//!   pack registers is used. An emote set's `quest` is the label a quest or test action answers
//!   to, not a quest, and is not looked up.
//! - **Spells:** every spell a spell book or an action names is in the files' spell table.
//! - **What the server does otherwise** (warnings, as they stop nothing): a generator that makes
//!   more at the start than it keeps at most at once, which the server raises its most to; a
//!   portal with no destination, which sends no one anywhere; a wield requirement with no
//!   minimum, which anyone meets; and a set a creature draws by chance whose chances pass 100%,
//!   whose last entry the server makes only for what is left of the 100%.
//!
//! Each finding names the file and the line its row starts on. The checks read rows, not
//! weenies, so they run as well over a whole world database as over a pack's files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::content::{bare, unquote, PropertyKind, Schema};
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
    /// Every loot table's id, the packs' and the world database's, death and wielded alike, when
    /// the world database's are known; and the symbols of the tables the packs name so, in full.
    pub loot_tables: Option<BTreeSet<u32>>,
    pub loot_symbols: BTreeSet<String>,
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
    let mut weenies: BTreeMap<String, WeenieRows> = BTreeMap::new();
    sql_ids::rows(text, |row| {
        let table = row.table;
        gather(&mut weenies, row);
        if table == "weenie_properties_d_i_d" {
            let what = match row.get("type").and_then(sql_ids::number) {
                Some(35) => Some("its death treasure"),
                Some(32) => Some("its wielded treasure"),
                _ => None,
            };
            if let Some(f) = what.and_then(|w| missing_loot(row, "value", w, pack, known)) {
                warnings.push(Finding {
                    at: Loc::new(path, lines.at(row.at)),
                    text: f,
                    warning: true,
                });
            }
        }
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
                let place = row.get("where_Create").and_then(sql_ids::number);
                if place.is_some_and(|p| p & sql_ids::TREASURE_PLACE != 0) {
                    if let Some(f) = missing_loot(
                        row,
                        "weenie_Class_Id",
                        "a generator's treasure entry",
                        pack,
                        known,
                    ) {
                        warnings.push(Finding {
                            at: Loc::new(path, lines.at(row.at)),
                            text: f,
                            warning: true,
                        });
                    }
                } else {
                    class(
                        "weenie_Class_Id",
                        "a generator profile",
                        &mut lines,
                        &mut fault,
                    );
                }
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
    let mut lines = Lines::new(text);
    for w in weenies.values() {
        for (at, text) in w.warnings() {
            out.push(Finding {
                at: Loc::new(path, lines.at(at)),
                text,
                warning: true,
            });
        }
    }
}

/// What a row whose `column` names a loot table (`what` of it) says when the packs and the world
/// do not have it, the world's being known.
fn missing_loot(
    row: &sql_ids::Row<'_>,
    column: &str,
    what: &str,
    pack: &str,
    known: &Known,
) -> Option<String> {
    let v = row.get(column)?.trim();
    // A pack's own table, by its symbol: one a pack holds.
    if let Some(inner) = v.strip_prefix("{{").and_then(|s| s.strip_suffix("}}")) {
        let (kind, sym) = inner.split_once(':')?;
        if kind.trim() != crate::symbol::Kind::Treasure.name() {
            return None;
        }
        let sym = crate::symbol::qualify(pack, sym.trim()).ok()?;
        return (!known.loot_symbols.contains(&sym)).then(|| {
            format!(
                "{}: {what} names loot table {sym}, which no pack has",
                row.table
            )
        });
    }
    let id = sql_ids::number(v).filter(|id| *id != 0)?;
    known
        .loot_tables
        .as_ref()
        .is_some_and(|t| !t.contains(&id))
        .then(|| {
            format!(
                "{}: {what} names loot table {id}, which no pack has and the world does not have",
                row.table
            )
        })
}

/// The weenie types of the creatures, which draw what they carry by chance.
const CREATURES: [u32; 9] = [10, 11, 12, 15, 41, 60, 61, 69, 71];

/// A portal's weenie type.
const PORTAL: u32 = 7;

/// Each wield requirement's property and its minimum's.
const WIELD_NEEDS: [(u32, u32); 4] = [(158, 160), (270, 272), (273, 275), (276, 278)];

/// What one weenie's rows say, as the checks of a weenie as a whole read them: each with where
/// its row starts.
#[derive(Default)]
struct WeenieRows {
    /// Its weenie type.
    kind: Option<(u32, usize)>,
    ints: BTreeMap<u32, (i64, usize)>,
    /// Whether it has a destination, or one set from where it stands.
    destination: bool,
    /// Its create list's rows, in order: destination, shade.
    creates: Vec<(u32, f64, usize)>,
}

/// What `row` says of its weenie, added to `weenies`.
fn gather(weenies: &mut BTreeMap<String, WeenieRows>, row: &sql_ids::Row<'_>) {
    let owner = if row.table == "weenie" {
        "class_Id"
    } else {
        "object_Id"
    };
    let Some(w) = row.get(owner).map(str::trim) else {
        return;
    };
    let number = |c: &str| row.get(c).and_then(|v| sql_ids::number(&bare(v)));
    match row.table {
        "weenie" => {
            if let Some(t) = number("type") {
                weenies.entry(w.to_owned()).or_default().kind = Some((t, row.at));
            }
        }
        "weenie_properties_int" => {
            let v = row
                .get("value")
                .and_then(|v| bare(v).trim().parse::<i64>().ok());
            if let (Some(id), Some(v)) = (number("type"), v) {
                weenies
                    .entry(w.to_owned())
                    .or_default()
                    .ints
                    .insert(id, (v, row.at));
            }
        }
        "weenie_properties_position" => {
            if matches!(number("position_Type"), Some(2 | 26)) {
                weenies.entry(w.to_owned()).or_default().destination = true;
            }
        }
        "weenie_properties_create_list" => {
            let shade = row
                .get("shade")
                .and_then(|v| bare(v).trim().parse::<f64>().ok());
            weenies.entry(w.to_owned()).or_default().creates.push((
                number("destination_Type").unwrap_or(0),
                shade.unwrap_or(0.0),
                row.at,
            ));
        }
        _ => {}
    }
}

/// A chance, 0 to 1, as a percent the findings say: at most two places.
fn percent(p: f64) -> String {
    let s = format!("{:.2}", p * 100.0);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl WeenieRows {
    /// The warnings of the weenie as a whole, each with where its row starts.
    fn warnings(&self) -> Vec<(usize, String)> {
        let Some((kind, weenie_at)) = self.kind else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let int = |id: u32| self.ints.get(&id).copied();
        // More at the start than at most at once: the server keeps the start's as its most.
        if let (Some((most, _)), Some((first, at))) = (int(81), int(82)) {
            if most > 0 && first > most {
                out.push((
                    at,
                    format!(
                        "weenie_properties_int: it makes {first} at the start but keeps at most \
                         {most} at once, so the server keeps at most {first}"
                    ),
                ));
            }
        }
        if kind == PORTAL && !self.destination {
            out.push((
                weenie_at,
                "weenie: a portal with no destination, which the server sends no one through"
                    .to_owned(),
            ));
        }
        // A skill, an attribute, a vital, a level, a training or a number property of at least
        // nothing: everyone has that much.
        for (need, least) in WIELD_NEEDS {
            if let Some((what @ 1..=9, at)) = int(need) {
                if int(least).is_none_or(|(n, _)| n == 0) {
                    out.push((
                        at,
                        format!(
                            "weenie_properties_int: wield requirement {what} has no minimum, so \
                             anyone may wield it"
                        ),
                    ));
                }
            }
        }
        if CREATURES.contains(&kind) {
            out.extend(self.sets_past_whole());
        }
        out
    }

    /// The rows of a creature's lists (what it wields; what it drops) that take a set drawn by
    /// chance past 100%, as the server adds the chances up in its own single precision.
    fn sets_past_whole(&self) -> Vec<(usize, String)> {
        const CONTAIN: u32 = 1;
        const WIELD: u32 = 2;
        const TREASURE: u32 = 8;
        let wields = |d: u32| d & WIELD != 0;
        let drops = |d: u32| d & CONTAIN != 0 || (d & TREASURE != 0 && d & WIELD == 0);
        let mut out = Vec::new();
        for list in [&wields as &dyn Fn(u32) -> bool, &drops] {
            let mut total = 0.0f32;
            for (d, shade, at) in self.creates.iter().filter(|(d, _, _)| list(*d)) {
                if d & TREASURE == 0 || *shade == 0.0 {
                    continue;
                }
                if total >= 1.0 {
                    total = 0.0;
                }
                let before = total;
                #[allow(clippy::cast_possible_truncation)] // the server's own single precision
                {
                    total += *shade as f32;
                }
                if total > 1.0 + 1e-4 {
                    out.push((
                        *at,
                        format!(
                            "weenie_properties_create_list: a set drawn by chance adds up to \
                             {}%, but the server stops at 100%, so this entry is made {}% of \
                             the time, not {}%",
                            percent(f64::from(total)),
                            percent(1.0 - f64::from(before)),
                            percent(*shade)
                        ),
                    ));
                }
            }
        }
        out
    }
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
    let id = dereth_primitives::DataId(0x0E00_000E);
    set.read_portal(id)
        .and_then(|b| set.decode::<dereth_assets::tables::SpellTable>(id, &b).ok())
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
                // A table a pack names by its symbol.
                if matches!(row.table, "treasure_death" | "treasure_wielded") {
                    let sym = row
                        .get("treasure_Type")
                        .and_then(|v| v.trim().strip_prefix("{{treasure:"))
                        .and_then(|v| v.strip_suffix("}}"))
                        .and_then(|v| crate::symbol::qualify(&p.name, v.trim()).ok());
                    known.loot_symbols.extend(sym);
                }
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
