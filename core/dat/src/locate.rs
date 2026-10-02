//! Finding the retail dats: the one place that knows which file marks a dat directory and what
//! the four files are called on disk.
//!
//! Nothing here reads the environment or knows an install location. A caller hands
//! [`locate_retail_dats`] the directories it is willing to use, in its own order of preference --
//! the client passes `--dat-dir`, the working directory and the executable's directory; the server
//! passes its configured `dat_files_directory` or its search folders; the tests pass their
//! test-only variable -- and gets back the first that holds
//! `client_portal.dat`, or an error naming every directory it tried.
//!
//! A path to one of the files is built here too ([`RetailDat::in_dir`]), so no other crate joins a
//! retail file name onto a directory.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::DatKind;

/// One of the four retail data files, by the name it has on disk. They order as [`RetailDat::ALL`]
/// lists them, so a map keyed by the file walks them in the order the client opens them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RetailDat {
    /// `client_portal.dat`: the portal database, and the file whose presence marks a dat
    /// directory.
    Portal,
    /// `client_cell_1.dat`: the landblocks and interior cells.
    Cell,
    /// `client_local_English.dat`: the language database.
    Local,
    /// `client_highres.dat`: the optional high-resolution portal partition.
    HighRes,
}

impl RetailDat {
    /// All four, in the order the client opens them (the high-resolution file last, and only on
    /// request).
    pub const ALL: [Self; 4] = [Self::Portal, Self::Cell, Self::Local, Self::HighRes];

    /// The three a store cannot open without.
    pub const REQUIRED: [Self; 3] = [Self::Portal, Self::Cell, Self::Local];

    /// The file's name on disk.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Portal => "client_portal.dat",
            Self::Cell => "client_cell_1.dat",
            Self::Local => "client_local_English.dat",
            Self::HighRes => "client_highres.dat",
        }
    }

    /// The id space the file holds. `client_highres.dat` is a *disjoint partition* of the portal
    /// id space rather than an override ([`crate::store`]), so it answers `Portal` too.
    #[must_use]
    pub const fn kind(self) -> DatKind {
        match self {
            Self::Portal | Self::HighRes => DatKind::Portal,
            Self::Cell => DatKind::Cell,
            Self::Local => DatKind::Local,
        }
    }

    /// The file's path inside `dir`.
    #[must_use]
    pub fn in_dir(self, dir: &Path) -> PathBuf {
        dir.join(self.file_name())
    }
}

/// One of the two data files from before Throne of Destiny (June 2005), by its name on disk. Strings
/// and interface layouts, which later moved to the language file, were in the portal file then.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreTodDat {
    /// `portal.dat`: the portal database, strings and layouts included.
    Portal,
    /// `cell.dat`: the landblocks and interior cells.
    Cell,
}

impl PreTodDat {
    /// Both, portal first.
    pub const ALL: [Self; 2] = [Self::Portal, Self::Cell];

    /// The file's name on disk.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Portal => "portal.dat",
            Self::Cell => "cell.dat",
        }
    }

    /// The file's path inside `dir`.
    #[must_use]
    pub fn in_dir(self, dir: &Path) -> PathBuf {
        dir.join(self.file_name())
    }
}

/// Whether `dir` holds a dat set from before Throne of Destiny: `portal.dat` and `cell.dat`.
#[must_use]
pub fn holds_pre_tod_dats(dir: &Path) -> bool {
    PreTodDat::ALL.iter().all(|d| d.in_dir(dir).is_file())
}

/// Whether `dir` holds the retail dats: whether `client_portal.dat` is a file in it.
#[must_use]
pub fn holds_retail_dats(dir: &Path) -> bool {
    RetailDat::Portal.in_dir(dir).is_file()
}

/// A directory [`locate_retail_dats`] found `client_portal.dat` in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatDir(PathBuf);

impl DatDir {
    /// The directory, spelled as the caller spelled the candidate.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The directory as an owned path.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// One file's path in this directory.
    #[must_use]
    pub fn file(&self, dat: RetailDat) -> PathBuf {
        dat.in_dir(&self.0)
    }

    /// Open the store over this directory: [`crate::RetailDatStore::open_dir`].
    ///
    /// # Errors
    ///
    /// Whatever `open_dir` returns: a required file is missing or does not open.
    pub fn open(&self) -> Result<crate::RetailDatStore, crate::DatError> {
        crate::RetailDatStore::open_dir(&self.0)
    }
}

impl AsRef<Path> for DatDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// No candidate directory held `client_portal.dat`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("retail dats not found: no client_portal.dat under {}", tried_list(.tried))]
pub struct DatsNotFound {
    /// Every directory looked in, in the order they were tried.
    pub tried: Vec<PathBuf>,
}

fn tried_list(tried: &[PathBuf]) -> String {
    if tried.is_empty() {
        return "any directory (none was given)".to_owned();
    }
    tried
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first of `candidates` that holds `client_portal.dat`.
///
/// # Errors
///
/// [`DatsNotFound`], naming every candidate, when none holds it.
pub fn locate_retail_dats(candidates: &[PathBuf]) -> Result<DatDir, DatsNotFound> {
    candidates
        .iter()
        .find(|dir| holds_retail_dats(dir))
        .map(|dir| DatDir(dir.clone()))
        .ok_or_else(|| DatsNotFound {
            tried: candidates.to_vec(),
        })
}

/// The directories declared read-only by [`protect_install`], canonical.
fn protected() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    static PROTECTED: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    PROTECTED
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Declare `dir` a read-only install for the rest of this process: [`crate::DatWriter`] refuses
/// to open any file in it, and the containers read out of it stay open once walked, so a process
/// that reopens the same install many times walks each file once.
///
/// Nothing calls this on its own; a process that keeps a pristine copy of the files (a test run
/// reading the one install it was pointed at) calls it for that copy. A directory that does not
/// exist is ignored.
pub fn protect_install(dir: &Path) {
    let Ok(dir) = dir.canonicalize() else {
        return;
    };
    let mut set = protected();
    if !set.contains(&dir) {
        set.push(dir);
    }
}

/// Whether `path` (canonical, or a file that may not exist yet) is in a directory
/// [`protect_install`] named.
pub(crate) fn is_protected(path: &Path) -> bool {
    let dir = path.parent().unwrap_or(path);
    let Ok(dir) = dir.canonicalize() else {
        return false;
    };
    protected().iter().any(|p| dir.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_candidate_holding_the_marker_wins_and_a_miss_names_every_candidate() {
        let root = std::env::temp_dir().join(format!("dereth-locate-{}", std::process::id()));
        let (a, b, c) = (root.join("a"), root.join("b"), root.join("c"));
        for d in [&a, &b, &c] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(RetailDat::Portal.in_dir(&b), b"").unwrap();
        std::fs::write(RetailDat::Portal.in_dir(&c), b"").unwrap();
        let found = locate_retail_dats(&[a.clone(), b.clone(), c.clone()]).unwrap();
        assert_eq!(found.path(), b);

        let miss = locate_retail_dats(&[a.clone(), root.join("absent")]).unwrap_err();
        let text = miss.to_string();
        assert!(text.starts_with("retail dats not found"), "{text}");
        assert!(text.contains(&a.display().to_string()), "{text}");
        assert!(text.contains("absent"), "{text}");
        assert!(locate_retail_dats(&[]).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_protected_install_covers_files_inside_it_and_nothing_beside_it() {
        let root = std::env::temp_dir().join(format!("dereth-protect-{}", std::process::id()));
        let (inside, beside) = (root.join("install"), root.join("copy"));
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&beside).unwrap();
        protect_install(&inside);
        assert!(is_protected(&RetailDat::Local.in_dir(&inside)));
        assert!(!is_protected(&RetailDat::Local.in_dir(&beside)));
        let _ = std::fs::remove_dir_all(&root);
    }
}
