//! Behaviour: none (experimental)
//!
//! The remaster's levels of detail, chosen by their error on the screen and faded from one level
//! into the next.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterCapture, RemasterLod, RemasterLodStats};

use crate::instruments::remaster_frames;
use crate::instruments::stations::Shot;

/// A station opened on a hardware Vulkan device of `size`, the remaster on with the light per
/// pixel and its levels chosen as `lod` says, stepped six frames with the clock held.
fn open(
    store: &Arc<RetailDatStore>,
    name: &str,
    lod: RemasterLod,
    size: (u32, u32),
) -> (Gpu, Shot) {
    let station = remaster_frames::stations(store, &[name])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_lod(lod, &mut gpu);
    for _ in 0..6 {
        shot.step_held(&mut gpu);
    }
    (gpu, shot)
}

/// Close a station's device, checking the validation layer saw nothing.
fn close(store: &Arc<RetailDatStore>, name: &str, mut gpu: Gpu, mut shot: Shot) {
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{name}: the validation layer: {errors:#?}"
    );
}

fn stats(shot: &Shot, gpu: &mut Gpu) -> RemasterLodStats {
    shot.scene
        .draw
        .remaster_lod_stats(gpu)
        .expect("the remaster is on")
}

/// Outdoors, with levels chosen by their error on the screen, parts draw finer levels than
/// retail chose for them and none a coarser one, some fade between two, and no pipeline is
/// built after start-up.
#[test]
fn outdoors_parts_draw_finer_levels_than_retail_and_some_fade() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["holtburg", "forest", "rain-forest", "snow-village"] {
        let (mut gpu, mut shot) = open(&store, name, RemasterLod::ON, (1920, 1080));
        let built = shot
            .scene
            .draw
            .remaster_counts(&mut gpu)
            .map_or(0, |c| c.pipelines_built);
        shot.step_held(&mut gpu);
        let s = stats(&shot, &mut gpu);
        let after = shot
            .scene
            .draw
            .remaster_counts(&mut gpu)
            .map_or(0, |c| c.pipelines_built);
        eprintln!("{name}: {s:?}");
        close(&store, name, gpu, shot);
        assert_eq!(after, built, "{name}: a pipeline was built after start-up");
        assert!(s.parts > 0, "{name}: no part chosen for");
        assert!(s.finer > 0, "{name}: no part finer than retail");
        assert!(s.fading > 0, "{name}: no part fading");
        assert_eq!(s.coarser, 0, "{name}: a part coarser than retail");
    }
}

/// The remaster's last frame.
fn capture(shot: &Shot, gpu: &mut Gpu) -> RemasterCapture {
    shot.scene
        .draw
        .remaster_capture(gpu)
        .expect("the remaster drew")
}

/// The world's pixels that differ between two of the remaster's frames: the sky's clouds drift
/// with every frame drawn, as retail's do, and are left out.
fn changed(a: &RemasterCapture, b: &RemasterCapture) -> usize {
    a.color
        .chunks(4)
        .zip(b.color.chunks(4))
        .zip(a.class.iter().zip(&b.class))
        .filter(|((x, y), (ca, cb))| (**ca != 0 || **cb != 0) && x != y)
        .count()
}

/// In the forest, with levels chosen by their error on the screen and some parts fading, a still
/// view draws the same world frame after frame.
#[test]
fn a_still_view_with_parts_fading_draws_the_same_pixels_frame_after_frame() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut gpu, mut shot) = open(&store, "forest", RemasterLod::ON, (1920, 1080));
    let s = stats(&shot, &mut gpu);
    let first = capture(&shot, &mut gpu);
    let mut worst = 0;
    for _ in 0..8 {
        shot.step_held(&mut gpu);
        let now = capture(&shot, &mut gpu);
        worst = worst.max(changed(&first, &now));
    }
    eprintln!("forest: {s:?}; the most pixels changed from the first frame: {worst}");
    close(&store, "forest", gpu, shot);
    assert!(s.fading > 0, "no part fading");
    assert_eq!(worst, 0, "a still view changed");
}

/// The largest patch of touching pixels one step of the sweep changes: a part handing over at
/// once changes its whole outline in one step; fading, a step changes pixels scattered through
/// the pattern.
fn largest_patch(a: &RemasterCapture, b: &RemasterCapture) -> usize {
    let (w, h) = (a.width as usize, a.height as usize);
    let changed: Vec<bool> = a
        .color
        .chunks(4)
        .zip(b.color.chunks(4))
        .zip(a.class.iter().zip(&b.class))
        .map(|((x, y), (ca, cb))| (*ca != 0 || *cb != 0) && x != y)
        .collect();
    let mut seen = vec![false; changed.len()];
    let mut largest = 0;
    for start in 0..changed.len() {
        if !changed[start] || seen[start] {
            continue;
        }
        let (mut size, mut stack) = (0, vec![start]);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % w, i / w);
            let mut near = |j: usize| {
                if changed[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                near(i - 1);
            }
            if x + 1 < w {
                near(i + 1);
            }
            if y > 0 {
                near(i - w);
            }
            if y + 1 < h {
                near(i + w);
            }
        }
        largest = largest.max(size);
    }
    largest
}

/// The largest patch of touching pixels a step of the sweep may change with the parts fading:
/// measured at the forest at 1080p at 3, against 2,529 handing over at once.
const FADED_PATCH: usize = 8;

/// In the forest, with the view still, moving the error bar from 10 to 13 pixels in steps of a
/// tenth moves the parts through their hand-overs. Handing over at once, a step changes a part's
/// whole outline together; fading, no step changes a patch of more than a few touching pixels,
/// and the sweep changes no more pixels in all.
#[test]
fn parts_fading_through_their_hand_overs_change_few_pixels_at_a_time() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let sweep = |fade: f32| -> (usize, usize, usize) {
        let lod = RemasterLod {
            fade,
            ..RemasterLod::ON
        };
        let (mut gpu, mut shot) = open(&store, "forest", lod, (1920, 1080));
        let mut last = capture(&shot, &mut gpu);
        let (mut most, mut all, mut patch) = (0, 0, 0);
        for k in 1..=30u8 {
            let bar = RemasterLod {
                error_px: 10.0 + f32::from(k) * 0.1,
                ..lod
            };
            shot.scene.draw.set_remaster_lod(bar, &mut gpu);
            shot.step_held(&mut gpu);
            let now = capture(&shot, &mut gpu);
            let c = changed(&last, &now);
            most = most.max(c);
            all += c;
            patch = patch.max(largest_patch(&last, &now));
            last = now;
        }
        close(&store, "forest", gpu, shot);
        (most, all, patch)
    };
    let (faded_most, faded_all, faded_patch) = sweep(RemasterLod::ON.fade);
    let (at_once_most, at_once_all, at_once_patch) = sweep(0.0);
    eprintln!(
        "forest: fading, the most a step changed {faded_most}, all {faded_all}, the largest patch \
         {faded_patch}; at once, {at_once_most}, all {at_once_all}, the largest patch \
         {at_once_patch}"
    );
    assert!(at_once_most > 0, "no part handed over");
    assert!(
        faded_all <= at_once_all + at_once_all / 10,
        "fading, the sweep changed {faded_all} pixels; at once, {at_once_all}"
    );
    assert!(
        faded_patch <= FADED_PATCH,
        "fading, a step changed a patch of {faded_patch} pixels"
    );
    assert!(
        at_once_patch > 4 * faded_patch,
        "at once, the largest patch {at_once_patch}; fading, {faded_patch}"
    );
}

/// Where a picture of each run is written, when it is set: a folder outside the tree.
fn pictures() -> Option<std::path::PathBuf> {
    std::env::var_os("DERETH_LOD_PICTURES").map(std::path::PathBuf::from)
}

/// Looking out over the land from the heights by Holtburg, with levels chosen by their error on
/// the screen, far trees are drawn as impostors: their pictures made once, a card drawn for each.
#[test]
fn far_trees_are_drawn_as_impostors() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["vista", "holtburg", "forest"] {
        for (label, lod) in [("off", RemasterLod::OFF), ("on", RemasterLod::ON)] {
            let (mut gpu, mut shot) = open(&store, name, lod, (1920, 1080));
            shot.step_held(&mut gpu);
            let imp = shot
                .scene
                .draw
                .remaster_impostor_stats(&mut gpu)
                .expect("the remaster is on");
            let s = stats(&shot, &mut gpu);
            eprintln!("{name} {label}: {s:?} {imp:?}");
            if let Some(dir) = pictures() {
                let rgba = crate::instruments::stations::capture(&mut gpu);
                let (w, h) = gpu.size();
                crate::instruments::stations::write_png(
                    &dir.join(format!("{name}-{label}.png")),
                    w,
                    h,
                    &rgba,
                );
            }
            close(&store, name, gpu, shot);
            if label == "on" && name == "vista" {
                assert!(imp.trees > 0, "{name}: no tree's pictures made");
                assert!(imp.cards > 0, "{name}: no card drawn");
            }
        }
    }
}

/// With levels off, as retail chooses them, the parts whose level turns toward the viewer are
/// drawn turned, as retail draws them: looking at the standing stones by Holtburg from 150 m,
/// where they draw their turned level, the remaster with retail's light per vertex covers
/// exactly the pixels Retail Look covers (drawn unturned, 8 pixels differed).
#[test]
fn retails_turning_cards_are_drawn_turned_with_levels_off() {
    use crate::instruments::stations::{Clock, Place, Station};
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = Station {
        name: "standing stones",
        why: "the ring of standing stones by Holtburg from 150 m, drawn at their turned level",
        place: Place::Look {
            block: 0xA9B3,
            x: 11.6,
            y: 5.0,
            eye: 6.0,
            at: [11.6, 157.0, 96.0],
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    };
    let size = remaster_frames::size();
    let retail = remaster_frames::run(&store, &station, remaster_frames::Draw::Retail, size);
    let frame = remaster_frames::run(&store, &station, remaster_frames::Draw::Vertex, size);
    let n = remaster_frames::compare(&retail, &frame);
    eprintln!(
        "standing stones: mean {:.4}, over 8 {:.3}%, silhouettes {:.4}%, {} lit",
        n.mean, n.over8, n.silhouette, n.lit
    );
    let (mut gpu, shot) = open(&store, "holtburg", RemasterLod::OFF, (960, 540));
    let s = stats(&shot, &mut gpu);
    close(&store, "holtburg", gpu, shot);
    assert!(s.turned > 0, "no part turned at Holtburg");
    assert_eq!(
        n.silhouette, 0.0,
        "the world's outline against Retail Look's"
    );
}

/// In the forest with the sun's shadows on and the view still, moving the error bar from 10 to
/// 60 pixels in steps of five passes trees from their meshes into their impostors. Wherever the
/// ground shows before and after a step, the sun reaching it does not change: trees cast their
/// shadows from their finest meshes whatever is drawn, so a tree's shadow does not change as the
/// tree becomes an impostor. (On the trees themselves the surface drawn changes, and with it the
/// sun on it; far off, two of a tree's surfaces can share one depth, so only the ground is
/// counted.)
#[test]
fn a_trees_shadow_does_not_change_as_it_becomes_an_impostor() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut gpu, mut shot) = open(&store, "forest", RemasterLod::ON, (1920, 1080));
    shot.scene.draw.set_remaster_shadows(
        dereth_scene::world_scene::RemasterShadows {
            on: true,
            ..dereth_scene::world_scene::RemasterShadows::default()
        },
        &mut gpu,
    );
    for _ in 0..3 {
        shot.step_held(&mut gpu);
    }
    let cards = |shot: &Shot, gpu: &mut Gpu| {
        shot.scene
            .draw
            .remaster_impostor_stats(gpu)
            .expect("the remaster is on")
            .cards
    };
    let first_cards = cards(&shot, &mut gpu);
    let mut last = capture(&shot, &mut gpu);
    let (mut ground, mut ground_changed, mut shadowed) = (0usize, 0usize, 0usize);
    for k in 1..=10u8 {
        let bar = RemasterLod {
            error_px: 10.0 + f32::from(k) * 5.0,
            ..RemasterLod::ON
        };
        shot.scene.draw.set_remaster_lod(bar, &mut gpu);
        shot.step_held(&mut gpu);
        let now = capture(&shot, &mut gpu);
        let (Some(a), Some(b)) = (&last.sun_visibility, &now.sun_visibility) else {
            panic!("the shadows drew no sun's share");
        };
        for i in 0..now.class.len() {
            // The ground, before and after.
            if now.class[i] != 1 || last.class[i] != 1 {
                continue;
            }
            ground += 1;
            shadowed += usize::from(b[i] < 255);
            ground_changed += usize::from(a[i] != b[i]);
        }
        last = now;
    }
    let last_cards = cards(&shot, &mut gpu);
    close(&store, "forest", gpu, shot);
    eprintln!(
        "forest: cards {first_cards} to {last_cards}; over the steps {ground} ground pixels, \
         {shadowed} of them shadowed, the sun changed on {ground_changed}"
    );
    assert!(last_cards > first_cards, "no tree became an impostor");
    assert!(shadowed > 0, "no shadow on the ground");
    assert_eq!(ground_changed, 0, "a shadow on the ground changed");
}
