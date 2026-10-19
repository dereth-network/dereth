//! A set of data files on this machine, with or without an overlay over them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::overlay::OverlayDir;
use dereth_dat::{ContainerEra, DatFile, ModernDat};
use dereth_primitives::DataId;

use crate::error::Error;

/// The later set of files in a folder: the portal and cell files, and the language and high-res
/// files when they are there.
#[derive(Debug)]
pub struct DatSet {
    pub dir: PathBuf,
    pub portal: DatFile,
    pub cell: DatFile,
    pub local: Option<DatFile>,
    pub highres: Option<DatFile>,
}

fn open(dir: &Path, t: ModernDat) -> Result<Option<DatFile>, Error> {
    let path = t.in_dir(dir);
    if !path.is_file() {
        return Ok(None);
    }
    DatFile::open(&path)
        .map(Some)
        .map_err(|e| Error::Dat(format!("{}: {e}", path.display())))
}

impl DatSet {
    /// The files in `dir`.
    ///
    /// # Errors
    /// No portal or cell file there, a file that will not open, or files from before Throne of
    /// Destiny (a world builder writes the later layout only).
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let need = |t: ModernDat| -> Result<DatFile, Error> {
            open(dir, t)?.ok_or_else(|| {
                if dereth_dat::holds_classic_dats(dir) {
                    Error::Refused(format!(
                        "{} holds the files from before Throne of Destiny, which this tool does \
                         not write",
                        dir.display()
                    ))
                } else {
                    Error::Dat(format!("{} has no {}", dir.display(), t.file_name()))
                }
            })
        };
        let portal = need(ModernDat::Portal)?;
        let cell = need(ModernDat::Cell)?;
        if portal.era() != ContainerEra::Modern || cell.era() != ContainerEra::Modern {
            return Err(Error::Refused(format!(
                "{}: the files are in the layout from before Throne of Destiny",
                dir.display()
            )));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            portal,
            cell,
            local: open(dir, ModernDat::Local)?,
            highres: open(dir, ModernDat::HighRes)?,
        })
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
