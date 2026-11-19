//! `STARTUP_FILTERING`: the sampler-filtering preference the renderer comes up on.
//!
//! `dereth_render_cpu::sampler` re-exports it (and `dereth_render::sampler` re-exports that), as
//! does `dereth_client_contract::sampler`. It is the render preferences' default for
//! `Render.TextureFiltering` and the answer for a profile that does not name it, so the first
//! game's runtime reads it too; the rest of the sampler policy (the `Filter` enum, the capability
//! bits and `resolve`) is the renderer's and stays there.

/// Startup selects overall graphics quality 3 before loading preferences.
/// This is NOT the options panel's Restore Defaults value1.
pub const STARTUP_FILTERING: u32 = 0;
