//! The data-file patching state a test server builds at start, as the server's own start builds it,
//! but with each data file that has no overlay over it walked once for the whole test program.
//! Walking a file reads and compresses every one of its records (the retail cell file alone holds
//! hundreds of thousands), so a walk per server, with many servers starting at once, has them
//! queue on the same files and take minutes. A file's walk depends on nothing but the file, so
//! every server over it is given a copy of the one walk. A file with an overlay over it is walked
//! by each server, since its overlay is that test's own.
//! Fixture: a table of finished walks, by the file's path, length and modification time.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::SystemTime;

use dereth_primitives::ContainerEra;
use empyrean_common::config_manager::ConfigManager;
use empyrean_dat::{DatDatabase, DatDatabaseType, DatManager};
use empyrean_world::managers::ddd_manager::{self, DatFileSize, DddManagerState};

/// What one file's walk adds to the state.
struct Walked {
    iterations: BTreeMap<u32, Vec<u32>>,
    sizes: HashMap<u32, DatFileSize>,
    compressed: HashMap<u32, Vec<u8>>,
}

/// A file as walked: its path, length and modification time, which database of its set it is,
/// the set's era, and whether the walk keeps the compressed records.
type Key = (
    String,
    u64,
    Option<SystemTime>,
    DatDatabaseType,
    ContainerEra,
    bool,
);

/// `ddd_manager::initialize` on `state`: every database of `dats`, each one with no overlay over
/// it taken from its file's one walk.
pub(crate) fn initialize(state: &mut DddManagerState, dats: &DatManager) {
    let databases = [
        (DatDatabaseType::Portal, Some(&**dats.portal_dat())),
        (DatDatabaseType::Cell, Some(&**dats.cell_dat())),
        (DatDatabaseType::Language, Some(&**dats.language_dat())),
        (DatDatabaseType::HighRes, dats.high_res_dat()),
    ];
    for (db, dat) in databases {
        let Some(dat) = dat else { continue };
        match walked(db, dat) {
            Some(w) => {
                state.iterations.insert(db, w.iterations.clone());
                state.dat_file_sizes.insert(db, w.sizes.clone());
                state
                    .compressed_dat_files_cache
                    .insert(db, w.compressed.clone());
            }
            None => {
                ddd_manager::init_iterations_of(state, db, dat, |_, _| true);
            }
        }
    }
}

/// A file's walk, finished or under way.
type Walk = Arc<OnceLock<Arc<Walked>>>;

/// The one walk of `dat`'s file, or `None` when it has an overlay over it or is not a file.
fn walked(db: DatDatabaseType, dat: &DatDatabase) -> Option<Arc<Walked>> {
    static WALKS: Mutex<Vec<(Key, Walk)>> = Mutex::new(Vec::new());
    if dat.overlay().is_some() {
        return None;
    }
    let path = dat.file_path();
    let file = std::fs::metadata(&path)
        .ok()
        .filter(std::fs::Metadata::is_file)?;
    let precache = ConfigManager::config().ddd.precache_compressed_dat_files;
    let key: Key = (
        path,
        file.len(),
        file.modified().ok(),
        db,
        dat.container_era(),
        precache,
    );
    let once = {
        let mut walks = WALKS.lock().unwrap_or_else(PoisonError::into_inner);
        match walks.iter().find(|(k, _)| *k == key) {
            Some((_, once)) => Arc::clone(once),
            None => {
                let once = Arc::new(OnceLock::new());
                walks.push((key, Arc::clone(&once)));
                once
            }
        }
    };
    // A server that asks while the walk is under way waits for it, rather than walking too.
    let walked = once.get_or_init(|| {
        let mut own = DddManagerState::new();
        ddd_manager::init_iterations_of(&mut own, db, dat, |_, _| true);
        Arc::new(Walked {
            iterations: own.iterations.remove(&db).unwrap_or_default(),
            sizes: own.dat_file_sizes.remove(&db).unwrap_or_default(),
            compressed: own
                .compressed_dat_files_cache
                .remove(&db)
                .unwrap_or_default(),
        })
    });
    Some(Arc::clone(walked))
}
