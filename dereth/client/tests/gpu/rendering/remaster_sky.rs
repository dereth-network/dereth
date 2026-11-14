//! The remaster's sky and fog. With the atmosphere off, the sky is retail's own, as Retail Look
//! draws it. With it on, the physical sky is drawn over the authored dome: its mean color through
//! the day stays within a measured band of Retail Look's, closer in the high sky than near the
//! horizon, where the low sun's glow is its own, and dusk keeps its purple. The fog drawn from its
//! volume is retail's range fog without its glow; with the glow it only brightens. God Rays only
//! brighten it too, the air toward a low sun most, high more than low. The far land holds still
//! while the camera turns by a hair, as a settling camera does from one frame to the next. The
//! sun's shadows darken the low fog in front of the far land without hard edges.
//!
//! The sky's pixels are those neither Retail Look nor the remaster drew the world on; the high
//! sky is those in the top two fifths of the frame.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_scene::world_scene::RemasterSky;

use crate::instruments::remaster_frames::{self, Draw, Frame};
use crate::instruments::stations::{Clock, Station};

const SIZE: (u32, u32) = (480, 270);

/// The largest a channel of the sky's mean color may be off Retail Look's at any hour, over the
/// whole sky and over the high sky. Measured: 22.6 and 13.3 at dawn, the most of any hour.
const SKY_BAND: f64 = 25.0;
const HIGH_SKY_BAND: f64 = 15.0;

/// How much of Retail Look's purple the high sky keeps at dusk, red and blue over green each.
/// Measured: 0.99 and 0.82.
const DUSK_PURPLE_KEPT: f64 = 0.75;

/// The town station at `clock`.
fn town(store: &RetailDatStore, clock: Clock) -> Station {
    let mut s = remaster_frames::stations(store, &["holtburg"])
        .pop()
        .expect("the town");
    s.clock = clock;
    s
}

/// The sky's pixels of `retail` and `r`: where neither drew the world.
fn sky_pixels(retail: &Frame, r: &Frame) -> Vec<usize> {
    (0..retail.depth.len())
        .filter(|&i| retail.depth[i] >= 1.0 && r.remaster_depth[i] >= 1.0 && r.class[i] == 0)
        .collect()
}

/// The mean color of `f` over `pixels`, in display levels.
#[allow(clippy::cast_precision_loss)] // pixel counts and levels
fn mean(f: &Frame, pixels: &[usize]) -> [f64; 3] {
    let mut sum = [0.0; 3];
    for &i in pixels {
        for (c, s) in sum.iter_mut().enumerate() {
            *s += f64::from(f.rgba[i * 4 + c]);
        }
    }
    sum.map(|s| s / pixels.len().max(1) as f64)
}

/// The largest channel difference of two mean colors.
fn off(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|c| (a[c] - b[c]).abs()).fold(0.0, f64::max)
}

/// The mean of the largest channel difference between `a` and `b` over `pixels`.
#[allow(clippy::cast_precision_loss)] // pixel counts and levels
fn mean_error(a: &Frame, b: &Frame, pixels: &[usize]) -> f64 {
    let sum: f64 = pixels
        .iter()
        .map(|&i| {
            (0..3)
                .map(|c| f64::from(a.rgba[i * 4 + c].abs_diff(b.rgba[i * 4 + c])))
                .fold(0.0, f64::max)
        })
        .sum();
    sum / pixels.len().max(1) as f64
}

/// The pixels `f` drew the world on.
fn world(f: &Frame) -> Vec<usize> {
    (0..f.class.len()).filter(|&i| f.class[i] != 0).collect()
}

/// The town on a rainy day, per pixel, with the fog drawn from its volume glowing as `glow` says.
fn fogged(store: &Arc<RetailDatStore>, glow: f32) -> Frame {
    remaster_frames::run_with(
        store,
        &town(store, Clock::Rainy(0.5)),
        Draw::Pixel,
        SIZE,
        Some(RemasterSky {
            atmosphere: false,
            fog: true,
            glow,
            far_keep: 0.0,
            air: 0.0,
        }),
    )
}

#[test]
fn with_the_atmosphere_off_the_sky_is_retail_looks() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for (name, clock) in [
        ("noon", Clock::Sunny(0.5)),
        ("dusk", Clock::SunUp(5.0)),
        ("rain", Clock::Rainy(0.5)),
    ] {
        let station = town(&store, clock);
        let retail = remaster_frames::run(&store, &station, Draw::Retail, SIZE);
        let r = remaster_frames::run(&store, &station, Draw::Vertex, SIZE);
        let sky = sky_pixels(&retail, &r);
        let error = mean_error(&retail, &r, &sky);
        println!(
            "{name}: {} sky pixels, retail {:.1?}, remaster {:.1?}, mean error {error:.2}",
            sky.len(),
            mean(&retail, &sky),
            mean(&r, &sky)
        );
        assert!(sky.len() > 10_000, "{name}: the sky is in view");
        assert!(
            error < 1.0,
            "{name}: the sky is Retail Look's, {error:.2} levels off"
        );
    }
}

#[test]
fn the_physical_sky_stays_within_its_band_of_retail_look_through_the_day() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let sky = RemasterSky {
        atmosphere: true,
        fog: true,
        glow: 0.0,
        far_keep: 0.0,
        air: 0.0,
    };
    for (name, clock) in [
        ("night", Clock::Sunny(0.0)),
        ("dawn", Clock::SunRise(5.0)),
        ("morning", Clock::Sunny(0.35)),
        ("noon", Clock::Sunny(0.5)),
        ("afternoon", Clock::Sunny(0.65)),
        ("dusk", Clock::SunUp(5.0)),
        ("rain", Clock::Rainy(0.5)),
    ] {
        let station = town(&store, clock);
        let retail = remaster_frames::run(&store, &station, Draw::Retail, SIZE);
        let r = remaster_frames::run_with(&store, &station, Draw::Pixel, SIZE, Some(sky));
        let pixels = sky_pixels(&retail, &r);
        let top = (SIZE.0 * SIZE.1 * 2 / 5) as usize;
        let high: Vec<usize> = pixels.iter().copied().filter(|&i| i < top).collect();
        let (a, b) = (mean(&retail, &pixels), mean(&r, &pixels));
        let (ah, bh) = (mean(&retail, &high), mean(&r, &high));
        println!(
            "{name}: sky retail {a:.1?} remaster {b:.1?} off {:.1}; high sky retail {ah:.1?} \
             remaster {bh:.1?} off {:.1}",
            off(a, b),
            off(ah, bh)
        );
        assert!(high.len() > 10_000, "{name}: the high sky is in view");
        assert!(
            off(a, b) <= SKY_BAND,
            "{name}: the sky's mean color is {:.1} levels off Retail Look's",
            off(a, b)
        );
        assert!(
            off(ah, bh) <= HIGH_SKY_BAND,
            "{name}: the high sky's mean color is {:.1} levels off Retail Look's",
            off(ah, bh)
        );
        if name == "dusk" {
            let (red, blue) = (
                (bh[0] - bh[1]) / (ah[0] - ah[1]),
                (bh[2] - bh[1]) / (ah[2] - ah[1]),
            );
            println!("dusk keeps {red:.2} of the red and {blue:.2} of the blue over the green");
            assert!(
                red >= DUSK_PURPLE_KEPT && blue >= DUSK_PURPLE_KEPT,
                "dusk keeps its purple: {red:.2} of the red and {blue:.2} of the blue"
            );
        }
    }
}

#[test]
fn the_fog_volume_without_its_glow_is_retail_range_fog() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let plain = remaster_frames::run(&store, &town(&store, Clock::Rainy(0.5)), Draw::Pixel, SIZE);
    let volume = fogged(&store, 0.0);
    let world = world(&plain);
    let error = mean_error(&plain, &volume, &world);
    let over = world
        .iter()
        .filter(|&&i| (0..3).any(|c| plain.rgba[i * 4 + c].abs_diff(volume.rgba[i * 4 + c]) > 2))
        .count();
    println!(
        "{} world pixels, mean error {error:.3}, {over} over 2 levels",
        world.len()
    );
    assert!(world.len() > 10_000, "the town is in view");
    assert!(error < 0.1, "the fog is retail's, {error:.3} levels off");
    assert!(
        over * 1000 <= world.len(),
        "{over} pixels are over 2 levels off retail's fog"
    );
}

#[test]
fn the_fogs_glow_only_brightens_it() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (dull, glowing) = (fogged(&store, 0.0), fogged(&store, 1.0));
    let world = world(&dull);
    let level = |f: &Frame, i: usize| -> i32 { (0..3).map(|c| i32::from(f.rgba[i * 4 + c])).sum() };
    let darker = world
        .iter()
        .filter(|&&i| level(&glowing, i) < level(&dull, i) - 3)
        .count();
    let brighter = world
        .iter()
        .filter(|&&i| level(&glowing, i) > level(&dull, i) + 6)
        .count();
    println!(
        "{} world pixels, {darker} darker, {brighter} brighter",
        world.len()
    );
    assert_eq!(darker, 0, "the glow darkens nothing");
    assert!(
        brighter * 20 >= world.len(),
        "the glow brightens the fog: {brighter} of {} pixels",
        world.len()
    );
}

/// The forest looking into a low morning sun past trees and hills, per pixel with the sun's
/// shadows, the fog drawn from its volume with God Rays at `level`; without their clear air when
/// `clear` is false, the fog's glow alone.
fn shafts(store: &Arc<RetailDatStore>, level: u32, clear: bool) -> Frame {
    let station = remaster_frames::stations(store, &["grove-sun"])
        .pop()
        .expect("the grove");
    let sky = RemasterSky {
        atmosphere: false,
        fog: true,
        glow: 0.0,
        far_keep: 0.0,
        air: 0.0,
    }
    .with_god_rays(level);
    let sky = RemasterSky {
        air: if clear { sky.air } else { 0.0 },
        ..sky
    };
    remaster_frames::run_with(store, &station, Draw::Shadowed, SIZE, Some(sky))
}

/// God Rays only brighten: low brightens the air toward a low sun and darkens nothing, most of it
/// the clear air's shafts beyond the fog's glow, and high brightens it more than low does.
#[test]
fn god_rays_only_brighten_the_air_toward_the_sun_and_high_more_than_low() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let [off, low, high] = [0, 1, 2].map(|level| shafts(&store, level, true));
    let glow = shafts(&store, 1, false);
    let level = |f: &Frame, i: usize| -> i32 { (0..3).map(|c| i32::from(f.rgba[i * 4 + c])).sum() };
    let pixels = off.rgba.len() / 4;
    let darker = |a: &Frame, b: &Frame| {
        (0..pixels)
            .filter(|&i| level(b, i) < level(a, i) - 3)
            .count()
    };
    let added = |b: &Frame| -> i64 {
        (0..pixels)
            .map(|i| i64::from((level(b, i) - level(&off, i)).max(0)))
            .sum()
    };
    let brighter = (0..pixels)
        .filter(|&i| level(&low, i) > level(&off, i) + 6)
        .count();
    let (low_added, high_added, glow_added) = (added(&low), added(&high), added(&glow));
    println!(
        "{pixels} pixels: low brightens {brighter}, adding {low_added} levels ({glow_added} its \
         glow alone), high {high_added}; {} and {} darker",
        darker(&off, &low),
        darker(&low, &high)
    );
    assert_eq!(darker(&off, &low), 0, "low darkens nothing");
    assert_eq!(darker(&low, &high), 0, "high darkens nothing low drew");
    assert!(
        brighter * 50 >= pixels,
        "low brightens the air toward the sun: {brighter} of {pixels} pixels"
    );
    assert!(
        low_added > glow_added * 2,
        "the clear air's shafts add more than the fog's glow: {low_added} against {glow_added}"
    );
    assert!(
        high_added * 2 > low_added * 3,
        "high brightens it more than low: {high_added} against {low_added}"
    );
}

/// A hill north of Holtburg looking out at the far mountains, where the far land was seen to
/// flicker.
const FAR_MOUNTAINS: &str = "dereth report 1; loc 0xA9B4001F [85.297768 165.863739 66.005005] \
                             -0.988372 0.000000 -0.000000 -0.152058; weather off";

/// Every box the options page ticks at first.
const PAGE_BOXES: &str = "RemasterGrade=1,RemasterAir=1,RemasterGrass=1,RemasterFlowers=1";

/// How far off land is far, metres: well past the reach of the low fog.
const FAR_LAND: f64 = 4000.0;

/// The most each pixel of the far land may change, on average, in display levels, from one frame
/// to the next while the camera turns by a hair. Measured: 0.56; 1.73 when the low fog took a far
/// point's height over its ground from the depth.
const FAR_LAND_STILL: f64 = 1.0;

/// **The far land holds still while the camera turns by a hair**: with every box the options page
/// ticks at first, the camera held over the hill and turned back and forth by a hundred-thousandth
/// of a radian each frame, as a settling camera moves, each pixel of the land past [`FAR_LAND`]
/// changes by less than [`FAR_LAND_STILL`] a frame on average. The turn moves the picture by a
/// hundredth of a pixel.
#[test]
#[allow(clippy::cast_precision_loss)] // pixel counts and levels
fn the_far_land_holds_still_while_the_camera_turns_by_a_hair() {
    use crate::instruments::remaster_look_frames::EVERY_BOX;
    use crate::instruments::stations::{report_station, Shot};
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    use dereth_client_runtime::report::VisualReport;
    use dereth_render::device::{DeviceConfig, Gpu};

    const HAIR: f32 = 1e-5;
    const FRAMES: usize = 12;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = VisualReport::parse(FAR_MOUNTAINS).expect("the report reads");
    let station = report_station(&report, Some(8));
    let cfg = DeviceConfig {
        width: 960,
        height: 540,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&format!("{EVERY_BOX},{PAGE_BOXES}"))
            .expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    shot.step_held(&mut gpu);
    // Every far tree's pictures on the device, a few a frame.
    for _ in 0..400 {
        shot.step_held(&mut gpu);
        let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
        if s.is_none_or(|s| s.wanted > 0 && s.pictures >= s.wanted) {
            break;
        }
    }
    let camera = shot.scene.camera;
    let (at, yaw, pitch) = (camera.position, camera.yaw, camera.pitch);
    shot.hold_camera(at, yaw, pitch);
    for _ in 0..16 {
        shot.step_held(&mut gpu);
    }
    // The far land: the ground and what stands in the sun, past [`FAR_LAND`].
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    assert_eq!(
        (c.width, c.height),
        c.render_size,
        "the frame is drawn at its own size"
    );
    let inverse = glam::DMat4::from_cols_array(&c.view_projection.map(f64::from)).inverse();
    let eye = glam::DVec3::from_array(c.eye.map(f64::from));
    let (w, h) = (c.width as usize, c.height as usize);
    let far: Vec<usize> = (0..w * h)
        .filter(|&i| matches!(c.class[i], 1 | 2))
        .filter(|&i| {
            let ndc = glam::DVec4::new(
                ((i % w) as f64 + 0.5) / w as f64 * 2.0 - 1.0,
                1.0 - ((i / w) as f64 + 0.5) / h as f64 * 2.0,
                f64::from(c.depth[i]),
                1.0,
            );
            let p = inverse * ndc;
            (p.truncate() / p.w - eye).length() > FAR_LAND
        })
        .collect();
    assert!(
        far.len() > 2_000,
        "the far land is in view: {} pixels",
        far.len()
    );
    let mut last = c.color;
    let mut total = 0.0;
    for k in 0..FRAMES {
        let turn = if k % 2 == 0 { HAIR } else { 0.0 };
        shot.hold_camera(at, yaw + turn, pitch + turn);
        shot.step_held(&mut gpu);
        let now = shot
            .scene
            .draw
            .remaster_capture(&mut gpu)
            .expect("the remaster drew")
            .color;
        let moved: f64 = far
            .iter()
            .map(|&i| {
                (0..3)
                    .map(|j| f64::from(now[i * 4 + j].abs_diff(last[i * 4 + j])))
                    .fold(0.0, f64::max)
            })
            .sum::<f64>()
            / far.len() as f64;
        println!("frame {k}: the far land's pixels moved {moved:.2} levels from the last");
        total += moved;
        last = now;
    }
    let mean = total / FRAMES as f64;
    assert!(
        mean < FAR_LAND_STILL,
        "the far land's pixels change by {mean:.2} levels a frame while the camera turns by a hair"
    );
}

/// Below the desert's mesas at dawn, looking across the low ground at them: the low fog lies thick
/// and the mesas' shadows reach far across it.
const MESAS_AT_DAWN: &str = "dereth report 1; loc 0x817D0013 [71.660690 48.951466 29.687843] \
                             0.762868 0.000000 0.000000 -0.646555; camera 0x817D [65.81277 \
                             47.979786 32.11413] yaw -1.4061389 pitch -0.1550004 cell 0x817D0012; \
                             time 0.206 Foredawn-and-Half, set here; clock 308714622.0172466; \
                             weather auto";

/// How far ahead of the eye land lies past the reach of the sun's shadows on the land, metres
/// along the view: there the shadows darken only the fog in front of it.
const PAST_THE_SHADOWS: f64 = 600.0;

/// How many pixels each way round a pixel of the far ground must be far ground too for the pixel
/// to count.
const AROUND: usize = 2;

/// How sharply the fog's shadows may change from a pixel to the next on ground at about the same
/// distance, in display levels: a step this large or larger is the edge of a shadow drawn in
/// the fog.
const SHADOW_STEP: i32 = 4;

/// The largest share of the neighbouring pixels of the ground past the shadows' reach whose
/// fog's shadows change by [`SHADOW_STEP`] or more between them. Measured: 0.005% with the shadows
/// sought along each line of sight at a dozen places, moved on each frame; 0.93% when they were
/// sought at four fixed places.
const SHADOW_EDGES: f64 = 0.001;

/// **The low fog's shadows have no hard edges**: below the mesas at dawn, with every box the
/// options page ticks at first, the ground past the reach of the sun's shadows on the land,
/// drawn with the shadows and without, differs between the two only by the fog's shadows. Between
/// neighbouring pixels of it at about the same distance, away from the sky and from what stands
/// on it, that difference changes by [`SHADOW_STEP`] or more for no more than [`SHADOW_EDGES`] of
/// them, and the shadows still darken the fog. Sought at a few fixed places along each line, the
/// shadows stood in the fog as hard-edged copies of the mesas' shadows, in steps.
#[test]
#[allow(clippy::cast_precision_loss)] // pixel counts and levels
fn the_low_fogs_shadows_have_no_hard_edges() {
    use crate::instruments::remaster_look_frames::EVERY_BOX;
    use crate::instruments::stations::{report_station, Shot};
    use dereth_client_runtime::render_prefs::FidelityPreferences;
    use dereth_client_runtime::report::VisualReport;
    use dereth_render::device::{DeviceConfig, Gpu};

    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let report = VisualReport::parse(MESAS_AT_DAWN).expect("the report reads");
    let station = report_station(&report, Some(8));
    let cfg = DeviceConfig {
        width: 960,
        height: 540,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    let mut draw = |shadows: u32| {
        shot.scene.draw.cfg.render.fidelity = FidelityPreferences::parse_switch(&format!(
            "{EVERY_BOX},{PAGE_BOXES},RemasterShadows={shadows}"
        ))
        .expect("the boxes read");
        shot.poll(&mut gpu);
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        for _ in 0..400 {
            shot.step_held(&mut gpu);
            let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
            if s.is_none_or(|s| s.wanted > 0 && s.pictures >= s.wanted) {
                break;
            }
        }
        // The smoothing gathers every frame of its sequence and settles.
        for _ in 0..32 {
            shot.step_held(&mut gpu);
        }
        shot.scene
            .draw
            .remaster_capture(&mut gpu)
            .expect("the remaster drew")
    };
    let lit = draw(1);
    let unshadowed = draw(0);
    assert_eq!(
        (lit.width, lit.height),
        lit.render_size,
        "the frame is drawn at its own size"
    );
    let inverse = glam::DMat4::from_cols_array(&lit.view_projection.map(f64::from)).inverse();
    let eye = glam::DVec3::from_array(lit.eye.map(f64::from));
    let ahead = {
        let p = inverse * glam::DVec4::new(0.0, 0.0, 0.5, 1.0);
        (p.truncate() / p.w - eye).normalize()
    };
    let (w, h) = (lit.width as usize, lit.height as usize);
    // Each pixel's distance where it is ground past the shadows' reach.
    let far: Vec<Option<f64>> = (0..w * h)
        .map(|i| {
            if lit.class[i] != 1 || unshadowed.class[i] != 1 {
                return None;
            }
            let ndc = glam::DVec4::new(
                ((i % w) as f64 + 0.5) / w as f64 * 2.0 - 1.0,
                1.0 - ((i / w) as f64 + 0.5) / h as f64 * 2.0,
                f64::from(lit.depth[i]),
                1.0,
            );
            let p = inverse * ndc;
            let to = p.truncate() / p.w - eye;
            (to.dot(ahead) > PAST_THE_SHADOWS).then_some(to.length())
        })
        .collect();
    // The pixels whose neighbours out to [`AROUND`] are all such ground at about the same
    // distance: away from the sky, from what stands on the ground and from where nearer ground
    // hides farther, whose edges are the frame's own and not the fog's.
    let inside: Vec<bool> = (0..w * h)
        .map(|i| {
            let Some(d) = far[i] else {
                return false;
            };
            let (x, y) = (i % w, i / w);
            if x < AROUND || y < AROUND || x + AROUND >= w || y + AROUND >= h {
                return false;
            }
            (y - AROUND..=y + AROUND).all(|ny| {
                (x - AROUND..=x + AROUND)
                    .all(|nx| far[ny * w + nx].is_some_and(|e| (d - e).abs() <= d * 0.02))
            })
        })
        .collect();
    let shadow = |i: usize| -> [i32; 3] {
        [0, 1, 2].map(|c| i32::from(lit.color[i * 4 + c]) - i32::from(unshadowed.color[i * 4 + c]))
    };
    let (mut pairs, mut edges, mut darkened) = (0usize, 0usize, 0usize);
    for i in (0..w * h).filter(|&i| inside[i]) {
        if shadow(i).iter().any(|v| *v < -2) {
            darkened += 1;
        }
        let right = (i % w + 1 < w).then_some(i + 1);
        let down = (i / w + 1 < h).then_some(i + w);
        for j in [right, down].into_iter().flatten().filter(|&j| inside[j]) {
            pairs += 1;
            let (a, b) = (shadow(i), shadow(j));
            if (0..3).any(|c| (a[c] - b[c]).abs() >= SHADOW_STEP) {
                edges += 1;
            }
        }
    }
    let share = edges as f64 / pairs.max(1) as f64;
    println!(
        "{pairs} neighbouring pairs of ground past the shadows' reach, {darkened} pixels darkened by \
         the fog's shadows; {edges} pairs ({:.3}%) a step of {SHADOW_STEP} levels or more apart",
        share * 100.0
    );
    assert!(pairs > 20_000, "the far land is in view: {pairs} pairs");
    assert!(
        darkened * 20 > pairs,
        "the fog's shadows darken the far land: {darkened} pixels"
    );
    assert!(
        share <= SHADOW_EDGES,
        "the fog's shadows change by {SHADOW_STEP} levels or more between {edges} of {pairs} \
         neighbouring pairs"
    );
}
