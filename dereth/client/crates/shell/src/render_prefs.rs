//! The `Render.*` preference owners, over the scene's: every item of
//! [`dereth_scene::render_prefs`] is glob-re-exported here, so
//! `dereth_client::render_prefs::RenderPreferences` and its siblings resolve here too. What this
//! module adds is the Apply over the whole renderer, which holds both owners.

pub use dereth_scene::render_prefs::*;

/// The options page's Apply, for every `Render.*` name this module owns.
///
/// Retail has no per-name apply at all: every
/// registration passes the same render-preference-changed callback, which only recomputes the
/// cached overall graphics quality; the next frame's preference poll performs the work.
/// So the page writes the variable and the renderer
/// notices — which is what this does: the gpu-owned names land on the device, the scene-owned
/// ones land on [`dereth_scene::world::SceneConfig::render`], and `view_params` and the part pass read
/// them on the next frame.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub fn apply_preference_requests(
    renderer: &mut crate::gpu::Renderer,
    requests: Vec<dereth_ui_screens::UiRequest>,
) -> Vec<dereth_ui_screens::UiRequest> {
    let (world, gpu) = renderer.world_mut_and_gpu();
    let remaining = apply_gpu_preference_requests(gpu, requests);
    let Some(world) = world else { return remaining };
    apply_scene_preference_requests(&mut world.cfg.render, remaining)
}
