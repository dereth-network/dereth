//! Integration contracts grouped by subject.

mod content;
mod export;
mod support;
use support::world_dump_fixture as fixture;

pub(crate) fn patch_fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patches")
}
#[cfg(feature = "real-content")]
mod instruments;
