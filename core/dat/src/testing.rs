//! Where the tests find the retail dats: the one lookup every test in the workspace, client and
//! server alike, goes through. Built only for tests (the `test-support` feature, which the crates
//! enable in their `[dev-dependencies]`).
//!
//! The dats are wherever `DERETH_TEST_DAT_DIR` says, and nowhere else: the workspace knows no
//! install location of its own. Set it in the environment, or once for every build under a
//! directory in a Cargo configuration file there (`[env]`, with `relative = true` for a path
//! relative to that directory). Pointing it at an empty directory is how a run proves it passes
//! with no dats.
//!
//! The search itself is [`crate::locate_modern_dats`]; this module only supplies the candidate.
//! The directory found is declared read-only for the run ([`crate::protect_install`]), so no test
//! can write to the install every other test reads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::{locate_modern_dats, DatDir, DatsNotFound, ModernDat, RetailDatStore};

/// A uniquely owned temporary directory. Existing paths are never removed during allocation.
#[derive(Debug)]
pub struct ScratchDir {
    path: PathBuf,
    owned: bool,
}

impl ScratchDir {
    /// Allocate an empty directory using an ASCII alphanumeric, hyphen or underscore tag.
    ///
    /// # Errors
    /// Returns an invalid-input error for an unsafe tag, or the directory creation error.
    pub fn new(tag: &str) -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self::create_in(&std::env::temp_dir(), tag, &NEXT)
    }

    fn create_in(root: &Path, tag: &str, next: &AtomicU64) -> std::io::Result<Self> {
        if tag.is_empty()
            || !tag
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsafe directory tag",
            ));
        }
        loop {
            let serial = next.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("dereth-{tag}-{}-{serial}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path, owned: true }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// The allocated path, also available after explicit cleanup.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Remove the owned directory. Successful cleanup is idempotent.
    ///
    /// # Errors
    /// Returns the removal error and retains ownership so cleanup can be retried.
    pub fn cleanup(&mut self) -> std::io::Result<()> {
        if self.owned {
            std::fs::remove_dir_all(&self.path)?;
            self.owned = false;
        }
        Ok(())
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// The test-only variable naming the retail dat directory.
pub const DAT_DIR_VAR: &str = "DERETH_TEST_DAT_DIR";

/// The directories looked in: `DERETH_TEST_DAT_DIR` when it is set and not empty, else none.
#[must_use]
pub fn candidates() -> Vec<PathBuf> {
    std::env::var_os(DAT_DIR_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

/// The located directory, once per test process. A directory found is protected for the run.
///
/// # Errors
///
/// [`DatsNotFound`] naming every candidate.
pub fn locate() -> Result<&'static DatDir, &'static DatsNotFound> {
    static FOUND: OnceLock<Result<DatDir, DatsNotFound>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let found = locate_modern_dats(&candidates());
            if let Ok(dir) = &found {
                crate::protect_install(dir.path());
            }
            found
        })
        .as_ref()
}

/// The retail dat directory, or -- when none was found -- the candidate, so that a path built from
/// it names where the dats were expected (an empty path when `DERETH_TEST_DAT_DIR` is unset).
#[must_use]
pub fn dat_dir() -> PathBuf {
    match locate() {
        Ok(dir) => dir.path().to_path_buf(),
        Err(miss) => miss.tried.first().cloned().unwrap_or_default(),
    }
}

/// One retail file's path in [`dat_dir`].
#[must_use]
pub fn dat_file(dat: ModernDat) -> PathBuf {
    dat.in_dir(&dat_dir())
}

/// Whether the retail dats were found.
#[must_use]
pub fn have_dats() -> bool {
    locate().is_ok()
}

/// Why the retail dats cannot be read, as the line a skipped or failed test prints, or `None`
/// when all the files a store needs are there.
#[must_use]
pub fn shortfall() -> Option<String> {
    let reason = match locate() {
        Ok(dir) => RetailDatStore::shortfall_in(dir.path())?,
        Err(miss) if miss.tried.is_empty() => {
            format!("retail dats not found: {DAT_DIR_VAR} is unset")
        }
        Err(miss) => miss.to_string(),
    };
    Some(format!(
        "{reason} (set {DAT_DIR_VAR} to the directory holding them)"
    ))
}

/// Panic with [`shortfall`] unless the retail dats are there, **without opening them**: for a
/// test whose subject opens the files itself.
///
/// # Panics
///
/// When the dats are not found.
pub fn require_dats() {
    if let Some(msg) = shortfall() {
        panic!("{msg}");
    }
}

/// The store over [`dat_dir`], or `None` when the dats are not there.
///
/// **Prefer [`open_store_or_fail`] in a test.** A `let Some(s) = open_store() else { return }`
/// makes a test pass having read nothing: a skipped test and a passing test are the same green
/// line. Take the `Option` only where the absence is genuinely environmental, and say so at the
/// site.
#[must_use]
pub fn open_store() -> Option<RetailDatStore> {
    if shortfall().is_some() {
        return None;
    }
    RetailDatStore::open_dir(&dat_dir()).ok()
}

/// The store over [`dat_dir`].
///
/// # Panics
///
/// With [`shortfall`]'s line when the dats are not there, or with the open error when they are
/// and do not open.
#[must_use]
pub fn open_store_or_fail() -> RetailDatStore {
    require_dats();
    let dir = dat_dir();
    RetailDatStore::open_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "the retail dats under {} are all present and did not open: {e}",
            dir.display()
        )
    })
}

/// The test-only variable naming a directory that holds the February 2005 dat set (`portal.dat`
/// and `cell.dat`, from before Throne of Destiny).
pub const CLASSIC_DAT_DIR_VAR: &str = "DERETH_TEST_PRETOD_DAT_DIR";

/// The directory `DERETH_TEST_PRETOD_DAT_DIR` names, or `None` when it is unset or empty.
#[must_use]
pub fn classic_dat_dir() -> Option<PathBuf> {
    std::env::var_os(CLASSIC_DAT_DIR_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Why the February 2005 dats cannot be read, or `None` when both files are there.
#[must_use]
pub fn classic_shortfall() -> Option<String> {
    match classic_dat_dir() {
        None => Some(format!(
            "the February 2005 dats were not found: {CLASSIC_DAT_DIR_VAR} is unset (set it to \
             the directory holding portal.dat and cell.dat)"
        )),
        Some(dir) if !crate::holds_classic_dats(&dir) => Some(format!(
            "the February 2005 dats were not found: no portal.dat and cell.dat under {} \
             ({CLASSIC_DAT_DIR_VAR})",
            dir.display()
        )),
        Some(_) => None,
    }
}

/// The store over the February 2005 dat set. The directory is protected for the run, as the
/// retail install is.
///
/// # Panics
///
/// With [`classic_shortfall`]'s line when the files are not there, or with the open error when
/// they are and do not open.
#[must_use]
pub fn open_classic_store_or_fail() -> RetailDatStore {
    if let Some(msg) = classic_shortfall() {
        panic!("{msg}");
    }
    let dir = classic_dat_dir().unwrap_or_default();
    crate::protect_install(&dir);
    RetailDatStore::open_classic_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "the February 2005 dats under {} did not open: {e}",
            dir.display()
        )
    })
}

/// One folder holding both dat sets, as a player's one `--dat-dir` does: the retail files of
/// [`dat_dir`] and the February 2005 `portal.dat` and `cell.dat` of [`classic_dat_dir`], hard-linked
/// into a folder under the temp directory (no copy is made). The folder is named after the two it
/// joins, so every test process reuses it, and it is protected for the run as the install is.
///
/// # Panics
///
/// With the shortfall line when either set is not there, or when a file cannot be linked (the
/// temp directory on another volume than the dats).
#[must_use]
pub fn both_sets_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(msg) = shortfall() {
            panic!("{msg}");
        }
        if let Some(msg) = classic_shortfall() {
            panic!("{msg}");
        }
        let later = dat_dir();
        let older = classic_dat_dir().unwrap_or_default();
        // FNV-1a over the two folders' spellings: a stable name for the pair.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{}|{}", later.display(), older.display()).bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        let dir = std::env::temp_dir().join(format!("dereth-both-dat-sets-{hash:016x}"));
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("the folder {} could not be made: {e}", dir.display()));
        let files = ModernDat::ALL
            .iter()
            .map(|d| d.in_dir(&later))
            .chain(crate::ClassicDat::ALL.iter().map(|d| d.in_dir(&older)))
            .filter(|p| p.is_file());
        for src in files {
            let dst = dir.join(src.file_name().unwrap_or_default());
            let current = std::fs::metadata(&dst)
                .ok()
                .zip(std::fs::metadata(&src).ok())
                .is_some_and(|(d, s)| d.len() == s.len() && d.modified().ok() == s.modified().ok());
            if current {
                continue;
            }
            let _ = std::fs::remove_file(&dst);
            if let Err(e) = std::fs::hard_link(&src, &dst) {
                // Another test process may have linked it first.
                if !dst.is_file() {
                    panic!(
                        "{} could not be linked into {}: {e}",
                        src.display(),
                        dir.display()
                    );
                }
            }
        }
        crate::protect_install(&dir);
        dir
    })
    .clone()
}

/// The test-only variable naming a directory of historical dat captures: one folder per capture,
/// named by its date, holding the files of that capture (`portal.dat`, `cell.dat`,
/// `client_portal.dat`, `client_cell_1.dat`, `client_highres.dat`, `client_local_english.dat`,
/// any subset, the names in lower case).
pub const DAT_CAPTURES_DIR_VAR: &str = "DERETH_TEST_DAT_CAPTURES_DIR";

/// Every capture file under `DERETH_TEST_DAT_CAPTURES_DIR`, as `(capture folder, file name,
/// path)`, oldest capture first. The directory is protected for the run, as the retail install
/// is.
///
/// # Panics
///
/// When the variable is unset or empty, or names a directory that cannot be read.
#[must_use]
pub fn dat_captures_or_fail() -> Vec<(String, String, PathBuf)> {
    let dir = std::env::var_os(DAT_CAPTURES_DIR_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!(
                "the dat captures were not found: {DAT_CAPTURES_DIR_VAR} is unset (set it to the \
                 directory holding one folder per capture)"
            )
        });
    crate::protect_install(&dir);
    let read = |d: &std::path::Path| {
        std::fs::read_dir(d).unwrap_or_else(|e| {
            panic!(
                "the dat captures under {} ({DAT_CAPTURES_DIR_VAR}) cannot be read: {e}",
                d.display()
            )
        })
    };
    let mut out = Vec::new();
    for capture in read(&dir).flatten() {
        if !capture.path().is_dir() {
            continue;
        }
        let folder = capture.file_name().to_string_lossy().into_owned();
        for file in read(&capture.path()).flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if name.ends_with(".dat") && file.path().is_file() {
                out.push((folder.clone(), name, file.path()));
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod scratch_tests {
    use super::*;

    /// Behaviour: none (temporary directory owners remain independent and support checked cleanup).
    #[test]
    fn same_tag_owners_are_independent_and_cleanup_is_explicit() {
        let first = ScratchDir::new("independent").unwrap();
        let mut second = ScratchDir::new("independent").unwrap();
        assert_ne!(first.path(), second.path());
        std::fs::write(second.path().join("kept"), b"retained").unwrap();
        let first_path = first.path().to_owned();
        drop(first);
        assert!(!first_path.exists());
        assert_eq!(
            std::fs::read(second.path().join("kept")).unwrap(),
            b"retained"
        );
        second.cleanup().unwrap();
        assert!(!second.path().exists());
        second.cleanup().unwrap();
    }

    /// Behaviour: none (allocation skips existing candidates without altering their contents).
    #[test]
    fn existing_candidates_are_preserved() {
        let parent = ScratchDir::new("collision").unwrap();
        let occupied = parent
            .path()
            .join(format!("dereth-child-{}-0", std::process::id()));
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("kept"), b"original").unwrap();
        let child = ScratchDir::create_in(parent.path(), "child", &AtomicU64::new(0)).unwrap();
        assert_eq!(
            child.path(),
            parent
                .path()
                .join(format!("dereth-child-{}-1", std::process::id()))
        );
        drop(child);
        assert_eq!(std::fs::read(occupied.join("kept")).unwrap(), b"original");
    }

    /// Behaviour: none (invalid tags and non-collision filesystem errors do not retry).
    #[test]
    fn unsafe_tags_and_non_directory_roots_fail() {
        for tag in ["", "../escape", "a/b", "a\\b", "/absolute"] {
            assert_eq!(
                ScratchDir::new(tag).unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput
            );
        }
        let parent = ScratchDir::new("failure").unwrap();
        let file = parent.path().join("file");
        std::fs::write(&file, b"file").unwrap();
        let next = AtomicU64::new(0);
        assert!(ScratchDir::create_in(&file, "child", &next).is_err());
        assert_eq!(next.load(Ordering::Relaxed), 1);
    }
}
