//! `STARTUP_FILTERING` — the sampler-filtering preference the renderer comes up on.
//!
//! `dereth_render_cpu::sampler` re-exports it (and `dereth_render::sampler` re-exports that). It is
//! `RenderPreferences::default`'s value for `Render.TextureFiltering` and `texture_filtering`'s
//! answer for a profile that does not name it, so `dereth_client_runtime::render_prefs` reads it;
//! the rest of that module — the `Filter` enum, the capability bits and `resolve` — is the
//! renderer's and stays there.

/// Startup selects overall graphics quality 3 before loading preferences.
/// This is NOT the options panel's Restore Defaults value1.
pub const STARTUP_FILTERING: u32 = 0;
