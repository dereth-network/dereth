//! Data files on disk (the `fs` feature): a container opened from a file, read with positioned
//! reads so one open file serves any number of threads.
//!
//! A read the file ends before its buffer is full is a short read ([`crate::checked::ShortRead`]),
//! and the open file's size and modification time, taken at the opening, are what its watch
//! ([`FileWatch`]) compares when a record fails ([`DatFile::read_checked`]).

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use crate::checked::{Exact, ReadAt, Watch};
use crate::container::DatFile;
use dereth_dat::DatError;

impl DatFile {
    /// Open a data file on disk, read-only, watched for changes ([`FileWatch`]).
    ///
    /// # Errors
    /// The file does not open, or is not a container in this game's layout.
    pub fn open(path: &Path) -> Result<Self, DatError> {
        let file = Arc::new(File::open(path)?);
        let watch = FileWatch::new(Arc::clone(&file))?;
        Ok(Self::from_storage(path.to_path_buf(), Box::new(Exact(file)))?.watched(Arc::new(watch)))
    }
}

impl ReadAt for File {
    #[cfg(unix)]
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        std::os::unix::fs::FileExt::read_at(self, buf, offset)
    }

    /// `seek_read` moves the handle's cursor as a side effect, which no read here depends on:
    /// every read names its own offset.
    #[cfg(windows)]
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        std::os::windows::fs::FileExt::seek_read(self, buf, offset)
    }

    /// A platform with neither positioned read opens no file by path; its containers come through
    /// [`DatFile::from_storage`].
    #[cfg(not(any(unix, windows)))]
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// An open file's size and modification time, compared with what they were at the opening.
#[derive(Debug)]
pub struct FileWatch {
    file: Arc<File>,
    len: u64,
    modified: Option<SystemTime>,
}

impl FileWatch {
    /// Watch `file` from now.
    ///
    /// # Errors
    /// The file's size does not read.
    pub fn new(file: Arc<File>) -> io::Result<Self> {
        let m = file.metadata()?;
        Ok(Self {
            len: m.len(),
            modified: m.modified().ok(),
            file,
        })
    }
}

/// A time as seconds since 1970, to the nanosecond, or `unknown`.
fn when(t: Option<SystemTime>) -> String {
    match t.map(|t| t.duration_since(SystemTime::UNIX_EPOCH)) {
        Some(Ok(d)) => format!("{}.{:09}", d.as_secs(), d.subsec_nanos()),
        _ => "unknown".into(),
    }
}

impl Watch for FileWatch {
    fn changed(&self) -> io::Result<Option<String>> {
        let m = self.file.metadata()?;
        let (len, modified) = (m.len(), m.modified().ok());
        if len == self.len && modified == self.modified {
            return Ok(None);
        }
        Ok(Some(format!(
            "{} bytes modified at {} when opened, now {len} bytes modified at {}",
            self.len,
            when(self.modified),
            when(modified)
        )))
    }
}
