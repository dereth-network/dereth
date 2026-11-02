//! The file underneath: opened by the host, read and written whole at an offset.

use dereth_dat::DatStorageMut;

use super::{DiskError, Result};

/// How a data file is to be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// A file that is there, for reading (and writing unless `read_only`).
    Existing { read_only: bool },
    /// A new, empty file, replacing one of the same name.
    Create,
}

/// Where data files are opened: the host's file system, or memory in tests.
pub trait Files {
    /// Open `name` (a folder and a file name joined by [`join`]); `None` when it does not open.
    fn open(&mut self, name: &str, mode: OpenMode) -> Option<Box<dyn DatStorageMut>>;
}

/// The name a file is opened by: the folder, a `/` when it does not already end in a slash or a
/// backslash, then the file's name. An empty folder leaves the name alone; an empty name leaves
/// the folder alone, without the slash.
// D2-17749
#[must_use]
pub fn join(file: &str, path: &str) -> String {
    let mut name = String::from(path);
    if !path.is_empty() && !file.is_empty() && !path.ends_with(['\\', '/']) {
        name.push('/');
    }
    name.push_str(file);
    name
}

/// An open file, or none.
#[derive(Debug, Default)]
pub struct Handle {
    file: Option<Box<dyn DatStorageMut>>,
}

impl Handle {
    /// No file.
    // D2-17742
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open `file` in `path` through the host. A file opened to be created is empty.
    ///
    /// # Errors
    /// [`DiskError::NotOpened`] when the host cannot open it.
    // D2-17749
    pub fn open_file(
        &mut self,
        files: &mut dyn Files,
        file: &str,
        path: &str,
        mode: OpenMode,
    ) -> Result<()> {
        self.file = files.open(&join(file, path), mode);
        if self.file.is_some() {
            Ok(())
        } else {
            Err(DiskError::NotOpened)
        }
    }

    /// Take a file the host already opened.
    pub fn adopt(&mut self, file: Box<dyn DatStorageMut>) {
        self.file = Some(file);
    }

    /// Whether a file is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }

    /// Close the file.
    ///
    /// # Errors
    /// [`DiskError::CloseFailed`] when no file was open.
    // D2-17744, D2-17746
    pub fn close(&mut self) -> Result<()> {
        match self.file.take() {
            Some(_) => Ok(()),
            None => Err(DiskError::CloseFailed),
        }
    }

    /// Fill `buf` from `at`.
    ///
    /// # Errors
    /// [`DiskError::NoHandle`] with no file open; [`DiskError::ShortRead`] when the file ends
    /// first.
    // D2-17747
    pub fn sync_read(&mut self, buf: &mut [u8], at: u32) -> Result<()> {
        let f = self.file.as_mut().ok_or(DiskError::NoHandle)?;
        f.read_exact_at(u64::from(at), buf)
            .map_err(|_| DiskError::ShortRead)
    }

    /// Write `buf` at `at`; a file grows to hold it.
    ///
    /// # Errors
    /// [`DiskError::NoHandle`] with no file open; [`DiskError::ShortWrite`] when it does not all
    /// go out.
    // D2-17748
    pub fn sync_write(&mut self, buf: &[u8], at: u32) -> Result<()> {
        let f = self.file.as_mut().ok_or(DiskError::NoHandle)?;
        f.write_all_at(u64::from(at), buf)
            .map_err(|_| DiskError::ShortWrite)
    }

    /// One little-endian word at `at`.
    ///
    /// # Errors
    /// As [`Handle::sync_read`].
    pub fn read_word(&mut self, at: u32) -> Result<u32> {
        let mut w = [0; 4];
        self.sync_read(&mut w, at)?;
        Ok(u32::from_le_bytes(w))
    }

    /// One little-endian word written at `at`.
    ///
    /// # Errors
    /// As [`Handle::sync_write`].
    pub fn write_word(&mut self, w: u32, at: u32) -> Result<()> {
        self.sync_write(&w.to_le_bytes(), at)
    }
}
