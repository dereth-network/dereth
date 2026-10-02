//! The classic interface's data: the art and creation tables of a pre-Throne-of-Destiny
//! portal.dat, read through the shared dat reader, and the font atlas format its text is drawn
//! from.
//!
//! **Depends on** `dereth-dat` (the container) and `dereth-primitives` (the 1252 code page and
//! data ids). **Used by** the classic interface, which reads the player's own portal.dat at run
//! time rather than shipping any of its contents, and by the hosts that draw its fonts.
//!
//! **Must never** write into the portal it reads.
//!
//! - [`ClassicPortal`]: the portal: every id and every record's bytes.
//! - [`image`]: the 24-bit RGB records (`0x06`) as RGBA.
//! - [`appearance`]: indexed textures (`0x05`), palettes (`0x04`), palette sets (`0x0F`) and the
//!   face texture named by an object description.
//! - [`creation`]: the character-creation tables: heritages, starting areas, skills and help text.
//! - [`fonts`]: the font atlas the classic text is drawn from, and the source a host draws it with.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use dereth_dat::{DatFile, RetailDatStore};
use dereth_primitives::DataId;

pub mod appearance;
pub mod creation;
pub mod fonts;
pub mod image;
mod reader;

/// A pre-Throne-of-Destiny portal.dat, opened read-only.
#[derive(Clone)]
pub struct ClassicPortal {
    source: Source,
}

#[derive(Clone)]
enum Source {
    File(Arc<DatFile>),
    Store(RetailDatStore),
}

impl fmt::Debug for ClassicPortal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClassicPortal")
            .field("iteration", &self.iteration())
            .field("records", &self.portal().len())
            .finish_non_exhaustive()
    }
}

impl ClassicPortal {
    /// Open the file at `path`.
    ///
    /// # Errors
    ///
    /// The file cannot be read, or is not a pre-Throne-of-Destiny portal.
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_file(file).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The same, over a whole file already in memory.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        let file = DatFile::from_storage("portal.dat".into(), Box::new(bytes))
            .map_err(|e| e.to_string())?;
        Self::from_file(file)
    }

    /// The portal of an opened dat set: its own portal when the set is from before Throne of
    /// Destiny, else the older portal attached beside it for presentation. `None` when the set
    /// has neither.
    #[must_use]
    pub fn of_store(store: &RetailDatStore) -> Option<Self> {
        let older = if store.era() == dereth_dat::ContainerEra::PreTod {
            Some(store.clone())
        } else {
            store.legacy_files()
        }?;
        Some(Self {
            source: Source::Store(older),
        })
    }

    fn from_file(file: DatFile) -> Result<Self, String> {
        if file.era() != dereth_dat::ContainerEra::PreTod {
            return Err("not a portal from before Throne of Destiny".into());
        }
        Ok(Self {
            source: Source::File(Arc::new(file)),
        })
    }

    fn portal(&self) -> &DatFile {
        match &self.source {
            Source::File(file) => file,
            Source::Store(store) => store.portal(),
        }
    }

    /// The older dat set this portal belongs to, when it came from one.
    #[must_use]
    pub fn store(&self) -> Option<&RetailDatStore> {
        match &self.source {
            Source::Store(store) => Some(store),
            Source::File(_) => None,
        }
    }

    /// Every record id, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<u32> {
        self.portal().iter_ids().map(|id| id.0).collect()
    }

    /// The record `id`'s bytes, `None` when the portal has no such record.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<Vec<u8>> {
        self.portal().read(DataId(id)).ok()
    }

    /// The portal's iteration, from its header.
    #[must_use]
    pub fn iteration(&self) -> u32 {
        self.portal().header_iteration().unwrap_or(0)
    }

    /// A record every decoder needs whole: missing is an error naming it.
    pub(crate) fn require(&self, id: u32) -> Result<Vec<u8>, String> {
        self.get(id)
            .ok_or_else(|| format!("record {id:08X} is not in the portal"))
    }
}
