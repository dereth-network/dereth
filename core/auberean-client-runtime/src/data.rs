//! The client's data files, as the data download sees them: which versions of each file it holds.
//!
//! Asked by the server, the client answers with its language and, for each of its data files in
//! the order it opened them, the file's id (its set in the high word, its subset in the low: the
//! language of a local file, the region of a cell file) and the set of versions its iteration list
//! holds, as the list stores them.

use std::sync::Arc;

use auberean_dat::container::ITERATION_LIST;
use auberean_dat::DatFile;
use auberean_protocol::login::{DataFileIterations, DataHeld};
use auberean_protocol::types::IterationSet;
use auberean_protocol::wire::{Field, Reader};

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

impl DataFiles {
    /// The answer to the server's question: the language, then each file's versions.
    // D2-05138, D2-12846
    #[must_use]
    pub fn held(&self) -> DataHeld {
        let mut files = Vec::new();
        for dat in &self.files {
            let Ok(raw) = dat.read(ITERATION_LIST) else {
                continue;
            };
            let mut r = Reader::new(&raw);
            let Ok(iterations) = IterationSet::read(&mut r) else {
                continue;
            };
            files.push(DataFileIterations {
                dat_file: file_id(dat),
                iterations,
            });
        }
        DataHeld {
            language: self.language,
            files,
        }
    }
}
