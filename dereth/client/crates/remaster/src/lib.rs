//! The remaster: the world drawn from retained data, lit by retail's own light model from the
//! frame's light inputs.
//!
//! **Depends on** `dereth-gfx` (the backend layer), `dereth-render` (the Vulkan device's world
//! hook it draws through), `dereth-primitives` (the texture type its pictures are held in) and
//! `glam`. **Used by** the scene (`dereth-scene`), behind its `remaster` feature, and nothing
//! else.
//!
//! **Must never** reach the simulation, physics, world, network, runtime or interface crates, or
//! a platform: the scene hands it plain data ([`scene`]) after each frame's walk, and it changes
//! only pixels.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`assets`] | the meshes, materials, textures and ground surfaces drawn with, decoded once and held while used |
//! | [`look`] | the look: the adjustments laid over retail's light inputs, and where they come from |
//! | [`hook`] | the remaster hooked into the Vulkan device, and where the scene's frames wait for it |
//! | [`render`] | the retained scene on the device, and each frame's passes |
//! | [`scene`] | the scene's changes and each frame's state as plain data, and the retained scene they build |
//! | [`shaders`] | the shaders, compiled ahead of time and embedded: each entry point's blobs |

pub mod assets;
pub mod hook;
pub mod look;
pub mod render;
pub mod scene;
pub mod shaders;
