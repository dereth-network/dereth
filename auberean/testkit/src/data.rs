//! Where the private test data is.
//!
//! - The retail data files, the end-of-retail set: `auberean_dat::testing`
//!   (`AUBEREAN_TEST_DAT_DIR`), re-exported here.
//! - The capture index (the recorded retail sessions, one SQLite file): `AUBEREAN_TEST_CAPTURE_INDEX`,
//!   and nowhere else. A test that needs it fails, naming the variable, when it is not found
//!   ([`require_capture_index`]).

use std::path::PathBuf;

pub use auberean_dat::testing::{
    install, require_install, require_target_install, shortfall, CHECKED, END_OF_RETAIL, TARGET,
};

/// The variable naming the capture index file.
pub const CAPTURE_INDEX_VAR: &str = "AUBEREAN_TEST_CAPTURE_INDEX";

/// The capture index, if it is found.
#[must_use]
pub fn capture_index() -> Option<PathBuf> {
    std::env::var_os(CAPTURE_INDEX_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

/// Why the capture index cannot be read, as the line a failed test prints, or `None` when it is
/// found.
#[must_use]
pub fn capture_index_shortfall() -> Option<String> {
    match std::env::var_os(CAPTURE_INDEX_VAR).filter(|v| !v.is_empty()) {
        None => Some(format!(
            "the capture index was not found: {CAPTURE_INDEX_VAR} is unset (set it to the index \
             of the recorded sessions)"
        )),
        Some(p) if !PathBuf::from(&p).is_file() => Some(format!(
            "the capture index was not found: no file at {} ({CAPTURE_INDEX_VAR})",
            PathBuf::from(p).display()
        )),
        Some(_) => None,
    }
}

/// The capture index.
///
/// # Panics
///
/// With [`capture_index_shortfall`]'s line when it is not found.
#[must_use]
pub fn require_capture_index() -> PathBuf {
    if let Some(msg) = capture_index_shortfall() {
        panic!("{msg}");
    }
    capture_index().unwrap_or_default()
}
