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
//! - `DERETH_WATER_CHECK`: set, each water pixel shows what its reflection found instead: red a
//!   surface, green the land, blue the sky.

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

/// The pillar on the bank of the lake, the body standing in the water before it.
const PILLAR: &str = "dereth report 1; loc 0xA9B50015 [65.766556 103.075867 27.554998] -0.561041                       0.000000 0.000000 -0.827788";

/// The lake's bank by a tree, the body standing in the water beside it.
const TREE: &str = "dereth report 1; loc 0xA9B5001C [83.963875 82.829918 27.904999] -0.645227                     0.000000 0.000000 -0.763991";

/// Every view: its name, then how it is drawn.
const SCENES: [&str; 16] = [
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
    "wade-deep",
    "wade-deep-walk",
    "login-river",
    "shore-night",
    "pillar-night",
    "tree-bank",
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
    /// A report's place at `hour` of a rainy day, its camera moved as [`View::Report`]'s is.
    Rainy {
        line: &'static str,
        back: f32,
        up: f32,
        pitch: f32,
        hour: f32,
    },
    /// A free camera `eye` metres over block-local `(x, y)` of `block`, at `hour` of a fair day.
    Free {
        block: u16,
        x: f32,
        y: f32,
        eye: f32,
        yaw: f32,
        pitch: f32,
        hour: f32,
    },
    /// A report's place with the body and its camera carried `ahead` metres along the camera's
    /// heading, the body dropped from `drop` metres over where it stood.
    Walked {
        line: &'static str,
        ahead: f32,
        drop: f32,
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
        // The select screen's view over Holtburg and its river, as it starts its swing.
        "login-river" => View::Free {
            block: 0xA9B3,
            x: 96.0,
            y: 124.8,
            eye: 45.0,
            yaw: 0.0,
            pitch: -0.42,
            hour: 0.4,
        },
        // The pond's lily pads and its shore at night, the camera low over the water.
        "shore-night" => View::Rainy {
            line: POND,
            back: 3.0,
            up: -3.4,
            pitch: 0.45,
            hour: 0.126,
        },
        "pillar-night" => View::Rainy {
            line: PILLAR,
            back: 0.0,
            up: 0.0,
            pitch: 0.0,
            hour: 0.126,
        },
        // A tree on the dry bank of the lake, the body in the water beside it, in the afternoon.
        "tree-bank" => View::Rainy {
            line: TREE,
            back: 0.0,
            up: 0.0,
            pitch: 0.0,
            hour: 0.55,
        },
        "wade-up" => report(POND, 0.0, -3.0, 0.75),
        "wade-deep" | "wade-deep-walk" => View::Walked {
            line: LAKE,
            ahead: 30.0,
            drop: 0.5,
        },
        _ => report(POND, 0.0, 0.0, 0.0),
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The station and report a view is drawn from.
fn place(store: &Arc<RetailDatStore>, v: &View) -> (Station, Option<VisualReport>) {
    match v {
        View::Rainy {
            line,
            back,
            up,
            pitch,
            hour,
        } => {
            let (mut st, report) = place(
                store,
                &View::Report {
                    line,
                    back: *back,
                    up: *up,
                    pitch: *pitch,
                },
            );
            st.clock = stations::Clock::Rainy(*hour);
            (st, report)
        }
        View::Free {
            block,
            x,
            y,
            eye,
            yaw,
            pitch,
            hour,
        } => (
            Station {
                name: "free",
                why: "a free camera",
                place: stations::Place::Free {
                    block: *block,
                    x: *x,
                    y: *y,
                    eye: *eye,
                    yaw: *yaw,
                    pitch: *pitch,
                },
                clock: stations::Clock::Sunny(*hour),
                weather: false,
                land_radius: 8,
            },
            None,
        ),
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
        View::Walked { line, ahead, drop } => {
            let mut report = VisualReport::parse(line).expect("the view's report reads");
            if let (Some(c), Some(loc)) = (report.camera.as_mut(), report.loc.as_mut()) {
                let (dx, dy) = (
                    -dereth_primitives::num::math::sinf(c.yaw) * ahead,
                    dereth_primitives::num::math::cosf(c.yaw) * ahead,
                );
                c.origin.x += dx;
                c.origin.y += dy;
                c.origin.z -= 1.5;
                c.pitch += 0.2;
                c.cell = None;
                let o = &mut loc.frame.origin;
                o.x += dx;
                o.y += dy;
                o.z += drop;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let cell = 1 + (o.y / 24.0) as u32 + (o.x / 24.0) as u32 * 8;
                loc.cell = dereth_primitives::CellId((loc.cell.0 & 0xFFFF_0000) | cell);
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
        if env("DERETH_WATER_CHECK").is_some() {
            // What each water pixel's reflection found: red a surface, green the land, blue
            // the sky.
            let w = shot.scene.draw.remaster_water();
            shot.scene.draw.set_remaster_water(
                dereth_scene::world_scene::RemasterWater { check: true, ..w },
                &mut gpu,
            );
        }
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        for _ in 0..24 {
            shot.step_held(&mut gpu);
        }
        if name.starts_with("wade") || name.ends_with("night") || name == "tree-bank" {
            // The body stands in the water once the world has moved on a second.
            for _ in 0..30 {
                shot.step_by(&mut gpu, 1.0 / 30.0);
            }
        }
        let write = |what: &str, rgba: &[u8], height: u32| {
            if let Some(dir) = dir.as_ref() {
                let file = dir.join(format!("{what}-{tag}.png"));
                stations::write_png(&file, size.0, height, rgba);
            }
        };
        if name.ends_with("walk") {
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

/// The select screen's view over Holtburg as its camera swings as the select screen swings it, a
/// frame every sixtieth of a second from the middle of the swing, where it turns fastest, each
/// written as `swing-<tag>-NNN.png` under `DERETH_REMASTER_CAPTURE_DIR`.
///
/// - `DERETH_SWING_FRAMES`: how many frames (default 90).
/// - `DERETH_REMASTER_LOOK_BOXES`, `DERETH_REMASTER_LOOK_TAG` and `DERETH_REMASTER_SIZE` as for
///   [`capture_the_remasters_water`].
#[test]
#[ignore = "instrument: writes the select screen's swing frame by frame; run with --ignored"]
fn capture_the_select_screens_swing() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let frames: usize = env("DERETH_SWING_FRAMES")
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    let size = remaster_frames::size();
    let (st, _) = place(&store, &view("login-river"));
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    // The select screen's swing: 0.45 radians either way over 80 seconds.
    let home = shot.scene.camera.yaw;
    for k in 0..frames {
        #[allow(clippy::cast_precision_loss)] // a frame count
        let t = k as f32 / 60.0;
        shot.scene.camera.yaw =
            home + 0.45 * dereth_primitives::num::math::sinf(std::f32::consts::TAU * t / 80.0);
        shot.step_by(&mut gpu, 1.0 / 60.0);
        let rgba = capture(&mut gpu);
        if let Some(dir) = dir.as_ref() {
            stations::write_png(
                &dir.join(format!("swing-{tag}-{k:03}.png")),
                size.0,
                size.1,
                &rgba,
            );
        }
    }
}

/// One frame's measure of the select screen's view: where the land meets the sky, and its colors
/// and distances.
struct Measured {
    /// The mean row, over every eighth column, of the first pixel from the top that is not sky.
    horizon: f64,
    /// Each pixel's lightness, 0 to 255, at the size the world was drawn.
    light: Vec<f32>,
    /// Each pixel's distance band: 0 under 150 meters, 1 to 500, 2 to 1,500, 3 beyond, 4 the sky.
    band: Vec<u8>,
    width: usize,
    height: usize,
}

/// The measure of a frame: its lightness from `shown`, the picture shown (`w` by `h`), and its
/// distances from `c`, the remaster's own pictures at the size the world was drawn.
fn measured(
    c: &dereth_scene::world_scene::RemasterCapture,
    shown: &[u8],
    w: usize,
    h: usize,
) -> Measured {
    // The size the remaster's class and depth hold, which an upscaler makes the render size.
    let (cw, ch) = if c.class.len() == (c.width * c.height) as usize {
        (c.width as usize, c.height as usize)
    } else {
        (c.render_size.0 as usize, c.render_size.1 as usize)
    };
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let mut drawn_band = vec![4u8; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let i = y * cw + x;
            let class = c.class[i];
            if class == 0 || class == 5 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)] // a pixel's place
            let ndc = glam::Vec4::new(
                (x as f32 + 0.5) / cw as f32 * 2.0 - 1.0,
                1.0 - (y as f32 + 0.5) / ch as f32 * 2.0,
                c.depth[i],
                1.0,
            );
            let p = inv * ndc;
            let d = (p.truncate() / p.w).distance(eye);
            drawn_band[i] = match d {
                d if d < 150.0 => 0,
                d if d < 500.0 => 1,
                d if d < 1_500.0 => 2,
                _ => 3,
            };
        }
    }
    let mut band = vec![4u8; w * h];
    let mut light = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = &shown[i * 4..i * 4 + 3];
            light[i] =
                0.2126 * f32::from(px[0]) + 0.7152 * f32::from(px[1]) + 0.0722 * f32::from(px[2]);
            band[i] = drawn_band[(y * ch / h) * cw + x * cw / w];
        }
    }
    let mut rows = 0.0f64;
    let mut n = 0.0f64;
    for x in (0..cw).step_by(8) {
        if let Some(y) = (0..ch).find(|y| drawn_band[y * cw + x] != 4) {
            #[allow(clippy::cast_precision_loss)] // a row
            {
                rows += y as f64;
            }
            n += 1.0;
        }
    }
    Measured {
        horizon: rows / n.max(1.0),
        light,
        band,
        width: w,
        height: h,
    }
}

/// The share of each distance band's pixels whose lightness changed by more than 10 of 255 from
/// one frame to the next, and their mean change.
fn flicker(a: &Measured, b: &Measured) -> [(f64, f64); 5] {
    let mut out = [(0.0, 0.0); 5];
    let mut counts = [0.0f64; 5];
    for i in 0..a.width * a.height {
        let k = a.band[i] as usize;
        let d = f64::from((a.light[i] - b.light[i]).abs());
        counts[k] += 1.0;
        out[k].0 += f64::from(u8::from(d > 10.0));
        out[k].1 += d;
    }
    for k in 0..5 {
        if counts[k] > 0.0 {
            out[k].0 /= counts[k];
            out[k].1 /= counts[k];
        }
    }
    out
}

/// What moves in the select screen's view: its camera held still (`DERETH_SWING_STILL`), or swung
/// as the select screen swings it from the middle of its swing, where it turns fastest, a frame
/// every sixtieth of a second. Printed: how far the line where the land meets the sky moves from
/// frame to frame (it should not, as the camera only turns about the upright), and, for each band
/// of distance, the share of its pixels that change by more than 10 of 255 from one frame to the
/// next, and their mean change, its worst frame and the frames five times its mean; and how
/// sharp the last frame is.
///
/// - `DERETH_SWING_FRAMES`: how many frames (default 90).
/// - `DERETH_SWING_UPSCALE`: `fsr`, `dlss` or `metalfx`, drawn at two thirds of the pixels; none
///   by default.
/// - `DERETH_REMASTER_LOOK_BOXES` and `DERETH_REMASTER_SIZE` as for [`capture_the_remasters_water`].
#[test]
#[ignore = "instrument: measures what moves in the select screen's view; run with --ignored"]
fn measure_the_select_screens_swing() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let still = env("DERETH_SWING_STILL").is_some();
    let frames: usize = env("DERETH_SWING_FRAMES")
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    let size = remaster_frames::size();
    let (st, _) = place(&store, &view("login-river"));
    #[allow(unused_mut)]
    let mut cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    // What DLSS asks of the device, as the client asks for it: nothing unless its library stands
    // beside the test binary.
    #[cfg(all(feature = "dlss", windows))]
    {
        cfg.remaster_extensions = dereth_scene::world_scene::remaster_device_extensions();
    }
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    // An upscaler at two thirds of the pixels, where `DERETH_SWING_UPSCALE` names one.
    if let Some(mode) = env("DERETH_SWING_UPSCALE") {
        use dereth_scene::world_scene::{RemasterUpscale, RemasterUpscaleMode};
        let mode = match mode.as_str() {
            "fsr" => RemasterUpscaleMode::Fsr,
            "dlss" => RemasterUpscaleMode::DlssSr,
            "metalfx" => RemasterUpscaleMode::MetalFx,
            other => panic!("no upscaler {other}: fsr, dlss or metalfx"),
        };
        shot.scene.draw.set_remaster_upscale(
            RemasterUpscale {
                mode,
                scale: 0.67,
                ..RemasterUpscale::default()
            },
            &mut gpu,
        );
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let home = shot.scene.camera.yaw;
    let mut seen: Vec<Measured> = Vec::new();
    let mut profiles: Vec<Vec<f64>> = Vec::new();
    let mut last_shown = Vec::new();
    for k in 0..frames {
        #[allow(clippy::cast_precision_loss)] // a frame count
        let t = k as f32 / 60.0;
        if !still {
            shot.scene.camera.yaw =
                home + 0.45 * dereth_primitives::num::math::sinf(std::f32::consts::TAU * t / 80.0);
        }
        shot.step_by(&mut gpu, 1.0 / 60.0);
        let shown = capture(&mut gpu);
        profiles.push(row_profile(&shown, size.0 as usize, size.1 as usize));
        last_shown.clone_from(&shown);
        let c = shot
            .scene
            .draw
            .remaster_capture(&mut gpu)
            .expect("the remaster drew");
        seen.push(measured(&c, &shown, size.0 as usize, size.1 as usize));
    }
    let steps: Vec<f64> = seen
        .windows(2)
        .map(|w| (w[1].horizon - w[0].horizon).abs())
        .collect();
    #[allow(clippy::cast_precision_loss)] // a frame count
    let mean_step = steps.iter().sum::<f64>() / steps.len().max(1) as f64;
    let most = steps.iter().copied().fold(0.0, f64::max);
    let (lo, hi) = seen.iter().fold((f64::MAX, f64::MIN), |(lo, hi), m| {
        (lo.min(m.horizon), hi.max(m.horizon))
    });
    println!(
        "the upscaler drawn: {}",
        dereth_scene::world_scene::remaster_upscaler_drawn()
    );
    println!(
        "{}: the horizon's row moves {mean_step:.3} a frame on the mean, {most:.3} at most, \
         {:.3} from lowest to highest",
        if still { "still" } else { "swinging" },
        hi - lo
    );
    // The picture shown: how far each frame is moved up or down from the one before, found where
    // the mean lightness of its rows, across the middle half of its width, best matches.
    let shifts: Vec<f64> = profiles
        .windows(2)
        .map(|w| vertical_shift(&w[0], &w[1]))
        .collect();
    #[allow(clippy::cast_precision_loss)] // a frame count
    let mean_shift = shifts.iter().map(|s| s.abs()).sum::<f64>() / shifts.len().max(1) as f64;
    let most_shift = shifts.iter().map(|s| s.abs()).fold(0.0, f64::max);
    let total: f64 = shifts.iter().sum();
    println!(
        "  shown: moved up or down {mean_shift:.3} pixels a frame on the mean, {most_shift:.3} at \
         most, {total:.3} in all"
    );
    // How sharp the last frame is: the mean step in lightness from each pixel to the next one
    // across and down.
    let (w, h) = (size.0 as usize, size.1 as usize);
    let l = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        0.2126 * f64::from(last_shown[i])
            + 0.7152 * f64::from(last_shown[i + 1])
            + 0.0722 * f64::from(last_shown[i + 2])
    };
    let mut edge = 0.0;
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            edge += (l(x + 1, y) - l(x, y)).abs() + (l(x, y + 1) - l(x, y)).abs();
        }
    }
    #[allow(clippy::cast_precision_loss)] // a picture's size
    let sharpness = edge / ((w - 1) * (h - 1)) as f64;
    println!("  the last frame's sharpness: {sharpness:.3}");
    let mut sums = [(0.0, 0.0); 5];
    // Each band's worst frame, and the frames whose share is five times the band's mean or more.
    let mut worst = [(0.0f64, 0usize); 5];
    let mut shares: Vec<[f64; 5]> = Vec::new();
    for (k, w) in seen.windows(2).enumerate() {
        let f = flicker(&w[0], &w[1]);
        shares.push(f.map(|x| x.0));
        for (b, (s, x)) in sums.iter_mut().zip(f).enumerate() {
            s.0 += x.0;
            s.1 += x.1;
            if x.0 > worst[b].0 {
                worst[b] = (x.0, k + 1);
            }
        }
    }
    #[allow(clippy::cast_precision_loss)] // a frame count
    let n = (seen.len() - 1).max(1) as f64;
    for (b, (name, s)) in [
        "under 150 m",
        "150-500 m",
        "500-1500 m",
        "beyond 1500 m",
        "the sky",
    ]
    .iter()
    .zip(sums)
    .enumerate()
    {
        let mean = s.0 / n;
        let spikes = shares
            .iter()
            .filter(|x| x[b] >= 5.0 * mean && x[b] > 0.005)
            .count();
        println!(
            "  {name:>14}: {mean:.4} of its pixels change by more than 10 a frame, by {:.2} on the \
             mean; at worst {:.4} (frame {}); {spikes} frames five times the mean",
            s.1 / n,
            worst[b].0,
            worst[b].1
        );
    }
}

/// The mean lightness of each row of the RGBA picture `rgba`, across the middle half of its width.
fn row_profile(rgba: &[u8], w: usize, h: usize) -> Vec<f64> {
    (0..h)
        .map(|y| {
            let mut sum = 0.0;
            for x in w / 4..w * 3 / 4 {
                let i = (y * w + x) * 4;
                sum += 0.2126 * f64::from(rgba[i])
                    + 0.7152 * f64::from(rgba[i + 1])
                    + 0.0722 * f64::from(rgba[i + 2]);
            }
            #[allow(clippy::cast_precision_loss)] // a width
            {
                sum / (w / 2) as f64
            }
        })
        .collect()
}

/// How many rows, to a fiftieth, `b` is moved down from `a`, within three either way: where the
/// rows' lightness, read between rows, differs least.
fn vertical_shift(a: &[f64], b: &[f64]) -> f64 {
    let at = |p: &[f64], y: f64| {
        let y0 = y.floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a row in range
        let i = y0 as usize;
        let f = y - y0;
        p[i] * (1.0 - f) + p[(i + 1).min(p.len() - 1)] * f
    };
    let n = a.len();
    let mut best = (f64::MAX, 0.0);
    for k in -150..=150 {
        let s = f64::from(k) / 50.0;
        let mut err = 0.0;
        for y in 8..n - 8 {
            #[allow(clippy::cast_precision_loss)] // a row
            let d = at(a, y as f64) - at(b, y as f64 + s);
            err += d * d;
        }
        if err < best.0 {
            best = (err, s);
        }
    }
    best.1
}
