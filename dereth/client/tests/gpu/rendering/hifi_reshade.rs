//! The re-shaded world: drawn with the pipelines derived from the ordinary ones and written back
//! through the neutral resolve, it is the ordinary frame but for blending in linear light; it is
//! built without holding up a frame; and a frame that steps indoors is re-shaded only up to the
//! step.
//!
//! Each station is loaded twice and stepped the same way, once with the presentation off and once
//! with re-shading on (the lighting option, whose own pass draws nothing yet, so the picture is
//! the neutral resolve), because the sky's clouds move with every frame drawn. Between steps of
//! the re-shaded run the test waits for the pipelines being built, so the frame it compares is
//! re-shaded whole.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(all(gpu, feature = "hifi"))]

use std::time::{Duration, Instant};

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// Frames each station is stepped before it is compared.
const STEPS: usize = 6;
/// The longest the test waits for the pipelines between two steps.
const SETTLE: Duration = Duration::from_secs(60);

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

/// Set `shot`'s `[Fidelity]` preferences and poll them, as the application does.
fn set(shot: &mut Shot, gpu: &mut Gpu, prefs: FidelityPreferences) {
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&crate::common::dats(), gpu)
        .expect("the preferences poll");
}

/// `station` loaded with `prefs` and stepped [`STEPS`] frames, waiting for the presentation's
/// pipelines after each when `settle`; the last frame and the presentation's report of it.
fn run(
    gpu: &mut Gpu,
    station: &Station,
    prefs: FidelityPreferences,
    settle: bool,
) -> (Vec<u8>, Option<dereth_render::wgpu::sidecar::SidecarReport>) {
    let store = crate::common::dats();
    let mut shot = Shot::open(&store, gpu, station);
    set(&mut shot, gpu, prefs);
    for _ in 0..STEPS {
        shot.step(gpu);
        if settle {
            assert!(
                gpu.hifi_settle(SETTLE),
                "{}: the pipelines were not built in {SETTLE:?}",
                station.name
            );
        }
    }
    let px = capture(gpu);
    let report = gpu.hifi_report();
    set(&mut shot, gpu, FidelityPreferences::default());
    (px, report)
}

/// How two pictures differ: the mean absolute difference over every colour channel, in levels,
/// and the share of pixels whose largest channel difference is more than `edge` levels -- a
/// surface drawn where the other picture has another, as at a silhouette.
fn difference(a: &[u8], b: &[u8], edge: u8) -> (f64, f64) {
    assert_eq!(a.len(), b.len());
    let mut sum = 0u64;
    let mut far = 0usize;
    let pixels = a.len() / 4;
    for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        let d: Vec<u8> = (0..3).map(|c| pa[c].abs_diff(pb[c])).collect();
        sum += d.iter().map(|v| u64::from(*v)).sum::<u64>();
        far += usize::from(d.iter().any(|v| *v > edge));
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    (sum as f64 / (pixels * 3) as f64, far as f64 / pixels as f64)
}

/// With `DERETH_HIFI_CAPTURE_DIR` set, write the two frames of `name` and their difference
/// (eight times brighter) into its `reshade/parity` folder.
fn keep(name: &str, off: &[u8], on: &[u8]) {
    let Some(root) = std::env::var_os("DERETH_HIFI_CAPTURE_DIR") else {
        return;
    };
    let dir = std::path::Path::new(&root).join("reshade").join("parity");
    let diff: Vec<u8> = off
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0)
        .flat_map(|(a, b)| {
            let d = |c: usize| a[c].abs_diff(b[c]).saturating_mul(8);
            [d(0), d(1), d(2), 255]
        })
        .collect();
    for (suffix, px) in [("off", off), ("on", on), ("diff", &diff[..])] {
        hifi_stations::write_png(&dir.join(format!("{name}-{suffix}.png")), W, H, px);
    }
}

/// The count the presentation noted under `name` for the last frame.
fn note(report: &dereth_render::wgpu::sidecar::SidecarReport, name: &str) -> u64 {
    report
        .notes
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| panic!("no note {name:?} in {report:?}"), |(_, v)| *v)
}

/// The levels past which a pixel counts as a different surface.
const SILHOUETTE: u8 = 48;

fn outdoor(station: &Station) -> bool {
    !matches!(station.name, "doorway" | "indoor" | "dungeon")
}

/// Behaviour: hifi.reshade.the-parity-view-matches-the-ordinary-frame
/// At every outdoor station, every draw of the world is re-shaded, and the neutral resolve of the
/// re-shaded world differs from the ordinary frame by less than two levels on average, with fewer
/// than one percent of pixels different by more than a silhouette's worth (the edges of the
/// foliage's cut-out cards are where they differ).
#[test]
fn the_parity_view_matches_the_ordinary_frame_at_every_outdoor_station() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    let stations: Vec<Station> = hifi_stations::isolation_stations(&store)
        .into_iter()
        .filter(outdoor)
        .collect();
    assert!(!stations.is_empty());
    let mut failures = Vec::new();
    for station in &stations {
        let (off, _) = run(&mut gpu, station, FidelityPreferences::default(), false);
        let (on, report) = run(&mut gpu, station, fidelity("Debug=7"), true);
        let report = report.expect("the presentation was installed");
        assert!(report.composited, "{}: {report:?}", station.name);
        let recorded = note(&report, "drawn as recorded before the re-shade");
        assert!(
            note(&report, "re-shaded") == 1
                && report.census.reshaded > 0
                && u64::from(report.census.legacy) == recorded,
            "{}: not every draw of the world after the sky was re-shaded: {:?}",
            station.name,
            report
        );
        let (mean, silhouette) = difference(&off, &on, SILHOUETTE);
        keep(station.name, &off, &on);
        eprintln!(
            "{}: mean difference {mean:.3} levels, {:.3}% of pixels past {SILHOUETTE} levels, \
             {} draws re-shaded",
            station.name,
            silhouette * 100.0,
            report.census.reshaded
        );
        if mean >= 2.0 || silhouette >= 0.01 {
            failures.push(format!(
                "{}: mean {mean:.3}, silhouette {:.3}%",
                station.name,
                silhouette * 100.0
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// Behaviour: hifi.reshade.an-indoor-frame-re-shades-only-its-outdoor-part
/// At the stations that step indoors, the frame with re-shading on is composited, the world after
/// the step is replayed with its own pipelines, and the picture matches the ordinary frame within
/// the outdoor stations' bounds; at the doorway the outdoor part is re-shaded.
#[test]
fn an_indoor_frame_is_re_shaded_up_to_its_step_and_drawn_as_recorded_after() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    for station in hifi_stations::stations(&store)
        .iter()
        .filter(|s| !outdoor(s))
    {
        let (off, _) = run(&mut gpu, station, FidelityPreferences::default(), false);
        let (on, report) = run(&mut gpu, station, fidelity("Lighting=1"), true);
        let report = report.expect("the presentation was installed");
        assert!(report.composited, "{}: {report:?}", station.name);
        let (mean, silhouette) = difference(&off, &on, SILHOUETTE);
        eprintln!(
            "{}: mean difference {mean:.3} levels, {:.3}% past {SILHOUETTE} levels, census {:?}",
            station.name,
            silhouette * 100.0,
            report.census
        );
        let (before, after) = (
            note(&report, "drawn as recorded before the re-shade"),
            note(&report, "drawn as recorded after an indoor step"),
        );
        eprintln!(
            "{}: {before} draws as recorded before, {after} after the step",
            station.name
        );
        assert_eq!(
            note(&report, "re-shaded"),
            1,
            "{}: {report:?}",
            station.name
        );
        assert!(
            after > 0,
            "{}: the frame did not step indoors",
            station.name
        );
        assert!(
            report.census.reshaded > 0,
            "{}: nothing was re-shaded",
            station.name
        );
        assert_eq!(
            u64::from(report.census.legacy),
            before + after,
            "{}: a draw was drawn as recorded outside the sky and the indoor step",
            station.name
        );
        assert!(
            mean < 2.0 && silhouette < 0.005,
            "{}: mean {mean:.3}, silhouette {:.3}%",
            station.name,
            silhouette * 100.0
        );
    }
}

/// Behaviour: hifi.reshade.enabling-never-holds-up-a-frame
/// From the frame that turns re-shading on (the parity view, with no pass of its own) until every
/// pipeline it needs is built, and for some frames after, no frame takes 50 milliseconds: the
/// frames are drawn the ordinary way while the pipelines are built off the frame, and then
/// re-shaded. Turning it off again leaves the device holding exactly the objects it held before.
#[test]
fn turning_re_shading_on_never_holds_a_frame_past_fifty_milliseconds() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == "holtburg")
        .expect("the town station");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    for _ in 0..30 {
        shot.step(&mut gpu);
    }
    let mut off_ms = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        shot.step(&mut gpu);
        off_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let objects_off = gpu.device_objects();
    let mut on_ms = Vec::new();
    let mut overlay_frames = 0;
    let mut after = None;
    let started = Instant::now();
    for frame in 0..2_000 {
        let t = Instant::now();
        if frame == 0 {
            shot.scene.draw.cfg.render.fidelity = fidelity("Debug=7");
            shot.scene
                .update_from_preferences(&store, &mut gpu)
                .expect("the preferences poll");
        }
        shot.step(&mut gpu);
        on_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        let report = gpu.hifi_report().expect("installed");
        assert!(report.composited, "frame {frame}: {report:?}");
        let reshaded = note(&report, "re-shaded") == 1;
        if reshaded {
            after = Some(after.map_or(0, |n| n + 1));
        } else {
            overlay_frames += 1;
            assert_eq!(
                after, None,
                "frame {frame} fell back after re-shading began"
            );
        }
        if after == Some(30) {
            break;
        }
    }
    let worst = on_ms.iter().copied().fold(0.0, f64::max);
    let worst_off = off_ms.iter().copied().fold(0.0, f64::max);
    eprintln!(
        "re-shading reached after {overlay_frames} ordinary frames, {:.0} ms; worst frame {worst:.2} ms \
         on, {worst_off:.2} ms off; first frame {:.2} ms",
        started.elapsed().as_secs_f64() * 1000.0,
        on_ms[0]
    );
    assert!(
        after == Some(30),
        "re-shading never began over {} frames",
        on_ms.len()
    );
    assert!(
        overlay_frames > 0,
        "the pipelines were ready before the first frame"
    );
    assert!(
        worst < 50.0,
        "a frame took {worst:.2} ms while re-shading was turned on"
    );

    shot.scene.draw.cfg.render.fidelity = FidelityPreferences::default();
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    assert!(gpu.hifi_sidecar_mut().is_none());
    shot.step(&mut gpu);
    assert_eq!(
        gpu.device_objects(),
        objects_off,
        "the re-shade left an object on the device"
    );
}

/// The share of the pixels in the lower third of `rgba` (`W` by `H`) that the census view tints
/// as landscape: its blue well above its red and green.
fn landscape_share(rgba: &[u8]) -> f64 {
    let (mut all, mut ground) = (0usize, 0usize);
    for y in (H * 2 / 3)..H {
        for x in 0..W {
            let at = ((y * W + x) * 4) as usize;
            let (r, g, b) = (
                i32::from(rgba[at]),
                i32::from(rgba[at + 1]),
                i32::from(rgba[at + 2]),
            );
            all += 1;
            ground += usize::from(b > r + 20 && b > g + 20);
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = ground as f64 / all as f64;
    share
}

/// Behaviour: hifi.reshade.the-landscape-takes-the-fields-smooth-normals
/// Where the drawn ground fills the lower third of the view, most of it is found on the
/// landscape field and given the field's smooth normal and the landscape class, which the
/// census view tints.
#[test]
fn the_ground_in_view_takes_the_landscape_fields_normal_and_class() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    let mut failures = Vec::new();
    for station in hifi_stations::stations(&store)
        .iter()
        .filter(|s| matches!(s.name, "holtburg" | "coast" | "forest" | "vista"))
    {
        let (census, report) = run(&mut gpu, station, fidelity("Debug=6"), true);
        let report = report.expect("the presentation was installed");
        assert_eq!(
            note(&report, "re-shaded"),
            1,
            "{}: {report:?}",
            station.name
        );
        let share = landscape_share(&census);
        eprintln!(
            "{}: {:.1}% of the lower third is landscape",
            station.name,
            share * 100.0
        );
        if share < 0.5 {
            failures.push(format!("{}: {:.1}%", station.name, share * 100.0));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
