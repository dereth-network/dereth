use super::*;

// Synthetic provided mip colors isolate SAMPLING policy from the separately tested ImgTex
// generator. Device descriptors are immutable and actual shader readback, not a smoothness score.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn sampler_banks_sample_the_selected_mip_and_survive_mid_frame_changes() {
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 16,
            height: 16,
            ..Default::default()
        },
    )
    .unwrap();
    let _ = shared::sampler_bank_captures(&mut gpu);
}

/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn sampler_bias_changes_at_begin_scene_and_guarded_preview_restores_global_baseline() {
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 1,
            height: 1,
            ..Default::default()
        },
    )
    .unwrap();
    shared::sampler_bias_changes_at_begin_scene(&mut gpu);
}

// Shared source is instantiated with this backend's types and assertion helpers.
#[allow(clippy::duplicate_mod)]
#[path = "../tests/sampler.rs"]
mod shared;

fn assert_preference(gpu: &Gpu, preference: u32) {
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(
        desc.Filter,
        [
            D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            D3D12_FILTER_MIN_MAG_MIP_LINEAR,
            D3D12_FILTER_MIN_MAG_MIP_LINEAR,
            D3D12_FILTER_ANISOTROPIC
        ][preference as usize]
    );
    assert_eq!(desc.MipLODBias, if preference == 2 { -1.4 } else { 0.0 });
    assert_eq!(desc.MaxAnisotropy, D3D12_REQ_MAXANISOTROPY);
}

fn assert_point_request(gpu: &Gpu, request: u32) {
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(desc.Filter, D3D12_FILTER_MIN_MAG_MIP_POINT);
    assert_eq!(
        desc.AddressU,
        if request == 3 || request == 7 {
            D3D12_TEXTURE_ADDRESS_MODE_CLAMP
        } else {
            D3D12_TEXTURE_ADDRESS_MODE_WRAP
        }
    );
    assert_eq!(
        desc.AddressV,
        if request == 3 || request == 6 {
            D3D12_TEXTURE_ADDRESS_MODE_CLAMP
        } else {
            D3D12_TEXTURE_ADDRESS_MODE_WRAP
        }
    );
    let i = gpu.bound_sampler.get().unwrap() as usize;
    if request == 7 {
        assert_eq!(
            gpu.sampler_descriptions[i].Filter,
            gpu.sampler_descriptions[i + 1].Filter
        );
    }
}

fn assert_live_linear(gpu: &Gpu) {
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(
        desc.Filter, D3D12_FILTER_MIN_MAG_MIP_LINEAR,
        "filter mode reads live preference"
    );
}

fn assert_live_anisotropic(gpu: &Gpu) {
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(desc.Filter, D3D12_FILTER_ANISOTROPIC);
}

fn bound_bias(gpu: &Gpu) -> f32 {
    gpu.bound_sampler_description().unwrap().MipLODBias
}
