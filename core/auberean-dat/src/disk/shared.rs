//! The data files as the data cache holds them: each one shared between the game and the thread
//! that loads and stores records, and the four the client opens (the portal, a language's local
//! file, a cell file and the high-resolution pictures), with the rule for which file a record is
//! in.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dereth_dat::overlay::Layer;

use super::controller::{ObjectInfo, OpenSpec, Store, OPEN_EXPANDABLE};
use super::device::Files;
use super::device::OpenMode;
use super::intset::VersionSet;
use super::overlay::Overlay;
use crate::container::DatFile;
use crate::qualified::QualifiedId;
use crate::types::{type_of_id, RecordType};

/// The notice shown when a file opens but its list of data versions does not read: the file is
/// closed.
pub const NO_VERSIONS_NOTICE: u32 = 0xCE;

/// How opening a shared file went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Ready,
    /// The file did not open (or was not made).
    Failed,
    /// It opened, but its list of data versions did not read; it is closed again and
    /// [`NO_VERSIONS_NOTICE`] is shown, naming it.
    NoVersions,
}

/// One data file behind a lock, with the data versions it held when opened.
#[derive(Debug)]
pub struct SharedStore {
    store: Mutex<Store>,
    /// The data versions the file holds, read when it was opened.
    pub held: Mutex<VersionSet>,
}

impl SharedStore {
    /// A file not yet opened, dating what it stores by `clock`.
    // D2-05122
    #[must_use]
    pub fn new(clock: Box<dyn FnMut() -> u32 + Send>) -> Self {
        Self {
            store: Mutex::new(Store::new(clock)),
            held: Mutex::new(VersionSet::new()),
        }
    }

    /// The file itself, locked.
    pub fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Open (or make) the file, then read its list of data versions into [`SharedStore::held`];
    /// a file whose list does not read is closed.
    // D2-05136
    pub fn init_file(&self, files: &mut dyn Files, spec: &mut OpenSpec) -> Opened {
        let mut s = self.lock();
        if !s.init_file(files, spec) {
            return Opened::Failed;
        }
        match s.load_iteration_list() {
            Some(set) => {
                *self.held.lock().unwrap_or_else(PoisonError::into_inner) = set;
                Opened::Ready
            }
            None => {
                s.close();
                Opened::NoVersions
            }
        }
    }

    /// As [`Store::load_data`], under the lock.
    // D2-05106
    pub fn load_data(&self, id: u32) -> Option<Vec<u8>> {
        self.lock().load_data(id)
    }

    /// As [`Store::save_data`], under the lock.
    // D2-05107
    pub fn save_data(&self, id: u32, data: &[u8]) -> bool {
        self.lock().save_data(id, data)
    }

    /// As [`Store::delete_data`], under the lock.
    // D2-05108
    pub fn delete_data(&self, id: u32) -> bool {
        self.lock().delete_data(id)
    }

    /// As [`Store::is_member`], under the lock.
    // D2-05109
    pub fn is_member(&self, id: u32) -> Option<ObjectInfo> {
        self.lock().is_member(id)
    }

    /// As [`Store::delete_data_by_mask`], under the lock.
    // D2-05110
    pub fn delete_data_by_mask(&self, id: u32, mask: u32) -> bool {
        self.lock().delete_data_by_mask(id, mask)
    }

    /// Store what a world's overlay holds, as a download stores what it brings: its deletions, then
    /// its records, then its revisions among the file's data versions, or in their place when
    /// they are the world's own list. A record's bytes are the overlay's, so every read after this
    /// finds them. Whether all of it was stored.
    pub fn take_overlay(&self, layer: &Layer) -> bool {
        for t in layer.tombstones() {
            if t.mask == 0 {
                self.delete_data(t.id);
            } else {
                self.delete_data_by_mask(t.id, t.mask);
            }
        }
        for (id, _) in layer.records() {
            let Ok(bytes) = layer.read(id) else {
                return false;
            };
            if !self.save_data(id.raw(), &bytes) {
                return false;
            }
        }
        let exact = layer.manifest().exact_iterations;
        if !exact && layer.own_iterations().is_empty() {
            return true;
        }
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if exact {
            *held = VersionSet::new();
        }
        for v in layer.own_iterations() {
            held.add(v.cast_signed());
        }
        held.sort();
        self.lock().save_iteration_list(&mut held)
    }

    /// Whether the file is open.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.lock().initialized
    }

    /// The file's id in the data download.
    #[must_use]
    pub fn dat_file_id(&self) -> u64 {
        self.lock().get_dat_file_id()
    }
}

/// Which of the client's four files is which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Portal = 0,
    Local = 1,
    Cell = 2,
    HighRes = 3,
}

/// The data files the client has open: the four by kind, and every one by its id.
#[derive(Debug, Default)]
pub struct StoreSet {
    pub by_slot: [Option<Arc<SharedStore>>; 4],
    pub by_id: HashMap<u64, Arc<SharedStore>>,
}

impl StoreSet {
    /// The file a record is in: the one named by `dat_file` when it is open; else by the record's
    /// type (its id's when it has none): a portal record from the high-resolution pictures when
    /// that file holds it, else from the portal; a cell record from the cell file; a local one
    /// from the language's file; any other, none.
    // D2-05158
    #[must_use]
    pub fn get_disk_controller(&self, q: QualifiedId, dat_file: u64) -> Option<Arc<SharedStore>> {
        if dat_file != 0 {
            if let Some(s) = self.by_id.get(&dat_file) {
                return Some(s.clone());
            }
        }
        let kind = if q.kind == 0 {
            type_of_id(q.id).unwrap_or(RecordType(0))
        } else {
            RecordType(q.kind)
        };
        let slot = |s: Slot| self.by_slot[s as usize].clone();
        if kind.is_portal_type() {
            if let Some(hi) = slot(Slot::HighRes) {
                if hi.is_member(q.id).is_some() {
                    return Some(hi);
                }
            }
            return slot(Slot::Portal);
        }
        if kind.is_cell_type() {
            return slot(Slot::Cell);
        }
        if kind.is_local_type() {
            return slot(Slot::Local);
        }
        None
    }

    /// Whether the file with this id is open.
    // D2-05159
    #[must_use]
    pub fn is_dat_file_loaded(&self, dat_file: u64) -> bool {
        self.by_id
            .get(&dat_file)
            .is_some_and(|s| s.is_initialized())
    }

    /// Whether a record is in the file it belongs to.
    // D2-12767
    #[must_use]
    pub fn is_on_disk(&self, q: QualifiedId) -> bool {
        self.get_disk_controller(q, 0)
            .is_some_and(|s| s.is_member(q.id).is_some())
    }

    /// Whether every one of the records is on disk.
    // D2-12790
    #[must_use]
    pub fn are_on_disk(&self, qs: &[QualifiedId]) -> bool {
        qs.iter().all(|q| self.is_on_disk(*q))
    }

    /// A record's bytes from the file it belongs to.
    // D2-12768
    #[must_use]
    pub fn load_data(&self, q: QualifiedId) -> Option<Vec<u8>> {
        self.get_disk_controller(q, 0)?.load_data(q.id)
    }

    /// Store a record in the file it belongs to; whether it was stored.
    // D2-12770, D2-12771
    #[must_use]
    pub fn save_data(&self, q: QualifiedId, data: &[u8]) -> bool {
        self.get_disk_controller(q, 0)
            .is_some_and(|s| s.save_data(q.id, data))
    }

    /// Remove a record from the file it belongs to; whether it went.
    // D2-12769, D2-12772
    #[must_use]
    pub fn purge_data(&self, q: QualifiedId) -> bool {
        self.get_disk_controller(q, 0)
            .is_some_and(|s| s.delete_data(q.id))
    }

    /// A save request carried out: the record stored in the file `dat_file` names (or the one its
    /// type belongs to); whether it was stored.
    // D2-12822
    #[must_use]
    pub fn worker_execute_save_request(&self, q: QualifiedId, dat_file: u64, data: &[u8]) -> bool {
        self.get_disk_controller(q, dat_file)
            .is_some_and(|s| s.save_data(q.id, data))
    }

    /// A purge request carried out: whether the record went.
    // D2-12823
    #[must_use]
    pub fn worker_execute_purge_request(&self, q: QualifiedId) -> bool {
        self.purge_data(q)
    }
}

/// The one file a store over an [`Overlay`] opens.
#[derive(Debug)]
struct OverlayFiles(Option<Overlay>);

impl Files for OverlayFiles {
    fn open(&mut self, _name: &str, mode: OpenMode) -> Option<Box<dyn dereth_dat::DatStorageMut>> {
        match mode {
            OpenMode::Existing { .. } => self.0.take().map(|o| Box::new(o) as _),
            OpenMode::Create => None,
        }
    }
}

impl StoreSet {
    /// Stores over files only read, each written in memory ([`Overlay`]): the portal, the
    /// high-resolution pictures (subset `HiFi`), cell and local files put in their slots by their
    /// headers' data sets, every one by its id. A file whose list of data versions does not read
    /// is left out. Records stored are dated by `clock`. A file read through a world's overlay
    /// takes what the overlay holds as it opens ([`SharedStore::take_overlay`]): the stores read
    /// the file's bytes, so they would never see it otherwise.
    #[must_use]
    pub fn over_files(files: &[Arc<DatFile>], clock: &Arc<dyn Fn() -> u32 + Send + Sync>) -> Self {
        let mut set = Self::default();
        for f in files {
            let h = f.header();
            let slot = match (h.data_set, h.data_subset) {
                (1, crate::container::HIRES_SUBSET) => Slot::HighRes,
                (1, _) => Slot::Portal,
                (2, _) => Slot::Cell,
                (3, _) => Slot::Local,
                _ => continue,
            };
            let c = clock.clone();
            let s = SharedStore::new(Box::new(move || c()));
            let mut spec = OpenSpec::new(
                "",
                "",
                h.data_set,
                h.data_subset,
                OPEN_EXPANDABLE,
                0,
                0,
                -1,
                -1,
                false,
            );
            let mut one = OverlayFiles(Some(Overlay::new(f.clone())));
            if s.init_file(&mut one, &mut spec) != Opened::Ready {
                continue;
            }
            if f.shared().layer().is_some_and(|l| !s.take_overlay(l)) {
                continue;
            }
            let s = Arc::new(s);
            set.by_id.insert(s.dat_file_id(), s.clone());
            if set.by_slot[slot as usize].is_none() {
                set.by_slot[slot as usize] = Some(s);
            }
        }
        set
    }
}
