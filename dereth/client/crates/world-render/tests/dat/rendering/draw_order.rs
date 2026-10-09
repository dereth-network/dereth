//! Batches are far-to-near between and within blocks; sky passes land in their slots; a dungeon
//! takes the indoor path with Z clear; the alpha list keeps cell order; a dungeon web goes on the
//! clip list, and a sealed dungeon room flushes the lists only after every cell's objects.
//! Fixture: the shipped retail DAT records and recorded inputs.

// Index arithmetic over a 5x5 block window and an 8x8 cell grid, bounded by the loops themselves.
#![allow(clippy::cast_possible_truncation)]

use std::collections::BTreeMap;

use dereth_assets::{decode_any, DecodedAsset};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{CellId, DataId, Frame, MeshHandle, Vec3};
use dereth_terrain::testing::Recorder;
use dereth_world_render::cells::portal_view::{
    construct_view, indoor_steps, CellPortal, IndoorStep, TraversalCell, OUTDOORS,
};
use dereth_world_render::degrade_loop::DegradeGovernor;
use dereth_world_render::frame::{block_steps, outdoor_steps, BlockStep, OutdoorStep};
use dereth_world_render::land::emit::draw_land_cell;
use dereth_world_render::objects::alpha::{AlphaEntry, AlphaList, AlphaLists};
use {
    dereth_terrain::land::mesh::generate_landblock_with_table,
    dereth_terrain::land::mesh::height_table, dereth_terrain::land::mesh::Direction,
};
use {
    dereth_terrain::land::order::block_draw_order, dereth_terrain::land::order::block_orient,
    dereth_terrain::land::order::cell_draw_order, dereth_terrain::land::order::side_cell_count,
};

const REGION_ID: DataId = DataId(0x1300_0000);

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

/// Behaviour: rendering.draw-order.blocks-and-cells-draw-far-to-near-with-the-sky-around-them
#[test]
fn the_recorded_batch_sequence_is_far_to_near_between_and_within_blocks() {
    let s = store();
    let b = s
        .read_typed(DbType::Region, REGION_ID)
        .expect("the region record");
    let DecodedAsset::Region(region) = decode_any(DbType::Region, REGION_ID, &b).expect("decodes")
    else {
        panic!("0x13000000 is not a region")
    };
    let table = height_table(&region);
    let _governor = DegradeGovernor::pinned(dereth_terrain::consts::PINNED_DEG_MUL);

    // A 5x5 window (mid_radius 2) centred on a real, land-bearing block.
    let mid_radius = 2i32;
    let mid_width = (2 * mid_radius + 1) as u32;
    let (vbx, vby) = (0xA9i32, 0xB4i32);

    // Build every block of the window at the LOD its ring implies.
    let mut meshes = BTreeMap::new();
    for xi in 0..mid_width as i32 {
        for yi in 0..mid_width as i32 {
            let (bx, by) = (vbx + xi - mid_radius, vby + yi - mid_radius);
            // LINT-OK: both bytes of a landblock id.
            let id = DataId(((bx as u32) << 24) | ((by as u32) << 16) | 0xFFFF);
            let Ok(bytes) = s.read_typed(DbType::LandBlock, id) else {
                continue;
            };
            let DecodedAsset::Landblock(lb) =
                decode_any(DbType::LandBlock, id, &bytes).expect("decodes")
            else {
                continue;
            };
            let (lod, dir) = block_orient(xi - mid_radius, yi - mid_radius);
            let mut m = generate_landblock_with_table(&lb, &region, &table, bx, by, lod, dir);
            dereth_terrain::land::lighting::bake_lighting(
                &mut m,
                &dereth_terrain::land::lighting::LandscapeLighting::default(),
            );
            // LINT-OK: index arithmetic, bounded by mid_width.
            meshes.insert((mid_width as i32 * xi + yi) as u32, (m, xi, yi));
        }
    }
    assert_eq!(
        meshes.len(),
        (mid_width * mid_width) as usize,
        "the whole window loaded"
    );

    // The frame.
    let blocks = block_draw_order(mid_width);
    let steps = outdoor_steps(&blocks, &|b| meshes.contains_key(&b), true);
    let mut r = Recorder::new();
    // A camera high above the middle of the viewer's block, so every cell faces it and nothing is
    // back-face culled away -- the ordering, not the culling, is what this test measures.
    let viewer = Vec3::new(96.0, 96.0, 400.0);
    // (block ring, window index, cell distance-from-closest) per recorded batch.
    let mut keys: Vec<(i32, u32, i32)> = Vec::new();

    for step in &steps {
        let OutdoorStep::Block(bi) = step else {
            continue;
        };
        let (mesh, xi, yi) = &meshes[bi];
        let ring = (xi - mid_radius).abs().max((yi - mid_radius).abs());
        let n = mesh.side_cell_count;
        let (_, dir) = block_orient(xi - mid_radius, yi - mid_radius);
        let cell_dir = dereth_terrain::land::order::get_dir(xi - mid_radius, yi - mid_radius);
        // `cell_draw_order` takes the viewer's cell within its own block, already divided by
        // `8 / side_cell_count`, so a reduced-detail block indexes a coarser grid.
        let step = 8 / n;
        let viewer_cell = (4 / step, 4 / step);
        let closest = dereth_terrain::land::order::closest_cell(n, cell_dir, viewer_cell);
        let array = cell_draw_order(n, cell_dir, viewer_cell);
        assert_eq!(
            side_cell_count(block_orient(xi - mid_radius, yi - mid_radius).0),
            n
        );
        let _ = dir;
        for bs in block_steps(&array, &|_| true, &|_| false) {
            let BlockStep::LandCell(c) = bs else { continue };
            let (i, j) = (
                usize::from(c) / usize::from(n),
                usize::from(c) % usize::from(n),
            );
            // The viewer's block sits at the window centre; every block is drawn in its own
            // block-local space, so the viewer converts to that space for the back-face test.
            let bx = f32::from((xi - mid_radius) as i16) * 192.0;
            let by = f32::from((yi - mid_radius) as i16) * 192.0;
            let local = Vec3::new(viewer.x - bx, viewer.y - by, viewer.z);
            if draw_land_cell(mesh, i, j, local, Frame::default(), None, &mut r).is_some() {
                let d = (i as i32 - i32::from(closest.0))
                    .abs()
                    .max((j as i32 - i32::from(closest.1)).abs());
                keys.push((ring, *bi, d));
            }
        }
    }

    assert!(!r.draws.is_empty(), "{} batches recorded", r.draws.len());
    // Between blocks: the ring must never increase.
    assert!(
        keys.windows(2).all(|w| w[0].0 >= w[1].0),
        "block rings must be non-increasing: the landscape draws far to near"
    );
    // Within one block: the cell distance from that block's closest cell must never increase.
    assert!(
        keys.windows(2)
            .all(|w| w[0].1 != w[1].1 || w[0].2 >= w[1].2),
        "cell distance must be non-increasing within a block"
    );
    // Each block's batches are contiguous: a block is finished before the next one starts.
    let mut seen: Vec<u32> = Vec::new();
    for w in &keys {
        if seen.last() != Some(&w.1) {
            assert!(
                !seen.contains(&w.1),
                "block {} was revisited after another block",
                w.1
            );
            seen.push(w.1);
        }
    }
    assert_eq!(
        seen.len(),
        (mid_width * mid_width) as usize,
        "every block drew something"
    );
    assert_eq!(
        keys[0].0, mid_radius,
        "the first batch is on the outermost ring"
    );
    assert_eq!(
        keys[keys.len() - 1].0,
        0,
        "the last batch is in the viewer's own block"
    );
    // Batches are submitted in the order they were produced, and never reordered.
    let order: Vec<MeshHandle> = r.draw_order();
    let mut sorted = order.clone();
    sorted.sort_unstable_by_key(|m| m.0);
    assert_eq!(
        order, sorted,
        "the recorder must preserve submission order exactly"
    );
}

/// Behaviour: rendering.draw-order.blocks-and-cells-draw-far-to-near-with-the-sky-around-them
/// Oracle: `10-frame-composition.md` — "clear → sky pass 0 → landblocks far→near → sky pass 1 →
/// alpha list". The two sky passes must land in those exact slots, because pass 0 is painted under
/// everything and pass 1 (weather) over it.
#[test]
fn the_sky_passes_land_in_their_contract_slots() {
    let blocks = block_draw_order(5);
    let s = outdoor_steps(&blocks, &|_| true, true);
    let sky0 = s
        .iter()
        .position(|x| *x == OutdoorStep::SkyPass0)
        .expect("sky pass 0");
    let sky1 = s
        .iter()
        .position(|x| *x == OutdoorStep::SkyPass1)
        .expect("sky pass 1");
    let first_block = s
        .iter()
        .position(|x| matches!(x, OutdoorStep::Block(_)))
        .expect("blocks");
    let last_block = s
        .iter()
        .rposition(|x| matches!(x, OutdoorStep::Block(_)))
        .expect("blocks");
    let alpha = s
        .iter()
        .position(|x| *x == OutdoorStep::FlushAlphaList)
        .expect("the alpha flush");
    assert!(s[0] == OutdoorStep::Clear);
    assert!(sky0 < first_block, "pass 0 is painted before any terrain");
    assert!(last_block < sky1, "pass 1 is painted after all of it");
    assert!(sky1 < alpha, "and the alpha list is flushed last of all");
}

/// Oracle: indoor rendering, driven by a **real dungeon** from the retail cell dat: the
/// sequence is outdoors-through-portals → Z clear → portal depth stamps → env cells far→near →
/// alpha list, and the Z clear plus the depth stamp are asserted as *present*.
///
/// "Without it the outdoors seen through a doorway z-fights with the interior."
#[test]
fn a_retail_dungeon_takes_the_indoor_path_with_its_z_clear() {
    let s = store();
    // Find a landblock whose env cells include one with a portal to the outdoors, which is what
    // makes the Z clear necessary.
    let mut cells: BTreeMap<u32, TraversalCell> = BTreeMap::new();
    let mut entry: Option<CellId> = None;
    'outer: for lb in [
        0xA9B4u32, 0x0007, 0x1234, 0x7D64, 0x8000, 0xC6A9, 0xDA55, 0xE74E,
    ] {
        cells.clear();
        for idx in 0x0100u32..0x0200 {
            let id = DataId((lb << 16) | idx);
            let Ok(bytes) = s.read_typed(DbType::Cell, id) else {
                continue;
            };
            let DecodedAsset::EnvCell(ec) = decode_any(DbType::Cell, id, &bytes).expect("decodes")
            else {
                continue;
            };
            let portals: Vec<CellPortal> = ec
                .portals
                .iter()
                .map(|p| CellPortal {
                    // flags bit 1 is portal_side; the viewer inside the cell is on that side, so
                    // a positive distance with portal_side 0 makes the portal an exit.
                    portal_side: u8::from(p.flags & 2 != 0),
                    other_cell_id: if p.other_cell_id == 0xFFFF_FFFF {
                        OUTDOORS
                    } else {
                        p.other_cell_id
                    },
                    other_portal_id: i32::from(p.other_portal_id),
                    exact_match: p.flags & 1 != 0,
                    vertex_dist_sq: [f32::from(p.polygon_id) + 1.0; 4],
                    // Stand the viewer on the cell's own side of every portal plane.
                    viewpoint_side_distance: if p.flags & 2 != 0 { -1.0 } else { 1.0 },
                })
                .collect();
            cells.insert(
                id.0,
                TraversalCell {
                    id: CellId(id.0),
                    portals,
                },
            );
        }
        for (k, c) in &cells {
            if c.portals.iter().any(|p| p.other_cell_id == OUTDOORS) && c.portals.len() > 1 {
                entry = Some(CellId(*k));
                break 'outer;
            }
        }
    }
    let Some(entry) = entry else {
        eprintln!("skipping: no retail env cell with both an outdoor portal and an interior one");
        return;
    };

    let view = construct_view(&cells, entry, &|_, _| true);
    assert!(
        !view.cell_draw_list.is_empty(),
        "the traversal visited at least the entry cell"
    );
    assert!(
        view.outside_view_count > 0,
        "the entry cell opens outdoors, so the outdoor pass must run"
    );
    let steps = indoor_steps(&view);
    assert_eq!(
        steps,
        vec![
            IndoorStep::OutdoorsThroughPortals,
            IndoorStep::FlushBeforeClear,
            IndoorStep::ZClear,
            IndoorStep::PortalDepthStamps,
            IndoorStep::IndoorLighting,
            IndoorStep::EnvCellMeshes,
            IndoorStep::Objects,
            IndoorStep::FlushAlphaList,
        ],
        "indoor draw sequence, entry cell {entry}"
    );
    // The traversal reached more than the entry cell, and the cell draw draws them far to near.
    let far_to_near = view.draw_order();
    assert_eq!(far_to_near.len(), view.cell_draw_list.len());
    assert_eq!(
        *far_to_near.last().expect("non-empty"),
        view.cell_draw_list[0],
        "the nearest cell -- the one the viewer stands in -- is drawn last"
    );
}

#[test]
fn the_alpha_list_inherits_the_cell_order_and_is_not_re_sorted() {
    let array = cell_draw_order(8, Direction::InViewerBlock, (3, 4));
    let mut lists = AlphaLists::new();
    let mut expected = Vec::new();
    for step in block_steps(&array, &|_| true, &|_| false) {
        let BlockStep::SortCell(c) = step else {
            continue;
        };
        // One translucent subset per cell, in the cell's draw order.
        lists.push(
            AlphaList::Blend,
            AlphaEntry {
                mesh: MeshHandle(u32::from(c)),
                surface_num: 0,
                texture: None,
                first_of_kind: true,
                world_matrix: Frame::default(),
                multipass: false,
                range: 0..3,
            },
        );
        expected.push(MeshHandle(u32::from(c)));
    }
    let mut r = Recorder::new();
    assert_eq!(lists.flush(0.0, &mut r), expected.len());
    assert_eq!(
        r.draw_order(),
        expected,
        "the alpha list flushes in insertion order"
    );
    // And that order is the far-to-near cell order, not a sorted one.
    let mut sorted = expected.clone();
    sorted.sort_unstable_by_key(|m| m.0);
    assert_ne!(
        sorted, expected,
        "the cell order is not the numeric order, so a sort would show"
    );
}

/// Drudge Hideout's entry room, and the surfaces of what stands in it: the cobweb `0x0800013C`
/// and the floor stain `0x08000140` (both `Translucent | ClipMap`), and the cage door's bars
/// `0x080004BE` (`ClipMap`).
const HIDEOUT_ROOM: u32 = 0x019E_0114;
const CLIP_MAPPED: [u32; 3] = [0x0800_013C, 0x0800_0140, 0x0800_04BE];

/// Behaviour: rendering.draw-order.a-dungeon-web-is-drawn-over-what-stands-behind-it
/// A cobweb's subset goes on the clip list, drawn in place as well with Multiple Pass Alpha on and
/// only from the list with it off; and in a dungeon room sealed from the outdoors, the frame's
/// only alpha flush comes after every cell's objects, the creatures and doors among them.
#[test]
fn a_dungeon_web_is_clip_listed_and_its_room_flushes_after_every_cells_objects() {
    use dereth_world_render::objects::draw::{classify_subset_passes, subset_mask, SubsetPasses};
    let s = store();
    for id in CLIP_MAPPED {
        let b = s
            .read_typed(DbType::Surface, DataId(id))
            .expect("the surface record");
        let DecodedAsset::Surface(surface) =
            decode_any(DbType::Surface, DataId(id), &b).expect("decodes")
        else {
            panic!("{id:#010X} is not a surface")
        };
        let mask = subset_mask(surface.surface_type);
        assert_eq!(mask, 8, "{id:#010X} (type {:#x})", surface.surface_type);
        let delay = dereth_terrain::consts::S_ALPHA_DELAY_MASK;
        assert_eq!(
            classify_subset_passes(mask, delay, true),
            SubsetPasses {
                list: Some(AlphaList::Clip),
                immediate: true,
                multipass: true,
            },
            "{id:#010X} with Multiple Pass Alpha on"
        );
        assert_eq!(
            classify_subset_passes(mask, delay, false),
            SubsetPasses {
                list: Some(AlphaList::Clip),
                immediate: false,
                multipass: false,
            },
            "{id:#010X} with Multiple Pass Alpha off"
        );
    }

    let mut cells: BTreeMap<u32, TraversalCell> = BTreeMap::new();
    for idx in 0x0100u32..0x0200 {
        let id = DataId((HIDEOUT_ROOM & 0xFFFF_0000) | idx);
        let Ok(bytes) = s.read_typed(DbType::Cell, id) else {
            continue;
        };
        let DecodedAsset::EnvCell(ec) = decode_any(DbType::Cell, id, &bytes).expect("decodes")
        else {
            continue;
        };
        let portals = ec
            .portals
            .iter()
            .map(|p| CellPortal {
                portal_side: u8::from(p.flags & 2 != 0),
                other_cell_id: if p.other_cell_id == 0xFFFF_FFFF {
                    OUTDOORS
                } else {
                    p.other_cell_id
                },
                other_portal_id: i32::from(p.other_portal_id),
                exact_match: p.flags & 1 != 0,
                vertex_dist_sq: [f32::from(p.polygon_id) + 1.0; 4],
                viewpoint_side_distance: if p.flags & 2 != 0 { -1.0 } else { 1.0 },
            })
            .collect();
        cells.insert(
            id.0,
            TraversalCell {
                id: CellId(id.0),
                portals,
            },
        );
    }
    assert!(
        cells.len() > 50,
        "the hideout has {} cells; its cell records are not being read",
        cells.len()
    );
    let view = construct_view(&cells, CellId(HIDEOUT_ROOM), &|_, _| true);
    assert_eq!(
        view.outside_view_count, 0,
        "no portal chain out of the hideout reaches the outdoors"
    );
    assert!(
        view.cell_draw_list.contains(&CellId(HIDEOUT_ROOM)),
        "the walk draws the entry room"
    );
    let steps = indoor_steps(&view);
    let objects = steps
        .iter()
        .position(|x| *x == IndoorStep::Objects)
        .expect("the cells' objects are drawn");
    let flushes: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, x)| matches!(x, IndoorStep::FlushBeforeClear | IndoorStep::FlushAlphaList))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        flushes,
        vec![steps.len() - 1],
        "the one flush is the last step: {steps:?}"
    );
    assert!(
        objects < flushes[0],
        "and it follows the objects: {steps:?}"
    );
}
