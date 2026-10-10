//! Frames of bodies moving past a still camera, drawn where physics has them and between their
//! physics ticks: a player running past, a drudge running after him, an arrow flying, and the
//! player teleported back along his way by the server half way through, at 240 frames a second.
//! Each subject is written to `DERETH_TEST_SMOOTH_MOVEMENT_DUMP` (default: `smooth-movement/` in
//! cargo's scratch folder for integration tests): every frame of both rows as its own PNG
//! (`<subject>/stepping-NNN.png` and `<subject>/smooth-NNN.png`), one PNG of two rows of every
//! fourth frame of the first ninety-six, stepping above and smooth below, and a row a frame in
//! `movement.tsv` there (where the subject is drawn in each row, and how far that is from the
//! frame before). The local body is drawn between ticks in both rows, as the Horizon interface
//! draws it. They assert nothing; the claims are the drawn world's own tests.

#![cfg(gpu)]

use std::io::Write;
use std::sync::Arc;

use dereth_animation::motion::interp::InterpretedMotionState;
use dereth_animation::MotionCommand;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{Frame, LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::types::{physicsdesc::flags, PhysicsDesc};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 800;
const H: u32 = 600;
/// The part of each frame kept: the middle of the picture, where the subject passes.
const TILE_W: u32 = 640;
const TILE_H: u32 = 300;
const FPS: f64 = 240.0;
/// Consecutive frames kept: half a second, fifteen physics ticks.
const SHOWN: usize = 120;
/// Every this many frames goes into the strip, and how many go in.
const STRIP_EVERY: usize = 4;
const STRIP_TILES: usize = 24;

/// A player, as the server makes one, and his motions; a drudge and its motions; an arrow's setup,
/// flown as a spell bolt flies (no motion table, a velocity, the missile state word, no gravity).
const HUMAN: u32 = 0x0200_0001;
const HUMAN_MOTIONS: u32 = 0x0900_0001;
const DRUDGE: u32 = 0x0200_07DD;
const DRUDGE_MOTIONS: u32 = 0x0900_0008;
const ARROW: u32 = 0x0200_0124;
const BOLT_STATE: u32 = 0x0002_8B48;
const CREATURE_STATE: u32 = 0x0000_0408;

const RUNNER: ObjectId = ObjectId(0x8300_0310);
const CHASER: ObjectId = ObjectId(0x8300_0311);
const BOLT: ObjectId = ObjectId(0x8300_0312);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Running,
    Chasing,
    Arrow,
    Teleport,
}

impl Subject {
    const fn name(self) -> &'static str {
        match self {
            Self::Running => "player-running-past",
            Self::Chasing => "drudge-chasing",
            Self::Arrow => "arrow-flying",
            Self::Teleport => "player-teleported",
        }
    }

    const fn id(self) -> ObjectId {
        match self {
            Self::Running | Self::Teleport => RUNNER,
            Self::Chasing => CHASER,
            Self::Arrow => BOLT,
        }
    }

    /// Where the camera stands, in the body's own axes (right, ahead, up), looking left across
    /// the subject's way.
    const fn camera(self) -> (f32, f32, f32) {
        match self {
            Self::Running | Self::Teleport => (10.0, 4.0, 1.4),
            Self::Chasing => (10.0, -3.0, 1.2),
            Self::Arrow => (14.0, -3.0, 1.8),
        }
    }
}

fn out_dir() -> String {
    std::env::var("DERETH_TEST_SMOOTH_MOVEMENT_DUMP")
        .unwrap_or_else(|_| format!("{}/smooth-movement", env!("CARGO_TARGET_TMPDIR")))
}

fn event(stream: &mut ObjectStream, opcode: dereth_protocol::Opcode, body: Vec<u8>, now: f64) {
    stream.apply_event(&SessionEvent::WorldObject { opcode, body }, LocalTime(now));
}

fn create(
    stream: &mut ObjectStream,
    (id, setup, mtable): (ObjectId, u32, Option<u32>),
    at: Position,
    velocity: Option<Vec3>,
    state: u32,
    now: f64,
) {
    let mut bitfield = flags::POSITION | flags::SETUP;
    if mtable.is_some() {
        bitfield |= flags::MTABLE;
    }
    if velocity.is_some() {
        bitfield |= flags::VELOCITY;
    }
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id,
        physicsdesc: PhysicsDesc {
            bitfield,
            state,
            setup_id: Some(setup),
            mtable_id: mtable,
            velocity: velocity.map(Into::into),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: at.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
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
    event(
        stream,
        dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
        body,
        now,
    );
    stream.world.update_visible_object_list();
}

/// The server teleporting `id` to `to`: a `Movement_PositionEvent` (`0xF748`) with a newer
/// teleport stamp.
fn teleport(stream: &mut ObjectStream, id: ObjectId, to: Position, now: f64) {
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    let body = dereth_protocol::write_body(&MovementPositionEvent {
        id,
        position: PositionPack {
            flags: position_flags::IS_GROUNDED,
            origin: dereth_protocol::types::Origin {
                objcell_id: to.cell.0,
                origin: to.frame.origin.into(),
            },
            orientation: to.frame.rotation.into(),
            instance_timestamp: 1,
            position_timestamp: 1,
            teleport_timestamp: 1,
            ..PositionPack::default()
        },
    })
    .expect("encode");
    event(
        stream,
        dereth_protocol::Opcode::MOVEMENT_POSITION_EVENT,
        body,
        now,
    );
}

/// `SHOWN` consecutive frames of `subject`, drawn between ticks (`smooth`) or where physics has
/// it, each the middle of the picture, and where the subject was drawn on each.
fn frames(store: &Arc<RetailDatStore>, subject: Subject, smooth: bool) -> Vec<(Vec<u8>, Vec3)> {
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
    scene.smooth_movement = smooth;
    scene
        .character
        .as_mut()
        .expect("a body")
        .drawn_between_ticks = true;
    let mut stream = ObjectStream::new();
    let mut t = 1.0f64;
    let mut out = Vec::new();
    #[allow(clippy::cast_possible_truncation)]
    let dt = (1.0 / FPS) as f32;
    // Settled at 120, the movers made; set going at 121; the frames kept from 180, and the
    // teleport half way through them.
    let (made, going, kept) = (120, 121, 180);
    let total = kept + SHOWN;
    let mut axes = None;
    for i in 0..total {
        t += 1.0 / FPS;
        if i == made {
            let at = scene.character.as_ref().expect("a body").position();
            let m = dereth_primitives::frame::l2g(at.frame.rotation);
            axes = Some((at, m));
            let along = |right: f32, ahead: f32, up: f32| {
                let v = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(right, ahead, up));
                let o = at.frame.origin;
                Position::new(
                    at.cell,
                    Frame::new(
                        Vec3::new(o.x + v.x, o.y + v.y, o.z + v.z),
                        at.frame.rotation,
                    ),
                )
            };
            let flight = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(0.0, 15.0, 0.0));
            create(
                &mut stream,
                (RUNNER, HUMAN, Some(HUMAN_MOTIONS)),
                along(3.0, 0.0, 0.0),
                None,
                CREATURE_STATE,
                t,
            );
            create(
                &mut stream,
                (CHASER, DRUDGE, Some(DRUDGE_MOTIONS)),
                along(3.0, -6.0, 0.0),
                None,
                CREATURE_STATE,
                t,
            );
            if subject == Subject::Arrow {
                create(
                    &mut stream,
                    (BOLT, ARROW, None),
                    along(5.0, -12.0, 1.5),
                    Some(flight),
                    BOLT_STATE,
                    t,
                );
            }
        }
        if i == going {
            let run = |scene: &WorldScene, id: ObjectId, state: &InterpretedMotionState| {
                if let Some(o) = scene.objects.get(&id) {
                    o.sim
                        .driver
                        .borrow_mut()
                        .move_to_interpreted_state(state, false);
                }
            };
            run(
                &scene,
                RUNNER,
                &InterpretedMotionState {
                    forward_command: MotionCommand::RUN_FORWARD,
                    forward_speed: 2.5,
                    ..InterpretedMotionState::default()
                },
            );
            run(
                &scene,
                CHASER,
                &InterpretedMotionState {
                    forward_command: MotionCommand::RUN_FORWARD,
                    forward_speed: 1.5,
                    turn_command: MotionCommand::TURN_RIGHT,
                    turn_speed: 0.2,
                    ..InterpretedMotionState::default()
                },
            );
        }
        if subject == Subject::Teleport && i == kept + SHOWN / 2 {
            if let Some(p) = scene.objects.get(&RUNNER).and_then(|o| o.sim.position) {
                let m = dereth_primitives::frame::l2g(p.frame.rotation);
                let back = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(0.0, -3.0, 0.0));
                let o = p.frame.origin;
                let to = Position::new(
                    p.cell,
                    Frame::new(
                        Vec3::new(o.x + back.x, o.y + back.y, o.z + back.z),
                        p.frame.rotation,
                    ),
                );
                teleport(&mut stream, RUNNER, to, t);
            }
        }
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        if let Some(c) = scene.character.as_mut() {
            stream.sync_physics_at(store, &mut c.world, LocalTime(t));
        }
        scene.update(Default::default(), Default::default(), LocalTime(t), dt);
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            Default::default(),
            LocalTime(t),
            1.0 / FPS,
        );
        // A still camera at the side of the subject's way, looking across it.
        if let Some((at, m)) = axes {
            let (right, ahead, up) = subject.camera();
            let v = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(right, ahead, up));
            let body = scene
                .character
                .as_ref()
                .expect("a body")
                .render_frame_of(at)
                .origin;
            scene.camera.position = Vec3::new(body.x + v.x, body.y + v.y, body.z + v.z);
            let left = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(-1.0, 0.0, 0.0));
            scene.camera.yaw = dereth_primitives::num::math::atan2f(-left.x, left.y);
            scene.camera.pitch = -0.05;
        }
        scene.stream(store, &mut gpu).expect("stream");
        if i >= kept {
            scene
                .reserve_upload_arena(&mut gpu)
                .expect("reserve the arena");
            gpu.begin_frame().expect("begin");
            scene.draw(&mut gpu).expect("draw");
            gpu.end_frame().expect("end");
            let drawn = scene
                .objects
                .get(&subject.id())
                .map_or(Vec3::ZERO, |o| o.frame.origin);
            out.push((tile(&gpu.capture().expect("capture").to_rgba()), drawn));
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

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png header");
    wr.write_image_data(rgba).expect("png data");
}

/// Two rows of tiles, one image.
fn write_strip(path: &str, rows: &[Vec<&Vec<u8>>]) {
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
    write_png(path, w, h, &rgba);
    eprintln!("wrote {path}");
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_SMOOTH_MOVEMENT_DUMP set and --ignored"]
fn render_the_smooth_movement_frames() {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the dump folder");
    let store = crate::common::dats();
    let mut tsv = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{dir}/movement.tsv"))
        .expect("the measurements file");
    for subject in [
        Subject::Running,
        Subject::Chasing,
        Subject::Arrow,
        Subject::Teleport,
    ] {
        let stepping = frames(&store, subject, false);
        let smooth = frames(&store, subject, true);
        let sub = format!("{dir}/{}", subject.name());
        std::fs::create_dir_all(&sub).expect("the subject's folder");
        let step = |a: Vec3, b: Vec3| Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z).magnitude();
        for (n, ((a, pa), (b, pb))) in stepping.iter().zip(&smooth).enumerate() {
            write_png(&format!("{sub}/stepping-{n:03}.png"), TILE_W, TILE_H, a);
            write_png(&format!("{sub}/smooth-{n:03}.png"), TILE_W, TILE_H, b);
            let (sa, sb) = if n == 0 {
                (0.0, 0.0)
            } else {
                (step(stepping[n - 1].1, *pa), step(smooth[n - 1].1, *pb))
            };
            writeln!(
                tsv,
                "{}\t{n}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{sa:.4}\t{sb:.4}",
                subject.name(),
                pa.x,
                pa.y,
                pa.z,
                pb.x,
                pb.y,
                pb.z
            )
            .expect("a row");
        }
        write_strip(
            &format!("{dir}/{}.png", subject.name()),
            &[strip_tiles(&stepping), strip_tiles(&smooth)],
        );
    }
}

/// The tiles of `frames` a strip shows.
fn strip_tiles(frames: &[(Vec<u8>, Vec3)]) -> Vec<&Vec<u8>> {
    frames
        .iter()
        .step_by(STRIP_EVERY)
        .take(STRIP_TILES)
        .map(|(t, _)| t)
        .collect()
}
