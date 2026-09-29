//! Integration contracts grouped by subject.
use std::path::{Path, PathBuf};

/// `empyrean-content`'s patch fixtures (a hand-written dump, SQL patches and JSON content).
pub fn patch_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/content/tests/fixtures/patches")
}
mod import;

mod support;
use support::world_dump_fixture as fixture;
