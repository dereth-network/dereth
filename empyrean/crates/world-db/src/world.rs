//! A world of layers on disk: its `world.toml`, its base pack of rows and its patches, as the
//! shared pack format's lineage reads them (`dereth_pack::lineage`), folded into one world.
//!
//! ```text
//! world.toml                                packs = ["base"], lineage = [...], [ids] classes
//! base/                                     the base: a whole known world, its rows
//! base/pack.toml, base/world-db.toml        its header and source; its rows' era and stamp
//! patches/<yyyy-mm>-<name>/patch.toml       a patch, a delta on the layer before
//! patches/<yyyy-mm>-<name>/packs/<pack>/    its packs of rows
//! ```
//!
//! The base's first pack is the snapshot every later layer folds over; its other packs, and every
//! patch's, are rows that set, unset and remove field by field ([`crate::layers`]). A weenie the
//! base doesn't have takes a class of the world's own, `[ids] classes = [first, last]`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use dereth_pack::lineage::{LayerDef, Lineage};
use dereth_pack::store::DiskStore;
use dereth_pack::symbol::{Numbering, SymbolKind};
use dereth_pack::Dialect;
use empyrean_common::era::EraId;
use empyrean_content::WorldContent;

use crate::files::{meta, read_dir};
use crate::layers::{fold_layers, item_form, Layers, PatchLayer};
use crate::value::R;

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
         cook-book <source> <target>, recipe <id>, recipe-vital <id>, spell <id>, quest <name>, \
         event <name>, poi <name>, house-portal <house> <cell>, death-treasure <type>, \
         grouped-treasure <type>, wielded-treasure <type>, gem-count <gem> <tier>, \
         material-base <code> <tier> <material>, material-color <material> <color> <palette>, \
         material-group <group> <tier> <material>, loot-policy, version, or landblock <XXYY>";
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

/// The world `world.toml` names at its last layer, folded; its ids not yet numbered.
///
/// # Errors
/// A world file or lineage that doesn't read, a base without a pack of rows, a checkpoint layer
/// (its snapshot is not converted here yet), and whatever folding the layers finds.
pub fn fold_world(world_toml: &Path) -> R<WorldContent> {
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
    let [base, patches @ ..] = lineage.layers.as_slice() else {
        return Err("world.toml: no base layer".into());
    };
    let first = base
        .packs
        .first()
        .ok_or("world.toml: the base has no pack of rows (packs = [\"base\"])")?;
    let base_files = read_dir(first)?;
    let m = meta(&base_files).map_err(|e| format!("{}: {e}", first.display()))?;
    let mut layers = Layers {
        base: base_files,
        base_packs: base.packs[1..]
            .iter()
            .map(|p| pack_of(p))
            .collect::<R<_>>()?,
        base_overrides: base.overrides.clone(),
        patches: Vec::new(),
        authored_classes,
    };
    for p in patches {
        if p.is_checkpoint() {
            return Err(format!(
                "{}: a checkpoint's snapshot isn't converted here yet",
                p.id
            ));
        }
        layers.patches.push(PatchLayer {
            id: p.id.clone(),
            packs: packs_of(p)?,
            overrides: p.overrides.clone(),
        });
    }
    fold_layers(&layers, m.era, m.stamp)
}
