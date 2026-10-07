//! A building seen from outdoors shows its interior only through the openings of the shell it is
//! drawing: from 33 metres west of a Holtburg house, at the shipped Degrade Distance of 50 the
//! house is drawn at full detail and its front door opens onto the room behind it and the object
//! standing there; with the Degrade Distance at 0 the same house is drawn at its next level of
//! detail, which has no openings, and none of its rooms is drawn, given a view, or lets the object
//! inside be drawn. A door placed in the same room but standing across the doorway, so that it
//! is registered in the land cell outside too, is drawn from outdoors at either level.
//! Fixture: the house with a cellar on Holtburg's landblock 0xA9B4 from the retail dats, the body
//! on the line through its front doorway, facing it, and a door object created through the
//! application's own object stream (no datagram leaves this process). The object's own record
//! keeps it at full detail to 100 metres past the Degrade Distance, so it is drawn at full detail
//! in every arm.

#![cfg(gpu)]

use crate::rendering::building_boundary_draw::{place_door, store, DOOR, DOOR_ROT, HOLTBURG};
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::frame::V3 as _;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use std::collections::BTreeSet;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 800;
const H: u32 = 600;

/// The room behind the house's front doorway `0xA9B40145`, x [136.70, 142.10], y [3.90, 13.10].
const ROOM: u32 = 0xA9B4_0143;
/// In the room, on the line from the body through the middle of the front doorway (y 4.20 to
/// 6.10 in the west wall at x 136.30).
const IN_THE_ROOM: Vec3 = Vec3::new(139.00, 5.15, 94.082_001);
/// The body: 33 metres west of the doorway on the same line, in land cell 0x21. The house's
/// shell switches to its next level at 24 metres past the Degrade Distance.
const BODY_CELL: u32 = 0xA9B4_0021;
const BODY_AT: Vec3 = Vec3::new(103.00, 5.15, 95.00);
/// In the room but standing in its front doorway: the object's body crosses the doorway into
/// the land cell outside, 0x29.
const IN_THE_DOORWAY: Vec3 = Vec3::new(136.75, 5.155, 94.082_001);
/// In the open between the body and the house, in land cell 0x29: the object's own control.
const OPEN_CELL: u32 = 0xA9B4_0029;
const IN_THE_OPEN: Vec3 = Vec3::new(122.00, 5.15, 94.50);

struct Look {
    /// The interior cells the frame drew.
    cells: BTreeSet<u32>,
    /// How many view polygons the room was given.
    room_views: usize,
    /// How many of the object's subsets were submitted.
    object_subsets: usize,
    /// How many building openings the frame drew an interior through.
    openings: usize,
    /// Whether the camera stayed outdoors.
    outdoors: bool,
    /// The detail level the house's shell drew.
    shell_level: usize,
    /// The cells the object is registered in.
    object_cells: BTreeSet<u32>,
}

/// Six frames with the body west of the front door facing it, the object at `at` in `cell`, and
/// the Degrade Distance preference at `degrade_distance`.
fn look(store: &Arc<RetailDatStore>, degrade_distance: f32, cell: u32, at: Vec3) -> Look {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        ..SceneConfig::default()
    };
    cfg.render.degrade_distance = degrade_distance;
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    // Facing east, toward the doorway.
    let yaw = 270.0f32.to_radians();
    let q = Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5));
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(CellId(BODY_CELL), Frame::new(BODY_AT, q)));
    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    place_door(
        &mut stream,
        Position::new(CellId(cell), Frame::new(at, DOOR_ROT)),
        now,
        false,
    );
    for _ in 0..6 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        if let Some(c) = scene.character.as_mut() {
            stream.sync_physics_at(store, &mut c.world, LocalTime(now));
        }
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
            1.0 / 30.0,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
    let body = scene.character.as_ref().expect("a body");
    let camera_cell = body.camera.viewer_cell.unwrap_or(body.position().cell);
    let physics = &body.world;
    let door = physics.by_object_id(DOOR).and_then(|h| physics.get(h));
    let placed = door.and_then(|b| b.cell);
    let object_cells: BTreeSet<u32> = door
        .map(|b| {
            b.shadow_objects
                .iter()
                .filter(|s| s.cell_present)
                .map(|s| s.cell_id.0)
                .collect()
        })
        .unwrap_or_default();
    // The house is the building whose origin is nearest the room.
    let (house_at, shell_level) = scene
        .building_shell_levels()
        .into_iter()
        .min_by(|a, b| {
            let d = |p: Vec3| p.sub(IN_THE_ROOM).mag2();
            d(a.0).total_cmp(&d(b.0))
        })
        .expect("the house is resident");
    assert!(
        house_at.sub(IN_THE_ROOM).mag2() < 15.0 * 15.0,
        "the nearest building to the room, at {house_at:?}, is the house"
    );
    assert_eq!(
        placed,
        Some(CellId(cell)),
        "the object is placed where it was put"
    );
    for p in scene
        .part_degrade_probe()
        .into_iter()
        .filter(|p| p.object == Some(DOOR))
    {
        assert_eq!(
            p.level, 0,
            "the object's part {} is drawn at full detail at {:.1} m",
            p.part, p.distance
        );
    }
    Look {
        cells: scene.drawn_cells().unwrap_or_default(),
        room_views: scene.cell_view_polys(CellId(ROOM)).len(),
        object_subsets: scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(DOOR))
            .count(),
        openings: scene.building_portal_screen_polygons(W, H).len(),
        outdoors: dereth_physics::landdefs::is_outdoors(camera_cell),
        shell_level,
        object_cells,
    }
}

/// Behaviour: rendering.interior.degraded-shell-opens-no-interior
#[test]
fn a_house_drawn_below_full_detail_shows_no_room_and_nothing_standing_in_it() {
    let store = store();
    let full = look(&store, 50.0, ROOM, IN_THE_ROOM);
    eprintln!(
        "Degrade Distance 50: cells {:x?}, room views {}, object subsets {}, openings {}",
        full.cells, full.room_views, full.object_subsets, full.openings
    );
    assert!(full.outdoors, "the camera stays outdoors");
    assert_eq!(full.shell_level, 0, "the house is drawn at full detail");
    // The premise: at full detail the front door opens onto the room, and the object in it is
    // drawn through the doorway.
    assert!(
        full.cells.contains(&ROOM),
        "the full-detail house's front door opens onto the room"
    );
    assert!(
        full.room_views > 0,
        "the room has a view through the doorway"
    );
    assert!(
        full.object_subsets > 0,
        "the object in the room is drawn through the doorway"
    );

    // The object's own control: at the same station and Degrade Distance, standing in the open,
    // it is drawn, so its absence from the room below is the shell's doing.
    let open = look(&store, 0.0, OPEN_CELL, IN_THE_OPEN);
    assert!(open.outdoors, "the camera stays outdoors");
    assert!(open.object_subsets > 0, "the object is drawn in the open");

    let degraded = look(&store, 0.0, ROOM, IN_THE_ROOM);
    eprintln!(
        "Degrade Distance 0: cells {:x?}, room views {}, object subsets {}, openings {}",
        degraded.cells, degraded.room_views, degraded.object_subsets, degraded.openings
    );
    assert!(degraded.outdoors, "the camera stays outdoors");
    assert_eq!(
        degraded.shell_level, 1,
        "the house is drawn at its next level of detail"
    );
    let house: BTreeSet<u32> = full
        .cells
        .iter()
        .copied()
        .filter(|c| c >> 16 == u32::from(HOLTBURG))
        .collect();
    assert!(
        degraded.cells.is_disjoint(&house),
        "the house's next level has no openings, yet cells {:x?} of it were drawn",
        degraded.cells
    );
    assert_eq!(degraded.room_views, 0, "the room gets no view");
    assert_eq!(
        degraded.object_subsets, 0,
        "the object in the room is drawn through a shell that has no doorway"
    );
}

/// Behaviour: rendering.interior.a-room-object-across-a-doorway-is-drawn-from-outdoors
#[test]
fn a_door_in_a_room_that_crosses_its_doorway_is_drawn_from_outdoors_at_any_shell_level() {
    let store = store();
    for (degrade_distance, level) in [(50.0, 0), (0.0, 1)] {
        let at = look(&store, degrade_distance, ROOM, IN_THE_DOORWAY);
        eprintln!(
            "Degrade Distance {degrade_distance}: shell level {}, object cells {:x?}, \
             object subsets {}",
            at.shell_level, at.object_cells, at.object_subsets
        );
        assert!(at.outdoors, "the camera stays outdoors");
        assert_eq!(at.shell_level, level, "the house's shell level");
        // The premise: the door is placed in the room and registered outside as well.
        assert!(
            at.object_cells.contains(&ROOM) && at.object_cells.contains(&OPEN_CELL),
            "the door is registered in the room and the land cell outside, not {:x?}",
            at.object_cells
        );
        assert!(
            at.object_subsets > 0,
            "the door across the doorway is drawn from outdoors with the shell at level {level}"
        );
    }
}
