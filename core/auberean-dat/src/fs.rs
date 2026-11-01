//! Data files on disk (the `fs` feature): a container opened from a file, read with positioned
//! reads so one open file serves any number of threads.

use std::fs::File;
use std::path::Path;

use crate::container::DatFile;
use dereth_dat::DatError;

impl DatFile {
    /// Open a data file on disk, read-only.
    ///
    /// # Errors
    /// The file does not open, or is not a container in this game's layout.
    pub fn open(path: &Path) -> Result<Self, DatError> {
        Self::from_storage(path.to_path_buf(), Box::new(File::open(path)?))
    }
}
