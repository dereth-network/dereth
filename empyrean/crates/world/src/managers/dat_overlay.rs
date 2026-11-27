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
//!   rather than patched into an incoherent world; a client that checks bases by their records is
//!   left to check a file laid out otherwise itself); `DDD_OverlayManifestMessage` (`0xF7EC`) names
//!   the world, the bases, every record with its SHA-256 and every deletion; the world's cell
//!   records come in the patch itself rather than being purged for the client to ask for one by
//!   one; and a client that names the world its iteration lists are of, and names another, is told
//!   this world's name and asked again (`0xF7E5`), once a session, so it answers from this world's
//!   own folder.
//! - **Deletions** (V438): a record the overlay deletes from the portal, language or high-res file
//!   is purged in its revision, which ACE never does; a deleted cell takes its landblock family's
//!   purge for a client without the extension (the retail purge's own reach), and stays a
//!   single-record deletion in the manifest for one with it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use dereth_protocol::admin as proto;
use empyrean_dat::{DatDatabase, DatDatabaseType, DatManager};
use empyrean_net::GameMessageGroup;

use crate::managers::ddd_manager::HI_FI_STRING_AS_INT;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

/// Not ACE (V452): the world's files opened again, with the data overlay in `overlay` over them
/// (the one over them now, read afresh, when `None`): the world and its physics read the new files
/// from now on, and so do its patches when the overlay is patched into clients (listing every
/// file's revision again takes a while; a world whose overlay is not patched keeps the base's
/// lists, which do not change). A landblock already loaded keeps what it was built from until
/// [`reload_landblock`] builds it again.
///
/// # Errors
/// The files cannot be opened again, or the overlay is refused; the world keeps its files.
pub fn reopen(w: &mut crate::World, overlay: Option<&Path>) -> Result<(), String> {
    let dats = w.dats.reopen(overlay)?;
    crate::physics::phys_ext::use_dats(w, &dats);
    w.dats = Arc::clone(&dats);
    // The patches list the new files' revisions, when the overlay is patched into clients.
    let patching = empyrean_common::config_manager::ConfigManager::try_config()
        .is_some_and(|c| c.dat_overlay.patching);
    if patching {
        let debug = w.ddd_manager.debug;
        w.ddd_manager = crate::managers::ddd_manager::DddManagerState::new();
        w.ddd_manager.debug = debug;
        crate::managers::ddd_manager::initialize(&mut w.ddd_manager, &dats);
    }
    Ok(())
}

/// Not ACE (V565): the world's own placements of `landblocks` read again from the overlay over
/// the world's files now (its [`empyrean_content::own_placements::FILE`]), so a landblock built
/// again places what the overlay places now; none, where it has no file. A world with no overlay
/// keeps them as they are.
///
/// # Errors
/// The file is there and cannot be read, or does not read as placements; the world keeps them as
/// they are.
pub fn reread_own_placements(w: &mut crate::World, landblocks: &[u16]) -> Result<(), String> {
    let Some(dir) = w
        .dats
        .cell_dat()
        .overlay()
        .and_then(|l| l.path().parent().map(Path::to_path_buf))
    else {
        return Ok(());
    };
    read_own_placements_in(w, &dir, landblocks)
}

/// Not ACE (V565): the world's own placements of `landblocks` read again from overlay folder
/// `dir`'s [`empyrean_content::own_placements::FILE`]; none, where it has no file.
///
/// # Errors
/// As [`reread_own_placements`].
pub fn read_own_placements_in(
    w: &mut crate::World,
    dir: &Path,
    landblocks: &[u16],
) -> Result<(), String> {
    let file = dir.join(empyrean_content::own_placements::FILE);
    let rows = match std::fs::read_to_string(&file) {
        Ok(text) => empyrean_content::own_placements::parse(&text)
            .map_err(|e| format!("{}: {e}", file.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    w.content.replace_own(landblocks, rows);
    Ok(())
}

/// The file in a world's overlay folder naming the world's own towns and landmarks: a places
/// document, which a client names them by on its maps in place of the game's map's notes.
pub const WORLD_PLACES_FILE: &str = "places.tsv";

/// Not ACE (V578): the world's own places document ([`WORLD_PLACES_FILE`]) in the folder of the
/// data overlay over `dats`, read now; `None` for files with no overlay, a folder without the
/// file, or a file that cannot be read as text (logged).
#[must_use]
pub fn world_places(dats: &DatManager) -> Option<String> {
    let dir = DATABASES.into_iter().find_map(|db| {
        database(dats, db)
            .and_then(DatDatabase::overlay)
            .and_then(|l| l.path().parent().map(Path::to_path_buf))
    })?;
    let file = dir.join(WORLD_PLACES_FILE);
    match std::fs::read_to_string(&file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            log::warn!(
                "The world's own places are not sent: {}: {e}",
                file.display()
            );
            None
        }
    }
}

/// Not ACE (V452): landblock `landblock` built again from the files and the world database as
/// they are now, as `reload-landblock` builds the player's own: its objects destroyed (players
/// stay), its instances read again, its physics built again, and its objects made again. Answers
/// whether it was loaded; one that is not drops what the land source holds of it, and is read
/// afresh when it next loads.
pub fn reload_landblock(w: &mut crate::World, landblock: u16) -> bool {
    let id = empyrean_entity::LandblockId::new((u32::from(landblock) << 16) | 0xFFFF);
    w.content.clear_cached_instances_by_landblock(landblock);
    if !crate::managers::landblock_manager::is_loaded(w, id) {
        crate::physics::phys_ext::forget_landblock(w, landblock);
        return false;
    }
    crate::entity::landblock::destroy_all_non_player_objects(w, id);
    crate::physics::phys_ext::reload_landblock(w, landblock);
    crate::entity::landblock::init(w, id, true);
    true
}

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

/// What a client that keeps overlays says of its files in its `0xF7E6`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClientBases<'a> {
    /// The base files it holds, each by its layout.
    pub bases: &'a [proto::OverlayBase],
    /// It checks a base the manifest names by its records itself, so a file of its laid out
    /// otherwise may still be the base.
    pub checks_records: bool,
    /// Not ACE (V520): it holds the earlier portal file of a step up to this world's, and is
    /// patched up to the world's file beneath the world's overlay.
    pub step: Option<&'static crate::managers::launch_base::BaseStep>,
}

impl<'a> ClientBases<'a> {
    /// What `response` says.
    #[must_use]
    pub fn of(response: &'a proto::DddInterrogationResponse) -> Self {
        Self {
            bases: &response.overlay_bases,
            checks_records: response.flags & proto::DddInterrogationResponse::FLAG_RECORDS != 0,
            step: None,
        }
    }

    /// The layout of the client's file `db`, if it named one.
    fn held(&self, db: DatDatabaseType) -> Option<[u8; 32]> {
        let pair = wire_pair(db);
        self.bases
            .iter()
            .find(|b| (b.dat_file_type, b.dat_file_id) == pair)
            .map(|b| b.fingerprint)
    }
}

/// The layouts a client's file may have and hold the overlay's base: the one the overlay was
/// written over, and that of this server's own file, which the overlay was checked against when it
/// was opened.
fn base_layouts(d: &DatDatabase, layer: &empyrean_dat::data_overlay::Layer) -> [[u8; 32]; 2] {
    let made = layer.manifest().base_fingerprint;
    [made, d.base_fingerprint().unwrap_or(made)]
}

/// Why a client holding `client`'s bases cannot take this world's overlay: the first overlaid
/// file whose base it does not hold. `None` when it holds every one, or when the overlay names a
/// base by its records and the client, which checks those itself, holds a file of that kind laid
/// out otherwise.
#[must_use]
pub fn base_mismatch(dats: &DatManager, client: &ClientBases<'_>) -> Option<String> {
    for db in overlaid(dats) {
        let d = database(dats, db)?;
        let layer = d.overlay()?;
        let held = client.held(db);
        if held.is_some_and(|h| base_layouts(d, layer).contains(&h)) {
            continue;
        }
        // The client is patched up to the base first (V520).
        if db == DatDatabaseType::Portal && client.step.is_some() {
            continue;
        }
        if client.checks_records && held.is_some() && layer.manifest().base_records.is_some() {
            continue;
        }
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

/// The world's overlay manifest, for `client`. Each file's base is named by the layout the client
/// holds when that is one of the base's, else by the overlay's own; its records are named only to
/// a client that checks them. For a client patched up to the world's portal file first (V520),
/// the portal's base is the client's earlier file, and the step's records and revisions are the
/// overlay's too.
#[must_use]
pub fn manifest(dats: &DatManager, client: &ClientBases<'_>) -> proto::DddOverlayManifest {
    let mut files = Vec::new();
    let mut total: u64 = 0;
    let overlaid = overlaid(dats);
    // A world with an overlay over its other files alone: the step is the portal's overlay.
    let portal_only_stepped = !overlaid.is_empty() && !overlaid.contains(&DatDatabaseType::Portal);
    if let Some(step) = client.step.filter(|_| portal_only_stepped) {
        let (dat_file_type, dat_file_id) = wire_pair(DatDatabaseType::Portal);
        let records = step.manifest_records(dats);
        total += records.iter().map(|r| u64::from(r.size)).sum::<u64>();
        files.push(proto::OverlayFileManifest {
            dat_file_type,
            dat_file_id,
            base_name: "portal.dat".to_owned(),
            base_fingerprint: step.from.fingerprint,
            base_records: client.checks_records.then_some(step.from.records),
            base_iterations: step.from.iteration,
            exact_iterations: false,
            revisions: step.revisions().collect(),
            records,
            tombstones: Vec::new(),
        });
    }
    for db in overlaid {
        let Some(d) = database(dats, db) else {
            continue;
        };
        let Some(layer) = d.overlay() else {
            continue;
        };
        let m = layer.manifest();
        let (dat_file_type, dat_file_id) = wire_pair(db);
        let step = client.step.filter(|_| db == DatDatabaseType::Portal);
        let stepped = step.map(|s| s.manifest_records(dats)).unwrap_or_default();
        total += stepped.iter().map(|r| u64::from(r.size)).sum::<u64>();
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
            .collect::<Vec<_>>();
        let (base_fingerprint, base_records, base_iterations, revisions, records) = match step {
            Some(step) => (
                step.from.fingerprint,
                Some(step.from.records),
                step.from.iteration,
                step.revisions()
                    .chain(layer.own_iterations().iter().copied())
                    .collect(),
                stepped.into_iter().chain(records).collect(),
            ),
            None => (
                client
                    .held(db)
                    .filter(|h| base_layouts(d, layer).contains(h))
                    .unwrap_or(m.base_fingerprint),
                m.base_records,
                m.base_iterations,
                layer.own_iterations().to_vec(),
                records,
            ),
        };
        files.push(proto::OverlayFileManifest {
            dat_file_type,
            dat_file_id,
            base_name: m.base_name.clone(),
            base_fingerprint,
            base_records: base_records.filter(|_| client.checks_records),
            base_iterations,
            exact_iterations: m.exact_iterations,
            revisions,
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
