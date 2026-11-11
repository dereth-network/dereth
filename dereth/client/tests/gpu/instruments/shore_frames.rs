//! The remaster's water at its shores, where the bed is drawn lowered under it: a lake's
//! shallows by a stone pillar, a strip of land between two waters with lily pads off it,
//! Holtburg's lake shore by its boardwalk, a bank by a tree, Holtburg's dock, and the lake out
//! where it is deep, each at noon and in the evening, the camera held behind the body. Beside
//! each picture, where `DERETH_SHORE_DEBUG` is set, a picture of what the frame drew there: the
//! water's pixels shaded by how far under its surface the bed is drawn (black at the edge, white
//! at four meters), the land's pixels dark green, the rest dark gray.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_SHORE_SPOTS`: a comma list of the spots (default every one of [`SPOTS`]).
//! - `DERETH_SHORE_EXTRA`: a spot of the caller's own, `name|report line|heading`.
//! - `DERETH_SHORE_HOURS`: a comma list of `noon`, `dusk` and `rain` (a rainy dawn) (default
//!   `noon` and `dusk`).
//! - `DERETH_SHORE_HEADINGS`: a comma list of the camera's headings, radians from north
//!   (default each spot's own).
//! - `DERETH_SHORE_CAMERA`: `back,up,pitch`, meters and radians (default `6,2.2,-0.3`).
//! - `DERETH_REMASTER_LOOK_BOXES`: the boxes, as `--fidelity` takes them (default every box that
//!   draws something of its own); `RemasterWater=0` among them draws the water off.
//! - `DERETH_REMASTER_LOOK_TAG`: a word in the pictures' names, to keep runs apart.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_ART_PACK`: the art pack the remaster's pack boxes draw (default none).
//! - `DERETH_WATER_ABSORPTION`: how strongly the water takes the light of the bed seen through
//!   it (default as made).
//! - `DERETH_SHORE_DEBUG`: set, the debug picture is written beside each.
//!
//! A second instrument traces a body walking into the water, frame by frame
//! ([`trace_a_walk_into_the_water`]).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::remaster_frames;
use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, report_station, Clock, Shot};

/// Each spot: its name, where the body stands, and the camera's heading.
const SPOTS: [(&str, &str, f32); 6] = [
    (
        "pillar",
        "dereth report 1; loc 0xA9B50015 [59.258690 104.685966 27.904999] 0.723986 0.000000 \
         0.000000 -0.689815",
        -2.5,
    ),
    (
        "strip",
        "dereth report 1; loc 0xA9B50008 [11.774128 187.334106 27.554998] 0.161152 0.000000 \
         0.000000 -0.986930",
        -2.82,
    ),
    (
        "boardwalk",
        "dereth report 1; loc 0xABB50007 [8.40 147.43 28.35] 0.723986 0.000000 0.000000 \
         -0.689815",
        1.57,
    ),
    (
        "lake",
        "dereth report 1; loc 0xA9B5001C [83.963875 82.829918 27.904999] -0.645227 0.000000 \
         0.000000 -0.763991",
        -1.0,
    ),
    (
        "dock",
        "dereth report 1; loc 0xAAB50003 [12.083943 49.621696 27.554998] -0.988524 0.000000 \
         0.000000 -0.151063",
        0.3,
    ),
    (
        "deep",
        "dereth report 1; loc 0xA9B5001E [85.0 125.0 27.1] 1.0 0.0 0.0 0.0",
        -2.5,
    ),
];

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn list(name: &str) -> Option<Vec<String>> {
    env(name).map(|v| v.split(',').map(|s| s.trim().to_owned()).collect())
}

/// The share of the bed's own light a water pixel `i` of capture `c` keeps, as the water
/// works it out: the light runs down to the bed and back up along the line of sight, through
/// `under` meters of water, faded over the water's clarity (its green, of the three); with
/// `refracted`, the line of sight under the surface bent by it, as light entering water is.
fn kept(c: &dereth_scene::world_scene::RemasterCapture, i: usize, refracted: bool) -> Option<f32> {
    if c.kind[i] != 7 || c.bed[i] == 0 {
        return None;
    }
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let (w, h) = (c.width as usize, c.height as usize);
    // LINT-OK: pixel coordinates.
    #[allow(clippy::cast_precision_loss)]
    let (x, y) = ((i % w) as f32, (i / w) as f32);
    // LINT-OK: frame sizes.
    #[allow(clippy::cast_precision_loss)]
    let ndc = glam::Vec4::new(
        (x + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y + 0.5) / h as f32 * 2.0,
        c.depth[i],
        1.0,
    );
    let p = inv * ndc;
    let bed = (p / p.w).truncate();
    let eye = glam::Vec3::from(c.eye);
    let under = f32::from(c.bed[i]) * 0.04;
    let v = (eye - bed).normalize();
    let up = v.z.max(1e-3);
    let down = if refracted {
        (1.0 - (1.0 - up * up) / (1.33 * 1.33)).sqrt()
    } else {
        up
    };
    let through = under / down;
    Some(dereth_primitives::num::math::expf(-(through + under) / 1.6))
}

/// The debug picture of capture `c`: the water's pixels by how much of the bed's light they
/// keep, the line of sight bent where it enters the water (white all of it, blue none), the
/// land's dark green, the rest dark gray.
fn debug_picture(c: &dereth_scene::world_scene::RemasterCapture) -> Vec<u8> {
    let mut out = Vec::with_capacity(c.kind.len() * 4);
    for i in 0..c.kind.len() {
        let px = if let Some(k) = kept(c, i, true) {
            // LINT-OK: a share shown as a byte.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (k * 255.0) as u8;
            [v, v, 255, 255]
        } else if c.kind[i] == 7 {
            [255, 255, 255, 255]
        } else if c.class[i] == 1 {
            [20, 60, 20, 255]
        } else {
            [40, 40, 40, 255]
        };
        out.extend(px);
    }
    out
}

/// What the water pixels of `c` keep of their bed's light, by how far under the surface the bed
/// lies: means over the bands 0-0.25, 0.25-0.5, 0.5-1 and 1-2 meters, straight and refracted.
fn keeps(c: &dereth_scene::world_scene::RemasterCapture) -> String {
    let bands = [(0.0, 0.25), (0.25, 0.5), (0.5, 1.0), (1.0, 2.0)];
    let mut out = String::new();
    for (lo, hi) in bands {
        let (mut a, mut b, mut n) = (0.0f64, 0.0f64, 0u32);
        for i in 0..c.kind.len() {
            let under = f32::from(c.bed[i]) * 0.04;
            if under <= lo || under > hi {
                continue;
            }
            if let (Some(x), Some(y)) = (kept(c, i, false), kept(c, i, true)) {
                a += f64::from(x);
                b += f64::from(y);
                n += 1;
            }
        }
        let m = f64::from(n.max(1));
        out.push_str(&format!(
            " [{lo}-{hi} m: {n} px, keeps {:.2} straight, {:.2} refracted]",
            a / m,
            b / m
        ));
    }
    out
}

/// Each spot drawn at each hour and heading, its pictures written under
/// `DERETH_REMASTER_CAPTURE_DIR`.
#[test]
#[ignore = "instrument: writes the remaster's water at its shores; run with --ignored"]
fn capture_the_remasters_shores() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let spots = list("DERETH_SHORE_SPOTS");
    let hours = list("DERETH_SHORE_HOURS").unwrap_or_else(|| vec!["noon".into(), "dusk".into()]);
    let headings: Option<Vec<f32>> = list("DERETH_SHORE_HEADINGS")
        .map(|v| v.iter().map(|h| h.parse().expect("a heading")).collect());
    let camera: Vec<f32> = env("DERETH_SHORE_CAMERA")
        .unwrap_or_else(|| "6,2.2,-0.3".to_owned())
        .split(',')
        .map(|v| v.trim().parse().expect("a number"))
        .collect();
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let debug = env("DERETH_SHORE_DEBUG").is_some();
    let size = remaster_frames::size();
    let backend = crate::common::remaster_backend();
    // A spot of the caller's own: `name|report line|heading`.
    let extra = env("DERETH_SHORE_EXTRA").map(|v| {
        let mut parts = v.splitn(3, '|');
        let name: &'static str = Box::leak(parts.next().expect("a name").to_owned().into());
        let line: &'static str = Box::leak(parts.next().expect("a report").to_owned().into());
        let heading = parts
            .next()
            .map_or(0.0, |h| h.trim().parse().expect("a heading"));
        (name, line, heading)
    });
    for (name, line, own) in SPOTS.into_iter().chain(extra) {
        if spots.as_ref().is_some_and(|s| !s.iter().any(|n| n == name)) {
            continue;
        }
        for hour in &hours {
            for heading in headings.clone().unwrap_or_else(|| vec![own]) {
                let report = VisualReport::parse(line).expect("the spot's report reads");
                let mut st = report_station(&report, Some(8));
                st.clock = match hour.as_str() {
                    "dusk" => Clock::SunUp(5.0),
                    "rain" => Clock::Rainy(0.28),
                    _ => Clock::Sunny(0.5),
                };
                st.weather = hour == "rain";
                let cfg = DeviceConfig {
                    width: size.0,
                    height: size.1,
                    remaster: true,
                    debug,
                    ..DeviceConfig::default()
                };
                let mut gpu = Gpu::new_on(backend, None, &cfg).expect("a device");
                let mut shot = Shot::open(&store, &mut gpu, &st);
                shot.apply_report(&report);
                shot.poll(&mut gpu);
                if let Some(path) = env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
                    let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
                    let folder = path.is_dir().then(|| path.clone());
                    shot.scene.draw.set_remaster_pack(Some(pack), folder);
                }
                shot.scene.draw.cfg.render.fidelity =
                    FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
                shot.follow = Some([camera[0], camera[1], camera[2], heading]);
                shot.poll(&mut gpu);
                if let Some(a) = env("DERETH_WATER_ABSORPTION") {
                    let w = shot.scene.draw.remaster_water();
                    shot.scene.draw.set_remaster_water(
                        dereth_scene::world_scene::RemasterWater {
                            absorption: a.parse().expect("a number"),
                            ..w
                        },
                        &mut gpu,
                    );
                }
                shot.step_held(&mut gpu);
                shot.scene.wait_for_remaster_ground();
                for _ in 0..24 {
                    shot.step_held(&mut gpu);
                }
                // The body stands in the water once the world has moved on a second.
                for _ in 0..30 {
                    shot.step_by(&mut gpu, 1.0 / 30.0);
                }
                for _ in 0..8 {
                    shot.step_held(&mut gpu);
                }
                let what = format!("{name}-{hour}-{heading}-{tag}");
                if let Some(c) = shot.scene.character.as_ref() {
                    let body = c.render_frame().origin;
                    println!(
                        "{what}: the body at {:.3}, its feet drawn {:.3} off it, the drawn ground \
                         under it at {:?}",
                        body.z,
                        shot.scene.remaster_shift_at(body),
                        shot.scene.remaster_ground_at(body.x, body.y)
                    );
                }
                let rgba = capture(&mut gpu);
                if let Some(dir) = dir.as_ref() {
                    stations::write_png(&dir.join(format!("{what}.png")), size.0, size.1, &rgba);
                }
                if debug {
                    let c = shot
                        .scene
                        .draw
                        .remaster_capture(&mut gpu)
                        .expect("the remaster drew");
                    println!("{what}:{}", keeps(&c));
                    if let Some(dir) = dir.as_ref() {
                        stations::write_png(
                            &dir.join(format!("{what}-debug.png")),
                            c.width,
                            c.height,
                            &debug_picture(&c),
                        );
                    }
                }
                drop(shot);
            }
        }
    }
}

/// A body walked from where `DERETH_SHORE_WALK` puts it, `report line|heading degrees|frames`
/// (default the pillar's shallows, walking out into the lake, 150 frames), at 30 frames a second,
/// each frame's line printed: where physics puts the body, where its feet are drawn, the drawn
/// ground under it and retail's ground there. `DERETH_SHORE_WALK_RUN` set, it runs. Where `DERETH_REMASTER_CAPTURE_DIR` is
/// set, every tenth frame is written there.
#[test]
#[ignore = "instrument: traces a body walking into the water; run with --ignored"]
fn trace_a_walk_into_the_water() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let spec = env("DERETH_SHORE_WALK").unwrap_or_else(|| format!("{}|-51.7|150", SPOTS[0].1));
    let mut parts = spec.split('|');
    let line = parts.next().expect("a report").to_owned();
    let heading: f32 = parts
        .next()
        .map_or(0.0, |h| h.trim().parse().expect("degrees"));
    let frames: u32 = parts
        .next()
        .map_or(150, |n| n.trim().parse().expect("a count"));
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let size = remaster_frames::size();
    let report = VisualReport::parse(&line).expect("the spot's report reads");
    let st = report_station(&report, Some(8));
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    let r = heading.to_radians();
    if let Some(c) = shot.scene.character.as_mut() {
        let mut at = c.position();
        at.frame = dereth_primitives::Frame::new(
            at.frame.origin,
            dereth_primitives::Quat::new(
                dereth_primitives::num::math::cosf(r * 0.5),
                0.0,
                0.0,
                dereth_primitives::num::math::sinf(r * 0.5),
            ),
        );
        c.teleport(at);
    }
    shot.follow = Some([6.0, 2.2, -0.3, r + std::f32::consts::PI]);
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..30 {
        shot.step_by(&mut gpu, 1.0 / 30.0);
    }
    shot.input.forward = true;
    shot.input.run = env("DERETH_SHORE_WALK_RUN").is_some();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(&store), &region)
        .expect("the land source opens");
    println!("frame x y physics feet ground retail");
    for k in 0..frames {
        shot.step_by(&mut gpu, 1.0 / 30.0);
        if let Some(c) = shot.scene.character.as_ref() {
            let body = c.render_frame().origin;
            let feet = body.z + shot.scene.remaster_shift_at(body);
            let ground = shot.scene.remaster_ground_at(body.x, body.y);
            let at = c.position();
            let retail = land.ground_height(
                dereth_primitives::LandblockId(u16::try_from(at.cell.0 >> 16).expect("a block")),
                at.frame.origin.x,
                at.frame.origin.y,
            );
            println!(
                "{k} {:.3} {:.3} {:.3} {:.3} {} {}",
                body.x,
                body.y,
                body.z,
                feet,
                ground.map_or("-".to_owned(), |g| format!("{g:.3}")),
                retail.map_or("-".to_owned(), |g| format!("{g:.3}"))
            );
        }
        if k % 10 == 0 {
            if let Some(dir) = dir.as_ref() {
                let rgba = capture(&mut gpu);
                stations::write_png(&dir.join(format!("walk-{k:03}.png")), size.0, size.1, &rgba);
            }
        }
    }
    drop(shot);
}
