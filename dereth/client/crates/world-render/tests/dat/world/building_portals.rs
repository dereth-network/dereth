//! Drawing-BSP portal indices are in range; every building portal leads into its own block and its
//! stab list; the walk reaches every opening and the gate admits one side; the sidedness epsilon is
//! inclusive; a shell runs its openings once for depth and once for the interior.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::world::LandblockInfo;
use dereth_assets::{Decode, EnvCell, GfxObj};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, Vec3};
use dereth_world_render::cells::portal_view::{
    build_draw_portals_only, draw_portal, BuildingPortal, PortalMode, PortalPoly, Sidedness,
};
use dereth_world_render::objects::buildings::portal_pass;

const HOLTBURG: u16 = 0xA9B4;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn lbi(store: &RetailDatStore, block: u16) -> Option<LandblockInfo> {
    let id = DataId((u32::from(block) << 16) | 0xFFFE);
    let bytes = store.read_typed(DbType::Lbi, id).ok()?;
    LandblockInfo::decode_payload(id, &bytes).ok()
}

fn gfxobj(store: &RetailDatStore, id: DataId) -> Option<GfxObj> {
    let bytes = store.read_typed(DbType::GfxObj, id).ok()?;
    GfxObj::decode_payload(id, &bytes).ok()
}

/// The building's portal-side view of one interior cell.
fn env_cell(store: &RetailDatStore, id: DataId) -> Option<EnvCell> {
    let bytes = store.read_typed(DbType::Cell, id).ok()?;
    EnvCell::decode_payload(id, &bytes).ok()
}

/// The client's own widening, transcribed once so every assertion below uses the same one:
/// `portal_side = ((~flags) >> 1) & 1` and `other_cell_id` is the low 16 bits.
fn bld_portals(block: u16, b: &dereth_assets::world::BuildInfo) -> Vec<BuildingPortal> {
    let base = u32::from(block) << 16;
    b.portals
        .iter()
        .map(|p| BuildingPortal {
            portal_side: u8::from((!p.flags >> 1) & 1 != 0),
            other_cell_id: base | u32::from(p.other_cell_id),
            other_portal_id: i32::from(p.other_portal_id),
            stab_list: Vec::new(),
        })
        .collect()
}

/// Every shell BSP portal entry, whatever the viewpoint: `build_draw_portals_only` only ever
/// yields a subset of these, so this is the ceiling the walk is compared against.
fn all_portal_polys(g: &GfxObj) -> Vec<PortalPoly> {
    let mut out = Vec::new();
    if let Some(t) = &g.drawing_bsp {
        for n in &t.nodes {
            for &(poly, portal) in &n.in_portals {
                if let (Ok(polygon), Ok(portal_index)) =
                    (usize::try_from(poly), usize::try_from(portal))
                {
                    out.push(PortalPoly {
                        polygon,
                        portal_index,
                    });
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// 1. The join.
// ---------------------------------------------------------------------------------------------

/// **Every drawing-BSP portal entry indexes the building portal and graphics-polygon arrays.**
///
/// The portal draw opens by indexing `outdoor_portal_list` with the portal entry's `portal_index`
/// and takes the entry's `portal` as the polygon. Neither is bounds-checked in the client, so if either
/// index were an *id* rather than an index the client would read past the array. That makes the
/// index interpretation directly testable.
///
/// Sampled over a stripe of the world rather than one block, because a single town is not evidence
/// that a format claim holds.
#[test]
fn every_drawing_bsp_portal_index_is_in_range_of_its_buildings_portal_list() {
    let store = store();
    let (mut blocks, mut buildings, mut portals, mut with_bsp) = (0u32, 0u32, 0u32, 0u32);
    for bx in 0u16..=0xFE {
        for by in (0u16..=0xFE).step_by(16) {
            let block = (bx << 8) | by;
            let Some(info) = lbi(&store, block) else {
                continue;
            };
            if info.buildings.is_empty() {
                continue;
            }
            blocks += 1;
            for b in &info.buildings {
                buildings += 1;
                let Some(g) = gfxobj(&store, b.id) else {
                    continue;
                };
                let polys = all_portal_polys(&g);
                if polys.is_empty() {
                    continue;
                }
                with_bsp += 1;
                for p in &polys {
                    portals += 1;
                    assert!(
                        p.portal_index < b.portals.len(),
                        "block {block:#06X} building {:#010X}: portal_index {} of {}",
                        b.id.0,
                        p.portal_index,
                        b.portals.len()
                    );
                    assert!(
                        p.polygon < g.polygons.len(),
                        "block {block:#06X} building {:#010X}: polygon {} of {}",
                        b.id.0,
                        p.polygon,
                        g.polygons.len()
                    );
                }
            }
        }
    }
    assert!(
        blocks > 0,
        "only {blocks} blocks with buildings in the sample"
    );
    assert!(portals > 0, "only {portals} building portals in the sample");
    eprintln!(
        "{blocks} blocks, {buildings} buildings ({with_bsp} with portal nodes), {portals} portal \
         polygons"
    );
}

/// Behaviour: world.portals.every-building-opening-leads-into-a-cell-of-its-own-block
/// **A building portal names a cell of its own block, and a real portal of that cell.**
///
/// `other_cell_id` is stored as the low 16 bits and the client ORs in the landblock base. Leaving
/// the base off gives a self-consistent-looking `0x0116` that addresses nothing, so this check
/// covers the building half of that failure mode too.
///
/// `other_portal_id` is then passed as the entry-portal index into that cell's view. The index and
/// its side polarity determine which adjoining interior becomes visible.
#[test]
fn every_building_portal_leads_into_a_cell_of_its_own_block() {
    let store = store();
    let Some(info) = lbi(&store, HOLTBURG) else {
        panic!("Holtburg has a land-block metadata record");
    };
    assert!(!info.buildings.is_empty(), "the fixture contains buildings");

    // The block's cells, by full id.
    let mut cells: BTreeMap<u32, EnvCell> = BTreeMap::new();
    for i in 0..info.num_cells {
        let id = DataId((u32::from(HOLTBURG) << 16) | (0x0100 + i));
        if let Some(c) = env_cell(&store, id) {
            cells.insert(id.0, c);
        }
    }
    assert!(info.num_cells > 0, "interior cells are exercised");
    assert_eq!(
        cells.len(),
        info.num_cells as usize,
        "every declared interior cell decodes"
    );

    let mut checked = 0usize;
    let mut reached: BTreeSet<u32> = BTreeSet::new();
    for b in &info.buildings {
        for p in bld_portals(HOLTBURG, b) {
            checked += 1;
            let cell = cells.get(&p.other_cell_id).unwrap_or_else(|| {
                panic!(
                    "building {:#010X} opens onto {:#010X}, not a cell of this block",
                    b.id.0, p.other_cell_id
                )
            });
            reached.insert(p.other_cell_id);
            let idx = usize::try_from(p.other_portal_id).expect("a non-negative portal index");
            assert!(
                idx < cell.portals.len(),
                "cell {:#010X} has {} portals; the building names index {idx}",
                p.other_cell_id,
                cell.portals.len()
            );
            // The cell's own matching portal is the one that leads **outside**: this opening is a
            // window or a door in the shell, and resolves the leads-outside flag
            // to 0xFFFFFFFF.
            assert_eq!(
                cell.portals[idx].other_cell_id, 0xFFFF_FFFF,
                "cell {:#010X} portal {idx} is the building's opening, so it must lead outdoors",
                p.other_cell_id
            );
        }
    }
    assert!(checked > 0, "building openings are exercised");
    assert_eq!(
        checked,
        info.buildings
            .iter()
            .map(|building| building.portals.len())
            .sum::<usize>()
    );
    eprintln!(
        "{checked} openings reaching {} distinct cells",
        reached.len()
    );
}

/// **Every building portal's target cell is in that portal's own `stab_list`.**
///
/// Portal construction pushes a view onto every cell in the portal's `stab_list` and *then* runs
/// `copy_view(other's top portal_view, …)`, and only that stab-list push ever pushes a view. A
/// target outside its own stab list would therefore have `num_view == 0` and the client would index
/// `portal_view.data[-1]`. It never does — which is what makes the stab list usable as the
/// traversal's bound rather than merely as a hint.
///
/// The same list is also what stops one house's window showing the next house's rooms:
/// the hand-off to neighbouring portals refuses a neighbour with no live `portal_view`.
#[test]
fn a_building_portals_target_cell_is_always_in_its_own_stab_list() {
    let store = store();
    let (mut checked, mut widest) = (0u32, 0usize);
    for bx in 0u16..=0xFE {
        for by in (0u16..=0xFE).step_by(16) {
            let block = (bx << 8) | by;
            let Some(info) = lbi(&store, block) else {
                continue;
            };
            for b in &info.buildings {
                for p in &b.portals {
                    checked += 1;
                    widest = widest.max(p.stab_list.len());
                    assert!(
                        p.stab_list.contains(&p.other_cell_id),
                        "block {block:#06X} building {:#010X}: opening onto {:#06X} whose stab \
                         list is {:?}",
                        b.id.0,
                        p.other_cell_id,
                        p.stab_list
                    );
                }
            }
        }
    }
    assert!(checked > 0, "only {checked} openings in the sample");
    eprintln!("{checked} openings; the widest stab list holds {widest} cells");
}

// ---------------------------------------------------------------------------------------------
// 2. The traversal and the gate.
// ---------------------------------------------------------------------------------------------

/// **The BSP walk reaches every opening, and the gate opens each from exactly one side.**
///
/// The BSP walk emits a node's portals only in its POSITIVE and
/// NEGATIVE arms, so a single viewpoint sees a subset; over a ring of viewpoints around the
/// building every opening must be reachable, or the shell carries a portal the renderer could never
/// draw through.
///
/// The sidedness gate is an exclusive-or: for a fixed opening, exactly one of
/// the two sides of its plane admits a viewer. Testing both signs of the plane distance is the
/// whole content of "you see into a window from outside it and not from inside it".
#[test]
fn the_walk_reaches_every_opening_and_the_gate_admits_exactly_one_side() {
    let store = store();
    let Some(info) = lbi(&store, HOLTBURG) else {
        panic!("Holtburg has a land-block metadata record")
    };

    for b in &info.buildings {
        let Some(g) = gfxobj(&store, b.id) else {
            continue;
        };
        let Some(bsp) = g.drawing_bsp.as_ref() else {
            continue;
        };
        let expected: BTreeSet<usize> = all_portal_polys(&g)
            .into_iter()
            .map(|p| p.portal_index)
            .collect();
        if expected.is_empty() {
            continue;
        }
        let portals = bld_portals(HOLTBURG, b);

        // A ring of viewpoints at building scale, above and below the openings.
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for i in 0..16 {
            #[allow(clippy::cast_precision_loss)] // 0..16
            let a = i as f32 * std::f32::consts::TAU / 16.0;
            for z in [-4.0f32, 1.5, 8.0] {
                let vp = Vec3::new(math::cosf(a) * 20.0, math::sinf(a) * 20.0, z);
                for p in build_draw_portals_only(bsp, vp) {
                    seen.insert(p.portal_index);
                }
            }
        }
        assert_eq!(
            seen,
            expected,
            "building {:#010X}: the walk never reaches {:?}",
            b.id.0,
            expected.difference(&seen).collect::<Vec<_>>()
        );

        // The gate, per opening.
        for p in all_portal_polys(&g) {
            let open = |d: f32| {
                draw_portal(p, &portals, d, PortalMode::BuildView, &|_| true, &|_| true)
                    .interior
                    .is_some()
            };
            assert!(
                open(1.0) ^ open(-1.0),
                "building {:#010X} opening {}: the sidedness gate is not exclusive",
                b.id.0,
                p.portal_index
            );
            assert!(
                !open(0.0),
                "building {:#010X} opening {}: a viewer in the portal plane sees through it",
                b.id.0,
                p.portal_index
            );
            // The side that opens is the one `portal_side` names, and the cell it opens onto is the
            // one the building-portal record names — not the polygon's own index.
            let side = portals[p.portal_index].portal_side;
            let d = if side == 0 { 1.0 } else { -1.0 };
            let got = draw_portal(p, &portals, d, PortalMode::BuildView, &|_| true, &|_| true);
            assert_eq!(
                got.interior.map(|i| i.cell.0),
                Some(portals[p.portal_index].other_cell_id)
            );
        }
    }
}

/// **`Sidedness` is the client's own branch, epsilon included.**
///
/// `0.0002` is every culling epsilon in the renderer, and the boundary is *inclusive* on both
/// sides: `d <= 0.0002` is not
/// POSITIVE and `-0.0002 <= d` is not NEGATIVE.
#[test]
fn the_sidedness_epsilon_is_inclusive_on_both_sides() {
    assert_eq!(
        dereth_world_render::cells::portal_view::sidedness(0.000_2),
        Sidedness::InPlane
    );
    assert_eq!(
        dereth_world_render::cells::portal_view::sidedness(-0.000_2),
        Sidedness::InPlane
    );
    assert_eq!(
        dereth_world_render::cells::portal_view::sidedness(0.000_200_1),
        Sidedness::Positive
    );
    assert_eq!(
        dereth_world_render::cells::portal_view::sidedness(-0.000_200_1),
        Sidedness::Negative
    );
}

/// **The two passes are `1` then `2`, over the same order.**
///
/// The building draw runs `build_draw_portals_only(bsp, 1)` and then
/// `(bsp, 2)`; both walks see the same viewpoint, so they visit the same portals in the same order.
/// Asserting it against a real shell is what makes [`portal_pass`]'s single ordering claim a
/// statement about the retail data rather than about a `Vec`.
#[test]
fn a_real_shell_runs_its_openings_once_for_depth_and_once_for_the_interior() {
    let store = store();
    let Some(info) = lbi(&store, HOLTBURG) else {
        panic!("Holtburg has a land-block metadata record")
    };
    // The largest shell in the block, so the walk has real depth to it.
    let b = info
        .buildings
        .iter()
        .max_by_key(|b| b.portals.len())
        .expect("Holtburg has buildings");
    let g = gfxobj(&store, b.id).expect("the shell decodes");
    let bsp = g.drawing_bsp.as_ref().expect("the shell has a drawing BSP");
    let order = build_draw_portals_only(bsp, Vec3::new(0.0, -20.0, 2.0));
    assert!(!order.is_empty(), "the shell yielded no openings at all");
    let pass = portal_pass(&order);
    assert_eq!(pass.len(), order.len() * 2);
    let (stamps, views): (Vec<_>, Vec<_>) =
        pass.iter().partition(|(m, _)| *m == PortalMode::StampDepth);
    assert_eq!(
        stamps.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
        views.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
        "the two passes walk the same order"
    );
    let last_stamp = pass
        .iter()
        .rposition(|(m, _)| *m == PortalMode::StampDepth)
        .expect("a stamp");
    let first_view = pass
        .iter()
        .position(|(m, _)| *m == PortalMode::BuildView)
        .expect("a view");
    assert!(
        last_stamp < first_view,
        "every opening is stamped before any interior is drawn"
    );
}
