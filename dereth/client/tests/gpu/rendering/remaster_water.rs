//! The remaster's water: drawn only where the ground is water, and off the frame drawn without it;
//! what it reflects the same however the camera turns; drawn over a body standing in it, below
//! its surface; the trees and pillars
//! standing in it meeting it, near and far; rings round a body walking in it from the first step;
//! its ripples and its color over a lowered bed; and the puddles of a rainy day as a switch of
//! their own.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterCapture, RemasterLight, RemasterTerrain, RemasterWater};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{report_station, Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

/// The water's kind among the material's kinds.
const WATER: u8 = 7;

/// A lake under hills at dusk, the camera on its bank looking across it.
const LAKE: &str = "dereth report 1; loc 0xA9B50011 [64.387108 22.052589 38.978706] 0.920786 \
                    0.000000 0.000000 -0.390069; camera 0xA9B5 [60.244957 18.04098 42.136875] \
                    yaw -0.8014121 pitch -0.27999994 cell 0xA9B50011; time 0.885; clock \
                    306487158.54054123; weather auto";

/// A shore of the same lake, the camera low by the water.
const SHORE: &str = "dereth report 1; loc 0xA9B5000D [39.936409 110.799110 27.554998] 0.614589 \
                     0.000000 0.000000 -0.788848; camera 0xA9B5 [34.34513 112.209175 30.713133] \
                     yaw -1.8178362 pitch -0.28000048 cell 0xA9B5000D; time 0.781; clock \
                     306486365.4719249; weather auto";

fn device() -> Gpu {
    device_of(W, H)
}

fn device_of(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    gpu
}

/// Where a run is drawn from: a station, or a report's place (its camera turned down by `pitch`
/// radians more).
enum At<'a> {
    Station(Station),
    Report(&'a str, f32),
}

/// The remaster drawn `at` with the water as `water` says, its light per pixel, stepped a few
/// frames held still; the last frame.
fn run(store: &Arc<RetailDatStore>, at: &At<'_>, water: RemasterWater) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = match at {
        At::Station(st) => {
            let mut shot = Shot::open(store, &mut gpu, st);
            shot.poll(&mut gpu);
            shot
        }
        At::Report(line, down) => {
            let mut report = VisualReport::parse(line).expect("the report reads");
            if let Some(c) = report.camera.as_mut() {
                c.pitch -= down;
            }
            let st = report_station(&report, Some(3));
            let mut shot = Shot::open(store, &mut gpu, &st);
            shot.apply_report(&report);
            shot.poll(&mut gpu);
            shot
        }
    };
    shot.scene.draw.set_remaster_water(water, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

fn water() -> RemasterWater {
    RemasterWater {
        on: true,
        ..RemasterWater::default()
    }
}

/// The pixels whose color differs between `a` and `b`, of the water's kind and of any other.
fn changed(a: &RemasterCapture, b: &RemasterCapture) -> (usize, usize) {
    let mut out = (0, 0);
    for i in 0..a.kind.len() {
        if a.color[i * 4..i * 4 + 3] != b.color[i * 4..i * 4 + 3] {
            if a.kind[i] == WATER {
                out.0 += 1;
            } else {
                out.1 += 1;
            }
        }
    }
    out
}

/// **The water is drawn only where the ground is water, and off it is the frame drawn without
/// it**: along the shore, the water changes its own pixels and no other; off, the frame is the one
/// drawn with no water at all.
#[test]
fn the_water_is_drawn_only_where_the_ground_is_water() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let at = At::Report(SHORE, 0.0);
    let none = run(&store, &at, RemasterWater::default());
    let on = run(&store, &at, water());
    let off = run(
        &store,
        &at,
        RemasterWater {
            waves: 0.5,
            ..RemasterWater::default()
        },
    );
    let (water_px, other) = changed(&on, &none);
    let total = on.kind.iter().filter(|k| **k == WATER).count();
    eprintln!("shore: {water_px} of {total} water pixels changed, {other} others");
    assert!(water_px > total / 4, "the water is drawn");
    assert_eq!(other, 0, "nothing else changes");
    assert_eq!(off.color, none.color, "off, the frame without it");
}

/// The mean difference, in levels per channel, between what the water shows at the same points of
/// its surface in `a` and in `b` (drawn from the same eye, turned differently), and how many
/// points both show.
fn same_water(a: &RemasterCapture, b: &RemasterCapture) -> (f64, usize) {
    let inv = glam::Mat4::from_cols_array(&a.view_projection).inverse();
    let to_b = glam::Mat4::from_cols_array(&b.view_projection);
    let (w, h) = (b.width as usize, b.height as usize);
    let (mut sum, mut n) = (0u64, 0usize);
    for i in 0..a.kind.len() {
        if a.kind[i] != WATER {
            continue;
        }
        let p = surface_of(a, &inv, i);
        let c = to_b * p.extend(1.0);
        if c.w <= 0.0 {
            continue;
        }
        let (x, y) = (
            (c.x / c.w + 1.0) * 0.5 * w as f32,
            (1.0 - c.y / c.w) * 0.5 * h as f32,
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (x as usize, y as usize);
        if x < 2 || y < 2 || x + 2 >= w || y + 2 >= h {
            continue;
        }
        let j = y * w + x;
        if b.kind[j] != WATER {
            continue;
        }
        sum += (0..3)
            .map(|k| u64::from(a.color[i * 4 + k].abs_diff(b.color[j * 4 + k])))
            .sum::<u64>();
        n += 1;
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / (3 * n.max(1)) as f64;
    (mean, n)
}

/// **What the water reflects does not change as the camera turns down**: across the lake from
/// its bank, from the camera level and from it turned down until no sky is in the picture, the
/// points of the water both pictures show reflect the same: within a level and a half traced
/// through the scene, and within eight found on screen, where only what the camera sees of the
/// trees and the hills can stand in for them. A reflection is of what stands along the reflected
/// ray, and the sky is the sky all round the eye, not the part of it the camera happens to see.
#[test]
fn turning_the_camera_down_leaves_what_the_water_reflects() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut out = Vec::new();
    for traced in [false, true] {
        let still = RemasterWater {
            waves: 0.0,
            rings: false,
            traced,
            ..water()
        };
        let level = run(&store, &At::Report(LAKE, 0.0), still);
        let down = run(&store, &At::Report(LAKE, 0.45), still);
        let sky = down.class.iter().filter(|c| **c == 0).count();
        let (mean, n) = same_water(&down, &level);
        eprintln!("lake, traced {traced}: turned down ({sky} sky pixels), {n} points of the water, mean difference {mean:.2}");
        out.push((traced, sky, n, mean));
    }
    for (traced, sky, n, mean) in out {
        assert_eq!(sky, 0, "no sky in the picture turned down");
        assert!(n > 1_000, "the pictures share the water: {n}");
        // On screen, what the camera sees of the trees and the land stands in for them only while
        // it sees them; traced, the scene is the same whichever way the camera turns.
        let most = if traced { 1.5 } else { 8.0 };
        assert!(
            mean < most,
            "traced {traced}: the water reflects the same: {mean:.2}"
        );
    }
}

/// **The puddles are a switch of their own, and gather only on a rainy day**: in the town on a
/// rainy day, puddles alone change the flat ground's pixels; on a sunny day they change nothing;
/// and the water alone changes no ground that is not water.
#[test]
fn the_puddles_are_a_switch_of_their_own_and_gather_only_on_a_rainy_day() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = |name: &str| {
        remaster_frames::stations(&store, &[name])
            .pop()
            .unwrap_or_else(|| panic!("the {name} station"))
    };
    let puddles = RemasterWater {
        puddles: true,
        ..RemasterWater::default()
    };
    let rain = At::Station(station("rain"));
    let wet = run(&store, &rain, puddles);
    let dry = run(&store, &rain, RemasterWater::default());
    let water_only = run(&store, &rain, water());
    let sunny = At::Station(station("holtburg"));
    let sun_puddles = run(&store, &sunny, puddles);
    let sun_dry = run(&store, &sunny, RemasterWater::default());
    let (_, ground) = changed(&wet, &dry);
    let (_, by_water) = changed(&water_only, &dry);
    eprintln!(
        "town: {ground} pixels gather puddles on a rainy day; the water alone changes {by_water}"
    );
    assert!(ground > 2_000, "puddles on a rainy day: {ground}");
    assert_eq!(by_water, 0, "the water alone gathers no puddles");
    assert_eq!(
        sun_puddles.color, sun_dry.color,
        "no puddles on a sunny day"
    );
}

/// A pond with lily pads at dusk, the camera over a body standing in it.
const POND: &str = "dereth report 1; loc 0xA8B60012 [55.615215 30.300829 27.554998] 0.069254 \
                    0.000000 0.000000 -0.997599; camera 0xA8B6 [54.51643 35.022827 32.58985] \
                    yaw -2.9129736 pitch -0.6300002 cell 0xA8B60012; time 0.771; clock \
                    306486294.7270536; weather auto";

/// The pond's frames, the ground drawn from its splats (so the water reaches the shore as it
/// blends), the waves still, the rings as `rings` says, the body walking (`walk`) or standing,
/// each step a thirtieth of a second.
fn wading(
    store: &Arc<RetailDatStore>,
    rings: bool,
    walk: bool,
    frames: usize,
) -> Vec<RemasterCapture> {
    let report = VisualReport::parse(POND).expect("the report reads");
    let st = report_station(&report, Some(3));
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, &st);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            rings,
            waves: 0.0,
            ..water()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain {
            splat: true,
            ..Default::default()
        },
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.step_held(&mut gpu);
    shot.input.forward = walk;
    let mut out = Vec::new();
    for _ in 0..frames {
        shot.step_by(&mut gpu, 1.0 / 30.0);
        out.push(
            shot.scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew"),
        );
    }
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    out
}

/// **Rings spread round a body walking in the water from its first step**: in the pond, a body
/// walking forward rings the water in every frame from the first step on (the frames with the
/// rings differ from the same frames without them by thousands of pixels each), and it rings it
/// more walking than standing.
#[test]
fn rings_spread_round_a_body_walking_in_the_water_from_its_first_step() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let frames = 10;
    let calm = wading(&store, false, true, frames);
    let ringed = wading(&store, true, true, frames);
    let standing_calm = wading(&store, false, false, frames);
    let standing = wading(&store, true, false, frames);
    let ringing = |a: &RemasterCapture, b: &RemasterCapture| {
        (0..a.kind.len())
            .filter(|i| {
                a.kind[*i] == WATER && a.color[i * 4..i * 4 + 3] != b.color[i * 4..i * 4 + 3]
            })
            .count()
    };
    let walking: Vec<usize> = calm
        .iter()
        .zip(&ringed)
        .map(|(a, b)| ringing(a, b))
        .collect();
    let still = ringing(&standing_calm[frames - 1], &standing[frames - 1]);
    eprintln!("pond: ringing pixels walking {walking:?}; standing, the last frame {still}");
    assert!(
        walking.iter().all(|n| *n > 5_000),
        "rings from the first step: {walking:?}"
    );
    assert!(walking[frames - 1] > still, "more walking than standing");
}

/// The pond's frames seen from beside the body standing in it, low over the water, the ground as
/// the player draws it (its bed lowered under the water), the waves still and no rings, the water
/// drawn or not as `on` says, each step a thirtieth of a second.
fn wading_seen(store: &Arc<RetailDatStore>, on: bool, frames: usize) -> Vec<RemasterCapture> {
    let mut report = VisualReport::parse(POND).expect("the report reads");
    if let Some(c) = report.camera.as_mut() {
        c.origin.z -= 3.0;
        c.pitch += 0.75;
    }
    let st = report_station(&report, Some(3));
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, &st);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            on,
            rings: false,
            waves: 0.0,
            ..RemasterWater::default()
        },
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster_terrain(RemasterTerrain::player(), &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let mut out = Vec::new();
    for _ in 0..frames {
        shot.step_by(&mut gpu, 1.0 / 30.0);
        out.push(
            shot.scene
                .draw
                .remaster_capture(&mut gpu)
                .expect("the remaster drew"),
        );
    }
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    out
}
/// The characters' kind among the material's kinds.
const CHARACTER: u8 = 2;

/// **The water covers a body standing in it below its surface, and leaves it above**: in the
/// pond, a body standing in the water (retail sinks it most of a metre), seen from beside it,
/// has its pixels below the water's surface drawn as water over it, and its pixels well above
/// the surface as they are drawn with no water at all.
#[test]
fn the_water_covers_a_body_standing_in_it_below_its_surface() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let frames = 6;
    let with = wading_seen(&store, true, frames);
    let without = wading_seen(&store, false, frames);
    let (a, b) = (&with[frames - 1], &without[frames - 1]);
    let inv = glam::Mat4::from_cols_array(&b.view_projection).inverse();
    // The water's level: where the water pixels' surface lies, round the body.
    let mut levels: Vec<f32> = (0..b.kind.len())
        .filter(|i| b.kind[*i] == WATER)
        .map(|i| surface_of(b, &inv, i).z)
        .collect();
    levels.sort_by(f32::total_cmp);
    let level = levels[levels.len() / 2];
    let (mut below, mut covered, mut above, mut changed) = (0usize, 0usize, 0usize, 0usize);
    for i in 0..b.kind.len() {
        if b.kind[i] != CHARACTER {
            continue;
        }
        let z = place_of(b, &inv, i).z;
        let differs = (0..3).any(|k| a.color[i * 4 + k].abs_diff(b.color[i * 4 + k]) > 2);
        if z < level - 0.05 {
            below += 1;
            covered += usize::from(differs);
        } else if z > level + 0.1 {
            above += 1;
            changed += usize::from(differs);
        }
    }
    eprintln!(
        "pond: water at {level:.2}; {covered} of {below} body pixels below it covered, \
         {changed} of {above} above it changed"
    );
    assert!(below > 300, "the body stands in the water: {below}");
    assert!(
        covered * 10 >= below * 9,
        "covered below: {covered} of {below}"
    );
    assert!(above > 300, "the body stands out of the water: {above}");
    assert!(changed * 50 <= above, "left above: {changed} of {above}");
}

/// The shore's frame at `size`, with the camera carried `back` metres back along its view and
/// `up` metres up, the ground drawn as `ground` says, the waves still and no rings, and the water
/// as `check` says (its check view or its own colors).
fn shore_from(
    store: &Arc<RetailDatStore>,
    size: (u32, u32),
    at: (f32, f32),
    ground: RemasterTerrain,
    check: bool,
) -> RemasterCapture {
    shore_with(
        store,
        size,
        at,
        ground,
        RemasterWater {
            waves: 0.0,
            rings: false,
            check,
            ..water()
        },
    )
}

/// [`shore_from`] with the water drawn as `water` says.
fn shore_with(
    store: &Arc<RetailDatStore>,
    size: (u32, u32),
    (back, up): (f32, f32),
    ground: RemasterTerrain,
    water: RemasterWater,
) -> RemasterCapture {
    shore_looking(store, size, (back, up, 0.0), ground, water)
}

/// [`shore_with`] with the camera turned down `down` radians more.
fn shore_looking(
    store: &Arc<RetailDatStore>,
    size: (u32, u32),
    from: (f32, f32, f32),
    ground: RemasterTerrain,
    water: RemasterWater,
) -> RemasterCapture {
    shore_seen(store, size, from, (ground, water, false))
}

/// [`shore_looking`], `bare` with no ripples' picture for the water to lay.
fn shore_seen(
    store: &Arc<RetailDatStore>,
    size: (u32, u32),
    (back, up, down): (f32, f32, f32),
    (ground, water, bare): (RemasterTerrain, RemasterWater, bool),
) -> RemasterCapture {
    let mut report = VisualReport::parse(SHORE).expect("the report reads");
    if let Some(c) = report.camera.as_mut() {
        c.pitch -= down;
        let (s, co) = (
            dereth_primitives::num::math::sinf(c.yaw),
            dereth_primitives::num::math::cosf(c.yaw),
        );
        c.origin.x += s * back;
        c.origin.y -= co * back;
        c.origin.z += up;
        c.cell = None;
    }
    let st = report_station(&report, Some(3));
    let mut gpu = device_of(size.0, size.1);
    let mut shot = Shot::open(store, &mut gpu, &st);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_water(water, &mut gpu);
    shot.scene.draw.set_remaster_terrain(ground, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.scene
        .draw
        .set_remaster_water_without_picture(&mut gpu, bare);
    // The player's ground is drawn once the world's vertices are read, off this thread: two
    // captures compared must not fall either side of that.
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// Where the water's surface is at the water pixel `i` of `c`: where the view's ray meets the
/// water's level, over the bed where the ground draws its bed lower.
fn surface_of(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> glam::Vec3 {
    let bed = place_of(c, inv, i);
    let eye = glam::Vec3::from(c.eye);
    let under = f32::from(c.bed[i]) * 0.04;
    if under > 0.0 && eye.z > bed.z + under {
        eye + (bed - eye) * ((eye.z - bed.z - under) / (eye.z - bed.z))
    } else {
        bed
    }
}

/// Where the pixel `i` of `c` stands, from its depth.
#[allow(clippy::cast_precision_loss)]
fn place_of(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> glam::Vec3 {
    let (w, h) = (c.width as usize, c.height as usize);
    let (x, y) = ((i % w) as f32, (i / w) as f32);
    let ndc = glam::Vec4::new(
        (x + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y + 0.5) / h as f32 * 2.0,
        c.depth[i],
        1.0,
    );
    let p = *inv * ndc;
    (p / p.w).truncate()
}

/// What the water shows where the objects within `reach` metres stand in it.
#[derive(Debug, Default, Clone, Copy)]
struct Bases {
    /// The bases: an object's lowest pixel in its column, with water at most four pixels under
    /// it and nothing of an object between, that stands where the water is (within half a metre
    /// and a hundredth of the distance of it).
    bases: usize,
    /// The pixels between a base and the water under it that are the sky, or nothing drawn
    /// (black).
    gaps: usize,
    /// The two water pixels under each base.
    ring: usize,
    /// Of those, the ones whose reflection found the sky (as the same frame's check view says),
    /// under a solid object and under a plant's cut-out leaves: a sky-colored band between an
    /// object and its reflection.
    sky_solid: usize,
    sky_leaves: usize,
}

/// An object's pixel outdoors, among the light classes.
const OBJECT_LIT: u8 = 2;
/// The plants' kind among the material's kinds.
const FOLIAGE: u8 = 1;

/// The bases of the objects within `reach` metres standing in the water of `c`, `check` being the
/// same frame's check view.
fn bases(c: &RemasterCapture, check: &RemasterCapture, reach: f32) -> Bases {
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let (w, n) = (c.width as usize, c.class.len());
    let object = |i: usize| c.class[i] == OBJECT_LIT && c.kind[i] != WATER;
    let sky = |i: usize| c.class[i] == 0 || c.class[i] == 5 || c.depth[i] >= 1.0;
    let unlit = |i: usize| c.color[i * 4..i * 4 + 3].iter().all(|v| *v <= 2);
    let mut out = Bases::default();
    for i in 0..n.saturating_sub(w) {
        if !object(i) || object(i + w) {
            continue;
        }
        let Some(r) = (1..=4)
            .take_while(|r| i + r * w < n && !object(i + r * w))
            .find(|r| c.kind[i + r * w] == WATER)
        else {
            continue;
        };
        let (base, under) = (place_of(c, &inv, i), surface_of(c, &inv, i + r * w));
        let far = (under - eye).length();
        let span = 0.5 + 0.01 * far;
        if far > reach || base.z - under.z > span || (base - under).truncate().length() > span {
            continue;
        }
        out.bases += 1;
        out.gaps += (1..r)
            .map(|q| i + q * w)
            .filter(|j| sky(*j) || unlit(*j))
            .count();
        for j in (0..2).map(|q| i + (r + q) * w).filter(|j| *j < n) {
            if c.kind[j] != WATER {
                break;
            }
            out.ring += 1;
            if check.color[j * 4..j * 4 + 3] == [0, 0, 255] {
                if c.kind[i] == FOLIAGE {
                    out.sky_leaves += 1;
                } else {
                    out.sky_solid += 1;
                }
            }
        }
    }
    out
}

/// **The trees and pillars standing in the water meet it, near and far**: at the shore, from
/// where the camera stands and from forty metres farther back and four higher, with the ground
/// drawn on retail's grid from its splats and drawn as the player draws it (its bed lowered under
/// the water), the bases of the trees and pillars standing in the water meet it with no sky or
/// black pixel between, and the water just under them reflects them: under a pillar never the
/// sky, under a tree's cut-out leaves the sky at most a quarter of it, through the gaps between
/// the leaves.
#[test]
fn the_trees_and_pillars_standing_in_the_water_meet_it_near_and_far() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (1280, 960);
    let grids = [
        (
            "retail's grid",
            RemasterTerrain {
                splat: true,
                ..RemasterTerrain::default()
            },
        ),
        ("the player's ground", RemasterTerrain::player()),
    ];
    for ((name, at), (grid, ground)) in [("near", (0.0, 0.0)), ("far", (40.0, 4.0))]
        .into_iter()
        .flat_map(|v| grids.map(|g| (v, g)))
    {
        let drawn = shore_from(&store, size, at, ground, false);
        let check = shore_from(&store, size, at, ground, true);
        let b = bases(&drawn, &check, 150.0);
        let name = format!("{name}, {grid}");
        eprintln!("shore, {name}: {b:?}");
        assert!(b.bases >= 6, "{name}: the bases are in the picture: {b:?}");
        assert_eq!(
            b.gaps, 0,
            "{name}: nothing between an object and the water: {b:?}"
        );
        assert_eq!(b.sky_solid, 0, "{name}: no sky under a pillar: {b:?}");
        assert!(
            b.sky_leaves * 4 <= b.ring,
            "{name}: little sky under the leaves: {b:?}"
        );
    }
}

/// **The water ripples near, lies calm far, and over a lowered bed takes retail's water color
/// under the bed's light**: looking down on the lake with the ground as the player draws it
/// (its bed lowered under the water, where it shows retail's own water picture), the deep water is
/// what that bed shows without the water's absorption, within a sixth or so in each channel, not a
/// darker color of its own, and the water pixels with no bed under them are drawn the same; and
/// from the shore, a quarter or more of the water within forty meters moves with the ripples, and
/// next to none of it past 125.
///
/// Behaviour: remaster.water.remaster-ripples-and-retails-color
#[test]
fn the_water_ripples_near_lies_calm_far_and_takes_retails_color_over_its_bed() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let still = RemasterWater {
        waves: 0.0,
        rings: false,
        ..water()
    };
    let ground = RemasterTerrain::player();
    let above = (0.0, 30.0, 0.7);
    let with = shore_looking(&store, (W, H), above, ground, still);
    let without = shore_looking(
        &store,
        (W, H),
        above,
        ground,
        RemasterWater {
            absorption: 0.0,
            ..still
        },
    );
    let (mut deep, mut level, mut same) = (0usize, 0usize, 0usize);
    let (mut sum_with, mut sum_without) = ([0u64; 3], [0u64; 3]);
    for i in 0..with.kind.len() {
        if with.kind[i] != WATER {
            continue;
        }
        if with.bed[i] >= 25 {
            deep += 1;
            for k in 0..3 {
                sum_with[k] += u64::from(with.color[i * 4 + k]);
                sum_without[k] += u64::from(without.color[i * 4 + k]);
            }
        } else if with.bed[i] == 0 {
            level += 1;
            if with.color[i * 4..i * 4 + 3] == without.color[i * 4..i * 4 + 3] {
                same += 1;
            }
        }
    }
    eprintln!(
        "lake from above: {deep} deep pixels, mean {sum_with:?} against {sum_without:?}; \
         {same} of {level} over no bed the same"
    );
    assert!(deep > 1000, "the bed is lowered: {deep}");
    assert_eq!(same, level, "no bed, no change");
    for k in 0..3 {
        let (a, b) = (sum_with[k] / deep as u64, sum_without[k] / deep as u64);
        assert!(
            a.abs_diff(b) <= b / 6 + 3,
            "channel {k}: the deep water {a}, retail's water under the bed's light {b}"
        );
    }

    // The ripples: near water moves with them, and water far enough off lies calm; and they are
    // the picture's: with no picture to lay, the water lies as it does with no ripples at all.
    let view = (0.0, 0.0, 0.0);
    let calm = shore_looking(&store, (W, H), view, ground, still);
    let rippled = shore_looking(
        &store,
        (W, H),
        view,
        ground,
        RemasterWater {
            rings: false,
            ..water()
        },
    );
    let inv = glam::Mat4::from_cols_array(&calm.view_projection).inverse();
    let eye = glam::Vec3::from(calm.eye);
    let (mut near, mut moved, mut far, mut far_moved) = (0usize, 0usize, 0usize, 0usize);
    for i in 0..calm.kind.len() {
        if calm.kind[i] != WATER {
            continue;
        }
        let off = (surface_of(&calm, &inv, i) - eye).length();
        let differs = (0..3).any(|k| calm.color[i * 4 + k].abs_diff(rippled.color[i * 4 + k]) > 2);
        if off < 40.0 {
            near += 1;
            moved += usize::from(differs);
        } else if off > 125.0 {
            far += 1;
            far_moved += usize::from(differs);
        }
    }
    eprintln!("the ripples: {moved} of {near} near pixels moved, {far_moved} of {far} far ones");
    assert!(near > 500, "the water runs near: {near}");
    assert!(
        moved * 4 >= near,
        "the near water ripples: {moved} of {near}"
    );
    assert!(
        far_moved * 100 <= far,
        "the far water lies calm: {far_moved} of {far}"
    );
    let bare = shore_seen(
        &store,
        (W, H),
        view,
        (
            ground,
            RemasterWater {
                rings: false,
                ..water()
            },
            true,
        ),
    );
    let (mut water_pixels, mut bare_moved) = (0usize, 0usize);
    for i in 0..calm.kind.len() {
        if calm.kind[i] == WATER {
            water_pixels += 1;
            bare_moved += usize::from(
                (0..3).any(|k| calm.color[i * 4 + k].abs_diff(bare.color[i * 4 + k]) > 2),
            );
        }
    }
    eprintln!("with no picture: {bare_moved} of {water_pixels} moved");
    assert!(
        bare_moved * 100 <= water_pixels,
        "the ripples are the picture's: {bare_moved} of {water_pixels} moved without it"
    );
}

/// **A film of water over the bank shows the wet ground**: on a shore with the bed lowered, where
/// the ground is water but the bed lies at the water's level the still water changes nothing of
/// what is drawn: no pale rim of sky along the shore.
#[test]
fn a_film_of_water_over_the_bank_shows_the_wet_ground() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let ground = RemasterTerrain::player();
    let still = RemasterWater {
        waves: 0.0,
        rings: false,
        ..water()
    };
    let from = (0.0, 8.0, 0.2);
    let with = shore_looking(&store, (W, H), from, ground, still);
    let without = shore_looking(
        &store,
        (W, H),
        from,
        ground,
        RemasterWater { on: false, ..still },
    );
    let (mut film, mut differs) = (0usize, 0usize);
    for i in 0..with.kind.len() {
        if with.kind[i] == WATER && with.bed[i] == 0 {
            film += 1;
            differs += usize::from(
                (0..3).any(|k| with.color[i * 4 + k].abs_diff(without.color[i * 4 + k]) > 3),
            );
        }
    }
    eprintln!("the film: {differs} of {film} pixels differ");
    assert!(film > 100, "the shore's film is in view: {film}");
    assert!(
        differs * 50 <= film,
        "the film shows the ground: {differs} of {film} differ"
    );
}
