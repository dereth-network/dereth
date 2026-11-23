//! Two packs compared record by record, ids aside: what a world's rows must rebuild of the pack
//! they were written from.
//!
//! Where a stable key keys a table (a weenie's class, a placement's landblock, a spell's id), the
//! records are compared field by field by that key, as `empyrean-import --check --fields` does;
//! where a dump's own ids key one (quests, events, the treasure tables), its records are compared
//! as a whole set, but for wielded treasure, whose order within a type is what a set means.

use std::fmt::Debug;

use empyrean_content::import::check::{self, fields::flatten_debug};
use empyrean_content::models::world::{
    Event, HousePortal, PointsOfInterest, Quest, TreasureGemCount, TreasureMaterialBase,
    TreasureMaterialColor, TreasureMaterialGroups, TreasureWielded, Version,
};
use empyrean_content::pack::{Codec, Pack, TableId};
use empyrean_content::PackError;

/// A record as its fields, by path, its ids left out.
type Record = Vec<(String, String)>;

/// The tables a dump's own ids key.
const BY_DUMP_ID: [TableId; 10] = [
    TableId::QUEST,
    TableId::EVENT,
    TableId::POINTS_OF_INTEREST,
    TableId::HOUSE_PORTAL,
    TableId::TREASURE_WIELDED,
    TableId::TREASURE_GEM_COUNT,
    TableId::TREASURE_MATERIAL_BASE,
    TableId::TREASURE_MATERIAL_COLOR,
    TableId::TREASURE_MATERIAL_GROUPS,
    TableId::VERSION,
];

fn by_id<T: Codec + Debug>(p: &Pack, t: TableId, sorted: bool) -> Result<Vec<Record>, PackError> {
    let mut v: Vec<Record> = p
        .all::<T>(t)?
        .into_iter()
        .map(|(_, r)| flatten_debug(&r, ""))
        .collect();
    if sorted {
        v.sort();
    }
    Ok(v)
}

fn by_id_tables(p: &Pack) -> Result<Vec<(&'static str, Vec<Record>)>, PackError> {
    Ok(vec![
        ("quest", by_id::<Quest>(p, TableId::QUEST, true)?),
        ("event", by_id::<Event>(p, TableId::EVENT, true)?),
        (
            "poi",
            by_id::<PointsOfInterest>(p, TableId::POINTS_OF_INTEREST, true)?,
        ),
        (
            "house_portal",
            by_id::<HousePortal>(p, TableId::HOUSE_PORTAL, true)?,
        ),
        (
            "treasure_wielded",
            by_id::<TreasureWielded>(p, TableId::TREASURE_WIELDED, false)?,
        ),
        (
            "treasure_gem",
            by_id::<TreasureGemCount>(p, TableId::TREASURE_GEM_COUNT, true)?,
        ),
        (
            "tmat_base",
            by_id::<TreasureMaterialBase>(p, TableId::TREASURE_MATERIAL_BASE, true)?,
        ),
        (
            "tmat_color",
            by_id::<TreasureMaterialColor>(p, TableId::TREASURE_MATERIAL_COLOR, true)?,
        ),
        (
            "tmat_groups",
            by_id::<TreasureMaterialGroups>(p, TableId::TREASURE_MATERIAL_GROUPS, true)?,
        ),
        ("version", by_id::<Version>(p, TableId::VERSION, true)?),
    ])
}

/// Every way `rebuilt` differs from `original`, ids aside, one line each; none when it holds every
/// record of `original`, field for field.
///
/// # Errors
/// A pack whose records don't read.
pub fn differences(original: &Pack, rebuilt: &Pack) -> Result<Vec<String>, PackError> {
    let mut out = Vec::new();
    let diffs: Vec<_> = check::diff(original, rebuilt)?
        .into_iter()
        .filter(|d| TableId(d.id) != TableId::WEENIE_INDEX && !BY_DUMP_ID.contains(&TableId(d.id)))
        .collect();
    for d in &diffs {
        if !d.added.is_empty() || !d.removed.is_empty() {
            out.push(format!(
                "{}: {} added, {} removed",
                d.table,
                d.added.len(),
                d.removed.len()
            ));
        }
    }
    for r in check::fields::changed_fields(&diffs, original, rebuilt)? {
        // A recipe mod's own rows name it by a dump's id too.
        for c in r
            .changes
            .iter()
            .filter(|c| !c.field.ends_with("recipe_mod_id"))
        {
            out.push(format!(
                "{} {:#x}: {} was {:?}, now {:?}",
                r.table, r.key, c.field, c.old, c.new
            ));
        }
    }
    for ((name, a), (_, b)) in by_id_tables(original)?
        .into_iter()
        .zip(by_id_tables(rebuilt)?)
    {
        if a.len() != b.len() {
            out.push(format!("{name}: {} records, now {}", a.len(), b.len()));
        } else if let Some(i) = a.iter().zip(&b).position(|(x, y)| x != y) {
            out.push(format!(
                "{name}: record {i} differs: {:?}, now {:?}",
                a[i], b[i]
            ));
        }
    }
    Ok(out)
}

/// The pack's own id for a world of rows: BLAKE3 of its files, path and text each, in order.
#[must_use]
pub fn dataset_of(files: &std::collections::BTreeMap<String, String>) -> [u8; 16] {
    let mut h = blake3::Hasher::new();
    for (path, text) in files {
        h.update(path.as_bytes());
        h.update(&[0]);
        h.update(text.as_bytes());
        h.update(&[0]);
    }
    let mut out = [0; 16];
    out.copy_from_slice(&h.finalize().as_bytes()[..16]);
    out
}
