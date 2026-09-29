//! Shared inputs and adapters for this test tier.

use std::path::{Path, PathBuf};

/// The workspace root, two levels above this crate: where the checkers these tests run live, and
/// the tree they check.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
