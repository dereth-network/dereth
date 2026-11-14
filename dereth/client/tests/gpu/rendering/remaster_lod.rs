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
/// and the sweep changes no more pixels in all. The trees' impostors are left out: moving the bar
/// also moves a tree's hand-overs across the room the window leaves before its impostor, where
/// a level is passed over by design, which no step of a walk does.
#[test]
fn parts_fading_through_their_hand_overs_change_few_pixels_at_a_time() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let sweep = |fade: f32| -> (usize, usize, usize) {
        let lod = RemasterLod {
            fade,
            impostors: false,
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

/// Every tree drawn as its impostor, wherever it stands.
const EVERY_IMPOSTOR: RemasterLod = RemasterLod {
    error_px: 1.0e9,
    ..RemasterLod::ON
};

/// No tree drawn as its impostor.
const NO_IMPOSTOR: RemasterLod = RemasterLod {
    impostors: false,
    ..RemasterLod::ON
};

/// The frames of the smoothing's sequence.
const SEQUENCE: usize = 8;

/// `shot` with the smoothing on, laying a sequence of frames over each other.
fn smoothed(shot: &mut Shot, gpu: &mut Gpu) {
    let post = dereth_scene::world_scene::RemasterPost {
        taa: true,
        ..shot.scene.draw.remaster_post()
    };
    shot.scene.draw.set_remaster_post(post, gpu);
}

/// The G-buffer's pixels that differ between two frames: their class or their color before light.
fn drawn_apart(a: &RemasterCapture, b: &RemasterCapture) -> usize {
    a.class
        .iter()
        .zip(&b.class)
        .zip(a.albedo.iter().zip(&b.albedo))
        .filter(|((ca, cb), (x, y))| ca != cb || x != y)
        .count()
}

/// The share of the pixels of `c` lit by the sun (what stands outdoors) whose shown brightness
/// stands out from their four neighbors' by more than an eighth: a dot pattern's mark.
fn speckle(c: &RemasterCapture) -> f64 {
    let w = c.width as usize;
    let luma = |i: usize| {
        let p = &c.color[i * 4..i * 4 + 3];
        0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
    };
    let (mut lit, mut out) = (0usize, 0usize);
    for i in w + 1..c.class.len().saturating_sub(w + 1) {
        if c.class[i] != 2 || i % w == 0 || i % w == w - 1 {
            continue;
        }
        lit += 1;
        let around = (luma(i - 1) + luma(i + 1) + luma(i - w) + luma(i + w)) * 0.25;
        out += usize::from((luma(i) - around).abs() > 32.0);
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = out as f64 / lit.max(1) as f64;
    share
}

/// In the forest with the smoothing on, every tree drawn as its impostor shows little more speckle
/// than the same trees drawn as their meshes: the screen patterns that pick a card's view and cut
/// its coverage turn a step each frame, so the smoothing blends the views and the coverage rather
/// than keeping a fixed dot pattern (held still, the impostors showed 2.8 times the meshes'
/// speckle). A sequence apart the cards draw the same pixels, so a still view settles. (The sky's
/// clouds drift with every frame drawn, so the picture shown is not compared.)
#[test]
fn an_impostor_shows_no_dot_pattern_once_the_smoothing_lays_its_frames_over_each_other() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut measured = Vec::new();
    for (label, lod) in [("meshes", NO_IMPOSTOR), ("impostors", EVERY_IMPOSTOR)] {
        let (mut gpu, mut shot) = open(&store, "forest", lod, (1920, 1080));
        smoothed(&mut shot, &mut gpu);
        for _ in 0..SEQUENCE * 3 {
            shot.step_held(&mut gpu);
        }
        let cards = shot
            .scene
            .draw
            .remaster_impostor_stats(&mut gpu)
            .expect("the remaster is on")
            .cards;
        let frames: Vec<RemasterCapture> = (0..=SEQUENCE)
            .map(|_| {
                shot.step_held(&mut gpu);
                capture(&shot, &mut gpu)
            })
            .collect();
        close(&store, "forest", gpu, shot);
        let drawn = drawn_apart(&frames[0], &frames[SEQUENCE]);
        let shown = changed(&frames[0], &frames[SEQUENCE]);
        let next = changed(&frames[0], &frames[1]);
        let dots = speckle(&frames[SEQUENCE]);
        eprintln!(
            "forest, {label}: {cards} cards; a sequence apart the G-buffer drew {drawn} pixels \
             apart and the picture shown changed {shown} ({next} from one frame to the next); \
             speckle {dots:.4}"
        );
        measured.push((cards, drawn, dots));
    }
    let (mesh, impostor) = (measured[0], measured[1]);
    assert!(impostor.0 > 0, "no tree drawn as its impostor");
    assert_eq!(
        impostor.1, 0,
        "a sequence apart, the cards drew other pixels"
    );
    assert!(
        impostor.2 <= mesh.2 * 2.2,
        "the impostors' speckle {:.4} against the meshes' {:.4}",
        impostor.2,
        mesh.2
    );
}

/// Each pixel's share of `frames` that draws something lit by the sun (what stands outdoors).
fn standing(frames: &[RemasterCapture]) -> Vec<f32> {
    let mut share = vec![0.0f32; frames.first().map_or(0, |f| f.class.len())];
    for f in frames {
        for (s, c) in share.iter_mut().zip(&f.class) {
            *s += f32::from(u8::from(*c == 2));
        }
    }
    #[allow(clippy::cast_precision_loss)] // a few frames
    let n = frames.len().max(1) as f32;
    share.iter_mut().for_each(|s| *s /= n);
    share
}

/// In the forest with the smoothing on, every tree drawn as its impostor covers, over a sequence
/// of frames, the pixels its mesh covers: of the pixels the meshes cover in most of the frames,
/// the impostors leave few open in most of them, rather than thinning their crowns to a few blots
/// with holes between (with pictures drawn at their own size and cut at half, 0.84% were left
/// open).
#[test]
fn a_trees_impostor_covers_its_crown_where_its_mesh_does() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut shares = Vec::new();
    for lod in [NO_IMPOSTOR, EVERY_IMPOSTOR] {
        let (mut gpu, mut shot) = open(&store, "forest", lod, (1920, 1080));
        smoothed(&mut shot, &mut gpu);
        for _ in 0..SEQUENCE * 2 {
            shot.step_held(&mut gpu);
        }
        let frames: Vec<RemasterCapture> = (0..SEQUENCE)
            .map(|_| {
                shot.step_held(&mut gpu);
                capture(&shot, &mut gpu)
            })
            .collect();
        close(&store, "forest", gpu, shot);
        shares.push(standing(&frames));
    }
    let (mesh, impostor) = (&shares[0], &shares[1]);
    let covered = mesh.iter().filter(|&&m| m > 0.5).count();
    let open = mesh
        .iter()
        .zip(impostor)
        .filter(|(m, i)| **m > 0.5 && **i < 0.5)
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = open as f64 / covered.max(1) as f64;
    eprintln!(
        "forest: the meshes cover {covered} pixels; the impostors leave {open} open ({share:.4})"
    );
    assert!(covered > 0, "the meshes cover nothing");
    assert!(
        share < 0.0075,
        "the impostors leave {share:.4} of what the meshes cover open"
    );
}

/// The vista with its scenery two rings of blocks out, the remaster's boxes ticked through the
/// preferences poll as the player ticks them, the sun's shadows drawn and its levels chosen as
/// the Detail box (`detail`) says; drawn until it settles, the clock held.
fn vista_two_rings_out(store: &Arc<RetailDatStore>, detail: bool) -> RemasterCapture {
    let station = remaster_frames::stations(store, &["vista"])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: 960,
        height: 540,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open_with(store, &mut gpu, &station, false, Some(2));
    shot.poll(&mut gpu);
    let boxes = format!(
        "Remaster=1,RemasterShadows=1,RemasterScenery=2,RemasterDetail={}",
        u8::from(detail)
    );
    shot.scene.draw.cfg.render.fidelity =
        dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch(&boxes)
            .expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let c = capture(&shot, &mut gpu);
    close(store, "vista", gpu, shot);
    c
}

/// How many rings of blocks round the viewer's the world drawn at each of `c`'s pixels stands
/// in; `None` where only the sky is drawn. The frame's space has its origin at the south-west
/// corner of the viewer's block.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // pixels and blocks
fn rings_out(c: &RemasterCapture) -> Vec<Option<i32>> {
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let (w, h) = (c.width as usize, c.height as usize);
    (0..c.class.len())
        .map(|i| {
            if matches!(c.class[i], 0 | 5) {
                return None;
            }
            let (x, y) = (i % w, i / w);
            let p = inv.project_point3(glam::Vec3::new(
                (x as f32 + 0.5) / w as f32 * 2.0 - 1.0,
                1.0 - (y as f32 + 0.5) / h as f32 * 2.0,
                c.depth[i],
            ));
            let block = |v: f32| (v / 192.0).floor() as i32;
            Some(block(p.x).abs().max(block(p.y).abs()))
        })
        .collect()
}

/// **Past the blocks retail stands objects on, the Detail box changes nothing drawn**: over the
/// vista with the scenery two rings of blocks out, the world drawn in the blocks two rings out
/// (in both frames, and away from the edges of what stands nearer) is the same with the box
/// unticked as with it ticked, pixel for pixel and in the sun that reaches each: the far trees
/// drawn as the remaster chooses their levels, their impostors, and their shadows over them.
/// Retail would draw them as nothing there, while every tree still casts its shadow.
#[test]
fn past_retails_ring_the_detail_box_changes_nothing_drawn() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let off = vista_two_rings_out(&store, false);
    let on = vista_two_rings_out(&store, true);
    let (rings_off, rings_on) = (rings_out(&off), rings_out(&on));
    // The pixels two rings out in both frames, but for those beside a nearer one in either: the
    // edges of what stands nearer, drawn at another level with the box unticked, blend over them.
    let near = |i: usize| {
        [&rings_off, &rings_on]
            .iter()
            .any(|r| r[i].is_some_and(|n| n < 2))
    };
    let w = off.width as usize;
    let beside = |i: usize| {
        let (x, y) = (i % w, i / w);
        (y.saturating_sub(1)..=y + 1)
            .flat_map(|v| (x.saturating_sub(1)..=(x + 1).min(w - 1)).map(move |u| v * w + u))
            .any(|j| j < off.class.len() && near(j))
    };
    let far: Vec<usize> = (0..off.class.len())
        .filter(|i| rings_off[*i] == Some(2) && rings_on[*i] == Some(2) && !beside(*i))
        .collect();
    let objects = |c: &RemasterCapture| far.iter().filter(|i| c.class[**i] == 2).count();
    let color = far
        .iter()
        .filter(|i| off.color[**i * 4..**i * 4 + 4] != on.color[**i * 4..**i * 4 + 4])
        .count();
    let sun = |c: &RemasterCapture| {
        let s = c
            .sun_visibility
            .as_ref()
            .expect("the sun's shadows are drawn");
        far.iter().map(|i| s[*i]).collect::<Vec<_>>()
    };
    let shade = sun(&off)
        .iter()
        .zip(sun(&on))
        .filter(|(a, b)| **a != *b)
        .count();
    eprintln!(
        "vista: {} pixels two rings out, {} of them objects with the box ticked and {} unticked; \
         {color} differ in color, {shade} in the sun",
        far.len(),
        objects(&on),
        objects(&off)
    );
    assert!(objects(&on) > 1_000, "the far trees are in view");
    assert_eq!(objects(&off), objects(&on), "the far trees are drawn");
    assert_eq!((color, shade), (0, 0), "the same world two rings out");
}
