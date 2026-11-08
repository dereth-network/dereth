//! The remaster's water at the views its reflections and its wading are judged from: a lake with
//! the camera tilted up toward the sky, level, and down until no sky shows; the same lake from far
//! off; puddles at the viewer's feet on a rainy day; a body standing in a pond, seen from above and
//! from beside it; and a strip of frames as the body walks through the pond. Each is written as a
//! picture, with the remaster's time on the device and the water pass's share of it printed.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_WATER_SCENES`: a comma list of the views (default every one of [`SCENES`]).
//! - `DERETH_REMASTER_LOOK_BOXES`: the boxes, as `--fidelity` takes them (default every box that
//!   draws something of its own).
//! - `DERETH_REMASTER_LOOK_TAG`: a word in the pictures' names, to keep runs apart.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_ART_PACK`: the art pack the remaster's pack boxes draw (default none).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::remaster_frames;
use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, report_station, Shot, Station};

/// A lake under hills at noon, the camera on its bank looking across it.
const LAKE: &str = "dereth report 1; loc 0xA9B50011 [64.387108 22.052589 38.978706] 0.920786 \
                    0.000000 0.000000 -0.390069; camera 0xA9B5 [60.244957 18.04098 42.136875] \
                    yaw -0.8014121 pitch -0.27999994 cell 0xA9B50011; time 0.5; clock \
                    306486600.0; weather auto";

/// A pond with lily pads, the camera over a body standing in it, at noon.
const POND: &str = "dereth report 1; loc 0xA8B60012 [55.615215 30.300829 27.554998] 0.069254 \
                    0.000000 0.000000 -0.997599; camera 0xA8B6 [54.51643 35.022827 32.58985] \
                    yaw -2.9129736 pitch -0.6300002 cell 0xA8B60012; time 0.5; clock \
                    306486600.0; weather auto";

/// Every view: its name, then how it is drawn.
const SCENES: [&str; 10] = [
    "lake-up",
    "lake-level",
    "lake-down",
    "lake-far",
    "puddle-feet",
    "puddle-ahead",
    "wade-above",
    "wade-beside",
    "wade-up",
    "wade-walk",
];

/// Where a view is drawn from.
enum View {
    /// A report's place, its camera moved `back` metres back along its view, `up` metres up and
    /// pitched `pitch` radians more.
    Report {
        line: &'static str,
        back: f32,
        up: f32,
        pitch: f32,
    },
    /// A station by name, its free camera carried `ahead` metres along its heading and pitched
    /// `pitch` radians more.
    Station {
        name: &'static str,
        ahead: f32,
        pitch: f32,
    },
}

fn view(name: &str) -> View {
    let report = |line, back, up, pitch| View::Report {
        line,
        back,
        up,
        pitch,
    };
    match name {
        "lake-up" => report(LAKE, 0.0, -1.5, 0.42),
        "lake-level" => report(LAKE, 0.0, 0.0, 0.0),
        "lake-down" => report(LAKE, 0.0, 0.0, -0.15),
        "lake-far" => report(LAKE, 140.0, 25.0, 0.05),
        "puddle-feet" => View::Station {
            name: "rain",
            ahead: 92.0,
            pitch: -0.6,
        },
        "puddle-ahead" => View::Station {
            name: "rain",
            ahead: 80.0,
            pitch: -0.12,
        },
        "wade-beside" => report(POND, -1.5, -3.6, 0.45),
        "wade-up" => report(POND, 0.0, -3.0, 0.75),
        _ => report(POND, 0.0, 0.0, 0.0),
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The station and report a view is drawn from.
fn place(store: &Arc<RetailDatStore>, v: &View) -> (Station, Option<VisualReport>) {
    match v {
        View::Report {
            line,
            back,
            up,
            pitch,
        } => {
            let mut report = VisualReport::parse(line).expect("the view's report reads");
            if let Some(c) = report.camera.as_mut() {
                let (s, co) = (
                    dereth_primitives::num::math::sinf(c.yaw),
                    dereth_primitives::num::math::cosf(c.yaw),
                );
                c.origin.x += s * back;
                c.origin.y -= co * back;
                c.origin.z += up;
                c.pitch += pitch;
                if *back != 0.0 || *up != 0.0 {
                    c.cell = None;
                }
            }
            (report_station(&report, Some(8)), Some(report))
        }
        View::Station { name, ahead, pitch } => {
            let mut st = stations::stations(store)
                .into_iter()
                .find(|s| s.name == *name)
                .unwrap_or_else(|| panic!("no station {name}"));
            if let stations::Place::Free {
                x,
                y,
                yaw,
                pitch: p,
                ..
            } = &mut st.place
            {
                *x -= dereth_primitives::num::math::sinf(*yaw) * ahead;
                *y += dereth_primitives::num::math::cosf(*yaw) * ahead;
                *p += pitch;
            }
            (st, None)
        }
    }
}

/// The remaster's time on the device a frame, and the water's share (its pass and the sky all
/// round the eye it reads), over a dozen frames held still after the last.
fn timings(shot: &mut Shot, gpu: &mut Gpu) -> String {
    let (mut total, mut water) = (0.0, 0.0);
    let n = 12;
    for _ in 0..n {
        shot.step_held(gpu);
        // The frame's times are read back with it.
        let _ = capture(gpu);
        let t = shot.scene.draw.remaster_timings(gpu);
        total += t.iter().map(|p| p.1).sum::<f64>();
        water += t
            .iter()
            .filter(|p| p.0.contains("water") || p.0.contains("sky probe"))
            .map(|p| p.1)
            .sum::<f64>();
    }
    let n = f64::from(n);
    format!(
        "{:.2} ms on the device, water {:.3} ms (means of a dozen frames)",
        total / n,
        water / n
    )
}

/// Each view drawn with the boxes, its pictures written under `DERETH_REMASTER_CAPTURE_DIR`.
#[test]
#[ignore = "instrument: writes the remaster's water at its judged views; run with --ignored"]
fn capture_the_remasters_water() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names: Vec<String> = env("DERETH_WATER_SCENES").map_or_else(
        || SCENES.iter().map(|s| (*s).to_owned()).collect(),
        |v| v.split(',').map(|s| s.trim().to_owned()).collect(),
    );
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = remaster_frames::size();
    let backend = crate::common::remaster_backend();
    for name in &names {
        let at = place(&store, &view(name));
        let cfg = DeviceConfig {
            width: size.0,
            height: size.1,
            remaster: true,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new_on(backend, None, &cfg).expect("a device");
        let mut shot = Shot::open(&store, &mut gpu, &at.0);
        if let Some(r) = at.1.as_ref() {
            shot.apply_report(r);
        }
        shot.poll(&mut gpu);
        if let Some(path) = env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
            let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
            let folder = path.is_dir().then(|| path.clone());
            shot.scene.draw.set_remaster_pack(Some(pack), folder);
        }
        shot.scene.draw.cfg.render.fidelity =
            FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
        shot.poll(&mut gpu);
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        for _ in 0..24 {
            shot.step_held(&mut gpu);
        }
        if name.starts_with("wade") {
            // The body stands in the water once the world has moved on a little.
            for _ in 0..8 {
                shot.step_by(&mut gpu, 1.0 / 30.0);
            }
        }
        let write = |what: &str, rgba: &[u8], height: u32| {
            if let Some(dir) = dir.as_ref() {
                let file = dir.join(format!("{what}-{tag}.png"));
                stations::write_png(&file, size.0, height, rgba);
            }
        };
        if name == "wade-walk" {
            // A strip down the page: a frame every tenth of a second as the body walks.
            let mut strip = Vec::new();
            let mut frames = 0;
            shot.input.forward = true;
            for k in 0..24 {
                shot.step_by(&mut gpu, 1.0 / 30.0);
                if k % 3 == 2 {
                    strip.extend(capture(&mut gpu));
                    frames += 1;
                }
            }
            println!("{name}: {}", timings(&mut shot, &mut gpu));
            write(name, &strip, size.1 * frames);
        } else {
            let rgba = capture(&mut gpu);
            println!("{name}: {}", timings(&mut shot, &mut gpu));
            write(name, &rgba, size.1);
        }
        drop(shot);
    }
}
