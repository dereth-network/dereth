//! An object whose create names an outdoor land cell but whose point lies inside a building cannot
//! be placed, and is destroyed 25 seconds after its create; the same object created with the room
//! cell that contains the point keeps its cell and keeps drawing.
//!
//! The outdoor-cell placement arm does not descend into a building, and the land-cell insertion
//! runs before the other-cell checks, so the enclosed room rejects the placement. The create
//! handler keeps an object that found a cell; for a nonzero position cell whose body found none it
//! sets a destruction deadline of now plus 25.0 seconds. The early reading counts submitted door
//! parts only (not the pass or pixels); the late reading checks the object table and draw list.
//!
//! Fixture: a recorded door (`0x0200_024F`) at Holtburg's room `0xA9B4_0143`, created through
//! `ObjectStream::apply_event` (no socket), on the retail dats and a software device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::character::CharacterInput;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, ServerTime, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;

const W: u32 = 1200;
const H: u32 = 900;
const HOLTBURG: u16 = 0xA9B4;

/// The recorded **outdoor** land cell used by this selected door station.
const OWNER_CELL: u32 = 0xA9B4_0029;
/// The room that geometrically contains the point below.
const ROOM_CELL: u32 = 0xA9B4_0143;
/// The selected point inside that room, two metres in front of the camera.
const IN_THE_ROOM: Vec3 = Vec3::new(138.60, 7.00, 94.082_001);
/// The station's standing point and heading.
const STAND: Vec3 = Vec3::new(139.50, 7.00, 94.00);
const FACING_WEST: f32 = 90.0;
const DOOR_ROT: Quat = Quat::new(0.707_107, 0.0, 0.0, -0.707_107);

/// The recorded door setup at this landblock and the station's synthetic object id.
const DOOR_SETUP: u32 = 0x0200_024F;
const DOOR: ObjectId = ObjectId(0x5000_0D25);

/// How long an object that found no cell lives after its create.
const DESTRUCTION_TIME: f64 = 25.0;

/// Apply a synthetic object-create event directly through the current object stream.
fn place_door(stream: &mut ObjectStream, at: Position, now: f64) {
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: DOOR,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: dereth_protocol::types::physicsdesc::PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(DOOR_SETUP),
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
            ..dereth_protocol::types::physicsdesc::PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("encode");
    stream.apply_event(
        &dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
}

/// One reading of the station.
struct Reading {
    /// Total door subsets submitted by the first of six drawn samples after object/physics sync.
    /// This count proves submission only; it does not identify the draw pass or pixel visibility.
    first_frame_parts: usize,
    /// Subsets the **last** drawn frame submitted.
    parts_drawn: usize,
    /// Whether `ObjectStream` still holds a presence for the door.
    in_tables: bool,
    /// Whether the current physics body has an assigned cell.
    has_cell: bool,
}

/// Drive the recorded door station toward `early` seconds of client time and read it, then drive
/// the same station on toward `late` seconds and read it again.
///
/// `cell` is the cell the create names: `OWNER_CELL` is the outdoor land cell the recorded create
/// carried, `ROOM_CELL` the interior cell that contains the same point.
fn run(store: &Arc<RetailDatStore>, cell: u32, early: f64, late: f64) -> (Reading, Reading) {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    let yaw = FACING_WEST.to_radians();
    let q = Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5));
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(CellId(ROOM_CELL), Frame::new(STAND, q)));

    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    place_door(
        &mut stream,
        Position::new(CellId(cell), Frame::new(IN_THE_ROOM, DOOR_ROT)),
        now,
    );

    // Six drawn samples settle the station. Nondrawing samples then advance the clock toward each
    // requested time in increments of at most 0.5 seconds while each `scene.update` still receives
    // a fixed 1/30-second delta, and three drawn samples then advance beyond it by three headless
    // steps before it is read. The sampled times are therefore near the requested durations, not
    // exact transition frames.
    let step = dereth_client::app::HEADLESS_STEP;
    let frame =
        |scene: &mut WorldScene, gpu: &mut Gpu, stream: &mut ObjectStream, now: f64, draw: bool| {
            scene
                .sync_objects(store, gpu, stream)
                .expect("sync_objects");
            if let Some(c) = scene.character.as_mut() {
                stream.sync_physics_at(store, &mut c.world, LocalTime(now));
            }
            stream.use_time::<dereth_client_net::client_session::testing::MockTransport>(
                ServerTime(now),
                None,
            );
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
            dereth_client::camera::update_viewer(
                scene,
                dereth_client::camera::CameraInput::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
            if draw {
                scene.stream(store, gpu).expect("stream");
                scene.reserve_upload_arena(gpu).expect("reserve the arena");
                gpu.begin_frame().expect("begin");
                scene.draw(gpu).expect("draw");
                gpu.end_frame().expect("end");
            }
        };
    let mut first_frame_parts = 0usize;
    for i in 0..6 {
        now += step;
        frame(&mut scene, &mut gpu, &mut stream, now, true);
        if i == 0 {
            first_frame_parts = scene
                .drawn_part_order()
                .iter()
                .filter(|e| e.object == Some(DOOR))
                .count();
        }
    }
    let read = |seconds: f64,
                now: &mut f64,
                scene: &mut WorldScene,
                gpu: &mut Gpu,
                stream: &mut ObjectStream| {
        while *now + step < seconds {
            *now += (seconds - *now).min(0.5);
            frame(scene, gpu, stream, *now, false);
        }
        for _ in 0..3 {
            *now += step;
            frame(scene, gpu, stream, *now, true);
        }
        let parts_drawn = scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(DOOR))
            .count();
        let in_tables = stream.presence(DOOR).is_some();
        let has_cell = scene
            .character
            .as_ref()
            .expect("a body")
            .world
            .by_object_id(DOOR)
            .and_then(|h| scene.character.as_ref().expect("a body").world.get(h))
            .is_some_and(|b| b.cell.is_some());
        eprintln!(
            "unplaceable door, cell {cell:#010X} after {now:.1} s: door subsets on the first \
             frame {first_frame_parts}, on the last {parts_drawn}, still in the tables \
             {in_tables}, physics-object cell {has_cell}, viewer cell {:?}",
            scene
                .character
                .as_ref()
                .and_then(|c| c.camera.viewer_cell)
                .map(|c| format!("{:#010X}", c.0)),
        );
        Reading {
            first_frame_parts,
            parts_drawn,
            in_tables,
            has_cell,
        }
    };
    let at_early = read(early, &mut now, &mut scene, &mut gpu, &mut stream);
    let at_late = read(late, &mut now, &mut scene, &mut gpu, &mut stream);
    (at_early, at_late)
}

/// The control: a door created in the room cell has a cell and is drawn after about one second,
/// and the same run near 30 seconds still holds and draws it.
#[test]
fn a_door_in_the_cell_that_contains_it_keeps_its_cell_and_keeps_drawing() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let (early, late) = run(&store, ROOM_CELL, 1.0, DESTRUCTION_TIME + 5.0);
    assert!(
        early.has_cell,
        "the room-cell control has no assigned cell in the early sample"
    );
    assert!(
        early.parts_drawn > 0,
        "the room-cell control has no submitted parts in the final early draw sample"
    );
    // The late reading remains present and drawn because this placement has a cell.
    assert!(
        late.in_tables && late.parts_drawn > 0,
        "the room-cell control is absent or undrawn after {:.0} s ({} subsets, in tables {}); a cell-bearing object must remain outside the destruction path",
        DESTRUCTION_TIME + 5.0,
        late.parts_drawn,
        late.in_tables
    );
}

/// Behaviour: world.placement.an-unplaceable-outdoor-object-is-destroyed-after-twenty-five-seconds
///
/// **Rejected placement.** At this selected point, a door created with the outdoor land cell has
/// no assigned cell. The early reading checks that state, table presence, and first sampled draw;
/// the same run's late reading checks that it is absent from the table and final draw list.
#[test]
fn an_unplaceable_outdoor_cell_object_is_destroyed_after_twenty_five_seconds() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    // Early sample: no cell, still in the table, and at least one part in the first draw list.
    let (early, late) = run(&store, OWNER_CELL, 1.0, DESTRUCTION_TIME + 5.0);
    assert!(
        !early.has_cell,
        "the outdoor-cell station unexpectedly has an assigned cell in the early sample: {OWNER_CELL:#010X}"
    );
    assert!(
        early.in_tables,
        "the outdoor-cell station is absent from the object table in the early sample, so absence at {:.0} s would not establish later destruction",
        DESTRUCTION_TIME + 5.0
    );
    assert!(
        early.first_frame_parts > 0,
        "the outdoor-cell station submitted no parts in its first sampled draw; this check proves submission only for source cell {OWNER_CELL:#010X}"
    );
    // Late sample: after roughly 30 seconds it is absent from the table and final draw list.
    assert!(
        !late.in_tables,
        "the outdoor-cell station remains in the object table {:.0} s after create; a nonzero source cell with no assigned cell must enter the 25-second destruction path",
        DESTRUCTION_TIME + 5.0
    );
    // The final draw-list assertion accompanies the load-bearing table-removal assertion; it does
    // not identify an exact deletion frame or prove continuous visibility before removal.
    assert_eq!(
        late.parts_drawn,
        0,
        "the unplaceable door still submits {} subsets on the last frame {:.0} s after its create",
        late.parts_drawn,
        DESTRUCTION_TIME + 5.0
    );
}
