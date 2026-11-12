//! The remaster's own effects over retail's light, each off until it is asked for, and kept while
//! the remaster is off: the ambient occlusion, the post passes (the scene in linear light, the
//! anti-aliasing, the bloom and the tone map), the water and the puddles, and the upscaler.

use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

/// A skinned body as the remaster draws it, and what its subsets name as their surfaces.
pub use dereth_remaster::assets::{
    skin::SkinModel as RemasterSkinBody, SurfaceRef as RemasterSurfaceRef,
};
/// How the remaster's ambient occlusion is found.
pub use dereth_remaster::render::ao::AoSettings as RemasterAo;
/// How the remaster's post passes draw, and one picture of them read back.
pub use dereth_remaster::render::post::{
    PostPicture as RemasterPostPicture, PostSettings as RemasterPost,
};
/// How the remaster upscales its frame, and which upscaler draws it.
pub use dereth_remaster::render::upscale::{
    UpscaleMode as RemasterUpscaleMode, UpscaleSettings as RemasterUpscale,
};
/// The remaster's skinned bodies drawn, by their meshes' ids.
pub type RemasterSkinBodies = std::collections::BTreeMap<u32, std::sync::Arc<RemasterSkinBody>>;
/// How the remaster draws the water and the puddles.
pub use dereth_remaster::render::water::WaterSettings as RemasterWater;

impl SceneDraw {
    /// How the remaster draws the water and the puddles, from its next frame on, and from when
    /// it is turned on.
    pub fn set_remaster_water(&mut self, settings: RemasterWater, gpu: &mut Gpu) {
        self.remaster_view.get_mut().water = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_water(settings);
        }
    }

    /// How the remaster draws the water and the puddles.
    #[must_use]
    pub fn remaster_water(&self) -> RemasterWater {
        self.remaster_view.borrow().water
    }

    /// How the remaster finds its ambient occlusion, from its next frame on, and from when it is
    /// turned on.
    pub fn set_remaster_ao(&mut self, settings: RemasterAo, gpu: &mut Gpu) {
        self.remaster_view.get_mut().ao = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_ao(settings);
        }
    }

    /// How the remaster finds its ambient occlusion.
    #[must_use]
    pub fn remaster_ao(&self) -> RemasterAo {
        self.remaster_view.borrow().ao
    }

    /// The remaster's last frame of ambient occlusion at the world's size, row by row, 1 open
    /// and 0 shut in; `None` while the remaster or the occlusion is off. The device waits for its
    /// work first.
    #[must_use]
    pub fn remaster_ao_picture(&self, gpu: &mut Gpu) -> Option<(u32, u32, Vec<f32>)> {
        remaster(gpu)?.renderer().read_ao()
    }

    /// Whether the remaster poses skinned bodies on the device (on by default) or on the
    /// processor, for an instrument comparing the two.
    pub fn set_remaster_skin_gpu(&mut self, on: bool, gpu: &mut Gpu) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_skin_gpu(on);
        }
    }

    /// What each server object's body last played of its own clips, by the object's id: the name
    /// of the slot whose clip it played (`None` while it stands at rest), how far through it it
    /// was drawn, and whether the speed its motion asks paced it. For an instrument to read.
    pub fn remaster_skin_playing(
        &mut self,
        gpu: &mut Gpu,
    ) -> Vec<(u32, Option<&'static str>, f32, bool)> {
        remaster(gpu).map_or_else(Vec::new, |r| {
            r.renderer()
                .skin_playing()
                .into_iter()
                .filter_map(|(key, a, s, p)| match key {
                    dereth_remaster::scene::ObjectKey::Server(id) => {
                        Some((id, a.map(dereth_remaster::assets::skin::Slot::name), s, p))
                    }
                    _ => None,
                })
                .collect()
        })
    }

    /// Whether the remaster draws a skinned body still dying when its object leaves on to the end
    /// of its death (on by default), for an instrument showing what that adds.
    pub fn set_remaster_skin_ghosts(&mut self, on: bool, gpu: &mut Gpu) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_skin_ghosts(on);
        }
    }

    /// The server objects whose bodies are drawn on as ghosts after they left, still dying, and
    /// the corpses hidden under them, by their ids. For an instrument to read.
    pub fn remaster_skin_ghosts(&mut self, gpu: &mut Gpu) -> (Vec<u32>, Vec<u32>) {
        let ids = |keys: Vec<dereth_remaster::scene::ObjectKey>| {
            keys.into_iter()
                .filter_map(|k| match k {
                    dereth_remaster::scene::ObjectKey::Server(id) => Some(id),
                    _ => None,
                })
                .collect()
        };
        remaster(gpu).map_or_else(
            || (Vec::new(), Vec::new()),
            |r| {
                let (ghosts, hidden) = r.renderer().skin_ghosts();
                (ids(ghosts), ids(hidden))
            },
        )
    }

    /// The remaster's skinned bodies drawn, by their meshes' ids: for an instrument.
    pub fn remaster_skin_bodies(&mut self, gpu: &mut Gpu) -> Option<RemasterSkinBodies> {
        Some(remaster(gpu)?.renderer().skin_bodies())
    }

    /// Whether the remaster's motion pass follows a skinned body vertex by vertex (on by
    /// default), for an instrument showing what that adds.
    pub fn set_remaster_skin_motion(&mut self, on: bool, gpu: &mut Gpu) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_skin_motion(on);
        }
    }

    /// How the remaster's post passes draw, from its next frame on, and from when it is turned
    /// on.
    pub fn set_remaster_post(&mut self, settings: RemasterPost, gpu: &mut Gpu) {
        self.remaster_view.get_mut().post = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_post(settings);
        }
    }

    /// How the remaster's post passes draw.
    #[must_use]
    pub fn remaster_post(&self) -> RemasterPost {
        self.remaster_view.borrow().post
    }

    /// How the remaster upscales its frame, from its next frame on, and from when it is turned on.
    pub fn set_remaster_upscale(&mut self, settings: RemasterUpscale, gpu: &mut Gpu) {
        self.remaster_view.get_mut().upscale = settings;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_upscale(settings);
        }
    }

    /// How the remaster upscales its frame.
    #[must_use]
    pub fn remaster_upscale(&self) -> RemasterUpscale {
        self.remaster_view.borrow().upscale
    }

    /// Frames the remaster's upscaler has drawn since it was turned on; 0 while it is off.
    #[must_use]
    pub fn remaster_upscaled_frames(&self, gpu: &mut Gpu) -> u64 {
        remaster(gpu).map_or(0, |r| r.renderer().upscaled_frames())
    }

    /// Frames MetalFX's scaler has scaled since the remaster started; 0 while it is off or where
    /// there is no Metal.
    #[must_use]
    pub fn remaster_metalfx_frames(&self, gpu: &mut Gpu) -> u64 {
        remaster(gpu).map_or(0, |r| r.renderer().metalfx_frames())
    }

    /// The remaster's last frame of the post passes: the scene's colors, the colors shown and the
    /// moving objects' motion; `None` while the remaster or the passes are off. The device waits
    /// for its work first.
    #[must_use]
    pub fn remaster_post_picture(&self, gpu: &mut Gpu) -> Option<RemasterPostPicture> {
        remaster(gpu)?.renderer().read_post()
    }
}

/// The extensions of Vulkan's instance and device the remaster's DLSS needs: none unless its DLL
/// stands beside the client.
#[cfg(all(feature = "dlss", windows))]
#[must_use]
pub fn remaster_device_extensions() -> (
    &'static [&'static std::ffi::CStr],
    &'static [&'static std::ffi::CStr],
) {
    dereth_remaster::render::dlss::extensions()
}

/// Whether NVIDIA's DLSS library stands beside the client, so the remaster can load it.
#[cfg(all(feature = "dlss", windows))]
#[must_use]
pub fn remaster_dlss_carried() -> bool {
    dereth_remaster::render::dlss::dll_beside_client()
}

/// The upscaler that drew the remaster's last frame: `none`, `FSR`, `DLSS` or `MetalFX`.
#[must_use]
pub fn remaster_upscaler_drawn() -> &'static str {
    dereth_remaster::render::upscale::drawn_now()
}

/// Ends the process now with `code` where the remaster's DLSS library was started in it, once
/// everything of the client's own is closed; returns where it was not. See
/// `dereth_remaster::render::dlss::end_process_if_started`.
#[cfg(all(feature = "dlss", windows))]
pub fn remaster_end_process(code: u32) {
    dereth_remaster::render::dlss::end_process_if_started(code);
}
