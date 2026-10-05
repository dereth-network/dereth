//! The cell world entry assigns an object: a point outside a building keeps the outdoor cell it
//! carries, a point inside a building's shell that carries an outdoor cell is refused, and a
//! point named by its own room cell stays in that room. A grid over a whole landblock checks
//! that placement succeeds somewhere.
//!
//! Fixture: the retail dats' Holtburg landblock and its cells, a `PhysicsWorld` and objects with
//! empty setup geometry; no device, window or network.

#![cfg(windows)]

use std::sync::Arc;

use dereth_physics::source::SetupGeometry;
use dereth_physics::{LandSource, PhysicsWorld};
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};
use dereth_world_data::land_source::DatLandSource;
use dereth_world_data::landblock::load_region;

const HOLTBURG: u16 = 0xA9B4;
/// The **outdoor** landcell a Holtburg house's front doorway falls in.
const OUTDOOR: u32 = 0xA9B4_0029;
const ROT: Quat = Quat::new(0.707_107, 0.0, 0.0, -0.707_107);

fn world() -> (Arc<DatLandSource>, PhysicsWorld) {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let region = load_region(&store).expect("the region decodes");
    let land = Arc::new(DatLandSource::new(store, &region).expect("the height table validates"));
    land.load_block_cells(LandblockId(HOLTBURG));
    // Cell lookup needs the landblock resident as well as its loaded environment cells.
    let _ = land.landblock(LandblockId(HOLTBURG));
    let w = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
    (land, w)
}

/// Create an empty-geometry object and return its assigned cell after enter_world. The boolean
/// result is printed, not returned to the caller; these tests do not inspect a destruction queue.
fn enter(w: &mut PhysicsWorld, n: u32, cell: u32, at: Vec3) -> Option<CellId> {
    let h = w.create(
        ObjectId(0x5000_0000 + n),
        Arc::new(SetupGeometry::default()),
        true,
    );
    let ok = w.enter_world(h, &Position::new(CellId(cell), Frame::new(at, ROT)));
    let got = w.get(h).and_then(|o| o.cell);
    eprintln!(
        "enter_world({cell:#010X}, [{:.2} {:.2} {:.2}]) -> {ok}, cell {}",
        at.x,
        at.y,
        at.z,
        got.map_or_else(|| "NONE".to_owned(), |c| format!("{:#010X}", c.0)),
    );
    got
}

/// Behaviour: physics.enter-world.refuses-a-body-inside-a-building
/// An object carrying an outdoor cell at a point inside a building is refused, not moved into
/// the room there:
///
/// * for a cell index below 0x100, position adjustment normalises outside coordinates and
///   resolves the land cell; descending into a child room belongs to the interior-cell branch,
///   so placement is validated in the outdoor land cell;
/// * building collision then runs with no interior cell hit, so the enclosed volume counts as
///   solid, placement collides, and world entry refuses the object.
///
/// ACE's indoor-cell convention supplies the valid interior-cell control. The two shell-interior
/// points carrying the outdoor cell end with no assigned cell, while bare ground, the doorway and
/// a correctly named room keep their expected cells. Inputs are direct physics calls, not packets.
#[test]
fn enter_world_keeps_outdoor_points_and_refuses_points_inside_a_building() {
    let (_land, mut w) = world();
    // Bare terrain, well clear of the house: the control. Nothing may refuse this.
    let open = enter(&mut w, 1, OUTDOOR, Vec3::new(133.00, 5.15, 94.20));
    // The west face of doorway cell `0xA9B40145`, still outside the shell.
    let doorway = enter(
        &mut w,
        2,
        OUTDOOR,
        Vec3::new(136.289_993, 5.155, 94.082_001),
    );
    // About 0.61 m farther east, inside the doorway shell for this station.
    let inner = enter(&mut w, 3, OUTDOOR, Vec3::new(136.90, 5.155, 94.082_001));
    // Inner-room point 0xA9B40143, also used by gpu/world/unplaceable_object_lifetime.rs.
    let room = enter(&mut w, 4, OUTDOOR, Vec3::new(138.60, 7.00, 94.082_001));
    // The same room point, carrying the cell that contains it.
    let interior = enter(&mut w, 5, 0xA9B4_0143, Vec3::new(138.60, 7.00, 94.082_001));

    // **The premises.** An instrument that refuses everything reports the same "NONE" as one
    // pointed at a building, so the two positions that must succeed are asserted first.
    assert_eq!(
        open,
        Some(CellId(OUTDOOR)),
        "an object on bare terrain outside the house was not placed in the outdoor landcell it \
         carried, so this station is measuring a broken placement rather than a building"
    );
    assert_eq!(
        interior,
        Some(CellId(0xA9B4_0143)),
        "an object created in room 0xA9B40143 at a point inside it must stay in that room"
    );
    assert_eq!(
        doorway,
        Some(CellId(OUTDOOR)),
        "the doorway point is outside the shell's west face and must still place"
    );

    // The finding.
    assert_eq!(
        inner, None,
        "the inside-doorway point carrying an outdoor cell must finish without an assigned cell"
    );
    assert_eq!(
        room, None,
        "the inner-room point carrying outdoor cell 0xA9B40029 must finish without an assigned cell"
    );
}

/// Placement is a transition that can refuse a position. This grid reports how often it succeeds,
/// refuses or changes the assigned cell; it does not assert an acceptable refusal percentage,
/// unchanged cells, rendering or server-packet fidelity.
///
/// Sample 96x96 centres at coordinates 1..191 with step 2. Height is the maximum of the four
/// surrounding terrain vertices plus 0.5, not interpolation at the sample point. Normalize a
/// valid outdoor seed cell from x/y, then create/place/destroy one empty-geometry object. Every
/// normalization must succeed and at least one placement must succeed; other counts are reports.
#[test]
fn a_placement_grid_over_a_real_landblock_places_objects() {
    let (land, mut w) = world();
    let lb = land
        .landblock(LandblockId(HOLTBURG))
        .expect("Holtburg's land half is resident");
    /// The highest of the four terrain vertices around `(x, y)`, so the sample is above the
    /// ground on a slope rather than buried in it. 24 m between vertices, 9 to a side.
    fn ground(lb: &dereth_physics::land::LandblockCollision, x: f32, y: f32) -> f32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = ((x / 24.0) as usize, (y / 24.0) as usize);
        let mut z = f32::MIN;
        for di in 0..2usize {
            for dj in 0..2usize {
                let idx = (i + di).min(8) * 9 + (j + dj).min(8);
                z = z.max(lb.vertices[idx].z);
            }
        }
        z
    }
    let mut n = 0u32;
    let mut placed = 0u32;
    let mut refused: Vec<(f32, f32)> = Vec::new();
    let mut moved = 0u32;
    let mut id = 0u32;
    let mut y = 1.0f32;
    while y < 192.0 {
        let mut x = 1.0f32;
        while x < 192.0 {
            // Outside normalisation refuses cell index 0, and a block<<16 seed falls into the
            // lost-cell success path. Seed index 1, then recompute its cell from x/y.
            let mut cell = CellId((u32::from(HOLTBURG) << 16) | 1);
            let mut origin = Vec3::new(x, y, ground(&lb, x, y) + 0.5);
            assert!(
                dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin),
                "adjust_to_outside refused [{x} {y}], so the grid is not pointed at the block"
            );
            id += 1;
            n += 1;
            let h = w.create(
                ObjectId(0x5100_0000 + id),
                Arc::new(SetupGeometry::default()),
                true,
            );
            if w.enter_world(h, &Position::new(cell, Frame::new(origin, ROT))) {
                placed += 1;
                let got = w.get(h).and_then(|o| o.cell);
                if got != Some(cell) {
                    if moved < 5 {
                        eprintln!(
                            "  [{x:.1} {y:.1}] carried {:#010X} landed {}",
                            cell.0,
                            got.map_or_else(|| "NONE".to_owned(), |c| format!("{:#010X}", c.0))
                        );
                    }
                    moved += 1;
                }
            } else {
                refused.push((x, y));
            }
            w.destroy(h);
            x += 2.0;
        }
        y += 2.0;
    }
    eprintln!(
        "grid over {HOLTBURG:#06X}: {n} points, {placed} placed, {} refused, {moved} landed \
         in a cell other than the one they carried",
        refused.len()
    );
    for (x, y) in refused.iter().take(20) {
        eprintln!("  refused [{x:.1} {y:.1}]");
    }
    assert!(
        placed > 0,
        "every point on a whole landblock was refused, so this instrument is measuring itself"
    );
}
