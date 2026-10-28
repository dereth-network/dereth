//! A set of data files on this machine, with or without an overlay over them.
//!
//! A set is the files of one era: the end of retail's (`client_portal.dat`, `client_cell_1.dat`
//! and the language and high-res files beside them) or Infiltration's (`portal.dat` and
//! `cell.dat`, from February 2005, before Throne of Destiny). The era goes with the set
//! everywhere: every record is read and written in the layout of its era's files, and a world is
//! built over the files of the era it names, and over no other.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_assets::{AssetError, Decode};
use dereth_dat::overlay::OverlayDir;
use dereth_dat::{ContainerEra, DatFile, FileSet, ModernDat};
use dereth_primitives::{DataId, EraId};

use crate::error::Error;

/// One era's files in a folder: the portal and cell files, and the language and high-res files
/// when the era has them and they are there.
#[derive(Debug)]
pub struct DatSet {
    pub dir: PathBuf,
    /// The era whose files these are, which every record of them is read and written as.
    pub era: EraId,
    pub portal: DatFile,
    pub cell: DatFile,
    pub local: Option<DatFile>,
    pub highres: Option<DatFile>,
}

fn open_file(path: &Path) -> Result<Option<DatFile>, Error> {
    if !path.is_file() {
        return Ok(None);
    }
    DatFile::open(path)
        .map(Some)
        .map_err(|e| Error::Dat(format!("{}: {e}", path.display())))
}

/// The era a set of files in the `layout` is taken for when nothing names one: Infiltration's
/// for the files from before Throne of Destiny, the end of retail's otherwise.
#[must_use]
pub const fn era_of_layout(layout: ContainerEra) -> EraId {
    match layout {
        ContainerEra::Classic => EraId::Infiltration,
        ContainerEra::Modern => EraId::Eor,
    }
}

/// An era's files as a sentence names them.
#[must_use]
pub fn files_of(era: EraId) -> String {
    let set = file_set(era);
    let name = |t| set.name(t).unwrap_or_default();
    match era.container_era() {
        ContainerEra::Classic => format!(
            "the {} files ({} and {})",
            era_name(era),
            name(ModernDat::Portal),
            name(ModernDat::Cell)
        ),
        ContainerEra::Modern => {
            format!("the {} files ({})", era_name(era), name(ModernDat::Portal))
        }
    }
}

/// The files of `era`: their names on disk and their layout.
#[must_use]
pub const fn file_set(era: EraId) -> FileSet {
    FileSet::of_era(era.container_era())
}

/// An era's name as a sentence says it.
#[must_use]
pub const fn era_name(era: EraId) -> &'static str {
    match era {
        EraId::Infiltration => "Infiltration",
        EraId::Eor => "end-of-retail",
    }
}

/// Whether `dir` holds the files of `era`: its portal and cell files.
#[must_use]
pub fn holds(dir: &Path, era: EraId) -> bool {
    file_set(era).holds(dir)
}

impl DatSet {
    /// The files in `dir`: the end of retail's when they are there, else Infiltration's.
    ///
    /// # Errors
    /// Neither set there, or a file that will not open or is not in its era's layout.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        if !holds(dir, EraId::Eor) && holds(dir, EraId::Infiltration) {
            return Self::open_era(dir, EraId::Infiltration);
        }
        Self::open_era(dir, EraId::Eor)
    }

    /// The files of `era` in `dir`.
    ///
    /// # Errors
    /// The era's portal or cell file not there (with what is there instead, when the folder
    /// holds another era's), a file that will not open, or one in another era's layout.
    pub fn open_era(dir: &Path, era: EraId) -> Result<Self, Error> {
        let layout = era.container_era();
        if !holds(dir, era) {
            let other = EraId::ALL
                .into_iter()
                .find(|e| e.container_era() != layout && holds(dir, *e));
            return Err(match other {
                Some(other) => Error::Refused(format!(
                    "{} holds {}, not {}",
                    dir.display(),
                    files_of(other),
                    files_of(era)
                )),
                None => Error::Dat(format!("{} does not hold {}", dir.display(), files_of(era))),
            });
        }
        let need = |path: PathBuf| -> Result<DatFile, Error> {
            let f = open_file(&path)?
                .ok_or_else(|| Error::Dat(format!("{} is not there", path.display())))?;
            if f.era() != layout {
                return Err(Error::Refused(format!(
                    "{} is not in the layout of {}",
                    path.display(),
                    files_of(era)
                )));
            }
            Ok(f)
        };
        let set = file_set(era);
        let path = |t| set.in_dir(t, dir);
        Ok(Self {
            dir: dir.to_path_buf(),
            era,
            portal: need(path(ModernDat::Portal).unwrap_or_default())?,
            cell: need(path(ModernDat::Cell).unwrap_or_default())?,
            local: path(ModernDat::Local).map_or(Ok(None), |p| open_file(&p))?,
            highres: path(ModernDat::HighRes).map_or(Ok(None), |p| open_file(&p))?,
        })
    }

    /// The files a world built on `era` is built over, in `dir`.
    ///
    /// # Errors
    /// As [`Self::open_era`]: the folder holds another era's files, say, in words for a person.
    pub fn for_world(dir: &Path, era: EraId) -> Result<Self, Error> {
        Self::open_era(dir, era).map_err(|e| match e {
            Error::Refused(why) => Error::Refused(format!(
                "this world is built on {}, and {why}",
                files_of(era)
            )),
            other => other,
        })
    }

    /// The layout of the set's files.
    #[must_use]
    pub fn layout(&self) -> ContainerEra {
        self.era.container_era()
    }

    /// The files' set: their names on disk and their layout.
    #[must_use]
    pub const fn file_set(&self) -> FileSet {
        file_set(self.era)
    }

    /// The name on disk of the file `t` stands for: `portal.dat` and `cell.dat` for the files
    /// from before Throne of Destiny (whose portal file holds what the later language and
    /// high-res files hold), the later names otherwise. An overlay names its base by it.
    #[must_use]
    pub const fn base_name(&self, t: ModernDat) -> &'static str {
        self.file_set().holder_name(t)
    }

    /// The path of the file `t` stands for.
    #[must_use]
    pub fn path_of(&self, t: ModernDat) -> PathBuf {
        self.dir.join(self.base_name(t))
    }

    /// The record `id`'s bytes read as a `T` in the layout of the set's era.
    ///
    /// # Errors
    /// The bytes do not read as a `T` of the era.
    pub fn decode<T: Decode>(&self, id: DataId, bytes: &[u8]) -> Result<T, AssetError> {
        T::decode_payload_for(self.era, id, bytes)
    }

    /// The file `t`, when the set has it.
    #[must_use]
    pub fn file(&self, t: ModernDat) -> Option<&DatFile> {
        match t {
            ModernDat::Portal => Some(&self.portal),
            ModernDat::Cell => Some(&self.cell),
            ModernDat::Local => self.local.as_ref(),
            ModernDat::HighRes => self.highres.as_ref(),
        }
    }

    /// The files the set has, in the order the client opens them.
    pub fn files(&self) -> impl Iterator<Item = (ModernDat, &DatFile)> + '_ {
        ModernDat::ALL
            .into_iter()
            .filter_map(|t| self.file(t).map(|f| (t, f)))
    }

    /// Which file of the portal's two holds the portal-file record `id`.
    #[must_use]
    pub fn portal_holder(&self, id: DataId) -> Option<ModernDat> {
        if self.portal.contains(id) {
            Some(ModernDat::Portal)
        } else if self.highres.as_ref().is_some_and(|h| h.contains(id)) {
            Some(ModernDat::HighRes)
        } else {
            None
        }
    }

    /// Whether the portal file or the high-res file holds `id`.
    #[must_use]
    pub fn has_portal(&self, id: DataId) -> bool {
        self.portal_holder(id).is_some()
    }

    /// The portal-file record `id`, from whichever of the two holds it.
    #[must_use]
    pub fn read_portal(&self, id: DataId) -> Option<Vec<u8>> {
        let f = self.file(self.portal_holder(id)?)?;
        f.read(id).ok()
    }

    /// The same files with the overlay in `overlay` over them. `world_key`, when given, is the
    /// world the overlay must belong to.
    ///
    /// # Errors
    /// An overlay made against other files or for another world, or one that will not open.
    pub fn with_overlay(&self, overlay: &Path, world_key: Option<&str>) -> Result<Self, Error> {
        let dir = OverlayDir::new(overlay)?;
        let lay = |t: ModernDat, f: &DatFile| -> Result<DatFile, Error> {
            Ok(match dir.layer_over(t, f, world_key)? {
                Some(layer) => f.layered(Arc::new(layer)),
                None => f.base(),
            })
        };
        Ok(Self {
            dir: self.dir.clone(),
            era: self.era,
            portal: lay(ModernDat::Portal, &self.portal)?,
            cell: lay(ModernDat::Cell, &self.cell)?,
            local: self
                .local
                .as_ref()
                .map(|f| lay(ModernDat::Local, f))
                .transpose()?,
            highres: self
                .highres
                .as_ref()
                .map(|f| lay(ModernDat::HighRes, f))
                .transpose()?,
        })
    }
}
