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
use dereth_physics::transition::place::{scatter_sequence, scatter_sequence_from};
use dereth_physics::transition::{
    collide, ObjectInfoState, Transition, TransitionCtx, TransitionRules, TransitionState,
};
use dereth_physics::{LandblockCollision, PhysicsObj, PhysicsState, SetupGeometry, V3};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

/// A world's rules as another Turbine world sets them: shorter blocks, a steeper walkable limit, a
/// shorter sub-step, a wider placement floor, and the other choice of every switch.
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
}

impl HostWorld {
    fn new(always_adjust: bool) -> Self {
        Self {
            table: [0.0; LAND_HEIGHT_TABLE_LEN],
            asks: Arc::new(AtomicU32::new(0)),
            always_adjust,
        }
    }

    fn cell(&self, id: CellId) -> Arc<Ground> {
        Arc::new(Ground {
            id,
            asks: Arc::clone(&self.asks),
            always_adjust: self.always_adjust,
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
