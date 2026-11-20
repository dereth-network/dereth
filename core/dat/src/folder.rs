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
pub(crate) fn open_disk_mut(path: &Path) -> Result<DiskFile, DatError> {
    refuse_protected(path)?;
    crate::store::shared::forget(path);
    DiskFile::new(OpenOptions::new().read(true).write(true).open(path)?)
}

/// Make the container at `path` on disk, empty, in place of any there is. As
/// [`open_disk_mut`], refused inside a read-only install and forgotten first.
///
/// # Errors
/// As [`open_disk_mut`].
pub(crate) fn create_disk(path: &Path) -> Result<DiskFile, DatError> {
    refuse_protected(path)?;
    crate::store::shared::forget(path);
    DiskFile::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?,
    )
}

/// A container on disk open for writing, as [`File`]'s own storage is, with the parts of it the
/// writer has read kept in memory: a write goes to the file at once, as before, and to any page
/// it holds, so what it reads back is what the file holds. The writer reads the same directory
/// nodes and free-list links over and over (a dozen nodes and their blocks for each record it
/// saves), and each read on its own was a seek and a read of a few bytes.
#[derive(Debug)]
pub(crate) struct DiskFile {
    file: File,
    /// The file's length as this handle has made it.
    len: u64,
    pages: std::collections::HashMap<u64, Box<[u8]>>,
}

impl DiskFile {
    /// The bytes of one page.
    const PAGE: usize = 4096;
    /// The pages held at most (32 MiB); when it is full, they are let go of together.
    const MOST: usize = 8192;

    fn new(file: File) -> Result<Self, DatError> {
        let len = file.metadata()?.len();
        Ok(Self {
            file,
            len,
            pages: std::collections::HashMap::new(),
        })
    }

    /// The page holding offset `n * PAGE`, read in when it is not held.
    fn page(&mut self, n: u64) -> io::Result<&[u8]> {
        if !self.pages.contains_key(&n) {
            if self.pages.len() >= Self::MOST {
                self.pages.clear();
            }
            let mut b = vec![0u8; Self::PAGE].into_boxed_slice();
            let at = n * Self::PAGE as u64;
            if at < self.len {
                let have =
                    usize::try_from((self.len - at).min(Self::PAGE as u64)).unwrap_or(Self::PAGE);
                self.file.seek(SeekFrom::Start(at))?;
                self.file.read_exact(&mut b[..have])?;
            }
            self.pages.insert(n, b);
        }
        Ok(self.pages.get(&n).map_or(&[][..], |p| &p[..]))
    }
}

impl DatStorageMut for DiskFile {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if offset.saturating_add(buf.len() as u64) > self.len {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let page = Self::PAGE as u64;
        let mut done = 0;
        while done < buf.len() {
            let at = offset + done as u64;
            #[allow(clippy::cast_possible_truncation)] // Below `PAGE`.
            let within = (at % page) as usize;
            let take = (buf.len() - done).min(Self::PAGE - within);
            let p = self.page(at / page)?;
            buf[done..done + take].copy_from_slice(&p[within..within + take]);
            done += take;
        }
        Ok(())
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(buf)?;
        self.len = self.len.max(offset + buf.len() as u64);
        let page = Self::PAGE as u64;
        let mut done = 0;
        while done < buf.len() {
            let at = offset + done as u64;
            #[allow(clippy::cast_possible_truncation)] // Below `PAGE`.
            let within = (at % page) as usize;
            let take = (buf.len() - done).min(Self::PAGE - within);
            if let Some(p) = self.pages.get_mut(&(at / page)) {
                p[within..within + take].copy_from_slice(&buf[done..done + take]);
            }
            done += take;
        }
        Ok(())
    }
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

    #[test]
    fn a_disk_container_reads_back_every_write_through_the_pages_it_holds_and_the_file_has_them() {
        let dir = std::env::temp_dir().join(format!("dereth-disk-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.dat");
        let mut w = create_disk(&path).unwrap();
        let edge = DiskFile::PAGE as u64;
        // Across a page's edge, read once so both pages are held, then written over.
        w.write_all_at(edge - 3, b"abcdef").unwrap();
        let mut buf = [0u8; 6];
        w.read_exact_at(edge - 3, &mut buf).unwrap();
        assert_eq!(&buf, b"abcdef");
        w.write_all_at(edge - 1, b"XY").unwrap();
        w.read_exact_at(edge - 3, &mut buf).unwrap();
        assert_eq!(&buf, b"abXYef", "a write to a page it holds is read back");
        // A held page past the old end, then a write into it that makes the file longer.
        w.write_all_at(3 * edge + 10, b"z").unwrap();
        let mut gap = [9u8; 4];
        w.read_exact_at(3 * edge, &mut gap).unwrap();
        assert_eq!(gap, [0; 4], "the gap reads as zeros");
        w.write_all_at(3 * edge + 1, b"q").unwrap();
        w.read_exact_at(3 * edge, &mut gap).unwrap();
        assert_eq!(gap, [0, b'q', 0, 0]);
        assert_eq!(
            w.read_exact_at(3 * edge + 10, &mut [0u8; 2])
                .unwrap_err()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
        // Another handle reads the file as written: nothing waits in memory.
        let disk = std::fs::read(&path).unwrap();
        let at = usize::try_from(edge).unwrap();
        assert_eq!(&disk[at - 3..at + 3], b"abXYef");
        assert_eq!(&disk[3 * at..3 * at + 2], &[0, b'q']);
        assert_eq!(disk.len(), 3 * at + 11);
        drop(w);
        let mut again = open_disk_mut(&path).unwrap();
        again.read_exact_at(edge - 3, &mut buf).unwrap();
        assert_eq!(&buf, b"abXYef", "reopened");
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
