//! A layer of a world repository written from a whole world: the rows a patch's pack needs so the
//! world at that layer is a given folder of rows, such as a newer snapshot of the world the base
//! was written from.
//!
//! The world below the pack ([`Repo::fold_below`]) and the world wanted are compared item by item.
//! An item the world below lacks is written whole; one both hold is written as the layer's change
//! of it ([`layer_rows::delta`]): its key, the fields that differ, and in `remove_fields` those
//! below that the wanted item lacks; one they hold alike is left out; and one the wanted world
//! lacks is taken away by a `[[remove]]` row in the pack's `remove.toml`. Each row goes in the file
//! its kind's rule names, so the layer reads as any other written by hand. Folded, the world at the
//! layer is the world wanted, item for item and field for field.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_primitives::EraId;

use crate::layer_rows;
use crate::mirror::REMOVALS;
use crate::repo::{Repo, RepoLayer, RowPack, RowSchema, ShownWorld};

/// What [`write()`] writes: each file of the pack with its text, and how many items it makes, changes
/// and takes away.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Written {
    pub files: BTreeMap<PathBuf, String>,
    pub made: usize,
    pub changed: usize,
    pub removed: usize,
}

/// The world a folder of rows `dir` holds, as a base holds its own: every item once.
///
/// # Errors
/// A folder that doesn't read, or a row that doesn't: one of no kind the schema knows, or an item
/// set twice.
pub fn world_of_rows(schema: &RowSchema, dir: &Path) -> Result<ShownWorld, String> {
    let repo = Repo {
        path: dir.join("world.toml"),
        title: None,
        era: EraId::Eor,
        stamp: None,
        layers: vec![RepoLayer {
            id: "base".into(),
            checkpoint: false,
            packs: vec![RowPack::read(dir)?],
            overrides: Vec::new(),
        }],
    };
    repo.fold(schema, 0)
}

/// The files pack `pack` of layer `at` of `repo` needs for the world at that layer to be `wanted`.
/// The pack must hold no rows yet.
///
/// # Errors
/// A layer past those that can be folded, a pack it doesn't have or one that holds rows, or a row
/// that doesn't read or write.
pub fn write(
    schema: &RowSchema,
    repo: &Repo,
    at: usize,
    pack: usize,
    wanted: &ShownWorld,
) -> Result<Written, String> {
    let layer = repo
        .layers
        .get(at)
        .ok_or_else(|| format!("the world has no layer {at}"))?;
    if at == 0 {
        return Err("the base is a whole world: a layer above it is written from another".into());
    }
    let p = layer
        .packs
        .get(pack)
        .ok_or_else(|| format!("{}: no pack {pack}", layer.id))?;
    if !p.files.is_empty() {
        return Err(format!(
            "{}: the pack holds rows already; write the layer into an empty one",
            p.dir.display()
        ));
    }
    let below = repo.fold_below(schema, at, pack)?;
    let mut out = Written::default();
    // Each file's rows, its kind's and the row, in the order of the items' names.
    let mut by_file: BTreeMap<PathBuf, Vec<(String, toml::Table)>> = BTreeMap::new();
    for (name, item) in &wanted.items {
        let row = item.row(schema);
        let was = below.items.get(name);
        if let Some(b) = was.filter(|b| b.kind != item.kind) {
            return Err(format!(
                "{name}: a {} below, a {} wanted",
                b.kind, item.kind
            ));
        }
        let was = was.map(|b| b.row(schema));
        let Some(delta) = layer_rows::delta(schema, &item.kind, was.as_ref(), &row) else {
            continue;
        };
        let file = p.dir.join(layer_rows::file_of(schema, &item.kind, &row)?);
        by_file
            .entry(file)
            .or_default()
            .push((item.kind.clone(), delta));
        if was.is_some() {
            out.changed += 1;
        } else {
            out.made += 1;
        }
    }
    for (file, rows) in by_file {
        let rows: Vec<(&str, &toml::Table)> = rows.iter().map(|(k, r)| (k.as_str(), r)).collect();
        out.files.insert(file, layer_rows::new_file(schema, &rows));
    }
    let gone: Vec<&str> = below
        .items
        .keys()
        .filter(|n| !wanted.items.contains_key(*n))
        .map(String::as_str)
        .collect();
    out.removed = gone.len();
    if !gone.is_empty() {
        out.files
            .insert(p.dir.join(REMOVALS), layer_rows::removals_file(&gone));
    }
    Ok(out)
}
