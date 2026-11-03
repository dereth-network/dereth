//! The transition's settings: a world that tunes the swept-sphere core, and a world whose cells
//! the core does not build itself.
//! Fixture: a synthetic world of flat ground on 160 m landblocks of 20 m cells, answered for by
//! its own land source; synthetic contact planes and spheres.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use dereth_physics::arena::Arena;
use dereth_physics::cell::{Cell, CellArray, CellInfo, HostCell};
use dereth_physics::geom::{Plane, Sphere};
use dereth_physics::globals::LAND_HEIGHT_TABLE_LEN;
use dereth_physics::source::{EnvCellGeometry, LandSource};
use dereth_physics::transition::insert::{
    calc_num_steps, find_transitional_position, transitional_insert, validate_transition,
};
use dereth_physics::transition::place::{
    find_placement_pos, scatter_sequence, scatter_sequence_by, scatter_sequence_from,
};
use dereth_physics::transition::{
    collide, walk, ObjectInfoState, Slide, Transition, TransitionCtx, TransitionRules,
    TransitionState, Turning,
};
use dereth_physics::{LandblockCollision, PhysicsObj, PhysicsState, SetupGeometry, V3};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

/// A world's rules as another Turbine world sets them: shorter blocks, a steeper walkable limit, a
/// shorter sub-step, a wider placement floor with rings counted by nine tenths of the radius, and
/// the other choice of every switch.
fn other_rules() -> TransitionRules {
    TransitionRules {
        walkable_z: 0.700_91,
        step_fraction: 0.9,
        block_length: 160.0,
        placement_radius_floor: 0.5,
        stop_on_restored_contact: false,
        retry_each_cell: false,
        recheck_landing_with_budget: false,
        edge_slide_offset_across_blocks: true,
        step_down_guards_zero_drop: true,
        placement_retries_other_cells: true,
        turning: Turning::Plain,
        placement_ring_fraction: 0.9,
        placement_lifts_before_scatter: true,
        parallel_band: 0.0,
        env_collision_normal_up: true,
        slide: None,
    }
}

const CELL_LENGTH: f32 = 20.0;
const BLOCK_LENGTH: f32 = 160.0;

/// Flat ground at `z = 0` in every 20 m cell, counting how often it is asked for collisions.
#[derive(Debug)]
struct Ground {
    id: CellId,
    asks: Arc<AtomicU32>,
    /// What it answers when it is not touched: `Ok`, or always `Adjusted` to make the transition
    /// retry.
    always_adjust: bool,
    /// It refuses a sphere reaching below the ground (`Collided`) rather than lifting it.
    refuse_below: bool,
}

/// The land cell (1 to 64) of a point in a 160 m block, x-major as the land cells are numbered.
fn cell_index(local: Vec3) -> Option<u16> {
    if !(0.0..BLOCK_LENGTH).contains(&local.x) || !(0.0..BLOCK_LENGTH).contains(&local.y) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x, y) = (
        (local.x / CELL_LENGTH) as u16,
        (local.y / CELL_LENGTH) as u16,
    );
    Some(x * 8 + y + 1)
}

impl HostCell for Ground {
    fn pos(&self, id: CellId) -> Position {
        Position::new(id, Frame::new(Vec3::ZERO, Quat::IDENTITY))
    }

    fn point_in_cell(&self, local: Vec3) -> bool {
        cell_index(local) == Some(self.id.index())
    }

    fn find_env_collisions(&self, _ctx: &TransitionCtx<'_>, t: &mut Transition) -> TransitionState {
        self.asks.fetch_add(1, Ordering::Relaxed);
        if self.always_adjust {
            return TransitionState::Adjusted;
        }
        if self.refuse_below {
            let s = t.sphere_path.global_sphere[0];
            return if s.center.z - s.radius < 0.0 {
                TransitionState::Collided
            } else {
                TransitionState::Ok
            };
        }
        let off = t.rules.block_offset(t.sphere_path.check_pos.cell, self.id);
        let mut s = t.sphere_path.global_sphere[0];
        s.center = s.center.sub(off);
        let ground = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        collide::validate_walkable(t, &s, &ground, false, 0.0, self.id)
    }
}

/// A world of 160 m blocks whose land source builds its own cells.
struct HostWorld {
    table: [f32; LAND_HEIGHT_TABLE_LEN],
    asks: Arc<AtomicU32>,
    always_adjust: bool,
    refuse_below: bool,
}

impl HostWorld {
    fn new(always_adjust: bool) -> Self {
        Self {
            table: [0.0; LAND_HEIGHT_TABLE_LEN],
            asks: Arc::new(AtomicU32::new(0)),
            always_adjust,
            refuse_below: false,
        }
    }

    fn cell(&self, id: CellId) -> Arc<Ground> {
        Arc::new(Ground {
            id,
            asks: Arc::clone(&self.asks),
            always_adjust: self.always_adjust,
            refuse_below: self.refuse_below,
        })
    }
}

impl LandSource for HostWorld {
    fn landblock(&self, _id: LandblockId) -> Option<Arc<LandblockCollision>> {
        None
    }

    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN] {
        &self.table
    }

    fn env_cell(&self, _cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        None
    }

    fn host_cell(&self, id: CellId) -> Option<Arc<dyn HostCell>> {
        (1..=64)
            .contains(&id.index())
            .then(|| self.cell(id) as Arc<dyn HostCell>)
    }

    fn host_cell_list(
        &self,
        pos: &Position,
        spheres: &[Sphere],
        arr: &mut CellArray,
        want_container: bool,
        _hits_interior_cell: &mut bool,
    ) -> Option<Option<Cell>> {
        arr.cells.clear();
        arr.added_outside = true;
        let c = spheres.first()?.center;
        // The block the center has walked into, and the center in its frame.
        let block = pos.cell.landblock();
        #[allow(clippy::cast_possible_truncation)]
        let (dx, dy) = (
            (c.x / BLOCK_LENGTH).floor() as i32,
            (c.y / BLOCK_LENGTH).floor() as i32,
        );
        let (bx, by) = (i32::from(block.x()) + dx, i32::from(block.y()) + dy);
        let local = Vec3::new(
            c.x - dx as f32 * BLOCK_LENGTH,
            c.y - dy as f32 * BLOCK_LENGTH,
            c.z,
        );
        let (Ok(bx), Ok(by), Some(index)) = (u8::try_from(bx), u8::try_from(by), cell_index(local))
        else {
            return Some(None);
        };
        let id = CellId(u32::from(bx) << 24 | u32::from(by) << 16 | u32::from(index));
        let cell = Cell::Host {
            id,
            cell: self.cell(id),
        };
        arr.cells.push(CellInfo {
            cell_id: id,
            cell: Some(cell.clone()),
        });
        Some(want_container.then_some(cell))
    }
}

/// A transition for one sphere of radius 0.5 resting on the ground at `from`, moving to `to`.
fn walk(rules: TransitionRules, from: Position, to: Position) -> (Transition, Arena<PhysicsObj>) {
    let mut objects = Arena::new();
    let mover = objects.insert(PhysicsObj::new(
        ObjectId(1),
        Arc::new(SetupGeometry::dummy()),
        0.0,
        true,
    ));
    let mut t = Transition::default();
    t.set_rules(rules);
    t.object_info.object = Some(mover);
    t.object_info.state = ObjectInfoState::CONTACT | ObjectInfoState::ON_WALKABLE;
    t.object_info.scale = 1.0;
    t.object_info.step_up_height = 0.6;
    t.object_info.step_down_height = 0.6;
    t.object_info.step_down = true;
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    t.sphere_path.init_path(Some(from.cell), Some(from), &to);
    let ground = Plane {
        normal: Vec3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    t.init_contact_plane(from.cell, ground, false);
    (t, objects)
}

fn at(cell: u32, x: f32, y: f32) -> Position {
    Position::new(
        CellId(cell),
        Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
    )
}

/// A world whose land source answers for its own cells walks a body across them: on 160 m blocks,
/// a body walking east past a block's edge stands in the next block's first column of cells, its
/// position re-measured from that block's corner by the world's block length.
///
/// Behaviour: physics.host-cells.a-world-with-its-own-cells-walks-a-body-across-them
#[test]
fn a_world_with_its_own_cells_walks_a_body_across_them() {
    let world = HostWorld::new(false);
    // Cell 57 of block (0x10, 0x20) is its east column (x 140..160), row 0.
    let from = at(0x1020_0039, 159.5, 5.0);
    let to = at(0x1020_0039, 160.5, 5.0);
    let (mut t, objects) = walk(other_rules(), from, to);
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &world,
        objects: &objects,
        cells: &cells,
        mover: t.object_info.object,
        object_table: None,
        entry_host: None,
    };
    let state = PhysicsState::DEFAULT;
    assert!(
        find_transitional_position(&ctx, &mut t, state),
        "the walk succeeds"
    );
    let end = t.sphere_path.curr_pos;
    assert_eq!(end.cell, CellId(0x1120_0001), "the next block's first cell");
    assert!(
        (end.frame.origin.x - 0.5).abs() < 1e-3,
        "measured from the next block's corner by 160 m: {:?}",
        end.frame.origin
    );
    assert!(
        end.frame.origin.z.abs() < 1e-3,
        "on the ground: {:?}",
        end.frame.origin
    );
    assert!(
        world.asks.load(Ordering::Relaxed) > 0,
        "the world's cells were asked"
    );
}

/// A world whose sub-step is a shorter fraction of the sphere's radius splits the same move into
/// more sub-steps: two meters for a 0.5 m sphere is four steps at the full radius, five at 0.9 of it.
///
/// Behaviour: physics.transition.a-world-sets-the-sub-step-length
#[test]
fn a_world_sets_the_sub_step_length() {
    let (from, to) = (at(0x1020_0001, 1.0, 1.0), at(0x1020_0001, 3.0, 1.0));
    let (ours, _) = walk(TransitionRules::default(), from, to);
    let (theirs, _) = walk(other_rules(), from, to);
    assert_eq!(calc_num_steps(&ours).2, 4);
    assert_eq!(calc_num_steps(&theirs).2, 5);
}

/// A world sets where ground stops being walkable: a surface tilted 46 degrees from level is
/// walkable under this client's limit and not under a 45.5 degree one.
///
/// Behaviour: physics.transition.a-world-sets-the-walkable-limit
#[test]
fn a_world_sets_the_walkable_limit() {
    let tilt = 46.0_f64.to_radians();
    #[allow(clippy::cast_possible_truncation)]
    let (sin, cos) = (
        dereth_primitives::num::math::sin(tilt) as f32,
        dereth_primitives::num::math::cos(tilt) as f32,
    );
    let slope = Plane {
        normal: Vec3::new(sin, 0.0, cos),
        d: 0.0,
    };
    for (rules, walkable) in [(TransitionRules::default(), true), (other_rules(), false)] {
        let (mut t, objects) = walk(rules, at(0x1020_0001, 1.0, 1.0), at(0x1020_0001, 1.5, 1.0));
        let world = HostWorld::new(false);
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        t.collision_info.set_contact_plane(slope, false);
        let _ = validate_transition(&ctx, &mut t, TransitionState::Ok, true);
        assert_eq!(
            t.object_info.has(ObjectInfoState::ON_WALKABLE),
            walkable,
            "walkable z {}",
            rules.walkable_z
        );
    }
}

/// When a move comes to rest against something while the ground it last stood on is still under
/// it, the ground is restored either way; this client also stops the body dead, and a world may
/// keep its velocity instead.
///
/// Behaviour: physics.transition.a-world-chooses-whether-restored-ground-stops-the-body
#[test]
fn a_world_chooses_whether_restored_ground_stops_the_body() {
    for (rules, stops) in [(TransitionRules::default(), true), (other_rules(), false)] {
        let (mut t, objects) = walk(rules, at(0x1020_0001, 1.0, 1.0), at(0x1020_0001, 1.5, 1.0));
        let world = HostWorld::new(false);
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        t.sphere_path.cache_global_curr_center();
        let _ = validate_transition(&ctx, &mut t, TransitionState::Collided, true);
        assert!(
            t.collision_info.contact_plane_valid,
            "the ground is restored"
        );
        assert_eq!(t.kill_velocity, stops);
    }
}

/// This client asks a cell for its collisions again after each adjustment, up to the attempt
/// budget, inside each attempt; a world may ask it once an attempt. A cell that always adjusts is
/// asked nine times by a three-attempt insert here, three times there.
///
/// Behaviour: physics.transition.a-world-chooses-whether-each-cell-is-retried
#[test]
fn a_world_chooses_whether_each_cell_is_retried() {
    for (rules, asks) in [(TransitionRules::default(), 9), (other_rules(), 3)] {
        let world = HostWorld::new(true);
        let (mut t, objects) = walk(rules, at(0x1020_0001, 1.0, 1.0), at(0x1020_0001, 1.5, 1.0));
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        let check = t.sphere_path.curr_pos;
        t.sphere_path.set_check_pos(&check, Some(check.cell));
        assert_eq!(
            transitional_insert(&ctx, &mut t, 3),
            TransitionState::Adjusted
        );
        assert_eq!(world.asks.load(Ordering::Relaxed), asks);
    }
}

/// Placement scatters its candidate points from a smallest radius the world sets: a 0.3 m body
/// scatters as a 0.48 m one in this client, as a 0.5 m one in a world whose floor is 0.5.
///
/// Behaviour: physics.transition.a-world-sets-the-placement-scatter-floor
#[test]
fn a_world_sets_the_placement_scatter_floor() {
    assert_eq!(scatter_sequence(0.3), scatter_sequence_from(0.3, 0.48));
    assert_eq!(scatter_sequence_from(0.3, 0.5), scatter_sequence(0.5));
    assert_ne!(scatter_sequence(0.3), scatter_sequence(0.5));
}

/// A world may count the placement scatter's rings by a fraction of the radius: a 0.5 m body
/// scatters in rings 0.5 m apart, eight samples in the first, in this client, and in rings 4/9 m
/// apart, six in the first, where the rings are counted by nine tenths of it.
///
/// Behaviour: physics.transition.a-world-sets-how-placement-scatters
#[test]
fn a_world_sets_how_placement_scatters() {
    let ours = scatter_sequence(0.5);
    assert_eq!(ours.iter().filter(|s| s.ring == 0).count(), 8);
    assert!((ours[0].distance - 0.5).abs() < 1e-6);
    assert_eq!(ours.last().map(|s| s.ring), Some(7));
    let tenths = scatter_sequence_by(0.5, 0.5, 0.9);
    assert_eq!(tenths.iter().filter(|s| s.ring == 0).count(), 6);
    assert!((tenths[0].distance - 4.0 / 9.0).abs() < 1e-6);
    assert_eq!(tenths.last().map(|s| s.ring), Some(8));
    assert_eq!(scatter_sequence_by(0.5, 0.48, 1.0), ours);
}

/// A world may try a placement that may slide, refused where it was asked, once more 0.1 m above
/// before the scatter: a body asked to stand 5 cm into ground that refuses it stands 5 cm above
/// it there; in this client the scatter finds no room at that height and the placement fails.
///
/// Behaviour: physics.transition.a-world-may-lift-a-placement-before-scattering
#[test]
fn a_world_may_lift_a_placement_before_scattering() {
    for (rules, placed) in [(TransitionRules::default(), false), (other_rules(), true)] {
        let world = HostWorld {
            refuse_below: true,
            ..HostWorld::new(false)
        };
        let mut asked = at(0x1020_0001, 30.0, 30.0);
        asked.frame.origin.z = -0.05;
        let (mut t, objects) = walk(rules, asked, asked);
        t.sphere_path.init_path(Some(asked.cell), None, &asked);
        t.sphere_path.placement_allows_sliding = true;
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        assert_eq!(find_placement_pos(&ctx, &mut t), placed);
        if placed {
            let at = t.sphere_path.check_pos.frame.origin;
            assert!((at.z - 0.05).abs() < 1e-5, "{at:?}");
            assert!((at.x - 30.0).abs() < 1e-5 && (at.y - 30.0).abs() < 1e-5);
        }
    }
}

/// A square floor 10 m across, solid below it, open above, within 7.2 m of its middle.
fn square_floor() -> dereth_physics::geom::bsp::BspTree {
    use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
    use dereth_physics::geom::Polygon;
    let square = Polygon::new(vec![
        Vec3::new(-5.0, -5.0, 0.0),
        Vec3::new(5.0, -5.0, 0.0),
        Vec3::new(5.0, 5.0, 0.0),
        Vec3::new(-5.0, 5.0, 0.0),
    ]);
    let bound = Sphere::new(Vec3::ZERO, 7.2);
    let leaf = |solid: bool, in_polys: Vec<u32>| BspNode {
        sphere: bound,
        splitting_plane: Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 1000.0,
        },
        pos_child: None,
        neg_child: None,
        kind: BspNodeKind::Leaf {
            leaf_index: 0,
            solid,
        },
        in_polys,
    };
    BspTree {
        nodes: vec![
            BspNode {
                sphere: bound,
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 0.0,
                },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: vec![],
            },
            leaf(false, vec![]),
            leaf(true, vec![0]),
        ],
        polygons: vec![square],
    }
}

/// A world whose objects take a scale along each axis has their trees tested at it: a square
/// floor 10 m across stretched three times along x meets a body placed 12 m along x from its
/// middle, sunk 0.2 m into its top, and not one placed 6 m along y; tested at one scale along every
/// axis it meets neither. At an even scale along each axis it meets a body where the one scale
/// does, and lets one stand where the one scale does.
///
/// Behaviour: physics.transition.a-world-may-test-an-objects-trees-at-a-scale-along-each-axis
#[test]
fn a_world_may_test_an_objects_trees_at_a_scale_along_each_axis() {
    use dereth_physics::source::PhysicsPart;
    use dereth_physics::transition::TreeScale;
    let world = HostWorld::new(false);
    let middle = Vec3::new(80.0, 80.0, 10.0);
    let part = PhysicsPart {
        pos: Position::new(CellId(0x1020_0001), Frame::new(middle, Quat::IDENTITY)),
        physics_bsp: Some(Arc::new(square_floor())),
        ..PhysicsPart::default()
    };
    let placed = |dx: f32, dy: f32, dz: f32, scale: TreeScale| {
        let mut asked = at(0x1020_0001, middle.x + dx, middle.y + dy);
        asked.frame.origin.z = middle.z + dz;
        let (mut t, objects) = walk(TransitionRules::default(), asked, asked);
        t.sphere_path.init_path(Some(asked.cell), None, &asked);
        t.sphere_path.set_check_pos(&asked, Some(asked.cell));
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        collide::object_part_find_obj_collisions_scaled(&ctx, &mut t, &part, scale)
    };
    let stretched = TreeScale::axes(Vec3::new(3.0, 1.0, 1.0));
    assert_eq!(
        placed(12.0, 0.0, -0.2, stretched),
        TransitionState::Collided
    );
    assert_eq!(placed(0.0, 6.0, -0.2, stretched), TransitionState::Ok);
    assert_eq!(
        placed(12.0, 0.0, -0.2, TreeScale::Uniform(1.0)),
        TransitionState::Ok
    );
    for (dx, dy, dz) in [
        (8.0, 0.0, -0.2),
        (0.0, 8.0, -0.2),
        (11.0, 0.0, -0.2),
        (3.0, 3.0, 0.1),
    ] {
        assert_eq!(
            placed(dx, dy, dz, TreeScale::axes(Vec3::new(2.0, 2.0, 2.0))),
            placed(dx, dy, dz, TreeScale::Uniform(2.0)),
            "({dx}, {dy}, {dz})"
        );
    }
    assert_eq!(
        placed(8.0, 0.0, -0.2, TreeScale::Uniform(2.0)),
        TransitionState::Collided
    );
    assert_eq!(
        placed(3.0, 3.0, 0.1, TreeScale::Uniform(2.0)),
        TransitionState::Ok
    );
}

/// A world sets how near to parallel the polygon tests take a movement to be: a body sunk a
/// quarter of a millimeter into a floor and stepping down a little under 0.0002 m is lifted onto
/// it in a world whose band is 0, and left where it is in this client's (whose band is 0.0002);
/// a step down a little over the band lifts it in both.
///
/// Behaviour: physics.transition.a-world-sets-the-polygon-tests-parallel-band
#[test]
fn a_world_sets_the_polygon_tests_parallel_band() {
    use dereth_physics::source::PhysicsPart;
    use dereth_physics::transition::TreeScale;
    let world = HostWorld::new(false);
    let middle = Vec3::new(80.0, 80.0, 10.0);
    let part = PhysicsPart {
        pos: Position::new(CellId(0x1020_0001), Frame::new(middle, Quat::IDENTITY)),
        physics_bsp: Some(Arc::new(square_floor())),
        ..PhysicsPart::default()
    };
    let step_down = |rules: TransitionRules, amount: f32| {
        let mut asked = at(0x1020_0001, middle.x, middle.y);
        asked.frame.origin.z = middle.z - 0.000_25;
        let (mut t, objects) = walk(rules, asked, asked);
        t.sphere_path.set_check_pos(&asked, Some(asked.cell));
        t.sphere_path.step_down = true;
        t.sphere_path.step_down_amt = amount;
        t.sphere_path.walk_interp = 1.0;
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        let r = collide::object_part_find_obj_collisions_scaled(
            &ctx,
            &mut t,
            &part,
            TreeScale::Uniform(1.0),
        );
        (r, t.sphere_path.check_pos.frame.origin.z)
    };
    let exact = TransitionRules {
        parallel_band: 0.0,
        ..TransitionRules::default()
    };
    let (lifted, z) = step_down(exact, 0.000_18);
    assert_eq!(lifted, TransitionState::Adjusted);
    assert!((z - middle.z).abs() < 2e-6, "on the floor: {z}");
    assert_eq!(
        step_down(TransitionRules::default(), 0.000_18).0,
        TransitionState::Ok
    );
    for rules in [exact, TransitionRules::default()] {
        assert_eq!(step_down(rules, 0.000_21).0, TransitionState::Adjusted);
    }
}

thread_local! {
    /// The normals the recording slide below was handed, on this test's thread.
    static HANDED: std::cell::RefCell<Vec<Vec3>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A world's own slide that records the normal it is handed, turns it straight down, and collides
/// without moving the mover.
fn recording_slide(
    _t: &mut Transition,
    _sphere: &Sphere,
    normal: &mut Vec3,
    _curr: Vec3,
) -> TransitionState {
    HANDED.with(|h| h.borrow_mut().push(*normal));
    *normal = Vec3::new(0.0, 0.0, -1.0);
    TransitionState::Collided
}

/// A world's own turn of a tree's normal into the world: always along y.
fn turning_to_y(_frame: &Frame, _normal: Vec3) -> Vec3 {
    Vec3::new(0.0, 1.0, 0.0)
}

/// The normals handed to the recording slide since the last asking.
fn handed() -> Vec<Vec3> {
    HANDED.with(|h| std::mem::take(&mut *h.borrow_mut()))
}

fn with_own_slide() -> TransitionRules {
    TransitionRules {
        slide: Some(Slide {
            along: recording_slide,
            turn: turning_to_y,
        }),
        ..TransitionRules::default()
    }
}

/// A two-sphere body over the square floor's middle, sphere `low` (0 or 1) sunk 0.1 m into its top
/// and moving down into it, the other 5 m above, in contact with the ground.
fn over_the_floor(rules: TransitionRules, low: usize) -> Transition {
    let mut t = Transition::default();
    t.set_rules(rules);
    let (a, b) = if low == 0 { (0.4, 5.4) } else { (5.4, 0.4) };
    t.sphere_path.init_sphere(
        &[
            Sphere::new(Vec3::new(0.0, 0.0, a), 0.5),
            Sphere::new(Vec3::new(0.0, 0.0, b), 0.5),
        ],
        1.0,
    );
    let cell = CellId(0x1020_0001);
    let from = Position::new(cell, Frame::new(Vec3::new(0.0, 0.0, 0.2), Quat::IDENTITY));
    let to = Position::new(cell, Frame::new(Vec3::ZERO, Quat::IDENTITY));
    t.sphere_path.init_path(Some(cell), Some(from), &to);
    t.sphere_path.set_check_pos(&to, Some(cell));
    t.sphere_path.cache_localspace_sphere(&to, 1.0);
    t.object_info.set(ObjectInfoState::CONTACT, true);
    t
}

/// A world may slide the mover by its own slide wherever the transition slides it along a surface:
/// the step-up slide (which keeps the step-up normal as the world's slide turned it), the slide
/// along the edge of the ground a body would walk off, a back face's slide (keeping its normal as
/// turned), and a physics tree's step-up slide and second sphere's slide, whose normals the
/// world's own turn takes into the world. This client's slide leaves every normal as it was. A
/// mover that slides along what it meets takes this client's slide in any world.
///
/// Behaviour: physics.transition.a-world-may-slide-by-its-own-slide
#[test]
fn a_world_may_slide_by_its_own_slide() {
    let east = Vec3::new(1.0, 0.0, 0.0);
    let down = Vec3::new(0.0, 0.0, -1.0);

    // The step-up slide.
    for (rules, own) in [
        (TransitionRules::default(), false),
        (with_own_slide(), true),
    ] {
        let mut t = over_the_floor(rules, 0);
        t.sphere_path.step_up = true;
        t.sphere_path.step_up_normal = east;
        let r = collide::step_up_slide(&mut t);
        assert!(!t.sphere_path.step_up && !t.collision_info.contact_plane_valid);
        if own {
            assert_eq!((r, handed()), (TransitionState::Collided, vec![east]));
            assert_eq!(t.sphere_path.step_up_normal, down);
        } else {
            assert_eq!(handed(), vec![]);
            assert_eq!(t.sphere_path.step_up_normal, east);
        }

        // A mover that slides along what it meets takes this client's slide.
        let mut t = over_the_floor(rules, 0);
        t.object_info.slides = true;
        t.sphere_path.step_up_normal = east;
        let _ = collide::step_up_slide(&mut t);
        assert_eq!(handed(), vec![]);
    }

    // The slide along the edge of the ground the body would walk off: a floor 1 m across under the
    // body's first sphere, which has moved east past its east edge.
    let floor = dereth_physics::geom::Polygon::new(vec![
        Vec3::new(-1.0, -0.5, 0.0),
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::new(-1.0, 0.5, 0.0),
    ]);
    for (rules, own) in [
        (TransitionRules::default(), false),
        (with_own_slide(), true),
    ] {
        let mut t = over_the_floor(rules, 0);
        t.sphere_path.walkable = Some(floor.clone());
        t.sphere_path.walkable_check_pos = Sphere::new(Vec3::new(0.3, 0.0, 0.5), 0.5);
        t.sphere_path.walkable_up = Vec3::new(0.0, 0.0, 1.0);
        t.sphere_path.walkable_pos = t.sphere_path.check_pos;
        t.sphere_path.global_sphere[0].center = Vec3::new(0.3, 0.0, 0.5);
        t.sphere_path.global_curr_center[0] = Vec3::new(-0.2, 0.0, 0.5);
        let _ = walk::precipice_slide(&mut t);
        assert_eq!(handed().len(), usize::from(own));
    }

    // A back face's slide inside the insert.
    for (rules, own) in [
        (TransitionRules::default(), false),
        (with_own_slide(), true),
    ] {
        let world = HostWorld::new(false);
        let (mut t, objects) = walk(rules, at(0x1020_0001, 1.0, 1.0), at(0x1020_0001, 1.5, 1.0));
        let cells = BTreeMap::new();
        let ctx = TransitionCtx {
            land: &world,
            objects: &objects,
            cells: &cells,
            mover: t.object_info.object,
            object_table: None,
            entry_host: None,
        };
        let check = t.sphere_path.curr_pos;
        t.sphere_path.set_check_pos(&check, Some(check.cell));
        t.sphere_path.neg_poly_hit = true;
        t.sphere_path.neg_step_up = false;
        t.sphere_path.neg_collision_normal = east;
        let _ = transitional_insert(&ctx, &mut t, 1);
        if own {
            assert_eq!(handed(), vec![east]);
            assert_eq!(t.sphere_path.neg_collision_normal, down);
        } else {
            assert_eq!(handed(), vec![]);
            assert_eq!(t.sphere_path.neg_collision_normal, east);
        }
    }

    // A physics tree's step-up slide (the first sphere sunk into its top, with nowhere to step up
    // to) and its second sphere's slide: the normal handed over is the world's own turn of the
    // floor's.
    let tree = square_floor();
    let land = dereth_physics::StaticLandSource::linear();
    let objects = Arena::new();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    for low in [0, 1] {
        let mut t = over_the_floor(TransitionRules::default(), low);
        assert_ne!(
            collide::bsp_find_collisions(&ctx, &mut t, &tree, 1.0),
            TransitionState::Ok
        );
        assert_eq!(handed(), vec![]);
        let mut t = over_the_floor(with_own_slide(), low);
        assert_eq!(
            collide::bsp_find_collisions(&ctx, &mut t, &tree, 1.0),
            TransitionState::Collided
        );
        assert_eq!(handed(), vec![Vec3::new(0.0, 1.0, 0.0)], "sphere {low}");
    }
}

/// A cell with no portals holds no point at all in this client, whatever its cell tree says; a
/// world whose closed rooms are stood in takes its cell tree's word for it.
///
/// Behaviour: physics.cells.a-world-chooses-whether-a-cell-with-no-portals-holds-points
#[test]
fn a_world_chooses_whether_a_cell_with_no_portals_holds_points() {
    use dereth_physics::geom::{BspNode, BspNodeKind, BspTree};
    let above_the_floor = BspTree {
        nodes: vec![BspNode {
            sphere: Sphere::default(),
            splitting_plane: Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Node,
            in_polys: Vec::new(),
        }],
        polygons: Vec::new(),
    };
    let id = CellId(0x1020_0100);
    let room = |holds_without_portals| Cell::Env {
        id,
        geom: Arc::new(EnvCellGeometry {
            id,
            cell_bsp: Some(Arc::new(above_the_floor.clone())),
            holds_without_portals,
            ..EnvCellGeometry::default()
        }),
    };
    let (inside, outside) = (Vec3::new(1.0, 1.0, 1.0), Vec3::new(1.0, 1.0, -1.0));
    assert!(!room(false).point_in_cell(inside));
    assert!(room(true).point_in_cell(inside));
    assert!(!room(true).point_in_cell(outside));
}

/// A mover in the air that an interior cell's tree moves or stops has collided with it. A world may
/// make that a collision straight up when nothing has named another normal; this client names none.
///
/// Behaviour: physics.transition.a-world-may-take-an-interior-collision-as-straight-up
#[test]
fn a_world_may_take_an_interior_collision_as_straight_up() {
    let id = CellId(0x1020_0100);
    let cell = Cell::Env {
        id,
        geom: Arc::new(EnvCellGeometry {
            id,
            physics_bsp: Some(Arc::new(square_floor())),
            ..EnvCellGeometry::default()
        }),
    };
    let land = dereth_physics::StaticLandSource::linear();
    let objects = Arena::new();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    for (rules, normal) in [
        (TransitionRules::default(), None),
        (other_rules(), Some(Vec3::new(0.0, 0.0, 1.0))),
    ] {
        let mut t = over_the_floor(rules, 0);
        t.object_info.set(ObjectInfoState::CONTACT, false);
        t.sphere_path.check_pos.cell = id;
        let r = collide::env_find_env_collisions(&ctx, &mut t, &cell);
        assert_ne!(r, TransitionState::Ok, "the floor is met");
        let ci = &t.collision_info;
        assert_eq!(
            ci.collision_normal_valid.then_some(ci.collision_normal),
            normal
        );
        assert!(ci.collided_with_environment);
    }
}
