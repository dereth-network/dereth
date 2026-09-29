//! Helpers shared by the modules of this binary.
//!
//! [`paths`] holds the fixture-path helpers that used to be copied into every test file. The `dat`
//! and `gpu` binaries have their own common modules and import this helper file and
//! [`corpus_size`] individually with `#[path]`, leaving one source definition of each.

pub mod paths;

pub use paths::*;

/// The recorded corpus's own sizes, so that a promotion into `fixtures/message-corpus` is one
/// regeneration of the fixture set and not an edit to every suite that counts it.
pub mod corpus_size;

pub use corpus_size::*;

/// When a recording's client entered the world and as whom, read from its own datagrams, so a
/// replay re-enters on every cycle the recording did rather than once. The reader is the
/// workspace's shared one.
pub use dereth_client_net::recording::{recorded_enter_world_requests, RecordedEntry};
