//! The `Render.*` preference owners that hold a device.
//!
//! The model half of this module -- `RenderPreferences`, the fourteen names, the choice decodes
//! and `texture_filtering` -- is [`dereth_client_runtime::render_prefs`], glob-re-exported here,
//! so `dereth_client::render_prefs::RenderPreferences` and every one of its siblings resolve here
//! too.
//!
//! What this module adds is the options page's Apply for the two names whose owner *is* the device
//! (`Render.TextureFiltering` -> `Gpu::set_texture_filtering`, `Render.ScreenBrightness` ->
//! `Gpu::set_gamma`) and the scene-owned filter beside it. The first takes a
//! `dereth_render::device::Gpu`, which is why they live in this crate; the Apply over the whole
//! renderer is the client shell's.

pub use dereth_client_runtime::render_prefs::*;

/// The `Render.*` names whose owner is [`crate::world::SceneConfig::render`] rather than the
/// device: the projection pair, multi-pass alpha, the detail levels and the two degrade knobs.
pub fn apply_scene_preference_requests(
    prefs: &mut RenderPreferences,
    requests: Vec<dereth_client_contract::UiRequest>,
) -> Vec<dereth_client_contract::UiRequest> {
    use dereth_client_contract::UiRequest;
    requests
        .into_iter()
        .filter(|r| {
            if let UiRequest::SetPreference(name, value) = r {
                // `Render.ScreenBrightness` normally never reaches here: the device consumed it
                // above, and `Gpu::gamma()` is its live owner from that point on. It stays in
                // [`RenderPreferences::set_named`] so that a build with no device still records
                // it, which is what the start-up path hands the device in the first place.
                return !prefs.set_named(name, value);
            }
            true
        })
        .collect()
}

/// The same owner at synchronous UI delivery boundaries, where the caller also holds the
/// renderer's live scene. Field-disjoint borrows avoid delaying a request or copying scene state.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub fn apply_gpu_preference_requests(
    gpu: &mut dereth_render::device::Gpu,
    requests: Vec<dereth_client_contract::UiRequest>,
) -> Vec<dereth_client_contract::UiRequest> {
    use dereth_client_contract::{PrefValue, UiRequest};
    requests
        .into_iter()
        .filter(|r| {
            match r {
                UiRequest::SetPreference(name, PrefValue::Int(value))
                    if name.eq_ignore_ascii_case(TEXTURE_FILTERING) =>
                {
                    gpu.set_texture_filtering(u32::from_ne_bytes(value.to_ne_bytes()));
                    seed_ui_registry(gpu.texture_filtering());
                    return false;
                }
                // `Render.ScreenBrightness` sets gamma, which clamps to
                // `[-0.2, 1.0]` and installs the 256-entry ramp. The
                // clamp lives in `Gpu::set_gamma`, as it does in the client's own setter, so the
                // slider's declared `-1.0 .. 1.0` range cannot push the picture past it.
                UiRequest::SetPreference(name, PrefValue::Float(value))
                    if name.eq_ignore_ascii_case(SCREEN_BRIGHTNESS) =>
                {
                    gpu.set_gamma(*value);
                    return false;
                }
                _ => {}
            }
            true // Wrong types and other owners remain visible to the caller, in order.
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_default_is_not_restore_defaults_and_shell_seed_uses_live_owner() {
        use dereth_ui_screens::{
            options::{config, store},
            PrefValue,
        };
        store::init();
        assert_eq!(store::inq_value(TEXTURE_FILTERING), Some(PrefValue::Int(0)));
        let restored = config::restore_defaults_requests()
            .into_iter()
            .find_map(|r| match r {
                dereth_ui_screens::UiRequest::SetPreference(n, v) if n == TEXTURE_FILTERING => {
                    Some(v)
                }
                _ => None,
            });
        assert_eq!(restored, Some(PrefValue::Int(1)));
        seed_ui_registry(3);
        assert_eq!(store::inq_value(TEXTURE_FILTERING), Some(PrefValue::Int(3)));
        store::init(); // real shell-rebuild registration behavior
        seed_ui_registry(2);
        assert_eq!(store::inq_value(TEXTURE_FILTERING), Some(PrefValue::Int(2)));
    }
}
