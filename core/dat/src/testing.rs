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
//! The search itself is [`crate::locate_retail_dats`]; this module only supplies the candidate.
//! The directory found is declared read-only for the run ([`crate::protect_install`]), so no test
//! can write to the install every other test reads.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::{locate_retail_dats, DatDir, DatsNotFound, RetailDat, RetailDatStore};

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
            let found = locate_retail_dats(&candidates());
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
pub fn dat_file(dat: RetailDat) -> PathBuf {
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
pub const PRE_TOD_DAT_DIR_VAR: &str = "DERETH_TEST_PRETOD_DAT_DIR";

/// The directory `DERETH_TEST_PRETOD_DAT_DIR` names, or `None` when it is unset or empty.
#[must_use]
pub fn pre_tod_dat_dir() -> Option<PathBuf> {
    std::env::var_os(PRE_TOD_DAT_DIR_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Why the February 2005 dats cannot be read, or `None` when both files are there.
#[must_use]
pub fn pre_tod_shortfall() -> Option<String> {
    match pre_tod_dat_dir() {
        None => Some(format!(
            "the February 2005 dats were not found: {PRE_TOD_DAT_DIR_VAR} is unset (set it to \
             the directory holding portal.dat and cell.dat)"
        )),
        Some(dir) if !crate::holds_pre_tod_dats(&dir) => Some(format!(
            "the February 2005 dats were not found: no portal.dat and cell.dat under {} \
             ({PRE_TOD_DAT_DIR_VAR})",
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
/// With [`pre_tod_shortfall`]'s line when the files are not there, or with the open error when
/// they are and do not open.
#[must_use]
pub fn open_pre_tod_store_or_fail() -> RetailDatStore {
    if let Some(msg) = pre_tod_shortfall() {
        panic!("{msg}");
    }
    let dir = pre_tod_dat_dir().unwrap_or_default();
    crate::protect_install(&dir);
    RetailDatStore::open_pre_tod_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "the February 2005 dats under {} did not open: {e}",
            dir.display()
        )
    })
}
