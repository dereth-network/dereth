//! The physical sky and the air of the high-fidelity presentation: the sky takes the authored
//! dome's place under the clouds, which stay; the farther the land, the more it takes the sky's
//! colour over the horizon; hills at the edge of the land fade evenly, with no bright band below
//! their crest; the authored night sky, its stars and the hills' line against it are kept; at
//! dusk the near land toward the sun keeps its colour and a near tree its silhouette; a rainy
//! sky is no brighter than the authored one; interiors and frames split between an interior and
//! the outdoors are drawn as they always are; and the frame the scene records is the same with
//! them on.
//!
//! Each frame is compared with the same station loaded again and stepped the same number of
//! frames with the presentation off, because the clouds move with every frame drawn.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::sidecar::SidecarReport;
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, moved, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// Frames a station is stepped before it is captured.
const STEPS: usize = 6;

fn device() -> Gpu {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        hifi: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("these stations need a wgpu device and none opened: {e}"));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasteriser",
        gpu.adapter_name()
    );
    gpu
}

fn fidelity(spec: &str) -> FidelityPreferences {
    FidelityPreferences::parse_switch(spec).expect("a fidelity spec")
}

fn station(name: &str) -> Station {
    hifi_stations::stations(&crate::common::dats())
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// One run of a station: its last frame, the frame's recorded digest, and what the
/// presentation did with it.
struct Run {
    rgba: Vec<u8>,
    digest: Option<u64>,
    report: Option<SidecarReport>,
}

/// `station` loaded with `prefs` and stepped [`STEPS`] frames, on a device of its own so the
/// texture slots, and with them the recorded frame, are the same from run to run.
fn run(station: &Station, prefs: FidelityPreferences) -> Run {
    let mut device = device();
    let gpu = &mut device;
    let store = crate::common::dats();
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&store, gpu)
        .expect("the preferences poll");
    gpu.set_frame_digest(true);
    for _ in 0..STEPS {
        shot.step(gpu);
    }
    Run {
        rgba: capture(gpu),
        digest: gpu.last_frame_digest(),
        report: gpu.hifi_report(),
    }
}

fn luminance(rgba: &[u8], x: u32, y: u32) -> f32 {
    let i = ((y * W + x) * 4) as usize;
    0.2126 * f32::from(rgba[i]) + 0.7152 * f32::from(rgba[i + 1]) + 0.0722 * f32::from(rgba[i + 2])
}

/// The frame's depth view at `station`: near white, far black on a logarithmic scale; the sky,
/// where nothing was drawn with depth, black.
fn depth_view(station: &Station) -> Vec<u8> {
    run(station, fidelity("Debug=2")).rgba
}

/// The first row of land from the top in column `x` of a depth view, if any.
fn land_top(depth: &[u8], x: u32) -> Option<u32> {
    (0..H).find(|y| depth[((y * W + x) * 4) as usize] > 0)
}

/// The mean luminance step between neighbours two pixels apart over rows `rows`: near zero for
/// a smooth gradient, large where the clouds' texture is.
fn texture_energy(rgba: &[u8], rows: std::ops::Range<u32>) -> f32 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in rows {
        for x in (40..W - 2).step_by(2) {
            sum += (luminance(rgba, x, y) - luminance(rgba, x + 2, y)).abs();
            n += 1.0;
        }
    }
    sum / n
}

/// Behaviour: hifi.sky.the-physical-sky-takes-the-domes-place-under-the-clouds
/// At the town on a sunny noon, the sky changes, the clouds are still drawn over the new sky
/// with their texture, the world is replayed without its fog, and the scene records exactly the
/// frame it records with the presentation off.
#[test]
fn the_physical_sky_takes_the_domes_place_and_the_clouds_stay_over_it() {
    let _gpu = crate::common::gpu_lock();
    let town = station("holtburg");
    let off = run(&town, FidelityPreferences::default());
    let on = run(&town, fidelity("Sky=3"));
    let report = on.report.expect("the presentation is installed");
    assert!(report.composited, "{report:?}");
    for pass in ["sky tables", "sky", "atmosphere"] {
        assert!(report.passes.contains(&pass), "no {pass}: {report:?}");
    }
    assert!(
        report.census.reframed > 0,
        "the fog was not taken out: {report:?}"
    );
    assert_eq!(
        on.digest, off.digest,
        "the scene recorded another frame, or the recorded constants changed"
    );
    let sky = 0..H / 3;
    let bytes = (sky.end * W * 4) as usize;
    let changed = moved(&off.rgba[..bytes], &on.rgba[..bytes]);
    assert!(
        changed > bytes / 3,
        "the sky barely changed: {changed} of {bytes}"
    );
    let (before, after) = (
        texture_energy(&off.rgba, sky.clone()),
        texture_energy(&on.rgba, sky),
    );
    eprintln!("cloud texture: {before:.2} off, {after:.2} on");
    assert!(
        after > before * 0.5 && after > 0.3,
        "the clouds are gone from the sky: {after:.2} against {before:.2}"
    );
}

/// Behaviour: hifi.sky.the-farther-the-land-the-more-it-takes-the-horizons-colour
/// Over the wide view from high ground, the farther the land, the nearer its colour comes to
/// the sky's just over the horizon: binned by the frame's depth, the land's mean distance from
/// that colour falls from the nearest land to the farthest, where it is well under two thirds
/// of what it is near.
#[test]
fn the_farther_the_land_the_more_it_takes_the_colour_of_the_sky_over_the_horizon() {
    let _gpu = crate::common::gpu_lock();
    let vista = station("vista");
    let on = run(&vista, fidelity("Sky=3"));
    // The world's depth, near white and far black on a logarithmic scale; the sky is black.
    let depth = run(&vista, fidelity("Debug=2"));
    let at = |x: u32, y: u32| ((y * W + x) * 4) as usize;
    // The sky's colour over the horizon: the sky pixels within a few rows above the land in
    // every column.
    let mut horizon = [0.0f64; 3];
    let mut n = 0.0;
    for x in 0..W {
        let Some(top) = (0..H).find(|y| depth.rgba[at(x, *y)] > 0) else {
            continue;
        };
        for y in top.saturating_sub(12)..top.saturating_sub(4) {
            for (c, h) in horizon.iter_mut().enumerate() {
                *h += f64::from(on.rgba[at(x, y) + c]);
            }
            n += 1.0;
        }
    }
    assert!(n > 1000.0, "no sky over the land");
    for h in &mut horizon {
        *h /= n;
    }
    let bins = 4usize;
    let (lo, hi) = (20u8, 120u8);
    let mut distance = vec![(0.0f64, 0u32); bins];
    for p in 0..(W * H) as usize {
        let g = depth.rgba[p * 4];
        if g < lo || g >= hi {
            continue;
        }
        let bin = usize::from(g - lo) * bins / usize::from(hi - lo);
        let d: f64 = (0..3)
            .map(|c| (f64::from(on.rgba[p * 4 + c]) - horizon[c]).abs())
            .sum();
        distance[bin].0 += d / 3.0;
        distance[bin].1 += 1;
    }
    // Far first: bin 0 holds the darkest depth, the farthest land.
    let means: Vec<f64> = distance
        .iter()
        .map(|(sum, n)| if *n == 0 { 0.0 } else { sum / f64::from(*n) })
        .collect();
    eprintln!("horizon {horizon:?}; distance from it, far to near: {means:?} over {distance:?}");
    assert!(distance.iter().all(|(_, n)| *n > 200), "{distance:?}");
    for w in means.windows(2) {
        assert!(
            w[0] <= w[1] + 0.5,
            "farther land is farther from the horizon's colour than nearer land: {means:?}"
        );
    }
    assert!(means[0] < means[bins - 1] * 0.6, "{means:?}");
}

/// Behaviour: hifi.sky.the-authored-night-sky-and-its-stars-are-kept
/// At midnight the physical sky draws nothing over the authored dark sky: high in the sky the
/// picture is the ordinary one to within the haze's dither, and its stars are still there. Low
/// over the horizon the haze's veil stays darker than the hazed land, so where the land meets
/// the sky it stands out from it at least half as much as it does with the air off.
#[test]
fn at_night_the_authored_sky_and_its_stars_are_kept() {
    let _gpu = crate::common::gpu_lock();
    let night = station("night");
    let off = run(&night, FidelityPreferences::default());
    let on = run(&night, fidelity("Sky=3"));
    let depth = depth_view(&night);
    // The land's luminance just under its line less the sky's just over it, summed over every
    // column where the land meets the sky.
    let line = |rgba: &[u8]| {
        let mut sum = 0.0;
        let mut n = 0;
        for x in 0..W {
            let Some(top) = land_top(&depth, x) else {
                continue;
            };
            if top < 10 || top + 10 >= H {
                continue;
            }
            let land: f32 = (top + 2..top + 6)
                .map(|y| luminance(rgba, x, y))
                .sum::<f32>()
                / 4.0;
            let sky: f32 = (top - 6..top - 2)
                .map(|y| luminance(rgba, x, y))
                .sum::<f32>()
                / 4.0;
            sum += land - sky;
            n += 1;
        }
        assert!(n > W / 2, "the land meets the sky in only {n} columns");
        sum / n as f32
    };
    let (line_off, line_on) = (line(&off.rgba), line(&on.rgba));
    eprintln!("the hills' line: {line_off:.2} off, {line_on:.2} on");
    assert!(
        line_off > 4.0,
        "the station's hills do not stand out: {line_off:.2}"
    );
    assert!(
        line_on >= line_off * 0.5,
        "the hills fade into the night sky: {line_on:.2} against {line_off:.2}"
    );
    assert!(on.report.is_some_and(|r| r.composited));
    let rows = H / 8;
    let bytes = (rows * W * 4) as usize;
    let mean: f64 = off.rgba[..bytes]
        .iter()
        .zip(&on.rgba[..bytes])
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / bytes as f64;
    // A star is a pixel brighter by far than the sky three pixels to either side of it.
    let stars = |rgba: &[u8]| {
        (0..rows)
            .flat_map(|y| (3..W - 3).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let l = luminance(rgba, *x, *y);
                l > luminance(rgba, x - 3, *y) + 25.0 && l > luminance(rgba, x + 3, *y) + 25.0
            })
            .count()
    };
    let (before, after) = (stars(&off.rgba), stars(&on.rgba));
    eprintln!("high sky: mean change {mean:.2}; stars {before} off, {after} on");
    assert!(mean < 3.0, "the night sky was drawn over: {mean:.2}");
    assert!(before > 20, "the station shows no stars: {before}");
    assert!(after * 10 >= before * 9, "stars lost: {after} of {before}");
}

/// How much brighter, at most, the land gets below its top in column `x` of `rgba` than at its
/// top, over the eighth of the frame under it, smoothed over five rows; `None` where the
/// column's land does not run that far unbroken.
fn bump_below_the_top(rgba: &[u8], depth: &[u8], x: u32) -> Option<f32> {
    let top = land_top(depth, x)?;
    let rows = H / 8;
    if top < 4 || top + rows >= H {
        return None;
    }
    if (top..top + rows).any(|y| depth[((y * W + x) * 4) as usize] == 0) {
        return None;
    }
    let lum: Vec<f32> = (top..top + rows).map(|y| luminance(rgba, x, y)).collect();
    let smooth: Vec<f32> = lum
        .windows(5)
        .map(|w| w.iter().sum::<f32>() / 5.0)
        .collect();
    let at_top = (smooth[0] + smooth[1]) / 2.0;
    Some(smooth[3..].iter().fold(f32::MIN, |m, v| m.max(*v)) - at_top)
}

/// Behaviour: hifi.sky.hills-at-the-edge-of-the-land-fade-evenly
/// Over the shore and the town, whose hills reach the edge of the resident landscape, the air
/// fades the hills evenly: in no column does the land below a hill's top brighten against the
/// top by more than a few levels beyond what it does with the air off, so no band of mist hangs
/// below the crests.
#[test]
fn hills_at_the_edge_of_the_land_fade_evenly_with_no_band_of_mist_below_their_crest() {
    let _gpu = crate::common::gpu_lock();
    for name in ["coast", "holtburg"] {
        let at = station(name);
        let off = run(&at, FidelityPreferences::default());
        let on = run(&at, fidelity("Sky=3"));
        let depth = depth_view(&at);
        let mut worst = (f32::MIN, 0);
        let mut columns = 0;
        for x in (0..W).step_by(2) {
            let (Some(b_on), Some(b_off)) = (
                bump_below_the_top(&on.rgba, &depth, x),
                bump_below_the_top(&off.rgba, &depth, x),
            ) else {
                continue;
            };
            columns += 1;
            if b_on - b_off > worst.0 {
                worst = (b_on - b_off, x);
            }
        }
        eprintln!(
            "{name}: {columns} columns; the most the land brightens below a crest beyond the \
             frame with the air off: {:.2} at x {}",
            worst.0, worst.1
        );
        assert!(
            columns > W as usize / 8,
            "{name}: too few columns of land: {columns}"
        );
        assert!(
            worst.0 < 5.0,
            "{name}: a band of mist below a crest at x {}: {:.2} levels",
            worst.1,
            worst.0
        );
    }
}

/// Behaviour: hifi.sky.at-dusk-the-near-field-keeps-its-colour
/// At dusk, with the sun low on the left of the view, the near land on that side is drawn
/// exactly as it is with the air off, and the near pine at the left edge, drawn over the glow,
/// stays dark against the sky behind it rather than taking the glow's colour.
#[test]
fn at_dusk_the_near_land_toward_the_sun_keeps_its_colour_and_a_near_tree_its_silhouette() {
    let _gpu = crate::common::gpu_lock();
    let dusk = station("dusk");
    let off = run(&dusk, FidelityPreferences::default());
    let on = run(&dusk, fidelity("Sky=3"));
    let depth = depth_view(&dusk);
    let at = |x: u32, y: u32| ((y * W + x) * 4) as usize;
    // The near land: closer than about sixty metres, in the half of the view toward the sun.
    let mut changes: Vec<u8> = Vec::new();
    for y in 0..H {
        for x in 0..W / 2 {
            if depth[at(x, y)] >= 100 {
                let i = at(x, y);
                changes.extend((0..3).map(|c| off.rgba[i + c].abs_diff(on.rgba[i + c])));
            }
        }
    }
    assert!(
        changes.len() > 30_000,
        "too little near land: {}",
        changes.len()
    );
    #[allow(clippy::cast_precision_loss)] // a pixel count
    let mean = changes.iter().map(|c| f64::from(*c)).sum::<f64>() / changes.len() as f64;
    changes.sort_unstable();
    let p99 = changes[changes.len() * 99 / 100];
    eprintln!("near land toward the sun: mean change {mean:.2}, 99th percentile {p99}");
    assert!(
        mean < 1.0 && p99 <= 3,
        "the near land changed: {mean:.2}, {p99}"
    );
    // The pine: what has depth at the left edge between a quarter and a half of the way down,
    // against the sky around it there.
    let (rows, cols) = (H * 28 / 100..H * 48 / 100, 0..W * 6 / 100);
    let mut pine = Vec::new();
    let mut sky = Vec::new();
    for y in rows {
        for x in cols.clone() {
            let l = luminance(&on.rgba, x, y);
            if depth[at(x, y)] > 0 {
                pine.push(l);
            } else {
                sky.push(l);
            }
        }
    }
    let median = |v: &mut Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    assert!(
        pine.len() > 500 && sky.len() > 500,
        "{} {}",
        pine.len(),
        sky.len()
    );
    let (pine, sky) = (median(&mut pine), median(&mut sky));
    eprintln!("the near pine: {pine:.1} against the sky behind it {sky:.1}");
    assert!(
        pine < sky * 0.4,
        "the near pine took the glow: {pine:.1} against {sky:.1}"
    );
}

/// Behaviour: hifi.sky.a-rainy-sky-is-no-brighter-than-the-authored-one
/// On a rainy day the sky, the clouds and the rain drawn over it are on the whole no brighter
/// with the physical sky than the authored sky is.
#[test]
fn a_rainy_sky_is_no_brighter_than_the_authored_one() {
    let _gpu = crate::common::gpu_lock();
    let rain = station("rain");
    let off = run(&rain, FidelityPreferences::default());
    let on = run(&rain, fidelity("Sky=3"));
    let depth = depth_view(&rain);
    let (mut sum_off, mut sum_on, mut n) = (0.0f64, 0.0f64, 0u32);
    for y in 0..H {
        for x in 0..W {
            if depth[((y * W + x) * 4) as usize] == 0 {
                sum_off += f64::from(luminance(&off.rgba, x, y));
                sum_on += f64::from(luminance(&on.rgba, x, y));
                n += 1;
            }
        }
    }
    assert!(n > W * H / 4, "too little sky: {n}");
    let (mean_off, mean_on) = (sum_off / f64::from(n), sum_on / f64::from(n));
    eprintln!("the rainy sky's mean luminance: {mean_off:.1} off, {mean_on:.1} on");
    assert!(
        mean_on <= mean_off + 0.5,
        "{mean_on:.1} against {mean_off:.1}"
    );
}

/// Behaviour: hifi.sky.interiors-and-split-frames-are-drawn-as-they-always-are
/// A body inside a room, and a body in a doorway with its camera in the room, are drawn through
/// the presentation with the sky and the air on exactly as they are with it off.
#[test]
fn interiors_and_split_frames_are_drawn_as_they_always_are() {
    let _gpu = crate::common::gpu_lock();
    for name in ["doorway", "indoor"] {
        let at = station(name);
        let off = run(&at, FidelityPreferences::default());
        let on = run(&at, fidelity("Sky=3"));
        let changed = moved(&off.rgba, &on.rgba);
        eprintln!("{name}: {changed} bytes differ");
        assert_eq!(changed, 0, "{name}: the interior changed");
        assert_eq!(on.digest, off.digest, "{name}");
    }
}
