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
//!
//! And a walk: the same body running from the same place across a landblock line, so the window
//! re-centres and every block whose ring changed is rebuilt, each frame written with how many of
//! the window's blocks had no land to draw. A block that was drawn before and has no land now is
//! counted apart from one that has just entered the window and has not been built yet.
//!
//! - `DERETH_WALK_DIR`: where the files go.
//! - `DERETH_WALK_CAMERA`: `orbit` (the default) or `game`, as for the turn.
//! - `DERETH_WALK_HEADING`: the heading run along, in degrees counter-clockwise from north
//!   (default 180: south, across the line into the block south of Holtburg).
//! - `DERETH_WALK_BACK`: `1` walks backwards instead, so the camera looks back along the walk:
//!   with a heading of 0, the body backs south across the same line looking north.
//! - `DERETH_WALK_FRAMES`: frames run (default 150, five seconds).
//! - `DERETH_WALK_PNG_EVERY`: write every this many frames as a PNG (default 1; `0` writes none).
//! - `DERETH_TURN_RADIUS`, `DERETH_TURN_BUDGET_MS` and `DERETH_HIFI_SIZE`, as for the turn.

#![cfg(gpu)]

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use dereth_scene::world_scene::SceneReads;

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

/// A set of landblocks, by block coordinates.
type Blocks = BTreeSet<(i32, i32)>;

/// Every block in the window, and those of them that have land to draw: a terrain mesh in its
/// slot and the drawing's own record of the block.
fn window_land(shot: &Shot) -> (Blocks, Blocks) {
    let w = &shot.scene.streamer.window;
    let n = w.mid_width();
    let (mut all, mut land) = (BTreeSet::new(), BTreeSet::new());
    for xi in 0..n {
        for yi in 0..n {
            if let Some(s) = w.slot(xi, yi) {
                let b = (s.block_x, s.block_y);
                all.insert(b);
                if s.mesh.is_some() && shot.scene.block_bake(b).is_some() {
                    land.insert(b);
                }
            }
        }
    }
    (all, land)
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_WALK_DIR set and --ignored"]
fn capture_a_walk() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (width, height) = env("DERETH_HIFI_SIZE")
        .and_then(|s| {
            let (w, h) = s.split_once(['x', 'X'])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((1920u32, 1080u32));
    let dir = PathBuf::from(env("DERETH_WALK_DIR").expect("DERETH_WALK_DIR"));
    std::fs::create_dir_all(&dir).expect("the capture folder");
    let mode = env("DERETH_WALK_CAMERA").unwrap_or_else(|| "orbit".into());
    let heading = num("DERETH_WALK_HEADING", 180.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let radius = num("DERETH_TURN_RADIUS", 25.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let budget = num("DERETH_TURN_BUDGET_MS", 4.0) as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = num("DERETH_WALK_FRAMES", 150.0).max(1.0) as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let png_every = num("DERETH_WALK_PNG_EVERY", 1.0).max(0.0) as usize;
    let station = Station {
        name: "walk",
        why: "",
        place: Place::Ground {
            block: hifi_stations::HOLTBURG,
            x: 86.600_27,
            y: 5.626_395,
            yaw: heading,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: radius,
    };
    let mut device = crate::common::test_gpu(width, height);
    let gpu = &mut device;
    let mut shot = Shot::open(&store, gpu, &station);
    shot.poll(gpu);
    if mode == "orbit" {
        let c = shot.scene.character.as_mut().expect("a body");
        c.camera.orbit_active = true;
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
    // The blocks drawn with land since they entered the window.
    let mut drawn = window_land(&shot).1;
    let back = env("DERETH_WALK_BACK").is_some_and(|v| v == "1");
    shot.input = dereth_client_runtime::character::CharacterInput {
        forward: !back,
        back,
        run: true,
        ..Default::default()
    };
    let mut log = String::new();
    let (mut lost_frames, mut lost_total, mut lost_worst) = (0usize, 0usize, 0usize);
    for f in 0..frames {
        let calls = gpu.draw_calls();
        shot.step(gpu);
        let calls = gpu.draw_calls() - calls;
        if png_every > 0 && f % png_every == 0 {
            let rgba = capture(gpu);
            write_png(&dir.join(format!("w{f:04}.png")), width, height, &rgba);
        }
        let (all, land) = window_land(&shot);
        drawn.retain(|b| all.contains(b));
        let lost: Vec<(i32, i32)> = drawn.difference(&land).copied().collect();
        let fresh = all.len() - land.len() - lost.len();
        drawn.extend(land.iter().copied());
        let stale = shot
            .scene
            .window_rings()
            .iter()
            .filter(|r| r.side_cell_count != r.expected_side_cell_count)
            .count();
        if !lost.is_empty() {
            lost_frames += 1;
            lost_total += lost.len();
            lost_worst = lost_worst.max(lost.len());
        }
        let block = shot.scene.streamer.viewer_block().unwrap_or((0, 0));
        let body = shot
            .scene
            .character
            .as_ref()
            .map(|c| (c.position().cell.0, c.position().frame.origin));
        let _ = writeln!(
            log,
            "{f:04} window={:02X}{:02X} body={body:X?} pending={} lost={} new={fresh} \
             stale={stale} draws={calls} first_lost={:?}",
            block.0,
            block.1,
            shot.scene.draw.pending_slot_count(),
            lost.len(),
            &lost[..lost.len().min(6)],
        );
    }
    let _ = writeln!(
        log,
        "frames with a drawn block lost: {lost_frames} of {frames}; blocks lost over all frames: \
         {lost_total}; most in one frame: {lost_worst}"
    );
    std::fs::write(dir.join("walk.txt"), log).expect("the log");
}
