//! `world cutover`: whether a shard database can move onto a world's layer, checked before a server
//! is pointed at it.
//!
//! A world numbers its placements as its source does, except where its newest checkpoint up to the
//! layer re-keys them (that checkpoint's `keys.toml`; the layers above it keep its keys). A shard
//! names a placement by its guid in two kinds of row ([`empyrean_store::saved_placements`]): a
//! saved static (an owned house, its slumlord, a hook or storage holding items) and a house
//! permission. Either one, under a guid the keys move or give to another placement, would follow
//! the guid onto another placement or onto none. The check refuses then, and lists each such row.
//!
//! A world whose layers up to the target are no checkpoint, or whose checkpoint re-keys nothing,
//! keeps its source's guids, so any shard moves onto it; and a shard a server has already started
//! on a world keyed by a checkpoint is numbered by the world's own guids, so it moves onto any of
//! its layers. The pack of such a layer carries the keys, and the server makes the same check when
//! it starts.

use std::path::{Path, PathBuf};

use empyrean_common::world_keys::WorldKeys;
use empyrean_store::saved_placements::{self, SavedPlacements};

/// The keys the newest checkpoint at or below layer `layer` of the world `world` gives its dump's
/// placements, and that checkpoint: `None` when no layer up to `layer` is a checkpoint, no keys
/// when it moves no guid.
///
/// # Errors
/// A world that doesn't read, a layer it doesn't have, and a `keys.toml` that doesn't read.
pub fn keys_at(world: &Path, layer: &str) -> Result<Option<(String, Option<WorldKeys>)>, String> {
    Ok(
        empyrean_world_db::world::checkpoint_keys_at(world, layer)?.map(|(checkpoint, keys)| {
            let keys = keys.and_then(|k| WorldKeys::of(&checkpoint, k.map));
            (checkpoint, keys)
        }),
    )
}

/// The shard database at `shard` against the world `world` at layer `layer`: what
/// [`saved_placements::refusals`]
/// finds, and the checkpoint whose keys it checked (`None` when no layer up to `layer` is a
/// checkpoint).
///
/// # Errors
/// A world that doesn't read, a layer it doesn't have, a `keys.toml` that doesn't read, and a shard
/// database that doesn't.
pub fn check(
    world: &Path,
    layer: &str,
    shard: &Path,
) -> Result<(Option<String>, SavedPlacements, Vec<String>), String> {
    let saved = saved_placements::read(shard).map_err(|e| format!("{}: {e}", shard.display()))?;
    let Some((checkpoint, keys)) = keys_at(world, layer)? else {
        return Ok((None, saved, Vec::new()));
    };
    let found = keys.map_or_else(Vec::new, |k| saved_placements::refusals(&saved, &k));
    Ok((Some(checkpoint), saved, found))
}

/// `world cutover --world <world.toml> --layer <id> --shard <shard database>`.
pub(super) fn run(args: &[String]) -> Result<bool, String> {
    let mut world: Option<PathBuf> = None;
    let mut layer: Option<String> = None;
    let mut shard: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--world" => world = Some(value()?.into()),
            "--layer" => layer = Some(value()?),
            "--shard" => shard = Some(value()?.into()),
            _ => return Err(super::USAGE.into()),
        }
    }
    let (Some(world), Some(layer), Some(shard)) = (world, layer, shard) else {
        return Err(super::USAGE.into());
    };
    let (checkpoint, saved, found) = check(&world, &layer, &shard)?;
    for line in &found {
        println!("{line}");
    }
    let rows = format!(
        "{} saved static(s) and {} house permission(s)",
        saved.statics.len(),
        saved.house_permissions.len()
    );
    match checkpoint {
        Some(c) if saved.numbered_by.is_some() => println!(
            "{layer}: {rows}, numbered already by a world's keys ({}), so none follows {c}'s keys elsewhere",
            saved.numbered_by.as_deref().unwrap_or_default()
        ),
        None => println!(
            "{layer}: no checkpoint up to it, so it keeps its source's guids; {rows}, none moved"
        ),
        Some(c) if found.is_empty() => {
            println!("{layer}: {rows}, none under a guid {c}'s keys move")
        }
        Some(c) => println!(
            "{layer}: {rows}; {} under a guid {c}'s keys move: refused",
            found.len()
        ),
    }
    Ok(found.is_empty())
}
