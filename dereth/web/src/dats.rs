//! The player's four data files, opened from whatever the platform reads them through.
//!
//! The container walks its whole directory at open and then reads one block chain per record, all
//! through [`DatStorage`]. In a browser that storage is a synchronous read the worker answers from
//! its own storage, which is why the client runs in a worker: the page's main thread may not block.

use std::fmt::Write as _;
use std::path::PathBuf;

use dereth_client_sdk::dat::{DatError, DatFile, DatStorage, RetailDatStore};

/// The four files, in the order a platform numbers them.
pub const FILE_NAMES: [&str; 4] = [
    "client_portal.dat",
    "client_cell_1.dat",
    "client_local_English.dat",
    "client_highres.dat",
];

/// What one opened file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub name: &'static str,
    pub entries: usize,
    pub block_size: u32,
    pub file_size: u32,
    /// The highest iteration in the file's iteration list: how far the file has been patched.
    pub iteration: Option<u32>,
}

impl FileReport {
    /// # Errors
    /// The iteration list's read or decode error.
    pub fn of(name: &'static str, file: &DatFile) -> Result<Self, DatError> {
        let iterations = file.iteration_list()?;
        Ok(Self {
            name,
            entries: file.len(),
            block_size: file.header().block_size,
            file_size: file.header().file_size,
            iteration: iterations.iter().copied().max(),
        })
    }
}

/// One line per file, for the console.
#[must_use]
pub fn describe(reports: &[FileReport]) -> String {
    let mut out = String::new();
    for r in reports {
        let _ = writeln!(
            out,
            "{:<26} iteration {:>5}  {:>7} entries  block {:#06x}  {} bytes",
            r.name,
            r.iteration
                .map_or_else(|| "-".to_string(), |i| i.to_string()),
            r.entries,
            r.block_size,
            r.file_size,
        );
    }
    out
}

/// Open the four files through `source` (called with each file's index in [`FILE_NAMES`]) and
/// report on each. The high-res file is optional, as it is to the client.
///
/// # Errors
/// The first required file that will not open.
pub fn open_store<S, F>(mut source: F) -> Result<(RetailDatStore, Vec<FileReport>), DatError>
where
    S: DatStorage + 'static,
    F: FnMut(usize) -> Option<S>,
{
    let mut open = |index: usize| -> Option<Result<DatFile, DatError>> {
        let s = source(index)?;
        Some(DatFile::from_storage(
            PathBuf::from(FILE_NAMES[index]),
            Box::new(s),
        ))
    };
    let missing = |index: usize| {
        DatError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            FILE_NAMES[index],
        ))
    };
    let portal = open(0).ok_or_else(|| missing(0))??;
    let cell = open(1).ok_or_else(|| missing(1))??;
    let local = open(2).ok_or_else(|| missing(2))??;
    let highres = open(3).transpose()?;
    let mut reports = vec![
        FileReport::of(FILE_NAMES[0], &portal)?,
        FileReport::of(FILE_NAMES[1], &cell)?,
        FileReport::of(FILE_NAMES[2], &local)?,
    ];
    if let Some(h) = &highres {
        reports.push(FileReport::of(FILE_NAMES[3], h)?);
    }
    Ok((
        RetailDatStore::open_with(portal, cell, local, highres),
        reports,
    ))
}

/// A data file the worker reads for the client, named by its index in [`FILE_NAMES`].
///
/// It holds nothing but the index, so it is `Send` and `Sync` as the store requires; the read
/// itself is a call into the worker's script.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct BrowserFile(pub u32);

#[cfg(target_arch = "wasm32")]
impl DatStorage for BrowserFile {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        // A dat file is under 4 GiB, so the offset is exact as a JavaScript number.
        #[allow(clippy::cast_precision_loss)]
        let at = offset as f64;
        if crate::browser::dat_read(self.0, at, buf) {
            Ok(())
        } else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}
