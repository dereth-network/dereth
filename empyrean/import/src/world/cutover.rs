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
//! keeps its source's guids, so any shard moves onto it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use empyrean_entity::enums::WeenieType;
use empyrean_store::saved_placements::{self, SavedPlacements};
use empyrean_world_db::checkpoint::Keys;

/// Every row of `saved` that would follow its guid elsewhere under `keys` (the keys of checkpoint
/// `checkpoint`), one line each.
#[must_use]
pub fn refusals(saved: &SavedPlacements, keys: &Keys, checkpoint: &str) -> Vec<String> {
    let moved: BTreeMap<u32, u32> = keys
        .map
        .iter()
        .filter(|(from, to)| from != to)
        .map(|(&f, &t)| (f, t))
        .collect();
    let taken: BTreeMap<u32, u32> = moved.iter().map(|(&f, &t)| (t, f)).collect();
    let why = |guid: u32| match (moved.get(&guid), taken.get(&guid)) {
        (Some(to), _) => Some(format!(
            "{checkpoint}'s keys.toml moves its placement to 0x{to:08X}"
        )),
        (None, Some(from)) => Some(format!(
            "{checkpoint}'s keys.toml gives its guid to the placement that was 0x{from:08X}"
        )),
        (None, None) => None,
    };
    let mut out = Vec::new();
    for s in &saved.statics {
        if let Some(why) = why(s.guid) {
            let kind = usize::try_from(s.weenie_type)
                .ok()
                .and_then(|t| WeenieType::NAMES.get(t))
                .copied()
                .unwrap_or("an unknown type");
            out.push(format!(
                "saved static 0x{:08X} (weenie {}, {kind}): {why}",
                s.guid, s.weenie_class_id
            ));
        }
    }
    for p in &saved.house_permissions {
        if let Some(why) = why(p.house) {
            out.push(format!(
                "house permission of house 0x{:08X} for player 0x{:08X}: {why}",
                p.house, p.player
            ));
        }
    }
    out
}

/// The shard database at `shard` against the world `world` at layer `layer`: what [`refusals`]
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
    let Some((checkpoint, keys)) = empyrean_world_db::world::checkpoint_keys_at(world, layer)?
    else {
        return Ok((None, saved, Vec::new()));
    };
    let found = keys.map_or_else(Vec::new, |k| refusals(&saved, &k, &checkpoint));
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
