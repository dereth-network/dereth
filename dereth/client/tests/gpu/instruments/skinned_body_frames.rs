//! A body drawn by the remaster with an art pack's skinned body in place of its parts, beside the
//! same body drawn with its parts: standing, running and casting, each followed by a camera in
//! front of it and to its side. Each subject is written to `DERETH_SKIN_DUMP` as one PNG of two
//! rows of moments, the parts above and the body below, both stepped exactly alike. It asserts
//! nothing.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_ART_PACK`: the pack's folder or packed file, with a `[[skin]]` for the body's setup
//!   (default: the tests' mannequin, smooth tubes over the human's limbs and trunk, made from its
//!   parts).
//! - `DERETH_SKIN_DUMP`: where the pictures go (required).
//! - `DERETH_SKIN_SUBJECTS`: a comma list of `stand`, `run`, `cast`, `walk` (default all).
//! - `DERETH_SKIN_MOMENTS`: how many moments a row (default 6), `DERETH_SKIN_GAP`: the frames
//!   between them (default 4, at thirty frames a second).
//! - `DERETH_SKIN_VIEW`: `front`, `side` or `back` (default front: ahead of the body and to its
//!   right).
//! - `DERETH_SKIN_CLOSE`: `knee`, `hip`, `elbow` or `shoulder`: a close camera at that joint's
//!   height, from the body's left (default none: the whole body).
//! - `DERETH_SKIN_FPS`: the frames a second it is stepped at (default 30).
//! - `DERETH_SKIN_SMOOTH`: `1` draws the body between its keyframes (default: snapped, as retail).
//! - `DERETH_SKIN_SEQUENCE`: `1` writes every frame of the moments' span, the parts on the left and
//!   the body on the right, as `<subject>/NNNN.png`, for a video.
//!
//! - `DERETH_SKIN_MOTION`: `1` turns the anti-aliasing on and adds, under each row of moments, the
//!   motion pass's picture: how far each pixel's surface moved since the last frame (dark: no
//!   motion drawn; black to red to yellow to white: 0 to 12 pixels and more). `compare` draws the
//!   skinned body twice, first with its vertices' last places withheld from the motion pass (it then
//!   moves only as its anchor part does), then given them.
//!
//! - `DERETH_SKIN_GPU`: `0` poses the bodies on the processor (default: on the device).
//!
//! It prints the time posing the bodies took, a body a frame (on the processor: all of it; on the
//! device: the joints worked out here), and the device's skinning pass's time a frame.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_animation::motion::MovementParameters;
use dereth_animation::MotionCommand;
use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{
    RemasterLight, RemasterPacks, RemasterPost, SceneReads, SceneWrites, WorldScene,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_primitives::num::math};

const W: u32 = 720;
const H: u32 = 720;
const TILE_W: u32 = 360;
const TILE_H: u32 = 600;

fn fps() -> f64 {
    env("DERETH_SKIN_FPS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.0)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// A run's moments, every frame of their span when a sequence is asked for, and the bodies
/// posed, their vertices and the nanoseconds posing them took.
type Run = (Vec<Vec<u8>>, Vec<Vec<u8>>, (u64, u64, u64), Vec<Vec<u8>>);

/// `moments` frames of `subject`, `gap` frames apart, with the pack's bodies drawn or not.
#[allow(clippy::too_many_lines)] // one run, set up, stepped and drawn
fn frames(
    store: &Arc<RetailDatStore>,
    pack: &Arc<ArtPack>,
    folder: Option<std::path::PathBuf>,
    subject: &str,
    bodies: bool,
    (moments, gap): (usize, usize),
    skin_motion: bool,
) -> Run {
    #[allow(non_snake_case)]
    let FPS = fps();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        time_of_day: Some(0.42),
        particles: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    scene.draw.set_remaster_pack(Some(Arc::clone(pack)), folder);
    let packs = RemasterPacks {
        characters: bodies,
        ..RemasterPacks::default()
    };
    scene.draw.set_remaster_packs(packs, &mut gpu);
    scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    scene.smooth_animation = env("DERETH_SKIN_SMOOTH").is_some_and(|v| v == "1");
    let motion = env("DERETH_SKIN_MOTION").is_some();
    if motion {
        scene.draw.set_remaster_post(
            RemasterPost {
                taa: true,
                ..RemasterPost::default()
            },
            &mut gpu,
        );
    }
    scene.draw.set_remaster_skin_motion(skin_motion, &mut gpu);
    // Posed on the device unless `DERETH_SKIN_GPU` is `0`.
    scene
        .draw
        .set_remaster_skin_gpu(env("DERETH_SKIN_GPU").is_none_or(|v| v != "0"), &mut gpu);
    let mut device_ms = (0.0f64, 0u32);
    let mut motions = Vec::new();
    let sequence = env("DERETH_SKIN_SEQUENCE").is_some_and(|v| v == "1");
    let close = env("DERETH_SKIN_CLOSE");
    let mut every = Vec::new();
    let mut stream = dereth_client_runtime::objects::ObjectStream::new();
    let mut input = CharacterInput::default();
    let params = MovementParameters::default();
    let view = env("DERETH_SKIN_VIEW").unwrap_or_else(|| "front".to_owned());
    let mut t = 1.0f64;
    let mut out = Vec::new();
    #[allow(clippy::cast_possible_truncation)]
    let dt = (1.0 / FPS) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let settle = (2.0 * FPS) as usize;
    let total = settle + 40 + moments * gap;
    for i in 0..total {
        t += 1.0 / FPS;
        if i == settle {
            let c = scene.character.as_mut().expect("a body");
            match subject {
                "run" => {
                    input.forward = true;
                    input.run = true;
                }
                "walk" => input.forward = true,
                "cast" => {
                    c.driver_mut()
                        .do_interpreted_motion(MotionCommand::MAGIC, &params);
                }
                _ => {}
            }
        }
        if subject == "cast" && i == settle + 30 {
            scene
                .character
                .as_mut()
                .expect("a body")
                .driver_mut()
                .do_interpreted_motion(MotionCommand::MAGIC_BLAST, &params);
        }
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(Default::default(), input, LocalTime(t), dt);
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            Default::default(),
            LocalTime(t),
            1.0 / FPS,
        );
        // A camera following the body: ahead of it and to its right, its side, or behind it.
        if let Some(c) = scene.character.as_ref() {
            let f = c.render_frame();
            let d = c.drawn_offset();
            let at = Vec3::new(f.origin.x + d.x, f.origin.y + d.y, f.origin.z + d.z);
            // The body's forward, its own second axis, turned about the upright axis.
            let m = dereth_primitives::frame::l2g(f.rotation).0;
            let turn = match (close.as_deref(), view.as_str()) {
                (Some(_), _) => -std::f32::consts::FRAC_PI_2 + 0.35,
                (_, "side") => std::f32::consts::FRAC_PI_2,
                (_, "back") => std::f32::consts::PI,
                _ => 0.6,
            };
            let (s, co) = (math::sinf(turn), math::cosf(turn));
            let len = (m[3] * m[3] + m[4] * m[4]).sqrt().max(1e-6);
            let (fx, fy) = (m[3] / len, m[4] / len);
            let (dx, dy) = (fx * co - fy * s, fx * s + fy * co);
            // A close camera looks at its joint's height, a little above it.
            let (dist, height) = match close.as_deref() {
                Some("knee") => (1.0, 0.5),
                Some("hip") => (1.1, 0.95),
                Some("elbow") => (0.95, 1.15),
                Some("shoulder") => (0.95, 1.4),
                _ => (2.3, 0.95),
            };
            let eye = Vec3::new(at.x + dx * dist, at.y + dy * dist, at.z + height + 0.15);
            scene.camera.position = eye;
            scene.camera.yaw = math::atan2f(-(at.x - eye.x), at.y - eye.y);
            scene.camera.pitch = math::atan2f(at.z + height - eye.z, dist);
        }
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        let shown = i + moments * gap >= total && (total - 1 - i).is_multiple_of(gap);
        if bodies {
            for (label, ms) in scene.draw.remaster_timings(&mut gpu) {
                if label == "skinning" {
                    device_ms.0 += ms;
                    device_ms.1 += 1;
                }
            }
        }
        if shown {
            out.push(tile(&gpu.capture().expect("capture").to_rgba()));
            if motion {
                if let Some(pic) = scene.draw.remaster_post_picture(&mut gpu) {
                    motions.push(motion_tile(pic.width, pic.height, &pic.motion, pic.jitter));
                }
            }
        }
        if sequence && i + moments * gap >= total {
            every.push(tile(&gpu.capture().expect("capture").to_rgba()));
        }
    }
    let counts = scene
        .draw
        .remaster_counts(&mut gpu)
        .map_or((0, 0, 0), |c| (c.skin_poses, c.skin_vertices, c.skin_nanos));
    if device_ms.1 > 0 {
        println!(
            "{subject}: the skinning pass took {:.3} ms on the device, a frame on average over {} frames",
            device_ms.0 / f64::from(device_ms.1),
            device_ms.1
        );
    }
    scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    scene.release_textures(&mut gpu);
    (out, every, counts, motions)
}

/// The middle tile of a motion picture as a heat map: how many pixels each surface moved since
/// the last frame, from its last place (clip space, as the passes read it back) to where it is
/// drawn, the frame's jitter taken off.
fn motion_tile(w: u32, h: u32, motion: &[[f32; 4]], jitter: [f32; 2]) -> Vec<u8> {
    #[allow(clippy::cast_precision_loss)]
    let (wf, hf) = (w as f32, h as f32);
    let mut rgba = Vec::with_capacity(motion.len() * 4);
    for (i, m) in motion.iter().enumerate() {
        if m[3] < 0.5 {
            rgba.extend([24, 24, 24, 255]);
            continue;
        }
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
        let ndc = [
            x / wf * 2.0 - 1.0 - jitter[0],
            1.0 - y / hf * 2.0 - jitter[1],
        ];
        let dx = (m[0] - ndc[0]) * wf * 0.5;
        let dy = (m[1] - ndc[1]) * hf * 0.5;
        let t = ((dx * dx + dy * dy).sqrt() / 12.0).clamp(0.0, 1.0) * 3.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0) as u8;
        rgba.extend([c(t), c(t - 1.0), c(t - 2.0), 255]);
    }
    if (w, h) == (W, H) {
        tile(&rgba)
    } else {
        rgba
    }
}

/// The middle `TILE_W` by `TILE_H` of a frame.
fn tile(rgba: &[u8]) -> Vec<u8> {
    let (x0, y0) = ((W - TILE_W) / 2, (H - TILE_H) / 2);
    let mut out = Vec::with_capacity((TILE_W * TILE_H * 4) as usize);
    for y in y0..y0 + TILE_H {
        let row = ((y * W + x0) * 4) as usize;
        out.extend_from_slice(&rgba[row..row + (TILE_W * 4) as usize]);
    }
    out
}

fn write_strip(path: &std::path::Path, rows: &[Vec<Vec<u8>>]) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (TILE_W * cols as u32, TILE_H * rows.len() as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (r, row) in rows.iter().enumerate() {
        for (c, t) in row.iter().enumerate() {
            for y in 0..TILE_H as usize {
                let src = y * TILE_W as usize * 4;
                let dst = ((r * TILE_H as usize + y) * w as usize + c * TILE_W as usize) * 4;
                rgba[dst..dst + TILE_W as usize * 4]
                    .copy_from_slice(&t[src..src + TILE_W as usize * 4]);
            }
        }
    }
    super::stations::write_png(path, w, h, &rgba);
    eprintln!("wrote {}", path.display());
}

#[test]
#[ignore = "instrument: draws a skinned body beside its parts and writes PNGs; run with --ignored"]
fn draw_a_skinned_body_beside_its_parts() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    // The pack named, or the tests' mannequin, made from the human's parts.
    let (pack, folder) = match env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
        Some(path) => (
            Arc::new(ArtPack::open_path(&path).expect("the pack opens")),
            path.is_dir().then_some(path),
        ),
        None => {
            use crate::rendering::remaster_skins::{in_memory, mannequin};
            (in_memory(&mannequin(&store)), None)
        }
    };
    println!(
        "{}: {} bodies; {} problems {:?}",
        pack.name,
        pack.skins.len(),
        pack.problems.len(),
        pack.problems
    );
    let dir = std::path::PathBuf::from(env("DERETH_SKIN_DUMP").expect("DERETH_SKIN_DUMP"));
    std::fs::create_dir_all(&dir).expect("the dump folder");
    let moments = env("DERETH_SKIN_MOMENTS")
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    let gap = env("DERETH_SKIN_GAP")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let subjects = env("DERETH_SKIN_SUBJECTS").unwrap_or_else(|| "stand,run,cast,walk".to_owned());
    let close = env("DERETH_SKIN_CLOSE").map_or(String::new(), |c| format!("-{c}"));
    for subject in subjects.split(',').map(str::trim) {
        let (parts, parts_every, _, parts_motion) = frames(
            &store,
            &pack,
            folder.clone(),
            subject,
            false,
            (moments, gap),
            true,
        );
        let compare = env("DERETH_SKIN_MOTION").is_some_and(|v| v == "compare");
        let withheld = compare.then(|| {
            frames(
                &store,
                &pack,
                folder.clone(),
                subject,
                true,
                (moments, gap),
                false,
            )
        });
        let (skinned, skinned_every, (poses, vertices, nanos), skinned_motion) = frames(
            &store,
            &pack,
            folder.clone(),
            subject,
            true,
            (moments, gap),
            true,
        );
        let mut rows = vec![parts];
        if !parts_motion.is_empty() {
            rows.push(parts_motion);
        }
        if let Some((w, _, _, wm)) = withheld {
            rows.push(w);
            rows.push(wm);
        }
        rows.push(skinned);
        if !skinned_motion.is_empty() {
            rows.push(skinned_motion);
        }
        write_strip(&dir.join(format!("{subject}{close}.png")), &rows);
        if poses > 0 {
            #[allow(clippy::cast_precision_loss)]
            let each = nanos as f64 / poses as f64 / 1e6;
            println!(
                "{subject}: {poses} bodies posed, {} vertices each, {each:.3} ms a body on the processor",
                vertices / poses
            );
        }
        if !parts_every.is_empty() {
            let seq = dir.join(subject);
            std::fs::create_dir_all(&seq).expect("the sequence folder");
            for (k, (a, b)) in parts_every.iter().zip(&skinned_every).enumerate() {
                write_strip(
                    &seq.join(format!("{k:04}.png")),
                    &[vec![a.clone(), b.clone()]],
                );
            }
        }
    }
}
