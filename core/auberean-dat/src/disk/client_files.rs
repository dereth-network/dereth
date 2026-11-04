//! The data files the client opens: the portal and the high-resolution pictures at start, the
//! language's local file, and the region's cell file, each found by [`FileSearch`] and opened as a
//! [`SharedStore`] in its slot of the [`StoreSet`].
//!
//! | file | data set, subset | opened | when not found |
//! |---|---|---|---|
//! | the portal (its name given) | 1, 0 | may grow; only read when asked | nothing opens |
//! | the high-resolution pictures (named) | 1, `HiFi` | as the portal, once it opened | (its failure is ignored) |
//! | `local_<language>.dat` | 3, the language | may grow; only read when asked | notice `0xCA` |
//! | `cell_<region>.dat` | 2, the region | may grow; only read when asked | made: 1 MB, blocks of 256 |
//!
//! Each is checked against the program's data versions ([`CLIENT_VERSIONS`]); the engine's
//! version is never checked, the game's always but where the spec says only the engine's
//! counts.
//!
//! The desktop client opens its files itself, only for reading, and does not use this: its tests
//! are what run it.

use std::sync::Arc;

use super::controller::{OpenSpec, OPEN_EXPANDABLE, OPEN_READ_ONLY};
use super::device::Files;
use super::lookfile::{FileSearch, LookHost};
use super::shared::{Opened, SharedStore, Slot, StoreSet};

/// The notice shown when a language's file is not found (or has no name).
pub const NO_LANGUAGE_NOTICE: u32 = 0xCA;
/// The high-resolution file's subset, `"HiFi"`.
pub const HIRES_SUBSET: u32 = 0x6946_6948;
/// A new cell file's size and block size.
pub const NEW_CELL_FILE: (u32, u32) = (0x10_0000, 0x100);
/// An open flag the client always sets with the rest, which opening does not read.
const OPEN_ALWAYS_SET: u32 = 1;
/// A file made when it is not found.
const OPEN_CREATE: u32 = super::controller::OPEN_CREATE;

/// The data versions the program asks of its files (negative: any).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackVersions {
    pub eng_data: i32,
    pub eng_cell: i32,
    pub game_data: i32,
    pub game_cell: i32,
}

/// This game's: any engine version, game data version 3, game cell version 0.
pub const CLIENT_VERSIONS: PackVersions = PackVersions {
    eng_data: -1,
    eng_cell: -1,
    game_data: 3,
    game_cell: 0,
};

/// The client's data files and what they were opened with.
pub struct ClientFiles {
    pub set: StoreSet,
    pub read_only: bool,
    pub eng_only: bool,
    pub versions: PackVersions,
    pub region: Option<u32>,
    pub language: Option<u32>,
    clock: Arc<dyn Fn() -> u32 + Send + Sync>,
}

impl core::fmt::Debug for ClientFiles {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClientFiles")
            .field("set", &self.set)
            .field("region", &self.region)
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

impl ClientFiles {
    /// No files yet; records stored are dated by `clock`.
    #[must_use]
    pub fn new(read_only: bool, eng_only: bool, clock: Arc<dyn Fn() -> u32 + Send + Sync>) -> Self {
        Self {
            set: StoreSet::default(),
            read_only,
            eng_only,
            versions: CLIENT_VERSIONS,
            region: None,
            language: None,
            clock,
        }
    }

    fn store(&self) -> SharedStore {
        let c = self.clock.clone();
        SharedStore::new(Box::new(move || c()))
    }

    fn flags(&self) -> u32 {
        OPEN_ALWAYS_SET | OPEN_EXPANDABLE | if self.read_only { OPEN_READ_ONLY } else { 0 }
    }

    fn put(&mut self, slot: Slot, s: Arc<SharedStore>) {
        if let Some(old) = self.set.by_slot[slot as usize].take() {
            self.set.by_id.retain(|_, v| !Arc::ptr_eq(v, &old));
        }
        self.set.by_id.insert(s.dat_file_id(), s.clone());
        self.set.by_slot[slot as usize] = Some(s);
    }

    /// The files part of the cache's start: the portal `name` found and opened, then the
    /// high-resolution pictures (`hires`, the install's name for them) beside it. Whether the
    /// portal opened.
    pub fn open_portal(
        &mut self,
        files: &mut dyn Files,
        look: &dyn LookHost,
        name: &str,
        hires: &str,
    ) -> bool {
        let mut search = FileSearch::new("", "");
        if !search.look_for_file(look, name, "", true) {
            return false;
        }
        let mut spec = OpenSpec::new(
            name,
            &search.get_found_path(),
            1,
            0,
            self.flags(),
            0,
            0,
            self.versions.eng_data,
            self.versions.game_data,
            self.eng_only,
        );
        let portal = Arc::new(self.store());
        if portal.init_file(files, &mut spec) != Opened::Ready {
            return false;
        }
        self.put(Slot::Portal, portal);
        spec.file = hires.into();
        spec.data_subset = HIRES_SUBSET;
        let high = Arc::new(self.store());
        if high.init_file(files, &mut spec) == Opened::Ready {
            self.put(Slot::HighRes, high);
        }
        true
    }

    /// Open the region's cell file, `cell_<region>.dat`, in place of the one open, made anew when
    /// it is not found; nothing to do for the region already open. Whether it is open.
    pub fn set_region(&mut self, files: &mut dyn Files, look: &dyn LookHost, region: u32) -> bool {
        if self.region == Some(region) {
            return true;
        }
        if let Some(old) = self.set.by_slot[Slot::Cell as usize].take() {
            self.set.by_id.retain(|_, v| !Arc::ptr_eq(v, &old));
            old.lock().close();
        }
        let name = format!("cell_{region}.dat");
        let mut search = FileSearch::new("", "");
        let found = search.look_for_file(look, &name, "", true);
        let mut flags = self.flags();
        if !found {
            flags |= OPEN_CREATE;
        }
        let mut spec = OpenSpec::new(
            &name,
            &search.get_found_path(),
            2,
            region,
            flags,
            NEW_CELL_FILE.0,
            NEW_CELL_FILE.1,
            self.versions.eng_cell,
            self.versions.game_cell,
            self.eng_only,
        );
        let cell = Arc::new(self.store());
        if cell.init_file(files, &mut spec) != Opened::Ready {
            return false;
        }
        self.put(Slot::Cell, cell);
        self.region = Some(region);
        true
    }

    /// Open the language's file, `local_<name>.dat` (`name` the language's name, none when the
    /// language has none), in place of the one open.
    ///
    /// # Errors
    /// [`NO_LANGUAGE_NOTICE`] when the language has no name or its file is not found; `0` when it
    /// does not open.
    pub fn set_language_internal(
        &mut self,
        files: &mut dyn Files,
        look: &dyn LookHost,
        language: u32,
        name: Option<&str>,
        eng_only: bool,
    ) -> Result<(), u32> {
        let name = name.ok_or(NO_LANGUAGE_NOTICE)?;
        let file = format!("local_{name}.dat");
        let mut search = FileSearch::new("", "");
        if !search.look_for_file(look, &file, "", true) {
            return Err(NO_LANGUAGE_NOTICE);
        }
        let mut spec = OpenSpec::new(
            &file,
            &search.get_found_path(),
            3,
            language,
            self.flags(),
            0,
            0,
            self.versions.eng_data,
            self.versions.game_data,
            eng_only,
        );
        if let Some(old) = self.set.by_slot[Slot::Local as usize].take() {
            self.set.by_id.retain(|_, v| !Arc::ptr_eq(v, &old));
            old.lock().close();
        }
        let local = Arc::new(self.store());
        if local.init_file(files, &mut spec) != Opened::Ready {
            return Err(0);
        }
        self.put(Slot::Local, local);
        self.language = Some(language);
        Ok(())
    }
}
