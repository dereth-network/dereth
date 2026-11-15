//! The sun's shadows by rays traced toward it through the scene's acceleration structures, where
//! the device traces rays: the shared pass (`dereth_gfx_passes::traced`), standing in for the
//! cascades' look-up (pass 7), and the renderer's hold on it. Where the device does not trace
//! rays, or the shadows are off, the cascades draw them as ever.
//!
//! Its shader is the shared `rt_shadow.slang`, compiled in this crate's folder against its own
//! `rt` module.

pub use dereth_gfx_passes::traced::{RtShadowPass, RtShadowPlan, RtShadowSettings};

impl super::Renderer {
    /// How the shadows are traced from the next frame on.
    pub fn set_rt_shadows(&mut self, settings: RtShadowSettings) {
        self.rt_shadow.set_settings(settings);
    }

    /// How the shadows are traced.
    #[must_use]
    pub fn rt_shadows(&self) -> RtShadowSettings {
        self.rt_shadow.settings()
    }
}
