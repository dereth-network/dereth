//! The client's drawn world: the landblock window, its objects, the sky, the particles and the
//! preview spaces, drawn on the graphics device.
//!
//! **Depends on** the runtime it draws (`dereth-client-runtime`) and the crates the drawing is
//! built from: `dereth-render` (the device), `dereth-world-render` (what the world draws and in what
//! order), `dereth-terrain`, `dereth-primitives`, `dereth-dat`, `dereth-assets`, `dereth-physics`, `dereth-animation`,
//! `dereth-audio`, `dereth-protocol` and `dereth-world-data`, the target projection from the contract
//! (`dereth-client-contract`) and character creation's model (`dereth-chargen`). **Used by** the client shell
//! (`dereth-client-shell`), the desktop client (`dereth-client`) and the browser client
//! (`dereth-web`).
//!
//! **Must never** reach a platform: no window, no event loop, no sound device and no operating
//! system call (`cargo xtask seams`, `seam: application halves`). The graphics device is
//! `dereth_render::device::Gpu`, on whichever backend the build has; the drawing is gated on there
//! being one, and without one the crate holds only the scene's plain types.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`world_scene`] | the landblock window, its objects and how a frame draws them |
//! | [`gpu`] | the device with the world and the preview spaces on it, and the first-pixel quad |
//! | [`sky`], [`particles`] | the sky dome and the particle emitters |
//! | [`preview`] | the creature-mode preview spaces |
//! | [`textures`], [`mip_worker`] | the dat texture lookup, and compressed mip chains built off thread |
//! | [`render_prefs`] | the `Render.*` owners on the device and on the scene |
//! | [`camera`] | camera comparisons against the renderer's view matrix |

/// Compare runtime cameras against the renderer's view matrix.
pub mod camera;
/// The device with the drawn world and the preview spaces on it.
pub mod gpu;
/// Building compressed textures' mip chains off the main thread.
pub mod mip_worker;
/// Presentation: the emitter geometry it caches is device-resident, so the whole module is
/// gated with the other presentation modules.
#[cfg(gpu)]
pub mod particles;
/// `impl PickScene for WorldScene`, the half of picking that names this crate's own scene
/// type. Gated exactly as `WorldScene` is.
#[cfg(gpu)]
mod pick_scene;
/// The paper-doll preview space.
pub mod preview;
/// The device- and scene-owned halves of the `Render.*` preferences, over a glob re-export of
/// [`dereth_client_runtime::render_prefs`].
pub mod render_prefs;
pub mod sky;
pub mod textures;
/// The drawn world. The simulation and the residency window are `dereth-client-runtime`'s; this
/// is the presentation half, and the type it exists for is `WorldScene`.
pub mod world_scene;
