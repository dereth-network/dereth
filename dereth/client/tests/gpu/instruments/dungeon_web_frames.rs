//! Frame generators for the dungeon-web stations: each writes PNGs of Drudge Hideout's entry room
//! with Multiple Pass Alpha on and off -- the body at a fixed spot with its own chase camera and the
//! room's two cage doors created where the hideout keeps them; the body, the drudge and the cage
//! door of the stations seen through a web; and a room whose floor carries the red stain
//! `0x010017B7`, the body standing on it -- to `DERETH_TEST_DUNGEON_WEBS_DUMP` (default:
//! `dungeon-webs/` in cargo's scratch folder for integration tests). They assert nothing; the
//! claims are in `rendering::dungeon_webs`, whose fixture these share.

#![cfg(gpu)]

use std::f32::consts::FRAC_1_SQRT_2;
use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

use crate::rendering::dungeon_webs::{shot, store, Arm, Subject, CAGE_DOOR, H, ROOM, W};

fn out_dir() -> String {
    std::env::var("DERETH_TEST_DUNGEON_WEBS_DUMP")
        .unwrap_or_else(|_| format!("{}/dungeon-webs", env!("CARGO_TARGET_TMPDIR")))
}

fn write_png(name: &str, rgba: &[u8]) {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("the png folder");
    let path = format!("{dir}/{name}.png");
    let f = std::fs::File::create(&path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(rgba).expect("png data");
    eprintln!("wrote {path}");
}

/// One object to create: its setup, cell, frame.
type Placement = (u32, u32, Frame);

/// The room's two cage doors, where the hideout keeps them: one in the doorway north of the room
/// and one in the doorway east of it.
fn hideout_doors() -> Vec<Placement> {
    vec![
        (
            CAGE_DOOR,
            0x019E_0113,
            Frame::new(Vec3::new(10.0, -34.75, 0.0), Quat::new(0.0, 0.0, 0.0, -1.0)),
        ),
        (
            CAGE_DOOR,
            0x019E_0123,
            Frame::new(
                Vec3::new(15.25, -40.0, 0.0),
                Quat::new(-FRAC_1_SQRT_2, 0.0, 0.0, -FRAC_1_SQRT_2),
            ),
        ),
    ]
}

fn create(objects: &mut ObjectStream, n: u32, (setup, cell, frame): Placement, now: f64) {
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: ObjectId(0x8300_0060 + n),
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: Some(setup),
            state: 0,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: cell,
                frame: dereth_protocol::types::Frame {
                    origin: frame.origin.into(),
                    orientation: frame.rotation.into(),
                },
            }),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance: 1,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("encode");
    objects.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    objects.world.update_visible_object_list();
}

/// The body at `at` in `cell`, the client's own chase camera behind it, `placements` created, and
/// the last of `frames` frames.
fn chase(
    store: &Arc<RetailDatStore>,
    multi_pass_alpha: bool,
    (cell, at): (u32, Frame),
    placements: &[Placement],
) -> Vec<u8> {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let mut cfg = SceneConfig {
        landblock: block,
        start_cell: Some(CellId(cell)),
        time_of_day: Some(0.5),
        particles: false,
        ..SceneConfig::default()
    };
    cfg.render.multi_pass_alpha = multi_pass_alpha;
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    {
        let c = scene.character.as_mut().expect("a body");
        c.land().load_block_cells(LandblockId(block));
        c.teleport(Position::new(CellId(cell), at));
    }
    let mut objects = ObjectStream::new();
    let mut now = 0.0f64;
    let mut rgba = Vec::new();
    for i in 0..36 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        if i == 4 {
            for (n, p) in placements.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation)] // a handful of placements
                create(&mut objects, n as u32, *p, now);
            }
        }
        scene
            .sync_objects(store, &mut gpu, &mut objects)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            dereth_client_runtime::platform::clock::HEADLESS_STEP,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    let body = scene.character.as_ref().expect("a body").position();
    eprintln!(
        "chase frame: body in {:#010X} at [{:.3} {:.3} {:.3}], viewer cell {:?}",
        body.cell.0,
        body.frame.origin.x,
        body.frame.origin.y,
        body.frame.origin.z,
        scene.viewer_cell_id()
    );
    scene.release_textures(&mut gpu);
    rgba
}

/// A spot in the entry room facing north-east, the cage doors ahead of it.
const ENTRY_SPOT: (u32, Frame) = (
    ROOM,
    Frame::new(
        Vec3::new(8.117_457, -41.239_388, 0.005),
        Quat::new(0.932_085, 0.0, 0.0, -0.362_241),
    ),
);

/// A room whose floor carries eleven of the red stain `0x010017B7`, with the body standing among
/// them.
const STAIN_SPOT: (u32, Frame) = (
    0xF418_010D,
    Frame::new(
        Vec3::new(34.9, 36.8, 162.75),
        Quat::new(0.924, 0.0, 0.0, -0.383),
    ),
);

#[test]
#[ignore = "instrument: writes PNGs; run with DERETH_TEST_DUNGEON_WEBS_DUMP set and --ignored"]
fn render_the_dungeon_web_frames() {
    let store = store();
    for (on, tag) in [(true, "on"), (false, "off")] {
        write_png(
            &format!("entry-room-player-{tag}"),
            &chase(&store, on, ENTRY_SPOT, &hideout_doors()),
        );
        for (subject, name) in [
            (Subject::Body, "player"),
            (Subject::Drudge, "drudge"),
            (Subject::Door, "cage-door"),
        ] {
            let s = shot(
                &store,
                Arm {
                    multi_pass_alpha: on,
                    statics: true,
                    subject,
                    through: true,
                    lit: true,
                },
            );
            write_png(&format!("web-station-{name}-{tag}"), &s.rgba);
        }
        write_png(
            &format!("red-stain-room-{tag}"),
            &chase(&store, on, STAIN_SPOT, &[]),
        );
    }
}
