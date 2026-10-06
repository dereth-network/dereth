//! Where written containers are kept: a container's bytes open for reading and writing at any
//! offset ([`DatStorageMut`]), and a folder of named containers ([`DatFolder`]).
//!
//! Reads of a container already go through [`DatStorage`]; this is the writing half of the same
//! seam. The writer ([`crate::write::DatWriter`]) reads and writes its container through a
//! [`DatStorageMut`], and a world's overlay folder ([`crate::overlay::OverlayDir`]) finds, opens,
//! makes and removes its containers through a [`DatFolder`]. A folder on disk ([`DiskFolder`]) is
//! the usual one. A platform with no file system the client can open by path (a browser, which
//! keeps a world's overlay in its own storage) supplies its own, and the writer and the overlay
//! behave exactly as they do on disk. [`MemoryFolder`] keeps its containers in memory.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::container::DatStorage;
use crate::error::DatError;

/// A container's bytes, open for reading and writing at any offset. A write past the end makes the
/// container longer; any gap it leaves reads as zeros.
pub trait DatStorageMut: std::fmt::Debug + Send {
    /// Fill all of `buf` from `offset`; end of data before `buf` is full is `UnexpectedEof`.
    ///
    /// # Errors
    /// Whatever the underlying storage reports.
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()>;

    /// Write all of `buf` at `offset`.
    ///
    /// # Errors
    /// Whatever the underlying storage reports; a storage that is full says so here.
    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()>;
}

/// A file on disk: `seek` and then `read_exact` or `write_all`, unbuffered, which is the client's
/// own synchronous write.
impl DatStorageMut for File {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;
        self.read_exact(buf)
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;
        self.write_all(buf)
    }
}

/// A folder of named containers: what an overlay folder keeps its containers in.
pub trait DatFolder: std::fmt::Debug + Send + Sync {
    /// The folder's name, for messages; a container in it is named `path().join(name)`.
    fn path(&self) -> &Path;

    /// Whether the folder holds a container called `name`.
    fn is_file(&self, name: &str) -> bool;

    /// The container `name`, for reading.
    ///
    /// # Errors
    /// [`io::ErrorKind::NotFound`] when there is none, and the storage's own errors.
    fn open(&self, name: &str) -> io::Result<Box<dyn DatStorage>>;

    /// The container `name`, for reading and writing.
    ///
    /// # Errors
    /// The storage's own errors, and any refusal to write there.
    fn open_mut(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError>;

    /// A new, empty container `name`, in place of any there is; the folder is made if it is not
    /// there yet.
    ///
    /// # Errors
    /// The storage's own errors, and any refusal to write there.
    fn create(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError>;

    /// Remove the container `name`.
    ///
    /// # Errors
    /// [`io::ErrorKind::NotFound`] when there is none, and the storage's own errors.
    fn remove(&self, name: &str) -> io::Result<()>;
}

/// A folder on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskFolder {
    dir: PathBuf,
}

impl DiskFolder {
    /// The folder at `dir`, which need not exist yet.
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }
}

/// Open the container at `path` on disk for reading and writing: refused inside a read-only
/// install, and forgotten by the process's table of open files first, because what follows may
/// change it without changing its length or its time.
///
/// # Errors
/// [`DatError::RetailDatRefused`] inside a read-only install, and I/O errors.
pub(crate) fn open_disk_mut(path: &Path) -> Result<File, DatError> {
    refuse_protected(path)?;
    crate::store::shared::forget(path);
    Ok(OpenOptions::new().read(true).write(true).open(path)?)
}

/// Make the container at `path` on disk, empty, in place of any there is. As
/// [`open_disk_mut`], refused inside a read-only install and forgotten first.
///
/// # Errors
/// As [`open_disk_mut`].
pub(crate) fn create_disk(path: &Path) -> Result<File, DatError> {
    refuse_protected(path)?;
    crate::store::shared::forget(path);
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?)
}

/// Refuse to write anything inside a read-only install.
pub(crate) fn refuse_protected(path: &Path) -> Result<(), DatError> {
    if crate::locate::is_protected(path) {
        return Err(DatError::RetailDatRefused(path.to_path_buf()));
    }
    Ok(())
}

impl DatFolder for DiskFolder {
    fn path(&self) -> &Path {
        &self.dir
    }

    fn is_file(&self, name: &str) -> bool {
        self.dir.join(name).is_file()
    }

    fn open(&self, name: &str) -> io::Result<Box<dyn DatStorage>> {
        Ok(Box::new(File::open(self.dir.join(name))?))
    }

    fn open_mut(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError> {
        Ok(Box::new(open_disk_mut(&self.dir.join(name))?))
    }

    fn create(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError> {
        std::fs::create_dir_all(&self.dir).map_err(DatError::from)?;
        Ok(Box::new(create_disk(&self.dir.join(name))?))
    }

    fn remove(&self, name: &str) -> io::Result<()> {
        std::fs::remove_file(self.dir.join(name))
    }
}

/// One container's bytes in memory, shared by every handle on it: a write through one is read
/// through all, as with a file.
#[derive(Debug, Clone, Default)]
pub struct MemoryFile(Arc<Mutex<Vec<u8>>>);

impl MemoryFile {
    /// The container's bytes as they are now.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn read_bytes_at(bytes: &[u8], offset: u64, buf: &mut [u8]) -> io::Result<()> {
    let start = usize::try_from(offset).map_err(|_| io::ErrorKind::UnexpectedEof)?;
    let end = start
        .checked_add(buf.len())
        .ok_or(io::ErrorKind::UnexpectedEof)?;
    let src = bytes.get(start..end).ok_or(io::ErrorKind::UnexpectedEof)?;
    buf.copy_from_slice(src);
    Ok(())
}

impl DatStorage for MemoryFile {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        read_bytes_at(
            &self.0.lock().unwrap_or_else(PoisonError::into_inner),
            offset,
            buf,
        )
    }
}

impl DatStorageMut for MemoryFile {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        DatStorage::read_exact_at(self, offset, buf)
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        let mut bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let start = usize::try_from(offset).map_err(|_| io::ErrorKind::InvalidInput)?;
        let end = start
            .checked_add(buf.len())
            .ok_or(io::ErrorKind::InvalidInput)?;
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        bytes[start..end].copy_from_slice(buf);
        Ok(())
    }
}

/// A folder kept in memory: its containers live as long as the folder (or a handle on one of
/// them) does.
#[derive(Debug, Clone, Default)]
pub struct MemoryFolder {
    dir: PathBuf,
    files: Arc<Mutex<BTreeMap<String, MemoryFile>>>,
}

impl MemoryFolder {
    /// An empty folder, named `dir` in messages.
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            files: Arc::default(),
        }
    }

    /// The container `name`, when the folder holds one.
    #[must_use]
    pub fn file(&self, name: &str) -> Option<MemoryFile> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    /// Every container's name, ascending.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }
}

impl DatFolder for MemoryFolder {
    fn path(&self) -> &Path {
        &self.dir
    }

    fn is_file(&self, name: &str) -> bool {
        self.file(name).is_some()
    }

    fn open(&self, name: &str) -> io::Result<Box<dyn DatStorage>> {
        let f = self.file(name).ok_or(io::ErrorKind::NotFound)?;
        Ok(Box::new(f))
    }

    fn open_mut(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError> {
        let f = self
            .file(name)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        Ok(Box::new(f))
    }

    fn create(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError> {
        let f = MemoryFile::default();
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name.to_owned(), f.clone());
        Ok(Box::new(f))
    }

    fn remove(&self, name: &str) -> io::Result<()> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_container_grows_on_a_write_past_its_end_and_every_handle_reads_the_write() {
        let folder = MemoryFolder::new(Path::new("mem"));
        assert!(!folder.is_file("a.dat"));
        let mut w = folder.create("a.dat").unwrap();
        w.write_all_at(4, b"xy").unwrap();
        let r = folder.open("a.dat").unwrap();
        let mut buf = [9u8; 6];
        r.read_exact_at(0, &mut buf).unwrap();
        assert_eq!(buf, [0, 0, 0, 0, b'x', b'y'], "the gap reads as zeros");
        assert_eq!(
            r.read_exact_at(5, &mut [0u8; 2]).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        w.write_all_at(0, b"ab").unwrap();
        r.read_exact_at(0, &mut buf).unwrap();
        assert_eq!(&buf[..2], b"ab", "a write is read through every handle");
        assert_eq!(folder.names(), ["a.dat"]);
        folder.remove("a.dat").unwrap();
        assert!(!folder.is_file("a.dat"));
        assert_eq!(
            folder.remove("a.dat").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(folder.open_mut("a.dat").is_err());
    }
}
