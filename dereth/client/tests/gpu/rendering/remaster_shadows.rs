//! The remaster's sun shadows: still under a still camera and a held sun, their cached cascades
//! drawing nothing again; free of pops as the camera walks, the casters being everything resident
//! rather than what is in view; and cast where the forest's trees stand.
//!
//! A pop is a shadow that appears or vanishes from one frame to the next where nothing moved. It
//! is counted by reprojection: each pixel the sun lights in a frame is found in the frame before
//! by its place in the world; where both frames saw the same spot, a change in the sun's share
//! there of more than a quarter is a changed pixel. A walk's frames change only along the shadows'
//! edges, a few pixels each; a pop changes a shadow's whole area at once.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterCapture, RemasterLight, RemasterShadowCasters, RemasterShadows,
};
use glam::{Mat4, Vec3, Vec4};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{Shot, Station};

/// The light classes the sun lights: the ground, and what stands in the sun.
fn sunlit(class: u8) -> bool {
    class == 1 || class == 2
}

/// The forest station, `east` metres east of where it stands, drawn by the remaster per pixel
/// under the sun's shadows from `casters`, on a device of its own.
fn forest(
    store: &Arc<RetailDatStore>,
    casters: RemasterShadowCasters,
    east: f32,
) -> (Gpu, Shot, Station) {
    let mut station = remaster_frames::stations(store, &["forest"])
        .pop()
        .expect("the forest");
    if let crate::instruments::stations::Place::Free { x, .. } = &mut station.place {
        *x += east;
    }
    let cfg = DeviceConfig {
        width: 960,
        height: 540,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_shadows(
        RemasterShadows {
            on: true,
            casters,
            ..RemasterShadows::default()
        },
        &mut gpu,
    );
    (gpu, shot, station)
}

/// The remaster's last frame.
fn capture(shot: &Shot, gpu: &mut Gpu) -> RemasterCapture {
    shot.scene
        .draw
        .remaster_capture(gpu)
        .expect("the remaster drew")
}

/// Where pixel `(x, y)` of a capture stands in the frame's space, from its depth.
#[allow(clippy::cast_precision_loss)] // pixel coordinates
fn place(c: &RemasterCapture, inv: &Mat4, x: u32, y: u32) -> Option<Vec3> {
    let i = (y * c.width + x) as usize;
    let d = *c.depth.get(i)?;
    if d >= 1.0 {
        return None;
    }
    let ndc = Vec4::new(
        (x as f32 + 0.5) / c.width as f32 * 2.0 - 1.0,
        1.0 - (y as f32 + 0.5) / c.height as f32 * 2.0,
        d,
        1.0,
    );
    let h = *inv * ndc;
    Some(h.truncate() / h.w)
}

/// The pixels the sun lights in `now` whose share changed by more than a quarter from `before`,
/// where `before` saw the same spot; and how many such spots both saw.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn changed(before: &RemasterCapture, now: &RemasterCapture) -> (usize, usize) {
    let (Some(vb), Some(vn)) = (before.sun_visibility.as_ref(), now.sun_visibility.as_ref()) else {
        panic!("both frames drew the sun's shadows");
    };
    let inv_now = Mat4::from_cols_array(&now.view_projection).inverse();
    let inv_before = Mat4::from_cols_array(&before.view_projection).inverse();
    let to_before = Mat4::from_cols_array(&before.view_projection);
    let (mut seen, mut moved) = (0, 0);
    for y in 0..now.height {
        for x in 0..now.width {
            let i = (y * now.width + x) as usize;
            if !sunlit(now.class[i]) {
                continue;
            }
            let Some(p) = place(now, &inv_now, x, y) else {
                continue;
            };
            let q = to_before * p.extend(1.0);
            let (u, v) = (q.x / q.w, q.y / q.w);
            let bx = ((u + 1.0) * 0.5 * before.width as f32).floor();
            let by = ((1.0 - v) * 0.5 * before.height as f32).floor();
            if bx < 0.0 || by < 0.0 || bx >= before.width as f32 || by >= before.height as f32 {
                continue;
            }
            let (bx, by) = (bx as u32, by as u32);
            let j = (by * before.width + bx) as usize;
            if !sunlit(before.class[j]) {
                continue;
            }
            // The same spot, seen in both: the place before within a few centimetres a metre.
            let Some(pb) = place(before, &inv_before, bx, by) else {
                continue;
            };
            let eye = Vec3::from(now.eye);
            if (pb - p).length() > 0.05 + 0.01 * (p - eye).length() {
                continue;
            }
            seen += 1;
            // Changed only beyond the share around that spot before: the walk moves each spot by
            // a fraction of a pixel, which moves a shadow's edge across a pixel without a pop.
            let (mut lo, mut hi) = (u8::MAX, u8::MIN);
            for ny in by.saturating_sub(1)..=(by + 1).min(before.height - 1) {
                for nx in bx.saturating_sub(1)..=(bx + 1).min(before.width - 1) {
                    let k = (ny * before.width + nx) as usize;
                    if sunlit(before.class[k]) {
                        lo = lo.min(vb[k]);
                        hi = hi.max(vb[k]);
                    }
                }
            }
            if u16::from(vn[i]) + 64 < u16::from(lo) || u16::from(vn[i]) > u16::from(hi) + 64 {
                moved += 1;
            }
        }
    }
    (moved, seen)
}

/// Under a still camera with the sun held, frame after frame is the same: the sun's share and the
/// picture don't change by a single byte, and the cascades, which hold still, draw none of the
/// blocks' casters again after the first frame.
#[test]
fn a_still_view_under_a_held_sun_changes_no_pixel_and_draws_no_cached_caster_again() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut gpu, mut shot, _) = forest(&store, RemasterShadowCasters::Resident, 0.0);
    shot.step_held(&mut gpu);
    let first = shot.scene.draw.remaster_counts(&mut gpu).expect("counted");
    assert_eq!(
        first.shadows.last_redrawn, 4,
        "the first frame draws every cascade"
    );
    assert!(first.shadows.last_static_items > 0, "{:?}", first.shadows);
    let mut last = capture(&shot, &mut gpu);
    for k in 1..6 {
        shot.step_held(&mut gpu);
        let c = shot.scene.draw.remaster_counts(&mut gpu).expect("counted");
        assert_eq!(c.shadows.last_redrawn, 0, "frame {k}: {:?}", c.shadows);
        assert_eq!(c.shadows.last_static_items, 0, "frame {k}");
        let now = capture(&shot, &mut gpu);
        assert_eq!(
            now.sun_visibility, last.sun_visibility,
            "frame {k}: the sun's share"
        );
        // The world's pixels; the sky's clouds drift with every frame drawn, as retail's do.
        let moved = now
            .color
            .chunks(4)
            .zip(last.color.chunks(4))
            .zip(&now.class)
            .filter(|((a, b), class)| **class != 0 && a != b)
            .count();
        assert_eq!(moved, 0, "frame {k}: the lit world");
        last = now;
    }
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// The camera walks 30 metres through the forest with the sun held, half a metre a frame, along
/// a path that passes the trees rather than through one, and each frame's changed pixels and the
/// pixels it and the frame before both saw.
fn walk(store: &Arc<RetailDatStore>, casters: RemasterShadowCasters) -> Vec<(usize, usize)> {
    let (mut gpu, mut shot, station) = forest(store, casters, 10.0);
    let mut out = Vec::new();
    shot.walk_to(&station, 0.0);
    shot.step_held(&mut gpu);
    let mut last = capture(&shot, &mut gpu);
    for k in 1..=60u8 {
        shot.walk_to(&station, f32::from(k) * 0.5);
        shot.step_held(&mut gpu);
        let now = capture(&shot, &mut gpu);
        out.push(changed(&last, &now));
        last = now;
    }
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    out
}

/// The most pixels of a walk's frame whose sun's share changes beyond its neighbourhood's by more
/// than a quarter, out of every hundred thousand the frame and the one before both saw: about
/// twice the most measured along the walk with everything resident casting (194). Casting from
/// what is in view alone, the same walk's worst frame measured 15,852.
const WALK_CHANGE_LIMIT: f64 = 400.0;

/// Walking through the forest under a held sun, no frame's shadows pop: with every resident caster
/// casting, the share of pixels whose sun changes stays under the limit at every frame. Cast from
/// what is in view alone, as shadows drawn from the view are, the same walk pops past it.
#[test]
fn walking_through_the_forest_the_shadows_never_pop() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let rate = |(moved, seen): (usize, usize)| {
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let r = moved as f64 * 100_000.0 / seen.max(1) as f64;
        r
    };
    let resident = walk(&store, RemasterShadowCasters::Resident);
    let worst = resident.iter().copied().map(rate).fold(0.0, f64::max);
    eprintln!(
        "everything resident: the worst frame {worst:.0} in 100,000; frames {:?}",
        resident
    );
    assert!(
        resident.iter().all(|(_, seen)| *seen > 50_000),
        "{resident:?}"
    );
    assert!(worst < WALK_CHANGE_LIMIT, "a pop: {worst:.0} in 100,000");
    let drawn = walk(&store, RemasterShadowCasters::InView);
    let worst_drawn = drawn.iter().copied().map(rate).fold(0.0, f64::max);
    eprintln!(
        "only what is in view: the worst frame {worst_drawn:.0} in 100,000; frames {:?}",
        drawn
    );
    assert!(
        worst_drawn > WALK_CHANGE_LIMIT,
        "the walk shows the pops shadows drawn from the view make: {worst_drawn:.0}"
    );
}

/// In the morning forest, the trees cast: a good share of what the sun lights is in their shade,
/// and the picture under the shadows is darker than without them where it is.
#[test]
fn the_forests_trees_cast_shadows() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut gpu, mut shot, _) = forest(&store, RemasterShadowCasters::Resident, 0.0);
    shot.step_held(&mut gpu);
    let c = capture(&shot, &mut gpu);
    let v = c.sun_visibility.as_ref().expect("the shadows drew");
    let lit: Vec<usize> = (0..v.len()).filter(|i| sunlit(c.class[*i])).collect();
    let shaded = lit.iter().filter(|i| v[**i] < 128).count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = shaded as f64 / lit.len().max(1) as f64;
    eprintln!(
        "{shaded} of {} sunlit pixels in shade ({:.1}%)",
        lit.len(),
        share * 100.0
    );
    assert!(lit.len() > 100_000);
    assert!(share > 0.05, "{share}");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
