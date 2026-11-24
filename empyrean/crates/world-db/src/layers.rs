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
//!
//! A weenie's property tables (`int`, `did`, `skill`, ..., `spell_book`, `book`) merge by key, so a
//! patch sets one property (`int.ItemType`); every list (a create list, the emotes, a recipe's
//! mods, a wielded type's entries) is one field, replaced whole.
//!
//! A row that says nothing of where it came from came from where its pack's header (`pack.toml`)
//! says: a base converted from another project's world says `from = "source"` there once.

use std::collections::BTreeMap;
use std::path::Path;

use dereth_pack::conflict::{Item, Override};
use dereth_pack::fold::{fold, Folded, Input, Merge, Resolved, Snapshot};
use dereth_pack::lineage::{Ack, BASE};
use dereth_pack::pack::PackHeader;
use dereth_pack::provenance::Provenance;
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
fn word_num(w: &str) -> Option<u32> {
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
        "quest" | "event" | "poi" if !rest.is_empty() => {
            Some(format!("{head} {}", rest.to_lowercase()))
        }
        "loot-policy" | "version" if rest.is_empty() => Some(head.to_owned()),
        _ => None,
    }
}

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
            "version" => Item::new("version"),
            other => return Err(format!("no kind of row is called {other}")),
        })
    }

    fn is_map(&self, kind: &str, field: &str) -> bool {
        kind == "weenie" && WEENIE_MAPS.contains(&field)
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
    for (item, t) in records::tables(&Ac1Rows, world, kinds) {
        let table = &t.table;
        let err = |e: String| format!("{item}: {e}");
        match t.kind.as_str() {
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
            "version" => c
                .versions
                .push(kinds::Fields::take(table, stamp).map_err(err)?),
            other => return Err(format!("{item}: no kind of row is called {other}")),
        }
    }
    Ok(c)
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
    /// The classes a weenie the base doesn't have may take, inclusive: the world's own.
    pub authored_classes: Option<(u32, u32)>,
}

/// One patch: its id, its packs (each its name and its files) in load order, and which wins; and
/// for a checkpoint, the whole known world it pins.
#[derive(Debug, Default, Clone)]
pub struct PatchLayer {
    pub id: String,
    pub packs: Vec<(String, BTreeMap<String, String>)>,
    pub overrides: Vec<Override>,
    pub checkpoint: Option<Box<CheckpointLayer>>,
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

/// `layers` folded: the world at the last and what each layer's fold found; every item's kind;
/// and the classes some snapshot has, which a world's own weenies needn't take from its range.
///
/// # Errors
/// What reading the base or a patch finds.
pub fn fold_inputs(layers: Layers) -> R<(Folded<Value>, Kinds, std::collections::BTreeSet<u32>)> {
    let (snapshot, mut kinds) = snapshot_of(&layers.base)?;
    let mut known: std::collections::BTreeSet<u32> = classes_of(&snapshot).collect();
    let (merge, k) =
        delta_of(&layers.base_packs, &layers.base_overrides).map_err(|e| format!("{BASE}: {e}"))?;
    kinds.extend(k);
    let mut inputs = vec![Input::checkpoint(BASE, snapshot, Vec::new(), merge)];
    for p in layers.patches {
        let (merge, k) = delta_of(&p.packs, &p.overrides).map_err(|e| format!("{}: {e}", p.id))?;
        match p.checkpoint {
            Some(c) => {
                let c = *c;
                known.extend(classes_of(&c.snapshot));
                kinds.extend(c.kinds);
                kinds.extend(k);
                inputs.push(Input::checkpoint(&p.id, c.snapshot, c.drift, merge));
            }
            None => {
                // A later layer's word for an item's kind wins.
                kinds.extend(k);
                inputs.push(Input::delta(&p.id, merge));
            }
        }
    }
    Ok((fold(inputs), kinds, known))
}

/// The world at the last of `layers`, folded field by field; its ids not yet numbered.
///
/// # Errors
/// What reading the base or a patch finds, every error the fold finds, and a weenie no snapshot
/// has outside the world's own classes.
pub fn fold_layers(layers: Layers, era: EraId, stamp: DotNetDateTime) -> R<WorldContent> {
    let authored_classes = layers.authored_classes;
    let (folded, kinds, known) = fold_inputs(layers)?;
    let mut errors: Vec<String> = folded
        .errors()
        .map(|(layer, f)| format!("{layer}: {f}"))
        .collect();
    let content = content_of(&folded.world, &kinds, era, stamp);
    if let (Ok(c), Some((first, last))) = (&content, authored_classes) {
        for w in &c.weenies {
            if !known.contains(&w.class_id) && !(first..=last).contains(&w.class_id) {
                errors.push(format!(
                    "weenie {}: a class the base doesn't have is the world's own, {first} to {last}",
                    w.class_id
                ));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    content
}
