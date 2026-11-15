//! The remaster: the world drawn on the shared backend layer and passes, beside the retail device
//! and never instead of it, the game's own light model under the passes' masks.
//!
//! A frame arrives from `auberean-draw` whole, as the retail device takes it. The remaster keeps
//! what stays from frame to frame on the device (the meshes' vertices and indices, the pictures),
//! writes each frame's light and each draw's constants as the retail device's shaders read them
//! (`auberean_draw::constants`), and draws the world's steps in the retail order with the game's
//! own programs ported to Slang (`shaders/light.slang`, `mesh.slang`, `terrain.slang`). With
//! every mask of the shared passes at its identity the frame is the retail one; the passes come
//! in over it one at a time.
//!
//! **Depends on** `dereth-gfx` (the backend layer), `dereth-gfx-passes` (the passes and the
//! frame's shared rows), `auberean-draw` (what a frame draws, and its constants),
//! `auberean-world` (the light and the arithmetic), `dereth-primitives` (the transcendental
//! functions) and `glam`. **Used by** the offline world viewer (`auberean-viewer`).
//!
//! **Must never** reach the simulation, the network, the shell, the scene or a platform, or
//! decide what to draw: it draws the frame it is handed, and changes only pixels. The retail
//! device (`auberean-gpu`) never depends on it, and it never on the retail device.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`render`] | the remaster: its device, what it keeps there, and each frame's draws |
//! | [`shaders`] | the shaders, compiled ahead of time and embedded: each entry point's blobs |
//! | `shared` | the one Vulkan device wgpu and the remaster's layer share, and its pictures lent to wgpu (not on macOS yet) |

pub mod render;
pub mod shaders;
#[cfg(not(target_arch = "wasm32"))]
pub mod shared;

pub use render::{Picture, Remaster};
