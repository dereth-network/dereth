//! Frames of the Horizon interface's names over the people and creatures they name, made in the
//! client's own frame order: the interface's frame first, from the world as the last frame left
//! it; then the world's step and the camera's; then what the interface drew over the world moved
//! to where the world is drawn on this frame; then the world drawn with the interface over it. Two
//! stations: a row of bodies of different heights standing before a still camera (a person, a
//! drudge, a lugian, a lugian made larger, a tusker, a rat and a chest), and a person running past
//! far away while the camera turns the other way. Each is written to `DERETH_TEST_NAMEPLATE_DUMP`
//! (default: `nameplates/` in cargo's scratch folder for integration tests): the row as one PNG,
//! the runner as every frame's PNG and one strip of them, and `names.tsv`, a row a frame: where the
//! runner's name stands, and where the top of his body is drawn on that frame. They assert
//! nothing; the claims are the interface's and the drawn world's own tests.

#![cfg(gpu)]

use std::io::Write;
use std::sync::Arc;

use dereth_animation::motion::interp::InterpretedMotionState;
use dereth_animation::MotionCommand;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_horizon::draw::DrawList;
use dereth_horizon::ui::game::{BlipKind, GameState, Nameplate};
use dereth_primitives::{Frame, LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::types::{physicsdesc::flags, PhysicsDesc};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 1280;
const H: u32 = 720;
const FPS: f64 = 60.0;
/// The runner's frames kept, and every this many of them in the strip.
const SHOWN: usize = 24;
const STRIP_EVERY: usize = 3;
/// The part of a runner's frame kept: the middle of the picture, where the runner passes.
const TILE_W: u32 = 480;
const TILE_H: u32 = 240;

/// A creature's state word: solid, and falling to the ground.
const CREATURE_STATE: u32 = 0x0000_0408;

/// One body the names are drawn over: what it is, how it is made, and where it stands from the
/// player, in the player's own axes (right, ahead).
struct Subject {
    id: ObjectId,
    name: &'static str,
    kind: BlipKind,
    setup: u32,
    mtable: Option<u32>,
    scale: Option<f32>,
    at: (f32, f32),
}

const ROW: [Subject; 7] = [
    Subject {
        id: ObjectId(0x8300_0401),
        name: "Aluvian",
        kind: BlipKind::Player,
        setup: 0x0200_0001,
        mtable: Some(0x0900_0001),
        scale: None,
        at: (-5.0, 13.0),
    },
    Subject {
        id: ObjectId(0x8300_0402),
        name: "Drudge Skulker",
        kind: BlipKind::Creature,
        setup: 0x0200_07DD,
        mtable: Some(0x0900_0008),
        scale: None,
        at: (-3.2, 13.0),
    },
    Subject {
        id: ObjectId(0x8300_0403),
        name: "Lugian",
        kind: BlipKind::Npc,
        setup: 0x0200_0A0B,
        mtable: Some(0x0900_0006),
        scale: None,
        at: (-1.2, 13.0),
    },
    Subject {
        id: ObjectId(0x8300_0404),
        name: "Gotrok Lugian",
        kind: BlipKind::Creature,
        setup: 0x0200_0A0B,
        mtable: Some(0x0900_0006),
        scale: Some(1.3),
        at: (1.6, 13.0),
    },
    Subject {
        id: ObjectId(0x8300_0405),
        name: "Tusker",
        kind: BlipKind::Creature,
        setup: 0x0200_0964,
        mtable: Some(0x0900_000C),
        scale: None,
        at: (4.4, 14.0),
    },
    Subject {
        id: ObjectId(0x8300_0406),
        name: "Rat",
        kind: BlipKind::Creature,
        setup: 0x0200_003D,
        mtable: Some(0x0900_000E),
        scale: None,
        at: (6.4, 13.0),
    },
    Subject {
        id: ObjectId(0x8300_0407),
        name: "Chest",
        kind: BlipKind::Portal,
        setup: 0x0200_007C,
        mtable: None,
        scale: None,
        at: (8.0, 13.0),
    },
];

/// A person running across the view far away, and his motions.
const RUNNER: Subject = Subject {
    id: ObjectId(0x8300_0410),
    name: "Runner",
    kind: BlipKind::Player,
    setup: 0x0200_0001,
    mtable: Some(0x0900_0001),
    scale: None,
    at: (1.0, 20.0),
};

fn out_dir() -> String {
    std::env::var("DERETH_TEST_NAMEPLATE_DUMP")
        .unwrap_or_else(|_| format!("{}/nameplates", env!("CARGO_TARGET_TMPDIR")))
}

/// The server making `s` at `at`.
fn create(stream: &mut ObjectStream, s: &Subject, at: Position, now: f64) {
    let mut bitfield = flags::POSITION | flags::SETUP;
    if s.mtable.is_some() {
        bitfield |= flags::MTABLE;
    }
    if s.scale.is_some() {
        bitfield |= flags::OBJSCALE;
    }
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: s.id,
        physicsdesc: PhysicsDesc {
            bitfield,
            state: CREATURE_STATE,
            setup_id: Some(s.setup),
            mtable_id: s.mtable,
            object_scale: s.scale,
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
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    stream.world.update_visible_object_list();
}

/// Where the interface places `s`'s name from the world as it stands, as the shell gives it the
/// world: on the top of its body as drawn, among the objects whose box is on screen.
fn anchor(scene: &WorldScene, id: ObjectId) -> Option<(f32, f32, dereth_horizon::draw::Rect)> {
    use dereth_client_contract::target::Projection;
    let Projection::OnScreen((x0, y0, x1, y1)) = scene.target_projection(id, (W, H))? else {
        return None;
    };
    let (x, y) = scene.target_top(id, (W, H))?;
    #[allow(clippy::cast_precision_loss)]
    let bounds = dereth_horizon::draw::Rect::new(
        x0 as f32,
        y0 as f32,
        (x1 - x0).max(0) as f32,
        (y1 - y0).max(0) as f32,
    );
    Some((x, y, bounds))
}

/// The names of `subjects` as the interface's frame has them, from the world as it stands.
fn nameplates(scene: &WorldScene, subjects: &[&Subject]) -> Vec<Nameplate> {
    let eye = scene.camera.frame().origin;
    subjects
        .iter()
        .filter_map(|s| {
            let (x, y, bounds) = anchor(scene, s.id)?;
            let o = scene.objects.get(&s.id)?.frame.origin;
            let d = Vec3::new(o.x - eye.x, o.y - eye.y, o.z - eye.z).magnitude();
            Some(Nameplate {
                id: s.id,
                name: s.name.to_owned(),
                x,
                y,
                kind: s.kind,
                selected: false,
                bounds,
                distance: Some(d),
                nearness: dereth_horizon::state::nearness(d),
            })
        })
        .collect()
}

/// What the interface draws for `state` this frame.
struct Interface {
    art: Arc<dereth_horizon::art::Art>,
    ui: dereth_horizon::ui::HorizonUi,
    list: DrawList,
    input: dereth_horizon::ui::input::InputFrame,
}

impl Interface {
    fn new() -> Self {
        let art = Arc::new(dereth_horizon::art::Art::new(Arc::new(
            dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
        )));
        Self {
            ui: dereth_horizon::ui::HorizonUi::new(Arc::clone(&art), Default::default()),
            art,
            list: DrawList::default(),
            input: Default::default(),
        }
    }

    fn frame(&mut self, nameplates: Vec<Nameplate>) {
        let state = GameState {
            in_world: true,
            connected: true,
            host: "instrument".into(),
            name: "Tester".into(),
            nameplates,
            radar_range: 75.0,
            ..GameState::default()
        };
        #[allow(clippy::cast_precision_loss)]
        self.ui.frame(
            &mut self.list,
            (W as f32, H as f32),
            1.0 / FPS,
            &state,
            &mut self.input,
        );
        self.input.next_frame();
    }

    /// What stands over the world moved to where the world is drawn this frame, as the shell
    /// moves it before the overlay is made.
    fn follow(&mut self, scene: &WorldScene) {
        self.list.follow(|id, anchor| match anchor {
            dereth_horizon::draw::Anchor::Top => scene.target_top(id, (W, H)),
            dereth_horizon::draw::Anchor::Origin =>
            {
                #[allow(clippy::cast_precision_loss)]
                scene
                    .target_origin(id, (W, H))
                    .map(|(x, y)| (x as f32, y as f32))
            }
        });
    }

    /// Where the name of `id` stands on, as the list has it now.
    fn placed(&self, id: ObjectId) -> Option<(f32, f32)> {
        self.list.anchored.iter().find(|a| a.id == id).map(|a| a.at)
    }

    /// The interface's quads laid over `rgba`, the drawn world, as the overlay blends them.
    fn over(&self, rgba: &mut [u8]) {
        #[allow(clippy::cast_precision_loss)]
        let screen = dereth_horizon::draw::Rect::new(0.0, 0.0, W as f32, H as f32);
        for q in &self.list.quads {
            if q.turn != 0.0 {
                continue;
            }
            let Some(r) = q
                .clip
                .map_or(Some(q.dst), |c| q.dst.intersect(&c))
                .and_then(|r| r.intersect(&screen))
            else {
                continue;
            };
            let image = q.tex.and_then(|t| self.art.image(t));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for py in (r.y.floor() as u32)..(r.bottom().ceil() as u32).min(H) {
                for px in (r.x.floor() as u32)..(r.right().ceil() as u32).min(W) {
                    #[allow(clippy::cast_precision_loss)]
                    let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                    if !r.contains(cx, cy) {
                        continue;
                    }
                    let [b, g, rr, a] = match &image {
                        Some(im) => {
                            let u = q.src.x + (cx - q.dst.x) * q.src.w / q.dst.w;
                            let v = q.src.y + (cy - q.dst.y) * q.src.h / q.dst.h;
                            im.pixel(u.max(0.0) as u32, v.max(0.0) as u32)
                        }
                        None => [255; 4],
                    };
                    let c = q.colour;
                    let ch = |s: u8, shift: u32| u32::from(s) * ((c >> shift) & 0xFF) / 255;
                    let (sr, sg, sb, sa) = (ch(rr, 16), ch(g, 8), ch(b, 0), ch(a, 24));
                    let i = ((py * W + px) * 4) as usize;
                    for (k, s) in [sr, sg, sb].into_iter().enumerate() {
                        let d = u32::from(rgba[i + k]);
                        rgba[i + k] = ((s * sa + d * (255 - sa)) / 255) as u8;
                    }
                }
            }
        }
    }
}

/// The scene at Holtburg with the player's body, and the stream its objects come from.
fn scene(store: &Arc<RetailDatStore>, gpu: &mut dereth_render::device::Gpu) -> WorldScene {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        time_of_day: Some(0.45),
        particles: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// A place `(right, ahead)` from `at` in its own axes, facing its way turned `turn` radians to
/// the right.
fn along(at: &Position, (right, ahead): (f32, f32), turn: f32) -> Position {
    let m = dereth_primitives::frame::l2g(at.frame.rotation);
    let v = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(right, ahead, 0.0));
    let o = at.frame.origin;
    // A little above, to fall to the ground wherever the ground is.
    let mut f = Frame::new(
        Vec3::new(o.x + v.x, o.y + v.y, o.z + 2.0),
        at.frame.rotation,
    );
    let heading = dereth_primitives::frame::get_heading(&f);
    dereth_primitives::frame::set_heading(&mut f, heading + turn.to_degrees());
    Position::new(at.cell, f)
}

/// Which way the stations look, in degrees clockwise of the way the body faces: over open
/// ground, clear of the town's houses.
const VIEW: f32 = 90.0;

/// `at` turned `degrees` clockwise.
fn turned(mut at: Position, degrees: f32) -> Position {
    let heading = dereth_primitives::frame::get_heading(&at.frame);
    dereth_primitives::frame::set_heading(&mut at.frame, heading + degrees);
    at
}

/// One frame of the world, the camera placed by `camera` after the camera's own step.
fn step(
    store: &Arc<RetailDatStore>,
    gpu: &mut dereth_render::device::Gpu,
    scene: &mut WorldScene,
    stream: &mut ObjectStream,
    t: f64,
    camera: impl FnOnce(&mut WorldScene),
) {
    scene
        .sync_objects(store, gpu, stream)
        .expect("sync_objects");
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics_at(store, &mut c.world, LocalTime(t));
    }
    #[allow(clippy::cast_possible_truncation)]
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(t),
        (1.0 / FPS) as f32,
    );
    dereth_client_runtime::camera::update_viewer(
        scene,
        Default::default(),
        LocalTime(t),
        1.0 / FPS,
    );
    camera(scene);
    scene.stream(store, gpu).expect("stream");
}

/// The camera at `(right, ahead)` from the body in its axes and `up` over where `on` stands,
/// looking `turn` radians right of the body's way and a little down.
fn place_camera(
    scene: &mut WorldScene,
    at: &Position,
    (right, ahead, up): (f32, f32, f32),
    turn: f32,
    on: ObjectId,
) {
    let m = dereth_primitives::frame::l2g(at.frame.rotation);
    let v = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(right, ahead, 0.0));
    let body = scene
        .character
        .as_ref()
        .expect("a body")
        .render_frame_of(*at)
        .origin;
    let ground = scene.objects.get(&on).map_or(body.z, |o| o.frame.origin.z);
    scene.camera.position = Vec3::new(body.x + v.x, body.y + v.y, ground + up);
    let (s, c) = (
        dereth_primitives::num::math::sinf(turn),
        dereth_primitives::num::math::cosf(turn),
    );
    let look = dereth_primitives::frame::localtoglobalvec(m, Vec3::new(s, c, 0.0));
    scene.camera.yaw = dereth_primitives::num::math::atan2f(-look.x, look.y);
    scene.camera.pitch = -0.12;
}

fn draw(gpu: &mut dereth_render::device::Gpu, scene: &mut WorldScene) -> Vec<u8> {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// The row of bodies, a second after they are made, with their names over them.
fn row(store: &Arc<RetailDatStore>) -> Vec<u8> {
    let mut gpu = crate::common::test_gpu(W, H);
    let mut scene = scene(store, &mut gpu);
    let mut stream = ObjectStream::new();
    let mut ui = Interface::new();
    let mut t = 1.0f64;
    let (made, shot) = (120, 240);
    let mut at = None;
    let subjects: Vec<&Subject> = ROW.iter().collect();
    let mut out = Vec::new();
    for i in 0..=shot {
        t += 1.0 / FPS;
        // The interface's frame, from the world as the last frame left it.
        ui.frame(nameplates(&scene, &subjects));
        if i == made {
            let here = turned(scene.character.as_ref().expect("a body").position(), VIEW);
            for s in &ROW {
                create(&mut stream, s, along(&here, s.at, std::f32::consts::PI), t);
            }
            at = Some(here);
        }
        step(store, &mut gpu, &mut scene, &mut stream, t, |scene| {
            if let Some(at) = at {
                place_camera(scene, &at, (-2.0, 1.5, 2.6), 0.1, ROW[2].id);
            }
        });
        ui.follow(&scene);
        if i == shot {
            out = draw(&mut gpu, &mut scene);
            ui.over(&mut out);
        }
    }
    scene.release_textures(&mut gpu);
    out
}

/// One frame of the runner, the middle of the picture, with where his name stands and where the
/// top of his body is drawn on it.
type RunnerFrame = (Vec<u8>, (f32, f32), Option<(f32, f32)>);

/// The runner's frames with his name over him, and for each where the name is placed and where
/// the place it is placed by stands in that frame.
fn runner(store: &Arc<RetailDatStore>) -> Vec<RunnerFrame> {
    let mut gpu = crate::common::test_gpu(W, H);
    let mut scene = scene(store, &mut gpu);
    let mut stream = ObjectStream::new();
    let mut ui = Interface::new();
    let mut t = 1.0f64;
    let (made, going, kept) = (120, 180, 200);
    let mut at = None;
    let mut crop = None;
    let mut out = Vec::new();
    for i in 0..kept + SHOWN {
        t += 1.0 / FPS;
        ui.frame(nameplates(&scene, &[&RUNNER]));
        if i == made {
            let here = turned(scene.character.as_ref().expect("a body").position(), VIEW);
            create(
                &mut stream,
                &RUNNER,
                along(&here, RUNNER.at, std::f32::consts::FRAC_PI_2),
                t,
            );
            at = Some(here);
        }
        if i == going {
            if let Some(o) = scene.objects.get(&RUNNER.id) {
                o.sim.driver.borrow_mut().move_to_interpreted_state(
                    &InterpretedMotionState {
                        forward_command: MotionCommand::RUN_FORWARD,
                        forward_speed: 2.5,
                        ..InterpretedMotionState::default()
                    },
                    false,
                );
            }
        }
        // The camera turns left as the runner runs right.
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let turn = -0.6 * ((i.saturating_sub(going)) as f64 / FPS) as f32;
        step(store, &mut gpu, &mut scene, &mut stream, t, |scene| {
            if let Some(at) = at {
                place_camera(scene, &at, (0.0, 0.5, 1.7), turn, RUNNER.id);
            }
        });
        ui.follow(&scene);
        if i >= kept {
            let used = ui.placed(RUNNER.id);
            let drawn = anchor(&scene, RUNNER.id).map(|(x, y, _)| (x, y));
            let mut rgba = draw(&mut gpu, &mut scene);
            ui.over(&mut rgba);
            // The tile is where the runner is on the first frame kept, and stays there.
            let (cx, cy) = *crop.get_or_insert(drawn.unwrap_or((0.0, 0.0)));
            out.push((
                tile(&rgba, (cx, cy)),
                used.unwrap_or((f32::NAN, f32::NAN)),
                drawn,
            ));
        }
    }
    scene.release_textures(&mut gpu);
    out
}

/// The `TILE_W` by `TILE_H` of a frame round `(x, y)`, a little below it, kept on the frame.
fn tile(rgba: &[u8], (x, y): (f32, f32)) -> Vec<u8> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0) = (
        ((x - TILE_W as f32 / 3.0).max(0.0) as u32).min(W - TILE_W),
        ((y - TILE_H as f32 / 3.0).max(0.0) as u32).min(H - TILE_H),
    );
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
    eprintln!("wrote {path}");
}

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_NAMEPLATE_DUMP set and --ignored"]
fn render_the_nameplate_frames() {
    let _gpu = crate::common::gpu_lock();
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the dump folder");
    let store = crate::common::dats();
    write_png(&format!("{dir}/row.png"), W, H, &row(&store));
    let frames = runner(&store);
    let sub = format!("{dir}/runner");
    std::fs::create_dir_all(&sub).expect("the runner's folder");
    let mut tsv = std::fs::File::create(format!("{dir}/names.tsv")).expect("the names file");
    writeln!(
        tsv,
        "frame\tplaced_x\tplaced_y\tdrawn_x\tdrawn_y\toff_x\toff_y"
    )
    .expect("write");
    for (n, (rgba, used, drawn)) in frames.iter().enumerate() {
        write_png(&format!("{sub}/frame-{n:03}.png"), TILE_W, TILE_H, rgba);
        let (dx, dy) = drawn.unwrap_or((f32::NAN, f32::NAN));
        writeln!(
            tsv,
            "{n}\t{:.1}\t{:.1}\t{dx:.1}\t{dy:.1}\t{:.1}\t{:.1}",
            used.0,
            used.1,
            used.0 - dx,
            used.1 - dy
        )
        .expect("write");
    }
    let tiles: Vec<&Vec<u8>> = frames
        .iter()
        .step_by(STRIP_EVERY)
        .map(|(rgba, _, _)| rgba)
        .collect();
    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (TILE_W * tiles.len() as u32, TILE_H);
    let mut strip = vec![0u8; (w * h * 4) as usize];
    for (c, t) in tiles.iter().enumerate() {
        for y in 0..TILE_H as usize {
            let src = y * TILE_W as usize * 4;
            let dst = (y * w as usize + c * TILE_W as usize) * 4;
            strip[dst..dst + TILE_W as usize * 4]
                .copy_from_slice(&t[src..src + TILE_W as usize * 4]);
        }
    }
    write_png(&format!("{dir}/runner.png"), w, h, &strip);
}
