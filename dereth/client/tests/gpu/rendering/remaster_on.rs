//! The remaster drawing the world: at the light study's twelve stations against Retail Look; its
//! box turned on and off as the preference poll applies it, a few rounds of it, with Retail
//! Look's frames between unchanged; a window resized under it; refused off the Vulkan device and
//! on one started without it; and what each frame uploads at the farthest draw distance.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`), a hardware Vulkan 1.3 device, and for the
//! refusals the `wgpu` and Direct3D 12 devices.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_contract::options::remaster;
use dereth_client_runtime::frame_events::RenderPrefWork;
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::remaster_frames::{self, compare, run, Draw, STATIONS};
use crate::instruments::stations::{self, capture, counted_step, moved, Shot};

/// The light study's per-pixel numbers at each station, in display levels and percent: outdoors
/// its model from the frame's light inputs alone, and indoors its model from each draw's own
/// inputs, as the remaster has the scene's own light sets there. The study never saw the indoor
/// station with its body in the middle of the room, so that bar is the remaster's own there.
const BARS: [(&str, f64, f64); 12] = [
    ("holtburg", 0.64, 0.7),
    ("forest", 0.72, 1.6),
    ("dusk", 0.68, 0.1),
    ("rain", 0.43, 0.4),
    ("rain-forest", 0.44, 1.2),
    ("night", 0.14, 0.0),
    ("lamps-street", 0.31, 0.0),
    ("snow-village", 0.63, 0.4),
    ("doorway", 2.59, 10.7),
    ("indoor", 5.38, 21.8),
    ("dungeon", 3.66, 24.6),
    ("dungeon-candles", 3.03, 16.2),
];

/// With every pass past retail's own light off, the remaster's numbers at each station: per
/// vertex, then per pixel, the mean in display levels and the share of pixels over 8 levels in
/// percent. They change only when the frame drawn with the passes off does. Measured on Windows:
/// other drivers round differently and hold only the bars.
const PASSES_OFF: [(&str, [f64; 4]); 12] = [
    ("holtburg", [0.25, 0.0, 0.45, 0.6]),
    ("forest", [0.39, 0.6, 0.69, 1.0]),
    ("dusk", [0.08, 0.0, 0.17, 0.0]),
    ("rain", [0.20, 0.0, 0.34, 0.3]),
    ("rain-forest", [0.38, 0.4, 0.47, 0.5]),
    ("night", [0.05, 0.0, 0.10, 0.0]),
    ("lamps-street", [0.08, 0.0, 0.17, 0.0]),
    ("snow-village", [0.09, 0.0, 0.81, 0.3]),
    ("doorway", [0.21, 0.7, 2.75, 10.5]),
    ("indoor", [0.16, 0.1, 5.38, 21.8]),
    ("dungeon", [0.08, 0.0, 3.69, 22.9]),
    ("dungeon-candles", [0.08, 0.0, 2.91, 14.1]),
];

/// [`PASSES_OFF`] where Direct3D 12 rounds them differently, per vertex, mean only.
const PASSES_OFF_D3D12: [(&str, f64); 3] = [("holtburg", 0.26), ("dusk", 0.07), ("doorway", 0.20)];

/// At every station, against Retail Look on the lit pixels: the remaster with retail's light per
/// vertex is within half a display level on average with at most 1% of pixels over 8 levels;
/// with it per pixel, within the study's own per-pixel numbers plus half a level and plus one
/// point; and either way the world covers the same pixels but for under 0.5% of the frame, and
/// no pipeline is built once the remaster has started. Every pass past retail's own light is off,
/// and the numbers are those it draws so, to the hundredth of a level and the tenth of a point.
#[test]
fn every_station_matches_retail_look_within_its_bars() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let stations = remaster_frames::stations(&store, &STATIONS);
    assert_eq!(stations.len(), STATIONS.len());
    let size = remaster_frames::size();
    let mut moved = Vec::new();
    for station in &stations {
        let (_, mean_bar, over_bar) = BARS
            .iter()
            .copied()
            .find(|b| b.0 == station.name)
            .expect("every station has its bar");
        let (_, pinned) = PASSES_OFF
            .iter()
            .copied()
            .find(|p| p.0 == station.name)
            .expect("every station has its numbers");
        let d3d12_mean = PASSES_OFF_D3D12
            .iter()
            .find(|p| crate::common::remaster_backend() == Backend::D3d12 && p.0 == station.name)
            .map(|p| p.1);
        let retail = run(&store, station, Draw::Retail, size);
        for (k, draw) in [Draw::Vertex, Draw::Pixel].into_iter().enumerate() {
            let frame = run(&store, station, draw, size);
            let n = compare(&retail, &frame);
            let what = format!("{} {draw:?}", station.name);
            eprintln!(
                "{what}: mean {:.2}, over 8 {:.1}%, silhouettes {:.2}%, {} lit",
                n.mean, n.over8, n.silhouette, n.lit
            );
            let measured = (format!("{:.2}", n.mean), format!("{:.1}", n.over8));
            let held = (
                format!(
                    "{:.2}",
                    d3d12_mean.filter(|_| k == 0).unwrap_or(pinned[k * 2])
                ),
                format!("{:.1}", pinned[k * 2 + 1]),
            );
            if cfg!(windows) && measured != held {
                moved.push(format!("{what}: {measured:?} against {held:?}"));
            }
            assert!(n.lit > 10_000, "{what}: lit pixels compared");
            match draw {
                Draw::Vertex => {
                    assert!(n.mean <= 0.5, "{what}: mean {:.2}", n.mean);
                    assert!(n.over8 <= 1.0, "{what}: over 8 {:.1}%", n.over8);
                }
                _ => {
                    assert!(n.mean <= mean_bar + 0.5, "{what}: mean {:.2}", n.mean);
                    assert!(n.over8 <= over_bar + 1.0, "{what}: over 8 {:.1}%", n.over8);
                }
            }
            assert!(
                n.silhouette < 0.5,
                "{what}: silhouettes {:.2}%",
                n.silhouette
            );
            let counts = frame.counts.expect("the remaster counted");
            assert_eq!(
                counts.pipelines_built, frame.pipelines_at_start,
                "{what}: pipelines built after start-up"
            );
            assert!(counts.frames >= 6, "{what}: the remaster drew every frame");
        }
    }
    assert!(
        moved.is_empty(),
        "the numbers with every pass off moved:
{}",
        moved.join(
            "
"
        )
    );
}

/// A body the chase camera has closed in on is faded out, wholly: the remaster draws what Retail
/// Look does, with it gone, and the view past it is the same.
#[test]
fn a_body_the_camera_has_closed_in_on_is_gone_as_in_retail_look() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = stations::Station {
        name: "camera in the body",
        why: "a body against a wall of a room, its chase camera pressed into it",
        place: stations::Place::Body {
            cell: stations::INDOOR_CELL,
            origin: dereth_primitives::Vec3::new(138.0, 14.0, 94.5),
            yaw: 0.0,
        },
        clock: stations::Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    };
    let size = remaster_frames::size();
    let retail = run(&store, &station, Draw::Retail, size);
    let frame = run(&store, &station, Draw::Vertex, size);
    let n = compare(&retail, &frame);
    assert!(n.lit > 10_000, "lit pixels compared: {}", n.lit);
    assert!(n.mean <= 0.5, "mean {:.2}", n.mean);
    assert!(n.over8 <= 1.0, "over 8 {:.1}%", n.over8);
}

/// With the particles drawn, as they are with the weather, at the street under the lamps by
/// night: the remaster draws the lamps' flames among the rest, and still matches Retail Look
/// within the bars of its light per vertex.
#[test]
fn the_particles_are_drawn_among_the_rest_as_retail_look_draws_them() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut station = remaster_frames::stations(&store, &["lamps-street"])
        .pop()
        .expect("the street");
    station.weather = true;
    let size = remaster_frames::size();
    let retail = run(&store, &station, Draw::Retail, size);
    let frame = run(&store, &station, Draw::Vertex, size);
    let n = compare(&retail, &frame);
    let counts = frame.counts.expect("the remaster counted");
    eprintln!(
        "mean {:.2}, over 8 {:.1}%, silhouettes {:.2}%; {} particle items",
        n.mean, n.over8, n.silhouette, counts.particle_items
    );
    assert!(counts.particle_items >= 8, "the lamps' flames are drawn");
    assert!(n.mean <= 0.5, "mean {:.2}", n.mean);
    assert!(n.over8 <= 1.0, "over 8 {:.1}%", n.over8);
    assert!(n.silhouette < 0.5, "silhouettes {:.2}%", n.silhouette);
}

/// `[Fidelity]` with the remaster's box ticked or not, under Horizon.
fn boxed(on: bool) -> FidelityPreferences {
    FidelityPreferences::parse_switch(if on { "Remaster=1" } else { "off" })
        .expect("a fidelity spec")
}

/// Set the shot's `[Fidelity]` preferences and poll them, as the application does, and what the
/// poll did.
fn poll_with(shot: &mut Shot, gpu: &mut Gpu, prefs: FidelityPreferences) -> RenderPrefWork {
    shot.scene.draw.cfg.render.fidelity = prefs;
    shot.scene
        .update_from_preferences(&crate::common::dats(), gpu)
        .expect("the preferences poll")
}

/// A device on `backend` with the validation layer where it is installed, asked for the
/// remaster's features when `remaster`.
fn device(backend: Backend, remaster: bool) -> Gpu {
    let cfg = DeviceConfig {
        width: 640,
        height: 480,
        remaster,
        debug: backend == Backend::Vulkan,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(backend, None, &cfg)
        .unwrap_or_else(|e| panic!("a {} device: {e}", backend.name()));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    gpu
}

/// The town by day drawn a frame for each of `boxes`, the box set before each as it says (frame 1 always
/// by Retail Look, as both runs first draw it), and the last frame's pixels, digest and counters
/// with how many frames the remaster drew.
fn toggled<const N: usize>(
    store: &Arc<RetailDatStore>,
    boxes: [bool; N],
) -> (Vec<u8>, Option<u64>, String, u64) {
    let station = remaster_frames::stations(store, &["holtburg"])
        .pop()
        .expect("the town");
    let mut gpu = device(crate::common::remaster_backend(), true);
    gpu.set_frame_digest(true);
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    let mut drew = 0;
    let mut counted = String::new();
    for (k, on) in boxes.into_iter().enumerate() {
        let what = format!("frame {}", k + 1);
        let work = poll_with(&mut shot, &mut gpu, boxed(on));
        assert_eq!(work.remaster_refused, None, "{what}");
        assert_eq!(shot.scene.draw.remaster_on(), on, "{what}");
        let before = shot
            .scene
            .draw
            .remaster_counts(&mut gpu)
            .map_or(0, |c| c.frames);
        if k + 1 == boxes.len() {
            counted = counted_step(&mut shot, &mut gpu);
        } else {
            shot.step(&mut gpu);
        }
        if let Some(c) = shot.scene.draw.remaster_counts(&mut gpu) {
            assert_eq!(c.frames, before + 1, "{what}: the remaster drew it");
            drew += 1;
        }
    }
    let pixels = capture(&mut gpu);
    let digest = gpu.last_frame_digest();
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (pixels, digest, counted, drew)
}

/// The box turned on, off, on and off again between frames drawn by Retail Look: the last
/// frame Retail Look draws is the frame it draws with the box never ticked, byte for byte, with
/// the same submitted draws and counters, and the remaster drew the frames the box was on for.
#[test]
fn turning_the_box_on_off_and_on_leaves_retail_looks_frame_unchanged() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (base, base_digest, base_counted, none) = toggled(&store, [false; 6]);
    assert_eq!(none, 0, "the remaster never drew");
    // The Vulkan device digests what it submits; the Direct3D 12 one digests nothing.
    assert_eq!(
        base_digest.is_some(),
        crate::common::remaster_backend() == Backend::Vulkan,
        "the digest"
    );
    let (pixels, digest, counted, drew) = toggled(&store, [false, true, true, false, true, false]);
    let changed = moved(&pixels, &base);
    eprintln!("{changed} bytes differ; the remaster drew {drew} frames");
    assert_eq!(changed, 0, "the pixels");
    assert_eq!(digest, base_digest, "the submitted frame");
    assert_eq!(counted, base_counted, "the counters");
    assert_eq!(drew, 3, "the remaster drew the frames its box was on for");
}

/// Four rounds of the box ticked and unticked, the remaster standing up and going away each
/// time, then a last frame by Retail Look: that frame is the frame of a run whose box was never
/// ticked, loaded and stepped the same way, byte for byte, with the same submitted draws and
/// counters.
///
/// Behaviour: remaster.toggle.retail-look-is-byte-identical-after-rounds-of-the-box
#[test]
fn four_rounds_of_the_box_leave_retail_looks_frame_byte_for_byte_as_never_ticked() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (base, base_digest, base_counted, none) = toggled(&store, [false; 10]);
    assert_eq!(none, 0, "the remaster never drew");
    let rounds = [
        false, true, false, true, false, true, false, true, false, false,
    ];
    let (pixels, digest, counted, drew) = toggled(&store, rounds);
    let changed = moved(&pixels, &base);
    eprintln!("{changed} bytes differ; the remaster drew {drew} frames");
    assert_eq!(changed, 0, "the pixels");
    assert_eq!(digest, base_digest, "the submitted frame");
    assert_eq!(counted, base_counted, "the counters");
    assert_eq!(drew, 4, "the remaster drew the frames its box was on for");
}

/// The box ticked on the `wgpu` device, and on a Vulkan device started without it, is refused:
/// the remaster stays off, the poll says why once, and Retail Look keeps drawing.
#[test]
fn the_box_is_refused_on_wgpu_and_on_a_vulkan_device_started_without_it() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = stations::Station {
        name: "still",
        why: "the ground under the town, looked straight down at",
        place: stations::Place::Free {
            block: stations::HOLTBURG,
            x: 96.0,
            y: 96.0,
            eye: 30.0,
            yaw: 0.0,
            pitch: -1.2,
        },
        clock: stations::Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    };
    for (backend, requested, line) in [
        (Backend::Wgpu, false, remaster::REFUSED_RENDERER),
        (Backend::Vulkan, false, remaster::REFUSED_UNTIL_RESTART),
    ]
    .into_iter()
    .filter(|b| b.0 != Backend::D3d12 || cfg!(windows))
    {
        let what = format!("{} asked {requested}", backend.name());
        let mut gpu = device(backend, requested);
        let mut shot = Shot::open(&store, &mut gpu, &station);
        shot.step(&mut gpu);
        let work = poll_with(&mut shot, &mut gpu, boxed(true));
        assert_eq!(work.remaster_refused, Some(line), "{what}");
        assert!(!shot.scene.draw.remaster_on(), "{what}");
        let work = poll_with(&mut shot, &mut gpu, boxed(true));
        assert_eq!(work.remaster_refused, None, "{what}: said once");
        let calls = gpu.draw_calls();
        shot.step(&mut gpu);
        assert!(gpu.draw_calls() > calls, "{what}: Retail Look draws");
        drop(shot);
        drop(gpu);
    }
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// When the device lets go of the remaster after a failure, the poll says so once, the remaster is
/// off, and Retail Look keeps drawing; the box stays as it was and isn't tried again.
#[test]
fn when_the_device_lets_go_of_the_remaster_the_player_is_told_and_retail_look_draws() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let mut gpu = device(Backend::Vulkan, true);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.poll(&mut gpu);
    assert_eq!(
        poll_with(&mut shot, &mut gpu, boxed(true)).remaster_refused,
        None
    );
    assert!(shot.scene.draw.remaster_on());
    shot.step(&mut gpu);

    drop(gpu.take_world_hook());
    assert_eq!(
        poll_with(&mut shot, &mut gpu, boxed(true)).remaster_refused,
        Some(remaster::FAILED)
    );
    assert!(!shot.scene.draw.remaster_on());
    assert_eq!(
        poll_with(&mut shot, &mut gpu, boxed(true)).remaster_refused,
        None,
        "said once, and not tried again"
    );
    assert!(!shot.scene.draw.remaster_on());
    let calls = gpu.draw_calls();
    shot.step(&mut gpu);
    assert!(gpu.draw_calls() > calls, "Retail Look draws");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// A scene made on a device where an earlier scene left its remaster running, as entering the
/// world does, starts its own with the box ticked: nothing is refused and the new scene's remaster
/// draws its frames, with no error from the validation layer.
#[test]
fn a_new_scene_takes_over_from_the_remaster_an_earlier_scene_left() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let mut gpu = device(Backend::Vulkan, true);
    for what in ["the first scene", "the next scene"] {
        let mut shot = Shot::open(&store, &mut gpu, &station);
        shot.poll(&mut gpu);
        let work = poll_with(&mut shot, &mut gpu, boxed(true));
        assert_eq!(work.remaster_refused, None, "{what}");
        assert!(shot.scene.draw.remaster_on(), "{what}");
        shot.step(&mut gpu);
        let counts = shot
            .scene
            .draw
            .remaster_counts(&mut gpu)
            .expect("the remaster counted");
        assert_eq!(counts.frames, 1, "{what}: its own remaster drew the frame");
        drop(shot);
    }
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// The box ticked and unticked four times over, a few frames each way: every time it is ticked
/// the remaster stands up and draws, every time it is unticked it is gone from the device and
/// Retail Look draws, and the last Retail Look frame is the one drawn before any tick, byte for
/// byte, with no error from the validation layer.
///
/// Behaviour: remaster.toggle.the-remaster-stands-up-and-goes-away-in-the-running-session
#[test]
fn ticking_the_remaster_on_and_off_four_times_stands_it_up_and_takes_it_down_each_time() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = stations::Station {
        name: "still",
        why: "the ground under the town, looked straight down at",
        place: stations::Place::Free {
            block: stations::HOLTBURG,
            x: 96.0,
            y: 96.0,
            eye: 30.0,
            yaw: 0.0,
            pitch: -1.2,
        },
        clock: stations::Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    };
    let mut gpu = device(crate::common::remaster_backend(), true);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.step(&mut gpu);
    let before = capture(&mut gpu);
    for round in 1..=4 {
        let work = poll_with(&mut shot, &mut gpu, boxed(true));
        assert_eq!(work.remaster_refused, None, "round {round}");
        assert!(shot.scene.draw.remaster_on(), "round {round}");
        for _ in 0..3 {
            shot.step(&mut gpu);
        }
        let counts = shot
            .scene
            .draw
            .remaster_counts(&mut gpu)
            .expect("the remaster stood up");
        assert_eq!(
            counts.frames, 3,
            "round {round}: a fresh remaster drew each frame"
        );
        let work = poll_with(&mut shot, &mut gpu, boxed(false));
        assert_eq!(work.remaster_refused, None, "round {round}");
        assert!(!shot.scene.draw.remaster_on(), "round {round}");
        assert!(
            shot.scene.draw.remaster_counts(&mut gpu).is_none(),
            "round {round}: it is gone from the device"
        );
        let calls = gpu.draw_calls();
        shot.step(&mut gpu);
        assert!(gpu.draw_calls() > calls, "round {round}: Retail Look draws");
    }
    let after = capture(&mut gpu);
    assert_eq!(
        moved(&after, &before),
        0,
        "Retail Look's frame after four rounds"
    );
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// A window resized while the remaster draws into it: on the Vulkan device and, on Windows, the
/// Direct3D 12 one, each size takes and the remaster draws every frame at it, with no error.
///
/// Behaviour: remaster.toggle.the-window-resizes-while-the-remaster-draws
#[test]
#[cfg(windows)]
fn the_window_resizes_while_the_remaster_draws_on_vulkan_and_direct3d_12() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let handles = crate::common::invisible_window();
    let backends: &[Backend] = if cfg!(windows) {
        &[Backend::Vulkan, Backend::D3d12]
    } else {
        &[Backend::Vulkan]
    };
    // One device at a time: a window holds one swap chain.
    for &backend in backends {
        let what = backend.name();
        let cfg = DeviceConfig {
            width: 800,
            height: 600,
            remaster: true,
            debug: backend == Backend::Vulkan,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new_on(backend, Some(handles), &cfg)
            .unwrap_or_else(|e| panic!("a {what} device on the window: {e}"));
        let mut shot = Shot::open(&store, &mut gpu, &station);
        shot.poll(&mut gpu);
        let work = poll_with(&mut shot, &mut gpu, boxed(true));
        assert_eq!(work.remaster_refused, None, "{what}");
        shot.step(&mut gpu);
        let mut drawn = 1;
        for (w, h) in [(1024, 640), (640, 480), (1280, 720), (801, 599)] {
            gpu.resize(w, h)
                .unwrap_or_else(|e| panic!("{what}: resizing to {w}x{h}: {e}"));
            assert_eq!(gpu.size(), (w, h), "{what}");
            for _ in 0..2 {
                shot.step(&mut gpu);
                drawn += 1;
            }
            let counts = shot
                .scene
                .draw
                .remaster_counts(&mut gpu)
                .unwrap_or_else(|| panic!("{what}: the remaster is still on at {w}x{h}"));
            assert_eq!(
                counts.frames, drawn,
                "{what}: it drew every frame at {w}x{h}"
            );
        }
        drop(shot);
        drop(gpu);
    }
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// The most a frame uploads to the device at the farthest draw distance, bytes: the frame's
/// constants and lights, every held block's place, the visible items and their commands, and
/// the rows of the parts that moved or are lit anew. The town by day sends about 95,000.
const FRAME_UPLOAD_LIMIT: u64 = 128 * 1024;

/// At the town by day with the landscape drawn 25 blocks out every way, each frame after the
/// first uploads under [`FRAME_UPLOAD_LIMIT`]: the scene is kept on the device, and a frame
/// sends only its own state.
/// A station by its name, its weather as the study's stations leave it unless `weather`.
fn box_station(store: &Arc<RetailDatStore>, place: &str, weather: bool) -> stations::Station {
    let mut station = stations::stations(store)
        .into_iter()
        .chain(stations::close_stations(store))
        .find(|s| s.name == place)
        .expect("the place");
    station.weather = weather;
    station
}

/// A station drawn with the remaster's box ticked and `boxes` poll by poll on a device of its own,
/// the frame after each poll, so that two runs compare frame for frame. `pixel` sets the remaster's
/// light per pixel directly, with no box asking for it.
fn boxes_run<const N: usize>(
    store: &Arc<RetailDatStore>,
    station: &stations::Station,
    pixel: bool,
    boxes: [&str; N],
) -> [Vec<u8>; N] {
    let mut gpu = device(Backend::Vulkan, true);
    let mut shot = Shot::open(store, &mut gpu, station);
    boxes.map(|spec| {
        let work = poll_with(
            &mut shot,
            &mut gpu,
            FidelityPreferences::parse_switch(&format!("Remaster=1,{spec}")).expect("a spec"),
        );
        assert_eq!(work.remaster_refused, None, "{} {spec}", station.name);
        if pixel {
            shot.scene
                .draw
                .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
        }
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        for _ in 0..12 {
            shot.step_held(&mut gpu);
        }
        capture(&mut gpu)
    })
}

/// How far apart two frames are, in levels a channel.
fn apart(a: &[u8], b: &[u8]) -> f64 {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = total as f64 / a.len() as f64;
    mean
}

/// Each of the remaster's own boxes changes what is drawn where it shows, live; and off again
/// the frame comes back to the one drawn with it never ticked. A box worked out per pixel brings
/// the light per pixel with it, and draws something of its own over that.
#[test]
fn each_of_the_remasters_own_boxes_changes_the_frame_and_off_puts_it_back() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for (place, weather, per_pixel, spec) in [
        ("holtburg", false, true, "RemasterShadows"),
        ("holtburg", false, true, "RemasterSky"),
        ("holtburg", false, true, "RemasterFog"),
        ("holtburg", false, true, "RemasterOcclusion"),
        ("holtburg", false, false, "RemasterBloom"),
        ("holtburg", false, false, "RemasterSmoothing"),
        ("holtburg", false, false, "RemasterDetail"),
        ("indoor", false, true, "RemasterIndoorLamps"),
        ("lamps-street", false, true, "RemasterOutdoorLamps"),
        ("coast", false, false, "RemasterWater"),
        ("rain-street", true, false, "RemasterPuddles"),
        ("rain-street", true, false, "RemasterRain"),
        ("doorway", false, true, "RemasterBounce"),
        ("holtburg", false, false, "RemasterUpscale"),
        ("holtburg", false, true, "RemasterTracedShadows"),
        ("holtburg", false, true, "RemasterTracedOcclusion"),
    ] {
        // Over Metal no ray is traced: the bounced light comes from the screen, which moves the
        // doorway's picture too little to measure, and the dungeon's plainly.
        let place = if spec == "RemasterBounce" && cfg!(target_os = "macos") {
            "dungeon"
        } else {
            place
        };
        let station = box_station(&store, place, weather);
        let never = boxes_run(&store, &station, false, ["", "", ""]);
        let on = format!("{spec}=1");
        let ticked = boxes_run(&store, &station, false, ["", &on, ""]);
        let base = if per_pixel {
            boxes_run(&store, &station, true, ["", ""])[1].clone()
        } else {
            never[1].clone()
        };
        let changed = apart(&base, &ticked[1]);
        let back = apart(&never[2], &ticked[2]);
        assert!(changed > 0.002, "{place} {spec}: moved {changed:.3}");
        assert!(back < 0.001, "{place} {spec}: off again {back:.3}");
    }
}

/// Each of the landscape's additions, over every ordinary box but those that gather over frames
/// (the smoothing, the occlusion and the bounced light, whose histories carry a frame into the next
/// by design), changes the frame where it shows; off again, the frame is the one drawn with it never
/// ticked, to the pixel.
#[test]
fn each_landscape_addition_changes_the_frame_and_off_puts_it_back_to_the_pixel() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let every = crate::instruments::remaster_look_frames::EVERY_BOX
        .replace("RemasterSmoothing=1,", "")
        .replace("RemasterOcclusion=1,", "")
        .replace(",RemasterBounce=1", "");
    let every = every.as_str();
    for (place, spec) in [
        ("ridge", "RemasterGrade"),
        ("vista", "RemasterAir"),
        ("ridge", "RemasterGrass"),
    ] {
        let station = box_station(&store, place, false);
        let on = format!("{every},{spec}=1");
        let never = boxes_run(&store, &station, false, [every, every, every]);
        let ticked = boxes_run(&store, &station, false, [every, &on, every]);
        let changed = apart(&never[1], &ticked[1]);
        let back = apart(&never[2], &ticked[2]);
        assert!(changed > 0.05, "{place} {spec}: moved {changed:.3}");
        assert!(back == 0.0, "{place} {spec}: off again {back:.4}");
    }
}

#[test]
fn each_frame_uploads_under_its_limit_at_the_farthest_draw_distance() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    station.land_radius = 25;
    let mut gpu = device(crate::common::remaster_backend(), true);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.step(&mut gpu);
    let first = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .expect("the remaster counted");
    const FRAMES: u64 = 10;
    for _ in 0..FRAMES {
        shot.step(&mut gpu);
    }
    let last = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .expect("the remaster counted");
    let per_frame = (last.frame_bytes - first.frame_bytes) / FRAMES;
    eprintln!(
        "{} blocks, {} items: {per_frame} bytes a frame; the scene {} bytes",
        last.blocks, last.items, last.scene_bytes
    );
    assert_eq!(last.frames - first.frames, FRAMES, "every frame drawn");
    assert!(
        last.blocks > 2_000,
        "the window's blocks are held: {}",
        last.blocks
    );
    assert!(per_frame < FRAME_UPLOAD_LIMIT, "{per_frame} bytes a frame");
    assert_eq!(
        last.scene_bytes, first.scene_bytes,
        "nothing of the scene is sent again while the view holds still"
    );
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
