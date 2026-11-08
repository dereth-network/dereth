//! The remaster's sky and fog. With the atmosphere off, the sky is retail's own, as Retail Look
//! draws it. With it on, the physical sky is drawn over the authored dome: its mean color through
//! the day stays within a measured band of Retail Look's, closer in the high sky than near the
//! horizon, where the low sun's glow is its own, and dusk keeps its purple. The fog drawn from its
//! volume is retail's range fog without its glow; with the glow it only brightens. God Rays only
//! brighten it too, the air toward a low sun most, high more than low.
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
