//! Outdoor cells outside the resident window resolve to nothing and widening admits the ring;
//! unknown interior indices resolve to nothing; the cell array has nulls past the window; sweeps
//! straddling the edge get the neighbour by id only; placement into a non-resident block takes the
//! lost-cell arm and ends in no cell.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::globals::LAND_HEIGHT_TABLE_LEN;
use dereth_physics::source::{BuildingGeometry, EnvCellGeometry};
use dereth_physics::{
    landdefs, CellArray, CellResolver, LandSource, LandblockCollision, PhysHandle, PhysicsWorld,
    StaticLandSource,
};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

const HOME: LandblockId = LandblockId(0xA9B4);
/// `add_flat_block(id, 10)` against the linear `2 * i` table.
const GROUND: f32 = 20.0;

// =================================================================================================
// A resident landscape window in miniature
// =================================================================================================

/// A [`LandSource`] whose **decodable** set and **resident** set differ, which is the whole of
/// what `DatLandSource` is and what `StaticLandSource` (whose two sets are the same map) cannot
/// express.
///
/// `resident` represents the loaded block slots the window test indexes
/// and the `load_state == 8` filters. Other blocks remain readable from disk but are
/// absent from the resident array, so resident-block lookup returns no block.
struct WindowedLand {
    inner: StaticLandSource,
    /// The radius and loaded center cell, expressed as the resulting slot
    /// set. Stated as the set rather than recomputed, so the test's window and the code's window
    /// cannot be the same expression.
    resident: std::collections::BTreeSet<u16>,
}

impl WindowedLand {
    /// Nine flat decodable blocks around [`HOME`], of which the `mid_width = 2*r+1` square
    /// centred on `HOME` is resident.
    fn new(mid_radius: i32) -> Self {
        let mut inner = StaticLandSource::linear();
        let mut resident = std::collections::BTreeSet::new();
        for dx in -1_i32..=1 {
            for dy in -1_i32..=1 {
                let id = block(dx, dy);
                inner.add_flat_block(id, 10);
                if dx.abs() <= mid_radius && dy.abs() <= mid_radius {
                    resident.insert(id.0);
                }
            }
        }
        Self { inner, resident }
    }

    /// Prefetch one interior cell of a resident block into the visible-cell table.
    fn prefetch_interior(&mut self, id: CellId) {
        self.inner.add_cell(EnvCellGeometry {
            id,
            frame: Frame::new(Vec3::new(10.0, 10.0, GROUND), Quat::IDENTITY),
            ..EnvCellGeometry::default()
        });
    }
}

impl LandSource for WindowedLand {
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        // Deliberately unconditional: the dat carries all nine, and that is the defect's premise.
        self.inner.landblock(id)
    }
    fn landblock_resident(&self, id: LandblockId) -> bool {
        self.resident.contains(&id.0)
    }
    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN] {
        self.inner.height_table()
    }
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        self.inner.env_cell(cell)
    }
    fn building_cells(&self, cell: CellId) -> Vec<CellId> {
        self.inner.building_cells(cell)
    }
    fn building(&self, cell: CellId) -> Option<Arc<BuildingGeometry>> {
        self.inner.building(cell)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn block(dx: i32, dy: i32) -> LandblockId {
    LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8)
}

fn land(mid_radius: i32) -> Arc<WindowedLand> {
    Arc::new(WindowedLand::new(mid_radius))
}

// =================================================================================================
// 1. The resolver itself
// =================================================================================================

/// Behaviour: physics.cells.cell-lookup-stops-at-the-resident-window
/// **The rejecting station.** A cell id in a landblock the window does not hold must resolve to
/// `None`, while every cell of every block inside the window resolves exactly as before.
///
/// Red before the fix with `left: 64  right: 0` on the outside count: the resolver asked
/// `LandSource::landblock`, which answers for all nine blocks, and every one of the 64 outside
/// cells came back `Some`.
#[test]
fn an_outdoor_cell_outside_the_resident_window_resolves_to_nothing() {
    // `mid_radius = 0`: `mid_width = 1`, the window is HOME alone, and the eight neighbours are
    // decodable-but-absent. Retail's bounds are what refuse them.
    let land = land(0);
    let resolver = CellResolver::new(&*land);

    let mut inside_resolved = 0_usize;
    let mut outside_resolved = 0_usize;
    let mut outside_decodable = 0_usize;
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            let b = block(dx, dy);
            //  accepts land indices 1..=0x40; that is the whole block.
            for index in 1_u16..=0x40 {
                let id = b.cell(index);
                if dx == 0 && dy == 0 {
                    if resolver.get_visible(id).is_some() {
                        inside_resolved += 1;
                    }
                } else {
                    if resolver.get_visible(id).is_some() {
                        outside_resolved += 1;
                    }
                    if LandSource::landblock(&*land, b).is_some() {
                        outside_decodable += 1;
                    }
                }
            }
        }
    }

    assert_eq!(
        inside_resolved, 64,
        "every land cell of the resident block still resolves"
    );
    assert_eq!(
        outside_decodable, 512,
        "the premise: the source can decode all eight neighbouring blocks, \
         so a resolver that asks only `landblock()` answers for every one of these cells"
    );
    assert_eq!(
        outside_resolved, 0,
        " answers NULL for a block outside the window \
         ({outside_decodable} of these cells are decodable and none of them is resident)"
    );
}

/// Widening the window by one block admits the whole ring.
#[test]
fn widening_the_window_by_one_block_admits_the_whole_ring() {
    let land = land(1);
    let resolver = CellResolver::new(&*land);
    let mut resolved = 0_usize;
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            for index in 1_u16..=0x40 {
                if resolver.get_visible(block(dx, dy).cell(index)).is_some() {
                    resolved += 1;
                }
            }
        }
    }
    assert_eq!(
        resolved,
        9 * 64,
        "mid_width = 3 holds every one of the nine blocks"
    );
}

/// Interior visible-cell lookup only probes the visible-cell hash table,
/// so an interior index the block does
/// not carry is NULL **however resident its landblock is**. This half already behaved — it is
/// here as the other control, and it passes before and after.
#[test]
fn an_interior_index_the_resident_block_does_not_carry_resolves_to_nothing() {
    let mut inner = WindowedLand::new(1);
    let carried = CellId(u32::from(HOME.0) << 16 | 0x0100);
    inner.prefetch_interior(carried);
    let land: Arc<WindowedLand> = Arc::new(inner);
    let resolver = CellResolver::new(&*land);

    assert!(
        LandSource::landblock_resident(&*land, HOME),
        "the block itself is resident, so this is the interior question alone"
    );
    assert!(
        resolver.get_visible(carried).is_some(),
        "the index the block carries resolves"
    );
    assert!(
        resolver
            .get_visible(CellId(u32::from(HOME.0) << 16 | 0x0101))
            .is_none(),
        "an index the block does not carry is NULL from the client's hash probe"
    );
    assert!(
        resolver
            .get_visible(CellId(u32::from(block(1, 0).0) << 16 | 0x0100))
            .is_none(),
        "an interior cell of a non-resident block is NULL too"
    );
}

// =================================================================================================
// 2. The cell array — `add_cell_block`, `add_all_outside_cells`, `find_cell_list`
// =================================================================================================

/// The outside-cell search adds an entry by id for every cell in the rectangle
/// and fills in the pointer, so past the window the array carries
/// `{ id, NULL }` rather than a cell. That null-pointer entry is what
/// the cell-list expansion loop skips and what the collision sweeps step over.
///
/// Red before the fix: every one of the 64 entries carried a resolved cell.
#[test]
fn the_cell_array_carries_null_entries_past_the_window() {
    let land = land(0);
    let resolver = CellResolver::new(&*land);
    let mut arr = CellArray::new();
    // The eight land-cell columns of HOME plus the eight of the block to its east, in **global**
    // land-cell coordinates — which is what `add_cell_block` takes.
    let (x0, y0) = landdefs::gid_to_lcoord(HOME.cell(1)).expect("HOME is in bounds");
    resolver.add_cell_block(&mut arr, x0, y0, x0 + 15, y0 + 7);

    assert_eq!(
        arr.len(),
        128,
        "16 columns x 8 rows of land cells were offered"
    );
    let resolved = arr.cells.iter().filter(|c| c.cell.is_some()).count();
    let by_id_only = arr.cells.iter().filter(|c| c.cell.is_none()).count();
    assert_eq!(resolved, 64, "the resident half of the rectangle resolves");
    assert_eq!(
        by_id_only, 64,
        "the half in the block east of the window is added by id with a NULL cell, \
         as returning NULL makes it"
    );
    for info in &arr.cells {
        let resident = info.cell_id.landblock() == HOME;
        assert_eq!(
            info.cell.is_some(),
            resident,
            "{:#010X} resolved={} resident={resident}",
            info.cell_id.0,
            info.cell.is_some()
        );
    }
}

///  on a sphere **straddling the window's edge**:
///  still offers the neighbour across the
/// landblock boundary — the boundary walk is in global land-cell coordinates and crosses blocks
/// transparently — and `get_visible` is what refuses it.
#[test]
fn a_sweep_straddling_the_window_edge_gets_the_neighbour_by_id_only() {
    let land = land(0);
    let resolver = CellResolver::new(&*land);
    // x = 191.8 is 0.2 m inside HOME's east face (a landblock is 8 * 24 = 192 m across), so a
    // 0.5 m sphere there is in a resident cell and offers
    // the cell across the block boundary: `local.x = 23.8 > lo = 24 - 0.5`.
    let mut id = HOME.cell(1);
    let mut origin = Vec3::new(191.8, 100.0, GROUND);
    assert!(
        landdefs::adjust_to_outside(&mut id, &mut origin),
        "on the world"
    );
    let pos = Position::new(id, Frame::new(origin, Quat::IDENTITY));
    //  runs `adjust_to_outside` on each sphere's **centre**, so
    // the centres it is handed are in the starting cell's landblock space, not object-local.
    let spheres = [Sphere::new(origin, 0.5)];

    let mut arr = CellArray::new();
    let mut hits_interior = false;
    let start = resolver.find_cell_list(&pos, &spheres, &mut arr, true, &mut hits_interior);

    let outside = arr
        .cells
        .iter()
        .filter(|c| c.cell_id.landblock() != HOME)
        .count();
    let outside_resolved = arr
        .cells
        .iter()
        .filter(|c| c.cell_id.landblock() != HOME && c.cell.is_some())
        .count();
    assert_eq!(
        outside,
        1,
        " offered exactly the cell across the east face \
         (the array is {:?})",
        arr.cells
            .iter()
            .map(|c| format!("{:#010X}", c.cell_id.0))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        outside_resolved, 0,
        "the cell east of the window is in the array by id with a NULL pointer"
    );
    assert_eq!(
        start.map(|c| c.id().landblock()),
        Some(HOME),
        "the sphere's own cell is inside the window, so the sweep still has a container"
    );
    assert!(!hits_interior, "nothing interior is involved");
}

// =================================================================================================
// 3. The transition — 's null-cell arm, reached from the resolver
// =================================================================================================

fn at(x: f32, y: f32) -> Position {
    let mut id = HOME.cell(1);
    let mut o = Vec3::new(x, y, GROUND + 0.1);
    assert!(
        landdefs::adjust_to_outside(&mut id, &mut o),
        "({x}, {y}) is off the world"
    );
    Position::new(id, Frame::new(o, Quat::IDENTITY))
}

fn body(w: &mut PhysicsWorld, id: u32) -> PhysHandle {
    w.create(ObjectId(id), player_geometry(), true)
}

/// A placement into a non resident block takes the lost cell arm.
#[test]
fn a_placement_into_a_non_resident_block_takes_the_lost_cell_arm() {
    let land = land(0);
    let mut w = PhysicsWorld::new(land);

    // (a) inside the window: an ordinary placement.
    let near = body(&mut w, 1);
    let near_pos = at(100.0, 100.0);
    assert!(
        w.enter_world(near, &near_pos),
        "entering the world answers 1"
    );
    assert!(
        w.get(near).expect("live").cell.is_some(),
        "the body is in a cell"
    );

    // (b) one block east, which the dat carries and the window does not.
    let far = body(&mut w, 2);
    let far_pos = at(292.0, 100.0);
    assert_eq!(
        far_pos.cell.landblock(),
        block(1, 0),
        "the destination really is in the block east of the window"
    );
    let ok = w.enter_world(far, &far_pos);
    let o = w.get(far).expect("live");
    assert_eq!(
        o.cell, None,
        "past the resident window: the object is in no cell at all"
    );
    assert_eq!(
        o.position.cell, far_pos.cell,
        " keeps the destination, not where it used to stand"
    );
    assert!(
        ok,
        "the arm returns OK_SPE; `enter_world`'s answer is not how a caller learns this"
    );
}

/// A body asked to move out of the window ends in no cell at all.
#[test]
fn a_body_asked_to_move_out_of_the_window_ends_in_no_cell_at_all() {
    let land = land(0);
    let mut w = PhysicsWorld::new(land);
    let h = body(&mut w, 1);
    assert!(
        w.enter_world(h, &at(180.0, 100.0)),
        "placed inside the window"
    );
    assert_eq!(
        w.get(h).expect("live").cell.map(CellId::landblock),
        Some(HOME),
        "standing in the resident block"
    );

    let away = at(292.0, 100.0);
    w.set_position(h, &away);
    let o = w.get(h).expect("live");
    assert_eq!(o.cell, None, " — no cell past the window");
    assert_eq!(
        o.position.cell, away.cell,
        "storing the position keeps the destination"
    );
    assert!(
        !o.transient_state.is_active(),
        "clearing bit 0x80 of the transient state (mask `0xffffff7f`) clears the active flag"
    );

    // The bracket: the same move to a point still inside the window keeps a cell.
    let back = body(&mut w, 2);
    assert!(
        w.enter_world(back, &at(180.0, 100.0)),
        "placed inside the window"
    );
    w.set_position(back, &at(100.0, 100.0));
    assert_eq!(
        w.get(back).expect("live").cell.map(CellId::landblock),
        Some(HOME),
        "an ordinary move inside the window is untouched"
    );
}

use crate::common::physics_fixture::player_geometry;
