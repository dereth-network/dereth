use super::*;

// Synthetic provided mip colors isolate SAMPLING policy from the separately tested ImgTex
// generator. Device descriptors are immutable and actual shader readback, not a smoothness score.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn sampler_banks_sample_the_selected_mip_and_survive_mid_frame_changes() {
    let _ = sampler_bank_captures(false);
}

/// The same draws with the pixel shader applying the bias, as a device whose samplers cannot
/// carry one does, sample exactly the texels the sampler's own bias samples.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn a_bias_applied_in_the_pixel_shader_samples_exactly_what_the_sampler_bias_samples() {
    let (Some(in_sampler), Some(in_shader)) =
        (sampler_bank_captures(false), sampler_bank_captures(true))
    else {
        return;
    };
    assert_eq!(in_sampler.len(), in_shader.len());
    for (i, (a, b)) in in_sampler.iter().zip(&in_shader).enumerate() {
        assert_eq!(
            a, b,
            "capture {i} differs between the sampler's bias and the shader's"
        );
    }
}

/// A landscape splat takes the filtering preference's bias as the legacy draws do, whether the
/// sampler or the pixel shader applies it: Sharp moves its base texture's mip footprint, and the
/// two paths sample exactly the same texels.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn a_splat_takes_the_sharp_bias_in_the_sampler_or_in_the_pixel_shader_alike() {
    let splat_captures = |shader_lod_bias: bool| -> Option<Vec<Vec<u8>>> {
        let mut gpu = device_or_skip(&DeviceConfig {
            width: 16,
            height: 16,
            force_shader_lod_bias: shader_lod_bias,
            ..Default::default()
        })?;
        let base = gpu.upload_texture(&shared::mip_chain_64()).unwrap();
        let splat = crate::device::TerrainSplat {
            base: Some(base),
            base_tiling: 1,
            overlays: Vec::new(),
        };
        // U spans 0..1 and V 0..0.75 across 16 pixels: 64 texels to 16 pixels, level 2 exactly,
        // which Sharp's bias pulls to a blend of levels 0 and 1.
        let mut q = quad_142(-1.0, -1.0, 1.0, 1.0, 0.5, 0xFFFF_FFFF);
        for v in q.as_chunks_mut::<24>().0 {
            let x = f32::from_le_bytes(v[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(v[4..8].try_into().unwrap());
            v[16..20].copy_from_slice(&((x + 1.0) * 0.5).to_le_bytes());
            v[20..24].copy_from_slice(&((1.0 - y) * 0.375).to_le_bytes());
        }
        let mut captures = Vec::new();
        for preference in [1, 2] {
            gpu.set_texture_filtering(preference);
            gpu.begin_frame().unwrap();
            gpu.draw_terrain_splat(
                &opaque_key(),
                &splat,
                &identity_frame(),
                &PerDrawConstants::identity(),
                &q,
            )
            .unwrap();
            gpu.end_frame().unwrap();
            captures.push(gpu.capture().unwrap().bgra);
        }
        gpu.release_texture(base);
        Some(captures)
    };
    let (Some(in_sampler), Some(in_shader)) = (splat_captures(false), splat_captures(true)) else {
        return;
    };
    assert_ne!(
        in_sampler[0], in_sampler[1],
        "Sharp changes the splat's mip footprint"
    );
    assert_ne!(
        in_shader[0], in_shader[1],
        "Sharp changes the splat's mip footprint"
    );
    assert_eq!(
        in_sampler, in_shader,
        "the splat samples the same texels either way"
    );
}

/// A device for `cfg`, or `None` (with a printed line) where Vulkan is unavailable: these tests
/// skip rather than fail then, as every device test in this module does.
fn device_or_skip(cfg: &DeviceConfig) -> Option<Gpu> {
    match Gpu::new(None, cfg) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("skipping: no Vulkan device available ({e})");
            None
        }
    }
}

/// Every capture of the bank walk below, on a device that applies the bias in the pixel shader
/// when `shader_lod_bias` asks (and always where its samplers cannot carry one); `None` where
/// there is no Vulkan device.
fn sampler_bank_captures(shader_lod_bias: bool) -> Option<Vec<Vec<u8>>> {
    let mut gpu = device_or_skip(&DeviceConfig {
        width: 16,
        height: 16,
        force_shader_lod_bias: shader_lod_bias,
        ..Default::default()
    })?;
    if shader_lod_bias {
        assert!(gpu.shader_lod_bias);
    }
    Some(shared::sampler_bank_captures(&mut gpu))
}

/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn sampler_bias_changes_at_begin_scene_and_guarded_preview_restores_global_baseline() {
    let Some(mut gpu) = device_or_skip(&DeviceConfig {
        width: 1,
        height: 1,
        ..Default::default()
    }) else {
        return;
    };
    shared::sampler_bias_changes_at_begin_scene(&mut gpu);
}

// Shared source is instantiated with this backend's types and assertion helpers.
#[allow(clippy::duplicate_mod)]
#[path = "../tests/sampler.rs"]
mod shared;

fn assert_preference(gpu: &Gpu, preference: u32) {
    let desc = gpu.bound_sampler_description().unwrap();
    let aniso = if gpu.anisotropy_supported {
        SamplerFilter::Anisotropic
    } else {
        SamplerFilter::Linear
    };
    assert_eq!(
        desc.filter,
        [
            SamplerFilter::LinearMipPoint,
            SamplerFilter::Linear,
            SamplerFilter::Linear,
            aniso
        ][preference as usize]
    );
    assert_eq!(desc.mip_lod_bias, if preference == 2 { -1.4 } else { 0.0 });
    assert_eq!(desc.max_anisotropy, gpu.max_anisotropy);
}

fn assert_point_request(gpu: &Gpu, request: u32) {
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(desc.filter, SamplerFilter::Point);
    assert_eq!(
        desc.address_u,
        if request == 3 || request == 7 {
            AddressMode::Clamp
        } else {
            AddressMode::Wrap
        }
    );
    assert_eq!(
        desc.address_v,
        if request == 3 || request == 6 {
            AddressMode::Clamp
        } else {
            AddressMode::Wrap
        }
    );
    let i = gpu.bound_sampler.get().unwrap() as usize;
    if request == 7 {
        assert_eq!(
            gpu.sampler_descriptions[i].filter,
            gpu.sampler_descriptions[i + 1].filter
        );
    }
}

fn assert_live_linear(gpu: &Gpu) {
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(
        desc.filter,
        SamplerFilter::Linear,
        "filter mode reads live preference"
    );
}

fn assert_live_anisotropic(gpu: &Gpu) {
    let desc = gpu.bound_sampler_description().unwrap();
    let aniso = if gpu.anisotropy_supported {
        SamplerFilter::Anisotropic
    } else {
        SamplerFilter::Linear
    };
    assert_eq!(desc.filter, aniso);
}

fn bound_bias(gpu: &Gpu) -> f32 {
    gpu.bound_sampler_description().unwrap().mip_lod_bias
}
