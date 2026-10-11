//! A turn on the spot: a body standing at one Holtburg place, turned in small steps between two
//! headings, each frame written as a PNG with what the scene held and drew for it, so a frame whose
//! picture changes more than the turn explains can be found and named.
//!
//! - `DERETH_TURN_DIR`: where the files go.
//! - `DERETH_TURN_CAMERA`: `orbit` (the Horizon interface's camera, the default), `game` (the
//!   classic and modern interfaces' camera) or `eye` (a free camera at the body's eye, turned in
//!   place with no body: the turn alone, with no eye movement).
//! - `DERETH_TURN_FROM`, `DERETH_TURN_TO`, `DERETH_TURN_STEP`: headings in degrees, turned
//!   counter-clockwise from north.
//! - `DERETH_TURN_FRAMES`: frames drawn at each heading, each one written (default 1).
//! - `DERETH_TURN_RADIUS`: the landscape window radius (default 25, the Extreme draw distance).
//! - `DERETH_TURN_BUDGET_MS`: the landscape's streaming budget a frame, as a connected client has
//!   (default 4; `0` builds everything at once, as an offline run does).
//! - `DERETH_TURN_DISTANCE`: the orbit camera's distance (default its own).
//! - `DERETH_HIFI_SIZE`: `WxH`, default `1920x1080`.

#![cfg(gpu)]

use std::fmt::Write as _;
use std::path::PathBuf;

use super::hifi_stations::{self, capture, write_png, Clock, Place, Shot, Station};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn num(name: &str, d: f32) -> f32 {
    env(name).and_then(|v| v.parse().ok()).unwrap_or(d)
}

/// The window slots that have a block but no terrain mesh: blocks that are not drawn.
fn holes(shot: &Shot) -> Vec<(i32, i32)> {
    let w = &shot.scene.streamer.window;
    let n = w.mid_width();
    let mut list = Vec::new();
    for xi in 0..n {
        for yi in 0..n {
            if let Some(s) = w.slot(xi, yi) {
                if s.mesh.is_none() {
                    list.push((s.block_x, s.block_y));
                }
            }
        }
    }
    list
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TURN_DIR set and --ignored"]
fn capture_a_turn() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (width, height) = env("DERETH_HIFI_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((1920u32, 1080u32));
    let dir = PathBuf::from(env("DERETH_TURN_DIR").expect("DERETH_TURN_DIR"));
    std::fs::create_dir_all(&dir).expect("the capture folder");
    let mode = env("DERETH_TURN_CAMERA").unwrap_or_else(|| "orbit".into());
    let (from, to, step) = (
        num("DERETH_TURN_FROM", -4.023),
        num("DERETH_TURN_TO", 47.54),
        num("DERETH_TURN_STEP", 0.5),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let radius = num("DERETH_TURN_RADIUS", 25.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let budget = num("DERETH_TURN_BUDGET_MS", 4.0) as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = num("DERETH_TURN_FRAMES", 1.0).max(1.0) as usize;
    let (x, y) = (86.600_27_f32, 5.626_395_f32);
    let place = |yaw: f32| {
        if mode == "eye" {
            Place::Free {
                block: hifi_stations::HOLTBURG,
                x,
                y,
                eye: 1.6,
                yaw: yaw.to_radians(),
                pitch: 0.0,
            }
        } else {
            Place::Ground {
                block: hifi_stations::HOLTBURG,
                x,
                y,
                yaw,
            }
        }
    };
    let station = |yaw: f32| Station {
        name: "turn",
        why: "",
        place: place(yaw),
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: radius,
    };
    let mut device = crate::common::test_gpu(width, height);
    let gpu = &mut device;
    let mut shot = Shot::open(&store, gpu, &station(from));
    shot.poll(gpu);
    if mode == "orbit" {
        let c = shot.scene.character.as_mut().expect("a body");
        c.camera.orbit_active = true;
        if let Some(d) = env("DERETH_TURN_DISTANCE").and_then(|v| v.parse().ok()) {
            c.camera.orbit.distance = d;
        }
        c.camera.place_orbit_anew();
    }
    // Everything streamed in at once, then the connected client's budget from here on.
    for _ in 0..8 {
        shot.step(gpu);
    }
    shot.scene.draw.cfg.stream_budget =
        (budget > 0).then(|| std::time::Duration::from_millis(budget));
    for _ in 0..600 {
        shot.step(gpu);
        if shot.scene.draw.pending_slot_count() == 0 && holes(&shot).is_empty() {
            break;
        }
    }
    for _ in 0..4 {
        shot.step(gpu);
    }
    let mut log = String::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((to - from) / step).ceil() as usize;
    let mut f = 0usize;
    for k in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let yaw = (from + step * k as f32).min(to);
        if mode == "eye" {
            shot.scene.camera.yaw = yaw.to_radians();
        } else if let Some(c) = shot.scene.character.as_mut() {
            // Turned where it stands: a teleport would put the camera back on the body.
            let r = yaw.to_radians();
            let q = dereth_primitives::Quat::new(
                dereth_primitives::num::math::cosf(r * 0.5),
                0.0,
                0.0,
                dereth_primitives::num::math::sinf(r * 0.5),
            );
            if let Some(o) = c.world.get_mut(c.handle) {
                o.position.frame.rotation = q;
            }
            if mode == "orbit" {
                c.camera.place_orbit_anew();
            }
        }
        for _ in 0..frames {
            let before = shot.scene.streamer.viewer_block().unwrap_or((0, 0));
            let calls = gpu.draw_calls();
            shot.step(gpu);
            let calls = gpu.draw_calls() - calls;
            let rgba = capture(gpu);
            write_png(&dir.join(format!("f{f:04}.png")), width, height, &rgba);
            let cam = shot.scene.camera;
            let block = shot.scene.streamer.viewer_block().unwrap_or((0, 0));
            let holes = holes(&shot);
            let census = dereth_client_runtime::present::Scene::census(&shot.scene);
            let viewer = shot.scene.character.as_ref().map(|c| {
                (
                    c.camera.viewer.cell.0,
                    c.camera.viewer.frame.origin,
                    c.camera.viewer_cell.map(|c| c.0),
                )
            });
            let _ = writeln!(
                log,
                "{f:04} k={k:03} yaw={yaw:.3} was={:02X}{:02X} window={:02X}{:02X} eye=({:.2},{:.2},{:.2}) \
                 cell={:?} pending={} holes={} draws={calls} blocks={} scenery={} buildings={} \
                 statics={} tris={} viewer={:X?} first_holes={:?}",
                before.0,
                before.1,
                block.0,
                block.1,
                cam.position.x,
                cam.position.y,
                cam.position.z,
                shot.scene.streamer.viewer_cell,
                shot.scene.draw.pending_slot_count(),
                holes.len(),
                census.blocks_meshed,
                census.scenery_objects,
                census.buildings,
                census.static_objects,
                census.object_triangles,
                viewer,
                &holes[..holes.len().min(6)],
            );
            f += 1;
        }
    }
    std::fs::write(dir.join("turn.txt"), log).expect("the log");
}
