//! The sun's share on the far land, frame after frame: a view drawn by the remaster with its
//! clock held, the camera held still or turning slowly, and for each frame how much of the far
//! land went from the sun to the shadow or back against the frame before, for finding shadows
//! that flicker far off.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_FAR_PLACE`: a report line saying where the player stands (default: a hill west of
//!   Holtburg, toward the snow-capped mountains). Where it names no camera, the camera stands
//!   behind the body along its heading.
//! - `DERETH_FAR_YAW`: the camera's heading instead, degrees; `DERETH_FAR_BACK` meters behind the
//!   body it stands (default 4) and `DERETH_FAR_UP` meters over it (default 0.8).
//! - `DERETH_FAR_DAY`: the fraction of the day (default 0.342, early morning), of the first
//!   Sunny day, or of the calendar's day `DERETH_FAR_DAYNUM` where that is given.
//! - `DERETH_FAR_CLOCK`: set, the clock runs at sixty frames a second.
//! - `DERETH_FAR_RADIUS`: the land held, blocks each way (default 8).
//! - `DERETH_FAR_BOXES`: the remaster's boxes as `--fidelity` takes them (default most boxes,
//!   the traced shadows among them).
//! - `DERETH_FAR_TURN`: radians a second the camera turns (default 0, held still).
//! - `DERETH_FAR_FRAMES`: frames drawn (default 60), at sixty a second; the first ten settle.
//! - `DERETH_FAR_DISTANCE`: how far off land is far, meters (default 1000).
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it): each
//!   frame's picture `f<frame>.png` and its sun's share `v<frame>.png`.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_primitives::num::math;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::RemasterCapture;

use super::stations::{self, report_station, Clock, Shot};

/// A hill west of Holtburg, toward the snow-capped mountains past the green hills.
const FAR_HILL: &str = "dereth report 1; loc 0xA9B70021 [110.009972 14.473573 39.679970] \
                                   -0.990364 0.000000 -0.000000 -0.138488; weather off";

/// Most boxes, the traced shadows among them.
const BOXES: &str = "Remaster=1,RemasterShadows=1,RemasterSky=1,RemasterFog=1,\
                                RemasterOcclusion=1,RemasterBloom=1,RemasterSmoothing=1,\
                                RemasterWater=1,RemasterDetail=1,RemasterTracedShadows=1,\
                                RemasterGrade=1,RemasterAir=1,RemasterGrass=1,RemasterFlowers=1";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// How far each pixel of `c` stands from its eye, meters; infinite where nothing was drawn with
/// depth.
fn distances(c: &RemasterCapture) -> Vec<f64> {
    let inv = glam::DMat4::from_cols_array(&c.view_projection.map(f64::from)).inverse();
    let eye = glam::DVec3::from_array(c.eye.map(f64::from));
    let (w, h) = (c.render_size.0.max(1), c.render_size.1.max(1));
    c.depth
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            if d >= 1.0 || d <= 0.0 {
                return f64::INFINITY;
            }
            #[allow(clippy::cast_possible_truncation)] // a pixel's index
            let (x, y) = ((i as u32 % w), (i as u32 / w));
            let ndc = glam::DVec4::new(
                (f64::from(x) + 0.5) / f64::from(w) * 2.0 - 1.0,
                1.0 - (f64::from(y) + 0.5) / f64::from(h) * 2.0,
                f64::from(d),
                1.0,
            );
            let p = inv * ndc;
            (p.truncate() / p.w - eye).length()
        })
        .collect()
}

#[test]
#[ignore = "instrument: draws a view frame after frame and counts the far land's shadows changing; run with --ignored"]
#[allow(clippy::cast_precision_loss)] // pixel counts
fn draw_the_far_lands_sun_share_frame_after_frame() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = super::remaster_frames::size();
    let line = env("DERETH_FAR_PLACE").unwrap_or_else(|| FAR_HILL.to_owned());
    let report = VisualReport::parse(&line).expect("the place's report reads");
    let radius: u32 = env("DERETH_FAR_RADIUS").map_or(8, |v| v.trim().parse().expect("blocks"));
    let mut station = report_station(&report, Some(radius));
    let day: f32 = env("DERETH_FAR_DAY").map_or(0.342, |v| v.trim().parse().expect("a day"));
    station.clock = Clock::Sunny(day);
    if let Some(k) = env("DERETH_FAR_DAYNUM").map(|v| v.trim().parse::<u32>().expect("a day")) {
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let length = f64::from(region.game_time.day_length);
        let abs = f64::from(k).mul_add(length, f64::from(day) * length)
            - region.game_time.zero_time_of_year;
        let mut c = dereth_client_runtime::game_clock::GameClock::new(&region);
        c.set_game_time(0.0, abs);
        let group =
            dereth_world_render::sky::present_day_group(&region, c.current_year, c.current_day)
                .map(|g| g.day_name.clone())
                .unwrap_or_default();
        println!("day {k}: {group}");
        station.clock = Clock::At(abs);
    }
    station.weather = false;
    let boxes = env("DERETH_FAR_BOXES").unwrap_or_else(|| BOXES.to_owned());
    let turn: f32 = env("DERETH_FAR_TURN").map_or(0.0, |v| v.trim().parse().expect("a turn"));
    let frames: usize = env("DERETH_FAR_FRAMES").map_or(60, |v| v.trim().parse().expect("a count"));
    let far: f64 =
        env("DERETH_FAR_DISTANCE").map_or(1000.0, |v| v.trim().parse().expect("a distance"));
    let running = env("DERETH_FAR_CLOCK").is_some();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    if let Some(d) = dir.as_ref() {
        std::fs::create_dir_all(d).expect("the pictures' folder");
    }

    let mut cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    #[cfg(all(feature = "dlss", windows))]
    {
        cfg.remaster_extensions = dereth_scene::world_scene::remaster_device_extensions();
    }
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.apply_report(&report);
    shot.frame_seconds = 1.0 / 60.0;
    let fidelity = FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    shot.scene.draw.cfg.render.fidelity = fidelity;
    shot.poll(&mut gpu);
    if !shot.scene.draw.remaster_on() {
        shot.scene
            .draw
            .set_remaster(true, &store, &mut gpu)
            .expect("the remaster starts");
        shot.poll(&mut gpu);
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..30 {
        shot.step_held(&mut gpu);
    }
    // The chase camera behind the body, along its heading, where the report names no camera.
    let (position, yaw, pitch) = match (report.camera, station.place) {
        (None, stations::Place::Body { yaw, .. }) => {
            let yaw = env("DERETH_FAR_YAW")
                .map_or(yaw, |v| v.trim().parse().expect("degrees"))
                .to_radians();
            let up: f32 = env("DERETH_FAR_UP").map_or(0.8, |v| v.trim().parse().expect("a height"));
            let body = shot.scene.camera.position;
            let (ax, ay) = (-math::sinf(yaw), math::cosf(yaw));
            let back: f32 =
                env("DERETH_FAR_BACK").map_or(4.0, |v| v.trim().parse().expect("a distance"));
            (
                dereth_primitives::Vec3::new(body.x - ax * back, body.y - ay * back, body.z + up),
                yaw,
                -0.12,
            )
        }
        _ => (
            shot.scene.camera.position,
            shot.scene.camera.yaw,
            shot.scene.camera.pitch,
        ),
    };
    println!(
        "camera {position:?} yaw {yaw} pitch {pitch}; day {day}; {boxes}; turning {turn} rad/s; \
         traces rays {}",
        shot.scene.draw.remaster_traces_rays(&mut gpu)
    );
    let mut last: Option<(Vec<u8>, Vec<u8>)> = None;
    let mut sums = (0.0f64, 0.0f64, 0usize);
    for k in 0..frames {
        #[allow(clippy::cast_precision_loss)] // a few hundred frames
        let t = k as f32 / 60.0;
        shot.hold_camera(position, yaw + turn * t, pitch);
        if running {
            shot.step(&mut gpu);
        } else {
            shot.step_held(&mut gpu);
        }
        if k < 10 {
            continue;
        }
        let picture = stations::capture(&mut gpu);
        let c = shot
            .scene
            .draw
            .remaster_capture(&mut gpu)
            .expect("the remaster drew");
        let sun = c.sun_visibility.clone().unwrap_or_default();
        let dist = distances(&c);
        let far_px: Vec<usize> = (0..dist.len())
            .filter(|&i| dist[i].is_finite() && dist[i] > far && matches!(c.class[i], 1 | 2))
            .collect();
        let shaded = far_px
            .iter()
            .filter(|&&i| sun.get(i).is_some_and(|v| *v < 128))
            .count();
        if let Some((was_sun, was_pic)) = last.as_ref() {
            let flips = far_px
                .iter()
                .filter(|&&i| match (sun.get(i), was_sun.get(i)) {
                    (Some(a), Some(b)) => a.abs_diff(*b) > 64,
                    _ => false,
                })
                .count();
            // The shown picture's change over the far land's pixels, where the picture's size is
            // the render size (no upscaler): mean of the largest channel difference.
            let same_size = picture.len() == c.color.len();
            let levels = if same_size && !far_px.is_empty() {
                far_px
                    .iter()
                    .map(|&i| {
                        (0..3)
                            .map(|j| picture[i * 4 + j].abs_diff(was_pic[i * 4 + j]))
                            .max()
                            .unwrap_or(0) as f64
                    })
                    .sum::<f64>()
                    / far_px.len() as f64
            } else {
                f64::NAN
            };
            let share = flips as f64 / far_px.len().max(1) as f64;
            sums.0 += share;
            sums.1 += levels;
            sums.2 += 1;
            println!(
                "frame {k:4}: far pixels {:7}, in shadow {:7}, sun's share flipped {flips:6} \
                 ({:.4}), picture {levels:.3} levels",
                far_px.len(),
                shaded,
                share,
            );
        }
        if let Some(d) = dir.as_ref() {
            stations::write_png(&d.join(format!("f{k:04}.png")), size.0, size.1, &picture);
            if !sun.is_empty() {
                let grey: Vec<u8> = sun.iter().flat_map(|v| [*v, *v, *v, 255]).collect();
                stations::write_png(
                    &d.join(format!("v{k:04}.png")),
                    c.render_size.0,
                    c.render_size.1,
                    &grey,
                );
            }
        }
        last = Some((sun, picture));
    }
    if sums.2 > 0 {
        println!(
            "mean over {} frames: far sun's share flipped {:.5}, picture {:.3} levels",
            sums.2,
            sums.0 / sums.2 as f64,
            sums.1 / sums.2 as f64
        );
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
}
