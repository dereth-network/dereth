//! A world of layers on disk: its `world.toml`, its base pack of rows and its patches, as the
//! shared pack format's lineage reads them (`dereth_pack::lineage`), folded into one world.
//!
//! ```text
//! world.toml                                packs = ["base"], lineage = [...], [ids] classes,
//!                                           [land] <file>_iteration (the base's data files)
//! base/                                     the base: a whole known world, its rows
//! base/pack.toml, base/world-db.toml        its header and source; its rows' era and stamp
//! patches/<yyyy-mm>-<name>/patch.toml       a patch, a delta on the layer before; <file>_iteration
//! patches/<yyyy-mm>-<name>/packs/<pack>/    its packs of rows
//! patches/<yyyy-mm>-<name>/checkpoint.toml  a checkpoint's pin ([`crate::checkpoint`])
//! patches/<yyyy-mm>-<name>/keys.toml        its placements' keys, generated
//! patches/<yyyy-mm>-<name>/drift.toml       its acknowledged drift
//! ```
//!
//! The base's first pack is the snapshot every later layer folds over; its other packs, and every
//! patch's, are rows that set, unset and remove field by field ([`crate::layers`]). A weenie the
//! base doesn't have takes a class of the world's own, `[ids] classes = [first, last]`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use dereth_pack::lineage::{LayerDef, Lineage};
use dereth_pack::store::DiskStore;
use dereth_pack::symbol::{Numbering, SymbolKind};
use dereth_pack::Dialect;
use empyrean_common::era::EraId;
use empyrean_content::WorldContent;

use crate::checkpoint::{self, apply_keys, keys_of, match_keys, Keys, Known, Pin};
use crate::files::{self, meta, read_dir, Meta};
use crate::layers::{
    fold_layers, item_form, snapshot_of_content, CheckpointLayer, Folding, Layers, PatchLayer,
};
use crate::value::R;
use dereth_pack::fold::Snapshot;
use dereth_pack::records::Kinds;

/// The first game's words for the pack format, as Empyrean's worlds of rows speak them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ac1World;

/// What a world's symbols name: its own weenie classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Class,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

impl SymbolKind for Kind {
    const KINDS: &'static [Self] = &[Self::Class];

    fn kind_name(self) -> &'static str {
        "class"
    }

    fn numbering(self) -> Numbering {
        Numbering::Counted {
            start: 1_000_000,
            world_sets: true,
        }
    }

    fn written_in_hex(self) -> bool {
        false
    }
}

impl Dialect for Ac1World {
    const GAME: &'static str = dereth_pack::DEFAULT_GAME;
    const PRODUCT: &'static str = "Empyrean";
    const BUILD_COMMAND: &'static str = "empyrean-world-db pack";
    const ITEM_FORMS: &'static str = "weenie <class>, spawn <guid>, encounter <XXYY> <x>,<y>, \
         cook-book <recipe> <source> <target>, recipe <id>, recipe-vital <id>, spell <id>, quest <name>, \
         event <name>, poi <name>, house-portal <house> <cell>, death-treasure <type>, \
         grouped-treasure <type>, wielded-treasure <type>, gem-count <gem> <tier> <count>, \
         material-base <code> <tier> <material>, material-color <material> <color> <palette>, \
         material-group <group> <tier>, loot-policy, version, or landblock <XXYY>";
    const NAMES_LINEAGE: bool = true;
    type Kind = Kind;
    type Era = EraId;

    fn era(name: Option<&str>) -> Result<EraId, String> {
        name.map_or(Ok(EraId::Eor), |n| {
            EraId::parse(n).ok_or_else(|| format!("{n:?} is not an era"))
        })
    }

    fn item(text: &str, _words: &[&str]) -> Option<String> {
        item_form(text)
    }
}

/// A pack's folder as its name and its rows.
fn pack_of(dir: &Path) -> R<(String, BTreeMap<String, String>)> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{}: not a pack folder", dir.display()))?;
    Ok((name, read_dir(dir)?))
}

fn packs_of(layer: &LayerDef) -> R<Vec<(String, BTreeMap<String, String>)>> {
    layer.packs.iter().map(|p| pack_of(p)).collect()
}

/// Where a checkpoint's placement keys come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeysFrom {
    /// Its `keys.toml`, or matched afresh where it has none.
    Files,
    /// Matched afresh, whatever its `keys.toml` says.
    Fresh,
}

/// One checkpoint as read: its id, its pin, and its placements' keys.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub id: String,
    pub pin: Pin,
    pub keys: Keys,
}

/// The world `world.toml` names, read as layers (a checkpoint's pinned dump, from `dumps` or where
/// [`Pin::dump`] looks, converted to rows, its placements keyed as `keys` says), with what its base
/// says of itself and each checkpoint read.
///
/// # Errors
/// A world file or lineage that doesn't read, a base without a pack of rows, a checkpoint whose
/// pin or dump doesn't read, and a `keys.toml` that doesn't.
pub fn layers_of(
    world_toml: &Path,
    keys: KeysFrom,
    dumps: Option<&Path>,
) -> R<(Layers, Meta, Vec<Checkpoint>)> {
    read_layers(world_toml, keys, dumps, false)
}

/// A checkpoint whose world is read only when a fold comes to it ([`Folding`]): its pin, the dump
/// to read, the keys its `keys.toml` gives (`None`: matched when read), and the guids the patches
/// up to it place, which a new placement's key may not take.
#[derive(Debug, Clone)]
pub struct PendingCheckpoint {
    pub id: String,
    pub pin: Pin,
    pub dump: std::path::PathBuf,
    pub keys: Option<Keys>,
    pub spawns: Vec<u32>,
    pub era: EraId,
    pub stamp: empyrean_common::dotnet::DotNetDateTime,
}

/// The placements of the world before a checkpoint, by their source's guids and their keys, and
/// every key the lineage has given: what keying a checkpoint's placements reads.
#[derive(Debug, Clone, Default)]
pub struct Keying {
    pub known: Vec<Known>,
    pub used: BTreeSet<u32>,
}

/// A checkpoint's world read as a snapshot: its dump converted and its placements keyed (by
/// `keys` when given, else matched against `keying`, which then holds the checkpoint's own).
///
/// # Errors
/// A dump that doesn't read or isn't the pin's, and a row that names no item.
pub fn read_checkpoint(
    p: &PendingCheckpoint,
    keying: &mut Keying,
) -> R<(Snapshot<toml::Value>, Kinds, Keys)> {
    let mut c = checkpoint::ingest(&p.pin, &p.dump, p.era)?;
    keying.used.extend(p.spawns.iter().copied());
    let keys = match &p.keys {
        Some(k) => k.clone(),
        None => match_keys(&keying.known, &keying.used, &c.landblock_instances),
    };
    let key = |g: u32| keys.map.get(&g).copied().unwrap_or(g);
    keying.known = c
        .landblock_instances
        .iter()
        .map(|i| Known::keyed(i, key(i.guid)))
        .collect();
    keying.used.extend(keying.known.iter().map(|k| k.key));
    apply_keys(&mut c, &keys.map);
    let (snapshot, kinds) = snapshot_of_content(c, p.stamp, &p.pin.source, &p.pin.tag)?;
    Ok((snapshot, kinds, keys))
}

/// [`layers_of`], each checkpoint read now, or (`lazy`) left for the fold to read.
fn read_layers(
    world_toml: &Path,
    keys: KeysFrom,
    dumps: Option<&Path>,
    lazy: bool,
) -> R<(Layers, Meta, Vec<Checkpoint>)> {
    let lineage =
        Lineage::read_world_file::<Ac1World>(&DiskStore, world_toml).map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(world_toml).map_err(|e| e.to_string())?;
    let file: toml::Table = toml::from_str(&text).map_err(|e| e.to_string())?;
    let authored_classes = match file.get("ids").and_then(|i| i.get("classes")) {
        None => None,
        Some(v) => {
            let span = v
                .as_array()
                .filter(|a| a.len() == 2)
                .and_then(|a| Some((a[0].as_integer()?, a[1].as_integer()?)))
                .and_then(|(a, b)| Some((u32::try_from(a).ok()?, u32::try_from(b).ok()?)))
                .ok_or("world.toml: [ids] classes is [first, last]")?;
            Some(span)
        }
    };
    let base_iterations = iterations_of(file.get("land"))?;
    let [base, patches @ ..] = lineage.layers.as_slice() else {
        return Err("world.toml: no base layer".into());
    };
    let first = base
        .packs
        .first()
        .ok_or("world.toml: the base has no pack of rows (packs = [\"base\"])")?;
    let base_files = read_dir(first)?;
    let m = meta(&base_files).map_err(|e| format!("{}: {e}", first.display()))?;
    // The placements of the world before each checkpoint, by their source's guids and their keys,
    // and every key the lineage has given.
    let known = Known::of(&files::read(&base_files)?);
    let used: BTreeSet<u32> = known.iter().map(|k| k.key).collect();
    let mut keying = Keying { known, used };
    let mut layers = Layers {
        base: base_files,
        base_packs: base.packs[1..]
            .iter()
            .map(|p| pack_of(p))
            .collect::<R<_>>()?,
        base_overrides: base.overrides.clone(),
        patches: Vec::new(),
        authored_classes,
        keying: Keying::default(),
        base_iterations,
    };
    let mut read = Vec::new();
    let mut spawns: Vec<u32> = Vec::new();
    // The era each layer folds in: the base's, until a checkpoint names another.
    let mut era = m.era;
    for p in patches {
        let below = era;
        let packs = packs_of(p)?;
        spawns.extend(spawns_in(&packs));
        let mut checkpoint = None;
        let mut pending = None;
        if let Some(pin) = &p.checkpoint {
            let pin = checkpoint::pin_of(pin).map_err(|e| format!("{}: {e}", p.id))?;
            era = pin.era.unwrap_or(era);
            let dump = pin.dump(dumps).map_err(|e| format!("{}: {e}", p.id))?;
            let keys_path = p.dir.join("keys.toml");
            let file_keys = match (keys, keys_path.is_file()) {
                (KeysFrom::Files, true) => {
                    let text = std::fs::read_to_string(&keys_path)
                        .map_err(|e| format!("{}: {e}", keys_path.display()))?;
                    Some(keys_of(&text).map_err(|e| format!("{}: {e}", p.id))?)
                }
                _ => None,
            };
            let todo = PendingCheckpoint {
                id: p.id.clone(),
                pin,
                dump,
                keys: file_keys,
                spawns: std::mem::take(&mut spawns),
                era,
                stamp: m.stamp,
            };
            if lazy {
                pending = Some(Box::new(todo));
            } else {
                let (snapshot, kinds, keys) =
                    read_checkpoint(&todo, &mut keying).map_err(|e| format!("{}: {e}", p.id))?;
                read.push(Checkpoint {
                    id: p.id.clone(),
                    pin: todo.pin,
                    keys,
                });
                checkpoint = Some(Box::new(CheckpointLayer {
                    snapshot,
                    kinds,
                    drift: p.drift.clone(),
                }));
            }
        }
        layers.patches.push(PatchLayer {
            id: p.id.clone(),
            era: (era != below).then_some(era),
            packs,
            overrides: p.overrides.clone(),
            checkpoint,
            pending,
            drift: p.drift.clone(),
            iterations: p
                .patch
                .as_ref()
                .map(|x| x.iterations.clone())
                .unwrap_or_default(),
            written: p.patch.as_ref().is_some_and(|x| x.written),
        });
    }
    layers.keying = keying;
    Ok((layers, m, read))
}

/// The data files' iterations a world file's `[land]` names for its base: each
/// `<file>_iteration`, as a patch names its own.
fn iterations_of(land: Option<&toml::Value>) -> R<BTreeMap<String, u32>> {
    let mut out = BTreeMap::new();
    for (key, v) in land.and_then(toml::Value::as_table).into_iter().flatten() {
        let Some(file) = key.strip_suffix("_iteration").filter(|f| !f.is_empty()) else {
            continue;
        };
        let n = v
            .as_integer()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                format!("world.toml: [land] {key}: an iteration, a whole number from 0")
            })?;
        out.insert(file.to_owned(), n);
    }
    Ok(out)
}

/// The guids of the placements `packs` set.
fn spawns_in(packs: &[(String, BTreeMap<String, String>)]) -> Vec<u32> {
    let mut out = Vec::new();
    for (_, files) in packs {
        for text in files.values() {
            let Ok(t) = toml::from_str::<toml::Table>(text) else {
                continue;
            };
            for row in t
                .get("placement")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(g) = row.get("guid").and_then(|g| crate::value::id_of(g).ok()) {
                    out.push(g);
                }
            }
        }
    }
    out
}

/// The world `world.toml` names at its last layer, folded; its ids not yet numbered.
///
/// # Errors
/// What [`layers_of`] finds, and whatever folding the layers finds.
pub fn fold_world(world_toml: &Path, dumps: Option<&Path>) -> R<WorldContent> {
    let (layers, m, _) = layers_of(world_toml, KeysFrom::Files, dumps)?;
    fold_layers(layers, m.era, m.stamp)
}

/// Layer `layer`'s checkpoint, when it is one: its pin, the dump the pin names (from `dumps` as
/// [`layers_of`] says), and the keys its `keys.toml` gives, if it has one.
///
/// # Errors
/// A world file that doesn't read, a layer it doesn't have, a pin that doesn't read, a dump that
/// isn't there, and a `keys.toml` that doesn't read.
pub fn checkpoint_of(
    world_toml: &Path,
    layer: &str,
    dumps: Option<&Path>,
) -> R<Option<(Pin, std::path::PathBuf, Option<Keys>)>> {
    let lineage =
        Lineage::read_world_file::<Ac1World>(&DiskStore, world_toml).map_err(|e| e.to_string())?;
    let l = lineage
        .layers
        .iter()
        .find(|l| l.id == layer)
        .ok_or_else(|| format!("{}: no layer {layer}", world_toml.display()))?;
    let Some(pin) = &l.checkpoint else {
        return Ok(None);
    };
    let pin = checkpoint::pin_of(pin).map_err(|e| format!("{layer}: {e}"))?;
    let dump = pin.dump(dumps).map_err(|e| format!("{layer}: {e}"))?;
    let keys_path = l.dir.join("keys.toml");
    let keys = if keys_path.is_file() {
        let text = std::fs::read_to_string(&keys_path)
            .map_err(|e| format!("{}: {e}", keys_path.display()))?;
        Some(keys_of(&text).map_err(|e| format!("{}: {e}", keys_path.display()))?)
    } else {
        None
    };
    Ok(Some((pin, dump, keys)))
}

/// The layers `world.toml` names, read and merged, to fold at any of them; a checkpoint's dump
/// from `dumps` as [`layers_of`] says.
///
/// # Errors
/// What [`layers_of`] finds.
pub fn open(world_toml: &Path, dumps: Option<&Path>) -> R<Folding> {
    let (layers, m, _) = read_layers(world_toml, KeysFrom::Files, dumps, true)?;
    Folding::new(layers, m.era, m.stamp)
}
