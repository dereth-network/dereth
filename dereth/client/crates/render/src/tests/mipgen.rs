use super::*;

// synthetic device inputs. These test runtime policy/linear filtering/resource ownership,
// not D3DX BOX or cross-driver rounding. The separate client tests supply real DAT/world owners.

fn mip_solid(width: u32, height: u32, color: [u8; 4]) -> TextureData {
    TextureData {
        width,
        height,
        format: TextureFormat::Bgra8,
        levels: vec![color.repeat((width * height) as usize)],
    }
}

/// Behaviour: rendering.textures.runtime-mips-preserve-channels-and-keep-their-owners-alive
pub(super) fn runtime_mips_preserve_every_channel_and_handle_rectangles_and_odd_extents() {
    let Some(mut gpu) = mip_test_device(16, 16) else {
        return;
    };
    assert!(gpu.imgtex_autogen_supported());
    for (w, h) in [(32, 8), (1, 16), (7, 3), (1, 1)] {
        let t = mip_solid(w, h, [24, 60, 100, 144]);
        let slot = gpu
            .upload_imgtex_keyed(TextureKey::UNCACHED, &t)
            .expect("runtime upload");
        let levels = gpu.texture_mip_levels(slot).expect("resident resource");
        assert_eq!(u32::from(levels), 1 + w.max(h).ilog2());
        for level in 0..levels {
            let pixels = gpu
                .capture_texture_level(slot, level)
                .expect("actual subresource");
            let expected = mip_solid((w >> level).max(1), (h >> level).max(1), [24, 60, 100, 144]);
            assert_eq!(
                (pixels.width, pixels.height),
                (expected.width, expected.height)
            );
            assert_eq!(
                pixels.bgra, expected.levels[0],
                "mip{level}: alpha must also be written"
            );
        }
        gpu.release_texture(slot);
        assert!(
            gpu.capture_texture_level(slot, 0).is_err(),
            "released resource is not observable"
        );
    }
    // At a 3-to-1 linear reduction the sample is at the source centre (middle texel), not
    // the first two texels' average. This distinguishes this path from the old CPU BOX helper.
    let odd = TextureData {
        width: 3,
        height: 1,
        format: TextureFormat::Bgra8,
        levels: vec![vec![0, 0, 0, 0, 40, 80, 120, 160, 240, 200, 180, 255]],
    };
    let slot = gpu
        .upload_imgtex_keyed(TextureKey::UNCACHED, &odd)
        .expect("odd runtime texture");
    assert_eq!(
        gpu.capture_texture_level(slot, 1).expect("odd mip").bgra,
        [40, 80, 120, 160]
    );
    assert_eq!(
        gpu.capture_texture_level(slot, 0).expect("base").bgra,
        odd.levels[0]
    );
    gpu.release_texture(slot);
}

pub(super) fn runtime_mips_leave_provided_bc_and_non_imgtex_uploads_unchanged() {
    let Some(mut gpu) = mip_test_device(16, 16) else {
        return;
    };
    let t = mip_solid(16, 16, [24, 60, 100, 144]);
    for key in [
        TextureKey::ui(1),
        TextureKey::font(0, 1),
        TextureKey::font(1, 1),
        TextureKey::solid_color(1),
        TextureKey::UNCACHED,
    ] {
        let slot = gpu
            .upload_texture_keyed(key, &t)
            .expect("provided UI/font/movie/debug upload");
        assert_eq!(gpu.texture_mip_levels(slot), Some(1));
        assert_eq!(
            gpu.capture_texture_level(slot, 0)
                .expect("provided copy")
                .bgra,
            t.levels[0]
        );
        gpu.release_texture(slot);
        if key.space() != crate::TextureSpace::World {
            assert!(
                gpu.upload_imgtex_keyed(key, &t).is_err(),
                "reject accidental UI autogen ownership"
            );
        }
    }
    let mut explicit = t.clone();
    explicit.levels.push([10, 20, 30, 40].repeat(8 * 8));
    let slot = gpu
        .upload_imgtex_keyed(TextureKey::UNCACHED, &explicit)
        .expect("explicit chain");
    assert_eq!(gpu.texture_mip_levels(slot), Some(2));
    for level in 0..2 {
        assert_eq!(
            gpu.capture_texture_level(slot, level)
                .expect("provided mip")
                .bgra,
            explicit.levels[usize::from(level)]
        );
    }
    gpu.release_texture(slot);
    for (format, block) in [
        (TextureFormat::Bc1, 8),
        (TextureFormat::Bc2, 16),
        (TextureFormat::Bc3, 16),
    ] {
        let bc = TextureData {
            width: 8,
            height: 8,
            format,
            levels: vec![vec![0; block * 4], vec![0; block]],
        };
        let slot = gpu
            .upload_imgtex_keyed(TextureKey::UNCACHED, &bc)
            .expect("BC provided chain");
        // Explicit two-level chains are never silently completed by either system or runtime
        // generation. The new BC1 system path only accepts a single-level image-texture source.
        assert_eq!(
            gpu.texture_mip_levels(slot),
            Some(2),
            "preserve provided BC chains"
        );
        gpu.release_texture(slot);
    }
    assert!(gpu.imgtex_autogen_supported());
    gpu.imgtex_autogen_supported = false; // synthetic unsupported-capability branch, not a user preference.
    let slot = gpu
        .upload_imgtex_keyed(TextureKey::UNCACHED, &t)
        .expect("unsupported-cap fallback");
    assert_eq!(gpu.texture_mip_levels(slot), Some(1));
    assert_eq!(
        gpu.capture_texture_level(slot, 0)
            .expect("fallback copy")
            .bgra,
        t.levels[0]
    );
    gpu.release_texture(slot);
}

/// Behaviour: rendering.textures.runtime-mips-preserve-channels-and-keep-their-owners-alive
pub(super) fn runtime_mips_keep_cache_links_and_open_frame_resources_alive() {
    let Some(mut gpu) = mip_test_device(16, 16) else {
        return;
    };
    let red = mip_solid(32, 32, [0, 0, 255, 255]);
    let green = mip_solid(32, 32, [0, 255, 0, 255]);
    let mut checker = green.clone();
    for (i, pixel) in checker.levels[0]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        pixel.copy_from_slice(if (i % 32 + i / 32) % 2 == 0 {
            &[0, 0, 240, 255]
        } else {
            &[0, 240, 0, 255]
        });
    }
    let key = TextureKey::world(0x0000_0000_0600_0001);
    let a = gpu.upload_imgtex_keyed(key, &red).expect("first upload");
    let stats = gpu.texture_table_stats();
    let b = gpu
        .upload_imgtex_keyed(key, &green)
        .expect("cache link, not another upload");
    assert_eq!(
        a, b,
        "first owner decides pixels under unchanged retail key"
    );
    assert_eq!(gpu.texture_table_stats().hits, stats.hits + 1);
    assert_eq!(
        gpu.capture_texture_level(a, 5).expect("cached tail").bgra,
        [0, 0, 255, 255]
    );
    gpu.release_texture(a);
    assert_eq!(
        gpu.texture_mip_levels(b),
        Some(6),
        "one surviving link owns all mips"
    );

    gpu.begin_frame().expect("open frame");
    gpu.bind_texture(b, 1);
    gpu.draw_dynamic(
        &opaque_key(),
        &crate::DrawConstants::default(),
        &identity_frame(),
        &PerDrawConstants::identity(),
        &quad_142(-1.0, -1.0, 1.0, 1.0, 0.5, 0xFFFF_FFFF),
    )
    .expect("draw before mid-frame generation");
    gpu.release_texture(b);
    let c = gpu
        .upload_imgtex_keyed(TextureKey::UNCACHED, &checker)
        .expect("mid-frame generation");
    assert_ne!(
        c, b,
        "upload fence must not recycle an open frame's descriptor"
    );
    assert_eq!(gpu.texture_mip_levels(c), Some(6));
    let mut second = quad_142(0.0, -1.0, 1.0, 1.0, 0.4, 0xFFFF_FFFF);
    for vertex in second.as_chunks_mut::<24>().0 {
        let x = f32::from_le_bytes(vertex[0..4].try_into().expect("x"));
        let y = f32::from_le_bytes(vertex[4..8].try_into().expect("y"));
        vertex[16..20].copy_from_slice(&x.to_le_bytes());
        vertex[20..24].copy_from_slice(&((1.0 - y) * 0.5).to_le_bytes());
    }
    gpu.bind_texture(c, 1);
    gpu.draw_dynamic(
        &opaque_key(),
        &crate::DrawConstants::default(),
        &identity_frame(),
        &PerDrawConstants::identity(),
        &second,
    )
    .expect("draw after generation uses new mips");
    gpu.end_frame()
        .expect("mip list must not reset the frame's command list");
    for (i, px) in gpu
        .capture()
        .expect("frame")
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        assert_eq!(
            px[..3],
            if i % 16 < 8 {
                [0, 0, 255]
            } else {
                [0, 120, 120]
            },
            "old draw survives, new draw samples the generated average, pixel{i}"
        );
    }
    gpu.release_texture(c);
    let d = gpu
        .upload_imgtex_keyed(TextureKey::UNCACHED, &green)
        .expect("reuse after fence");
    assert_eq!(gpu.texture_mip_levels(d), Some(6));
    assert_eq!(
        gpu.capture_texture_level(d, 5).expect("new tail").bgra,
        [0, 255, 0, 255]
    );
    gpu.release_texture(d);
    assert_eq!(gpu.live_textures(), 0);
}

pub(super) fn compressed_mips_preserve_subresources_owners_and_open_frame_links() {
    compressed_owner_lifetime(TextureFormat::Bc1);
}

pub(super) fn compressed_bc2_mips_preserve_subresources_owners_and_open_frame_links() {
    compressed_owner_lifetime(TextureFormat::Bc2);
}

pub(super) fn compressed_bc3_mips_preserve_subresources_owners_and_open_frame_links() {
    compressed_owner_lifetime(TextureFormat::Bc3);
}

pub(super) fn premultiplied_mips_dxt2_source_decoder_resource_and_minified_draw() {
    premultiplied_resource(
        crate::PixelFormatId::Dxt2,
        TextureFormat::Bc2Premultiplied,
        TextureFormat::Bc2,
    );
}

pub(super) fn premultiplied_mips_dxt4_source_decoder_resource_and_minified_draw() {
    premultiplied_resource(
        crate::PixelFormatId::Dxt4,
        TextureFormat::Bc3Premultiplied,
        TextureFormat::Bc3,
    );
}

pub(super) fn premultiplied_mips_dxt2_owners_and_open_frame_links() {
    compressed_owner_lifetime(TextureFormat::Bc2Premultiplied);
}

pub(super) fn premultiplied_mips_dxt4_owners_and_open_frame_links() {
    compressed_owner_lifetime(TextureFormat::Bc3Premultiplied);
}

fn premultiplied_resource(
    source: crate::PixelFormatId,
    format: TextureFormat,
    layout: TextureFormat,
) {
    // Explicitly synthetic source: installed DAT has no DXT2/4. The normal
    // decoder, image-texture upload and real minified draw still run unmodified.
    let alpha = if source == crate::PixelFormatId::Dxt2 {
        [0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe]
    } else {
        [0xe7, 0x19, 0x88, 0xc6, 0xfa, 0x77, 0x39, 0x05]
    };
    let mut block = alpha.to_vec();
    block.extend_from_slice(&[0x75, 0xd0, 0x23, 0x50, 0x50, 0x27, 0xbc, 0xee]);
    let bytes = block.repeat(32 * 32);
    let data = crate::decode_surface(
        crate::SourcePixels::Dxt {
            format: source,
            blocks: &bytes,
        },
        128,
        128,
    )
    .unwrap();
    assert_eq!(data.format, format);
    assert_eq!(data.levels, vec![bytes]);
    let Some(mut gpu) = mip_test_device(16, 16) else {
        return;
    };
    let slot = gpu
        .upload_imgtex_keyed(TextureKey::world(0x0600_0002), &data)
        .unwrap();
    assert_eq!(
        gpu.texture_mip_levels(slot),
        Some(4),
        "{}",
        generated_levels_message(source)
    );
    let expected = crate::mip::compressed_system_chain(&data).unwrap().unwrap();
    assert_eq!(expected.levels[0], data.levels[0]);
    for (level, bytes) in expected.levels.iter().enumerate() {
        let actual = gpu.capture_texture_level_data(slot, level as u16).unwrap();
        assert_eq!(
            actual.format, layout,
            "readback is device layout, not source provenance"
        );
        assert_eq!((actual.width, actual.height), (128 >> level, 128 >> level));
        assert_eq!(&actual.levels[0], bytes);
    }
    let mut quad = quad_142(-1.0, -1.0, 1.0, 1.0, 0.5, 0xffff_ffff);
    for vertex in quad.as_chunks_mut::<24>().0 {
        let x = f32::from_le_bytes(vertex[..4].try_into().unwrap());
        let y = f32::from_le_bytes(vertex[4..8].try_into().unwrap());
        vertex[16..20].copy_from_slice(&((x + 1.0) * 0.5).to_le_bytes());
        vertex[20..24].copy_from_slice(&((1.0 - y) * 0.5).to_le_bytes());
    }
    let draw = |gpu: &mut Gpu, texture| {
        gpu.begin_frame().unwrap();
        gpu.bind_texture(texture, 1);
        gpu.draw_dynamic(
            &opaque_key(),
            &crate::DrawConstants::default(),
            &identity_frame(),
            &PerDrawConstants::identity(),
            &quad,
        )
        .unwrap();
        gpu.end_frame().unwrap();
        gpu.capture().unwrap().bgra
    };
    let sampled = draw(&mut gpu, slot);
    assert_eq!(sampled, draw(&mut gpu, slot), "stationary sampled output");
    let tail = TextureData {
        width: 16,
        height: 16,
        format,
        levels: vec![expected.levels[3].clone()],
    };
    let tail_slot = gpu
        .upload_texture_keyed(TextureKey::UNCACHED, &tail)
        .unwrap();
    assert_eq!(
        sampled,
        draw(&mut gpu, tail_slot),
        "minified source samples generated final level"
    );
    let old = gpu
        .upload_texture_keyed(TextureKey::UNCACHED, &data)
        .unwrap();
    let old_pixels = draw(&mut gpu, old);
    let changed = sampled
        .as_chunks::<4>()
        .0
        .iter()
        .zip(old_pixels.as_chunks::<4>().0)
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    assert!(
        changed > 0,
        "synthetic source must reject old single-level sampling"
    );
    let wrong = crate::mip::compressed_system_chain(&TextureData {
        format: layout,
        ..data.clone()
    })
    .unwrap()
    .unwrap();
    assert_ne!(
        wrong.levels[1], expected.levels[1],
        "source fixture distinguishes premultiplied codec from plain BC"
    );
    eprintln!("synthetic {source:?} minified draw: {changed}/256 changed RGB pixels");
    for h in [slot, tail_slot, old] {
        gpu.release_texture(h);
    }
    assert_eq!(gpu.live_textures(), 0);
}

fn compressed_owner_lifetime(format: TextureFormat) {
    let Some(mut gpu) = mip_test_device(16, 16) else {
        return;
    };
    let block = |rgb: [u8; 8]| {
        if format == TextureFormat::Bc1 {
            rgb.to_vec()
        } else if matches!(format, TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied) {
            [vec![255, 255, 0, 0, 0, 0, 0, 0], rgb.to_vec()].concat()
        } else {
            [vec![255; 8], rgb.to_vec()].concat()
        }
    };
    let red = TextureData {
        width: 8,
        height: 8,
        format,
        levels: vec![block([0, 248, 0, 248, 0, 0, 0, 0]).repeat(4)],
    };
    let blue = TextureData {
        levels: vec![block([31, 0, 31, 0, 0, 0, 0, 0]).repeat(4)],
        ..red.clone()
    };
    let expected = crate::mip::compressed_system_chain(&red).unwrap().unwrap();
    let key = TextureKey::world(0x0600_0002);
    let a = gpu.upload_imgtex_keyed(key, &red).unwrap();
    let stats = gpu.texture_table_stats();
    // Same-key link must precede chain work: a deliberately invalid replacement
    // would fail preparation, but retail cache ownership keeps the first resource.
    let invalid = TextureData {
        width: 0,
        ..blue.clone()
    };
    let b = gpu.upload_imgtex_keyed(key, &invalid).unwrap();
    assert_eq!(a, b);
    assert_eq!(gpu.texture_table_stats().hits, stats.hits + 1);
    assert_eq!(gpu.texture_mip_levels(a), Some(4));
    for (i, bytes) in expected.levels.iter().enumerate() {
        let data = gpu.capture_texture_level_data(a, i as u16).unwrap();
        assert_eq!(
            (data.width, data.height),
            ((8 >> i).max(1), (8 >> i).max(1))
        );
        assert_eq!(&data.levels[0], bytes);
    }
    assert!(
        gpu.capture_texture_level(a, 0).is_err(),
        "BGRA API remains format restricted"
    );
    assert!(
        gpu.capture_texture_level_data(a, 4).is_err(),
        "invalid mip is rejected"
    );
    gpu.release_texture(a);
    gpu.begin_frame().unwrap();
    gpu.bind_texture(b, 1);
    gpu.draw_dynamic(
        &opaque_key(),
        &crate::DrawConstants::default(),
        &identity_frame(),
        &PerDrawConstants::identity(),
        &quad_142(-1.0, -1.0, 0.0, 1.0, 0.5, 0xffff_ffff),
    )
    .unwrap();
    gpu.release_texture(b);
    let c = gpu
        .upload_imgtex_keyed(TextureKey::UNCACHED, &blue)
        .unwrap();
    assert_ne!(
        b, c,
        "in-flight source descriptor must not be reused by a new compressed upload"
    );
    gpu.bind_texture(c, 1);
    gpu.draw_dynamic(
        &opaque_key(),
        &crate::DrawConstants::default(),
        &identity_frame(),
        &PerDrawConstants::identity(),
        &quad_142(0.0, -1.0, 1.0, 1.0, 0.5, 0xffff_ffff),
    )
    .unwrap();
    gpu.end_frame().unwrap();
    let frame = gpu.capture().unwrap();
    assert_eq!(
        &frame.bgra[(8 * 16 + 4) * 4..(8 * 16 + 4) * 4 + 3],
        &[0, 0, 255]
    );
    assert_eq!(
        &frame.bgra[(8 * 16 + 12) * 4..(8 * 16 + 12) * 4 + 3],
        &[255, 0, 0]
    );
    gpu.release_texture(c);
    assert!(gpu.capture_texture_level_data(c, 0).is_err());
    for key in [
        TextureKey::ui(2),
        TextureKey::font(0, 2),
        TextureKey::solid_color(2),
        TextureKey::UNCACHED,
    ] {
        let slot = gpu.upload_texture_keyed(key, &red).unwrap();
        assert_eq!(
            gpu.texture_mip_levels(slot),
            Some(1),
            "generic/UI/font/movie upload stays provided-only"
        );
        assert_eq!(
            gpu.capture_texture_level_data(slot, 0).unwrap().levels[0],
            red.levels[0]
        );
        gpu.release_texture(slot);
    }
    if matches!(format, TextureFormat::Bc2 | TextureFormat::Bc3) {
        // Preserve source identity through the normal asset decoder. Raw DXGI
        // readback cannot recover DXT2/4 premultiplication provenance.
        let (source_format, premult_format) = if format == TextureFormat::Bc2 {
            (crate::PixelFormatId::Dxt2, TextureFormat::Bc2Premultiplied)
        } else {
            (crate::PixelFormatId::Dxt4, TextureFormat::Bc3Premultiplied)
        };
        let premult = crate::decode_surface(
            crate::SourcePixels::Dxt {
                format: source_format,
                blocks: &red.levels[0],
            },
            8,
            8,
        )
        .unwrap();
        assert_eq!(premult.format, premult_format);
        assert_eq!(
            crate::mip::compressed_system_chain(&premult)
                .unwrap()
                .unwrap()
                .levels
                .len(),
            4
        );
        let slot = gpu
            .upload_imgtex_keyed(TextureKey::UNCACHED, &premult)
            .unwrap();
        assert_eq!(
            gpu.texture_mip_levels(slot),
            Some(4),
            "ported DXT2/4 keeps the source codec while generating system levels"
        );
        let resident = gpu.capture_texture_level_data(slot, 0).unwrap();
        assert_eq!(resident.format, format, "{LAYOUT_MESSAGE}");
        assert_eq!(resident.levels, premult.levels);
        gpu.release_texture(slot);
        let provided = TextureData {
            levels: vec![premult.levels[0].clone(), premult.levels[0][..16].to_vec()],
            ..premult
        };
        let slot = gpu
            .upload_imgtex_keyed(TextureKey::UNCACHED, &provided)
            .unwrap();
        assert_eq!(gpu.texture_mip_levels(slot), Some(2));
        assert_eq!(
            gpu.capture_texture_level_data(slot, 1).unwrap().levels[0],
            provided.levels[1]
        );
        gpu.release_texture(slot);
    }
}
