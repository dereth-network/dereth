//! Not ACE: patching a client that keeps overlays to a world served as base files plus a data
//! overlay (`[dat_overlay]`).
//!
//! The world's files are the base with the overlay over it, so ACE's own patch logic already
//! finds the overlay's revisions missing from a client holding the base and lists their records.
//! What this adds, all divergences:
//!
//! - **The overlay extension** (V437), only for a client that sets the overlay flag in its
//!   `DDD_InterrogationResponseMessage` (`0xF7E6`): the bases it holds are checked against the
//!   ones the overlay was made against (a client holding another base is refused, with the reason,
//!   rather than patched into an incoherent world); `DDD_OverlayManifestMessage` (`0xF7EC`) names
//!   the world, the bases, every record with its SHA-256 and every deletion; and the world's cell
//!   records come in the patch itself rather than being purged for the client to ask for one by one.
//! - **Deletions** (V438): a record the overlay deletes from the portal, language or high-res file
//!   is purged in its revision, which ACE never does; a deleted cell takes its landblock family's
//!   purge for a client without the extension (the retail purge's own reach), and stays a
//!   single-record deletion in the manifest for one with it.

use std::collections::HashMap;

use dereth_protocol::admin as proto;
use empyrean_dat::{DatDatabase, DatDatabaseType, DatManager};
use empyrean_net::GameMessageGroup;

use crate::managers::ddd_manager::HI_FI_STRING_AS_INT;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

/// The four databases, in the order a patch lists them.
pub const DATABASES: [DatDatabaseType; 4] = [
    DatDatabaseType::Portal,
    DatDatabaseType::Language,
    DatDatabaseType::Cell,
    DatDatabaseType::HighRes,
];

/// The landblock family mask the retail cell purge deletes by.
pub const LANDBLOCK_MASK: u32 = 0xFFFF_0000;

/// A database's wire pair, `(dat file type, dat file id)`.
#[must_use]
pub fn wire_pair(db: DatDatabaseType) -> (u32, u32) {
    match db {
        DatDatabaseType::Portal => (0, 1),
        DatDatabaseType::Cell => (1, 2),
        DatDatabaseType::Language => (1, 3),
        DatDatabaseType::HighRes => (HI_FI_STRING_AS_INT.cast_unsigned(), 1),
    }
}

/// The database of `dats` by type.
#[must_use]
pub fn database(dats: &DatManager, db: DatDatabaseType) -> Option<&DatDatabase> {
    match db {
        DatDatabaseType::Portal => Some(dats.portal_dat()),
        DatDatabaseType::Cell => Some(dats.cell_dat()),
        DatDatabaseType::Language => Some(dats.language_dat()),
        DatDatabaseType::HighRes => dats.high_res_dat(),
    }
}

/// The databases with an overlay over them. A file a set from before Throne of Destiny reads
/// twice (its portal file is also its language file) is named once, as the portal.
#[must_use]
pub fn overlaid(dats: &DatManager) -> Vec<DatDatabaseType> {
    let same_file = dats.language_dat().file_path() == dats.portal_dat().file_path();
    DATABASES
        .into_iter()
        .filter(|db| !(same_file && *db == DatDatabaseType::Language))
        .filter(|db| database(dats, *db).is_some_and(|d| d.overlay().is_some()))
        .collect()
}

/// Why a client holding `bases` cannot take this world's overlay: the first overlaid file whose
/// base it does not hold. `None` when it holds every one.
#[must_use]
pub fn base_mismatch(dats: &DatManager, bases: &[proto::OverlayBase]) -> Option<String> {
    for db in overlaid(dats) {
        let d = database(dats, db)?;
        let layer = d.overlay()?;
        let (ty, id) = wire_pair(db);
        let held = bases
            .iter()
            .find(|b| (b.dat_file_type, b.dat_file_id) == (ty, id))
            .map(|b| b.fingerprint);
        if held != Some(layer.manifest().base_fingerprint) {
            return Some(format!(
                "this world's {} overlay was made against {} {}, and the client holds {}",
                d.file_path(),
                layer.manifest().base_name,
                empyrean_dat::data_overlay::hex(&layer.manifest().base_fingerprint),
                held.map_or_else(
                    || "none".to_owned(),
                    |h| empyrean_dat::data_overlay::hex(&h)
                )
            ));
        }
    }
    None
}

/// Every deletion of the world's overlay, by database and revision: `(id, mask)`.
#[must_use]
pub fn deletions(dats: &DatManager) -> HashMap<(DatDatabaseType, u32), Vec<(u32, u32)>> {
    let mut out: HashMap<(DatDatabaseType, u32), Vec<(u32, u32)>> = HashMap::new();
    for db in overlaid(dats) {
        let Some(layer) = database(dats, db).and_then(DatDatabase::overlay) else {
            continue;
        };
        for t in layer.tombstones() {
            out.entry((db, t.iteration))
                .or_default()
                .push((t.id, t.mask));
        }
    }
    out
}

/// The world's overlay manifest.
#[must_use]
pub fn manifest(dats: &DatManager) -> proto::DddOverlayManifest {
    let mut files = Vec::new();
    let mut total: u64 = 0;
    for db in overlaid(dats) {
        let Some(layer) = database(dats, db).and_then(DatDatabase::overlay) else {
            continue;
        };
        let m = layer.manifest();
        let (dat_file_type, dat_file_id) = wire_pair(db);
        let records = layer
            .records()
            .map(|(id, e)| {
                total += u64::from(e.size);
                let sha256 = m.hashes.get(&id.raw()).copied().unwrap_or_else(|| {
                    layer
                        .read(id)
                        .map_or([0; 32], |b| empyrean_dat::data_overlay::record_hash(&b))
                });
                proto::OverlayRecord {
                    id: id.raw(),
                    iteration: e.iteration,
                    size: e.size,
                    sha256,
                }
            })
            .collect();
        files.push(proto::OverlayFileManifest {
            dat_file_type,
            dat_file_id,
            base_name: m.base_name.clone(),
            base_fingerprint: m.base_fingerprint,
            base_iterations: m.base_iterations,
            exact_iterations: m.exact_iterations,
            revisions: layer.own_iterations().to_vec(),
            records,
            tombstones: layer
                .tombstones()
                .iter()
                .map(|t| proto::OverlayTombstone {
                    id: t.id,
                    mask: t.mask,
                    iteration: t.iteration,
                })
                .collect(),
        });
    }
    proto::DddOverlayManifest {
        world_key: dats.portal_dat().overlay_world_key().unwrap_or_default(),
        total_bytes: u32::try_from(total).unwrap_or(u32::MAX),
        files,
    }
}

/// `0xF7EC` with the world's manifest.
#[must_use]
pub fn game_message_overlay_manifest(m: &proto::DddOverlayManifest) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::DDD_OverlayManifest,
        GameMessageGroup::DatabaseQueue,
        m,
    )
}

/// `0xF7E7` for a world with a data overlay: ACE's revisions, with the overlay's deletions purged
/// in theirs. With `eager_cells` (a client with the extension) a cell revision downloads its
/// records and purges only the overlay's whole-landblock deletions; without it, a cell revision is
/// ACE's (every record's landblock purged, for the client to ask for again), and a deleted cell
/// purges its landblock too.
#[must_use]
pub fn game_message_ddd_begin_ddd(
    total_file_size: u32,
    missing: &HashMap<DatDatabaseType, empyrean_common::dotnet::DotNetDict<u32, Vec<u32>>>,
    deletions: &HashMap<(DatDatabaseType, u32), Vec<(u32, u32)>>,
    eager_cells: bool,
) -> GameMessage {
    let mut revisions = Vec::new();
    // ACE's order: portal, language, cell, high-res.
    for db in DATABASES {
        let Some(iterations) = missing.get(&db) else {
            continue;
        };
        let (dat_file_type, dat_file_id) = wire_pair(db);
        for (iteration, files) in iterations.iter() {
            let gone = deletions
                .get(&(db, *iteration))
                .cloned()
                .unwrap_or_default();
            let (ids_to_download, ids_to_purge) = if db == DatDatabaseType::Cell {
                let families = gone
                    .iter()
                    .filter(|(_, mask)| *mask != 0)
                    .map(|(id, _)| id | 0xFFFF);
                if eager_cells {
                    (files.clone(), families.collect())
                } else {
                    let mut purge: Vec<u32> = files.clone();
                    purge.extend(gone.iter().map(|(id, _)| (id & LANDBLOCK_MASK) | 0xFFFF));
                    purge.sort_unstable();
                    purge.dedup();
                    (Vec::new(), purge)
                }
            } else {
                (files.clone(), gone.iter().map(|(id, _)| *id).collect())
            };
            revisions.push(proto::PatchRevision {
                dat_file_type,
                dat_file_id,
                iteration: *iteration,
                ids_to_download,
                ids_to_purge,
            });
        }
    }
    GameMessage::from_proto(
        GameMessageOpcode::DDD_BeginDDD,
        GameMessageGroup::DatabaseQueue,
        &proto::DddBeginDdd {
            data_expected: total_file_size,
            revisions,
        },
    )
}
