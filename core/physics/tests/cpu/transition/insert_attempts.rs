//! The insert's attempts: each one starts from the cell the check position is in when it begins,
//! so a body the other cells have carried into a neighboring cell is tested there first the next
//! time, and a body they have carried out of every cell is stopped.
//! Fixture: a synthetic world of two cells of its own side by side, answered for by its land
//! source; cells that record which attempt asked them, the one the body is leaving holding the
//! back of a wall that runs along the move.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dereth_physics::arena::Arena;
use dereth_physics::cell::{Cell, CellArray, CellInfo, HostCell};
use dereth_physics::geom::Sphere;
use dereth_physics::globals::LAND_HEIGHT_TABLE_LEN;
use dereth_physics::source::{EnvCellGeometry, LandSource};
use dereth_physics::transition::insert::transitional_insert;
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState};
use dereth_physics::{LandblockCollision, PhysicsObj, SetupGeometry};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

/// The cell the body is leaving, below 20 m in x, and the one beside it, 20 to 40 m.
const LEAVING: CellId = CellId(0x1020_0001);
const ENTERED: CellId = CellId(0x1020_0002);
const CELL_LENGTH: f32 = 20.0;

/// Which cell each ask went to, and in which attempt.
type Asks = Arc<Mutex<Vec<(u64, CellId)>>>;

/// The cell of a point by its x, or none past the second cell.
fn cell_at(x: f32) -> Option<CellId> {
    if (0.0..CELL_LENGTH).contains(&x) {
        Some(LEAVING)
    } else if (CELL_LENGTH..2.0 * CELL_LENGTH).contains(&x) {
        Some(ENTERED)
    } else {
        None
    }
}

/// A cell with nothing to stand on that records each ask; the one the body is leaving reports, on
/// its first ask, that a sphere touched the back of a wall facing across the move.
#[derive(Debug)]
struct Room {
    id: CellId,
    asks: Asks,
}

impl HostCell for Room {
    fn pos(&self, id: CellId) -> Position {
        Position::new(id, Frame::new(Vec3::ZERO, Quat::IDENTITY))
    }

    fn point_in_cell(&self, local: Vec3) -> bool {
        cell_at(local.x) == Some(self.id)
    }

    fn find_env_collisions(&self, _ctx: &TransitionCtx<'_>, t: &mut Transition) -> TransitionState {
        let mut asks = self.asks.lock().expect("the ask log");
        let first = !asks.iter().any(|(_, id)| *id == self.id);
        asks.push((t.attempt, self.id));
        if self.id == LEAVING && first {
            t.sphere_path
                .set_neg_poly_hit(false, Vec3::new(0.0, 1.0, 0.0));
        }
        TransitionState::Ok
    }
}

/// Two cells of the world's own in one block; the search lists both and finds the check position
/// in the one its first sphere's center is over.
struct TwoRooms {
    table: [f32; LAND_HEIGHT_TABLE_LEN],
    asks: Asks,
}

impl TwoRooms {
    fn new() -> Self {
        Self {
            table: [0.0; LAND_HEIGHT_TABLE_LEN],
            asks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn room(&self, id: CellId) -> Arc<Room> {
        Arc::new(Room {
            id,
            asks: Arc::clone(&self.asks),
        })
    }
}

impl LandSource for TwoRooms {
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
        [LEAVING, ENTERED]
            .contains(&id)
            .then(|| self.room(id) as Arc<dyn HostCell>)
    }

    fn host_cell_list(
        &self,
        _pos: &Position,
        spheres: &[Sphere],
        arr: &mut CellArray,
        want_container: bool,
        _hits_interior_cell: &mut bool,
    ) -> Option<Option<Cell>> {
        arr.cells.clear();
        arr.added_outside = true;
        for id in [LEAVING, ENTERED] {
            arr.cells.push(CellInfo {
                cell_id: id,
                cell: Some(Cell::Host {
                    id,
                    cell: self.room(id),
                }),
            });
        }
        let container = cell_at(spheres.first()?.center.x).map(|id| Cell::Host {
            id,
            cell: self.room(id),
        });
        Some(container.filter(|_| want_container))
    }
}

/// A body in the air at `from_x` in the cell it is leaving, its check position carried to `to_x`
/// while the check cell is still that cell, as a sub-step leaves it.
fn crossing(from_x: f32, to_x: f32) -> (Transition, Arena<PhysicsObj>) {
    let mut objects = Arena::new();
    let mover = objects.insert(PhysicsObj::new(
        ObjectId(1),
        Arc::new(SetupGeometry::dummy()),
        0.0,
        true,
    ));
    let mut t = Transition::default();
    t.object_info.object = Some(mover);
    t.object_info.scale = 1.0;
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    let at = |x| Position::new(LEAVING, Frame::new(Vec3::new(x, 10.0, 5.0), Quat::IDENTITY));
    let (from, to) = (at(from_x), at(to_x));
    t.sphere_path.init_path(Some(LEAVING), Some(from), &to);
    t.sphere_path.set_check_pos(&from, Some(LEAVING));
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(to_x - from_x, 0.0, 0.0));
    (t, objects)
}

/// A body whose check position has crossed into the neighboring cell is moved into it by the
/// check over the other cells; when the attempt does not settle (here the body slid along the back
/// of a wall), the next attempt asks the neighboring cell first and passes over it among the
/// others, rather than starting again from the cell the body left.
///
/// Behaviour: physics.transition.each-insert-attempt-starts-from-the-cell-the-check-position-is-in
#[test]
fn each_insert_attempt_starts_from_the_cell_the_check_position_is_in() {
    let world = TwoRooms::new();
    let (mut t, objects) = crossing(19.8, 20.2);
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &world,
        objects: &objects,
        cells: &cells,
        mover: t.object_info.object,
        object_table: None,
        entry_host: None,
    };

    assert_eq!(transitional_insert(&ctx, &mut t, 3), TransitionState::Ok);
    assert_eq!(t.sphere_path.check_cell, Some(ENTERED));
    let asks = world.asks.lock().expect("the ask log").clone();
    assert_eq!(
        asks,
        vec![(1, LEAVING), (1, ENTERED), (2, ENTERED), (2, LEAVING)],
        "the second attempt asks the cell the body is in now, then the one it left"
    );
}

/// A body whose check position has left both cells falls out of them, and an attempt that begins
/// with no cell to start from stops the body rather than asking the cell it left again.
///
/// Behaviour: physics.transition.an-insert-attempt-with-no-check-cell-collides
#[test]
fn an_insert_attempt_with_no_check_cell_collides() {
    let world = TwoRooms::new();
    // From the leaving cell's west edge out past it: the check position is over neither cell.
    let (mut t, objects) = crossing(0.2, -0.2);
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &world,
        objects: &objects,
        cells: &cells,
        mover: t.object_info.object,
        object_table: None,
        entry_host: None,
    };

    assert_eq!(
        transitional_insert(&ctx, &mut t, 3),
        TransitionState::Collided
    );
    assert_eq!(t.sphere_path.check_cell, None);
    let asks = world.asks.lock().expect("the ask log").clone();
    assert_eq!(
        asks,
        vec![(1, LEAVING), (1, ENTERED)],
        "nothing is asked once the check position has left every cell"
    );
}
