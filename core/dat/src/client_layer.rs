//! The client's own records: the art and other records Dereth ships for itself, read through the
//! retail files as if those files held them.
//!
//! A client layer is a container in the overlay's format ([`crate::overlay`]): an ordinary dat
//! container in the later layout whose manifest (`0xFFFF0003`) names [`CLIENT_KEY`] as its world
//! and the file it lies over by name (`portal.dat` for the files from before Throne of Destiny,
//! `client_portal.dat` for the later ones). It holds records in that file's own record layout,
//! and nothing else: no deletions, no iteration list, no base fingerprint, so it lies over any
//! file of that kind.
//!
//! A read through a file carrying it ([`DatFile::with_client_layer`]) asks the world's overlay
//! first, then the client layer, then the file itself, so a world's own record under the same id
//! wins over the client's. The client layer is in none of the file's iterations, and
//! [`DatFile::base`] leaves it out, so the data-patch path, which reports iterations and writes
//! against base files, never sees it. Only the client lays one; a server opens its files without.
//!
//! [`write`] makes one, the same bytes for the same records.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use dereth_primitives::DataId;

use crate::container::{ContainerEra, DatFile, PORTAL_DATFILE};
use crate::error::DatError;
use crate::overlay::{is_reserved, record_hash, ContainerManifest, Layer, OverlayError, MANIFEST};
use crate::write::{DatWriter, ROOM_RESERVE};
use crate::{ClassicDat, ModernDat};

/// The world a client layer's manifest names: no world's name, so no world's overlay is taken
/// for one.
pub const CLIENT_KEY: &str = "(the client's own records)";

/// The block size a client layer is written with: small, since its records are small.
const BLOCK: u32 = 0x100;

/// A client layer, opened.
#[derive(Clone, Debug)]
pub struct ClientLayer {
    layer: Arc<Layer>,
    era: ContainerEra,
}

impl ClientLayer {
    /// A client layer built into the program, `name` naming it in errors.
    ///
    /// # Errors
    /// As [`Self::from_file`].
    pub fn from_static(name: &str, bytes: &'static [u8]) -> Result<Self, OverlayError> {
        Self::from_file(DatFile::from_storage(name.into(), Box::new(bytes))?)
    }

    /// The client layer in `file`.
    ///
    /// # Errors
    /// [`OverlayError::NotOverlay`] for a container whose manifest is missing or is not a client
    /// layer's, or that holds anything but records (deletions, iterations).
    pub fn from_file(file: DatFile) -> Result<Self, OverlayError> {
        let path = file.path().to_path_buf();
        let not = || OverlayError::NotOverlay(path.clone());
        let manifest = file
            .read(MANIFEST)
            .and_then(|b| ContainerManifest::decode(&b))
            .map_err(|_| not())?;
        if manifest.world_key != CLIENT_KEY
            || file
                .base_entries()
                .any(|(id, _)| is_reserved(id) && id != MANIFEST)
        {
            return Err(not());
        }
        let era = era_of_base(&manifest.base_name).ok_or_else(not)?;
        Ok(Self {
            layer: Arc::new(Layer::client(file, manifest)),
            era,
        })
    }

    /// The layout of the files it lies over.
    #[must_use]
    pub fn era(&self) -> ContainerEra {
        self.era
    }

    /// The name of the file it lies over (`portal.dat`, `client_portal.dat`).
    #[must_use]
    pub fn base_name(&self) -> &str {
        &self.layer.manifest().base_name
    }

    /// Every record id it holds, ascending.
    #[must_use]
    pub fn ids(&self) -> Vec<DataId> {
        self.layer.records().map(|(id, _)| id).collect()
    }

    /// Its record `id`.
    ///
    /// # Errors
    /// [`DatError::NotFound`] when it holds none.
    pub fn read(&self, id: DataId) -> Result<Vec<u8>, DatError> {
        self.layer.read(id)
    }

    /// `file` with this layer beneath its overlay.
    #[must_use]
    pub fn over(&self, file: &DatFile) -> DatFile {
        file.with_client_layer(Arc::clone(&self.layer))
    }
}

/// The layout of a base file by its name.
fn era_of_base(name: &str) -> Option<ContainerEra> {
    if ClassicDat::Portal.file_name().eq_ignore_ascii_case(name) {
        Some(ContainerEra::Classic)
    } else if ModernDat::Portal.file_name().eq_ignore_ascii_case(name) {
        Some(ContainerEra::Modern)
    } else {
        None
    }
}

/// Write a client layer to `path` holding `records`, to lie over the portal files of `era`. The
/// same records give the same bytes: they are written in id order with no date, and the
/// container is made exactly as large as they need.
///
/// # Errors
/// [`DatError::NotFound`] for a reserved id, and the writer's errors.
pub fn write(
    path: &Path,
    era: ContainerEra,
    records: &BTreeMap<DataId, Vec<u8>>,
) -> Result<(), OverlayError> {
    if let Some(id) = records.keys().find(|id| is_reserved(**id)) {
        return Err(DatError::NotFound(*id).into());
    }
    let base_name = match era {
        ContainerEra::Classic => ClassicDat::Portal.file_name(),
        ContainerEra::Modern => ModernDat::Portal.file_name(),
    };
    let manifest = ContainerManifest {
        world_key: CLIENT_KEY.to_owned(),
        base_name: base_name.to_owned(),
        base_fingerprint: [0; 32],
        base_iterations: 0,
        exact_iterations: false,
        hashes: records
            .iter()
            .map(|(id, b)| (id.raw(), record_hash(b)))
            .collect(),
    };
    let manifest = manifest.encode();
    // Room for every chain, the directory's nodes as it grows, and the writer's reserve.
    let per = BLOCK - 4;
    let blocks = |len: usize| u32::try_from(len).unwrap_or(u32::MAX).div_ceil(per);
    let node = blocks(crate::btree::NODE_SIZE);
    let count = u32::try_from(records.len() + 1).unwrap_or(u32::MAX);
    let total = records.values().map(|b| blocks(b.len())).sum::<u32>()
        + blocks(manifest.len())
        + node * (1 + 2 * count.div_ceil(30))
        + ROOM_RESERVE
        + 2;
    let mut w = DatWriter::create(path, BLOCK, PORTAL_DATFILE, 0, 0x400 + BLOCK * total)?;
    for (id, bytes) in records {
        w.save(*id, bytes, 1, 0, 0)?;
    }
    w.save(MANIFEST, &manifest, 1, 0, 0)?;
    Ok(())
}
