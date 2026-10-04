//! The client's own small files: the preferences profile, the keymaps, the screen layouts and
//! the caches of work it can redo (the object identity verdicts).
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
/// directory's files, the read-only question the keymap save dialog asks, and where caches go.
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
    /// Whether the path names a file, rather than a directory.
    pub is_file: fn(&Path) -> bool,
    /// Remove one file. A missing file is [`io::ErrorKind::NotFound`].
    pub remove_file: fn(&Path) -> io::Result<()>,
    /// Remove only an empty directory. `false` means entries remain, including subdirectories.
    pub remove_empty_dir: fn(&Path) -> io::Result<bool>,
    /// Whether the file may not be written.
    pub read_only: fn(&Path) -> io::Result<bool>,
    /// Make a directory and every directory above it; a store with no directories has nothing
    /// to make.
    pub make_dirs: fn(&Path) -> io::Result<()>,
    /// Where this store keeps caches: per user, kept between runs, and safe to lose (everything
    /// in them can be worked out again). `None` when it keeps none.
    pub cache_dir: fn() -> Option<PathBuf>,
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
    is_file: Path::is_file,
    remove_file: |p| std::fs::remove_file(p),
    remove_empty_dir: |p| {
        if std::fs::read_dir(p)?.next().is_some() {
            return Ok(false);
        }
        std::fs::remove_dir(p)?;
        Ok(true)
    },
    read_only: |p| Ok(std::fs::metadata(p)?.permissions().readonly()),
    make_dirs: |p| std::fs::create_dir_all(p),
    cache_dir: disk_cache_dir,
};

/// The per-user cache folder of the disk: `%LOCALAPPDATA%\Dereth` on Windows,
/// `~/Library/Caches/Dereth` on macOS, and `$XDG_CACHE_HOME/dereth` (else `~/.cache/dereth`)
/// elsewhere. `None` when the variable it is found from is not an absolute path.
fn disk_cache_dir() -> Option<PathBuf> {
    let var = |n: &str| {
        std::env::var_os(n)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if cfg!(windows) {
        Some(var("LOCALAPPDATA")?.join("Dereth"))
    } else if cfg!(target_os = "macos") {
        Some(var("HOME")?.join("Library/Caches/Dereth"))
    } else {
        Some(
            var("XDG_CACHE_HOME")
                .or_else(|| var("HOME").map(|h| h.join(".cache")))?
                .join("dereth"),
        )
    }
}

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

/// Whether the path names a file, rather than a directory.
#[must_use]
pub fn is_file(path: &Path) -> bool {
    (host().is_file)(path)
}

/// Remove one file.
///
/// # Errors
/// The store's own; a missing file is [`io::ErrorKind::NotFound`].
pub fn remove_file(path: &Path) -> io::Result<()> {
    (host().remove_file)(path)
}

/// Remove an empty directory, or return `false` if any entry remains. Never removes contents.
///
/// # Errors
/// The store cannot inspect or remove the directory.
pub fn remove_empty_dir(path: &Path) -> io::Result<bool> {
    (host().remove_empty_dir)(path)
}

/// Whether the file may not be written.
///
/// # Errors
/// The store's own.
pub fn read_only(path: &Path) -> io::Result<bool> {
    (host().read_only)(path)
}

/// Make `dir` and every directory above it.
///
/// # Errors
/// The store's own.
pub fn make_dirs(dir: &Path) -> io::Result<()> {
    (host().make_dirs)(dir)
}

/// Where the store keeps caches; `None` when it keeps none.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    (host().cache_dir)()
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
        is_file: |p| MEMORY.with(|m| m.borrow().contains_key(p)),
        remove_file: |p| {
            MEMORY
                .with(|m| m.borrow_mut().remove(p))
                .map(|_| ())
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        },
        remove_empty_dir: |dir| Ok(MEMORY.with(|m| !m.borrow().keys().any(|p| p.starts_with(dir)))),
        read_only: |_| Ok(false),
        make_dirs: |_| Ok(()),
        cache_dir: || Some(PathBuf::from("/nowhere-on-disk/cache")),
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
    #[test]
    fn removing_files_and_retiring_directories_uses_the_installed_store() {
        install(IN_MEMORY);
        let dir = Path::new("/virtual/retirement");
        let nested = dir.join("child/keep.keymap");
        write(&nested, "keep").unwrap();
        assert!(is_file(&nested));
        assert!(!is_file(dir));
        assert!(
            list(dir).unwrap().is_empty(),
            "the child is not a direct file"
        );
        assert!(
            !remove_empty_dir(dir).unwrap(),
            "nested content prevents retirement"
        );
        remove_file(&nested).unwrap();
        assert!(!exists(&nested));
        assert!(remove_empty_dir(dir).unwrap());
        assert_eq!(
            remove_file(&nested).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        install(DISK);
    }

    #[test]
    fn disk_directory_retirement_keeps_empty_child_directories() {
        let dir = std::env::temp_dir().join(format!(
            "dereth-directory-retirement-{}",
            std::process::id()
        ));
        let child = dir.join("child");
        std::fs::create_dir_all(&child).unwrap();
        assert!(!(DISK.remove_empty_dir)(&dir).unwrap());
        assert!(child.is_dir());
        assert!((DISK.remove_empty_dir)(&child).unwrap());
        assert!((DISK.remove_empty_dir)(&dir).unwrap());
        assert_eq!(
            (DISK.remove_empty_dir)(&dir).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}
