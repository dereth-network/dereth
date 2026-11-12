//! A view drawn by the remaster frame after frame while the world's clock runs, held still or
//! turning slowly as the screens before the world turn theirs, under the boxes the player ticks:
//! each frame's pixels that changed from the last, and the frames written as pictures, for
//! finding what moves in a picture that should hold still.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_STILL_PLACE`: a report line saying where the camera stands (default: the camera the
//!   screens before the world start with over Holtburg, mid-morning on a clear day).
//! - `DERETH_STILL_FIDELITY`: the remaster's boxes as `--fidelity` takes them (default
//!   `Remaster=1`).
//! - `DERETH_ART_PACK`: an art pack drawn, all of it.
//! - `DERETH_STILL_TURN`: radians a second the camera turns (default 0, held still), and
//!   `DERETH_STILL_TILT` radians a second it tilts.
//! - `DERETH_STILL_FRAMES`: frames drawn (default 120), at sixty a second; the first half settle
//!   and are not written.
//! - `DERETH_STILL_CHECK`: set, the anti-aliasing checks its motion instead (the projection held,
//!   last frame's picture found where each pixel's surface was), and each written frame has a
//!   `c<frame>.png` of what it found beside the frame's own `s<frame>.png`.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterPacks, RemasterPost};

use super::stations::{self, report_station, Clock, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The pixels whose largest channel differs by more than `levels` between two frames.
fn apart(a: &[u8], b: &[u8], levels: u8) -> usize {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|k| p[k].abs_diff(q[k]) > levels))
        .count()
}

#[test]
#[ignore = "instrument: draws a view frame after frame and writes pictures; run with --ignored"]
fn draw_a_view_frame_after_frame() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = super::remaster_frames::size();
    let report = env("DERETH_STILL_PLACE")
        .map(|l| VisualReport::parse(&l).expect("the place's report reads"));
    let station = report.as_ref().map_or_else(
        || Station {
            name: "backdrop",
            why: "the screens before the world: Holtburg from the camera they start with",
            place: Place::Start { block: 0xA9B4 },
            clock: Clock::Sunny(0.4),
            weather: true,
            land_radius: 8,
        },
        |r| report_station(r, None),
    );
    let fidelity = env("DERETH_STILL_FIDELITY").unwrap_or_else(|| "Remaster=1".to_owned());
    let turn: f32 = env("DERETH_STILL_TURN").map_or(0.0, |v| v.trim().parse().expect("a turn"));
    let tilt: f32 = env("DERETH_STILL_TILT").map_or(0.0, |v| v.trim().parse().expect("a tilt"));
    let frames: usize =
        env("DERETH_STILL_FRAMES").map_or(120, |v| v.trim().parse().expect("a count"));
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    if let Some(d) = dir.as_ref() {
        std::fs::create_dir_all(d).expect("the pictures' folder");
    }

    #[allow(unused_mut)]
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
    if let Some(r) = report.as_ref() {
        shot.apply_report(r);
    }
    shot.frame_seconds = 1.0 / 60.0;
    if let Some(path) = env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
        let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
        let folder = path.is_dir().then(|| path.clone());
        shot.scene.draw.set_remaster_pack(Some(pack), folder);
    }
    let boxes = FidelityPreferences::parse_switch(&fidelity).expect("the boxes read");
    let remaster = boxes.remaster();
    shot.scene.draw.cfg.render.fidelity = boxes;
    shot.poll(&mut gpu);
    if remaster && !shot.scene.draw.remaster_on() {
        shot.scene
            .draw
            .set_remaster(true, &store, &mut gpu)
            .expect("the remaster starts");
        shot.poll(&mut gpu);
    }
    println!(
        "the remaster is {}",
        if shot.scene.draw.remaster_on() {
            "on"
        } else {
            "off"
        }
    );
    let check = env("DERETH_STILL_CHECK").is_some();
    if check {
        let post = RemasterPost {
            taa: true,
            check: true,
            ..shot.scene.draw.remaster_post()
        };
        shot.scene.draw.set_remaster_post(post, &mut gpu);
    }
    if env("DERETH_ART_PACK").is_some() {
        shot.scene
            .draw
            .set_remaster_packs(RemasterPacks::all(), &mut gpu);
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let (position, yaw, pitch) = (
        shot.scene.camera.position,
        shot.scene.camera.yaw,
        shot.scene.camera.pitch,
    );
    println!(
        "{}: camera {position:?} yaw {yaw} pitch {pitch}; {fidelity}; turning {turn} rad/s",
        station.name
    );
    let mut last: Option<Vec<u8>> = None;
    for k in 0..frames {
        #[allow(clippy::cast_precision_loss)] // a few hundred frames
        let t = k as f32 / 60.0;
        shot.hold_camera(position, yaw + turn * t, pitch + tilt * t);
        shot.step(&mut gpu);
        if k * 2 < frames {
            continue;
        }
        let now = stations::capture(&mut gpu);
        if let Some(was) = last.as_ref() {
            println!(
                "frame {k:4}: {:7} pixels more than 8 levels apart, {:7} more than 25",
                apart(was, &now, 8),
                apart(was, &now, 25)
            );
        }
        if let Some(d) = dir.as_ref() {
            stations::write_png(&d.join(format!("f{k:04}.png")), size.0, size.1, &now);
            if let Some(p) = check
                .then(|| shot.scene.draw.remaster_post_picture(&mut gpu))
                .flatten()
            {
                let bytes = |c: &[[f32; 4]]| -> Vec<u8> {
                    c.iter()
                        .flat_map(|v| {
                            // LINT-OK: a color clamped to a byte
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let b = |x: f32| {
                                (dereth_primitives::num::math::powf(x.clamp(0.0, 1.0), 1.0 / 2.2)
                                    * 255.0) as u8
                            };
                            [b(v[0]), b(v[1]), b(v[2]), 255]
                        })
                        .collect()
                };
                stations::write_png(
                    &d.join(format!("c{k:04}.png")),
                    p.width,
                    p.height,
                    &bytes(&p.shown),
                );
                stations::write_png(
                    &d.join(format!("s{k:04}.png")),
                    p.width,
                    p.height,
                    &bytes(&p.scene),
                );
            }
        }
        last = Some(now);
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
}
