//! A room's baked statics are drawn part by part by every cell they are registered in, each part
//! tested against the views of the cell drawing it: a piece of a Holtburg hall's first room that
//! reaches out through the door is drawn by the land cell outside whether or not the hall's shell
//! opens onto the room; the cellar stairs of the house with a cellar, which reach up into the
//! rooms above, are drawn by those rooms when the cellar itself is not reached; and pieces of a
//! reached room that no view of the room sees are not drawn, with not a pixel of the frame
//! changed by leaving them out; and a room reached through two of its building's openings
//! draws each piece once.
//! Fixture: Holtburg's landblock 0xA9B4 from the retail dats; the body outdoors facing the
//! buildings' doors; the scene's own record of which cell drew each part of each room static.

#![cfg(gpu)]

use crate::rendering::building_boundary_draw::{store, HOLTBURG};
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::frame::V3 as _;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_scene::world_scene::{CellStaticPartProbe, CellStaticRunDraw};
use std::collections::BTreeSet;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 800;
const H: u32 = 600;

/// The hall's first room, and the land cell outside its door that its piece reaches into.
const HALL_ROOM: u32 = 0xA9B4_0100;
const HALL_PIECE_SETUP: u32 = 0x0100_0C20;
const OUTSIDE_THE_HALL: u32 = 0xA9B4_001E;
/// 30 m north of the hall's door, on the line through it, in land cell 0x1F. The hall's shell
/// switches to its next level 24 m past the Degrade Distance.
const HALL_VIEW_CELL: u32 = 0xA9B4_001F;
const HALL_VIEW: Vec3 = Vec3::new(89.09, 166.54, 66.01);
const HALL_DOOR: Vec3 = Vec3::new(89.09, 136.54, 66.0);

/// The house with a cellar from 33 m west of its front doorway (the degraded-shell station).
const HOUSE_VIEW_CELL: u32 = 0xA9B4_0021;
const HOUSE_VIEW: Vec3 = Vec3::new(103.00, 5.15, 95.00);
const HOUSE_DOOR: Vec3 = Vec3::new(139.00, 5.15, 95.00);
/// The room behind its front door, and the cellar below whose stairs reach up into it.
const FRONT_ROOM: u32 = 0xA9B4_0143;
const CELLAR: u32 = 0xA9B4_0147;
const CELLAR_STAIRS_SETUP: u32 = 0x0200_01B4;

/// 15 m west of an opening of the building whose rooms include `0xA9B40101`, in land cell 0x16:
/// the room is reached through two of the building's openings from here.
const TWO_OPENINGS_VIEW_CELL: u32 = 0xA9B4_0016;
const TWO_OPENINGS_VIEW: (f32, f32) = (58.09, 131.54);
const TWO_OPENINGS_OPENING: (f32, f32) = (73.09, 131.54);
const ROOM_REACHED_TWICE: u32 = 0xA9B4_0101;

struct Look {
    parts: Vec<CellStaticPartProbe>,
    cells: BTreeSet<u32>,
    shell_level: usize,
    culled: u32,
    reached_again: u32,
    runs: Vec<CellStaticRunDraw>,
    outdoors: bool,
    rgba: Vec<u8>,
}

fn facing(from: Vec3, to: Vec3) -> Quat {
    let yaw = math::atan2f(-(to.x - from.x), to.y - from.y);
    Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5))
}

/// Twelve frames with the body at `at` facing `target`, the Degrade Distance preference at
/// `degrade_distance`, and the per-part view test and the registered-cell draws switched as
/// given; the shell level is the level of the building nearest `target`.
fn look(
    store: &Arc<RetailDatStore>,
    (cell, at, target): (u32, Vec3, Vec3),
    degrade_distance: f32,
    viewcone: bool,
    shadows: bool,
) -> Look {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        object_viewcone: viewcone,
        cell_static_shadows: shadows,
        ..SceneConfig::default()
    };
    cfg.render.degrade_distance = degrade_distance;
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(
            CellId(cell),
            Frame::new(at, facing(at, target)),
        ));
    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    for _ in 0..12 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, &mut gpu, &mut stream)
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
    let (_, shell_level) = scene
        .building_shell_levels()
        .into_iter()
        .min_by(|a, b| {
            let d = |p: Vec3| p.sub(target).mag2();
            d(a.0).total_cmp(&d(b.0))
        })
        .expect("a building is resident");
    Look {
        parts: scene.cell_static_parts(),
        cells: scene.drawn_cells().unwrap_or_default(),
        shell_level,
        culled: scene.drawn_cell_statics().culled,
        reached_again: scene.drawn_cell_statics().cells_reached_again,
        runs: scene.drawn_cell_static_runs(),
        outdoors: dereth_physics::landdefs::is_outdoors(camera_cell),
        rgba: gpu.capture().expect("capture").to_rgba(),
    }
}

/// The parts of the statics of `cell` whose setup is `setup`.
fn parts_of(look: &Look, cell: u32, setup: u32) -> Vec<&CellStaticPartProbe> {
    look.parts
        .iter()
        .filter(|p| p.cell.0 == cell && p.setup.0 == setup)
        .collect()
}

/// Behaviour: rendering.interior.a-room-static-across-an-outdoor-portal-is-drawn-from-outdoors
#[test]
fn a_room_piece_across_the_halls_door_is_drawn_by_the_land_cell_outside_at_any_shell_level() {
    let store = store();
    let station = (HALL_VIEW_CELL, HALL_VIEW, HALL_DOOR);
    let full = look(&store, station, 50.0, true, true);
    let degraded = look(&store, station, 0.0, true, true);
    let control = look(&store, station, 0.0, true, false);
    for l in [&full, &degraded, &control] {
        assert!(l.outdoors, "the camera stays outdoors");
    }
    assert_eq!(
        full.shell_level, 0,
        "at Degrade Distance 50 the hall is at full detail"
    );
    assert_eq!(
        degraded.shell_level, 1,
        "at Degrade Distance 0 it is at its next level"
    );

    // The premise: the piece is registered in its room and in the land cell outside the door.
    let piece = parts_of(&degraded, HALL_ROOM, HALL_PIECE_SETUP);
    assert_eq!(piece.len(), 1, "the hall's piece is one part");
    assert_eq!(
        piece[0].registered,
        vec![CellId(OUTSIDE_THE_HALL)],
        "the piece reaches through the door into the land cell outside"
    );

    // At full detail it is drawn, by the room through the door or by the land cell.
    let at_full = parts_of(&full, HALL_ROOM, HALL_PIECE_SETUP);
    assert!(
        at_full[0].drawn_by.is_some(),
        "the piece is drawn at full detail"
    );
    // Below full detail the shell opens nothing, so the room is not drawn, and the land cell
    // draws the piece.
    assert!(
        !degraded.cells.contains(&HALL_ROOM),
        "the degraded shell opens nothing: {:X?}",
        degraded.cells
    );
    assert_eq!(
        piece[0].drawn_by,
        Some(CellId(OUTSIDE_THE_HALL)),
        "the land cell outside draws the piece"
    );
    // Nothing else of the room is drawn: only the piece reaches outdoors.
    let rest: Vec<_> = degraded
        .parts
        .iter()
        .filter(|p| p.cell.0 == HALL_ROOM && p.setup.0 != HALL_PIECE_SETUP)
        .collect();
    assert!(!rest.is_empty(), "the room has other pieces");
    assert!(
        rest.iter().all(|p| p.drawn_by.is_none()),
        "none of the room's other pieces is drawn from outdoors"
    );
    // The control: drawn by its own room alone, the piece is not drawn at all.
    assert_eq!(
        parts_of(&control, HALL_ROOM, HALL_PIECE_SETUP)[0].drawn_by,
        None,
        "drawn by its own room alone, the piece is lost below full detail"
    );
}

/// Behaviour: rendering.interior.a-room-static-is-drawn-by-every-room-it-reaches
#[test]
fn the_cellar_stairs_are_drawn_by_the_rooms_above_when_the_cellar_is_not_reached() {
    let store = store();
    let station = (HOUSE_VIEW_CELL, HOUSE_VIEW, HOUSE_DOOR);
    let on = look(&store, station, 50.0, true, true);
    let off = look(&store, station, 50.0, true, false);
    assert!(on.outdoors, "the camera stays outdoors");
    assert_eq!(on.shell_level, 0, "the house is at full detail");
    assert!(
        on.cells.contains(&FRONT_ROOM) && !on.cells.contains(&CELLAR),
        "the front door opens onto the front room and not onto the cellar: {:X?}",
        on.cells
    );
    let stairs = parts_of(&on, CELLAR, CELLAR_STAIRS_SETUP);
    assert!(stairs.len() > 1, "the stairs are several parts");
    assert!(
        stairs
            .iter()
            .all(|p| p.registered.contains(&CellId(FRONT_ROOM))),
        "the stairs reach up into the front room"
    );
    let drawn: Vec<_> = stairs.iter().filter_map(|p| p.drawn_by).collect();
    eprintln!("the stairs' parts were drawn by {drawn:X?}");
    assert!(!drawn.is_empty(), "the rooms above draw some of the stairs");
    assert!(
        drawn.iter().all(|c| c.0 != CELLAR),
        "no part is drawn by the cellar, which is not reached"
    );
    assert!(
        parts_of(&off, CELLAR, CELLAR_STAIRS_SETUP)
            .iter()
            .all(|p| p.drawn_by.is_none()),
        "drawn by the cellar alone, no part of the stairs is drawn"
    );
}

/// Behaviour: rendering.interior.room-statics-no-view-of-their-room-sees-are-not-drawn
#[test]
fn pieces_of_a_reached_room_that_none_of_its_views_sees_are_left_out_and_no_pixel_changes() {
    let store = store();
    let station = (HOUSE_VIEW_CELL, HOUSE_VIEW, HOUSE_DOOR);
    let on = look(&store, station, 50.0, true, true);
    let off = look(&store, station, 50.0, false, true);
    assert!(on.cells.contains(&FRONT_ROOM), "the front room is reached");
    let room: Vec<_> = on.parts.iter().filter(|p| p.cell.0 == FRONT_ROOM).collect();
    let left_out: Vec<_> = room
        .iter()
        .filter(|p| p.drawn_by.is_none() && p.sphere.is_some())
        .collect();
    eprintln!(
        "front room: {} parts, {} left out ({:X?}); {} parts culled in all",
        room.len(),
        left_out.len(),
        left_out.iter().map(|p| p.setup.0).collect::<Vec<_>>(),
        on.culled
    );
    assert!(
        !left_out.is_empty(),
        "some pieces of the front room are outside its view through the door"
    );
    assert!(on.culled > 0, "the per-part test culled parts");
    // Without the test every part of the reached room is drawn.
    assert_eq!(off.culled, 0, "with the test off nothing is culled");
    assert!(
        off.parts
            .iter()
            .filter(|p| p.cell.0 == FRONT_ROOM && p.sphere.is_some())
            .all(|p| p.drawn_by.is_some()),
        "with the test off every piece of the front room is drawn"
    );
    // And what it left out was not on the screen.
    let differ = on
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(off.rgba.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differ, 0, "leaving those pieces out changes no pixel");
}

/// Behaviour: rendering.interior.a-room-static-is-drawn-once-a-frame
#[test]
fn a_room_reached_through_two_openings_submits_each_piece_once() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(Arc::clone(&store), &region)
        .expect("the heights load");
    let (x, y) = TWO_OPENINGS_VIEW;
    let z = land
        .ground_height(dereth_primitives::LandblockId(0xA9B4), x, y)
        .expect("the ground is there");
    let at = Vec3::new(x, y, z + 0.05);
    let opening = Vec3::new(TWO_OPENINGS_OPENING.0, TWO_OPENINGS_OPENING.1, at.z);
    let l = look(
        &store,
        (TWO_OPENINGS_VIEW_CELL, at, opening),
        50.0,
        true,
        true,
    );
    assert!(l.outdoors, "the camera stays outdoors");
    // The premise: a room is given a second turn, and the room is drawn.
    assert!(
        l.reached_again > 0,
        "a room is reached through a second opening"
    );
    assert!(
        l.cells.contains(&ROOM_REACHED_TWICE),
        "the room is reached: {:X?}",
        l.cells
    );
    assert!(
        l.runs.iter().any(|r| r.cell.0 == ROOM_REACHED_TWICE),
        "pieces of the room are drawn"
    );
    // Each part run is submitted once for each kind of submission, within each pass of the
    // frame.
    let mut seen = BTreeSet::new();
    let again: Vec<_> = l
        .runs
        .iter()
        .filter(|r| {
            !seen.insert((
                r.pass_of_frame,
                r.cell,
                r.blended_list,
                r.batch,
                r.part,
                r.pass,
            ))
        })
        .collect();
    eprintln!(
        "{} part runs submitted, {} rooms reached again; repeated: {again:X?}",
        l.runs.len(),
        l.reached_again
    );
    assert!(again.is_empty(), "no part run is submitted twice");
}
