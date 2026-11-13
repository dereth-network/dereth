//! A body walked forward under the orbit camera from where a report stood it, the remaster's
//! boxes ticked, frame by frame at a frame rate of the caller's: where physics has the body, where
//! it is drawn, how far the remaster lifts it onto its ground, where the camera stands and where
//! the frame is drawn from, and what each step took. It asserts nothing; it measures how evenly
//! the eye moves over the drawn ground.
//!
//! Each frame is a line of `walk.tsv` under `DERETH_CAMERA_WALK_DUMP` (default:
//! `camera-walk/` in cargo's scratch folder for integration tests), render space: where physics has
//! the feet (`phys_*`), where they are drawn (`drawn_*`, lifted), the lift and the lift the ground
//! has where physics has the feet, how far the lifted feet stand off the drawn ground (`clear`),
//! the camera (`camera_*`) and the eye the frame is drawn from (`eye_*`); then the step's
//! milliseconds on the processor, and its parts (`reserve` is the wait for the device). A summary
//! is printed: the height's second difference frame to frame, as root mean square and worst, for
//! the eye the frame is drawn from, the camera, the body as drawn and its lift.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_CAMERA_WALK`: `report line|heading degrees|frames` (default the uneven ground of
//!   [`PLACE`], the report's own heading, 360 frames). An empty heading keeps the report's.
//! - `DERETH_CAMERA_WALK_FPS`: frames a second (default 120).
//! - `DERETH_CAMERA_WALK_RUN`: set to `0`, the body walks rather than runs.
//! - `DERETH_CAMERA_WALK_RUN_RATE`: the run rate the body runs at (default the one it has, 1).
//! - `DERETH_CAMERA_WALK_TAG`: the dump's subfolder, to keep runs apart (default `walk`).
//! - `DERETH_REMASTER_LOOK_BOXES`: the boxes, as `--fidelity` takes them (default every box that
//!   draws something of its own); `Remaster=0` draws retail's ground alone.
//! - `DERETH_ART_PACK`: the art pack the remaster's pack boxes draw (default none).
//! - `DERETH_REMASTER_KNOBS`: the ground's knobs, `name = value` lines joined by `;`.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where every thirtieth frame's picture goes (none without it).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::io::Write as _;
use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_primitives::num::math;
use dereth_primitives::Vec3;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::remaster_frames;
use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, report_station, Shot};

/// Level ground that the drawn ground rises most of a meter off ahead, the camera behind a body
/// about to walk over it.
const PLACE: &str = "dereth report 1; loc 0x8A8C002A [122.750359 45.150970 124.037888] -0.927195 \
                     0.000000 0.000000 -0.374579; camera 0x8A8C [126.8507 40.90445 126.61211] yaw \
                     -5.5152984 pitch -0.17999999 cell 0x8A8C002A; time 0.355; clock \
                     308723411.0211927; weather auto";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn out_dir() -> std::path::PathBuf {
    let base = env("DERETH_CAMERA_WALK_DUMP")
        .unwrap_or_else(|| format!("{}/camera-walk", env!("CARGO_TARGET_TMPDIR")));
    let tag = env("DERETH_CAMERA_WALK_TAG").unwrap_or_else(|| "walk".to_owned());
    std::path::PathBuf::from(base).join(tag)
}

/// One frame of the walk, render space.
#[derive(Debug, Clone, Copy)]
struct WalkFrame {
    /// Where physics has the body's feet.
    physics: Vec3,
    /// Where the body is drawn between its ticks, before the remaster lifts it.
    between: Vec3,
    /// How far the remaster lifts the body as it draws it.
    lift: f32,
    /// How far the drawn ground lies above or below the body where it is drawn, lifted.
    clear: f32,
    /// Where the camera stands, and where the frame is drawn from.
    camera: Vec3,
    eye: Vec3,
    /// What the step took on the processor, milliseconds.
    ms: f64,
}

/// The root mean square and the worst of `v`'s second difference.
fn second_difference(v: &[f32]) -> (f32, f32) {
    let d: Vec<f32> = v.windows(3).map(|w| w[2] - 2.0 * w[1] + w[0]).collect();
    if d.is_empty() {
        return (0.0, 0.0);
    }
    // LINT-OK: a few hundred frames.
    #[allow(clippy::cast_precision_loss)]
    let rms = (d.iter().map(|x| x * x).sum::<f32>() / d.len() as f32).sqrt();
    (rms, d.iter().fold(0.0f32, |m, x| m.max(x.abs())))
}

/// A body walked forward from the place `DERETH_CAMERA_WALK` names under the orbit camera, the
/// body drawn between its ticks as the orbit camera draws it: a line a frame written, and how
/// evenly the eye moved printed.
#[test]
#[ignore = "instrument: traces the eye walking over the drawn ground; run with --ignored"]
fn trace_the_eye_walking_over_the_drawn_ground() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let spec = env("DERETH_CAMERA_WALK").unwrap_or_else(|| PLACE.to_owned());
    let mut parts = spec.split('|');
    let line = parts.next().expect("a report").to_owned();
    let heading: Option<f32> = parts
        .next()
        .filter(|h| !h.trim().is_empty())
        .map(|h| h.trim().parse().expect("degrees"));
    let frames: usize = parts
        .next()
        .map_or(360, |n| n.trim().parse().expect("a count"));
    let fps: f64 = env("DERETH_CAMERA_WALK_FPS").map_or(120.0, |v| v.trim().parse().expect("fps"));
    let run = env("DERETH_CAMERA_WALK_RUN").is_none_or(|v| v.trim() != "0");
    let run_rate: Option<f32> =
        env("DERETH_CAMERA_WALK_RUN_RATE").map(|v| v.trim().parse().expect("a rate"));
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let pictures = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let size = remaster_frames::size();
    let mut report = VisualReport::parse(&line).expect("the place's report reads");
    // A place given at height nought stands on retail's ground there.
    if let Some(loc) = report.loc.as_mut().filter(|l| l.frame.origin.z == 0.0) {
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(&store), &region)
            .expect("the land source opens");
        let o = loc.frame.origin;
        loc.frame.origin.z = land
            .ground_height(loc.cell.landblock(), o.x, o.y)
            .expect("the ground there");
    }
    let st = report_station(&report, Some(6));
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.apply_report(&report);
    // The camera follows the body; the report's camera only sets how it looks down.
    shot.held = None;
    shot.poll(&mut gpu);
    if let Some(path) = env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
        let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
        let folder = path.is_dir().then(|| path.clone());
        shot.scene.draw.set_remaster_pack(Some(pack), folder);
    }
    if let Some(lines) = env("DERETH_REMASTER_KNOBS") {
        let (knobs, notes) = dereth_terrain::land::drawn::Knobs::parse(&lines.replace(';', "\n"));
        assert!(notes.is_empty(), "the knobs read: {notes:?}");
        shot.scene.set_remaster_ground_knobs(knobs);
    }
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&boxes).expect("the boxes read");
    let pitch = report.camera.map_or(-0.18, |c| c.pitch);
    if let Some(c) = shot.scene.character.as_mut() {
        if let Some(h) = heading {
            let r = h.to_radians();
            let mut at = c.position();
            at.frame = dereth_primitives::Frame::new(
                at.frame.origin,
                dereth_primitives::Quat::new(math::cosf(r * 0.5), 0.0, 0.0, math::sinf(r * 0.5)),
            );
            c.teleport(at);
        }
        if run_rate.is_some() {
            c.hold_run_rate(run_rate);
            c.set_run_rate(run_rate);
        }
        c.camera.orbit_active = true;
        c.drawn_between_ticks = true;
        c.camera.orbit.pitch = pitch;
    }
    shot.frame_seconds = 1.0 / fps;
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    // Settled, standing.
    for _ in 0..60 {
        shot.step(&mut gpu);
    }
    shot.scene.wait_for_remaster_ground();
    shot.input.forward = true;
    shot.input.run = run;
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the dump's folder");
    let mut tsv = std::io::BufWriter::new(
        std::fs::File::create(dir.join("walk.tsv")).expect("the trace's file"),
    );
    writeln!(
        tsv,
        "frame\tt\tphys_x\tphys_y\tphys_z\tdrawn_x\tdrawn_y\tdrawn_z\tlift\tlift_at_physics\
         \tclear\tcamera_x\tcamera_y\tcamera_z\teye_x\teye_y\teye_z\tms\tsync\tupdate\tstream\
         \treserve\tdraw\tend"
    )
    .expect("write");
    let mut walk = Vec::with_capacity(frames);
    for k in 0..frames {
        shot.step(&mut gpu);
        let last = shot.last;
        let ms = last.sync + last.update + last.stream + last.reserve + last.draw + last.end;
        let Some(c) = shot.scene.character.as_ref() else {
            break;
        };
        let physics = c.render_frame().origin;
        let off = c.drawn_offset();
        let between = Vec3::new(physics.x + off.x, physics.y + off.y, physics.z + off.z);
        let lift = shot.scene.remaster_body_lift().unwrap_or(0.0);
        let at_physics = shot.scene.remaster_shift_at(physics);
        let clear = shot
            .scene
            .remaster_ground_at(between.x, between.y)
            .map_or(0.0, |g| between.z + lift - g);
        let camera = shot.scene.camera.position;
        let eye = shot.scene.draw.drawn_eye(&shot.scene.world).origin;
        let f = WalkFrame {
            physics,
            between,
            lift,
            clear,
            camera,
            eye,
            ms,
        };
        // LINT-OK: a frame count.
        #[allow(clippy::cast_precision_loss)]
        let t = k as f64 / fps;
        writeln!(
            tsv,
            "{k}\t{t:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\
             \t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
            f.physics.x,
            f.physics.y,
            f.physics.z,
            f.between.x,
            f.between.y,
            f.between.z + f.lift,
            f.lift,
            at_physics,
            f.clear,
            f.camera.x,
            f.camera.y,
            f.camera.z,
            f.eye.x,
            f.eye.y,
            f.eye.z,
            f.ms,
            last.sync,
            last.update,
            last.stream,
            last.reserve,
            last.draw,
            last.end,
        )
        .expect("write");
        walk.push(f);
        if k % 30 == 0 {
            if let Some(p) = pictures.as_ref() {
                let rgba = capture(&mut gpu);
                stations::write_png(&p.join(format!("walk-{k:03}.png")), size.0, size.1, &rgba);
            }
        }
    }
    tsv.flush().expect("write");
    drop(shot);
    let eye: Vec<f32> = walk.iter().map(|f| f.eye.z).collect();
    let camera: Vec<f32> = walk.iter().map(|f| f.camera.z).collect();
    let body: Vec<f32> = walk.iter().map(|f| f.between.z + f.lift).collect();
    let lift: Vec<f32> = walk.iter().map(|f| f.lift).collect();
    let flat = |a: Vec3, b: Vec3| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    let pulled = walk.iter().filter(|f| flat(f.eye, f.camera) > 1e-3).count();
    let worst_ms = walk.iter().fold(0.0f64, |m, f| m.max(f.ms));
    let travelled = walk
        .first()
        .zip(walk.last())
        .map_or(0.0, |(a, b)| flat(a.physics, b.physics));
    println!("{} frames at {fps} fps, {travelled:.1} m", walk.len());
    for (name, v) in [
        ("eye drawn from", &eye),
        ("camera", &camera),
        ("body drawn", &body),
        ("lift", &lift),
    ] {
        let (rms, worst) = second_difference(v);
        println!("  {name}: second difference rms {rms:.5} m, worst {worst:.5} m");
    }
    println!("  eye drawn in front of the drawn ground on {pulled} frames");
    println!("  worst step {worst_ms:.1} ms on the processor");
    println!("  trace: {}", dir.join("walk.tsv").display());
}
