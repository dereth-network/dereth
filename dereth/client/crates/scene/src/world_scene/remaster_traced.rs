//! The remaster's sun shadows and ambient occlusion traced through its acceleration structures,
//! where the device traces rays, as the scene asks for them; kept while the remaster is off.

pub use dereth_remaster::render::rt_ao::RtAoSettings as RemasterTracedAo;
pub use dereth_remaster::render::rt_shadow::RtShadowSettings as RemasterTracedShadows;
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

impl SceneDraw {
    /// Whether the remaster traces its sun's shadows, from its next frame on, and from when it is
    /// turned on. Where its device does not trace rays the cascades draw them.
    pub fn set_remaster_traced_shadows(&mut self, settings: RemasterTracedShadows, gpu: &mut Gpu) {
        self.remaster_view.get_mut().traced_shadows = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_rt_shadows(settings);
        }
    }

    /// Whether the remaster traces its ambient occlusion, from its next frame on, and from when
    /// it is turned on. Where its device does not trace rays the screen-space occlusion draws it.
    pub fn set_remaster_traced_ao(&mut self, settings: RemasterTracedAo, gpu: &mut Gpu) {
        self.remaster_view.get_mut().traced_ao = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_rt_ao(settings);
        }
    }

    /// How many of the remaster's frames traced the sun's shadows and the occlusion, since it was
    /// turned on; `None` while it is off.
    #[must_use]
    pub fn remaster_traced_frames(&self, gpu: &mut Gpu) -> Option<(u64, u64)> {
        Some(remaster(gpu)?.renderer().traced_frames())
    }
}
