//! The remaster's passes that name nothing of the game: the G-buffer's shared layout and the
//! passes that read it, for any game's remaster to draw with.
//!
//! A remaster draws its world into a G-buffer of the layout [`gbuffer`] names (each surface's
//! color, normal, light and material, the material's first byte its light class), with the
//! frame's constants in the rows [`frame`] names. Everything here reads only that: the pictures,
//! the depth and the constants, never a scene, an asset or a light rule of a game. The game's own
//! remaster decides what is drawn and how it is lit, and asks each pass for the masks its resolve
//! takes.
//!
//! **Depends on** `dereth-gfx` (the backend layer) and `glam`. **Used by** the remasters
//! (`dereth-remaster`, `auberean-remaster`), and nothing else.
//!
//! **Must never** reach a game: no scene, asset, dat, light rule, simulation or platform. A pass
//! that needs something of the game takes it as plain data from its caller.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`gbuffer`] | the G-buffer's targets, their formats and the light classes |
//! | [`frame`] | the camera, the rows of a constants buffer, a buffer rewritten every frame, the push words, the last frame's reprojection |
//! | [`ao`] | the ambient occlusion: how much of the sky each pixel's surroundings leave it |
//! | [`lamps`] | the lamps' lists over the pieces of the view, the target of their light, and the halos round them |
//! | [`post`] | the scene's target of its own, the temporal anti-aliasing, the bloom, the exposure and the tone curve |
//! | [`sun`] | the sun's cascades, their maps, and how much of the sun reaches each pixel it lights |
//! | [`shaders`] | the shaders, compiled ahead of time and embedded: each entry point's blobs |
//!
//! The Slang modules in the crate's `shaders/` folder (`bindless`, `frame`) are the vocabulary a
//! game's own shaders import, so its G-buffer and resolve speak the same layout as the passes.

pub mod ao;
pub mod frame;
pub mod gbuffer;
pub mod lamps;
pub mod post;
pub mod shaders;
pub mod sun;
