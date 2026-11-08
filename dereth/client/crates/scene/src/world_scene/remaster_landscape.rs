//! What the remaster adds to the landscape past retail's own pictures, each on its own box.

use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

/// Which of the landscape's additions the remaster draws.
pub use dereth_remaster::render::landscape::LandscapeSettings as RemasterLandscape;

impl SceneDraw {
    /// Which of the landscape's additions the remaster draws, from its next frame on, and from when
    /// it is turned on.
    pub fn set_remaster_landscape(&mut self, settings: RemasterLandscape, gpu: &mut Gpu) {
        self.remaster_view.get_mut().landscape = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_landscape(settings);
        }
    }

    /// Which of the landscape's additions the remaster draws.
    #[must_use]
    pub fn remaster_landscape(&self) -> RemasterLandscape {
        self.remaster_view.borrow().landscape
    }
}
