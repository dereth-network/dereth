//! Helpers shared by the modules of this binary.
//!
//! [`paths`] is the `cpu` binary's file, declared here with `#[path]` rather than copied: the
//! binaries need the same helpers, and there is one definition of each.

#[path = "../../cpu/common/paths.rs"]
pub mod paths;

pub use paths::*;

/// The `cpu` binary's corpus-size helpers, declared here the same way.
#[path = "../../cpu/common/corpus_size.rs"]
pub mod corpus_size;

pub use corpus_size::*;

/// Where a recording's client entered the world and as whom, so a replay re-enters on every cycle
/// the recording did. The reader is the workspace's shared one.
pub use dereth_client_net::recording::recorded_enter_world_requests;

/// The binary's single device lock. It is this binary's own file and not a `#[path]`
/// into `cpu`'s: the `cpu` and `dat` binaries create no device and have nothing to serialise.
pub mod gpu;

pub use gpu::gpu_lock;

/// The test device and the retail dats, opened the one way the tier's stations open them.
pub mod device;

pub use device::{dat_store, dats, test_gpu};

/// Real application construction and input helpers.
pub mod app;

#[path = "../../common/chat.rs"]
pub mod chat;
