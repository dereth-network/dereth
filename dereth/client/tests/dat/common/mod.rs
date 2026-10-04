//! Helpers shared by the modules of this binary.
//!
//! [`paths`] and [`corpus_size`] are helper files from the `cpu` tree, declared here separately
//! with `#[path]` rather than copied. The test binaries have their own common modules but share
//! these source definitions.

#[path = "../../cpu/common/paths.rs"]
pub mod paths;

pub use paths::*;

/// The `cpu` binary's corpus-size helpers, declared here the same way.
#[path = "../../cpu/common/corpus_size.rs"]
pub mod corpus_size;

pub use corpus_size::*;

/// The shared collision-walk probe: walks a body at an obstacle and judges where it came to rest.
/// Used by several modules of this binary.
pub mod collision_probe;

/// A headless `App` in gameplay over the retail dats, shared by the panel modules.
pub mod app;

/// A world with no device, for the modules whose claim is about the simulation and reads no pixel.
pub mod sim;

/// The real client shell and frame loop over a device-free world presentation.
pub mod sim_app;

#[path = "../../common/chat.rs"]
pub mod chat;
