//! Ambient occlusion by rays traced over each pixel's hemisphere through the scene's acceleration
//! structures, where the device traces rays: the shared pass (`dereth_gfx_passes::traced`),
//! standing in for the screen-space occlusion (pass 5) at the radius, falloff and strength the
//! occlusion's own settings give, and the renderer's hold on it. Where the device does not trace
//! rays, or the occlusion is off, the screen-space occlusion draws it as ever.
//!
//! Its shader is the shared `rt_ao.slang`, compiled in this crate's folder against its own `rt`
//! module.

pub use dereth_gfx_passes::traced::{RtAoPass, RtAoPlan, RtAoSettings};

impl super::Renderer {
    /// How the occlusion is traced from the next frame on.
    pub fn set_rt_ao(&mut self, settings: RtAoSettings) {
        self.rt_ao.set_settings(settings);
    }

    /// How the occlusion is traced.
    #[must_use]
    pub fn rt_ao(&self) -> RtAoSettings {
        self.rt_ao.settings()
    }

    /// Frames the shadows and the occlusion were traced in, since the renderer was made.
    #[must_use]
    pub fn traced_frames(&self) -> (u64, u64) {
        (self.rt_shadow.frames(), self.rt_ao.frames())
    }
}
