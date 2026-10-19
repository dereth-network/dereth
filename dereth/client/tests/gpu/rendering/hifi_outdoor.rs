//! Behaviour: none (experimental Horizon rendering effects: how bright the better lighting draws
//! the outdoors against the ordinary picture)
//!
//! The better lighting outdoors, by day: at midday, in the low sun of the evening, among trees,
//! in the town street and on snow, with the lighting alone and with every box ticked, the frame is
//! about as bright as the ordinary picture -- a little brighter at most, never darker -- however
//! low the sun. At night it stays as dark as it draws it. Under the overcast sky of a rainy
//! day, the bounced light leaves the world's own trees about as bright as the lighting alone
//! draws them.
//!
//! Each frame is compared with the same station loaded again on a device of its own and stepped
//! the same number of frames with the presentation off.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, write_png, Clock, Place, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// Frames a station is stepped before it is captured, with the presentation and without.
const STEPS: usize = 12;
/// Every box, as the options page ticks them.
const EVERY_BOX: &str =
    "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1,Weather=1";

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

fn station(name: &str) -> Station {
    let store = crate::common::dats();
    hifi_stations::stations(&store)
        .into_iter()
        .chain(hifi_stations::close_stations(&store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// `station` loaded with `prefs` and stepped [`STEPS`] frames on a device of its own, waiting for
/// the presentation's background work after each: the mean brightness of its last frame, 0..255.
fn mean(station: &Station, prefs: FidelityPreferences) -> f32 {
    let px = frame(station, prefs);
    let n = px.len() / 4;
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let sum: f32 = px.as_chunks::<4>().0.iter().map(luma).sum::<f32>() / n as f32;
    sum
}

fn luma(p: &[u8; 4]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

/// `station` loaded with `prefs` and stepped [`STEPS`] frames on a device of its own, waiting for
/// the presentation's background work after each: its last frame.
fn frame(station: &Station, prefs: FidelityPreferences) -> Vec<u8> {
    let mut device = device();
    let gpu = &mut device;
    let store = crate::common::dats();
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&store, gpu)
        .expect("the preferences poll");
    for _ in 0..STEPS {
        shot.step(gpu);
        assert!(gpu.hifi_settle(std::time::Duration::from_secs(60)));
    }
    if prefs.any_effective() {
        assert!(
            gpu.hifi_failed().is_none(),
            "{}: {:?}",
            station.name,
            gpu.hifi_failed()
        );
    }
    capture(gpu)
}

#[test]
fn by_day_the_better_lighting_draws_the_outdoors_about_as_bright_as_the_ordinary_picture() {
    let _gpu = crate::common::gpu_lock();
    let mut failures = Vec::new();
    // Midday in the town; the evening sun five and ten degrees up; trees and the town street in
    // it; snow on a rainy day; a body on a grassy slope at noon, where bounced light gathers.
    for name in [
        "holtburg",
        "dusk",
        "glade-1",
        "town-1",
        "snow-village",
        "figure",
    ] {
        let station = station(name);
        let off = mean(&station, FidelityPreferences::default());
        for spec in ["Lighting=1", EVERY_BOX] {
            let on = mean(
                &station,
                FidelityPreferences::parse_switch(spec).expect("a fidelity spec"),
            );
            let ratio = on / off;
            eprintln!("{name} with {spec}: mean {off:.1} -> {on:.1} ({ratio:.3})");
            // At most a sixth brighter; never more than a whisker darker.
            if !(0.97..=1.17).contains(&ratio) {
                failures.push(format!("{name} with {spec}: {on:.1} against {off:.1}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn at_night_the_better_lighting_keeps_the_outdoors_as_dark_as_it_draws_it() {
    let _gpu = crate::common::gpu_lock();
    let station = station("night");
    let off = mean(&station, FidelityPreferences::default());
    let on = mean(
        &station,
        FidelityPreferences::parse_switch("Lighting=1").expect("a fidelity spec"),
    );
    eprintln!("night: mean {off:.1} -> {on:.1}");
    // Darker than the ordinary picture, as the night's light draws it, but no darker than that.
    assert!(on < off && on > off * 0.75, "{on:.1} against {off:.1}");
}

/// The woodsman's camp west of Holtburg in the afternoon of the offline clock's Rainy day: a
/// stand of the world's own snowy firs a few meters off, under the overcast sky, from where a
/// player saw them drawn black and blue with the bounced light on.
fn firs_in_the_rain() -> Station {
    Station {
        name: "firs-in-the-rain",
        why: "the world's own firs close in, under an overcast sky",
        place: Place::Look {
            block: 0xA9B5,
            x: 0.5,
            y: 131.0,
            eye: 1.8,
            at: [5.6, 134.3, 29.3],
        },
        clock: Clock::Rainy(0.615),
        weather: false,
        land_radius: 2,
    }
}

/// The mean brightness, 0..255, of the frame's middle (a third of its width and height in from
/// either side), and the share of it drawn black.
fn middle(rgba: &[u8]) -> (f32, f32) {
    let (w, h) = (W as usize, H as usize);
    let (mut sum, mut black, mut n) = (0.0f32, 0u32, 0u32);
    for y in h / 3..h * 2 / 3 {
        for x in w / 3..w * 2 / 3 {
            let i = (y * w + x) * 4;
            let l = luma(&[rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]);
            sum += l;
            black += u32::from(l < 12.0);
            n += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let (mean, share) = (sum / n as f32, black as f32 / n as f32);
    (mean, share)
}

#[test]
fn under_an_overcast_sky_the_bounced_light_leaves_the_worlds_trees_lit() {
    let _gpu = crate::common::gpu_lock();
    let station = firs_in_the_rain();
    let keep = |name: &str, rgba: &[u8]| {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hifi-outdoor");
        std::fs::create_dir_all(&dir).expect("the scratch folder");
        write_png(&dir.join(format!("{name}.png")), W, H, rgba);
    };
    let alone = frame(
        &station,
        FidelityPreferences::parse_switch("Lighting=1").expect("a fidelity spec"),
    );
    keep("firs-lighting", &alone);
    let (lit, lit_black) = middle(&alone);
    for spec in ["Lighting=1,GlobalIllumination=1", EVERY_BOX] {
        let on = frame(
            &station,
            FidelityPreferences::parse_switch(spec).expect("a fidelity spec"),
        );
        keep(&format!("firs-{}", spec.len()), &on);
        let (mean, black) = middle(&on);
        let said = format!(
            "{spec}: the firs' mean {mean:.1}, {:.1}% black; the lighting alone {lit:.1}, {:.1}% \
             black",
            black * 100.0,
            lit_black * 100.0
        );
        eprintln!("{said}");
        // The bounced light darkens a crown's inside, where little of the sky reaches, and the
        // shadows and the occlusion darken it more; none of them draws the leaves black.
        assert!(mean >= lit * 0.5 && black <= lit_black + 0.05, "{said}");
    }
}
