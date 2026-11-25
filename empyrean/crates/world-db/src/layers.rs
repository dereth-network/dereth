//! A world's rows as the shared pack format reads them (`dereth_pack::records`): the item each
//! row sets, which of a row's tables merge by key, and the world folded from its layers.
//!
//! Each row sets one item, named in the one form an override, a `[[remove]]` and a drift name it:
//!
//! | Rows | Item |
//! |---|---|
//! | `[[weenie]]` | `weenie <class>` |
//! | `[[placement]]` | `spawn <guid>` (`spawn 0x7A9B400A`), at its spot of its landblock |
//! | `[[encounter]]` | `encounter <landblock> <x>,<y>` (`encounter A9A0 2,0`) |
//! | `[[cook_book]]` | `cook-book <recipe> <source> <target>`: one source on one target may make two recipes |
//! | `[[recipe]]`, `[[recipe_vital]]`, `[[spell]]` | `recipe <id>`, `recipe-vital <id>`, `spell <id>` |
//! | `[[quest]]`, `[[event]]`, `[[poi]]` | `quest <name>` ..., the name in lower case |
//! | `[[house_portal]]` | `house-portal <house> <cell>` |
//! | `[[treasure_death]]`, `[[grouped_treasure]]`, `[[treasure_wielded]]` | `death-treasure <type>`, `grouped-treasure <type>`, `wielded-treasure <type>` |
//! | the gem and material tables | `gem-count <gem> <tier> <count>`, `material-base <code> <tier> <material>`, `material-color <material> <color> <palette>`, `material-group <group> <tier>` (its materials in their order, one list) |
//! | `[[loot_policy]]`, `[[version]]` | `loot-policy`, `version` |
//! | `[[rules]]`, the world's own rules (`empyrean_common::world_rules`) | `rules` |
//!
//! A weenie's property tables (`int`, `did`, `skill`, ..., `spell_book`, `book`) merge by key, so a
//! patch sets one property (`int.ItemType`); every list (a create list, the emotes, a recipe's
//! mods, a wielded type's entries) is one field, replaced whole.
//!
//! A row that says nothing of where it came from came from where its pack's header (`pack.toml`)
//! says: a base converted from another project's world says `from = "source"` there once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use dereth_pack::conflict::{Item, Override};
use dereth_pack::fold::{fold, fold_each, resolve, Folded, Input, Merge, Resolved, Snapshot};
pub use dereth_pack::fold::{Finding, Severity};
use dereth_pack::lineage::{Ack, BASE};
use dereth_pack::pack::PackHeader;
use dereth_pack::provenance::{Provenance, Source};
use dereth_pack::records::{self, Kinds, RowKinds};
use dereth_pack::Loc;
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::era::EraId;
use empyrean_content::WorldContent;
use toml::{Table, Value};

use crate::files::{rows_only, PACK};
use crate::kinds;
use crate::value::{id_of, R};
use crate::world::Ac1World;

/// The tables of a weenie that merge by key.
pub const WEENIE_MAPS: &[&str] = &[
    "int",
    "int64",
    "bool",
    "float",
    "string",
    "did",
    "iid",
    "position",
    "attribute",
    "vital",
    "skill",
    "body_part",
    "spell_book",
    "book",
];

/// The AC1 world's words for rows.
#[derive(Debug, Default, Clone, Copy)]
pub struct Ac1Rows;

fn num(t: &Table, field: &str) -> Result<u32, String> {
    t.get(field)
        .map(id_of)
        .transpose()?
        .ok_or_else(|| format!("no {field}"))
}

fn name_of(t: &Table) -> Result<String, String> {
    t.get("name")
        .and_then(Value::as_str)
        .map(|n| n.trim().to_lowercase())
        .ok_or_else(|| "no name".to_owned())
}

/// A number as an item writes it: decimal, or hexadecimal (`0x...`).
pub(crate) fn word_num(w: &str) -> Option<u32> {
    match w.strip_prefix("0x").or_else(|| w.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => w.parse().ok(),
    }
}

/// The item `text` names, in its one form; `None` when it names none.
#[must_use]
pub fn item_form(text: &str) -> Option<String> {
    let text = text.trim();
    let (head, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let rest = rest.trim();
    let words: Vec<&str> = rest.split_whitespace().collect();
    let nums = |n: usize| -> Option<Vec<u32>> {
        (words.len() == n)
            .then(|| {
                words
                    .iter()
                    .map(|w| word_num(w))
                    .collect::<Option<Vec<_>>>()
            })
            .flatten()
    };
    let joined = |v: Vec<u32>| v.iter().map(u32::to_string).collect::<Vec<_>>().join(" ");
    match head {
        "weenie" | "recipe" | "recipe-vital" | "spell" | "death-treasure" | "grouped-treasure"
        | "wielded-treasure" => nums(1).map(|v| format!("{head} {}", v[0])),
        "material-group" => nums(2).map(|v| format!("{head} {}", joined(v))),
        "cook-book" | "gem-count" | "material-base" | "material-color" => {
            nums(3).map(|v| format!("{head} {}", joined(v)))
        }
        "spawn" => nums(1).map(|v| format!("spawn {:#010X}", v[0])),
        "house-portal" => nums(2).map(|v| format!("house-portal {} {:#010X}", v[0], v[1])),
        "encounter" => {
            let [lb, xy] = words.as_slice() else {
                return None;
            };
            let lb = u16::from_str_radix(lb.trim_start_matches("0x"), 16).ok()?;
            let (x, y) = xy.split_once(',')?;
            let (x, y) = (x.parse::<i32>().ok()?, y.parse::<i32>().ok()?);
            Some(format!("encounter {lb:04X} {x},{y}"))
        }
        "quest" | "event" | "poi" | "name-table" if !rest.is_empty() => {
            Some(format!("{head} {}", rest.to_lowercase()))
        }
        "loot-policy" | "version" | "rules" if rest.is_empty() => Some(head.to_owned()),
        _ => None,
    }
}

/// The rules row's tables a layer sets one key of: each system, each formula setting and each
/// property is a field of its own.
const RULES_MAPS: &[&str] = &["features", "formulas", "properties"];

impl RowKinds for Ac1Rows {
    fn item(&self, kind: &str, t: &Table) -> Result<Item, String> {
        let n = |f: &str| num(t, f);
        // A key the row leaves out is at its default, 0.
        let z = |f: &str| -> Result<u32, String> {
            t.get(f).map(id_of).transpose().map(|v| v.unwrap_or(0))
        };
        Ok(match kind {
            "weenie" => Item::new(format!("weenie {}", n("class")?)),
            "placement" => {
                let guid = n("guid")?;
                let cell = n("cell")?;
                let at = t.get("at").and_then(Value::as_array);
                let xy = |i: usize| {
                    at.and_then(|a| a.get(i))
                        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                        .unwrap_or(0.0)
                };
                #[allow(clippy::cast_possible_truncation)]
                Item::new(format!("spawn {guid:#010X}")).at(
                    (cell >> 16) as u16,
                    xy(0) as f32,
                    xy(1) as f32,
                )
            }
            "encounter" => {
                let lb = n("landblock")?;
                let cell = t
                    .get("cell")
                    .and_then(Value::as_array)
                    .filter(|c| c.len() == 2)
                    .ok_or("an encounter's cell isn't [x, y]")?;
                let c = |i: usize| {
                    cell[i]
                        .as_integer()
                        .ok_or("an encounter's cell isn't [x, y]")
                };
                #[allow(clippy::cast_possible_truncation)]
                Item::new(format!("encounter {:04X} {},{}", lb & 0xFFFF, c(0)?, c(1)?))
                    .in_landblock(lb as u16)
            }
            "cook_book" => Item::new(format!(
                "cook-book {} {} {}",
                n("recipe")?,
                n("source")?,
                n("target")?
            )),
            "recipe" => Item::new(format!("recipe {}", n("id")?)),
            "recipe_vital" => Item::new(format!("recipe-vital {}", n("recipe")?)),
            "spell" => Item::new(format!("spell {}", n("id")?)),
            "quest" | "event" | "poi" => Item::new(format!("{kind} {}", name_of(t)?)),
            "name_table" => Item::new(format!(
                "name-table {}",
                t.get("heritage")
                    .and_then(Value::as_str)
                    .ok_or("a name table names its heritage (heritage)")?
                    .to_lowercase()
            )),
            "house_portal" => Item::new(format!(
                "house-portal {} {:#010X}",
                n("house_id")?,
                n("obj_cell_id")?
            )),
            "treasure_death" => Item::new(format!("death-treasure {}", n("type")?)),
            "grouped_treasure" => Item::new(format!("grouped-treasure {}", n("treasure_type")?)),
            "treasure_wielded" => Item::new(format!("wielded-treasure {}", n("type")?)),
            "treasure_gem_count" => Item::new(format!(
                "gem-count {} {} {}",
                z("gem_code")?,
                z("tier")?,
                z("count")?
            )),
            "treasure_material_base" => Item::new(format!(
                "material-base {} {} {}",
                z("material_code")?,
                z("tier")?,
                z("material_id")?
            )),
            "treasure_material_color" => Item::new(format!(
                "material-color {} {} {}",
                z("material_id")?,
                z("color_code")?,
                z("palette_template")?
            )),
            "treasure_material_groups" => Item::new(format!(
                "material-group {} {}",
                z("material_group")?,
                z("tier")?
            )),
            "loot_policy" => Item::new("loot-policy"),
            "rules" => Item::new("rules"),
            "version" => Item::new("version"),
            other => return Err(format!("no kind of row is called {other}")),
        })
    }

    fn is_map(&self, kind: &str, field: &str) -> bool {
        (kind == "weenie" && WEENIE_MAPS.contains(&field))
            || (kind == "rules" && RULES_MAPS.contains(&field))
    }

    fn removal(&self, text: &str) -> Result<String, String> {
        item_form(text).ok_or_else(|| format!("{text:?} names no item"))
    }
}

/// Where a pack's rows came from when they say nothing: its header's `prov`, or a person.
///
/// # Errors
/// A header that doesn't read.
pub fn pack_prov(files: &BTreeMap<String, String>) -> R<Provenance> {
    let Some(text) = files.get(PACK) else {
        return Ok(Provenance::AUTHORED);
    };
    let header = PackHeader::parse::<Ac1World>(Path::new(PACK), text).map_err(|e| e.to_string())?;
    Ok(header.prov.unwrap_or(Provenance::AUTHORED))
}

/// A pack's files (each `path → text`, its header and meta file among them) as a snapshot: a whole
/// known world, each item once, a row that says nothing of where it came from from where the
/// pack's header says.
///
/// # Errors
/// A header or file that doesn't read, a row that names no item, two rows of one item.
pub fn snapshot_of(files: &BTreeMap<String, String>) -> R<(Snapshot<Value>, Kinds)> {
    let default = pack_prov(files)?;
    let mut snapshot = Snapshot::new();
    let mut kinds = Kinds::new();
    for (path, text) in rows_only(files) {
        let t: Table = toml::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        let at = Loc::new(Path::new(&path), 0);
        for (kind, v) in &t {
            for row in v
                .as_array()
                .ok_or_else(|| format!("{path}: {kind} isn't a list of rows"))?
            {
                let row = row
                    .as_table()
                    .ok_or_else(|| format!("{path}: a {kind} isn't a table"))?;
                records::snapshot_row(
                    &Ac1Rows,
                    kind,
                    row,
                    &at,
                    &default,
                    &mut snapshot,
                    &mut kinds,
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok((snapshot, kinds))
}

/// A converted world as a snapshot, its rows said to come from `source` at `revision`: the rows
/// [`crate::files::write`] would write of it, read as [`snapshot_of`] reads them, made one table
/// of the world at a time and without their text, each table of the world dropped once its rows
/// are in.
///
/// # Errors
/// A row that names no item, or two rows of one item.
pub fn snapshot_of_content(
    content: WorldContent,
    stamp: DotNetDateTime,
    source: &str,
    revision: &str,
) -> R<(Snapshot<Value>, Kinds)> {
    let default = Provenance {
        source: dereth_pack::provenance::Source::Snapshot,
        origin: Some(source.to_owned()),
        revision: Some(revision.to_owned()),
        ..Provenance::AUTHORED
    };
    let mut snapshot = Snapshot::new();
    let mut kinds = Kinds::new();
    crate::files::each_row(content, stamp, |path, kind, row| {
        let t = row.table();
        records::snapshot_row(
            &Ac1Rows,
            kind,
            &t,
            &Loc::new(Path::new(path), 0),
            &default,
            &mut snapshot,
            &mut kinds,
        )
        .map_err(|e| e.to_string())
    })?;
    Ok((snapshot, kinds))
}

/// A layer's packs, each its name and its files, read and merged in load order, two packs that
/// set one field differently settled by `overrides`; a row that says nothing of where it came
/// from, from where its pack's header says.
///
/// # Errors
/// Every problem of their files, and any conflict or item set twice in one pack.
pub fn delta_of(
    packs: &[(String, BTreeMap<String, String>)],
    overrides: &[Override],
) -> R<(Merge<Value>, Kinds)> {
    let mut rows = Vec::new();
    let mut removals = Vec::new();
    let mut kinds = Kinds::new();
    let mut problems = Vec::new();
    for (pack, files) in packs {
        let default = pack_prov(files).map_err(|e| format!("{pack}: {e}"))?;
        for (path, text) in rows_only(files) {
            let (file, errors) =
                records::read_pack_file_from(&Ac1Rows, pack, Path::new(&path), &text, &default);
            problems.extend(errors.iter().map(ToString::to_string));
            rows.extend(file.rows);
            removals.extend(file.removals);
            kinds.extend(file.kinds);
        }
    }
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    Ok((
        dereth_pack::fold::merge_layer(&rows, &removals, overrides),
        kinds,
    ))
}

/// The world `world` holds, each item's table converted by its kind; its ids not yet numbered.
///
/// # Errors
/// A table that doesn't read as its kind.
pub fn content_of(
    world: &Resolved<Value>,
    kinds: &Kinds,
    era: EraId,
    stamp: DotNetDateTime,
) -> R<WorldContent> {
    let mut c = WorldContent {
        era,
        ..WorldContent::default()
    };
    // One item at a time, its table dropped once it is read: never every table of the world at once.
    let mut one = |item: &str, kind: &str, table: &Table| -> R<()> {
        let err = |e: String| format!("{item}: {e}");
        match kind {
            "weenie" => c.weenies.push(kinds::weenie_of(table, stamp).map_err(err)?),
            "placement" => c
                .landblock_instances
                .push(kinds::placement_of(table, stamp).map_err(err)?),
            "encounter" => c
                .encounters
                .push(kinds::encounter_of(table, stamp).map_err(err)?),
            "cook_book" => c
                .cook_books
                .push(kinds::cook_book_of(table, stamp).map_err(err)?),
            "recipe" => c.recipes.push(kinds::recipe_of(table, stamp).map_err(err)?),
            "recipe_vital" => {
                let (k, mut v) = kinds::keyed_of::<
                    empyrean_content::models::world::RecipeVitalRequirement,
                >(table, "recipe", stamp)
                .map_err(err)?;
                v.id = k;
                c.recipe_vital_requirements.push(v);
            }
            "spell" => {
                let (k, mut v) =
                    kinds::keyed_of::<empyrean_content::models::world::Spell>(table, "id", stamp)
                        .map_err(err)?;
                v.id = k;
                c.spells.push(v);
            }
            "quest" => c
                .quests
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "event" => c
                .events
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "poi" => c
                .points_of_interest
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "house_portal" => c
                .house_portals
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "treasure_death" => {
                let (k, mut v) = kinds::keyed_of::<empyrean_content::models::world::TreasureDeath>(
                    table, "type", stamp,
                )
                .map_err(err)?;
                v.treasure_type = k;
                c.treasure_death.push(v);
            }
            "grouped_treasure" => c
                .grouped_treasure
                .push(kinds::grouped_treasure_of(table).map_err(err)?),
            "treasure_wielded" => c
                .treasure_wielded
                .extend(kinds::wielded_of(table, stamp).map_err(err)?),
            "treasure_gem_count" => c
                .treasure_gem_count
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "treasure_material_base" => c
                .treasure_material_base
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "treasure_material_color" => c
                .treasure_material_color
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            "treasure_material_groups" => c
                .treasure_material_groups
                .extend(kinds::material_groups_of(table, stamp).map_err(err)?),
            "loot_policy" => c
                .loot_policies
                .push(kinds::loot_policy_of(table).map_err(err)?),
            "rules" => c.rules = Some(kinds::rules_of(table).map_err(err)?),
            // The world builder's: checked, and not served.
            "name_table" => kinds::name_table_of(table).map_err(err)?,
            "version" => c
                .versions
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            other => return Err(format!("{item}: no kind of row is called {other}")),
        }
        Ok(())
    };
    let mut first: Option<String> = None;
    records::each_table(&Ac1Rows, world, kinds, |e, kind, table| {
        if first.is_none() {
            if let Err(why) = one(&e.item.name, kind, &table) {
                first = Some(why);
            }
        }
    });
    match first {
        Some(why) => Err(why),
        None => Ok(c),
    }
}

/// A world of layers: its base, a whole known world, then each patch in order.
#[derive(Debug, Default, Clone)]
pub struct Layers {
    /// The base's first pack, each `path → text`: its rows, its header and its meta file.
    pub base: BTreeMap<String, String>,
    /// The base's other packs, on its rows, and which of them wins.
    pub base_packs: Vec<(String, BTreeMap<String, String>)>,
    pub base_overrides: Vec<Override>,
    pub patches: Vec<PatchLayer>,
    /// The classes a weenie the base doesn't have may take, inclusive: the world's own. A weenie
    /// another world's row brings (`from = "source"`) keeps its source's class.
    pub authored_classes: Option<(u32, u32)>,
    /// What keying the next checkpoint read reads: the placements and keys so far.
    pub keying: crate::world::Keying,
    /// The iterations of the data files the base serves, by file (`portal`, `cell`, ...).
    pub base_iterations: BTreeMap<String, u32>,
}

/// One patch: its id, its packs (each its name and its files) in load order, and which wins; and
/// for a checkpoint, the whole known world it pins.
#[derive(Debug, Default, Clone)]
pub struct PatchLayer {
    pub id: String,
    /// The era the world is from this layer on, when it changes here; `None` keeps the era of
    /// the layer below.
    pub era: Option<EraId>,
    pub packs: Vec<(String, BTreeMap<String, String>)>,
    pub overrides: Vec<Override>,
    pub checkpoint: Option<Box<CheckpointLayer>>,
    /// A checkpoint left to read when a fold comes to it, and its acknowledged drift.
    pub pending: Option<Box<crate::world::PendingCheckpoint>>,
    pub drift: Vec<Ack>,
    /// The iterations of the data files it serves, by file (`portal`, `cell`, ...).
    pub iterations: BTreeMap<String, u32>,
    /// Whether its rows are written: until they are, what its data files find is no error of its
    /// own ([`LayerCheck::warnings`]). A checkpoint is written.
    pub written: bool,
}

/// A checkpoint's world as rows, and its acknowledged drift.
#[derive(Debug, Default, Clone)]
pub struct CheckpointLayer {
    pub snapshot: Snapshot<Value>,
    pub kinds: Kinds,
    pub drift: Vec<Ack>,
}

/// The classes of the weenies a snapshot has.
fn classes_of(snapshot: &Snapshot<Value>) -> impl Iterator<Item = u32> + '_ {
    snapshot
        .iter()
        .filter_map(|e| e.item.name.strip_prefix("weenie ")?.parse().ok())
}

/// The fields of a row of `kind` that name its item: every row of it sets them, to the same
/// values, so setting them again changes nothing worth saying.
#[must_use]
pub fn key_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "weenie" => &["class"],
        "placement" => &["guid"],
        "encounter" => &["landblock", "cell"],
        "cook_book" => &["recipe", "source", "target"],
        "recipe" | "spell" => &["id"],
        "recipe_vital" => &["recipe"],
        "quest" | "event" | "poi" => &["name"],
        "name_table" => &["heritage"],
        "house_portal" => &["house_id", "obj_cell_id"],
        "treasure_death" | "treasure_wielded" => &["type"],
        "grouped_treasure" => &["treasure_type"],
        "treasure_gem_count" => &["gem_code", "tier", "count"],
        "treasure_material_base" => &["material_code", "tier", "material_id"],
        "treasure_material_color" => &["material_id", "color_code", "palette_template"],
        "treasure_material_groups" => &["material_group", "tier"],
        _ => &[],
    }
}

/// `inputs`, the last checkpoint among them said to be the last, so its snapshot is let go once
/// folded in.
fn last_checkpoint(mut inputs: Vec<Input<Value>>) -> Vec<Input<Value>> {
    if let Some(i) = inputs
        .iter()
        .rposition(|i| matches!(i.kind, dereth_pack::fold::Kind::Checkpoint { .. }))
    {
        let last = inputs.remove(i).last_checkpoint();
        inputs.insert(i, last);
    }
    inputs
}

/// A world's layers, read and merged, to fold at any of them.
#[derive(Debug, Clone)]
pub struct Folding {
    /// Each layer's id, the base first.
    pub ids: Vec<String>,
    inputs: Vec<Input<Value>>,
    /// The kind of each item any layer sets. An item's name says its kind, so the latest word
    /// for it is every layer's: one map serves them all.
    kinds: Kinds,
    /// The classes each layer's snapshot has (none for a delta).
    snapshot_classes: Vec<BTreeSet<u32>>,
    authored_classes: Option<(u32, u32)>,
    /// The era each layer's world is in: the base's, until a layer changes it.
    eras: Vec<EraId>,
    stamp: DotNetDateTime,
    /// Each layer's checkpoint still to read, and what keying it reads.
    pending: Vec<Option<Box<crate::world::PendingCheckpoint>>>,
    keying: crate::world::Keying,
    /// The iterations of the data files each layer serves.
    iterations: Vec<BTreeMap<String, u32>>,
    /// Whether each layer's rows are written: the base's and a checkpoint's are.
    written: Vec<bool>,
    /// The data files a check looks a layer's records up in; none, and closure isn't checked.
    dats: Option<crate::closure::Dats>,
}

/// What a check finds in one layer.
#[derive(Debug, Clone)]
pub struct LayerCheck {
    pub id: String,
    /// What folding found in the layer: conflicts, stale removals, drift, no-ops, shadowed edits
    /// (but a row's own key set again, [`key_fields`]).
    pub findings: Vec<Finding>,
    /// What the layer breaks: a reference the world below it didn't have and it has to nothing
    /// ([`crate::refs`]), a weenie outside the world's own classes, a row that doesn't read as its
    /// kind, its loot ([`crate::loot`]), a record it names that its data files lack
    /// ([`crate::closure`]).
    pub problems: Vec<String>,
    /// The references to nothing a snapshot brings (the base's, a checkpoint's), its loot's
    /// half-made swap and the records its data files lack: its source's own, said once, where it
    /// brings them.
    pub inherited: Vec<String>,
    /// What the layer finds but is no fault of its rows: the records it names that the layer's
    /// data files lack, named below where no data files were checked for them, or named only by
    /// the destinations of portals nothing places; such a destination outside its room now; and,
    /// while the layer isn't written, what its data files find that would be its errors.
    pub warnings: Vec<String>,
    /// What is worth saying but no fault: the swap to ACE's loot, the data files it couldn't
    /// look its records up in, and the rooms the world names that changed under it.
    pub reports: Vec<String>,
}

impl LayerCheck {
    /// Its errors, one line each: the fold's and its problems.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity() == Severity::Error)
            .map(ToString::to_string)
            .chain(self.problems.iter().cloned())
            .collect()
    }
}

/// What a layer's world says of itself, as a check comes to it: [`LayerCheck`]'s problems,
/// inherited, warnings and reports.
#[derive(Debug, Default, Clone)]
struct Own {
    problems: Vec<String>,
    inherited: Vec<String>,
    warnings: Vec<String>,
    reports: Vec<String>,
}

impl Folding {
    /// `layers`, read and merged.
    ///
    /// # Errors
    /// What reading the base or a patch finds.
    pub fn new(layers: Layers, era: EraId, stamp: DotNetDateTime) -> R<Self> {
        let (snapshot, mut kinds) = snapshot_of(&layers.base)?;
        let base_classes = classes_of(&snapshot).collect();
        let (merge, k) = delta_of(&layers.base_packs, &layers.base_overrides)
            .map_err(|e| format!("{BASE}: {e}"))?;
        kinds.extend(k);
        let mut f = Self {
            ids: vec![BASE.to_owned()],
            inputs: vec![Input::checkpoint(BASE, snapshot, Vec::new(), merge)],
            kinds,
            snapshot_classes: vec![base_classes],
            authored_classes: layers.authored_classes,
            eras: vec![era],
            stamp,
            pending: vec![None],
            keying: layers.keying,
            iterations: vec![layers.base_iterations],
            written: vec![true],
            dats: None,
        };
        for p in layers.patches {
            let iterations = p.iterations;
            let era = p.era;
            let written = p.written || p.checkpoint.is_some() || p.pending.is_some();
            let (merge, k) =
                delta_of(&p.packs, &p.overrides).map_err(|e| format!("{}: {e}", p.id))?;
            if let Some(todo) = p.pending {
                // Its snapshot is read when a fold comes to it.
                f.push(
                    Input::checkpoint(&p.id, Snapshot::new(), p.drift, merge),
                    k,
                    BTreeSet::new(),
                );
                *f.pending.last_mut().expect("a layer was pushed") = Some(todo);
                *f.iterations.last_mut().expect("a layer was pushed") = iterations;
                *f.written.last_mut().expect("a layer was pushed") = written;
                if let Some(e) = era {
                    *f.eras.last_mut().expect("a layer was pushed") = e;
                }
                continue;
            }
            match p.checkpoint {
                Some(c) => {
                    let c = *c;
                    let classes = classes_of(&c.snapshot).collect();
                    let mut all = c.kinds;
                    all.extend(k);
                    f.push(
                        Input::checkpoint(&p.id, c.snapshot, c.drift, merge),
                        all,
                        classes,
                    );
                }
                None => f.push(Input::delta(&p.id, merge), k, BTreeSet::new()),
            }
            *f.iterations.last_mut().expect("a layer was pushed") = iterations;
            *f.written.last_mut().expect("a layer was pushed") = written;
            if let Some(e) = era {
                *f.eras.last_mut().expect("a layer was pushed") = e;
            }
        }
        Ok(f)
    }

    /// The layers, a check looking each one's records up in `dats` ([`crate::closure`]).
    #[must_use]
    pub fn with_dats(mut self, dats: crate::closure::Dats) -> Self {
        self.dats = Some(dats);
        self
    }

    /// The world with one more layer on top: `input`, the kinds of the items it sets in `kinds`,
    /// and the classes its snapshot has.
    pub fn push(&mut self, input: Input<Value>, kinds: Kinds, snapshot_classes: BTreeSet<u32>) {
        // A later layer's word for an item's kind wins.
        self.kinds.extend(kinds);
        self.ids.push(input.id.clone());
        self.inputs.push(input);
        self.snapshot_classes.push(snapshot_classes);
        self.pending.push(None);
        self.iterations.push(BTreeMap::new());
        self.written.push(true);
        // A layer keeps the era of the one below it until told otherwise.
        let below = self.eras.last().copied().unwrap_or_default();
        self.eras.push(below);
    }

    /// The era each layer's world is in, the base first.
    #[must_use]
    pub fn eras(&self) -> &[EraId] {
        &self.eras
    }

    /// How many layers there are, the base among them.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Whether layer `n` can change the world below it: the base, a checkpoint, or a delta with
    /// rows or removals.
    #[must_use]
    pub fn changes(&self, n: usize) -> bool {
        self.inputs.get(n).is_some_and(|i| {
            n == 0
                || !matches!(i.kind, dereth_pack::fold::Kind::Delta)
                || !i.change.edits.is_empty()
                || !i.change.removals.is_empty()
        })
    }

    /// The world at layer `n` folded from `world`, whatever its layers' findings; and a weenie no
    /// snapshot up to it has outside the world's own classes, unless it is another world's: a
    /// field of it says it came from a source, whose class it keeps.
    fn content_of_layer(
        &self,
        read: &Read,
        n: usize,
        world: &Resolved<Value>,
    ) -> (R<WorldContent>, Vec<String>) {
        let era = self.eras.get(n).copied().unwrap_or_default();
        let content = content_of(world, &read.kinds.borrow(), era, self.stamp);
        let mut problems = Vec::new();
        if let (Ok(c), Some((first, last))) = (&content, self.authored_classes) {
            let classes = read.classes.borrow();
            let known = |class: &u32| classes[..=n].iter().any(|s| s.contains(class));
            let sourced = |class: u32| {
                world.get(&format!("weenie {class}")).is_some_and(|e| {
                    e.fields
                        .iter()
                        .any(|(_, f)| f.prov.source == Source::Snapshot)
                })
            };
            for w in &c.weenies {
                if !known(&w.class_id)
                    && !(first..=last).contains(&w.class_id)
                    && !sourced(w.class_id)
                {
                    problems.push(format!(
                        "weenie {}: a class the base doesn't have is the world's own, {first} to {last}",
                        w.class_id
                    ));
                }
            }
        }
        (content, problems)
    }

    /// The world at layer `n`, its ids not yet numbered; the layers are folded up to it, and
    /// spent.
    ///
    /// # Errors
    /// Every error of the layers up to it.
    pub fn content_at(mut self, n: usize) -> R<WorldContent> {
        if n >= self.len() {
            return Err(format!("there is no layer {n}"));
        }
        let mut inputs = std::mem::take(&mut self.inputs);
        inputs.truncate(n + 1);
        let read = self.read();
        let folded = resolve(read.layers(last_checkpoint(inputs)), n);
        if let Some((_, why)) = read.failed.take() {
            return Err(why);
        }
        let mut errors: Vec<String> = folded
            .errors()
            .map(|(layer, f)| format!("{layer}: {f}"))
            .collect();
        let (content, problems) = self.content_of_layer(&read, n, &folded.world);
        errors.extend(problems);
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        content
    }

    /// Every layer checked, in one fold that spends the layers: what folding found in it, what
    /// the world at it names but doesn't have that the world below it had, its loot, and (given
    /// data files, [`Folding::with_dats`]) the records it names that its files lack and the rooms
    /// it names that changed under it ([`crate::closure`]). What a snapshot brings, at the base or
    /// a checkpoint, is its source's ([`LayerCheck::inherited`]); a layer that changes nothing has
    /// the world below it, and is looked up in its own data files when it names them. What a
    /// layer's data files find is its error only once its rows are written; until then, a
    /// warning.
    #[must_use]
    pub fn check(self) -> Vec<LayerCheck> {
        let last = self.len().saturating_sub(1);
        self.walk(last, None).0
    }

    /// Every layer checked as [`Folding::check`] checks it, in the same one fold, `each` handed the
    /// world at each layer that changes it (its ids not yet numbered) as the fold comes to it, and
    /// let go of before the next; a layer that changes nothing has the world below it. The layers
    /// are spent.
    pub fn check_each(self, each: &mut dyn FnMut(usize, WorldContent)) -> Vec<LayerCheck> {
        let last = self.len().saturating_sub(1);
        self.walk(last, Some(each)).0
    }

    /// Whether each layer's rows are written: the base's and a checkpoint's are, and a patch's
    /// once its `patch.toml` says so.
    #[must_use]
    pub fn written(&self) -> &[bool] {
        &self.written
    }

    /// The world at layer `n`, its ids not yet numbered, the layers up to it checked as
    /// [`Folding::check`] checks them, in the same one fold; the layers are spent.
    ///
    /// # Errors
    /// Every error of the layers up to it: the fold's, and the references to nothing a patch
    /// makes. What a snapshot brings is no error.
    pub fn checked_content_at(self, n: usize) -> R<WorldContent> {
        if n >= self.len() {
            return Err(format!("there is no layer {n}"));
        }
        let (checks, content) = self.walk(n, None);
        let errors: Vec<String> = checks
            .iter()
            .flat_map(|l| {
                l.errors()
                    .into_iter()
                    .map(move |e| format!("{}: {e}", l.id))
            })
            .collect();
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        content.unwrap_or_else(|| Err(format!("layer {n} has no world")))
    }

    /// The layers up to `upto` folded in one pass and checked; and the world at `upto`, unless
    /// `each` takes every world the fold makes ([`Folding::check_each`]).
    #[allow(clippy::too_many_lines)] // one fold, each layer's world checked as the fold comes to it
    fn walk(
        mut self,
        upto: usize,
        mut each: Option<&mut dyn FnMut(usize, WorldContent)>,
    ) -> (Vec<LayerCheck>, Option<R<WorldContent>>) {
        let mut inputs = std::mem::take(&mut self.inputs);
        inputs.truncate(upto + 1);
        let inputs = last_checkpoint(inputs);
        let snapshots: Vec<bool> = (0..inputs.len())
            .map(|n| n == 0 || !matches!(inputs[n].kind, dereth_pack::fold::Kind::Delta))
            .collect();
        let changes: Vec<bool> = (0..inputs.len())
            .map(|n| {
                n == 0
                    || !matches!(inputs[n].kind, dereth_pack::fold::Kind::Delta)
                    || !inputs[n].change.edits.is_empty()
                    || !inputs[n].change.removals.is_empty()
            })
            .collect();
        // The last layer that changes the world: its world is the one at `upto`.
        let last = (0..changes.len()).rev().find(|&n| changes[n]).unwrap_or(0);
        let dats = self.dats.take();
        let iterations = std::mem::take(&mut self.iterations);
        let written = std::mem::take(&mut self.written);
        let has_dats = dats.is_some();
        // A layer is looked at when it changes the world, or names data files of its own to look
        // the world below it up in.
        let looked_at =
            |n: usize| changes[n] || (has_dats && iterations.get(n).is_some_and(|i| !i.is_empty()));
        // The world at each layer looked at: what it breaks, what a snapshot brings, and reports.
        let mut own: Vec<Own> = vec![Own::default(); inputs.len()];
        let mut below: BTreeSet<String> = BTreeSet::new();
        let mut loot_below: Option<crate::loot::Loot> = None;
        // The records the world names, for its data files; and what the files below said.
        let mut named: Option<crate::closure::Named> = None;
        let mut files = dats.map(crate::closure::Checker::new);
        let mut kept: Option<R<WorldContent>> = None;
        let read = self.read();
        let reports =
            fold_each(read.layers(inputs), |n, world| {
                if !looked_at(n) {
                    return;
                }
                let mut o = Own::default();
                if changes[n] {
                    let (content, problems) = self.content_of_layer(&read, n, world);
                    o.problems = problems;
                    named = None;
                    match content {
                        Err(e) if n == last => {
                            o.problems.push(e.clone());
                            kept = Some(Err(e));
                        }
                        Ok(c) => {
                            let dangling = crate::refs::dangling(&c);
                            let new = dangling.iter().filter(|d| !below.contains(*d)).cloned();
                            if snapshots[n] {
                                o.inherited.extend(new);
                            } else {
                                o.problems.extend(new);
                            }
                            below = dangling.into_iter().collect();
                            let loot = crate::loot::Loot::of(&c);
                            let l = crate::loot::check(loot_below.as_ref(), &loot, snapshots[n]);
                            o.problems.extend(l.errors);
                            o.inherited.extend(l.inherited);
                            o.reports.extend(l.reports);
                            loot_below = Some(loot);
                            if has_dats {
                                named = Some(crate::closure::Named::of(&c));
                            }
                            match each.as_mut() {
                                Some(each) => each(n, c),
                                None if n == last => kept = Some(Ok(c)),
                                None => {}
                            }
                        }
                        Err(e) => o.problems.push(e),
                    }
                }
                if let (Some(files), Some(names)) = (&mut files, &named) {
                    let empty = BTreeMap::new();
                    let its = iterations.get(n).unwrap_or(&empty);
                    match files.layer(names, its, snapshots[n]) {
                        Ok(said) => {
                            if written.get(n).copied().unwrap_or(true) {
                                o.problems.extend(said.errors);
                            } else {
                                o.warnings.extend(said.errors.into_iter().map(|e| {
                                    format!("{e}; not an error until the layer is written")
                                }));
                            }
                            o.warnings.extend(said.warnings);
                            o.inherited.extend(said.inherited);
                            o.reports.extend(said.reports);
                        }
                        Err(e) => o.problems.push(e),
                    }
                }
                own[n] = o;
            })
            .layers;
        let failed = read.failed.take();
        let kinds = read.kinds.into_inner();
        // A row's key, set again by every row of its item, is no change and hides none.
        let keyed = |item: &str, field: Option<&String>| {
            field.is_some_and(|f| {
                kinds
                    .get(item)
                    .is_some_and(|k| key_fields(k).contains(&f.as_str()))
            })
        };
        let mut out: Vec<LayerCheck> = Vec::new();
        for (report, o) in reports.into_iter().zip(own) {
            let findings = report
                .findings
                .into_iter()
                .filter(|f| match f {
                    Finding::NoOp { item, field, .. } => !keyed(item, Some(field)),
                    Finding::Shadowed { item, field, .. } => !keyed(item, field.as_ref()),
                    _ => true,
                })
                .collect();
            out.push(LayerCheck {
                id: report.id,
                findings,
                problems: o.problems,
                inherited: o.inherited,
                warnings: o.warnings,
                reports: o.reports,
            });
        }
        // A checkpoint that couldn't be read ends the fold there: its error is its layer's.
        if let Some((n, why)) = failed {
            out.push(LayerCheck {
                id: self.ids[n].clone(),
                findings: Vec::new(),
                problems: vec![why.clone()],
                inherited: Vec::new(),
                warnings: Vec::new(),
                reports: Vec::new(),
            });
            if n <= upto {
                kept = Some(Err(why));
            }
        }
        (out, kept)
    }

    /// The layers' checkpoints still to read, taken out to be read as a fold comes to them, with
    /// the kinds and classes the layers have so far.
    fn read(&mut self) -> Read {
        Read {
            pending: std::cell::RefCell::new(std::mem::take(&mut self.pending)),
            keying: std::cell::RefCell::new(std::mem::take(&mut self.keying)),
            kinds: std::cell::RefCell::new(std::mem::take(&mut self.kinds)),
            classes: std::cell::RefCell::new(std::mem::take(&mut self.snapshot_classes)),
            failed: std::cell::RefCell::new(None),
        }
    }
}

/// A fold's reading of the checkpoints still to read: each read when the fold comes to it, and
/// let go as the fold takes it in; the kinds and classes of the layers read so far; and the first
/// checkpoint that couldn't be read, with why.
struct Read {
    pending: std::cell::RefCell<Vec<Option<Box<crate::world::PendingCheckpoint>>>>,
    keying: std::cell::RefCell<crate::world::Keying>,
    kinds: std::cell::RefCell<Kinds>,
    classes: std::cell::RefCell<Vec<BTreeSet<u32>>>,
    failed: std::cell::RefCell<Option<(usize, String)>>,
}

impl Read {
    /// `inputs` (layer 0 first), each checkpoint still to read put in its input as the fold asks
    /// for it; the first that can't be read ends them.
    fn layers(&self, inputs: Vec<Input<Value>>) -> impl Iterator<Item = Input<Value>> + '_ {
        inputs
            .into_iter()
            .enumerate()
            .map_while(move |(n, mut input)| {
                let todo = self.pending.borrow_mut().get_mut(n).and_then(Option::take);
                if let Some(todo) = todo {
                    match crate::world::read_checkpoint(&todo, &mut self.keying.borrow_mut()) {
                        Ok((snapshot, kinds, _)) => {
                            self.classes.borrow_mut()[n] = classes_of(&snapshot).collect();
                            self.kinds.borrow_mut().extend(kinds);
                            if let dereth_pack::fold::Kind::Checkpoint { snapshot: s, .. } =
                                &mut input.kind
                            {
                                *s = snapshot;
                            }
                        }
                        Err(why) => {
                            *self.failed.borrow_mut() = Some((n, format!("{}: {why}", todo.id)));
                            return None;
                        }
                    }
                }
                Some(input)
            })
    }
}

/// `layers` folded: the world at the last and what each layer's fold found; every item's kind;
/// and the classes some snapshot has, which a world's own weenies needn't take from its range.
///
/// # Errors
/// What reading the base or a patch finds.
pub fn fold_inputs(layers: Layers) -> R<(Folded<Value>, Kinds, BTreeSet<u32>)> {
    let mut f = Folding::new(layers, EraId::default(), DotNetDateTime::default())?;
    let inputs = last_checkpoint(std::mem::take(&mut f.inputs));
    let read = f.read();
    let folded = fold(read.layers(inputs));
    if let Some((_, why)) = read.failed.take() {
        return Err(why);
    }
    let known = read.classes.into_inner().into_iter().flatten().collect();
    Ok((folded, read.kinds.into_inner(), known))
}

/// The world at the last of `layers`, folded field by field; its ids not yet numbered.
///
/// # Errors
/// What reading the base or a patch finds, every error the fold finds, and a weenie no snapshot
/// has outside the world's own classes and no source's row brings.
pub fn fold_layers(layers: Layers, era: EraId, stamp: DotNetDateTime) -> R<WorldContent> {
    let f = Folding::new(layers, era, stamp)?;
    let last = f.len() - 1;
    f.content_at(last)
}
