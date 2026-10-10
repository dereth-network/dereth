//! Frame strips of bodies in motion drawn at their animations' keyframes and between them: a
//! body walking, running, turning and casting, and a drudge swinging at the air, each followed by
//! a camera at its side at 240 frames a second. Each subject is written to
//! `DERETH_TEST_SMOOTH_ANIMATION_DUMP` (default: `smooth-animation/` in cargo's scratch folder for
//! integration tests) as one PNG of two rows of consecutive frames, at keyframes above and between
//! them below, with one row of measurements a frame (how much of the picture changed since the
//! frame before, in each row) appended to `strips.tsv` there. The body is drawn between physics
//! ticks in both rows, as the Horizon interface draws it. They assert nothing; the claims are the
//! drawn world's own tests.

#![cfg(gpu)]

use std::io::Write;
use std::sync::Arc;

use dereth_animation::motion::MovementParameters;
use dereth_animation::MotionCommand;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, PhysicsDesc};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 800;
const H: u32 = 600;
/// Each frame of a strip: the middle of the picture, where the subject stands.
const TILE_W: u32 = 320;
const TILE_H: u32 = 520;
const FPS: f64 = 240.0;
/// Consecutive frames shown in a strip: a quarter of a second, seven or eight physics ticks.
const SHOWN: usize = 60;

/// A drudge, swinging at the air, and the motions the server gives one.
const DRUDGE: u32 = 0x0200_07DD;
const DRUDGE_MOTIONS: u32 = 0x0900_0008;
const DRUDGE_ID: ObjectId = ObjectId(0x8300_0200);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Walk,
    Run,
    Turn,
    Cast,
    Attack,
}

impl Subject {
    const fn name(self) -> &'static str {
        match self {
            Self::Walk => "walking",
            Self::Run => "running",
            Self::Turn => "turning",
            Self::Cast => "casting",
            Self::Attack => "drudge-attacking",
        }
    }
}

fn out_dir() -> String {
    std::env::var("DERETH_TEST_SMOOTH_ANIMATION_DUMP")
        .unwrap_or_else(|_| format!("{}/smooth-animation", env!("CARGO_TARGET_TMPDIR")))
}

fn create_drudge(stream: &mut ObjectStream, cell: u32, at: Vec3, now: f64) {
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: DRUDGE_ID,
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP | flags::MTABLE,
            setup_id: Some(DRUDGE),
            mtable_id: Some(DRUDGE_MOTIONS),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: cell,
                frame: dereth_protocol::types::Frame {
                    origin: at.into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance: 1,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        },
        ..Default::default()
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("encode");
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    stream.world.update_visible_object_list();
}

/// `SHOWN` consecutive frames of `subject`, drawn between keyframes (`smooth`) or at them, each
/// the middle of the picture.
fn frames(store: &Arc<RetailDatStore>, subject: Subject, smooth: bool) -> Vec<Vec<u8>> {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        time_of_day: Some(0.5),
        particles: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    scene.smooth_animation = smooth;
    scene
        .character
        .as_mut()
        .expect("a body")
        .drawn_between_ticks = true;
    let mut stream = ObjectStream::new();
    let mut input = CharacterInput::default();
    let params = MovementParameters::default();
    let mut t = 1.0f64;
    let mut out = Vec::new();
    #[allow(clippy::cast_possible_truncation)]
    let dt = (1.0 / FPS) as f32;
    let total = 480 + SHOWN;
    for i in 0..total {
        t += 1.0 / FPS;
        // Settled, the subject is set going; a second and a half later, the frames are taken.
        if i == 120 {
            let c = scene.character.as_mut().expect("a body");
            match subject {
                Subject::Walk => input.forward = true,
                Subject::Run => {
                    input.forward = true;
                    input.run = true;
                }
                Subject::Turn => input.turn_right = true,
                Subject::Cast => {
                    c.driver_mut()
                        .do_interpreted_motion(MotionCommand::MAGIC, &params);
                }
                Subject::Attack => {
                    let at = c.position();
                    create_drudge(
                        &mut stream,
                        at.cell.0,
                        Vec3::new(
                            at.frame.origin.x + 3.0,
                            at.frame.origin.y,
                            at.frame.origin.z,
                        ),
                        t,
                    );
                }
            }
        }
        if subject == Subject::Attack && i == 160 {
            scene
                .sync_objects(store, &mut gpu, &mut stream)
                .expect("sync_objects");
            if let Some(o) = scene.objects.get(&DRUDGE_ID) {
                o.sim
                    .driver
                    .borrow_mut()
                    .do_interpreted_motion(MotionCommand::HAND_COMBAT, &params);
            }
        }
        // The swing and the spell, begun a moment before the frames are taken.
        if i == total - SHOWN - 40 {
            match subject {
                Subject::Cast => {
                    scene
                        .character
                        .as_mut()
                        .expect("a body")
                        .driver_mut()
                        .do_interpreted_motion(MotionCommand::MAGIC_BLAST, &params);
                }
                Subject::Attack => {
                    if let Some(o) = scene.objects.get(&DRUDGE_ID) {
                        o.sim
                            .driver
                            .borrow_mut()
                            .do_interpreted_motion(MotionCommand::ATTACK_HIGH1, &params);
                    }
                }
                _ => {}
            }
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
        // A camera at the subject's side, east of it looking west, following it as it is drawn.
        let at = match subject {
            Subject::Attack => scene.objects.get(&DRUDGE_ID).map(|o| o.frame.origin),
            _ => scene.character.as_ref().map(|c| {
                let o = c.render_frame().origin;
                let d = c.drawn_offset();
                Vec3::new(o.x + d.x, o.y + d.y, o.z + d.z)
            }),
        };
        if let Some(at) = at {
            scene.camera.position = Vec3::new(at.x + 2.4, at.y, at.z + 1.0);
            scene.camera.yaw = std::f32::consts::FRAC_PI_2;
            scene.camera.pitch = -0.08;
        }
        scene.stream(store, &mut gpu).expect("stream");
        if i + SHOWN >= total {
            scene
                .reserve_upload_arena(&mut gpu)
                .expect("reserve the arena");
            gpu.begin_frame().expect("begin");
            scene.draw(&mut gpu).expect("draw");
            gpu.end_frame().expect("end");
            out.push(tile(&gpu.capture().expect("capture").to_rgba()));
        }
    }
    scene.release_textures(&mut gpu);
    out
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

/// How much of the picture changed from one frame to the next: the mean absolute difference of
/// its colour channels, out of 255.
fn change(a: &[u8], b: &[u8]) -> f64 {
    let sum: u64 = a
        .iter()
        .zip(b)
        .enumerate()
        .filter(|(i, _)| i % 4 != 3)
        .map(|(_, (x, y))| u64::from(x.abs_diff(*y)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let n = (a.len() / 4 * 3) as f64;
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / n;
    mean
}

fn write_strip(path: &str, rows: &[Vec<Vec<u8>>]) {
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
    let f = std::fs::File::create(path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png header");
    wr.write_image_data(&rgba).expect("png data");
    eprintln!("wrote {path}");
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_SMOOTH_ANIMATION_DUMP set and --ignored"]
fn render_the_smooth_animation_strips() {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the dump folder");
    let store = crate::common::dats();
    let mut tsv = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{dir}/strips.tsv"))
        .expect("the measurements file");
    for subject in [
        Subject::Walk,
        Subject::Run,
        Subject::Turn,
        Subject::Cast,
        Subject::Attack,
    ] {
        let at = frames(&store, subject, false);
        let between = frames(&store, subject, true);
        for (n, (a, b)) in at.windows(2).zip(between.windows(2)).enumerate() {
            writeln!(
                tsv,
                "{}\t{}\t{:.3}\t{:.3}",
                subject.name(),
                n + 1,
                change(&a[0], &a[1]),
                change(&b[0], &b[1])
            )
            .expect("a row");
        }
        write_strip(&format!("{dir}/{}.png", subject.name()), &[at, between]);
    }
}
