//! Behaviour: none (experimental Horizon rendering effects: how bright the better lighting draws
//! the outdoors against the ordinary picture)
//!
//! The better lighting outdoors, by day: at midday, in the low sun of the evening, among trees,
//! in the town street and on snow, with the lighting alone and with every box ticked, the frame is
//! about as bright as the ordinary picture -- a little brighter at most, never darker -- however
//! low the sun. At night it stays as dark as it draws it.
//!
//! Each frame is compared with the same station loaded again on a device of its own and stepped
//! the same number of frames with the presentation off.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, Shot, Station};

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
    let px = capture(gpu);
    let n = px.len() / 4;
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let sum: f32 = px
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
        .sum::<f32>()
        / n as f32;
    sum
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
