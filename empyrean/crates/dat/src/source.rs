//! Where a [`crate::DatDatabase`] gets its files: the retail dats ([`RealDats`]) or an in-memory
//! set built by a test ([`crate::FakeDats`]).
//!
//! Nothing here is ported from ACE; it is the seam that lets the unit tier run with no dats.

use std::fmt;
use std::path::{Path, PathBuf};

use dereth_dat::{DatError, DatFile, RetailDatStore};
use dereth_primitives::ContainerEra;

use crate::database::{CachedObject, DatDatabaseType};

/// A set of dat files, by database.
pub trait DatSource: Send + Sync + fmt::Debug {
    /// Whether this source has the database at all (the high-res dat is optional).
    fn has_database(&self, db: DatDatabaseType) -> bool;

    /// Every file id in the database, ascending (ACE's `AllFiles.Keys`).
    fn file_ids(&self, db: DatDatabaseType) -> Vec<u32>;

    /// How many files the database has (ACE's `AllFiles.Count`).
    fn file_count(&self, db: DatDatabaseType) -> usize {
        self.file_ids(db).len()
    }

    /// Whether the database has `id`.
    fn contains(&self, db: DatDatabaseType, id: u32) -> bool;

    /// The raw payload of `id`, or `None` when the database has no such file.
    fn read(&self, db: DatDatabaseType, id: u32) -> Option<Vec<u8>>;

    /// The iteration that introduced `id` (the directory entry's `iter_`, ACE's
    /// `DatFile.Iteration`), or `None` when the database has no such file.
    fn file_iteration(&self, db: DatDatabaseType, id: u32) -> Option<u32>;

    /// An already-decoded object for `id`, which the cache takes in place of decoding bytes.
    /// Only a test source has these.
    fn decoded(&self, _db: DatDatabaseType, _id: u32) -> Option<CachedObject> {
        None
    }

    /// A name for log lines: the file path, or what the fake is.
    fn describe(&self, db: DatDatabaseType) -> String;

    /// Which dat set the files belong to: the later four files unless a source says otherwise.
    fn container_era(&self) -> ContainerEra {
        ContainerEra::Tod
    }

    /// The whole file's iteration from its header, which only the files from before Throne of
    /// Destiny carry; `None` otherwise (the iteration is then the `0xFFFF0001` record).
    fn header_iteration(&self, _db: DatDatabaseType) -> Option<u32> {
        None
    }

    /// Not ACE: the world's data overlay over the database, when one is laid over it.
    fn overlay(&self, _db: DatDatabaseType) -> Option<&dereth_dat::overlay::Layer> {
        None
    }

    /// Not ACE: the fingerprint of the database's base file, as an overlay names its base.
    fn base_fingerprint(&self, _db: DatDatabaseType) -> Option<[u8; 32]> {
        None
    }

    /// Not ACE: the world the data overlay belongs to, when one is laid over the files.
    fn overlay_world_key(&self) -> Option<String> {
        None
    }
}

/// The retail dats from one directory.
///
/// The files are opened through `dereth-dat`'s process-wide table, so however many `RealDats` a
/// process makes, each file's directory is walked **once** (and a `DatManager` shared as an `Arc`
/// holds one `RealDats` anyway). Unlike `dereth-dat`'s store, there is no compiled-in directory: the
/// caller always names one.
#[derive(Debug)]
pub struct RealDats {
    dir: PathBuf,
    store: RetailDatStore,
    /// Not ACE: the world the data overlay over the files belongs to, when one is laid over them.
    overlay_world: Option<String>,
}

impl RealDats {
    /// Open the three required files under `dir`, and `client_highres.dat` when it is there
    /// (ACE opens it whenever it exists).
    ///
    /// # Errors
    ///
    /// A required file is missing or does not open.
    pub fn open(dir: &Path) -> Result<Self, DatError> {
        // Not ACE: a directory with `portal.dat` and `cell.dat` and no `client_portal.dat` is
        // the dat set from before Throne of Destiny, whose language reads the portal file answers.
        let store = if !dereth_dat::holds_retail_dats(dir) && dereth_dat::holds_pre_tod_dats(dir) {
            RetailDatStore::open_pre_tod_dir(dir)?
        } else {
            let store = RetailDatStore::open_dir(dir)?;
            store.grant_highres()?;
            store
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            store,
            overlay_world: None,
        })
    }

    /// Open the set under `dir` that a world drawn from the files of `era` reads: `portal.dat` and
    /// `cell.dat` for the files before Throne of Destiny, the later files (and `client_highres.dat`
    /// when it is there) otherwise. One folder may hold both sets; the era chooses, and a folder
    /// without the chosen set is refused rather than read as the other.
    ///
    /// # Errors
    ///
    /// A required file of that set is missing or does not open.
    pub fn open_era(dir: &Path, era: ContainerEra) -> Result<Self, DatError> {
        let store = match era {
            ContainerEra::PreTod => RetailDatStore::open_pre_tod_dir(dir)?,
            ContainerEra::Tod => {
                let store = RetailDatStore::open_dir(dir)?;
                store.grant_highres()?;
                store
            }
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            store,
            overlay_world: None,
        })
    }

    /// Not ACE: these files with the world's data overlay in `dir` over them, so every read is the
    /// world's: a record the overlay holds is its own, a record it deletes is not there, and its
    /// revisions are among each file's iterations. Each container must have been made against the
    /// file it lies over, and all of them for one world.
    ///
    /// # Errors
    /// The folder holds no overlay, holds base data files, or a container is refused.
    pub fn with_overlay(mut self, dir: &Path) -> Result<Self, String> {
        let folder = dereth_dat::overlay::OverlayDir::new(dir).map_err(|e| e.to_string())?;
        let key = folder
            .world_key()
            .ok_or_else(|| format!("{} holds no data overlay", dir.display()))?;
        self.store = self
            .store
            .with_overlay(&folder, Some(&key))
            .map_err(|e| e.to_string())?;
        self.overlay_world = Some(key);
        Ok(self)
    }

    /// The directory the files came from.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, db: DatDatabaseType) -> Option<&DatFile> {
        match db {
            DatDatabaseType::Portal => Some(self.store.portal()),
            DatDatabaseType::Cell => Some(self.store.cell()),
            DatDatabaseType::Language => Some(self.store.local()),
            DatDatabaseType::HighRes => self.store.highres(),
        }
    }
}

impl DatSource for RealDats {
    fn has_database(&self, db: DatDatabaseType) -> bool {
        self.file(db).is_some()
    }

    fn file_ids(&self, db: DatDatabaseType) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .file(db)
            .map(|f| f.iter_ids().map(|i| i.0).collect())
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    fn file_count(&self, db: DatDatabaseType) -> usize {
        self.file(db).map_or(0, DatFile::len)
    }

    fn contains(&self, db: DatDatabaseType, id: u32) -> bool {
        self.file(db)
            .is_some_and(|f| f.contains(dereth_primitives::DataId(id)))
    }

    fn read(&self, db: DatDatabaseType, id: u32) -> Option<Vec<u8>> {
        let file = self.file(db)?;
        match file.read(dereth_primitives::DataId(id)) {
            Ok(bytes) => Some(bytes),
            Err(DatError::NotFound(_)) => None,
            Err(e) => {
                log::error!("{:?} dat: reading 0x{id:08X} failed: {e}", db);
                None
            }
        }
    }

    fn file_iteration(&self, db: DatDatabaseType, id: u32) -> Option<u32> {
        self.file(db)?
            .entry(dereth_primitives::DataId(id))
            .map(|e| e.iteration)
    }

    fn container_era(&self) -> ContainerEra {
        self.store.era()
    }

    fn overlay(&self, db: DatDatabaseType) -> Option<&dereth_dat::overlay::Layer> {
        self.file(db)?.layer()
    }

    fn base_fingerprint(&self, db: DatDatabaseType) -> Option<[u8; 32]> {
        Some(dereth_dat::overlay::fingerprint(self.file(db)?))
    }

    fn overlay_world_key(&self) -> Option<String> {
        self.overlay_world.clone()
    }

    fn header_iteration(&self, db: DatDatabaseType) -> Option<u32> {
        self.file(db)?.header_iteration()
    }

    fn describe(&self, db: DatDatabaseType) -> String {
        if self.store.era() == ContainerEra::PreTod {
            let file = match db {
                DatDatabaseType::Cell => dereth_dat::PreTodDat::Cell,
                _ => dereth_dat::PreTodDat::Portal,
            };
            return file.in_dir(&self.dir).display().to_string();
        }
        let file = match db {
            DatDatabaseType::Portal => dereth_dat::RetailDat::Portal,
            DatDatabaseType::Cell => dereth_dat::RetailDat::Cell,
            DatDatabaseType::Language => dereth_dat::RetailDat::Local,
            DatDatabaseType::HighRes => dereth_dat::RetailDat::HighRes,
        };
        file.in_dir(&self.dir).display().to_string()
    }
}
