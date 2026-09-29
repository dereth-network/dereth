use super::*;

include!("mipgen_tests.rs");
include!("sampler_tests.rs");

/// Create a device, preferring a CPU rasteriser, or return `None` when Vulkan is unavailable.
/// Every test here skips rather than fails in that case: the pure-logic half of each unit is
/// tested elsewhere and does not need a device.
fn warp(width: u32, height: u32) -> Option<Gpu> {
    let cfg = DeviceConfig {
        width,
        height,
        ..DeviceConfig::default()
    };
    match Gpu::new(None, &cfg) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("skipping: no Vulkan device available ({e})");
            None
        }
    }
}

/// A 1x1 opaque-white texture, so `ps_modulate`'s `t * i.color` is the vertex colour.
fn white() -> TextureData {
    TextureData {
        width: 1,
        height: 1,
        format: TextureFormat::Bgra8,
        levels: vec![vec![0xFFu8; 4]],
    }
}

/// Identity everywhere: with `view_proj` and `world` both identity, a `0x142` vertex's position
/// *is* its clip-space position, so a test can name device depths directly.
fn identity_frame() -> PerFrameConstants {
    PerFrameConstants {
        view_proj: hlsl_matrix(glam::Mat4::IDENTITY),
        view: hlsl_matrix(glam::Mat4::IDENTITY),
        // Fog end 1, fog disabled.
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..PerFrameConstants::default()
    }
}

/// Two triangles covering `[x0, x1] x [y0, y1]` of clip space at depth `z`, FVF `0x142`.
fn quad_142(x0: f32, y0: f32, x1: f32, y1: f32, z: f32, argb: u32) -> Vec<u8> {
    let mut v = Vec::new();
    let mut push = |x: f32, y: f32| {
        for f in [x, y, z] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&argb.to_le_bytes());
        v.extend_from_slice(&0.0f32.to_le_bytes());
        v.extend_from_slice(&0.0f32.to_le_bytes());
    };
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)] {
        push(x, y);
    }
    v
}

/// The opaque state every catalogue row 1 draw uses: `One/Zero`, no blend, no alpha test,
/// depth `LESS` with the write on.
fn opaque_key() -> PipelineKey {
    PipelineKey {
        vertex_format: VertexFormat::XyzDiffuseTex1,
        src_blend: Blend::One,
        dst_blend: Blend::Zero,
        alpha_blend: false,
        alpha_test: false,
        z_write: true,
        z_func: ZFunc::Less,
        cull: Cull::None,
        stage_ops: crate::StageOps::BASE,
        fog: false,
        lighting: false,
    }
}

/// Oracle: the portal depth stamp runs with a mask of 7 —
/// depth test `DEPTHTEST_ALWAYS` with depth writes on and, from bit 0, the stamped depth is the
/// constant `0.999999`. The green must appear exactly where the stamp went and nowhere else.
#[test]
fn the_portal_stamp_resets_the_depth_inside_its_polygon_and_nowhere_else() {
    const N: u32 = 64;
    let Some(mut gpu) = warp(N, N) else { return };
    let tex = gpu.upload_texture(&white()).expect("upload");
    let per_frame = identity_frame();
    let draw = PerDrawConstants::identity();
    let red = 0xFFFF_0000u32;
    let green = 0xFF00_FF00u32;
    // The stamped rectangle, in clip space and then in pixels. Clip +y is the top row.
    let (sx0, sy0, sx1, sy1) = (-0.5f32, -0.5f32, 0.5f32, 0.5f32);

    let render = |gpu: &mut Gpu, stamp: bool| -> Vec<u8> {
        gpu.begin_frame().expect("begin");
        gpu.bind_texture(tex, 0);
        gpu.draw_dynamic(
            &opaque_key(),
            &crate::DrawConstants::default(),
            &per_frame,
            &draw,
            &quad_142(-1.0, -1.0, 1.0, 1.0, 0.2, red),
        )
        .expect("the near surface");
        if stamp {
            let poly: Vec<[f32; 4]> = [(sx0, sy0), (sx1, sy0), (sx1, sy1), (sx0, sy1)]
                .into_iter()
                .map(|(x, y)| [x, y, 0.5, 1.0])
                .collect();
            gpu.draw_portal_poly(&per_frame, &poly, crate::pso::portal_stamp_mask::BUILDING)
                .expect("the stamp");
        }
        gpu.bind_texture(tex, 0);
        gpu.draw_dynamic(
            &opaque_key(),
            &crate::DrawConstants::default(),
            &per_frame,
            &draw,
            &quad_142(-1.0, -1.0, 1.0, 1.0, 0.9, green),
        )
        .expect("the far surface");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    };

    let control = render(&mut gpu, false);
    assert_eq!(gpu.portal_stamps, 0);
    let stamped = render(&mut gpu, true);
    assert_eq!(gpu.portal_stamps, 1, "one portal-polygon draw was issued");

    for (i, px) in control.as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(
            (px[0], px[1], px[2]),
            (255, 0, 0),
            "pixel {i} of the control is {px:?}, not the near surface"
        );
    }

    #[allow(clippy::cast_precision_loss)] // a 64-pixel extent
    let fn_ = N as f32;
    let (mut inside_green, mut outside_red) = (0u32, 0u32);
    for (i, px) in stamped.as_chunks::<4>().0.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let (cx, cy) = (
            ((i as u32 % N) as f32 + 0.5) / fn_ * 2.0 - 1.0,
            1.0 - ((i as u32 / N) as f32 + 0.5) / fn_ * 2.0,
        );
        let inside = cx > sx0 && cx < sx1 && cy > sy0 && cy < sy1;
        if inside {
            assert_eq!(
                (px[0], px[1], px[2]),
                (0, 255, 0),
                "pixel {i} is inside the stamp and is {px:?}: the far surface did not get through"
            );
            inside_green += 1;
        } else {
            assert_eq!((px[0], px[1], px[2]), (255, 0, 0), "pixel {i} is outside the stamp and is {px:?}: the stamp cleared depth it does not own");
            outside_red += 1;
        }
    }
    assert_eq!(
        inside_green,
        N * N / 4,
        "the stamp covers a quarter of the frame"
    );
    assert_eq!(inside_green + outside_red, N * N);
    gpu.release_texture(tex);
    gpu.wait_idle().expect("idle");
}

/// The stamp writes **depth only**: a frame with it is byte-identical to one without.
#[test]
fn the_portal_stamp_writes_no_colour() {
    const N: u32 = 64;
    let Some(mut gpu) = warp(N, N) else { return };
    let tex = gpu.upload_texture(&white()).expect("upload");
    let per_frame = identity_frame();
    let draw = PerDrawConstants::identity();

    let render = |gpu: &mut Gpu, stamp: bool| -> Vec<u8> {
        gpu.begin_frame().expect("begin");
        gpu.bind_texture(tex, 0);
        gpu.draw_dynamic(
            &opaque_key(),
            &crate::DrawConstants::default(),
            &per_frame,
            &draw,
            &quad_142(-1.0, -1.0, 1.0, 1.0, 0.2, 0xFF33_66AA),
        )
        .expect("a surface");
        if stamp {
            let poly: Vec<[f32; 4]> = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
                .into_iter()
                .map(|(x, y): (f32, f32)| [x, y, 0.5, 1.0])
                .collect();
            gpu.draw_portal_poly(&per_frame, &poly, crate::pso::portal_stamp_mask::BUILDING)
                .expect("the stamp");
        }
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    };

    let plain = render(&mut gpu, false);
    let stamped = render(&mut gpu, true);
    assert_eq!(plain, stamped, "the stamp put colour in the frame buffer");
    gpu.release_texture(tex);
    gpu.wait_idle().expect("idle");
}

/// Fewer than three vertices — a polygon the clipper left below a triangle is not drawn at all.
#[test]
fn a_portal_polygon_below_three_vertices_is_not_stamped() {
    let Some(mut gpu) = warp(64, 64) else { return };
    gpu.begin_frame().expect("begin");
    for n in 0..3usize {
        let poly: Vec<[f32; 4]> = (0..n).map(|i| [i as f32 * 0.1, 0.0, 0.5, 1.0]).collect();
        gpu.draw_portal_poly(
            &identity_frame(),
            &poly,
            crate::pso::portal_stamp_mask::BUILDING,
        )
        .expect("no draw, no error");
    }
    assert_eq!(
        gpu.portal_stamps, 0,
        "a sub-triangle polygon must not reach the device"
    );
    gpu.end_frame().expect("end");
    gpu.wait_idle().expect("idle");
}

/// A 4x4 BGRA texture with one level. Small on purpose: these tests are about descriptor
/// bookkeeping, not about pixels, and every upload costs a wait.
fn tiny_texture() -> TextureData {
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![vec![0x7Fu8; 4 * 4 * 4]],
    }
}

// Oracle: "a create/release/create cycle reuses the slot rather than advancing the counter" --
// against the real device, so the wiring (fence value, retirement point, resource lifetime) is
// proved and not just the algebra.
#[test]
fn a_texture_released_and_re_uploaded_gets_its_descriptor_pair_back() {
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let first = gpu.upload_texture(&t).expect("upload");
    assert_eq!(gpu.descriptor_usage().frontier, 1);
    assert_eq!(gpu.live_textures(), 1);
    assert_eq!(gpu.release_texture(first), Released::Freed);
    assert_eq!(gpu.live_textures(), 0);
    assert_eq!(gpu.descriptor_usage().pending, 1);
    assert_eq!(gpu.descriptor_usage().free, 0);
    // Uploads do not wait for the device, so a released slot comes back once the device has passed
    // the release; in the running client every frame does that. This wait stands in for the frames.
    gpu.wait_idle().expect("idle");
    let second = gpu.upload_texture(&t).expect("upload");
    assert_eq!(second, first, "the same descriptor pair");
    let usage = gpu.descriptor_usage();
    assert_eq!(usage.frontier, 1, "the heap did not grow");
    assert_eq!(usage.live, 1);
    assert_eq!(gpu.descriptor_stats().reuses, 1);
    gpu.release_texture(second);
    gpu.wait_idle().expect("idle");
}

#[test]
fn n_device_releases_then_n_uploads_leave_the_heap_high_water_at_n() {
    const N: usize = 32;
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let first: Vec<TextureSlot> = (0..N)
        .map(|_| gpu.upload_texture(&t).expect("upload"))
        .collect();
    assert_eq!(gpu.descriptor_usage().frontier, N as u32);
    for s in &first {
        assert_eq!(gpu.release_texture(*s), Released::Freed);
    }
    assert_eq!(gpu.live_textures(), 0);
    // Uploads do not wait for the device, so a released slot comes back once the device has passed
    // the release; in the running client every frame does that. This wait stands in for the frames.
    gpu.wait_idle().expect("idle");
    let second: Vec<TextureSlot> = (0..N)
        .map(|_| gpu.upload_texture(&t).expect("upload"))
        .collect();
    let usage = gpu.descriptor_usage();
    assert_eq!(
        usage.frontier, N as u32,
        "2N would be the unbounded behaviour this replaces"
    );
    assert_eq!(usage.high_water, N as u32);
    assert_eq!(usage.live, N as u32);
    assert_eq!(gpu.descriptor_stats().reuses, N as u64);
    let mut a: Vec<u32> = first.iter().map(|s| s.0).collect();
    let mut b: Vec<u32> = second.iter().map(|s| s.0).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "the same set of slots, not a fresh run of them");
    for s in second {
        gpu.release_texture(s);
    }
    gpu.wait_idle().expect("idle");
}

// A release made inside an open frame must NOT be governed by the value that a mid-frame
// upload's wait completes -- the frame's own command buffer has not been submitted yet.
#[test]
fn a_slot_released_inside_a_frame_survives_an_intervening_wait_idle() {
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let slot = gpu.upload_texture(&t).expect("upload");
    gpu.begin_frame().expect("begin");
    gpu.bind_texture(slot, 0);
    assert_eq!(gpu.release_texture(slot), Released::Freed);
    let usage = gpu.descriptor_usage();
    assert_eq!(
        usage.deferred, 1,
        "held until the frame that may read it is submitted"
    );
    assert_eq!(usage.live, 1, "and still occupying budget until then");
    assert_eq!(usage.pending, 0);
    assert_eq!(usage.free, 0);
    let other = gpu.upload_texture(&t).expect("a mid-frame upload is legal");
    assert_ne!(
        other, slot,
        "the released slot must not be reissued to this upload"
    );
    assert_eq!(
        gpu.descriptor_usage().free,
        0,
        "reissuing here is the use-after-free"
    );
    assert_eq!(gpu.descriptor_usage().deferred, 1, "still held");
    gpu.end_frame().expect("end");
    let usage = gpu.descriptor_usage();
    assert_eq!(usage.deferred, 0);
    assert_eq!(usage.pending + usage.free, 1);
    gpu.wait_idle().expect("idle");
    gpu.begin_frame().expect("begin");
    let usage = gpu.descriptor_usage();
    assert_eq!(usage.pending, 0, "the fence has passed");
    assert_eq!(usage.free, 1);
    assert_eq!(gpu.descriptor_stats().retired, 1);
    assert!(gpu.frame_open(), "inside the frame this test just began");
    gpu.end_frame().expect("end");
    assert!(!gpu.frame_open());
    gpu.release_texture(other);
    gpu.wait_idle().expect("idle");
}

// A frame cannot be silently abandoned and begun again: `begin_frame` refuses while one is open,
// and the held release is still there for the eventual `end_frame`.
#[test]
fn a_frame_cannot_be_abandoned_so_a_held_release_is_never_stranded_silently() {
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let slot = gpu.upload_texture(&t).expect("upload");
    gpu.begin_frame().expect("begin");
    assert_eq!(gpu.release_texture(slot), Released::Freed);
    assert_eq!(gpu.descriptor_usage().deferred, 1);
    let e = gpu
        .begin_frame()
        .expect_err("a second begin_frame with no end_frame is refused");
    assert!(format!("{e}").contains("already open"), "{e}");
    assert!(
        gpu.frame_open(),
        "the frame is still the one that was begun"
    );
    assert_eq!(
        gpu.descriptor_usage().deferred,
        1,
        "and the release is still held, not lost"
    );
    gpu.end_frame().expect("end");
    assert_eq!(gpu.descriptor_usage().deferred, 0);
    gpu.wait_idle().expect("idle");
    gpu.begin_frame().expect("begin");
    assert_eq!(gpu.descriptor_usage().free, 1, "and it came back");
    gpu.end_frame().expect("end");
    gpu.wait_idle().expect("idle");
}

#[test]
fn a_full_descriptor_heap_fails_cleanly_and_counts_the_refusal() {
    let Some(mut gpu) = warp(64, 64) else { return };
    gpu.narrow_descriptor_budget(4);
    let t = tiny_texture();
    let held: Vec<TextureSlot> = (0..4)
        .map(|_| gpu.upload_texture(&t).expect("upload"))
        .collect();
    for i in 0..3 {
        let e = gpu.upload_texture(&t).expect_err("the budget is full");
        assert!(matches!(e, RenderError::Device(_)), "{e}");
        assert!(format!("{e}").contains("descriptor heap exhausted"), "{e}");
        assert_eq!(
            gpu.descriptor_stats().exhaustions,
            i + 1,
            "every refusal is counted"
        );
    }
    assert_eq!(gpu.live_textures(), 4);
    assert_eq!(gpu.descriptor_usage().live, 4);
    gpu.begin_frame().expect("the device still renders");
    for s in &held {
        gpu.bind_texture(*s, 0);
    }
    gpu.end_frame().expect("end");
    assert_eq!(gpu.release_texture(held[0]), Released::Freed);
    let again = gpu.upload_texture(&t).expect("a released slot makes room");
    assert_eq!(again, held[0]);
    assert_eq!(
        gpu.descriptor_stats().exhaustions,
        3,
        "and no further refusals"
    );
    for s in &held[1..] {
        gpu.release_texture(*s);
    }
    gpu.release_texture(again);
    gpu.wait_idle().expect("idle");
}

#[test]
fn a_keyed_upload_of_a_cached_texture_costs_no_descriptors() {
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let key = TextureKey::world(crate::descriptor::combined_texture_key(
        0x0400_0001,
        0x0600_0002,
    ));
    let a = gpu.upload_texture_keyed(key, &t).expect("upload");
    let b = gpu.upload_texture_keyed(key, &t).expect("cache hit");
    assert_eq!(a, b);
    assert_eq!(gpu.descriptor_usage().frontier, 1, "one texture, one pair");
    assert_eq!(gpu.texture_table_stats().hits, 1);
    assert_eq!(gpu.live_textures(), 1);
    assert_eq!(gpu.release_texture(a), Released::StillLinked(1));
    assert_eq!(gpu.descriptor_usage().live, 1);
    assert_eq!(gpu.release_texture(b), Released::Freed);
    assert_eq!(gpu.descriptor_usage().live, 0);
    let c = gpu.upload_texture(&t).expect("upload");
    let d = gpu.upload_texture(&t).expect("upload");
    assert_ne!(c, d, "custom_texture_table entries are never shared");
    assert_eq!(
        gpu.texture_table_stats().hits,
        1,
        "and neither was a cache hit"
    );
    gpu.release_texture(c);
    gpu.release_texture(d);
    gpu.wait_idle().expect("idle");
}

#[test]
fn a_double_release_at_the_device_is_counted_and_frees_the_pair_once() {
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let slot = gpu.upload_texture(&t).expect("upload");
    assert_eq!(gpu.release_texture(slot), Released::Freed);
    assert_eq!(gpu.release_texture(slot), Released::Unknown);
    assert_eq!(gpu.release_texture(TextureSlot(9998)), Released::Unknown);
    assert_eq!(gpu.texture_table_stats().unknown_releases, 2);
    assert_eq!(gpu.texture_table_stats().frees, 1);
    gpu.wait_idle().expect("idle");
    gpu.begin_frame().expect("begin");
    gpu.end_frame().expect("end");
    assert_eq!(
        gpu.descriptor_stats().retired,
        1,
        "one slot came back, not two"
    );
    assert_eq!(
        gpu.descriptor_stats().invalid_releases,
        0,
        "and the allocator saw one release"
    );
    gpu.wait_idle().expect("idle");
}

#[test]
fn a_long_session_of_screens_stays_bounded_by_what_is_resident() {
    let Some(mut gpu) = warp(64, 64) else { return };
    let t = tiny_texture();
    let mut resident: Vec<TextureSlot> = Vec::new();
    for _ in 0..12 {
        for s in resident.drain(..) {
            assert_eq!(gpu.release_texture(s), Released::Freed);
        }
        // Uploads do not wait for the device, so a released slot comes back once the device has passed
        // the release; in the running client every frame does that. This wait stands in for the frames.
        gpu.wait_idle().expect("idle");
        for _ in 0..16 {
            resident.push(
                gpu.upload_texture(&t)
                    .expect("a screen must not exhaust the heap"),
            );
        }
    }
    let usage = gpu.descriptor_usage();
    assert_eq!(usage.live, 16);
    assert_eq!(usage.high_water, 16);
    assert_eq!(
        usage.frontier, 16,
        "192 uploads across 12 screens used {} slots; the monotonic version used 192",
        usage.frontier
    );
    assert_eq!(gpu.descriptor_stats().allocations, 192);
    assert_eq!(gpu.descriptor_stats().reuses, 176);
    assert_eq!(gpu.descriptor_stats().exhaustions, 0);
    for s in resident.drain(..) {
        gpu.release_texture(s);
    }
    gpu.wait_idle().expect("idle");
}

/// Every catalogue row constructs a pipeline the driver accepts.
#[test]
fn every_catalogue_row_constructs_a_pipeline_the_driver_accepts() {
    let cfg = DeviceConfig {
        width: 64,
        height: 64,
        debug: true,
        ..DeviceConfig::default()
    };
    let Ok(mut gpu) = Gpu::new(None, &cfg) else {
        eprintln!("skipping: no Vulkan device available");
        return;
    };
    let built = gpu
        .build_whole_catalogue()
        .expect("the driver must accept every row");
    // 15 rows x 5 FVF families.
    assert_eq!(built, 15 * 5);
    let messages = gpu.debug_messages();
    assert!(
        messages.is_empty(),
        "the validation layer objected:{messages}"
    );
}

/// A capture after three heavy frames is clean under validation.
#[test]
fn a_capture_after_three_heavy_frames_is_clean_under_validation() {
    let cfg = DeviceConfig {
        width: 800,
        height: 600,
        debug: true,
        ..DeviceConfig::default()
    };
    let Ok(mut gpu) = Gpu::new(None, &cfg) else {
        eprintln!("skipping: no Vulkan device available");
        return;
    };
    let tex = gpu.upload_texture(&white()).expect("upload");
    let per_frame = identity_frame();
    let draw = PerDrawConstants::identity();
    let _ = gpu.debug_messages();
    const HEAVY: usize = 100;
    for _ in 0..FRAME_COUNT {
        gpu.begin_frame().expect("begin");
        gpu.bind_texture(tex, 0);
        for _ in 0..HEAVY {
            gpu.draw_dynamic(
                &opaque_key(),
                &crate::DrawConstants::default(),
                &per_frame,
                &draw,
                &quad_142(-1.0, -1.0, 1.0, 1.0, 0.5, 0xFF20_40FF),
            )
            .expect("draw");
        }
        gpu.end_frame().expect("end");
    }
    let image = gpu.capture().expect("capture");
    assert_eq!(image.bgra.len(), 800 * 600 * 4);
    assert!(
        image
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| (p[0], p[1], p[2]) == (0xFF, 0x40, 0x20)),
        "the last frame's fill"
    );
    let messages = gpu.debug_messages();
    assert!(
        messages.is_empty(),
        "the validation layer objected:{messages}"
    );
    gpu.release_texture(tex);
    gpu.wait_idle().expect("idle");
}

#[test]
fn a_thousand_frames_cycle_the_ring_without_stalling() {
    let Some(mut gpu) = warp(64, 64) else { return };
    for _ in 0..1000 {
        gpu.begin_frame().expect("begin");
        gpu.end_frame().expect("end");
    }
    assert_eq!(
        gpu.frame_stamp, 1000,
        "the frame stamp is bumped once per frame"
    );
    gpu.wait_idle().expect("idle");
}

#[test]
fn the_headless_capture_is_deterministic_across_runs() {
    let mut captures = Vec::new();
    for _ in 0..3 {
        let Some(mut gpu) = warp(800, 600) else {
            return;
        };
        gpu.begin_frame().expect("begin");
        gpu.end_frame().expect("end");
        captures.push(gpu.capture().expect("capture").to_rgba());
    }
    assert_eq!(captures[0].len(), 800 * 600 * 4);
    assert_eq!(captures[0], captures[1]);
    assert_eq!(captures[1], captures[2]);
    assert!(captures[0]
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    assert!(captures[0].as_chunks::<4>().0.iter().all(|p| p[3] == 0xFF));
}

/// A capture before any frame is black, not undefined memory.
#[test]
fn a_capture_before_the_first_frame_is_black() {
    let Some(mut gpu) = warp(16, 16) else { return };
    let image = gpu.capture().expect("capture");
    assert!(image.bgra.iter().all(|b| *b == 0));
}

/// The presentation extent can change between frames; the offscreen target follows it.
#[test]
fn a_resize_rebuilds_the_target_at_the_new_extent() {
    let Some(mut gpu) = warp(16, 16) else { return };
    gpu.begin_frame().expect("begin");
    gpu.end_frame().expect("end");
    gpu.resize(32, 24).expect("resize");
    assert_eq!(gpu.size(), (32, 24));
    gpu.begin_frame().expect("begin");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    assert_eq!((image.width, image.height), (32, 24));
    assert_eq!(image.bgra.len(), 32 * 24 * 4);
}

// Oracle: blend and comparison enums must be translated by name because their numeric values differ.
#[test]
fn blend_and_compare_translate_by_name_not_by_value() {
    assert_eq!(to_vk_blend(Blend::SrcAlpha), vk::BlendFactor::SRC_ALPHA);
    assert_eq!(
        to_vk_blend(Blend::InvSrcAlpha),
        vk::BlendFactor::ONE_MINUS_SRC_ALPHA
    );
    assert_eq!(to_vk_blend(Blend::DestColor), vk::BlendFactor::DST_COLOR);
    assert_eq!(to_vk_blend(Blend::Zero), vk::BlendFactor::ZERO);
    assert_eq!(to_vk_blend(Blend::One), vk::BlendFactor::ONE);
    assert_eq!(
        to_vk_blend(Blend::InvDestColor),
        vk::BlendFactor::ONE_MINUS_DST_COLOR
    );
    assert_eq!(to_vk_compare(ZFunc::Less), vk::CompareOp::LESS);
    assert_eq!(
        to_vk_compare(ZFunc::LessEqual),
        vk::CompareOp::LESS_OR_EQUAL
    );
    assert_eq!(to_vk_compare(ZFunc::Always), vk::CompareOp::ALWAYS);
}

#[test]
fn colour_blend_factors_become_their_alpha_analogues_in_the_alpha_slots() {
    assert_eq!(
        to_vk_blend_alpha(Blend::DestColor),
        vk::BlendFactor::DST_ALPHA
    );
    assert_eq!(
        to_vk_blend_alpha(Blend::InvDestColor),
        vk::BlendFactor::ONE_MINUS_DST_ALPHA
    );
    assert_eq!(
        to_vk_blend_alpha(Blend::SrcColor),
        vk::BlendFactor::SRC_ALPHA
    );
    assert_eq!(
        to_vk_blend_alpha(Blend::InvSrcColor),
        vk::BlendFactor::ONE_MINUS_SRC_ALPHA
    );
    for b in [
        Blend::Zero,
        Blend::One,
        Blend::SrcAlpha,
        Blend::InvSrcAlpha,
        Blend::DestAlpha,
    ] {
        assert_eq!(to_vk_blend_alpha(b), to_vk_blend(b), "{b:?}");
    }
}

#[test]
fn the_back_buffer_format_is_non_srgb_and_bgra_ordered() {
    assert_eq!(BACK_BUFFER_FORMAT, vk::Format::B8G8R8A8_UNORM);
    assert_ne!(BACK_BUFFER_FORMAT, vk::Format::B8G8R8A8_SRGB);
    assert_eq!(
        DEPTH_FORMAT,
        vk::Format::D24_UNORM_S8_UINT,
        "D24S8 is the client's first choice"
    );
}

#[test]
fn the_module_does_not_change_the_fpu_control_word() {
    assert!(assert_fpu_untouched());
}

#[test]
fn the_gamma_value_is_clamped_the_way_setgamma_clamps_it() {
    let Some(mut gpu) = warp(16, 16) else { return };
    gpu.set_gamma(5.0);
    assert_eq!(gpu.gamma(), 1.0);
    gpu.set_gamma(-5.0);
    assert!((gpu.gamma() - (-0.2)).abs() < 1e-6);
}

#[test]
fn the_frame_ring_is_three_deep() {
    assert_eq!(FRAME_COUNT, 3);
}

/// The constant block the two dynamic offsets index must hold both structs at their offsets.
#[test]
fn the_constant_block_holds_both_structs_aligned() {
    assert!(std::mem::size_of::<PerFrameConstants>() <= PER_DRAW_OFFSET);
    assert_eq!(std::mem::size_of::<PerFrameConstants>() % 16, 0);
    assert_eq!(std::mem::size_of::<PerDrawConstants>() % 16, 0);
    assert_eq!(CONSTANT_BLOCK_BYTES, 256 + 416);
}
