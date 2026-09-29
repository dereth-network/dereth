//! The retail data files turned into the shapes physics and animation consume, shared by the client
//! and the server.
//!
//! **Depends on** `dereth-primitives`, the container and decoders (`dereth-dat`, `dereth-assets`)
//! and the two engines it feeds (`dereth-physics`, `dereth-animation`). **Used by** the client
//! runtime (`dereth-client-runtime`, which re-exports each module at its old path), the SDK
//! (`dereth-client-sdk`) and the server (`empyrean-dat`, `empyrean-world`).
//!
//! **Must never** draw, read input, own a frame loop or know about a network session: every module
//! is an adapter, data-file records in and physics or animation structures out, so the client and
//! the server share one copy.

/// The decoded dat records turned into `dereth-animation`'s runtime shapes: the only place the decoders
/// and the animation layer meet.
pub mod anim_convert;

/// A `dereth_animation::AnimAssets` over the retail portal dat, with its record memo.
pub mod anim_assets;

/// The interior cells of a landblock, converted for physics, and the static objects baked into them.
pub mod env_cells;

/// A `dereth_physics::LandSource` over the retail cell dat.
pub mod land_source;

/// The landblock and region identities the landscape path is indexed by, and `WorldError`.
pub mod landblock;

/// Part-array initialization's answer for a setup id: whether it succeeds, and the holding locations.
pub mod physics_setup;

/// A setup record's collision half: spheres and heights, and the parts with their BSPs and bounds.
pub mod setup;
