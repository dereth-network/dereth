//! What a scene hands a renderer each frame and the renderer starts up with: the projection
//! parameters, the fog and light blocks and the startup sampler filtering, naming no game and no
//! graphics API.
//!
//! [`camera`] holds the projection parameters and `ViewParams`, which carries them with the fog and
//! light blocks; [`sampler`] the filtering the renderer comes up on.
//!
//! These are plain values that both sides of a renderer agree on: the scene or runtime that fills
//! them, which may not depend on a renderer, and the renderer that turns them into matrices and
//! device state. The arithmetic that does that turning is the renderer's (`dereth-render-cpu`).
//!
//! **Depends on** `dereth-primitives` (the viewport and the projection's trigonometry) and `glam`.
//! **Used by** the device-free half of rendering (`dereth-render-cpu`), which re-exports both
//! modules and is what both games' clients draw with, and the first game's contract
//! (`dereth-client-contract`), which re-exports them for its runtime and scenes.
//!
//! **Must never** name either game, a graphics API or the platform (`cargo xtask seams`, `seam:
//! the drawing both games share names no game`).

#![forbid(unsafe_code)]

/// The projection parameters, the fog and light blocks and `ViewParams`.
pub mod camera;
/// `STARTUP_FILTERING`, the texture filtering the renderer comes up on.
pub mod sampler;
