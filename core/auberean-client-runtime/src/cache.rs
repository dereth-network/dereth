//! The client's one data cache: its data files as the download stores into them, and the
//! records' caches over those files, shared by everything that reads a record (the scripts, the
//! looks, the text, the data cache's requests).
//!
//! The files are opened over the files as they are and written in memory for the session
//! (`auberean_dat::disk::StoreSet::over_files`): a record the server sends is stored there, and
//! every read after goes through them, so a stored record is read as the files' own are, from
//! whichever part of the client reads it. The records' caches ([`Database`]) are one for the
//! whole client: a record loaded for a request is the one a script finds, and the other way
//! round.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use auberean_assets::Database;
use auberean_dat::disk::StoreSet;
use auberean_dat::DatFile;
use auberean_text::DatSource;

/// The data cache, made at the first need; clones share it.
#[derive(Clone)]
pub struct DataCache(Arc<Inner>);

struct Inner {
    files: Vec<Arc<DatFile>>,
    /// Seconds since 1970, what a stored record is dated with.
    clock: Arc<AtomicU32>,
    made: OnceLock<Made>,
}

// D2-05154
struct Made {
    stores: Arc<StoreSet>,
    database: Arc<Database>,
}

impl core::fmt::Debug for DataCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DataCache")
            .field("files", &self.0.files.len())
            .field("open", &self.0.made.get().is_some())
            .finish()
    }
}

impl DataCache {
    /// A cache over these files (the portal file first), storing records dated by `clock`.
    // D2-05182
    #[must_use]
    pub fn new(files: Vec<Arc<DatFile>>, clock: Arc<AtomicU32>) -> Self {
        Self(Arc::new(Inner {
            files,
            clock,
            made: OnceLock::new(),
        }))
    }

    /// Whether `other` is this cache (a clone of it), not another made over the same files.
    #[must_use]
    pub fn is(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The files it was made over.
    #[must_use]
    pub fn files(&self) -> &[Arc<DatFile>] {
        &self.0.files
    }

    /// Whether it has no files (a client without data reads no record).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.files.is_empty()
    }

    /// Whether it has been opened (a record read, or the download begun).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.0.made.get().is_some()
    }

    /// The cache's start, once the host has opened the files: each file in its slot and known by
    /// its id, the records' caches over them, the master map found through the portal file.
    // D2-05172
    fn made(&self) -> &Made {
        self.0.made.get_or_init(|| {
            let clock = self.0.clock.clone();
            let clock: Arc<dyn Fn() -> u32 + Send + Sync> =
                Arc::new(move || clock.load(Ordering::Relaxed));
            let stores = Arc::new(StoreSet::over_files(&self.0.files, &clock));
            let database = Arc::new(Database::over_stores(self.0.files.clone(), stores.clone()));
            Made { stores, database }
        })
    }

    /// The files as the download stores into them.
    #[must_use]
    pub fn stores(&self) -> &StoreSet {
        &self.made().stores
    }

    /// The records' caches, reading through [`DataCache::stores`].
    #[must_use]
    pub fn database(&self) -> &Database {
        &self.made().database
    }

    /// A text source over the cache: string tables and the language read through it, so a
    /// table stored for the session is written from as the files' own are.
    #[must_use]
    pub fn text_source(&self) -> DatSource {
        let cache = self.clone();
        DatSource::with_reader(
            self.0.files.clone(),
            Arc::new(move |id| cache.database().read(id)),
        )
    }
}
