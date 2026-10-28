//! Where a world's sources are kept: read as text, written whole.
//!
//! The format reads and writes its files through a [`SourceStore`], never by itself. A folder on
//! disk ([`DiskStore`]) is the usual one; a builder with no file system it can open by path (a
//! browser's) supplies its own, and the format behaves exactly as it does on disk.
//! [`MemoryStore`] keeps its files in memory.

use std::collections::BTreeMap;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// A world's sources, by path.
pub trait SourceStore {
    /// The file at `path`, as bytes.
    ///
    /// # Errors
    /// [`io::ErrorKind::NotFound`] when there is none, and the storage's own errors.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// The file at `path`, as text.
    ///
    /// # Errors
    /// As [`SourceStore::read`], and [`io::ErrorKind::InvalidData`] for text that is not UTF-8.
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Whether there is a file at `path`.
    fn is_file(&self, path: &Path) -> bool;

    /// Write a new file at `path`, made durable before this returns.
    ///
    /// # Errors
    /// [`io::ErrorKind::AlreadyExists`] when there is one, and the storage's own errors.
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Put `bytes` at `path` in place of what is there, whole or not at all: a reader never sees
    /// half of them. `temp`, beside `path`, is where they wait until they replace it.
    ///
    /// # Errors
    /// The storage's own errors.
    fn replace(&self, path: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Write `bytes` at `path`, in place of any file there, making the folders it is in.
    ///
    /// # Errors
    /// The storage's own errors.
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Remove the file at `path`.
    ///
    /// # Errors
    /// [`io::ErrorKind::NotFound`] when there is none, and the storage's own errors.
    fn remove(&self, path: &Path) -> io::Result<()>;
}

/// Files on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiskStore;

impl SourceStore for DiskStore {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytes).and_then(|()| file.sync_all())
    }

    fn replace(&self, path: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()> {
        self.create_new(temp, bytes)?;
        std::fs::rename(temp, path)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
}

/// Files kept in memory, by path.
#[derive(Debug, Default)]
pub struct MemoryStore {
    files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
}

impl MemoryStore {
    fn files(&self) -> std::sync::MutexGuard<'_, BTreeMap<PathBuf, Vec<u8>>> {
        self.files.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every file, by path.
    #[must_use]
    pub fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.files().clone()
    }
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, path.display().to_string())
}

impl SourceStore for MemoryStore {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files()
            .get(path)
            .cloned()
            .ok_or_else(|| not_found(path))
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files().contains_key(path)
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut files = self.files();
        if files.contains_key(path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                path.display().to_string(),
            ));
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn replace(&self, path: &Path, _temp: &Path, bytes: &[u8]) -> io::Result<()> {
        self.files().insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.files().insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| not_found(path))
    }
}
