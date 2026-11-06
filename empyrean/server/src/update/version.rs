//! Release versions, read from Empyrean's release tags.

pub use dereth_server_update::version::Version;

/// The prefix of every Empyrean release tag (`empyrean-v0.1.0`).
pub const TAG_PREFIX: &str = "empyrean-v";

/// The version a release tag names (`empyrean-v0.1.0`); `None` for any other tag.
#[must_use]
pub fn from_tag(tag: &str) -> Option<Version> {
    Version::from_tag(TAG_PREFIX, tag)
}
