//! A body standing inside a building registers that building's interior cell, reached from the
//! outdoor land cell it is registered in: `DatLandSource::building_cells` names exactly the
//! interior cells each building's portals open onto, and the bounding-box cross-cell search adds
//! an offered interior cell only when the body's box reaches it.
//!
//! Fixture: Holtburg's twelve shipped `BuildInfo` records, decoded from the retail cell dat and
//! loaded through `DatLandSource`; the oracle decodes the landblock info a second time. Two
//! negatives: a body fifty metres above the same building (every interior cell still offered and
//! refused by the box test), and a body in a land cell with no building.

use crate::common::collision_probe;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::cell::{CellArray, CellResolver};
use dereth_physics::geom::BBox;
use dereth_physics::source::PhysicsPart;
use dereth_physics::{landdefs, LandSource};
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};
use dereth_world_data::land_source::DatLandSource;

use collision_probe::in_the_room;

/// Holtburg, whose shipped landblock-info row contains twelve `BuildInfo` entries used below.
const HOLTBURG: LandblockId = LandblockId(0xA9B4);

/// The retail dats, or **fail**: a skipped test and a passing one print the same green line.
fn store() -> Arc<RetailDatStore> {
    dereth_dat::testing::open_store()
        .map(Arc::new)
        .unwrap_or_else(|| {
            panic!(
                "the retail dats are this file's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        })
}

fn land(store: &Arc<RetailDatStore>) -> Arc<DatLandSource> {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    Arc::new(DatLandSource::new(Arc::clone(store), &region).expect("the retail height table"))
}

/// **The oracle, decoded from the DAT a second time.** For Holtburg's `LandblockInfo`, applying
/// `landdefs::adjust_to_outside` to a copy of each building's frame origin picks its outdoor land
/// cell, and each `BuildingPortal::other_cell_id` is a low-sixteen-bit cell reference within the
/// same landblock.
///
/// This is deliberately *not* read back out of `DatLandSource`: an oracle taken from the subject
/// asserts only that the subject is self-consistent.
fn portals_from_the_records(store: &Arc<RetailDatStore>) -> BTreeMap<u32, BTreeSet<u32>> {
    let lbi_id = dereth_world_data::landblock::lbi_did(HOLTBURG.0);
    let bytes = store
        .read_typed(DbType::Lbi, lbi_id)
        .expect("Holtburg's LBI");
    let lbi = dereth_assets::world::LandblockInfo::decode_payload(lbi_id, &bytes).expect("decodes");
    assert_eq!(
        lbi.buildings.len(),
        12,
        "Holtburg's LBI carries twelve BuildInfo entries"
    );

    let mut out: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for b in &lbi.buildings {
        let mut cell = HOLTBURG.cell(1);
        let mut origin = b.frame.origin;
        assert!(
            landdefs::adjust_to_outside(&mut cell, &mut origin),
            "inside the block"
        );
        let entry = out.entry(cell.0).or_default();
        for p in &b.portals {
            entry.insert((u32::from(HOLTBURG.0) << 16) | u32::from(p.other_cell_id));
        }
    }
    out
}

/// A point a body could stand at inside `cell`, in **landblock** metres, found by sampling the
/// cell's own geometry rather than by naming a coordinate.
///
/// `collision_probe::in_the_room` is the predicate — in the room by `cell_bsp`, not in the masonry
/// by `physics_bsp` — which is the same one the door and camera suites use.
fn a_standable_point(geom: &dereth_physics::source::EnvCellGeometry) -> Option<Vec3> {
    geom.cell_bsp.as_ref()?;
    for &z in &[0.5_f32, 1.0, 1.5] {
        for i in -20_i8..=20 {
            for j in -20_i8..=20 {
                let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                if in_the_room(geom, local) {
                    return Some(dereth_physics::math::localtoglobal(&geom.frame, local));
                }
            }
        }
    }
    None
}

/// A one-part object standing at `origin` (landblock metres of Holtburg) carrying a small
/// axis-aligned box, which selects the bounding-box arm of cross-cell calculation.
///
/// The box is 0.6 m on a side and not a cube: a cube is the one shape whose axis-aligned hull is
/// invariant under the swaps a wrong transform would make, and this file has no reason to hand a
/// wrong transform that particular gift.
fn part_at(origin: Vec3) -> PhysicsPart {
    let mut cell = HOLTBURG.cell(1);
    let mut o = origin;
    assert!(
        landdefs::adjust_to_outside(&mut cell, &mut o),
        "{origin:?} is not in an outdoor cell"
    );
    PhysicsPart {
        pos: Position::new(cell, Frame::new(origin, Quat::IDENTITY)),
        gfxobj_scale: 1.0,
        physics_bsp: None,
        bound_box: Some(BBox::new(
            Vec3::new(-0.3, -0.2, -0.1),
            Vec3::new(0.3, 0.2, 0.9),
        )),
        drawing_sphere: None,
        first_degrade_mode: None,
    }
}

fn shadow_set(source: &Arc<DatLandSource>, parts: &[PhysicsPart]) -> Vec<(u32, bool)> {
    let r = CellResolver::new(source.as_ref());
    let mut arr = CellArray::default();
    r.find_bbox_cell_list(&parts[0].pos, parts, &mut arr);
    let mut v: Vec<(u32, bool)> = arr
        .cells
        .iter()
        .map(|c| (c.cell_id.0, c.cell.is_some()))
        .collect();
    v.sort_unstable();
    v
}

/// **(1) The producer, against the records.**
///
/// `DatLandSource::building_cells` is a map filled while a landblock's cells are loaded; this
/// checks it against Holtburg's own `LandblockInfo`, decoded here independently. Both directions
/// are checked — no land cell may claim an interior cell the records do not give it, and none may
/// omit one — because a producer that returns everything and a producer that returns nothing fail
/// only one of the two.
#[test]
fn dat_land_source_names_exactly_the_interior_cells_holtburgs_buildings_open_onto() {
    let s = store();
    let src = land(&s);
    src.load_block_cells(HOLTBURG);

    let want = portals_from_the_records(&s);
    assert!(
        !want.is_empty(),
        "the records must name at least one building cell"
    );
    let total: usize = want.values().map(BTreeSet::len).sum();
    eprintln!(
        "Holtburg's twelve buildings stand in {} land cells and open onto {total} interior \
         cells",
        want.len()
    );
    assert!(
        total >= 12,
        "twelve buildings with at least one portal each: {total}"
    );

    for (cell, ids) in &want {
        let got: BTreeSet<u32> = src
            .building_cells(CellId(*cell))
            .into_iter()
            .map(|c| c.0)
            .collect();
        assert_eq!(
            &got, ids,
            "land cell {cell:#010X}: the decoded building-cell list disagrees with the \
             land-block metadata's portal list"
        );
    }

    // And the other direction: a land cell of this block with no building must answer with an
    // empty list, or "agrees on the cells that have buildings" is satisfied by a map that answers
    // the same thing everywhere.
    let empty: Vec<u32> = (1..=64_u16)
        .map(|i| (u32::from(HOLTBURG.0) << 16) | u32::from(i))
        .filter(|c| !want.contains_key(c))
        .collect();
    assert!(
        !empty.is_empty(),
        "some cell of Holtburg has no building, or this asserts nothing"
    );
    for c in &empty {
        assert!(
            src.building_cells(CellId(*c)).is_empty(),
            "land cell {c:#010X} has no building in the LBI but was given building cells"
        );
    }
}

/// Behaviour: world.building.a-body-in-a-building-registers-its-interior-cell
/// **(2) The arm, as a cell set, and (3) its negative — both over the shipped world.**
///
/// A part standing at a point inside one of Holtburg's building interiors, registered in the land
/// cell **outside** it, must have that interior cell in its shadow set. The point is not named:
/// it is sampled from the interior cell's own `cell_bsp` and `physics_bsp`, so the fixture cannot
/// be right about a room it is wrong about.
///
/// The entry must also be **resolved** (`CellInfo::cell` is present). The building-transit arm
/// adds a cell only after its collision BSP says the box reaches it, and only when the cell
/// resolves; an unresolved entry would mean another arm of `find_bbox_cell_list` merely named the
/// ID.
#[test]
fn a_body_standing_in_a_holtburg_building_registers_that_buildings_interior_cell() {
    let s = store();
    let src = land(&s);
    src.load_block_cells(HOLTBURG);

    let want = portals_from_the_records(&s);
    // The first (land cell, interior cell) pair whose interior cell is resident and has a point a
    // body could stand at. Named by search rather than by constant so that a re-ordering of the
    // LBI cannot silently turn this into a different assertion.
    let mut station = None;
    for (land_cell, ids) in &want {
        for id in ids {
            let Some(geom) = src.env_cell(CellId(*id)) else {
                continue;
            };
            if let Some(p) = a_standable_point(&geom) {
                station = Some((*land_cell, *id, p));
                break;
            }
        }
        if station.is_some() {
            break;
        }
    }
    let (land_cell, interior, point) = station.expect("a Holtburg interior with a standable point");
    eprintln!(
        "standing at {point:?} inside {interior:#010X}, whose building is in land cell \
         {land_cell:#010X}"
    );

    let part = part_at(point);
    assert_eq!(
        part.pos.cell.0, land_cell,
        "the probe must be registered in the land cell that owns the building, or the transit arm \
         is not the thing being tested"
    );

    let set = shadow_set(&src, std::slice::from_ref(&part));
    eprintln!("shadow set {set:#010X?}");
    assert!(
        set.contains(&(interior, true)),
        "the interior cell {interior:#010X} must be in the shadow set and resolved: {set:#010X?}"
    );

    // (3a) The NEAR negative: the same land cell, the same building, the same offered interior
    // cells -- and the part fifty metres above the roof, where the box reaches none of them. This
    // is what makes the assertion above a statement about `box_intersects_cell` rather than about
    // `building_cells`: dropping the box test would pass the far negative below, because a land
    // cell with no building never reaches the test at all.
    let airborne = part_at(Vec3::new(point.x, point.y, point.z + 50.0));
    assert_eq!(
        airborne.pos.cell.0, land_cell,
        "the near negative must stay in the building's own land cell"
    );
    assert!(
        !src.building_cells(airborne.pos.cell).is_empty(),
        "and the building cells must still be offered to it, or it tests nothing"
    );
    let above = shadow_set(&src, std::slice::from_ref(&airborne));
    eprintln!("near-negative shadow set {above:#010X?}");
    assert!(
        above.iter().all(|(id, _)| id & 0xFFFF <= 0x40),
        "fifty metres above the roof the box reaches no interior cell, though every one of them \
         was offered: {above:#010X?}"
    );

    // (3b) The FAR negative: a land cell with no building at all. Its shadow set may contain land
    // cells but must contain no interior one -- interior cell ids of this block have an index
    // above 0x0100, land cells 1..=0x40.
    let far = Vec3::new(point.x, (point.y + 100.0).min(180.0), point.z);
    let elsewhere = part_at(far);
    assert!(
        src.building_cells(elsewhere.pos.cell).is_empty(),
        "the negative station must be a land cell with no building; {:#010X} has one",
        elsewhere.pos.cell.0
    );
    let away = shadow_set(&src, std::slice::from_ref(&elsewhere));
    eprintln!("negative shadow set {away:#010X?}");
    assert!(
        away.iter().all(|(id, _)| id & 0xFFFF <= 0x40),
        "a body away from every building registers only land cells: {away:#010X?}"
    );
    assert!(
        !away.iter().any(|(id, _)| *id == interior),
        "and certainly not the interior cell the positive arm found"
    );
}
