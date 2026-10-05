use super::*;

/// A 64x64 texture whose seven levels are each a different solid colour.
pub(super) fn mip_chain_64() -> TextureData {
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
        levels: colors
            .iter()
            .enumerate()
            .map(|(i, c)| c.repeat((64usize >> i).pow(2)))
            .collect(),
    }
}

pub(super) fn sampler_bank_captures(gpu: &mut Gpu) -> Vec<Vec<u8>> {
    let mut captures = Vec::new();
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
        draw(gpu, &make_quad(-1.0, 1.0), 1);
        assert_preference(gpu, preference);
        gpu.end_frame().unwrap();
        samples.push(gpu.capture().unwrap().bgra);
    }
    assert_eq!(
        &samples[0][0..3],
        &[255, 255, 0],
        "Bilinear chooses nearest mip3"
    );
    assert_ne!(samples[0], samples[1], "fractional trilinear blends mips");
    assert_ne!(samples[1], samples[2], "Sharp changes mip footprint");

    gpu.begin_frame().unwrap();
    gpu.set_texture_filtering(0);
    draw(gpu, &make_quad(-1.0, 0.0), 1);
    let baseline_bias = bound_bias(gpu);
    let err: Result<(), &'static str> = gpu.with_preview_sharp(true, |gpu| {
        draw(gpu, &make_quad(0.0, 1.0), 1);
        assert_eq!(bound_bias(gpu), -1.4);
        Err("synthetic draw error after a recorded preview draw")
    });
    assert!(err.is_err());
    gpu.bind_texture(slot, 1);
    assert_eq!(
        bound_bias(gpu),
        baseline_bias,
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
            assert_point_request(gpu, request);
        }
        gpu.end_frame().unwrap();
    }
    gpu.release_texture(slot);
    captures
}

pub(super) fn sampler_bias_changes_at_begin_scene(gpu: &mut Gpu) {
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
    assert_live_linear(gpu);
    assert_eq!(
        bound_bias(gpu),
        0.0,
        "normal bias is not updated until BeginScene"
    );
    gpu.end_frame().unwrap();
    gpu.begin_frame().unwrap();
    gpu.bind_texture(slot, 1);
    assert_eq!(bound_bias(gpu), -1.4);
    gpu.set_texture_filtering(3);
    gpu.bind_texture(slot, 1);
    assert_live_anisotropic(gpu);
    assert_eq!(
        bound_bias(gpu),
        -1.4,
        "existing bias survives a mid-frame preference write"
    );
    gpu.with_preview_sharp(true, |gpu| {
        gpu.bind_texture(slot, 1);
    });
    gpu.bind_texture(slot, 1);
    assert_eq!(bound_bias(gpu), -1.4, "pref3 guard has no tail reset");
    gpu.set_texture_filtering(1);
    gpu.with_preview_sharp(true, |gpu| {
        gpu.bind_texture(slot, 1);
    });
    gpu.bind_texture(slot, 1);
    assert_eq!(
        bound_bias(gpu),
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
                let expected = if preference == 2 || (enabled && preference < 2) {
                    -1.4
                } else {
                    0.0
                };
                assert_eq!(bound_bias(gpu), expected);
                Err("synthetic preview error")
            });
            assert!(error.is_err());
            gpu.bind_texture(slot, 1);
            assert_eq!(bound_bias(gpu), if preference == 2 { -1.4 } else { 0.0 });
            gpu.end_frame().unwrap();
        }
    }
    gpu.release_texture(slot);
}
