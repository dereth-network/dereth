// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/DatDatabase.cs, Source/ACE.DatLoader/DatDatabaseType.cs
//! One open dat file: ACE's `DatDatabase`, its typed `ReadFromDat<T>` and its `FileCache`.
//!
//! The bytes come from a [`DatSource`] (the retail files, or [`crate::FakeDats`] in tests); the
//! decoding is the shared `dereth-assets` decoder for every type it covers, plus the few server-side
//! decoders in [`crate::file_types`]. A decoded object is cached behind a `OnceLock` per file id,
//! so a file is decoded at most once per process even when several threads ask for it at once.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

use dereth_assets::{self as assets, Decode};
use dereth_primitives::DataId;

use crate::source::DatSource;

/// ACE's `DatDatabaseType`: which of the four files a database is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum DatDatabaseType {
    /// `client_portal.dat` (ACE gives `client_highres.dat` the same value on disk).
    Portal = 1,
    /// `client_cell_1.dat`.
    Cell = 2,
    /// `client_local_English.dat`.
    Language = 3,
    /// `client_highres.dat`.
    HighRes = 4,
}

impl DatDatabaseType {
    /// The retail file name ACE's `DatManager.Initialize` opens for this database.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Portal => "client_portal.dat",
            Self::Cell => "client_cell_1.dat",
            Self::Language => "client_local_English.dat",
            Self::HighRes => "client_highres.dat",
        }
    }
}

/// A decoded dat object shared out of the cache.
pub type CachedObject = Arc<dyn Any + Send + Sync>;

/// Why a file could not be turned into a `T` (logged; `ReadFromDat` itself answers `None`).
#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("{0}")]
    Shared(#[from] assets::AssetError),
    #[error("{0}")]
    Dat(#[from] dereth_dat::DatError),
    #[error("{0}")]
    Server(String),
}

/// A type `ReadFromDat<T>` can produce: ACE's `FileType` with its `Unpack`.
pub trait DatFileType: Any + Send + Sync + Sized {
    /// Decode one file's full payload. `file_id` is the id it was read under.
    ///
    /// # Errors
    ///
    /// The payload does not decode as this type.
    fn unpack(file_id: u32, bytes: &[u8]) -> Result<Self, UnpackError>;
}

/// Every shared decoder is a `DatFileType` through its `Decode::decode_payload`, which also checks
/// that the payload ends exactly and echoes the id it was read under.
macro_rules! shared_file_types {
    ($($t:ty),* $(,)?) => {
        $(impl DatFileType for $t {
            fn unpack(file_id: u32, bytes: &[u8]) -> Result<Self, UnpackError> {
                Ok(<$t as Decode>::decode_payload(DataId(file_id), bytes)?)
            }
        })*
    };
}

shared_file_types!(
    assets::ActionMap,
    assets::Animation,
    assets::Attribute2ndTable,
    assets::BadData,
    assets::CellLandblock,
    assets::CharGen,
    assets::ChatPoseTable,
    assets::ClothingTable,
    assets::CombatManeuverTable,
    assets::ContractTable,
    assets::DidMapper,
    assets::DualDidMapper,
    assets::EnumMapper,
    assets::EnvCell,
    assets::Environment,
    assets::Font,
    assets::GfxObj,
    assets::GfxObjDegradeInfo,
    assets::LandblockInfo,
    assets::LanguageInfo,
    assets::LanguageString,
    assets::MasterInputMap,
    assets::MasterProperty,
    assets::MotionTable,
    assets::NameFilterTable,
    assets::ObjectHierarchy,
    assets::Palette,
    assets::PaletteSet,
    assets::ParticleEmitterInfo,
    assets::PhysicsScript,
    assets::PhysicsScriptTable,
    assets::QualityFilter,
    assets::Region,
    assets::RenderSurface,
    assets::RenderTexture,
    assets::Scene,
    assets::Setup,
    assets::SkillTable,
    assets::SoundTable,
    assets::SpellComponentTable,
    assets::SpellTable,
    assets::StringTable,
    assets::Surface,
    assets::SurfaceTexture,
    assets::TabooTable,
    assets::Wave,
    assets::XpTable,
);

type Slot = Arc<OnceLock<Option<CachedObject>>>;

/// ACE's `DatDatabase`: one dat file, its file list and its decoded-object cache.
pub struct DatDatabase {
    database_type: DatDatabaseType,
    source: Arc<dyn DatSource>,
    /// ACE's `FileCache`. The map only hands out slots; the decode runs outside its lock, once per
    /// id, inside the slot's `OnceLock`.
    file_cache: Mutex<HashMap<u32, Slot>>,
}

impl fmt::Debug for DatDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatDatabase")
            .field("type", &self.database_type)
            .field("source", &self.source)
            .field("cached", &self.file_cache_count())
            .finish()
    }
}

impl DatDatabase {
    /// ACE opens the file and walks its directory here; the [`DatSource`] has already done that.
    // ACE: DatDatabase.DatDatabase
    #[must_use]
    pub fn new(database_type: DatDatabaseType, source: Arc<dyn DatSource>) -> Self {
        Self {
            database_type,
            source,
            file_cache: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn database_type(&self) -> DatDatabaseType {
        self.database_type
    }

    /// Where the file lives, for log lines (ACE's `FilePath`).
    #[must_use]
    pub fn file_path(&self) -> String {
        self.source.describe(self.database_type)
    }

    /// ACE's `AllFiles.Keys`, ascending.
    #[must_use]
    pub fn all_files(&self) -> Vec<u32> {
        self.source.file_ids(self.database_type)
    }

    /// ACE's `AllFiles.Count`.
    #[must_use]
    pub fn all_files_count(&self) -> usize {
        self.source.file_count(self.database_type)
    }

    /// ACE's `AllFiles.ContainsKey` / `TryGetValue`.
    #[must_use]
    pub fn contains_file(&self, file_id: u32) -> bool {
        self.source.contains(self.database_type, file_id)
    }

    /// ACE's `AllFiles[file_id].Iteration`: the iteration that introduced the file, or `None` when
    /// the database has no such file.
    #[must_use]
    pub fn file_iteration(&self, file_id: u32) -> Option<u32> {
        self.source.file_iteration(self.database_type, file_id)
    }

    /// ACE's `FileCache.Count`: how many ids have been asked for.
    #[must_use]
    pub fn file_cache_count(&self) -> usize {
        self.file_cache.lock().map_or(0, |c| c.len())
    }

    /// The cached object for `file_id`, reading and decoding it on first use.
    ///
    /// Where ACE hands back `new T()` for a file that is absent (and caches that empty object),
    /// this answers `None` (and caches that answer); a call site ports ACE's "empty object" arm
    /// as its `None` arm. A payload that does not decode, or an id first read as another type, is
    /// logged and also answers `None` where ACE would throw.
    // ACE: DatDatabase.ReadFromDat
    pub fn read_from_dat<T: DatFileType>(&self, file_id: u32) -> Option<Arc<T>> {
        let slot = self.slot(file_id);
        let obj = slot.get_or_init(|| self.load::<T>(file_id)).clone()?;
        match obj.downcast::<T>() {
            Ok(t) => Some(t),
            Err(_) => {
                log::error!(
                    "{:?} dat: 0x{file_id:08X} is cached as another type than {}",
                    self.database_type,
                    std::any::type_name::<T>()
                );
                None
            }
        }
    }

    /// Decode `file_id` without touching the cache, for a loader that edits the object before
    /// caching it (ACE's `PortalDat.SkillTable.AddRetiredSkills()` edits the cached instance).
    pub(crate) fn unpack_uncached<T: DatFileType + Clone>(&self, file_id: u32) -> Option<T> {
        let obj = self.load::<T>(file_id)?;
        obj.downcast::<T>().ok().map(|t| (*t).clone())
    }

    /// Put an object in the cache under `file_id`, unless the id was already read.
    pub(crate) fn insert_cache<T: DatFileType>(&self, file_id: u32, obj: Arc<T>) {
        let _ = self.slot(file_id).set(Some(obj as CachedObject));
    }

    fn slot(&self, file_id: u32) -> Slot {
        let mut cache = self
            .file_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(cache.entry(file_id).or_default())
    }

    fn load<T: DatFileType>(&self, file_id: u32) -> Option<CachedObject> {
        if let Some(obj) = self.source.decoded(self.database_type, file_id) {
            return Some(obj);
        }
        let bytes = self.get_reader_for_file(file_id)?;
        match T::unpack(file_id, &bytes) {
            Ok(t) => Some(Arc::new(t) as CachedObject),
            Err(e) => {
                log::error!(
                    "{:?} dat: 0x{file_id:08X} does not decode as {}: {e}",
                    self.database_type,
                    std::any::type_name::<T>()
                );
                None
            }
        }
    }

    /// The raw payload of `file_id` (ACE's `DatReader.Buffer`), or `None` when the file has no such
    /// id. A missing `0xXXYYFFFE` is normal (not every landblock has a `LandblockInfo`) and is not
    /// logged.
    // ACE: DatDatabase.GetReaderForFile
    #[must_use]
    pub fn get_reader_for_file(&self, file_id: u32) -> Option<Vec<u8>> {
        if let Some(bytes) = self.source.read(self.database_type, file_id) {
            return Some(bytes);
        }
        if file_id & 0xFFFF != 0xFFFE {
            log::info!(
                "Unable to find object_id {file_id:08X} in {:?}",
                self.database_type
            );
        }
        None
    }

    /// The dat's total iteration (its version), 0 when it cannot be read.
    // ACE: DatDatabase.Iteration
    #[must_use]
    pub fn iteration(&self) -> i32 {
        self.get_total_iterations()
    }

    // ACE: DatDatabase.GetTotalIterations
    fn get_total_iterations(&self) -> i32 {
        let iteration = self
            .read_from_dat::<crate::file_types::Iteration>(crate::file_types::Iteration::FILE_ID);
        match iteration {
            Some(it) if it.total_iterations > 0 => {
                if it.ints.len() > 1 {
                    log::error!("{} contains an incomplete dat file!", self.file_path());
                }
                it.total_iterations
            }
            _ => {
                log::error!("Unable to read iteration from {}", self.file_path());
                0
            }
        }
    }

    /// ACE's `ExtractCategorizedPortalContents` writes every file to disk for the developer
    /// `export-portal-dat` commands; the command units port it with its caller.
    // ACE: DatDatabase.ExtractCategorizedPortalContents
    pub fn extract_categorized_portal_contents(&self, _path: &str) {
        empyrean_common::not_ported!("ACE: DatDatabase.ExtractCategorizedPortalContents");
    }
}
