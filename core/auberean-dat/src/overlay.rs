//! A world's overlay over this game's data files: the records a world adds, replaces or deletes,
//! kept in a folder of their own, so the files a player installed are never written.
//!
//! The overlay is the shared one ([`dereth_dat::overlay`]): the same containers in the same layout,
//! with the same deletions and revisions, whatever the base. What is this game's is which file
//! plays which role ([`role_of`]).
//!
//! A file opened over a folder ([`WorldOverlay::over`]) reads through it, by id and by directory. A
//! file that plays no role (another language's, another region's) is left as it is. The
//! data cache's stores take what the overlay holds when they open
//! ([`crate::disk::StoreSet::over_files`]), so a record reads the same whichever way it is read.
//!
//! A folder made against other files than the ones it lies over, or that holds the install's own
//! files, is refused, never read wrongly.

use std::path::Path;
use std::sync::Arc;

use dereth_dat::overlay::{OverlayDir, OverlayError};
use dereth_dat::{DatFolder, DiskFolder, ModernDat};

use crate::container::{DatFile, CELL_SET, HIRES_SUBSET};

/// A world's overlay folder, within the world's own folder.
pub const WORLD_FOLDER: &str = "overlay";

/// The header's data sets of the portal and language files.
const PORTAL_SET: u32 = 1;
const LOCAL_SET: u32 = 3;
/// The header's subsets of the one region and the one language an overlay lies over.
const FIRST_REGION: u32 = 1;
const ENGLISH: u32 = 0x8000_0001;

/// The role a file plays under a world's overlay, by its header: the portal, the high-resolution
/// pictures, the first region's cell file or the English file.
#[must_use]
pub fn role_of(file: &DatFile) -> Option<ModernDat> {
    let h = file.header();
    match (h.data_set, h.data_subset) {
        (PORTAL_SET, HIRES_SUBSET) => Some(ModernDat::HighRes),
        (PORTAL_SET, _) => Some(ModernDat::Portal),
        (CELL_SET, FIRST_REGION) => Some(ModernDat::Cell),
        (LOCAL_SET, ENGLISH) => Some(ModernDat::Local),
        _ => None,
    }
}

/// A file's layout, as it names the file a world's overlay lies over: always the file's own,
/// whatever overlay is over it, and it costs next to nothing.
#[must_use]
pub fn layout_of(file: &DatFile) -> [u8; 32] {
    dereth_dat::overlay::fingerprint(&file.shared().base())
}

/// The digest of a file's records, whatever their order: always the file's own. It reads the whole
/// file.
///
/// # Errors
/// A record that will not read.
pub fn records_of(file: &DatFile) -> Result<[u8; 32], dereth_dat::DatError> {
    dereth_dat::overlay::records_digest(&file.shared().base())
}

/// One world's overlay folder.
#[derive(Clone, Debug)]
pub struct WorldOverlay {
    dir: OverlayDir,
    world: String,
}

impl WorldOverlay {
    /// The overlay in the folder `dir` on disk.
    ///
    /// # Errors
    /// As [`WorldOverlay::in_folder`].
    pub fn open(dir: &Path) -> Result<Self, OverlayError> {
        Self::in_folder(Arc::new(DiskFolder::new(dir)))
    }

    /// The overlay of the world in the folder `world`; `None` for a world with no [`WORLD_FOLDER`]
    /// or nothing in it, whose records are the install's own.
    ///
    /// # Errors
    /// As [`WorldOverlay::in_folder`]: a folder holding containers that don't read is refused.
    pub fn of_world(world: &Path) -> Result<Option<Self>, OverlayError> {
        let dir = world.join(WORLD_FOLDER);
        if OverlayDir::new(&dir)?.containers().is_empty() {
            return Ok(None);
        }
        Self::open(&dir).map(Some)
    }

    /// The overlay kept in `folder`, wherever that keeps its containers: the world its containers
    /// name.
    ///
    /// # Errors
    /// [`OverlayError::BaseFolder`] for a folder holding an install's files, and
    /// [`OverlayError::NotOverlay`] for one holding no overlay.
    pub fn in_folder(folder: Arc<dyn DatFolder>) -> Result<Self, OverlayError> {
        let path = folder.path().to_path_buf();
        let dir = OverlayDir::in_folder(folder)?;
        let world = dir.world_key().ok_or(OverlayError::NotOverlay(path))?;
        Ok(Self { dir, world })
    }

    /// The world the overlay belongs to.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// `file` read through the overlay: as it is when it plays no role ([`role_of`]) or the folder
    /// holds nothing over it.
    ///
    /// # Errors
    /// [`OverlayError::BaseMismatch`] for a container made against other files,
    /// [`OverlayError::OtherWorld`] for one of another world, and its read errors.
    pub fn over(&self, file: DatFile) -> Result<DatFile, OverlayError> {
        let Some(role) = role_of(&file) else {
            return Ok(file);
        };
        match self
            .dir
            .layer_over(role, file.shared(), Some(&self.world))?
        {
            Some(layer) => Ok(file.layered(Arc::new(layer))),
            None => Ok(file),
        }
    }
}
