//! The high-fidelity lighting where it once looked wrong: at dusk a canopy the authored light
//! draws dark does not glow; with every box ticked the occlusion is still drawn indoors and
//! underground, where it is the only effect that draws; and at night the light that follows the
//! player pools no light on the ground round the body.
//!
//! Each frame is compared with the same station loaded again on a device of its own and stepped
//! the same number of frames with the presentation off.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, moved, Shot, Station};

const W: u32 = 960;
const H: u32 = 540;
/// Frames a station is stepped before it is captured, with the presentation and without.
const STEPS: usize = 40;
/// Every box, as the options page ticks them.
const EVERY_BOX: &str =
    "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Lamps=1,Sky=1";

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
    hifi_stations::isolation_stations(&store)
        .into_iter()
        .chain(hifi_stations::close_stations(&store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// `station` loaded with `prefs` and stepped [`STEPS`] frames on a device of its own: its last
/// frame.
fn run(station: &Station, prefs: FidelityPreferences) -> Vec<u8> {
    run_reshaded(station, prefs).0
}

/// [`run`], and whether its last frame was re-shaded.
fn run_reshaded(station: &Station, prefs: FidelityPreferences) -> (Vec<u8>, bool) {
    run_with(station, prefs, true)
}

/// [`run_reshaded`], with or without the light that follows the player.
fn run_with(station: &Station, prefs: FidelityPreferences, viewer_light: bool) -> (Vec<u8>, bool) {
    let mut device = device();
    let gpu = &mut device;
    let store = crate::common::dats();
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene.draw.set_viewer_light(viewer_light);
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&store, gpu)
        .expect("the preferences poll");
    // Every frame waits for the presentation's background work, so the frame is re-shaded
    // as soon as it can be, as it is a moment after a box is ticked.
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
    let reshaded = gpu
        .hifi_report()
        .is_some_and(|r| r.notes.iter().any(|(n, v)| *n == "re-shaded" && *v == 1));
    (capture(gpu), reshaded)
}

/// The mean brightness, 0..255, of the pixels of `rgba` in the rectangle of the frame from
/// `(x0, y0)` to `(x1, y1)`, each a fraction of the frame's width and height.
fn mean_luma(rgba: &[u8], (x0, y0): (f32, f32), (x1, y1): (f32, f32)) -> f32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    // LINT-OK: fractions of the frame's size, inside it
    let at = |f: f32, n: u32| (f * n as f32) as usize;
    let (w, h) = (W as usize, H as usize);
    let (mut sum, mut n) = (0.0f64, 0u32);
    for y in at(y0, H)..at(y1, H).min(h) {
        for x in at(x0, W)..at(x1, W).min(w) {
            let p = &rgba[(y * w + x) * 4..];
            sum += 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
            n += 1;
        }
    }
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a mean of bytes
    let mean = (sum / f64::from(n.max(1))) as f32;
    mean
}

/// The share of the frame, in percent, whose green stands well over its red and blue and is
/// not dark: a canopy lit up lime.
fn lime_share(rgba: &[u8]) -> f32 {
    let px = rgba.as_chunks::<4>().0;
    let lime = px
        .iter()
        .filter(|p| {
            let (r, g, b) = (f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
            g >= 64.0 && g > 1.35 * r.max(b)
        })
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = 100.0 * lime as f32 / px.len() as f32;
    share
}

/// Behaviour: hifi.lighting.at-dusk-a-dark-canopy-does-not-glow
/// At dusk in the glade, at the town's edge and on the dusk hillside, with the better lighting
/// alone and with every box ticked, no more than a few tenths of a percent more of the frame is
/// lit up lime than the authored light draws: the trees the dusk draws as dark shapes against
/// the sky stay dark.
#[test]
fn at_dusk_a_canopy_the_authored_light_draws_dark_does_not_glow() {
    let _gpu = crate::common::gpu_lock();
    for name in ["glade-1", "town-1", "dusk"] {
        let station = station(name);
        let off = lime_share(&run(&station, FidelityPreferences::default()));
        for spec in ["Lighting=1", EVERY_BOX] {
            let on = lime_share(&run(&station, fidelity(spec)));
            eprintln!("{name} {spec}: {on:.3}% lime against {off:.3}% off");
            assert!(
                on <= off + 0.3,
                "{name} with {spec}: {on:.3}% of the frame glows lime against {off:.3}% off"
            );
        }
    }
}

/// Behaviour: hifi.lighting.every-box-still-draws-the-occlusion-indoors
/// Underground, in a house's room and at a doorway with the camera in the room, every box ticked
/// changes the frame at least half as much as the ambient occlusion alone does: the bounced
/// light, which draws nothing there, does not switch the occlusion off, and a room drawn after
/// the frame steps indoors still takes it.
#[test]
fn with_every_box_ticked_the_occlusion_is_still_drawn_indoors_and_underground() {
    let _gpu = crate::common::gpu_lock();
    for name in ["dungeon", "indoor", "doorway"] {
        let station = station(name);
        let off = run(&station, FidelityPreferences::default());
        let ao = moved(&off, &run(&station, fidelity("AmbientOcclusion=1")));
        let (every_px, reshaded) = run_reshaded(&station, fidelity(EVERY_BOX));
        // Underground nothing is re-shaded; the room and the doorway are, up to their step.
        assert!(
            reshaded || name == "dungeon",
            "{name}: the frame with every box was not re-shaded"
        );
        let every = moved(&off, &every_px);
        eprintln!("{name}: occlusion alone {ao} bytes, every box {every} bytes");
        assert!(
            ao > 10_000,
            "{name}: the occlusion alone drew nothing: {ao}"
        );
        assert!(
            every * 2 >= ao,
            "{name}: every box changed {every} bytes against {ao} for the occlusion alone"
        );
    }
}

/// Behaviour: hifi.lighting.the-light-that-follows-the-player-pools-on-no-outdoor-ground
/// At midnight in the open in the town, with the better lighting, the ground round the body is
/// as bright with the light that follows the player as without it, as the ordinary frame draws
/// it: outdoors the landscape and everything on it are lit by the sun alone. And the body seen
/// from the front keeps at least nine tenths of the share of its ordinary brightness that the
/// ground keeps.
#[test]
fn at_night_in_the_open_the_light_that_follows_the_player_leaves_the_ground_as_drawn() {
    let _gpu = crate::common::gpu_lock();
    // The lower half of the chase camera's frame: the ground round the body's feet.
    let ground = |rgba: &[u8]| mean_luma(rgba, (0.0, 0.5), (1.0, 1.0));
    let square = station("square-night");
    let off = FidelityPreferences::default();
    let lit = fidelity("Lighting=1");
    let (plain, _) = run_with(&square, off, true);
    let (plain_without, _) = run_with(&square, off, false);
    let (relit, reshaded) = run_with(&square, lit, true);
    let (relit_without, _) = run_with(&square, lit, false);
    assert!(reshaded, "the night square was not re-shaded");
    let plain_moved = moved(&plain, &plain_without);
    let relit_moved = moved(&relit, &relit_without);
    let (with, without) = (ground(&relit), ground(&relit_without));
    eprintln!("square-night: the light moves {plain_moved} bytes plain, {relit_moved} relit");
    eprintln!(
        "square-night: the ground {with:.2} with it, {without:.2} without, {:.2} plain",
        ground(&plain)
    );
    assert_eq!(
        plain_moved, 0,
        "the ordinary frame outdoors changed with the light that follows the player"
    );
    assert!(
        with <= without * 1.01 + 0.5,
        "the light that follows the player brightened the ground: {with:.2} against {without:.2}"
    );
    // The body's face and chest, in the middle of the frame seen from the front, keeps as much
    // of its ordinary brightness as the ground round it does: the night's new light is darker
    // than the ordinary frame's everywhere, and the body is not darkened beyond it.
    let ground_share = with / ground(&plain).max(1.0);
    let body = |rgba: &[u8]| mean_luma(rgba, (0.45, 0.35), (0.55, 0.75));
    let face = station("face-night");
    let plain = body(&run(&face, off));
    let relit = body(&run(&face, lit));
    let body_share = relit / plain.max(1.0);
    eprintln!("face-night: the body {relit:.2} relit, {plain:.2} plain");
    eprintln!("the body keeps {body_share:.3} of its brightness, the ground {ground_share:.3}");
    assert!(
        body_share >= ground_share * 0.9,
        "the body kept {body_share:.3} of its brightness against the ground's {ground_share:.3}"
    );
}
