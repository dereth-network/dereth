//! This world's settings for the shared transition, and what the shared transition does in this
//! world's land.
//! Fixture: the constants themselves; level ground on one 160 m landblock, land with no block
//! loaded, and a room with a doorway out standing on level ground, built in the test.

use std::collections::BTreeMap;

use auberean_physics::cells::OUTDOORS;
use auberean_physics::land::Block;
use auberean_physics::rules::{rules, BLOCK_LENGTH, WALKABLE_Z};
use auberean_physics::Land;
use dereth_physics::arena::Arena;
use dereth_physics::cell::{CellArray, CellResolver};
use dereth_physics::geom::{Polygon, Sphere};
use dereth_physics::source::{CellPortal, EnvCellGeometry, LandSource};
use dereth_physics::transition::insert::check_other_cells;
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState, Turning};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

/// Ground stops being walkable at 45.5 degrees: the walkable limit is the cosine of 45.5 degrees,
/// rounded once to a 32-bit float; the sub-step is nine tenths of the sphere's radius, the
/// landblock 160 m, the placement floor 0.5 m; restored ground keeps the velocity, each cell is
/// asked once an attempt, a landing is re-checked once, and an edge slide measures across blocks.
///
/// Behaviour: motion.rules.this-worlds-transition-settings
#[test]
fn this_worlds_transition_settings() {
    // The host's own cosine as the reference the constant was written from.
    #[allow(clippy::cast_possible_truncation, clippy::disallowed_methods)]
    let cos = 45.5f64.to_radians().cos() as f32;
    assert_eq!(WALKABLE_Z.to_bits(), cos.to_bits());
    let r = rules();
    assert_eq!(r.walkable_z.to_bits(), WALKABLE_Z.to_bits());
    assert_eq!(
        (r.step_fraction, r.block_length, r.placement_radius_floor),
        (0.9, 160.0, 0.5)
    );
    assert_eq!(BLOCK_LENGTH, 160.0);
    assert!(!r.stop_on_restored_contact && !r.retry_each_cell && !r.recheck_landing_with_budget);
    assert!(r.edge_slide_offset_across_blocks);
}

/// A placement asks the cell it is in and then the others again after any adjustment, up to three
/// times; a move turns its sub-steps by a plain slerp, blending straight within 0.001 of no turn.
///
/// Behaviour: transition.rules.placements-retry-and-moves-turn-plainly
#[test]
fn placements_retry_the_other_cells_and_moves_turn_plainly() {
    let r = rules();
    assert!(r.placement_retries_other_cells);
    assert_eq!(r.turning, Turning::Plain);
}

/// The polygon tests take a movement as lying along a polygon only when it is exactly: a sphere
/// sunk a quarter of a millimeter into a floor and moving down into it by 0.00018 m is lifted
/// onto it, and an up vector lying in a polygon's plane is not refused, it finds the polygon's
/// footprint.
///
/// Behaviour: transition.rules.the-polygon-tests-take-only-an-exact-parallel-as-parallel
#[test]
fn the_polygon_tests_take_only_an_exact_parallel_as_parallel() {
    let band = rules().parallel_band;
    assert_eq!(band.to_bits(), 0.0f32.to_bits());
    let floor = Polygon::new(vec![
        Vec3::new(-4.0, -4.0, 0.0),
        Vec3::new(4.0, -4.0, 0.0),
        Vec3::new(4.0, 4.0, 0.0),
        Vec3::new(-4.0, 4.0, 0.0),
    ]);
    let mut center = Vec3::new(0.5, 0.5, 0.5 - 0.000_25);
    let lifted =
        floor.adjust_sphere_to_plane(&mut center, 0.5, Vec3::new(0.0, 0.0, -0.000_18), 1.0, band);
    assert!(lifted.is_some(), "lifted onto the floor");
    assert!((center.z - 0.5).abs() < 1e-6, "{center:?}");
    let sideways = Vec3::new(1.0, 0.0, 0.0);
    let above = Sphere::new(Vec3::new(0.5, 0.5, 1.0), 0.5);
    assert!(floor.check_walkable(&above, sideways, band));
    assert!(!floor.check_walkable(&above, sideways, dereth_physics::globals::EPSILON));
}

/// A transition for one sphere of radius 0.5 standing at `at`.
fn standing(at: Position) -> Transition {
    let mut t = Transition::default();
    t.init();
    t.set_rules(rules());
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    t.sphere_path.init_path(Some(at.cell), Some(at), &at);
    t.sphere_path.set_check_pos(&at, Some(at.cell));
    t
}

/// A move that has left every cell it knew, over land whose block is not loaded, falls out into
/// the 20 m land cell its point lies in, numbered across then up from 1.
///
/// Behaviour: transition.cells.a-move-that-leaves-every-cell-falls-into-the-land-cell-under-it
#[test]
fn a_move_that_leaves_every_cell_falls_into_the_land_cell_under_it() {
    let land = Land::new();
    let (objects, cells) = (Arena::new(), BTreeMap::new());
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let at = Position::new(
        CellId(0x1020_0001),
        Frame::new(Vec3::new(100.0, 60.0, 5.0), Quat::IDENTITY),
    );
    let mut t = standing(at);
    assert_eq!(check_other_cells(&ctx, &mut t, None), TransitionState::Ok);
    // Cell 5 across, 3 up: 5 * 8 + 3 + 1.
    assert_eq!(t.sphere_path.check_pos.cell, CellId(0x1020_002C));
    // Off the map it falls nowhere.
    let (mut id, mut o) = (CellId(0xFF20_0001), Vec3::new(170.0, 5.0, 0.0));
    assert!(!land.adjust_to_outside(&mut id, &mut o));
    assert_eq!(id, CellId(0));
}

/// The ground under a point on the land is the first triangle of its land cell it lies over, the
/// cell's squares taken x-major: a point on the line between the cell's first two 10 m squares
/// stands on the first square's triangle, not the second's.
///
/// Behaviour: transition.land.the-ground-under-a-point-is-the-first-triangle-of-its-cell-over-it
#[test]
fn the_ground_under_a_point_is_the_first_triangle_of_its_cell_over_it() {
    let block = Block::flat(0x40, 0x40, 10.0);
    let (ground, _) = block
        .ground_under(Vec3::new(10.0, 3.0, 10.0))
        .expect("ground under it");
    let corner = |x: f32, y: f32| ground.vertices.iter().any(|v| v.x == x && v.y == y);
    assert!(
        corner(0.0, 10.0),
        "the first square's: {:?}",
        ground.vertices
    );
    assert!(!corner(20.0, 0.0), "{:?}", ground.vertices);
}

/// Ground found under a body on the land is the land's own: the contact plane says it is the
/// landscape and not an interior's floor.
///
/// Behaviour: transition.contact.ground-on-the-land-is-the-landscape
#[test]
fn ground_on_the_land_is_the_landscape() {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, 10.0));
    let (objects, cells) = (Arena::new(), BTreeMap::new());
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    // Cell 1 across, 1 up.
    let id = CellId(0x4040_000A);
    let at = Position::new(id, Frame::new(Vec3::new(30.0, 30.0, 10.0), Quat::IDENTITY));
    let mut t = standing(at);
    let cell = land.host_cell(id).expect("a land cell");
    assert_eq!(cell.find_env_collisions(&ctx, &mut t), TransitionState::Ok);
    let ci = &t.collision_info;
    assert!(ci.contact_plane_valid && ci.contact_plane_is_landscape);
    assert!(!ci.contact_plane_is_environment);
}

/// A body inside a room, standing in its doorway out, reaches the land outside: the 20 m land cell
/// under it, numbered across then up from 1 (cell 2 across, 3 up, where a 24 m cell would be 2 and
/// 2), with that cell's ground.
///
/// Behaviour: transition.cells.a-body-in-a-doorway-out-reaches-the-20-m-land-cells-under-it
#[test]
fn a_body_in_a_doorway_out_reaches_the_20_m_land_cells_under_it() {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, 0.0));
    let room = CellId(0x4040_0100);
    // The doorway is the room's own plane y = 0, a 2 m square about the room's origin.
    let doorway = Polygon::new(vec![
        Vec3::new(-1.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, 1.0),
    ]);
    land.insert_cell(EnvCellGeometry {
        id: room,
        frame: Frame::new(Vec3::new(50.0, 70.0, 0.0), Quat::IDENTITY),
        portals: vec![CellPortal {
            other_cell_id: OUTDOORS,
            portal: doorway,
            portal_side: true,
            other_portal_id: 0,
            exact_match: false,
        }],
        ..EnvCellGeometry::default()
    });
    let at = Position::new(room, Frame::new(Vec3::new(50.0, 70.2, 0.0), Quat::IDENTITY));
    let mut arr = CellArray::new();
    let mut interior = false;
    CellResolver::new(&land).find_cell_list(
        &at,
        &[Sphere::new(Vec3::new(50.0, 70.2, 0.5), 0.5)],
        &mut arr,
        false,
        &mut interior,
    );
    let ids: Vec<CellId> = arr.cells.iter().map(|c| c.cell_id).collect();
    assert_eq!(ids, vec![room, CellId(0x4040_0014)]);
    assert!(
        arr.cells[1].cell.is_some(),
        "the land cell's block is in, so it has its ground"
    );
}
