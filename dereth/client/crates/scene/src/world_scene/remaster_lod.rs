//! The remaster's levels of detail as the scene chooses how they are picked, and reads back what
//! the last frame's choice did.

pub use dereth_remaster::render::impostor::ImpostorStats as RemasterImpostorStats;
use dereth_remaster::render::impostor::ImpostorStats;
pub use dereth_remaster::render::lod::{LodSettings as RemasterLod, LodStats as RemasterLodStats};
use dereth_remaster::render::lod::{LodSettings, LodStats};
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

impl SceneDraw {
    /// How the remaster picks its parts' levels, from the next frame on.
    pub fn set_remaster_lod(&mut self, lod: LodSettings, gpu: &mut Gpu) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_lod(lod);
        }
    }

    /// What the remaster's last choice of levels did; `None` while it is off.
    #[must_use]
    pub fn remaster_lod_stats(&self, gpu: &mut Gpu) -> Option<LodStats> {
        let r = remaster(gpu)?;
        Some(r.renderer().lod().stats())
    }

    /// What the far forest did in the remaster; `None` while it is off.
    #[must_use]
    pub fn remaster_forest_stats(
        &self,
        gpu: &mut Gpu,
    ) -> Option<super::remaster_pack::RemasterForestStats> {
        let r = remaster(gpu)?;
        Some(r.renderer().forest_stats())
    }

    /// What the remaster's impostors did; `None` while it is off.
    #[must_use]
    pub fn remaster_impostor_stats(&self, gpu: &mut Gpu) -> Option<ImpostorStats> {
        let r = remaster(gpu)?;
        Some(r.renderer().impostor_stats())
    }
}
