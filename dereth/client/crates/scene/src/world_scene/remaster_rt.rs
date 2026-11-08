//! The remaster's acceleration structures, where the device traces rays: whether it can, what
//! they hold, and a check of them against the picture the remaster drew; and the bounced light,
//! which traces against them where it can.

use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

/// How the remaster's bounced light is found.
pub use dereth_remaster::render::gi::GiSettings as RemasterGi;
/// What the remaster's acceleration structures hold.
pub use dereth_remaster::render::rt::RtCounts as RemasterAcceleration;

impl SceneDraw {
    /// How the remaster finds its bounced light, from its next frame on, and from when it is
    /// turned on.
    pub fn set_remaster_gi(&mut self, settings: RemasterGi, gpu: &mut Gpu) {
        self.remaster_view.get_mut().gi = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_gi(settings);
        }
    }

    /// How the remaster finds its bounced light.
    #[must_use]
    pub fn remaster_gi(&self) -> RemasterGi {
        self.remaster_view.borrow().gi
    }

    /// Keep the remaster's acceleration structures, built every frame, whether or not a pass of
    /// its frame traces rays. Nothing while the remaster is off, or where the device does not
    /// trace rays.
    pub fn set_remaster_acceleration(&mut self, on: bool, gpu: &mut Gpu) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_acceleration(on);
        }
    }

    /// Whether the remaster's device traces rays; `false` while it is off.
    #[must_use]
    pub fn remaster_traces_rays(&self, gpu: &mut Gpu) -> bool {
        remaster(gpu).is_some_and(|r| r.renderer().traces_rays())
    }

    /// What the remaster's acceleration structures held at its last frame; `None` while it is
    /// off.
    #[must_use]
    pub fn remaster_acceleration(&self, gpu: &mut Gpu) -> Option<RemasterAcceleration> {
        remaster(gpu).map(|r| r.renderer().acceleration_counts())
    }

    /// The remaster's last frame's acceleration structures checked against its picture: rays
    /// from the eye through every `step`th pixel a side, with mask `mask`, each one's hit
    /// distance (negative for a miss) and its instance's index of the user's, row by row. `None`
    /// while it is off or built none.
    #[must_use]
    pub fn remaster_acceleration_check(
        &self,
        gpu: &mut Gpu,
        step: u32,
        mask: u8,
    ) -> Option<Vec<(f32, u32)>> {
        let r = remaster(gpu)?;
        match r.renderer().acceleration_check(step, mask) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(target: "dereth::remaster", "the acceleration check: {e}");
                None
            }
        }
    }
}
