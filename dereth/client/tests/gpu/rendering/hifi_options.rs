//! What a `wgpu` device tells the options page of the experimental rendering effects: a device
//! started without them says the ray tracing the start with them has, so the lamps' box is never
//! greyed for a lack the graphics card does not have.
//!
//! Fixture: a hardware `wgpu` device off screen; no retail data.
//!
//! Behaviour: none (experimental Horizon interface)

#![cfg(all(gpu, feature = "hifi"))]

use dereth_render::device::{AdapterKind, Backend};
use dereth_render::wgpu::sidecar::wgpu;
use dereth_scene::gpu::SceneRenderer;

/// A `wgpu` device off screen, asked for the effects when `hifi` says so, as the client starts it.
fn started(hifi: bool) -> SceneRenderer {
    let r = SceneRenderer::new_on_for_hifi(Backend::Wgpu, None, 64, 64, hifi)
        .unwrap_or_else(|e| panic!("this test needs a wgpu device and none opened: {e}"));
    assert_eq!(
        r.gpu.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasteriser",
        r.gpu.adapter_name()
    );
    r
}

#[test]
fn a_wgpu_device_started_without_the_effects_says_the_ray_tracing_the_start_with_them_has() {
    let _gpu = crate::common::gpu_lock();
    let with = started(true);
    let traces = with
        .gpu
        .hifi_device_features()
        .expect("a wgpu device")
        .0
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY);
    let with = with.hifi_availability();
    let without = started(false).hifi_availability();
    assert!(with.wgpu && with.widened, "{with:?}");
    assert!(without.wgpu && !without.widened, "{without:?}");
    assert_eq!(with.rays, Some(traces), "started with the effects");
    assert_eq!(
        without.rays,
        Some(traces),
        "started without them, on the adapter that traces rays={traces}"
    );
}
