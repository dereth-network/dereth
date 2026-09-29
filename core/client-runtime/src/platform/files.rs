//! The client's own small files: the preferences profile, the keymaps and the screen layouts.
//!
//! Every read and write of those goes through here, so a host without a disk can keep them
//! somewhere else. With nothing installed they are the disk, through `std::fs`, exactly as before;
//! a host installs a [`crate::platform::files::FileHost`] on the thread that runs the client and its store is used
//! instead. A web page keeps them in the browser's storage this way.
//!
//! The data files are not these: they are opened once, by the platform, and never written.

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};

/// A store for the client's own files: whole-file reads and writes by path, a listing of one
/// directory's files, and the read-only question the keymap save dialog asks.
#[derive(Debug, Clone, Copy)]
pub struct FileHost {
    /// The whole file. A missing file is [`io::ErrorKind::NotFound`].
    pub read: fn(&Path) -> io::Result<Vec<u8>>,
    /// Replace the whole file, making it if it is not there.
    pub write: fn(&Path, &[u8]) -> io::Result<()>,
    /// The files directly in a directory.
    pub list: fn(&Path) -> io::Result<Vec<PathBuf>>,
    /// Whether the file is there.
    pub exists: fn(&Path) -> bool,
    /// Whether the file may not be written.
    pub read_only: fn(&Path) -> io::Result<bool>,
}

thread_local! {
    static HOST: Cell<Option<FileHost>> = const { Cell::new(None) };
}

/// Keep this thread's files in `host` from now on.
pub fn install(host: FileHost) {
    HOST.with(|h| h.set(Some(host)));
}

/// The disk, which is the store when none is installed.
pub const DISK: FileHost = FileHost {
    read: |p| std::fs::read(p),
    write: |p, bytes| std::fs::write(p, bytes),
    list: |dir| {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_file() {
                files.push(path);
            }
        }
        Ok(files)
    },
    exists: Path::exists,
    read_only: |p| Ok(std::fs::metadata(p)?.permissions().readonly()),
};

fn host() -> FileHost {
    HOST.with(Cell::get).unwrap_or(DISK)
}

/// The whole file.
///
/// # Errors
/// The store's own; a missing file is [`io::ErrorKind::NotFound`].
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    (host().read)(path)
}

/// The whole file, as UTF-8 text.
///
/// # Errors
/// As [`read`], or [`io::ErrorKind::InvalidData`] for text that is not UTF-8.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Replace the whole file.
///
/// # Errors
/// The store's own.
pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    (host().write)(path, bytes.as_ref())
}

/// The files directly in `dir`.
///
/// # Errors
/// The store's own.
pub fn list(dir: &Path) -> io::Result<Vec<PathBuf>> {
    (host().list)(dir)
}

/// Whether the file is there.
#[must_use]
pub fn exists(path: &Path) -> bool {
    (host().exists)(path)
}

/// Whether the file may not be written.
///
/// # Errors
/// The store's own.
pub fn read_only(path: &Path) -> io::Result<bool> {
    (host().read_only)(path)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (host file routing preserves whole-file operations).
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    thread_local! {
        static MEMORY: RefCell<BTreeMap<PathBuf, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    }

    const IN_MEMORY: FileHost = FileHost {
        read: |p| {
            MEMORY
                .with(|m| m.borrow().get(p).cloned())
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        },
        write: |p, bytes| {
            MEMORY.with(|m| m.borrow_mut().insert(p.to_path_buf(), bytes.to_vec()));
            Ok(())
        },
        list: |dir| {
            Ok(MEMORY.with(|m| {
                m.borrow()
                    .keys()
                    .filter(|p| p.parent() == Some(dir))
                    .cloned()
                    .collect()
            }))
        },
        exists: |p| MEMORY.with(|m| m.borrow().contains_key(p)),
        read_only: |_| Ok(false),
    };

    /// With nothing installed the files are the disk's: what `std::fs` wrote is what is read, and
    /// a missing file is not found.
    #[test]
    fn with_nothing_installed_the_files_are_the_disk_s() {
        let dir = std::env::temp_dir().join(format!("dereth-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        let path = dir.join("UserPreferences.ini");
        std::fs::write(&path, "[Display]\r\nWidth=800\r\n").expect("written");
        assert_eq!(
            read_to_string(&path).expect("read"),
            "[Display]\r\nWidth=800\r\n"
        );
        assert!(exists(&path));
        assert_eq!(list(&dir).expect("listed"), vec![path.clone()]);
        assert_eq!(
            read(&dir.join("absent.keymap")).map_err(|e| e.kind()),
            Err(io::ErrorKind::NotFound)
        );
        std::fs::remove_dir_all(&dir).expect("cleared");
    }

    /// An installed store is the one written and read, on the thread that installed it, and the
    /// disk is not touched.
    #[test]
    fn an_installed_store_takes_every_read_and_write() {
        install(IN_MEMORY);
        let dir = Path::new("/nowhere-on-disk/settings");
        let path = dir.join("acclient.keymap");
        assert!(!exists(&path));
        write(&path, "keymap text").expect("written");
        assert_eq!(read_to_string(&path).expect("read"), "keymap text");
        assert_eq!(list(dir).expect("listed"), vec![path.clone()]);
        assert!(!path.exists(), "nothing reached the disk");
    }
}
