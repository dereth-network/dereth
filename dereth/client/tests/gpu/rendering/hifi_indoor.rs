//! Behaviour: none (experimental Horizon rendering effects: the better lighting underground, in
//! a room and at a doorway, as it looks)
//!
//! The better lighting indoors: underground, beside a dungeon's candles, in a house's room and at
//! a doorway seen from the room, it changes the frame, and the frame stays within a sane
//! brightness of the ordinary one, neither dark nor brighter, and nothing near a light blown out
//! to white; with the lamps' rays a room's light does not reach a surface
//! past the walls that stand between them; and a pause between two frames, as a slow machine
//! draws them, does not change what the eye adapts to.
//!
//! Each frame is compared with the same station loaded again on a device of its own and stepped
//! the same number of frames with the presentation off.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, moved, Clock, Place, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// Frames a station is stepped before it is captured, with the presentation and without.
const STEPS: usize = 12;

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
    let store = crate::common::dats();
    hifi_stations::stations(&store)
        .into_iter()
        .chain(hifi_stations::close_stations(&store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// A pause longer than the half second between two frames over which the eye keeps its
/// adaptation.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(750);

/// `station` loaded with `prefs` and stepped [`STEPS`] frames on a device of its own, waiting for
/// the presentation's background work after each: its last frame, and whether it was re-shaded.
fn run(station: &Station, prefs: FidelityPreferences) -> (Vec<u8>, bool) {
    run_paused(station, prefs, None)
}

/// [`run`], with `pause` before the last step: the station as a slow machine draws it.
fn run_paused(
    station: &Station,
    prefs: FidelityPreferences,
    pause: Option<std::time::Duration>,
) -> (Vec<u8>, bool) {
    let mut device = device();
    let gpu = &mut device;
    let store = crate::common::dats();
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&store, gpu)
        .expect("the preferences poll");
    for i in 0..STEPS {
        if let Some(pause) = pause.filter(|_| i + 1 == STEPS) {
            std::thread::sleep(pause);
        }
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
    let reshaded = gpu
        .hifi_report()
        .is_some_and(|r| r.notes.iter().any(|(n, v)| *n == "re-shaded" && *v == 1));
    (capture(gpu), reshaded)
}

/// Each pixel's brightness, 0..255.
fn luma(rgba: &[u8]) -> Vec<f32> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
        .collect()
}

/// The share of a picture's pixels with a channel drawn at full: clipped.
fn clipped(rgba: &[u8]) -> f32 {
    let px = rgba.as_chunks::<4>().0;
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = px.iter().filter(|p| p[..3].contains(&255)).count() as f32 / px.len() as f32;
    share
}

/// Every box, as the options page ticks them.
const EVERY_BOX: &str =
    "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1,Weather=1";

/// The mean brightness of a picture, and the shares of its pixels that are near black and that
/// are near white.
fn brightness(rgba: &[u8]) -> (f32, f32, f32) {
    let l = luma(rgba);
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let n = l.len() as f32;
    #[allow(clippy::cast_precision_loss)]
    let share = |f: &dyn Fn(f32) -> bool| l.iter().filter(|v| f(**v)).count() as f32 / n;
    (
        l.iter().sum::<f32>() / n,
        share(&|v| v < 8.0),
        share(&|v| v >= 248.0),
    )
}

/// The mean brightness of the pixels of `rgba` in the rectangle from `(x0, y0)` to `(x1, y1)`,
/// each a fraction of the frame's width and height.
fn mean_in(rgba: &[u8], (x0, y0): (f32, f32), (x1, y1): (f32, f32)) -> f32 {
    let l = luma(rgba);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    // LINT-OK: fractions of the frame's size, inside it
    let (xa, ya, xb, yb) = (
        (x0 * W as f32) as usize,
        (y0 * H as f32) as usize,
        (x1 * W as f32) as usize,
        (y1 * H as f32) as usize,
    );
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in ya..yb {
        for x in xa..xb {
            sum += l[y * W as usize + x];
            n += 1.0;
        }
    }
    sum / n
}

#[test]
fn underground_and_in_a_room_the_better_lighting_changes_the_frame_within_a_sane_brightness() {
    let _gpu = crate::common::gpu_lock();
    let mut failures = Vec::new();
    for name in [
        "dungeon",
        "dungeon-cave",
        "dungeon-candles",
        "indoor",
        "doorway",
    ] {
        let station = station(name);
        let (off, _) = run(&station, FidelityPreferences::default());
        for spec in ["Lighting=1", EVERY_BOX] {
            let (on, reshaded) = run(&station, fidelity(spec));
            failures.extend(sane(name, spec, &off, &on, reshaded));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// What is wrong with `on`, a frame of the station `name` with `spec` ticked, against `off`, the
/// ordinary frame: unchanged, or not within a sane brightness of it -- its mean brightness within
/// a fifth or a quarter of the ordinary frame's, no great share of it fallen to black, and almost
/// nothing more of it drawn white or clipped, however close a light stands.
fn sane(name: &str, spec: &str, off: &[u8], on: &[u8], reshaded: bool) -> Vec<String> {
    let mut failures = Vec::new();
    let name = format!("{name} with {spec}");
    {
        let changed = moved(off, on);
        let (off_mean, off_dark, off_blown) = brightness(off);
        let (on_mean, on_dark, on_blown) = brightness(on);
        let (off_clipped, on_clipped) = (clipped(off), clipped(on));
        eprintln!(
            "{name}: {changed} bytes changed; mean {off_mean:.1} -> {on_mean:.1}, near black \
             {:.1}% -> {:.1}%, near white {:.2}% -> {:.2}%, clipped {:.2}% -> {:.2}%",
            off_dark * 100.0,
            on_dark * 100.0,
            off_blown * 100.0,
            on_blown * 100.0,
            off_clipped * 100.0,
            on_clipped * 100.0
        );
        if !reshaded {
            failures.push(format!("{name}: the frame was not re-shaded"));
        }
        // A fifth of the frame's bytes at least: the rooms take the light.
        if changed * 5 < off.len() {
            failures.push(format!("{name}: only {changed} bytes changed"));
        }
        let ratio = on_mean / off_mean;
        if !(0.8..=1.25).contains(&ratio) {
            failures.push(format!(
                "{name}: mean brightness {on_mean:.1} against {off_mean:.1}"
            ));
        }
        if on_dark > off_dark + 0.10 {
            failures.push(format!(
                "{name}: {:.1}% near black against {:.1}%",
                on_dark * 100.0,
                off_dark * 100.0
            ));
        }
        if on_blown > off_blown + 0.001 {
            failures.push(format!(
                "{name}: {:.2}% near white against {:.2}%",
                on_blown * 100.0,
                off_blown * 100.0
            ));
        }
        if on_clipped > off_clipped + 0.003 {
            failures.push(format!(
                "{name}: {:.2}% clipped against {:.2}%",
                on_clipped * 100.0,
                off_clipped * 100.0
            ));
        }
    }
    failures
}

#[test]
fn with_the_lamps_rays_a_rooms_light_does_not_reach_past_what_shuts_it_out() {
    let _gpu = crate::common::gpu_lock();
    {
        let gpu = device();
        assert!(
            gpu.hifi_rays() == Some(true),
            "{} traces no rays; this test needs a device that does",
            gpu.adapter_name()
        );
    }
    // A body in a corner of the drudges' hideout, the camera close behind it: up to the right a
    // stretch of wall within the reach of the hall's lights, with the corner's own walls and
    // ceiling standing between them and it.
    let station = Station {
        name: "dungeon-corner",
        why: "a corner of a dungeon where its walls stand between the lights and the wall in view",
        place: Place::Body {
            cell: 0x019E_011B,
            origin: dereth_primitives::Vec3::ZERO,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 1,
    };
    let (untraced, _) = run(&station, fidelity("Lighting=1"));
    let (traced, reshaded) = run(&station, fidelity("Lighting=1,Lamps=1"));
    assert!(reshaded);
    if let Some(root) = std::env::var_os("DERETH_HIFI_CAPTURE_DIR") {
        let dir = std::path::Path::new(&root).join("indoor");
        hifi_stations::write_png(&dir.join("corner-untraced.png"), W, H, &untraced);
        hifi_stations::write_png(&dir.join("corner-traced.png"), W, H, &traced);
    }
    let behind = ((0.70, 0.0), (1.0, 0.25));
    let (open, shut) = (
        mean_in(&untraced, behind.0, behind.1),
        mean_in(&traced, behind.0, behind.1),
    );
    // The barrel and the floor on the left, which the lights reach in the open.
    let clear = ((0.0, 0.3), (0.4, 0.7));
    let (clear_open, clear_shut) = (
        mean_in(&untraced, clear.0, clear.1),
        mean_in(&traced, clear.0, clear.1),
    );
    eprintln!(
        "behind the wall {open:.1} without rays, {shut:.1} with; in the open {clear_open:.1} \
         without, {clear_shut:.1} with"
    );
    // The exposure follows the frame, so the open floor is the measure: the wall behind keeps
    // less than three quarters of the share of its light the open floor keeps.
    let kept_behind = shut / open;
    let kept_clear = clear_shut / clear_open;
    assert!(
        kept_behind < kept_clear * 0.75,
        "the wall behind kept {kept_behind:.2} of its light, the open floor {kept_clear:.2}"
    );
    assert!(
        (0.8..1.25).contains(&kept_clear),
        "the open floor kept {kept_clear:.2} of its light"
    );
}

#[test]
fn in_a_cave_the_bounced_light_adds_the_light_of_its_walls_and_floor() {
    let _gpu = crate::common::gpu_lock();
    // A cave passage of a deep dungeon, its rock lit by the passage's own lights: the lit rock
    // throws light onto the rock across from it.
    let station = station("dungeon-cave");
    let (lit, _) = run(&station, fidelity("Lighting=1"));
    let (bounced, reshaded) = run(&station, fidelity("Lighting=1,GlobalIllumination=1"));
    assert!(reshaded);
    let (a, b) = (luma(&lit), luma(&bounced));
    let brighter = a.iter().zip(&b).filter(|(x, y)| **y > **x + 2.0).count();
    let darker = a.iter().zip(&b).filter(|(x, y)| **y + 2.0 < **x).count();
    eprintln!("{brighter} pixels brighter with the bounce, {darker} darker");
    // One pixel in two hundred at least is visibly lit by the rock around it.
    assert!(
        brighter * 200 > a.len(),
        "only {brighter} pixels took the bounce"
    );
    assert!(
        darker * 4 < brighter,
        "{darker} pixels darker against {brighter} brighter"
    );
}

#[test]
fn in_a_cave_a_pause_between_frames_does_not_change_the_bounced_light() {
    let _gpu = crate::common::gpu_lock();
    // The eye adapts to the cave's bounced light as it gathers, over the scene's time: a frame
    // drawn after a pause, as a slow or busy machine draws it, is the frame drawn without one.
    let station = station("dungeon-cave");
    let prefs = fidelity("Lighting=1,GlobalIllumination=1");
    let (steady, _) = run(&station, prefs);
    let (paused, reshaded) = run_paused(&station, prefs, Some(PAUSE));
    assert!(reshaded);
    let changed = moved(&steady, &paused);
    assert_eq!(
        changed, 0,
        "a pause before the last frame changed {changed} bytes of it"
    );
}
