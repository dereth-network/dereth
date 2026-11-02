//! The client's data files, in the order the client opens them, and its language. The data
//! download reads and stores through them ([`crate::download`]).

use std::sync::Arc;

use auberean_dat::DatFile;

/// The language word of English.
pub const ENGLISH: u32 = 0x8000_0001;

/// The client's data files, in the order it opened them.
#[derive(Clone, Default)]
pub struct DataFiles {
    pub files: Vec<Arc<DatFile>>,
    pub language: u32,
}

impl core::fmt::Debug for DataFiles {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DataFiles")
            .field("files", &self.files.len())
            .field("language", &self.language)
            .finish()
    }
}

/// A data file's id in the data download: its set and its subset.
#[must_use]
pub fn file_id(dat: &DatFile) -> u64 {
    dat.header().dat_file_id()
}
