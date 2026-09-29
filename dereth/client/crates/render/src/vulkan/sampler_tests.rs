// Synthetic provided mip colors isolate SAMPLING policy from the separately tested ImgTex
// generator. Device descriptors are immutable and actual shader readback, not a smoothness score.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn sampler_banks_sample_the_selected_mip_and_survive_mid_frame_changes() {
    sampler_bank_captures(false);
}

/// The same draws with the pixel shader applying the bias, as a device whose samplers cannot
/// carry one does, sample exactly the texels the sampler's own bias samples.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn a_bias_applied_in_the_pixel_shader_samples_exactly_what_the_sampler_bias_samples() {
    let in_sampler = sampler_bank_captures(false);
    let in_shader = sampler_bank_captures(true);
    assert_eq!(in_sampler.len(), in_shader.len());
    for (i, (a, b)) in in_sampler.iter().zip(&in_shader).enumerate() {
        assert_eq!(a, b, "capture {i} differs between the sampler's bias and the shader's");
    }
}

/// A landscape splat takes the filtering preference's bias as the legacy draws do, whether the
/// sampler or the pixel shader applies it: Sharp moves its base texture's mip footprint, and the
/// two paths sample exactly the same texels.
/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn a_splat_takes_the_sharp_bias_in_the_sampler_or_in_the_pixel_shader_alike() {
    let splat_captures = |shader_lod_bias: bool| -> Vec<Vec<u8>> {
        let mut gpu = Gpu::new(
            None,
            &DeviceConfig {
                width: 16,
                height: 16,
                force_shader_lod_bias: shader_lod_bias,
                ..Default::default()
            },
        )
        .unwrap();
        let base = gpu.upload_texture(&mip_chain_64()).unwrap();
        let splat = crate::device::TerrainSplat { base: Some(base), base_tiling: 1, overlays: Vec::new() };
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
            gpu.draw_terrain_splat(&opaque_key(), &splat, &identity_frame(), &PerDrawConstants::identity(), &q)
                .unwrap();
            gpu.end_frame().unwrap();
            captures.push(gpu.capture().unwrap().bgra);
        }
        gpu.release_texture(base);
        captures
    };
    let in_sampler = splat_captures(false);
    let in_shader = splat_captures(true);
    assert_ne!(in_sampler[0], in_sampler[1], "Sharp changes the splat's mip footprint");
    assert_ne!(in_shader[0], in_shader[1], "Sharp changes the splat's mip footprint");
    assert_eq!(in_sampler, in_shader, "the splat samples the same texels either way");
}

/// A 64x64 texture whose seven levels are each a different solid colour.
fn mip_chain_64() -> TextureData {
    let colors = [
        [0, 0, 255, 255],
        [0, 255, 0, 255],
        [255, 0, 0, 255],
        [255, 255, 0, 255],
        [0, 255, 255, 255],
        [255, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    TextureData {
        width: 64,
        height: 64,
        format: TextureFormat::Bgra8,
        levels: colors.iter().enumerate().map(|(i, c)| c.repeat((64usize >> i).pow(2))).collect(),
    }
}

/// Every capture of the bank walk below, on a device that applies the bias in the pixel shader
/// when `shader_lod_bias` asks (and always where its samplers cannot carry one).
fn sampler_bank_captures(shader_lod_bias: bool) -> Vec<Vec<u8>> {
    let mut captures = Vec::new();
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 16,
            height: 16,
            force_shader_lod_bias: shader_lod_bias,
            ..Default::default()
        },
    )
    .unwrap();
    if shader_lod_bias {
        assert!(gpu.shader_lod_bias);
    }
    let slot = gpu.upload_texture(&mip_chain_64()).unwrap();
    let make_quad = |left, right| {
        let mut q = quad_142(left, -1.0, right, 1.0, 0.5, 0xFFFF_FFFF);
        // U/V span1.5: 64 texels ->16 pixels gives log2(6), deliberately fractional LOD.
        for v in q.as_chunks_mut::<24>().0 {
            let x = f32::from_le_bytes(v[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(v[4..8].try_into().unwrap());
            v[16..20].copy_from_slice(&((x - left) / (right - left) * 1.5).to_le_bytes());
            v[20..24].copy_from_slice(&((1.0 - y) * 0.75).to_le_bytes());
        }
        q
    };
    let draw = |gpu: &mut Gpu, q: &[u8], request| {
        gpu.bind_texture(slot, request);
        gpu.draw_dynamic(
            &opaque_key(),
            &crate::DrawConstants::default(),
            &identity_frame(),
            &PerDrawConstants::identity(),
            q,
        )
        .unwrap();
    };
    let mut samples = Vec::new();
    for preference in [0, 1, 2, 3] {
        gpu.set_texture_filtering(preference);
        gpu.begin_frame().unwrap();
        draw(&mut gpu, &make_quad(-1.0, 1.0), 1);
        let desc = gpu.bound_sampler_description().unwrap();
        let aniso = if gpu.anisotropy_supported { SamplerFilter::Anisotropic } else { SamplerFilter::Linear };
        assert_eq!(
            desc.filter,
            [SamplerFilter::LinearMipPoint, SamplerFilter::Linear, SamplerFilter::Linear, aniso][preference as usize]
        );
        assert_eq!(desc.mip_lod_bias, if preference == 2 { -1.4 } else { 0.0 });
        assert_eq!(desc.max_anisotropy, gpu.max_anisotropy);
        gpu.end_frame().unwrap();
        samples.push(gpu.capture().unwrap().bgra);
    }
    assert_eq!(&samples[0][0..3], &[255, 255, 0], "Bilinear chooses nearest mip3");
    assert_ne!(samples[0], samples[1], "fractional trilinear blends mips");
    assert_ne!(samples[1], samples[2], "Sharp changes mip footprint");

    gpu.begin_frame().unwrap();
    gpu.set_texture_filtering(0);
    draw(&mut gpu, &make_quad(-1.0, 0.0), 1);
    let baseline_desc = gpu.bound_sampler_description().unwrap();
    let err: Result<(), &'static str> = gpu.with_preview_sharp(true, |gpu| {
        draw(gpu, &make_quad(0.0, 1.0), 1);
        assert_eq!(gpu.bound_sampler_description().unwrap().mip_lod_bias, -1.4);
        Err("synthetic draw error after a recorded preview draw")
    });
    assert!(err.is_err());
    gpu.bind_texture(slot, 1);
    assert_eq!(
        gpu.bound_sampler_description().unwrap().mip_lod_bias,
        baseline_desc.mip_lod_bias,
        "error restores before following binding"
    );
    gpu.set_texture_filtering(3); // does not retroactively change either already-recorded draw
    gpu.end_frame().unwrap();
    let result = gpu.capture().unwrap().bgra;
    captures.extend(samples);
    captures.push(result.clone());
    assert_ne!(
        &result[4 * 3..4 * 3 + 3],
        &result[4 * 12..4 * 12 + 3],
        "both recorded banks remain distinct after owner change"
    );
    // Explicit point and independent address axes survive every preference and preview bias.
    for preference in [0, 1, 2, 3] {
        gpu.set_texture_filtering(preference);
        gpu.begin_frame().unwrap();
        for request in [2, 3, 6, 7] {
            gpu.bind_texture(slot, request);
            let desc = gpu.bound_sampler_description().unwrap();
            assert_eq!(desc.filter, SamplerFilter::Point);
            assert_eq!(
                desc.address_u,
                if request == 3 || request == 7 { AddressMode::Clamp } else { AddressMode::Wrap }
            );
            assert_eq!(
                desc.address_v,
                if request == 3 || request == 6 { AddressMode::Clamp } else { AddressMode::Wrap }
            );
            let i = gpu.bound_sampler.get().unwrap() as usize;
            if request == 7 {
                assert_eq!(gpu.sampler_descriptions[i].filter, gpu.sampler_descriptions[i + 1].filter);
            }
        }
        gpu.end_frame().unwrap();
    }
    gpu.release_texture(slot);
    captures
}

/// Behaviour: rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes
#[test]
fn sampler_bias_changes_at_begin_scene_and_guarded_preview_restores_global_baseline() {
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig { width: 1, height: 1, ..Default::default() },
    )
    .unwrap();
    let slot = gpu
        .upload_texture(&TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![255; 4]],
        })
        .unwrap();
    gpu.set_texture_filtering(0);
    gpu.begin_frame().unwrap();
    gpu.set_texture_filtering(2);
    gpu.bind_texture(slot, 1);
    let desc = gpu.bound_sampler_description().unwrap();
    assert_eq!(desc.filter, SamplerFilter::Linear, "filter mode reads live preference");
    assert_eq!(desc.mip_lod_bias, 0.0, "normal bias is not updated until BeginScene");
    gpu.end_frame().unwrap();
    gpu.begin_frame().unwrap();
    gpu.bind_texture(slot, 1);
    assert_eq!(gpu.bound_sampler_description().unwrap().mip_lod_bias, -1.4);
    gpu.set_texture_filtering(3);
    gpu.bind_texture(slot, 1);
    let desc = gpu.bound_sampler_description().unwrap();
    let aniso = if gpu.anisotropy_supported { SamplerFilter::Anisotropic } else { SamplerFilter::Linear };
    assert_eq!(desc.filter, aniso);
    assert_eq!(desc.mip_lod_bias, -1.4, "existing bias survives a mid-frame preference write");
    gpu.with_preview_sharp(true, |gpu| {
        gpu.bind_texture(slot, 1);
    });
    gpu.bind_texture(slot, 1);
    assert_eq!(
        gpu.bound_sampler_description().unwrap().mip_lod_bias,
        -1.4,
        "pref3 guard has no tail reset"
    );
    gpu.set_texture_filtering(1);
    gpu.with_preview_sharp(true, |gpu| {
        gpu.bind_texture(slot, 1);
    });
    gpu.bind_texture(slot, 1);
    assert_eq!(
        gpu.bound_sampler_description().unwrap().mip_lod_bias,
        0.0,
        "guarded tail restores GLOBAL zero, not old scene bias"
    );
    gpu.end_frame().unwrap();
    for preference in [0, 1, 2, 3] {
        for enabled in [false, true] {
            gpu.set_texture_filtering(preference);
            gpu.begin_frame().unwrap();
            let error: Result<(), &'static str> = gpu.with_preview_sharp(enabled, |gpu| {
                gpu.bind_texture(slot, 1);
                let expected =
                    if preference == 2 || (enabled && preference < 2) { -1.4 } else { 0.0 };
                assert_eq!(gpu.bound_sampler_description().unwrap().mip_lod_bias, expected);
                Err("synthetic preview error")
            });
            assert!(error.is_err());
            gpu.bind_texture(slot, 1);
            assert_eq!(
                gpu.bound_sampler_description().unwrap().mip_lod_bias,
                if preference == 2 { -1.4 } else { 0.0 }
            );
            gpu.end_frame().unwrap();
        }
    }
    gpu.release_texture(slot);
}
