//! Behaviour: none (experimental)
//!
//! The remaster's sun shadows and ambient occlusion traced through its acceleration structures:
//! the traced sun's share agrees with the cascades' near the eye; the traced occlusion is darker
//! where the screen-space occlusion is; both hold still under a still view; the traced shadows on
//! far mountains hold still while the view is held under the anti-aliasing and while it turns;
//! and where the device does not trace rays, the cascades and the screen-space occlusion draw
//! them, the frame the same as with nothing traced.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterAo, RemasterCapture, RemasterLight, RemasterShadows, RemasterTracedAo,
    RemasterTracedShadows,
};

use crate::instruments::stations::{self, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// How far from the eye the cascades and the traced shadows are compared, meters: the first
/// cascades' reach.
const NEAR: f32 = 45.0;

/// What a run draws.
#[derive(Debug, Clone, Copy, Default)]
struct Setup {
    /// The cascades, traced or not.
    shadows: bool,
    traced_shadows: bool,
    /// The screen-space occlusion, traced or not.
    ao: bool,
    traced_ao: bool,
}

/// A run's frames: each one's capture and occlusion (1 everywhere without it), and how many of
/// the remaster's frames traced the shadows and the occlusion.
struct Run {
    frames: Vec<(RemasterCapture, Vec<f32>)>,
    traced: (u64, u64),
    traces_rays: bool,
}

fn station(store: &RetailDatStore, name: &str) -> Station {
    stations::stations(store)
        .into_iter()
        .chain(stations::close_stations(store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// `station` on a device of its own (`cfg` changing its config), the remaster on with its light
/// per pixel and what `setup` asks, stepped with the clock held, its last `keep` of `frames`
/// frames captured.
fn run_on(
    store: &Arc<RetailDatStore>,
    station: &Station,
    setup: Setup,
    frames: usize,
    keep: usize,
    cfg: impl FnOnce(&mut DeviceConfig),
) -> Run {
    let mut config = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    cfg(&mut config);
    let mut gpu = Gpu::new_on(Backend::Vulkan, None, &config).expect("a Vulkan device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_settings(dereth_scene::world_scene::RemasterSettings::off(), &mut gpu);
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_shadows(
        RemasterShadows {
            on: setup.shadows,
            ..RemasterShadows::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_ao(
        RemasterAo {
            on: setup.ao,
            ..RemasterAo::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_traced_shadows(
        RemasterTracedShadows {
            on: setup.traced_shadows,
            ..RemasterTracedShadows::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_traced_ao(
        RemasterTracedAo {
            on: setup.traced_ao,
            ..RemasterTracedAo::default()
        },
        &mut gpu,
    );
    let mut out = Vec::new();
    for k in 0..frames {
        shot.step_held(&mut gpu);
        if k + keep >= frames {
            let capture = shot
                .scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew");
            let ao = match shot.scene.draw.remaster_ao_picture(&mut gpu) {
                Some((_, _, v)) => v,
                None => vec![1.0; (capture.width * capture.height) as usize],
            };
            out.push((capture, ao));
        }
    }
    let traced = shot
        .scene
        .draw
        .remaster_traced_frames(&mut gpu)
        .expect("the remaster is on");
    let traces_rays = shot.scene.draw.remaster_traces_rays(&mut gpu);
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{}: the validation layer: {errors:#?}",
        station.name
    );
    Run {
        frames: out,
        traced,
        traces_rays,
    }
}

fn run(store: &Arc<RetailDatStore>, station: &Station, setup: Setup) -> Run {
    run_on(store, station, setup, 6, 1, |_| {})
}

/// How far the pixel `i` of `c` stands from the eye, meters; infinite where nothing was drawn
/// with depth.
#[allow(clippy::cast_precision_loss)]
fn distance(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> f32 {
    if c.depth[i] >= 1.0 {
        return f32::INFINITY;
    }
    let (w, h) = (c.width as usize, c.height as usize);
    let (x, y) = ((i % w) as f32, (i / w) as f32);
    let ndc = glam::Vec4::new(
        (x + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y + 0.5) / h as f32 * 2.0,
        c.depth[i],
        1.0,
    );
    let p = *inv * ndc;
    ((p / p.w).truncate() - glam::Vec3::from(c.eye)).length()
}

/// The traced sun's share agrees with the cascades' near the eye: in the forest, and among its
/// trees at noon, of the pixels the sun lights within [`NEAR`] meters, under three in a hundred
/// are in shadow (under half the sun) by one and lit by the other, and the shadowed pixels'
/// counts are within a tenth of each other.
#[test]
fn traced_shadows_agree_with_the_cascades_near_the_eye() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["forest", "trees-n-noon"] {
        let at = station(&store, name);
        let cascades = run(
            &store,
            &at,
            Setup {
                shadows: true,
                ..Setup::default()
            },
        );
        let traced = run(
            &store,
            &at,
            Setup {
                shadows: true,
                traced_shadows: true,
                ..Setup::default()
            },
        );
        if !traced.traces_rays {
            eprintln!("{name}: the device does not trace rays");
            return;
        }
        assert!(traced.traced.0 > 0, "{name}: the shadows were not traced");
        assert_eq!(cascades.traced.0, 0, "{name}: traced without being asked");
        let (a, _) = &cascades.frames[0];
        let (b, _) = &traced.frames[0];
        let va = a.sun_visibility.as_ref().expect("the cascades' share");
        let vb = b.sun_visibility.as_ref().expect("the traced share");
        let inv = glam::Mat4::from_cols_array(&a.view_projection).inverse();
        let (mut near, mut differ, mut shade_a, mut shade_b) = (0u32, 0u32, 0u32, 0u32);
        for i in 0..a.class.len() {
            if !(a.class[i] == 1 || a.class[i] == 2) || distance(a, &inv, i) > NEAR {
                continue;
            }
            near += 1;
            let (sa, sb) = (va[i] < 128, vb[i] < 128);
            shade_a += u32::from(sa);
            shade_b += u32::from(sb);
            differ += u32::from(sa != sb);
        }
        eprintln!(
            "{name}: of {near} sunlit pixels within {NEAR} m, {shade_a} shadowed by the cascades, \
             {shade_b} traced, {differ} differ"
        );
        assert!(near > W * H / 20, "{name}: too little near: {near}");
        assert!(shade_a > near / 50, "{name}: too little shadow: {shade_a}");
        assert!(
            differ * 100 < near * 3,
            "{name}: {differ} of {near} near pixels differ"
        );
        assert!(
            shade_a.abs_diff(shade_b) * 10 <= shade_a.max(shade_b).max(10),
            "{name}: {shade_a} shadowed by the cascades against {shade_b} traced"
        );
    }
}

/// The traced occlusion is darker where the screen-space occlusion is: in the forest, and among
/// its trees at noon, the traced occlusion of the pixels the screen's leaves under four
/// fifths open is at least a tenth darker on average than that of the pixels it leaves fully
/// open; and every value lies between 0 and 1.
#[test]
fn traced_occlusion_is_darker_where_the_screens_is() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["forest", "trees-n-noon"] {
        let at = station(&store, name);
        let screen = run(
            &store,
            &at,
            Setup {
                ao: true,
                ..Setup::default()
            },
        );
        let traced = run(
            &store,
            &at,
            Setup {
                ao: true,
                traced_ao: true,
                ..Setup::default()
            },
        );
        if !traced.traces_rays {
            eprintln!("{name}: the device does not trace rays");
            return;
        }
        assert!(traced.traced.1 > 0, "{name}: the occlusion was not traced");
        let (_, sa) = &screen.frames[0];
        let (c, ta) = &traced.frames[0];
        assert!(
            ta.iter().all(|v| (0.0..=1.0).contains(v)),
            "{name}: an occlusion out of range"
        );
        let (mut dark, mut dark_n, mut open, mut open_n) = (0.0f64, 0u32, 0.0f64, 0u32);
        for i in 0..c.class.len() {
            if !(1..=2).contains(&c.class[i]) {
                continue;
            }
            if sa[i] < 0.8 {
                dark += f64::from(ta[i]);
                dark_n += 1;
            } else if sa[i] > 0.99 {
                open += f64::from(ta[i]);
                open_n += 1;
            }
        }
        let dark = dark / f64::from(dark_n.max(1));
        let open = open / f64::from(open_n.max(1));
        eprintln!(
            "{name}: traced occlusion {dark:.3} over {dark_n} pixels the screen's darkens, \
             {open:.3} over {open_n} it leaves open"
        );
        assert!(dark_n > 500 && open_n > 1_000, "{name}: too few pixels");
        assert!(
            dark + 0.1 < open,
            "{name}: the traced occlusion is not darker where the screen's is"
        );
    }
}

/// Traced, the sun's share and the occlusion hold still under a still view: with the clock held,
/// the last three of six frames at the forest are the same in every pixel of the world.
#[test]
fn traced_shadows_and_occlusion_hold_still_under_a_still_view() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let at = station(&store, "forest");
    let all = Setup {
        shadows: true,
        traced_shadows: true,
        ao: true,
        traced_ao: true,
    };
    // The scene's meshes reach the acceleration structures over the first frames.
    let r = run_on(&store, &at, all, 40, 3, |_| {});
    if !r.traces_rays {
        eprintln!("the device does not trace rays");
        return;
    }
    assert!(
        r.traced.0 >= 3 && r.traced.1 >= 3,
        "the frames were not traced: {:?}",
        r.traced
    );
    let (first, first_ao) = &r.frames[0];
    let (mut changed, mut sun, mut occlusion, mut color) = (0usize, 0usize, 0usize, 0usize);
    for (c, ao) in &r.frames[1..] {
        for i in 0..c.class.len() {
            if first.class[i] == 0 {
                continue;
            }
            let v0 = first.sun_visibility.as_ref().map(|v| v[i]);
            let v1 = c.sun_visibility.as_ref().map(|v| v[i]);
            let moved = (
                v0 != v1,
                first_ao[i].to_bits() != ao[i].to_bits(),
                first.color[i * 4..i * 4 + 4] != c.color[i * 4..i * 4 + 4],
            );
            sun += usize::from(moved.0);
            occlusion += usize::from(moved.1);
            color += usize::from(moved.2);
            changed += usize::from(moved.0 || moved.1 || moved.2);
        }
    }
    eprintln!(
        "forest, traced and held: {changed} world pixels changed          ({sun} sun, {occlusion} occlusion, {color} color)"
    );
    assert_eq!(changed, 0, "the traced frame does not hold still");
}

/// Where the device does not trace rays, the cascades and the screen-space occlusion draw: on a
/// device made without ray queries, with the shadows and the occlusion asked to be traced, the
/// remaster says it does not trace rays, traces no frame, and draws the forest exactly as it does
/// with nothing asked to be traced, its sun's share and occlusion too. On this machine's own
/// device, where it traces rays, the same asks trace.
#[test]
fn without_ray_queries_the_cascades_and_the_screens_occlusion_draw() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let at = station(&store, "forest");
    let plain = Setup {
        shadows: true,
        ao: true,
        ..Setup::default()
    };
    let asked = Setup {
        traced_shadows: true,
        traced_ao: true,
        ..plain
    };
    let without = |c: &mut DeviceConfig| c.remaster_ray_queries = false;
    let refused = run_on(&store, &at, asked, 6, 1, without);
    let reference = run_on(&store, &at, plain, 6, 1, without);
    eprintln!(
        "forest without ray queries: traces rays {}, traced frames {:?}",
        refused.traces_rays, refused.traced
    );
    assert!(
        !refused.traces_rays,
        "a device made without ray queries traces them"
    );
    assert_eq!(refused.traced, (0, 0), "frames traced without ray queries");
    let ((a, a_ao), (b, b_ao)) = (&refused.frames[0], &reference.frames[0]);
    assert!(
        a.sun_visibility.is_some(),
        "the cascades drew no sun's share"
    );
    assert_eq!(
        a.sun_visibility, b.sun_visibility,
        "the sun's share differs"
    );
    assert!(
        a_ao.iter()
            .zip(b_ao)
            .all(|(x, y)| x.to_bits() == y.to_bits()),
        "the occlusion differs"
    );
    assert_eq!(a.color, b.color, "the frame differs");
    let traced = run(&store, &at, asked);
    eprintln!(
        "forest on this device: traces rays {}, traced frames {:?}",
        traced.traces_rays, traced.traced
    );
    if traced.traces_rays {
        assert!(
            traced.traced.0 > 0 && traced.traced.1 > 0,
            "asked, nothing was traced"
        );
    }
}

/// A hill west of Holtburg from 25 metres over it, looking out past the green hills at the
/// snow-capped mountains six kilometres off, early on a sunny morning, with the land held 24
/// blocks out so the mountains' ground is in the acceleration structures.
fn far_hill() -> Station {
    Station {
        name: "far-hill",
        why: "far mountains under a low morning sun",
        place: stations::Place::Free {
            block: 0xA9B7,
            x: 112.75,
            y: 6.95,
            eye: 27.0,
            yaw: 20f32.to_radians(),
            pitch: -0.12,
        },
        clock: stations::Clock::Sunny(0.342),
        weather: false,
        land_radius: 24,
    }
}

/// How far off land is far, meters.
const FAR: f32 = 1000.0;

/// The far land at [`far_hill`] drawn with the traced sun's shadows and the far detail, the clock
/// held: each frame's capture, the camera turned by `turn(k)` radians at step `k`, the
/// anti-aliasing (whose projection moves by part of a pixel every frame) on as `taa` says; the last `keep` of
/// `frames` captured. `None` where the device does not trace rays.
fn far_frames(
    store: &Arc<RetailDatStore>,
    taa: bool,
    frames: usize,
    keep: usize,
    turn: impl Fn(usize) -> f32,
) -> Option<Vec<RemasterCapture>> {
    let config = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Vulkan, None, &config).expect("a Vulkan device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let mut shot = Shot::open(store, &mut gpu, &far_hill());
    shot.poll(&mut gpu);
    // The ground as the remaster draws it, which far off is coarser than retail's.
    let boxes = format!(
        "Remaster=1,RemasterShadows=1,RemasterTracedShadows=1,RemasterDetail=1,RemasterSmoothing={}",
        u8::from(taa)
    );
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    if !shot.scene.draw.remaster_on() {
        shot.scene
            .draw
            .set_remaster(true, store, &mut gpu)
            .expect("the remaster starts");
        shot.poll(&mut gpu);
    }
    if !shot.scene.draw.remaster_traces_rays(&mut gpu) {
        return None;
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let camera = shot.scene.camera;
    let mut out = Vec::new();
    for k in 0..frames {
        shot.hold_camera(camera.position, camera.yaw + turn(k), camera.pitch);
        shot.step_held(&mut gpu);
        if k + keep >= frames {
            out.push(
                shot.scene
                    .draw
                    .remaster_capture(&mut gpu)
                    .expect("the remaster drew"),
            );
        }
    }
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    Some(out)
}

/// The share of the far land's pixels of `now` (those the sun lights, past [`FAR`] meters) in
/// the sun's shadow (under half of it) by one of `was` and `now` and lit by the other; with
/// `inside`, only those every pixel round which agrees with it in `was`, so the change is not a
/// shadow's edge moving by part of a pixel. And how many far pixels there are.
#[allow(clippy::cast_precision_loss)] // pixel counts
fn far_flips(was: &RemasterCapture, now: &RemasterCapture, inside: bool) -> (f64, usize) {
    let (a, b) = (
        was.sun_visibility.as_ref().expect("the sun's share"),
        now.sun_visibility.as_ref().expect("the sun's share"),
    );
    let inv = glam::Mat4::from_cols_array(&now.view_projection).inverse();
    let (w, h) = (now.width as usize, now.height as usize);
    let dark = |v: &[u8], i: usize| v[i] < 128;
    let far: Vec<usize> = (0..now.class.len())
        .filter(|&i| matches!(now.class[i], 1 | 2) && distance(now, &inv, i) > FAR)
        .collect();
    let flips = far
        .iter()
        .filter(|&&i| {
            let (x, y) = (i % w, i / w);
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h || dark(a, i) == dark(b, i) {
                return false;
            }
            !inside
                || (y - 1..=y + 1)
                    .all(|yy| (x - 1..=x + 1).all(|xx| dark(a, yy * w + xx) == dark(a, i)))
        })
        .count();
    (flips as f64 / far.len().max(1) as f64, far.len())
}

/// The most of `frames`' far land flipped between one frame and the next, as [`far_flips`]
/// counts it.
fn worst_flips(frames: &[RemasterCapture], inside: bool) -> f64 {
    let mut worst = 0.0f64;
    for pair in frames.windows(2) {
        let (share, far) = far_flips(&pair[0], &pair[1], inside);
        assert!(far > 2000, "too little far land in view: {far} pixels");
        worst = worst.max(share);
    }
    worst
}

/// The most of the far land's pixels whose sun's share may flip from one frame to the next while
/// the view is held under the anti-aliasing. Measured: 0.0001; 0.052 while the far ground drawn
/// under retail's, or a far point whose depth put it behind its surface, shaded itself.
const HELD_FLIPS: f64 = 0.005;

/// The most of the far land's pixels whose sun's share may flip from one frame to the next away
/// from a shadow's edge while the view turns. Measured: 0.0006; 0.049 before, as above.
const TURNING_FLIPS: f64 = 0.005;

/// **The traced shadows on the far mountains hold still while the view is held**: at
/// [`far_hill`] with the anti-aliasing on, whose projection moves by part of a pixel every frame,
/// under one in two hundred of the far land's pixels (past [`FAR`] meters) goes from the sun to
/// the shadow or back from one frame to the next, over sixteen frames.
#[test]
fn traced_shadows_on_far_mountains_hold_still_while_the_view_is_held() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let Some(frames) = far_frames(&store, true, 56, 17, |_| 0.0) else {
        eprintln!("the device does not trace rays");
        return;
    };
    let worst = worst_flips(&frames, false);
    eprintln!("far hill, held: at most {worst:.4} of the far land flipped in a frame");
    assert!(
        worst < HELD_FLIPS,
        "the far land's traced shadows flicker: {worst:.4} of it flipped in a frame"
    );
}

/// **The traced shadows on the far mountains hold still while the view turns**: at
/// [`far_hill`] without the anti-aliasing, the camera turning by a twentieth of a pixel a frame
/// one way and then back, under one in two hundred of the far land's pixels goes from the sun
/// to the shadow or back away from a shadow's edge from one frame to the next, over sixteen
/// frames.
#[test]
fn traced_shadows_on_far_mountains_hold_still_while_the_view_turns() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    // A pixel spans about a 600th of a radian across the view.
    #[allow(clippy::cast_precision_loss)] // a step
    let turn = |k: usize| 8e-5 * ((k % 16) as f32 - 8.0).abs();
    let Some(frames) = far_frames(&store, false, 56, 17, turn) else {
        eprintln!("the device does not trace rays");
        return;
    };
    let worst = worst_flips(&frames, true);
    eprintln!("far hill, turning: at most {worst:.4} of the far land flipped in a frame");
    assert!(
        worst < TURNING_FLIPS,
        "the far land's traced shadows flicker: {worst:.4} of it flipped in a frame"
    );
}
