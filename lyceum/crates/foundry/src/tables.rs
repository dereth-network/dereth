//! The game's own tables a world changes field by field: the skill table, the experience table, the
//! spell table and character creation. The client reads them, so a world that changes them changes
//! them in its overlay, where the client and the server both read them.
//!
//! A pack's `records/*.toml` may hold rows for them beside its new records. Each row sets only the
//! fields it names; every other field is the table's as it stands below (the world's imports, else
//! the player's own files), as a landblock edit changes only the vertices it names:
//!
//! ```toml
//! [[skill]]
//! skill = "Melee Defense"      # a skill by the files' name, spaces and case aside
//! trained_cost = 4
//! specialized_cost = 8
//!
//! [[skill]]
//! id = 1                       # a skill the table below lacks, by its number
//! like = "Heavy Weapons"       # optional: a new skill starts as a copy of this one
//! name = "Axe"
//! description = "Helps you fight with axes."
//! formula = { attributes = ["Strength", "Coordination"], divisor = 3 }
//! # also: icon, category, chargen_use, min_level, upper_bound, lower_bound, learn_mod;
//! # remove = true takes a skill out
//!
//! [[experience]]
//! max_level = 50               # the highest level: the curve is cut there, or grows to it
//! attribute = [0, 110, 277]    # whole lists of raise costs: attribute, vital, trained, specialized
//!
//! [[level]]
//! level = 2
//! xp = 1000                    # the experience to reach it
//! credits = 1                  # the skill credits it gives
//!
//! [[spell]]
//! id = 6                       # a spell by its number (names are shared: one name, several spells)
//! name = "Mend Self I"
//! mana = 10
//! # also: description, school, icon, category, bitfield, range_constant, range_mod, power,
//! # economy_mod, formula_version, component_loss, duration (seconds, for a spell that runs out),
//! # components (up to eight), caster_effect, target_effect, fizzle_effect, recovery_interval,
//! # recovery_amount, display_order, mana_mod
//!
//! [[starter-area]]
//! name = "Arwic"               # an area by its name; a new name is a new area
//! heritages = "all"            # or a list of heritages; a new area is every heritage's
//! at = [{ cell = 0xC6A90009, at = [84.0, 141.0, "ground"], heading = 90.0 }]
//! # remove = true takes an area out
//!
//! [[heritage]]
//! heritage = "Aluvian"
//! attribute_credits = 330
//! skill_credits = 50           # remove = true takes a heritage out: the game no longer offers it
//!
//! [[heritage-skill]]           # what a skill costs a heritage at creation
//! heritage = "Aluvian"
//! skill = "Axe"
//! trained_cost = 6
//! specialized_cost = 10        # remove = true takes it off the heritage's list
//!
//! [[heritage-template]]        # a "class" on the creation screen
//! heritage = "Aluvian"
//! name = "Axeman"
//! strength = 100               # also endurance, coordination, quickness, focus, self (10 when new)
//! trained = ["Axe", "Melee Defense", "Healing"]
//! specialized = ["Axe"]        # also icon, title; remove = true takes it out
//! ```
//!
//! Skills are named as the data files name them, spaces and case aside, after the pack's own skill
//! rows (so a heritage can offer a skill a row adds). Heritages and starter areas are named likewise.
//!
//! **Conflicts:** each field of each row is one thing packs can both set (`skill 6 trained-cost`,
//! `level 20 xp`, `starter-area arwic at`, `heritage-template aluvian axeman strength`), settled as
//! every other conflict is ([`crate::conflict`]).
//!
//! **Each era's layout:** a table is written in the layout of the world's files. Character creation
//! is written only in the end of retail's.

use std::collections::BTreeMap;

use dereth_assets::tables::{
    CharGen, CharGenTemplate, SkillBase, SkillFormula, SkillTable, SpellTable, StarterArea, XpTable,
};
use dereth_assets::Decode;
use dereth_pack::provenance::Provenance;
use dereth_primitives::{CellId, DataId, EraId, Frame, Position, Vec3};
use serde::Deserialize;

use crate::conflict::{self, Claim, Item, Override, Settled};
use crate::error::{Error, Loc};
use crate::source::{Located, Pack};

/// The character-creation table.
pub const CHARGEN: u32 = 0x0E00_0002;
/// The skill table.
pub const SKILLS: u32 = 0x0E00_0004;
/// The spell table.
pub const SPELLS: u32 = 0x0E00_000E;
/// The experience table.
pub const EXPERIENCE: u32 = 0x0E00_0018;

/// A row of one of the tables.
#[derive(Clone, Debug)]
pub enum TableRow {
    Skill(SkillRow),
    Experience(ExperienceRow),
    Level(LevelRow),
    Spell(SpellRow),
    StarterArea(StarterAreaRow),
    Heritage(HeritageRow),
    HeritageSkill(HeritageSkillRow),
    HeritageTemplate(HeritageTemplateRow),
}

impl TableRow {
    /// Where the row says it came from; `None` when a person wrote it.
    #[must_use]
    pub fn prov(&self) -> Option<&Provenance> {
        match self {
            Self::Skill(r) => r.prov.as_ref(),
            Self::Experience(r) => r.prov.as_ref(),
            Self::Level(r) => r.prov.as_ref(),
            Self::Spell(r) => r.prov.as_ref(),
            Self::StarterArea(r) => r.prov.as_ref(),
            Self::Heritage(r) => r.prov.as_ref(),
            Self::HeritageSkill(r) => r.prov.as_ref(),
            Self::HeritageTemplate(r) => r.prov.as_ref(),
        }
    }
}

/// `[[skill]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillRow {
    pub skill: Option<String>,
    pub id: Option<u32>,
    pub like: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<u32>,
    pub trained_cost: Option<i32>,
    pub specialized_cost: Option<i32>,
    pub category: Option<u32>,
    pub chargen_use: Option<i32>,
    pub min_level: Option<u32>,
    pub formula: Option<FormulaRow>,
    pub upper_bound: Option<f64>,
    pub lower_bound: Option<f64>,
    pub learn_mod: Option<f64>,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// A skill's formula: `(multiplier × first + multiplier × second + add) / divisor`, rounded.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FormulaRow {
    #[serde(default)]
    pub attributes: Vec<String>,
    pub multipliers: Option<Vec<u32>>,
    #[serde(default)]
    pub add: u32,
    pub divisor: u32,
}

/// `[[experience]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperienceRow {
    pub max_level: Option<u32>,
    pub attribute: Option<Vec<u32>>,
    pub vital: Option<Vec<u32>>,
    pub trained: Option<Vec<u32>>,
    pub specialized: Option<Vec<u32>>,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// `[[level]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelRow {
    pub level: u32,
    pub xp: Option<u64>,
    pub credits: Option<u32>,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// `[[spell]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellRow {
    pub id: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub school: Option<u32>,
    pub icon: Option<u32>,
    pub category: Option<u32>,
    pub bitfield: Option<u32>,
    pub mana: Option<i32>,
    pub range_constant: Option<f32>,
    pub range_mod: Option<f32>,
    pub power: Option<i32>,
    pub economy_mod: Option<f32>,
    pub formula_version: Option<u32>,
    pub component_loss: Option<f32>,
    pub duration: Option<f64>,
    pub components: Option<Vec<u32>>,
    pub caster_effect: Option<u32>,
    pub target_effect: Option<u32>,
    pub fizzle_effect: Option<u32>,
    pub recovery_interval: Option<f64>,
    pub recovery_amount: Option<f32>,
    pub display_order: Option<i32>,
    pub mana_mod: Option<i32>,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// `[[starter-area]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarterAreaRow {
    pub name: String,
    pub heritages: Option<toml::Value>,
    pub at: Option<Vec<AreaPlaceRow>>,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// One place a starter area starts characters: a cell, landblock meters (the height a number or
/// `"ground"`) and a heading in degrees clockwise from north.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaPlaceRow {
    pub cell: u32,
    pub at: Vec<toml::Value>,
    #[serde(default)]
    pub heading: f32,
}

/// `[[heritage]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeritageRow {
    pub heritage: String,
    pub attribute_credits: Option<u32>,
    pub skill_credits: Option<u32>,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// `[[heritage-skill]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeritageSkillRow {
    pub heritage: String,
    pub skill: String,
    pub trained_cost: Option<i32>,
    pub specialized_cost: Option<i32>,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// `[[heritage-template]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeritageTemplateRow {
    pub heritage: String,
    pub name: String,
    pub icon: Option<u32>,
    pub title: Option<u32>,
    pub strength: Option<u32>,
    pub endurance: Option<u32>,
    pub coordination: Option<u32>,
    pub quickness: Option<u32>,
    pub focus: Option<u32>,
    #[serde(rename = "self")]
    pub self_: Option<u32>,
    pub trained: Option<Vec<String>>,
    pub specialized: Option<Vec<String>>,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

/// The tables as the world below the packs holds them, and the ground under an outdoor place.
pub(crate) trait Below {
    /// A portal-file record and its version.
    fn portal(&self, id: u32) -> Option<(Vec<u8>, u16)>;
    /// The ground's height at landblock meters `x`, `y` of outdoor cell `cell`; `None` indoors or
    /// where the files have no such landblock.
    fn ground(&self, cell: u32, x: f32, y: f32) -> Option<f32>;
}

/// A table written: its id, its bytes in the world's layout and its version.
pub(crate) struct Written {
    pub id: u32,
    pub bytes: Vec<u8>,
    pub version: u16,
    /// What changed, for the build's report.
    pub summary: String,
}

/// A name as the tables compare names: lowercase, with no spaces.
fn key_of(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// One field a row sets: the item packs can both set, what the claim says, and its value.
struct Set<'r, R> {
    item: String,
    at: &'r Loc,
    pack: &'r str,
    value: String,
    row: R,
}

/// The row whose value stands for each item, the items in the order they were first set.
fn winners<'r, R>(
    overrides: &[Override],
    sets: Vec<Set<'r, R>>,
    settled: &mut Settled,
) -> Vec<(String, R, &'r Loc)> {
    let mut order: Vec<String> = Vec::new();
    let mut by: BTreeMap<String, Vec<Set<'r, R>>> = BTreeMap::new();
    for s in sets {
        if !by.contains_key(&s.item) {
            order.push(s.item.clone());
        }
        by.entry(s.item.clone()).or_default().push(s);
    }
    order
        .into_iter()
        .map(|item| {
            let list = by.remove(&item).unwrap_or_default();
            let claims: Vec<Claim> = list
                .iter()
                .map(|s| Claim::new(s.pack, s.at, format!("sets {}", s.item), s.value.clone()))
                .collect();
            let w = conflict::settle(overrides, &Item::new(item.clone()), &claims, settled);
            let mut list = list;
            let s = list.swap_remove(w);
            (item, s.row, s.at)
        })
        .collect()
}

/// Each pack's rows of one kind, with the pack's name and the row's place.
fn rows_of<R>(packs: &[Pack], pick: impl Fn(&TableRow) -> Option<&R>) -> Vec<(&str, &Loc, &R)> {
    packs
        .iter()
        .flat_map(|p| {
            p.tables
                .iter()
                .filter_map(|Located { at, item }| pick(item).map(|r| (p.name.as_str(), at, r)))
        })
        .collect()
}

/// Compile every table the packs change, over the tables `below` holds, in the layout of `era`'s
/// files; each conflict between packs goes into `settled`.
pub(crate) fn compile(
    packs: &[Pack],
    overrides: &[Override],
    era: EraId,
    below: &dyn Below,
    settled: &mut Settled,
) -> Result<Vec<Written>, Error> {
    let mut out = Vec::new();
    let skills = skills(packs, overrides, era, below, settled, &mut out)?;
    experience(packs, overrides, era, below, settled, &mut out)?;
    spells(packs, overrides, era, below, settled, &mut out)?;
    chargen(
        packs,
        overrides,
        era,
        below,
        settled,
        skills.as_ref(),
        &mut out,
    )?;
    Ok(out)
}

/// The record `id` below, decoded in `era`'s layout, and its version.
fn read<T: Decode>(
    below: &dyn Below,
    era: EraId,
    id: u32,
    what: &str,
    at: &Loc,
) -> Result<(T, u16), Error> {
    let Some((bytes, version)) = below.portal(id) else {
        return Err(Error::source(
            at,
            format!("the files have no {what} ({id:#010X})"),
        ));
    };
    let t = T::decode_payload_for(era, DataId(id), &bytes)
        .map_err(|e| Error::source(at, format!("the {what} ({id:#010X}) does not read: {e}")))?;
    Ok((t, version))
}

fn written(
    at: &Loc,
    id: u32,
    what: &str,
    r: Result<Vec<u8>, dereth_assets::AssetError>,
) -> Result<Vec<u8>, Error> {
    r.map_err(|e| {
        Error::source(
            at,
            format!("the {what} ({id:#010X}) cannot be written: {e}"),
        )
    })
}

// ---------------------------------------------------------------------------------------------
// skills
// ---------------------------------------------------------------------------------------------

/// The attribute numbers formulas use.
const ATTRIBUTES: [(&str, u32); 6] = [
    ("strength", 1),
    ("endurance", 2),
    ("quickness", 3),
    ("coordination", 4),
    ("focus", 5),
    ("self", 6),
];

fn formula_of(f: &FormulaRow, at: &Loc) -> Result<SkillFormula, Error> {
    if f.attributes.len() > 2 {
        return Err(Error::source(at, "a formula has at most two attributes"));
    }
    let mut attrs = [0u32; 2];
    for (i, a) in f.attributes.iter().enumerate() {
        attrs[i] = ATTRIBUTES
            .iter()
            .find(|(n, _)| *n == key_of(a))
            .map(|(_, v)| *v)
            .ok_or_else(|| {
                Error::source(
                    at,
                    format!("{a:?} is not an attribute: Strength, Endurance, Quickness, Coordination, Focus or Self"),
                )
            })?;
    }
    let mut mult = [0u32; 2];
    match &f.multipliers {
        Some(m) if m.len() != f.attributes.len() => {
            return Err(Error::source(
                at,
                "a formula has one multiplier for each attribute",
            ));
        }
        Some(m) => mult[..m.len()].copy_from_slice(m),
        None => {
            for m in mult.iter_mut().take(f.attributes.len()) {
                *m = 1;
            }
        }
    }
    Ok(SkillFormula {
        w: f.add,
        x: mult[0],
        y: mult[1],
        z: f.divisor,
        attr1: attrs[0],
        attr2: attrs[1],
    })
}

/// A new skill with nothing copied: no formula, usable untrained, a non-weapon skill.
fn new_skill(name: &str) -> SkillBase {
    SkillBase {
        description: String::new(),
        name: name.to_owned(),
        icon: 0,
        trained_cost: 0,
        specialized_cost: 0,
        category: 2,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 0,
            x: 0,
            y: 0,
            z: 0,
            attr1: 0,
            attr2: 0,
        },
        upper_bound: 0.0,
        lower_bound: 0.0,
        learn_mod: 1.0,
    }
}

/// The skill named `name` in `t` (spaces and case aside).
fn skill_named(t: &SkillTable, name: &str) -> Option<u32> {
    let want = key_of(name);
    t.skills
        .iter()
        .find(|(_, s)| !s.name.trim().is_empty() && key_of(&s.name) == want)
        .map(|(id, _)| *id)
}

/// The skill table as the packs leave it (`None` when no pack changes it, or `Some` with the
/// table below when they don't but character creation needs its names).
fn skills(
    packs: &[Pack],
    overrides: &[Override],
    era: EraId,
    below: &dyn Below,
    settled: &mut Settled,
    out: &mut Vec<Written>,
) -> Result<Option<SkillTable>, Error> {
    let rows = rows_of(packs, |r| match r {
        TableRow::Skill(s) => Some(s),
        _ => None,
    });
    // Character creation names skills too: the table is read for their names even when no
    // row changes it.
    let named_by = packs.iter().flat_map(|p| p.tables.iter()).find(|r| {
        matches!(
            r.item,
            TableRow::HeritageSkill(_) | TableRow::HeritageTemplate(_)
        )
    });
    let Some(first) = rows
        .first()
        .map(|(_, at, _)| *at)
        .or(named_by.map(|r| &r.at))
    else {
        return Ok(None);
    };
    let (mut t, version): (SkillTable, u16) = read(below, era, SKILLS, "skill table", first)?;
    if rows.is_empty() {
        return Ok(Some(t));
    }
    // Each row's skill: its number, or its name among the table's and the numbered rows' names.
    let named: BTreeMap<String, u32> = rows
        .iter()
        .filter_map(|(_, _, r)| Some((key_of(r.name.as_deref()?), r.id?)))
        .collect();
    let mut sets = Vec::new();
    for (pack, at, r) in &rows {
        let id = match (r.id, &r.skill) {
            (Some(id), _) => id,
            (None, Some(name)) => skill_named(&t, name)
                .or_else(|| named.get(&key_of(name)).copied())
                .ok_or_else(|| {
                    Error::source(
                        at,
                        format!("the skill table has no skill {name:?}; a new skill is given its number (id)"),
                    )
                })?,
            (None, None) => return Err(Error::source(at, "a skill row names its skill (skill) or its number (id)")),
        };
        let mut set = |field: &str, value: String| {
            sets.push(Set {
                item: format!("skill {id} {field}"),
                at,
                pack,
                value,
                row: (id, *r),
            });
        };
        if r.remove {
            set("remove", "true".into());
        }
        if let Some(v) = &r.like {
            set("like", format!("{v:?}"));
        }
        if let Some(v) = &r.name {
            set("name", format!("{v:?}"));
        }
        if let Some(v) = &r.description {
            set("description", format!("{v:?}"));
        }
        if let Some(v) = r.icon {
            set("icon", v.to_string());
        }
        if let Some(v) = r.trained_cost {
            set("trained-cost", v.to_string());
        }
        if let Some(v) = r.specialized_cost {
            set("specialized-cost", v.to_string());
        }
        if let Some(v) = r.category {
            set("category", v.to_string());
        }
        if let Some(v) = r.chargen_use {
            set("chargen-use", v.to_string());
        }
        if let Some(v) = r.min_level {
            set("min-level", v.to_string());
        }
        if let Some(v) = &r.formula {
            set("formula", format!("{v:?}"));
        }
        if let Some(v) = r.upper_bound {
            set("upper-bound", v.to_string());
        }
        if let Some(v) = r.lower_bound {
            set("lower-bound", v.to_string());
        }
        if let Some(v) = r.learn_mod {
            set("learn-mod", v.to_string());
        }
    }
    let below_t = t.clone();
    let mut changed = 0;
    for (item, (id, r), at) in winners(overrides, sets, settled) {
        changed += 1;
        let field = item.rsplit(' ').next().unwrap_or_default();
        if field == "remove" {
            t.skills.remove(&id);
            continue;
        }
        if let std::collections::btree_map::Entry::Vacant(slot) = t.skills.entry(id) {
            let start = match &r.like {
                Some(like) => {
                    let from = skill_named(&below_t, like).ok_or_else(|| {
                        Error::source(
                            at,
                            format!("the skill table has no skill {like:?} to start from"),
                        )
                    })?;
                    let mut s = below_t.skills[&from].clone();
                    if let Some(name) = &r.name {
                        s.name.clone_from(name);
                    }
                    s
                }
                None => {
                    let Some(name) = &r.name else {
                        return Err(Error::source(
                            at,
                            format!("skill {id} is new: it has a name"),
                        ));
                    };
                    new_skill(name)
                }
            };
            slot.insert(start);
        }
        let s = t.skills.get_mut(&id).expect("inserted above");
        match field {
            "like" => {}
            "name" => s.name.clone_from(r.name.as_ref().expect("set")),
            "description" => s
                .description
                .clone_from(r.description.as_ref().expect("set")),
            "icon" => s.icon = r.icon.expect("set"),
            "trained-cost" => s.trained_cost = r.trained_cost.expect("set"),
            "specialized-cost" => s.specialized_cost = r.specialized_cost.expect("set"),
            "category" => s.category = r.category.expect("set"),
            "chargen-use" => s.chargen_use = r.chargen_use.expect("set"),
            "min-level" => s.min_level = r.min_level.expect("set"),
            "formula" => s.formula = formula_of(r.formula.as_ref().expect("set"), at)?,
            "upper-bound" => s.upper_bound = r.upper_bound.expect("set"),
            "lower-bound" => s.lower_bound = r.lower_bound.expect("set"),
            "learn-mod" => s.learn_mod = r.learn_mod.expect("set"),
            _ => {}
        }
    }
    let bytes = written(first, SKILLS, "skill table", t.encode_for(era))?;
    out.push(Written {
        id: SKILLS,
        bytes,
        version,
        summary: format!("skill table: {changed} field(s) set"),
    });
    Ok(Some(t))
}

// ---------------------------------------------------------------------------------------------
// experience
// ---------------------------------------------------------------------------------------------

/// What an `[[experience]]` or `[[level]]` row sets.
enum XpRow<'r> {
    Table(&'r ExperienceRow),
    Level(&'r LevelRow),
}

fn experience(
    packs: &[Pack],
    overrides: &[Override],
    era: EraId,
    below: &dyn Below,
    settled: &mut Settled,
    out: &mut Vec<Written>,
) -> Result<(), Error> {
    let mut rows: Vec<(&str, &Loc, XpRow<'_>)> = Vec::new();
    for p in packs {
        for r in &p.tables {
            match &r.item {
                TableRow::Experience(e) => rows.push((&p.name, &r.at, XpRow::Table(e))),
                TableRow::Level(l) => rows.push((&p.name, &r.at, XpRow::Level(l))),
                _ => {}
            }
        }
    }
    let Some(first) = rows.first().map(|(_, at, _)| *at) else {
        return Ok(());
    };
    let (mut t, version): (XpTable, u16) = read(below, era, EXPERIENCE, "experience table", first)?;
    let mut sets = Vec::new();
    for (pack, at, r) in &rows {
        let mut set = |item: String, value: String| {
            sets.push(Set {
                item,
                at,
                pack,
                value,
                row: r,
            });
        };
        match r {
            XpRow::Table(e) => {
                if let Some(v) = e.max_level {
                    set("experience max-level".into(), v.to_string());
                }
                for (name, list) in [
                    ("attribute", &e.attribute),
                    ("vital", &e.vital),
                    ("trained", &e.trained),
                    ("specialized", &e.specialized),
                ] {
                    if let Some(v) = list {
                        set(format!("experience {name}"), format!("{v:?}"));
                    }
                }
            }
            XpRow::Level(l) => {
                if l.level == 0 {
                    return Err(Error::source(at, "levels count from 1"));
                }
                if let Some(v) = l.xp {
                    set(format!("level {} xp", l.level), v.to_string());
                }
                if let Some(v) = l.credits {
                    set(format!("level {} credits", l.level), v.to_string());
                }
            }
        }
    }
    let won = winners(overrides, sets, settled);
    let changed = won.len();
    // The cap first, so that a level row may set a level the cap adds.
    let mut set_xp: BTreeMap<usize, &Loc> = BTreeMap::new();
    let old_len = t.level_xp.len();
    for (item, r, _) in &won {
        if let (XpRow::Table(e), "experience max-level") = (r, item.as_str()) {
            let n = usize::try_from(e.max_level.expect("set")).unwrap_or(usize::MAX) + 1;
            t.level_xp.resize(n, 0);
            t.level_credits.resize(n, 0);
        }
    }
    for (item, r, at) in &won {
        match r {
            XpRow::Table(e) => match item.as_str() {
                "experience attribute" => t
                    .attribute_xp
                    .clone_from(e.attribute.as_ref().expect("set")),
                "experience vital" => t.vital_xp.clone_from(e.vital.as_ref().expect("set")),
                "experience trained" => t.trained_xp.clone_from(e.trained.as_ref().expect("set")),
                "experience specialized" => {
                    t.specialized_xp
                        .clone_from(e.specialized.as_ref().expect("set"));
                }
                _ => {}
            },
            XpRow::Level(l) => {
                let n = usize::try_from(l.level).unwrap_or(usize::MAX);
                if n >= t.level_xp.len() {
                    return Err(Error::source(
                        at,
                        format!(
                            "level {} is past the highest level, {}",
                            l.level,
                            t.level_xp.len().saturating_sub(1)
                        ),
                    ));
                }
                if item.ends_with(" xp") {
                    t.level_xp[n] = l.xp.expect("set");
                    set_xp.insert(n, at);
                } else {
                    t.level_credits[n] = l.credits.expect("set");
                }
            }
        }
    }
    // Every level the cap adds is given its experience, and the curve never falls.
    for n in old_len..t.level_xp.len() {
        if !set_xp.contains_key(&n) {
            return Err(Error::source(
                first,
                format!("level {n} is new: a [[level]] row gives its experience"),
            ));
        }
    }
    for n in 2..t.level_xp.len() {
        if t.level_xp[n] < t.level_xp[n - 1] {
            let at = set_xp
                .get(&n)
                .or_else(|| set_xp.get(&(n - 1)))
                .copied()
                .unwrap_or(first);
            return Err(Error::source(
                at,
                format!(
                    "level {n} takes less experience ({}) than level {} ({})",
                    t.level_xp[n],
                    n - 1,
                    t.level_xp[n - 1]
                ),
            ));
        }
    }
    let bytes = written(first, EXPERIENCE, "experience table", t.encode_for(era))?;
    out.push(Written {
        id: EXPERIENCE,
        bytes,
        version,
        summary: format!(
            "experience table: {changed} field(s) set, {} levels",
            t.level_xp.len().saturating_sub(1)
        ),
    });
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// spells
// ---------------------------------------------------------------------------------------------

fn spells(
    packs: &[Pack],
    overrides: &[Override],
    era: EraId,
    below: &dyn Below,
    settled: &mut Settled,
    out: &mut Vec<Written>,
) -> Result<(), Error> {
    let rows = rows_of(packs, |r| match r {
        TableRow::Spell(s) => Some(s),
        _ => None,
    });
    let Some(first) = rows.first().map(|(_, at, _)| *at) else {
        return Ok(());
    };
    let (mut t, version): (SpellTable, u16) = read(below, era, SPELLS, "spell table", first)?;
    let mut sets = Vec::new();
    for (pack, at, r) in &rows {
        let Some(s) = t.spells.get(&r.id) else {
            return Err(Error::source(
                at,
                format!(
                    "the spell table has no spell {}; a spell is changed, not added",
                    r.id
                ),
            ));
        };
        if r.duration.is_some() && s.duration.is_none() {
            return Err(Error::source(
                at,
                format!("spell {} does not run out: it has no duration", r.id),
            ));
        }
        if r.components.as_ref().is_some_and(|c| c.len() > 8) {
            return Err(Error::source(at, "a spell has at most eight components"));
        }
        let mut set = |field: &str, value: String| {
            sets.push(Set {
                item: format!("spell {} {field}", r.id),
                at,
                pack,
                value,
                row: *r,
            });
        };
        macro_rules! fields {
            ($($f:ident => $name:literal),* $(,)?) => {
                $(if let Some(v) = &r.$f { set($name, format!("{v:?}")); })*
            };
        }
        fields!(
            name => "name", description => "description", school => "school", icon => "icon",
            category => "category", bitfield => "bitfield", mana => "mana",
            range_constant => "range-constant", range_mod => "range-mod", power => "power",
            economy_mod => "economy-mod", formula_version => "formula-version",
            component_loss => "component-loss", duration => "duration", components => "components",
            caster_effect => "caster-effect", target_effect => "target-effect",
            fizzle_effect => "fizzle-effect", recovery_interval => "recovery-interval",
            recovery_amount => "recovery-amount", display_order => "display-order",
            mana_mod => "mana-mod",
        );
    }
    let won = winners(overrides, sets, settled);
    let changed = won.len();
    for (item, r, _) in won {
        let s = t.spells.get_mut(&r.id).expect("checked above");
        match item.rsplit(' ').next().unwrap_or_default() {
            "name" => s.name.clone_from(r.name.as_ref().expect("set")),
            "description" => s
                .description
                .clone_from(r.description.as_ref().expect("set")),
            "school" => s.school = r.school.expect("set"),
            "icon" => s.icon = r.icon.expect("set"),
            "category" => s.category = r.category.expect("set"),
            "bitfield" => s.bitfield = r.bitfield.expect("set"),
            "mana" => s.base_mana = r.mana.expect("set"),
            "range-constant" => s.base_range_constant = r.range_constant.expect("set"),
            "range-mod" => s.base_range_mod = r.range_mod.expect("set"),
            "power" => s.power = r.power.expect("set"),
            "economy-mod" => s.spell_economy_mod = r.economy_mod.expect("set"),
            "formula-version" => s.formula_version = r.formula_version.expect("set"),
            "component-loss" => s.component_loss = r.component_loss.expect("set"),
            "duration" => {
                if let Some(d) = &mut s.duration {
                    d.0 = r.duration.expect("set");
                }
            }
            "components" => s.comps.clone_from(r.components.as_ref().expect("set")),
            "caster-effect" => s.caster_effect = r.caster_effect.expect("set"),
            "target-effect" => s.target_effect = r.target_effect.expect("set"),
            "fizzle-effect" => s.fizzle_effect = r.fizzle_effect.expect("set"),
            "recovery-interval" => s.recovery_interval = r.recovery_interval.expect("set"),
            "recovery-amount" => s.recovery_amount = r.recovery_amount.expect("set"),
            "display-order" => s.display_order = r.display_order.expect("set"),
            "mana-mod" => s.mana_mod = r.mana_mod.expect("set"),
            _ => {}
        }
    }
    let bytes = written(first, SPELLS, "spell table", t.encode_for(era))?;
    out.push(Written {
        id: SPELLS,
        bytes,
        version,
        summary: format!("spell table: {changed} field(s) set"),
    });
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// character creation
// ---------------------------------------------------------------------------------------------

/// The heritage of `cg` named `name` (spaces and case aside).
fn heritage_named(cg: &CharGen, name: &str, at: &Loc) -> Result<u32, Error> {
    let want = key_of(name);
    cg.heritage_groups
        .iter()
        .find(|(_, h)| key_of(&h.name) == want)
        .map(|(k, _)| *k)
        .ok_or_else(|| {
            let known: Vec<&str> = cg
                .heritage_groups
                .values()
                .map(|h| h.name.as_str())
                .collect();
            Error::source(
                at,
                format!("{name:?} is not a heritage; the heritages are {known:?}"),
            )
        })
}

fn skill_id(skills: Option<&SkillTable>, name: &str, at: &Loc) -> Result<u32, Error> {
    skills
        .and_then(|t| skill_named(t, name))
        .ok_or_else(|| Error::source(at, format!("the skill table has no skill {name:?}")))
}

/// A starter area's places, as positions.
fn places(rows: &[AreaPlaceRow], below: &dyn Below, at: &Loc) -> Result<Vec<Position>, Error> {
    rows.iter()
        .map(|p| {
            let num = |v: &toml::Value| match v {
                #[allow(clippy::cast_possible_truncation)]
                toml::Value::Float(f) => Some(*f as f32),
                #[allow(clippy::cast_precision_loss)]
                toml::Value::Integer(i) => Some(*i as f32),
                _ => None,
            };
            let [x, y, z] = p.at.as_slice() else {
                return Err(Error::source(
                    at,
                    "a place is [x, y, z]: landblock meters, z a height or \"ground\"",
                ));
            };
            let (Some(x), Some(y)) = (num(x), num(y)) else {
                return Err(Error::source(at, "a place's x and y are numbers"));
            };
            let z = match z {
                toml::Value::String(s) if s == "ground" => {
                    below.ground(p.cell, x, y).ok_or_else(|| {
                        Error::source(at, format!("cell {:#010X} has no ground under it", p.cell))
                    })?
                }
                v => num(v)
                    .ok_or_else(|| Error::source(at, "a place's z is a number or \"ground\""))?,
            };
            Ok(Position::new(
                CellId(p.cell),
                Frame {
                    origin: Vec3::new(x, y, z),
                    rotation: crate::compile::heading_quat(p.heading),
                },
            ))
        })
        .collect()
}

/// What one row of character creation names, settled per field.
#[derive(Clone, Copy)]
enum CgRow<'r> {
    Area(&'r StarterAreaRow),
    Heritage(u32, &'r HeritageRow),
    HeritageSkill(u32, u32, &'r HeritageSkillRow),
    Template(u32, &'r HeritageTemplateRow),
}

#[allow(clippy::too_many_lines)]
fn chargen<'r>(
    packs: &'r [Pack],
    overrides: &[Override],
    era: EraId,
    below: &dyn Below,
    settled: &mut Settled,
    skills: Option<&SkillTable>,
    out: &mut Vec<Written>,
) -> Result<(), Error> {
    let mut rows: Vec<(&'r str, &'r Loc, &'r TableRow)> = Vec::new();
    for p in packs {
        for r in &p.tables {
            if matches!(
                r.item,
                TableRow::StarterArea(_)
                    | TableRow::Heritage(_)
                    | TableRow::HeritageSkill(_)
                    | TableRow::HeritageTemplate(_)
            ) {
                rows.push((&p.name, &r.at, &r.item));
            }
        }
    }
    let Some(first) = rows.first().map(|(_, at, _)| *at) else {
        return Ok(());
    };
    if !matches!(era, EraId::Eor | EraId::NewDereth) {
        return Err(Error::source(
            first,
            "character creation is written only in the end of retail's files",
        ));
    }
    let (mut cg, version): (CharGen, u16) =
        read(below, era, CHARGEN, "character-creation table", first)?;
    let names: BTreeMap<u32, String> = cg
        .heritage_groups
        .iter()
        .map(|(k, h)| (*k, key_of(&h.name)))
        .collect();
    let hname = |hk: u32| names.get(&hk).cloned().unwrap_or_default();
    let mut sets = Vec::new();
    for (pack, at, r) in &rows {
        let mut set = |item: String, value: String, row: CgRow<'r>| {
            sets.push(Set {
                item,
                at,
                pack,
                value,
                row,
            });
        };
        match r {
            TableRow::StarterArea(a) => {
                let k = key_of(&a.name);
                let row = CgRow::Area(a);
                if a.remove {
                    set(format!("starter-area {k} remove"), "true".into(), row);
                }
                if let Some(v) = &a.at {
                    set(format!("starter-area {k} at"), format!("{v:?}"), row);
                }
                if let Some(v) = &a.heritages {
                    set(format!("starter-area {k} heritages"), v.to_string(), row);
                }
            }
            TableRow::Heritage(h) => {
                let hk = heritage_named(&cg, &h.heritage, at)?;
                let row = CgRow::Heritage(hk, h);
                if h.remove {
                    set(format!("heritage {} remove", hname(hk)), "true".into(), row);
                }
                if let Some(v) = h.attribute_credits {
                    set(
                        format!("heritage {} attribute-credits", hname(hk)),
                        v.to_string(),
                        row,
                    );
                }
                if let Some(v) = h.skill_credits {
                    set(
                        format!("heritage {} skill-credits", hname(hk)),
                        v.to_string(),
                        row,
                    );
                }
            }
            TableRow::HeritageSkill(s) => {
                let hk = heritage_named(&cg, &s.heritage, at)?;
                let sk = skill_id(skills, &s.skill, at)?;
                let row = CgRow::HeritageSkill(hk, sk, s);
                if s.remove {
                    set(
                        format!("heritage-skill {} {sk} remove", hname(hk)),
                        "true".into(),
                        row,
                    );
                }
                if let Some(v) = s.trained_cost {
                    set(
                        format!("heritage-skill {} {sk} trained-cost", hname(hk)),
                        v.to_string(),
                        row,
                    );
                }
                if let Some(v) = s.specialized_cost {
                    set(
                        format!("heritage-skill {} {sk} specialized-cost", hname(hk)),
                        v.to_string(),
                        row,
                    );
                }
            }
            TableRow::HeritageTemplate(t) => {
                let hk = heritage_named(&cg, &t.heritage, at)?;
                let k = format!("heritage-template {} {}", hname(hk), key_of(&t.name));
                let row = CgRow::Template(hk, t);
                if t.remove {
                    set(format!("{k} remove"), "true".into(), row);
                }
                for (name, v) in [
                    ("icon", t.icon),
                    ("title", t.title),
                    ("strength", t.strength),
                    ("endurance", t.endurance),
                    ("coordination", t.coordination),
                    ("quickness", t.quickness),
                    ("focus", t.focus),
                    ("self", t.self_),
                ] {
                    if let Some(v) = v {
                        set(format!("{k} {name}"), v.to_string(), row);
                    }
                }
                for (name, v) in [("trained", &t.trained), ("specialized", &t.specialized)] {
                    if let Some(v) = v {
                        for s in v {
                            skill_id(skills, s, at)?;
                        }
                        set(format!("{k} {name}"), format!("{v:?}"), row);
                    }
                }
            }
            _ => {}
        }
    }
    let won = winners(overrides, sets, settled);
    let changed = won.len();

    // Starter areas: those removed go, those named are changed or added, then each heritage offers
    // the areas the rows say.
    let mut areas: Vec<(StarterArea, Option<usize>)> = cg
        .starter_areas
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, a)| (a, Some(i)))
        .collect();
    let mut offered: Vec<(String, Option<toml::Value>, &Loc)> = Vec::new();
    for (item, row, at) in &won {
        let CgRow::Area(a) = row else { continue };
        let k = key_of(&a.name);
        let field = item.rsplit(' ').next().unwrap_or_default();
        let found = areas.iter().position(|(x, _)| key_of(&x.name) == k);
        match field {
            "remove" => {
                if let Some(i) = found {
                    areas.remove(i);
                } else {
                    return Err(Error::source(
                        at,
                        format!("there is no starter area {:?} to take out", a.name),
                    ));
                }
            }
            "at" => {
                let locations = places(a.at.as_deref().unwrap_or_default(), below, at)?;
                match found {
                    Some(i) => areas[i].0.locations = locations,
                    None => {
                        areas.push((
                            StarterArea {
                                name: a.name.clone(),
                                locations,
                            },
                            None,
                        ));
                        if a.heritages.is_none() {
                            offered.push((k.clone(), None, at));
                        }
                    }
                }
            }
            "heritages" => offered.push((k.clone(), a.heritages.clone(), at)),
            _ => {}
        }
    }
    // The old index of each area to its new one; an area a row named heritages for but no place
    // is an error when it is new.
    let remap: BTreeMap<u32, u32> = areas
        .iter()
        .enumerate()
        .filter_map(|(new, (_, old))| {
            Some((u32::try_from((*old)?).ok()?, u32::try_from(new).ok()?))
        })
        .collect();
    for h in cg.heritage_groups.values_mut() {
        for list in [&mut h.primary_start_areas, &mut h.secondary_start_areas] {
            *list = list.iter().filter_map(|i| remap.get(i).copied()).collect();
        }
    }
    for (k, heritages, at) in offered {
        let Some(index) = areas.iter().position(|(a, _)| key_of(&a.name) == k) else {
            return Err(Error::source(at, "a new starter area has its places (at)"));
        };
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let all: Vec<u32> = cg.heritage_groups.keys().copied().collect();
        let offering: Vec<u32> = match &heritages {
            None => all,
            Some(toml::Value::String(s)) if s == "all" => all,
            Some(toml::Value::Array(list)) => list
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| Error::source(at, "a heritage is named"))
                        .and_then(|n| heritage_named(&cg, n, at))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => {
                return Err(Error::source(
                    at,
                    "heritages is \"all\" or a list of heritages",
                ))
            }
        };
        for (hk, h) in &mut cg.heritage_groups {
            let has =
                h.primary_start_areas.contains(&index) || h.secondary_start_areas.contains(&index);
            if offering.contains(hk) {
                if !has {
                    h.primary_start_areas.push(index);
                }
            } else {
                h.primary_start_areas.retain(|i| *i != index);
                h.secondary_start_areas.retain(|i| *i != index);
            }
        }
    }
    cg.starter_areas = areas.into_iter().map(|(a, _)| a).collect();

    // Heritages: their credits, their skills' costs and their templates.
    for (item, row, at) in &won {
        let field = item.rsplit(' ').next().unwrap_or_default();
        match *row {
            CgRow::Area(_) => {}
            CgRow::Heritage(hk, h) => {
                let g = cg.heritage_groups.get_mut(&hk).expect("named");
                match field {
                    "attribute-credits" => g.attribute_credits = h.attribute_credits.expect("set"),
                    "skill-credits" => g.skill_credits = h.skill_credits.expect("set"),
                    _ => {}
                }
            }
            CgRow::HeritageSkill(hk, sk, s) => {
                let g = cg.heritage_groups.get_mut(&hk).expect("named");
                let found = g.skills.iter().position(|(id, _, _)| *id == sk);
                if field == "remove" {
                    if let Some(i) = found {
                        g.skills.remove(i);
                    }
                    continue;
                }
                let i = match found {
                    Some(i) => i,
                    None => {
                        let (Some(t), Some(sp)) = (s.trained_cost, s.specialized_cost) else {
                            return Err(Error::source(
                                at,
                                format!("{} is new to the heritage: it has both costs", s.skill),
                            ));
                        };
                        g.skills.push((sk, t, sp));
                        g.skills.len() - 1
                    }
                };
                match field {
                    "trained-cost" => g.skills[i].1 = s.trained_cost.expect("set"),
                    "specialized-cost" => g.skills[i].2 = s.specialized_cost.expect("set"),
                    _ => {}
                }
            }
            CgRow::Template(hk, t) => {
                let g = cg.heritage_groups.get_mut(&hk).expect("named");
                let k = key_of(&t.name);
                let found = g.templates.iter().position(|x| key_of(&x.name) == k);
                if field == "remove" {
                    match found {
                        Some(0) => {
                            return Err(Error::source(
                                at,
                                "the first template (the player's own choice) stays",
                            ))
                        }
                        Some(i) => {
                            g.templates.remove(i);
                        }
                        None => {}
                    }
                    continue;
                }
                let i = match found {
                    Some(i) => i,
                    None => {
                        g.templates.push(CharGenTemplate {
                            name: t.name.clone(),
                            icon: 0,
                            title: 0,
                            attributes: [10; 6],
                            normal_skills: Vec::new(),
                            primary_skills: Vec::new(),
                        });
                        g.templates.len() - 1
                    }
                };
                let x = &mut g.templates[i];
                let ids = |list: &Option<Vec<String>>| -> Result<Vec<u32>, Error> {
                    list.as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(|s| skill_id(skills, s, at))
                        .collect()
                };
                match field {
                    "icon" => x.icon = t.icon.expect("set"),
                    "title" => x.title = t.title.expect("set"),
                    "strength" => x.attributes[0] = t.strength.expect("set"),
                    "endurance" => x.attributes[1] = t.endurance.expect("set"),
                    "coordination" => x.attributes[2] = t.coordination.expect("set"),
                    "quickness" => x.attributes[3] = t.quickness.expect("set"),
                    "focus" => x.attributes[4] = t.focus.expect("set"),
                    "self" => x.attributes[5] = t.self_.expect("set"),
                    "trained" => x.normal_skills = ids(&t.trained)?,
                    "specialized" => x.primary_skills = ids(&t.specialized)?,
                    _ => {}
                }
            }
        }
    }
    // Heritages taken out go last: what a row changed of one goes with it.
    for (item, row, at) in &won {
        if let (CgRow::Heritage(hk, _), true) = (row, item.ends_with(" remove")) {
            if cg.heritage_groups.len() == 1 && cg.heritage_groups.contains_key(hk) {
                return Err(Error::source(
                    at,
                    "character creation keeps at least one heritage",
                ));
            }
            cg.heritage_groups.remove(hk);
            cg.heritage_order.retain(|k| k != hk);
        }
    }
    out.push(Written {
        id: CHARGEN,
        bytes: cg.encode(),
        version,
        summary: format!(
            "character creation: {changed} field(s) set, {} starter area(s)",
            cg.starter_areas.len()
        ),
    });
    Ok(())
}
