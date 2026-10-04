//! A landblock that leaves the streaming window releases its resident interiors and baked physics
//! objects. Release removes the block's objects, releases the visible interiors on the block's
//! cell list, then releases the block itself; the window's scroll does this on each edge it
//! leaves, and flushing a released interior also removes its baked objects.
//!
//! The walk goes out five blocks on the diagonal and back, and is run with both arms of
//! `SceneConfig::release_interiors`: the disabled arm keeps every departed block resident, which
//! is the control. Each station reads cells, batches, bodies (interior and outdoor separately),
//! held triangles and descriptors, with release and accounting controls; collision and pixel
//! stations add evidence that the released walls no longer stop a body and that a round trip
//! draws the same frame. Fixture: Holtburg's window on the retail dats (fails without them) and a
//! software device.
//!
//! Sections of this module:
//! * `interior_cell_objects`: a departed block releases the objects in its cells, through the
//!   production path.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::character::CharacterInput;
use dereth_client::world::{block_xy, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_dat::RetailDatStore;
use dereth_physics::LandSource;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;

/// The retail dats, or a failed test: a test that returned early without them would read as a
/// pass that tested nothing.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Landblock offsets from Holtburg with land radius 3: a 7x7 window. Home leaves at +4 and
/// reenters at +3 on the return. End at the original station for a direct endpoint comparison.
///
/// A straight walk east plateaus in resident cells at station 2 because it finds no new interior
/// population; the diagonal exposes a different population at every station. A one-block diagonal
/// shift of a full 7x7 square introduces 13 blocks, versus 7 for an axial
/// shift; actual new interiors depend on the DAT rather than on those window counts alone.
const WALK: [(i32, i32); 11] = [
    (0, 0),
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 4),
    (5, 5),
    (4, 4),
    (3, 3),
    (2, 2),
    (1, 1),
    (0, 0),
];

/// The index of the far station, where the starting block has been out of the window for two
/// stations. Named rather than written as `5` at three call sites.
const FAR: usize = 5;

/// Everything one station of the walk holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Reading {
    blocks: usize,
    /// Resident interior cells available to physics collision lookup.
    cells: usize,
    /// Resident building shells registered in outdoor land cells.
    buildings: usize,
    /// Physics bodies from baked cell objects, interior and outdoor.
    bodies: usize,
    /// Bodies assigned to interior cell indices >= FIRST_ENV_CELL.
    interior_bodies: usize,
    /// Bodies in outdoor land cells (indices 1..=0x40), the outdoor scenery. Reported
    /// separately: the two kinds are created under different radii but their owning blocks are
    /// released under the same window rule.
    outdoor_bodies: usize,
    /// How many **landblocks** those bodies span. The count alone cannot say whether a residency
    /// is bounded by the window or by the walk, and that is the whole question here.
    body_blocks: usize,
    /// Draw batches those objects cost.
    batches: usize,
    /// Triangles they **hold** (every degrade level), the interior half of the scene's figure.
    cell_triangles: usize,
    /// Triangles the outdoor placements hold.
    object_triangles: usize,
    /// Currently held texture-descriptor pairs, not total uploaded bytes or a fixed heap capacity.
    slots: u32,
    /// Cumulative departed-block release count.
    blocks_released: u64,
    cells_released: u64,
    bodies_destroyed: u64,
    /// `CellLoadStats`, so the residency can be checked against `loaded - released`.
    loaded: u64,
    released: u64,
    /// `BuildingLoadStats`, likewise.
    registered: u64,
    shells_released: u64,
}

/// Split [`Reading::bodies`] into its interior and outdoor halves.
///
/// A cell ID is block << 16 | index. Interior indices begin at FIRST_ENV_CELL; outdoor land
/// cells occupy 1..=0x40. Use the shared discriminator, as cell_is_resident does, so the threshold
/// cannot drift. This splitter counts every non-interior entry in the outdoor bucket.
fn body_halves(scene: &WorldScene) -> (usize, usize) {
    let (mut interior, mut outdoor) = (0usize, 0usize);
    for cell in scene.cell_static_cells() {
        let n = scene.cell_static_handles(cell).len();
        if cell.0 & 0xFFFF >= dereth_client::env_cells::FIRST_ENV_CELL {
            interior += n
        } else {
            outdoor += n
        }
    }
    (interior, outdoor)
}

fn read(scene: &WorldScene) -> Reading {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let cs = land.cell_stats();
    let bs = land.building_stats();
    let (interior_bodies, outdoor_bodies) = body_halves(scene);
    Reading {
        blocks: scene.resident_blocks(),
        cells: land.resident_cells(),
        buildings: land.resident_buildings(),
        bodies: scene.cell_static_bodies(),
        interior_bodies,
        outdoor_bodies,
        body_blocks: blocks_spanned(&scene.cell_static_cells()),
        batches: scene.draw.stats.cell_static_batches,
        cell_triangles: scene.draw.stats.cell_static_triangles_resident,
        object_triangles: scene.draw.stats.object_triangles_resident,
        slots: scene.draw.stats.textures_uploaded,
        blocks_released: scene.draw.stats.blocks_released,
        cells_released: scene.draw.stats.cells_released,
        bodies_destroyed: scene.draw.stats.cell_statics_destroyed,
        loaded: cs.loaded,
        released: cs.released,
        registered: bs.registered,
        shells_released: bs.released,
    }
}

/// Build the scene with a body in it, settled on its starting block.
fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu, release: bool) -> WorldScene {
    let cfg = SceneConfig {
        release_interiors: release,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// Stand the body in the middle of one landblock and let the window catch up.
///
/// A teleport rather than a walk, deliberately: this test is about the block **window**, and a
/// scripted walk would spend a thousand frames crossing a block and would make the reading a
/// function of the physics as well as of the streaming.
fn go_to(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu, block: LandblockId) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let mid = 96.0f32;
    let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            block.cell(1),
            Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
        ));
    }
    scene.follow_character_now();
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
    scene.stream(store, gpu).expect("the streamed blocks build");
}

/// Draw and capture RGBA using App::frame's device sequence, without advancing the clock.
/// Repeated stations use the same simulation time; pixel equality remains an asserted property.
fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Vec<u8> {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// Run [`WALK`] and report a [`Reading`] at every station, starting with the settled home reading.
fn walk(store: &Arc<RetailDatStore>, gpu: &mut Gpu, release: bool) -> (Vec<Reading>, WorldScene) {
    let mut scene = embodied(store, gpu, release);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut out = Vec::with_capacity(WALK.len());
    for (dx, dy) in WALK {
        go_to(&mut scene, store, gpu, block_at(hx + dx, hy + dy));
        out.push(read(&scene));
    }
    (out, scene)
}

/// `(blockX, blockY)` as a [`LandblockId`], which is `x << 8 | y`.
fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above. Not a float conversion.
    LandblockId(((x as u16) << 8) | (y as u16))
}

/// How many distinct landblocks a set of cell ids spans. A cell id is `blockId << 16 | index`.
fn blocks_spanned(cells: &[CellId]) -> usize {
    cells
        .iter()
        .map(|c| c.0 >> 16)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

// ---------------------------------------------------------------------------------------------
// 1. Residency with and without release, over one fixed walk
// ---------------------------------------------------------------------------------------------

/// Count both arms of `SceneConfig::release_interiors` over the same dat-backed walk. Clearing it
/// keeps every departed block's interiors while both arms stream draw data the same way. Require
/// accumulated control residency, released populations and return counts, with separate controls
/// for the fields the switch should not affect.
#[test]
fn a_walk_that_leaves_a_region_and_returns_ends_where_it_started() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    let (off, mut off_scene) = walk(&store, &mut gpu, false);
    off_scene.release_textures(&mut gpu);
    drop(off_scene);
    let (on, mut on_scene) = walk(&store, &mut gpu, true);

    let fmt = |r: &[Reading], f: fn(&Reading) -> usize| -> Vec<usize> { r.iter().map(f).collect() };
    eprintln!("stations (landblock offsets from Holtburg): {WALK:?}");
    eprintln!("cells resident   OFF: {:?}", fmt(&off, |r| r.cells));
    eprintln!("cells resident   ON : {:?}", fmt(&on, |r| r.cells));
    eprintln!("bodies resident  OFF: {:?}", fmt(&off, |r| r.bodies));
    eprintln!("bodies resident  ON : {:?}", fmt(&on, |r| r.bodies));
    // The two halves of the line above.
    eprintln!(
        "  interior bodies OFF: {:?}",
        fmt(&off, |r| r.interior_bodies)
    );
    eprintln!(
        "  interior bodies ON : {:?}",
        fmt(&on, |r| r.interior_bodies)
    );
    eprintln!(
        "  outdoor bodies  OFF: {:?}",
        fmt(&off, |r| r.outdoor_bodies)
    );
    eprintln!(
        "  outdoor bodies  ON : {:?}",
        fmt(&on, |r| r.outdoor_bodies)
    );
    eprintln!("blocks w/ bodies OFF: {:?}", fmt(&off, |r| r.body_blocks));
    eprintln!("blocks w/ bodies ON : {:?}", fmt(&on, |r| r.body_blocks));
    eprintln!("shells resident  OFF: {:?}", fmt(&off, |r| r.buildings));
    eprintln!("shells resident  ON : {:?}", fmt(&on, |r| r.buildings));
    eprintln!("cell batches     OFF: {:?}", fmt(&off, |r| r.batches));
    eprintln!("cell batches     ON : {:?}", fmt(&on, |r| r.batches));
    eprintln!("blocks resident  OFF: {:?}", fmt(&off, |r| r.blocks));
    eprintln!("blocks resident  ON : {:?}", fmt(&on, |r| r.blocks));
    eprintln!(
        "descriptor slots OFF: {:?}",
        fmt(&off, |r| r.slots as usize)
    );
    eprintln!("descriptor slots ON : {:?}", fmt(&on, |r| r.slots as usize));

    let (h_off, e_off) = (off[0], off[WALK.len() - 1]);
    let (h_on, e_on) = (on[0], on[WALK.len() - 1]);
    #[allow(clippy::cast_precision_loss)] // counts of cells and bodies, four digits at most
    let ratio = |a: usize, b: usize| a as f64 / b as f64;
    eprintln!(
        "OVER THE WALK, end / home. cells: {} -> {} ({:.2}x) OFF, {} -> {} ({:.2}x) ON; \
         bodies: {} -> {} ({:.2}x) OFF, {} -> {} ({:.2}x) ON; shells: {} -> {} ({:.2}x) OFF, \
         {} -> {} ({:.2}x) ON; cell batches: {} -> {} ({:.2}x) OFF, {} -> {} ({:.2}x) ON; \
         descriptor slots: {} -> {} ({:.2}x) OFF, {} -> {} ({:.2}x) ON; triangles held \
         (interior): {} -> {} OFF, {} -> {} ON; (outdoor): {} -> {} OFF, {} -> {} ON",
        h_off.cells,
        e_off.cells,
        ratio(e_off.cells, h_off.cells),
        h_on.cells,
        e_on.cells,
        ratio(e_on.cells, h_on.cells),
        h_off.bodies,
        e_off.bodies,
        ratio(e_off.bodies, h_off.bodies),
        h_on.bodies,
        e_on.bodies,
        ratio(e_on.bodies, h_on.bodies),
        h_off.buildings,
        e_off.buildings,
        ratio(e_off.buildings, h_off.buildings),
        h_on.buildings,
        e_on.buildings,
        ratio(e_on.buildings, h_on.buildings),
        h_off.batches,
        e_off.batches,
        ratio(e_off.batches, h_off.batches),
        h_on.batches,
        e_on.batches,
        ratio(e_on.batches, h_on.batches),
        h_off.slots,
        e_off.slots,
        ratio(e_off.slots as usize, h_off.slots as usize),
        h_on.slots,
        e_on.slots,
        ratio(e_on.slots as usize, h_on.slots as usize),
        h_off.cell_triangles,
        e_off.cell_triangles,
        h_on.cell_triangles,
        e_on.cell_triangles,
        h_off.object_triangles,
        e_off.object_triangles,
        h_on.object_triangles,
        e_on.object_triangles,
    );

    // --- the denominators, first, because a zero from an instrument that never looked is not a
    // measurement. `blocks_released` counts a block even when it turned out to hold no interior.
    assert!(
        h_off.cells > 0,
        "the starting window holds no interior cells at all"
    );
    assert!(
        h_off.bodies > 0,
        "the starting window's cells hold no baked furniture"
    );
    // And both halves of that number are looking: an instrument that cannot look reports
    // absence, and there are two of them behind the one figure above.
    assert!(
        h_off.interior_bodies > 0,
        "no interior cell in the starting window holds furniture"
    );
    assert!(
        h_off.outdoor_bodies > 0,
        "no land cell in the starting window holds scenery"
    );
    assert_eq!(
        (
            e_off.blocks_released,
            e_off.cells_released,
            e_off.bodies_destroyed
        ),
        (0, 0, 0),
        "the disabled-release control released something"
    );
    assert!(
        e_on.blocks_released > 0,
        "no block was released over a walk of {} stations",
        WALK.len()
    );
    assert!(
        e_on.cells_released > 0,
        "blocks were released and none of them held a cell"
    );
    assert!(
        e_on.bodies_destroyed > 0,
        "cells were released and none of them held a body"
    );

    // The disabled arm must never decrease these physics-residency counts along this walk.
    // Require endpoint accumulation separately; its size depends on the new population visited,
    // so ratios are reported rather than prescribed. These are sampled counts, not a proof
    // about every possible walk or every future residency mechanism.
    for (k, w) in off.windows(2).enumerate() {
        assert!(
            w[1].cells >= w[0].cells,
            "the control arm's cells fell at station {}",
            k + 1
        );
        assert!(
            w[1].bodies >= w[0].bodies,
            "the control arm's bodies fell at station {}",
            k + 1
        );
        // Separately, so that a control arm that leaks only one of the two halves is
        // not covered by the other one growing faster.
        assert!(
            w[1].interior_bodies >= w[0].interior_bodies,
            "the control arm's interior bodies fell at station {}",
            k + 1
        );
        assert!(
            w[1].outdoor_bodies >= w[0].outdoor_bodies,
            "the control arm's outdoor bodies fell at station {}",
            k + 1
        );
        assert!(
            w[1].buildings >= w[0].buildings,
            "the control arm's building shells fell at station {}",
            k + 1
        );
    }
    assert!(
        e_off.cells > h_off.cells,
        "the control arm ended with {} cells and started with {}, so nothing accumulated and \
         this test is not measuring the leak",
        e_off.cells,
        h_off.cells
    );
    assert!(
        e_off.bodies > h_off.bodies,
        "the control arm's bodies did not accumulate"
    );
    assert!(
        e_off.buildings > h_off.buildings,
        "the control arm's shells did not accumulate"
    );
    // A count below the initial reading refutes non-decreasing residency on this walk.
    // Merely finding unequal minimum and maximum would not establish a decrease.
    let min_cells = on.iter().map(|r| r.cells).min().expect("stations");
    assert!(
        min_cells < h_on.cells,
        "the release arm's cell count never fell"
    );
    // The below-initial test is on the interior population. The outdoor scenery population
    // grows past its first reading on the way out (it is created under a wider radius) and never
    // falls below it, so the outdoor half below must show an adjacent decrease instead.
    let min_interior = on
        .iter()
        .map(|r| r.interior_bodies)
        .min()
        .expect("stations");
    assert!(
        min_interior < h_on.interior_bodies,
        "the release arm's interior body count never fell below its first reading ({} vs {})",
        min_interior,
        h_on.interior_bodies
    );
    // And the outdoor half is held to the claim it *can* answer: it falls somewhere. The control
    // arm cannot -- it is asserted monotone non-decreasing above -- so "falls at some station" is
    // a genuine differential between the arms and not a restatement of the series.
    assert!(
        on.windows(2)
            .any(|w| w[1].outdoor_bodies < w[0].outdoor_bodies),
        "the release arm's outdoor scenery bodies never fell at any station, so the outdoor half \
         of this table is never released: {:?}",
        fmt(&on, |r| r.outdoor_bodies)
    );
    assert!(
        on.windows(2).any(|w| w[1].bodies < w[0].bodies),
        "the release arm's total body count never fell at any station"
    );

    // At home again, require equal window/cell/shell/batch/triangle counts. Equal cardinalities
    // alone do not identify every restored object; later checks add selected geometry evidence.
    assert_eq!(
        e_on.blocks, h_on.blocks,
        "the block window did not come back"
    );
    assert_eq!(
        e_on.cells, h_on.cells,
        "the interior cells did not come back to their first-visit figure"
    );
    assert_eq!(
        e_on.buildings, h_on.buildings,
        "the building shells did not"
    );
    assert_eq!(e_on.batches, h_on.batches, "the cell batches did not");
    assert_eq!(
        e_on.cell_triangles, h_on.cell_triangles,
        "the interior triangles held did not"
    );
    assert_eq!(
        e_on.object_triangles, h_on.object_triangles,
        "the outdoor triangles held did not"
    );

    // Body counts are not required to return exactly. Cells/shells cover the land-radius 3
    // window, while baking creates body populations within scenery radius 1. A cell can acquire
    // furniture after the player approaches it and retain it while its block remains resident:
    // path-dependent residency, unlike retail's per-cell construction for every listed interior.
    // BlockDraw::cell_statics is populated through that baking route.
    //
    // Require the final body-block count to fit within the resident-block count and be smaller
    // than the leaking control's, with no fewer total bodies than the first visit. This bounds
    // counts; the inequality alone does not prove set inclusion for every body-owning block.
    // The endpoint body inequality remains visible as a separate residency limitation.
    assert!(
        e_on.body_blocks <= e_on.blocks,
        "the release arm holds bodies for {} blocks but only {} are resident, so a released \
         block's furniture survived it",
        e_on.body_blocks,
        e_on.blocks
    );
    assert!(
        e_off.body_blocks > e_on.body_blocks,
        "the control arm spans {} blocks of furniture and the release arm {}, so the release \
         changed no bound",
        e_off.body_blocks,
        e_on.body_blocks
    );
    assert!(
        e_on.bodies >= h_on.bodies,
        "the release arm ended with fewer bodies ({}) than it started with ({}), which is not \
         the path-dependence described above but a release that took too much",
        e_on.bodies,
        h_on.bodies
    );

    // --- the two arms must agree about everything the release does not touch, or the differential
    // above is reading something else. Both arms bake the same blocks from the same dat.
    for (k, (a, b)) in off.iter().zip(&on).enumerate() {
        assert_eq!(
            a.blocks, b.blocks,
            "station {k}: the block window differs between the arms"
        );
        assert_eq!(
            a.object_triangles, b.object_triangles,
            "station {k}: the outdoor bake differs between the arms"
        );
    }

    // --- the residency identities. `resident == loaded - released` and
    // `bodies == created - destroyed` are what separate "released the right things" from
    // "released nothing" and from "removed the table entry and leaked the body".
    let land = Arc::clone(on_scene.character.as_ref().expect("a body").land());
    let cs = land.cell_stats();
    assert_eq!(
        u64::try_from(e_on.cells).expect("small"),
        cs.loaded - cs.released,
        "resident cells {} is not loaded {} minus released {}",
        e_on.cells,
        cs.loaded,
        cs.released
    );
    assert_eq!(
        cs.blocks_released, e_on.blocks_released,
        "two counters for the same releases disagree"
    );
    let bs = land.building_stats();
    assert_eq!(
        u64::try_from(e_on.buildings).expect("small"),
        bs.registered - bs.released,
        "resident shells {} is not registered {} minus released {}",
        e_on.buildings,
        bs.registered,
        bs.released
    );
    let ss = on_scene.cell_static_stats();
    assert_eq!(
        u64::try_from(e_on.bodies).expect("small"),
        ss.created - ss.destroyed,
        "resident bodies {} is not created {} minus destroyed {}",
        e_on.bodies,
        ss.created,
        ss.destroyed
    );
    assert!(
        ss.cells_released > 0,
        "no cell was taken out of the static-object table"
    );

    on_scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 2. What a re-entered block costs the second time
// ---------------------------------------------------------------------------------------------

/// Retail's visible-cell lookup checks the resident table, then a released-cell cache, then the
/// database cache, so release does not imply another disk read. The client's decode memos
/// likewise survive release (`EnvCellLoader::environments` and `CellStaticObjects::geometry`).
///
/// Measure equal returning cell residency and additional load registrations after the far
/// station, plus bounded descriptor residency. The counters do not separately measure decode
/// calls, cache misses or disk I/O, so they cannot alone establish the exact cost of a return.
#[test]
fn a_re_entered_block_re_registers_and_holds_no_second_copy() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let (r, mut scene) = walk(&store, &mut gpu, true);
    let (home, far, back) = (r[0], r[FAR], r[WALK.len() - 1]);

    // Home lies outside the 7x7 window by +4. Require some release by +5 and confirm a home
    // interior is resident after returning. The separate wall test checks a selected cell's
    // actual absence at the far point; this aggregate reading alone does not name that cell.
    assert!(
        far.cells_released > 0,
        "nothing had been released by the far station"
    );
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let home_block = LandblockId(DEFAULT_LANDBLOCK);
    assert!(
        land.env_cell(CellId((u32::from(DEFAULT_LANDBLOCK) << 16) | 0x0100))
            .is_some(),
        "Holtburg's first interior cell is not resident at the end of the walk"
    );

    // Returning cell count matches home, while load registrations must rise after the far
    // station. This checks registration work, not independently measured redecoding cost.
    eprintln!(
        "home -> far -> back. cells {} -> {} -> {}; bodies {} -> {} -> {}; \
         loaded {} -> {} -> {}; released {} -> {} -> {}; created-side registrations rose by {}",
        home.cells,
        far.cells,
        back.cells,
        home.bodies,
        far.bodies,
        back.bodies,
        home.loaded,
        far.loaded,
        back.loaded,
        home.released,
        far.released,
        back.released,
        back.loaded - home.loaded,
    );
    assert_eq!(
        back.cells, home.cells,
        "the second visit is not the same residency as the first"
    );
    assert!(
        back.loaded > far.loaded,
        "no cell was loaded between the far station and home, so the block came back from a \
         table the release did not clear"
    );

    // Departed blocks return their texture links (`WorldScene::release_departed_blocks`), so a
    // return legitimately reacquires links: require more held pairs than at the far point but no
    // more than on the first home visit. These counts constrain retained descriptors, not total
    // allocation traffic, memo hits or execution time.
    assert!(
        back.slots > far.slots,
        "the return visit uploaded nothing ({} at the far station, {} at home) -- the blocks that \
         left never returned their links, rather than sharing a memo",
        far.slots,
        back.slots
    );
    assert!(
        back.slots <= home.slots,
        "the second visit holds {} pairs against the first visit's {}; the surface memo is not \
         being shared and the residency is a function of the walk rather than of the window",
        back.slots,
        home.slots
    );

    // Check a distant never-visited cell remains absent and at least one of the first 64 home
    // interior IDs is resident. This is a bounded presence check, not a full identity census.
    let far_away = CellId(0x0102_0100);
    assert!(
        land.env_cell(far_away).is_none(),
        "a block outside the window is resident"
    );
    let mut n = 0usize;
    for i in 0..64u32 {
        if land
            .env_cell(CellId((u32::from(home_block.0) << 16) | (0x0100 + i)))
            .is_some()
        {
            n += 1;
        }
    }
    assert!(
        n > 0,
        "the starting block has no resident interior cell after the return"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// Choosing a room that confines a body
// ---------------------------------------------------------------------------------------------
//
// A room is chosen by the property the wall test needs: it confines a body of the player's own
// width. `WorldScene::standable_point`'s zero-size point checks are not enough (a point 0.75 m
// inside a doorway is "in the room" and the body walks out of it).
//
// Select room, pose and heading by marching the live body's own spheres against cell geometry
// plus baked-object spheres and part BSPs read from the same physics arena. This avoids the
// dynamic transition/update path judged by the 300-step control, while sharing low-level geometry
// queries. The 24 straight marches can disagree with sliding/contact/gravity simulation, which is
// why the settle, minimum-wall-distance and blocked-arc requirements below exist.

/// One interior cell as an obstacle set: its own geometry, plus the world spheres and part BSPs of
/// the objects baked into it, taken from the same `PhysicsWorld` the body walks in.
struct CellObstacles {
    geom: Arc<dereth_physics::source::EnvCellGeometry>,
    spheres: Vec<dereth_physics::geom::Sphere>,
    parts: Vec<dereth_physics::source::PhysicsPart>,
}

/// Check a body's sphere against each placed part's bounding sphere and solid BSP in part-local
/// coordinates. BSP-only statics must be included (as in `world::interiors`): a sphere-only
/// obstacle census can miss geometry that the dynamic body collides with.
fn part_blocks(parts: &[dereth_physics::source::PhysicsPart], centre: Vec3, r: f32) -> bool {
    parts.iter().any(|part| {
        let Some(tree) = part.physics_bsp.as_ref() else {
            return false;
        };
        let Some(root) = part.physics_sphere() else {
            return false;
        };
        let inv = 1.0 / part.gfxobj_scale;
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let local = dereth_physics::math::globaltolocalvec(
            m,
            Vec3::new(
                centre.x - part.pos.frame.origin.x,
                centre.y - part.pos.frame.origin.y,
                centre.z - part.pos.frame.origin.z,
            ),
        );
        let ls = dereth_physics::geom::Sphere::new(
            Vec3::new(local.x * inv, local.y * inv, local.z * inv),
            r * inv,
        );
        dereth_physics::geom::bsp::spheres_intersect(&root, &ls)
            && tree.sphere_intersects_solid(&ls, false)
    })
}

/// Read the live body's local path spheres from its physics geometry. Reuse those shapes
/// rather than reconstructing radii/offsets that could drift from the dynamic collision body.
fn body_spheres(scene: &WorldScene) -> Vec<dereth_physics::geom::Sphere> {
    let c = scene.character.as_ref().expect("a body");
    let o = c
        .world
        .get(c.handle)
        .expect("the body is in the physics arena");
    let s = o.geometry.path_spheres().to_vec();
    assert!(
        !s.is_empty(),
        "the body has no path spheres, so every question below is a point test"
    );
    s
}

/// Query the first 64 interior IDs of this block and collect resident geometry/baked obstacles.
fn block_obstacles(scene: &WorldScene, block: LandblockId) -> Vec<CellObstacles> {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let c = scene.character.as_ref().expect("a body");
    let mut out = Vec::new();
    for i in 0..64u32 {
        let id = CellId((u32::from(block.0) << 16) | (0x0100 + i));
        let Some(geom) = LandSource::env_cell(land.as_ref(), id) else {
            continue;
        };
        let (mut spheres, mut parts) = (Vec::new(), Vec::new());
        for h in scene.cell_static_handles(id) {
            let Some(o) = c.world.get(*h) else { continue };
            let m = dereth_physics::math::l2g(o.position.frame.rotation);
            for x in o.geometry.path_spheres() {
                let p = dereth_physics::math::localtoglobalvec(m, x.center);
                spheres.push(dereth_physics::geom::Sphere::new(
                    Vec3::new(
                        p.x + o.position.frame.origin.x,
                        p.y + o.position.frame.origin.y,
                        p.z + o.position.frame.origin.z,
                    ),
                    x.radius * o.scale,
                ));
            }
            for k in 0..o.geometry.parts.len() {
                if let Some(pp) = o.geometry.placed_part(k, &o.position, o.scale) {
                    if pp.physics_bsp.is_some() {
                        parts.push(pp);
                    }
                }
            }
        }
        out.push(CellObstacles {
            geom,
            spheres,
            parts,
        });
    }
    out
}

/// Is the body's pose at `p` (block-local) free of solid geometry, and is `p` inside any of these
/// cells? Both answers, because *"free"* alone is true of open sky.
fn body_free_at(
    cells: &[CellObstacles],
    body: &[dereth_physics::geom::Sphere],
    p: Vec3,
) -> (bool, bool) {
    let mut inside_any = false;
    for c in cells {
        let g = &c.geom;
        let Some(cb) = g.cell_bsp.as_ref() else {
            continue;
        };
        if !cb.point_inside_cell_bsp(dereth_physics::math::globaltolocal(&g.frame, p)) {
            continue;
        }
        inside_any = true;
        for b in body {
            let wc = Vec3::new(p.x + b.center.x, p.y + b.center.y, p.z + b.center.z);
            if let Some(pb) = g.physics_bsp.as_ref() {
                let lc = dereth_physics::math::globaltolocal(&g.frame, wc);
                if pb.sphere_intersects_solid(
                    &dereth_physics::geom::Sphere::new(lc, b.radius),
                    false,
                ) {
                    return (false, inside_any);
                }
            }
            for o in &c.spheres {
                let (dx, dy, dz) = (wc.x - o.center.x, wc.y - o.center.y, wc.z - o.center.z);
                let r = b.radius + o.radius;
                if dx.mul_add(dx, dy.mul_add(dy, dz * dz)) <= r * r {
                    return (false, inside_any);
                }
            }
            if part_blocks(&c.parts, wc, b.radius) {
                return (false, inside_any);
            }
        }
    }
    (true, inside_any)
}

/// How far each march below runs before it gives up and answers [`Fate::Open`]. It is the **same
/// number** as the `< 12.0` control, deliberately: a heading whose wall is further away than the
/// control's own bound tells the control nothing.
const WALKWAY_M: f32 = 12.0;

/// Sample the straight march every 0.05 m, well below the measured 0.480 m body-sphere radius.
/// This is a discrete geometry probe, not a general continuous-collision proof.
const MARCH_STEP: f32 = 0.05;

/// Require 1 m continuously outside the queried interior set before calling it an exit.
/// As in `world::interiors`, this avoids treating the few-centimetre gap at a portal seam as a doorway.
const CLEAR_RUN: f32 = 1.0;

/// Result of a discrete straight sphere march against the collected cell/object geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fate {
    /// A sampled body sphere meets solid geometry at this distance.
    Blocked(f32),
    /// The sampled path stays outside the queried interior set for CLEAR_RUN without a hit.
    ClearExit(f32),
    /// Neither, within [`WALKWAY_M`].
    Open,
}

/// Rotate both local sphere offsets and the forward vector by the heading quaternion, then
/// sample along that direction, as in `world::interiors`. This avoids assuming a world-axis index.
///
/// Limitation: swapping forward (0, 1, 0) for (1, 0, 0) rotates the 24-heading sampling by six
/// indices, and the selected alcove (blocked 90 degrees either side) still finds a confining
/// direction, so this suite does not distinguish the local forward-axis spelling.
fn straight_fate(
    cells: &[CellObstacles],
    body: &[dereth_physics::geom::Sphere],
    start: Vec3,
    heading: Quat,
) -> Fate {
    let m = dereth_physics::math::l2g(heading);
    let dir = dereth_physics::math::localtoglobalvec(m, Vec3::new(0.0, 1.0, 0.0));
    let posed: Vec<dereth_physics::geom::Sphere> = body
        .iter()
        .map(|b| {
            dereth_physics::geom::Sphere::new(
                dereth_physics::math::localtoglobalvec(m, b.center),
                b.radius,
            )
        })
        .collect();
    let mut out_since: Option<f32> = None;
    let mut i = 0i32;
    loop {
        #[allow(clippy::cast_precision_loss)] // a bounded march counter
        let d = MARCH_STEP * (i as f32);
        if d > WALKWAY_M {
            return Fate::Open;
        }
        let p = Vec3::new(
            start.x + dir.x * d,
            start.y + dir.y * d,
            start.z + dir.z * d,
        );
        let (free, inside) = body_free_at(cells, &posed, p);
        if !free {
            return Fate::Blocked(d);
        }
        if inside {
            out_since = None;
        } else {
            let since = *out_since.get_or_insert(d);
            if d - since >= CLEAR_RUN {
                return Fate::ClearExit(since);
            }
        }
        i += 1;
    }
}

/// How many headings are tried at each standing pose, and the yaw of the first.
///
/// Exactly cardinal headings into axis-aligned Holtburg walls meet floating-point cancellation
/// that reads like a defect; 24 headings from 0 degrees would include four normal approaches.
/// Offset them by 7.5 degrees so none of these samples is exactly cardinal.
const HEADINGS: u32 = 24;
const HEADING_OFFSET_DEG: f32 = 7.5;

/// How wide the blocked arc around the chosen heading has to be, in [`HEADINGS`] steps. Six of
/// twenty-four is 90 degrees either side.
///
/// A wall-only heading lets sliding escape (39 m); blocked 45-degree neighbours still let the body
/// out through a portal (13.9 m). The wider arc constrains the selected walk rather than proving
/// no route out of the connected building within 12 m.
/// Dynamic sliding remains independently checked by the resident control.
const ARC: u32 = HEADINGS / 4;

/// The band the wall the body is aimed at has to fall in.
///
/// A run can cover 38 m in 10 seconds; a wall within 4 m leaves 8 m below the 12 m control bound
/// for sliding, which the straight probe cannot predict. With no minimum, a pose 0.05 m from a
/// wall barely moves and passes for lack of useful motion; requiring at least 1.5 m clear ahead
/// gives the later positive-motion assertion a denominator.
const WALL_AT_LEAST_M: f32 = 1.5;
const WALL_WITHIN_M: f32 = 4.0;

/// Yaw heading k rotates local +y to (-sin t, cos t). This matches the current
/// WorldScene::attach_character start-cell heading convention.
fn heading_of(k: u32) -> Quat {
    #[allow(clippy::cast_precision_loss)] // k < HEADINGS
    let deg = HEADING_OFFSET_DEG + 360.0 * (k as f32) / (HEADINGS as f32);
    let t = deg.to_radians() * 0.5;
    Quat::new(math::cosf(t), 0.0, 0.0, math::sinf(t))
}

/// Search downward at 0.05 m increments for a solid boundary within 4 m; return no settled pose
/// if this sampled search finds none. That does not prove the entire location lacks a floor.
const DROP_M: f32 = 4.0;
const DROP_STEP: f32 = 0.05;

/// Find the last sampled free pose before the first blocked downward step, or None within DROP_M.
/// This is not a search for the globally lowest free point or an independently verified floor normal.
///
/// Settling matters: a lattice origin is sampled above the cell frame, not the floor, and body
/// sphere centres at +0.475/+1.350 with radius 0.480 put the upper crown at 2.83 m for a 1 m
/// origin, inside a doorway lintel. Confinement at ceiling height is not walking confinement: the
/// dynamic body falls to the floor and walks out.
fn settle(cells: &[CellObstacles], body: &[dereth_physics::geom::Sphere], p: Vec3) -> Option<Vec3> {
    if !body_free_at(cells, body, p).0 {
        return None;
    }
    let mut here = p;
    let mut fallen = 0.0f32;
    while fallen < DROP_M {
        let next = Vec3::new(here.x, here.y, here.z - DROP_STEP);
        if !body_free_at(cells, body, next).0 {
            // Last free sample before a blocked downward step.
            return Some(here);
        }
        here = next;
        fallen += DROP_STEP;
    }
    None
}

/// Choose the first of the first 64 interior IDs that yields a qualifying pose/heading, then
/// maximize the central wall distance within that cell. The sampled requirements are:
/// 1. The cell is resident and its cell BSP contains the candidate origin.
/// 2. The body spheres initially fit the gathered cell/object geometry without overlap.
/// 3. A downward sample finds support, with the settled origin still inside some queried cell.
/// 4. The central heading hits a wall between 1.5 and 4 m away, inclusively.
/// 5. All 13 headings through 90 degrees either side hit within 4 m; neighboring headings are
///    not required to satisfy the central heading's 1.5 m lower bound.
///
/// Acceptance separately requires movement above 0.25 m and below 12 m over 300 dynamic steps.
///
/// This selector uses straight low-level geometry probes, not the full contact/sliding/gravity
/// update; where the two can disagree, the dynamic controls decide.
fn confining_room(scene: &WorldScene, block: LandblockId) -> Option<(CellId, Vec3, Quat, f32)> {
    let body = body_spheres(scene);
    let cells = block_obstacles(scene, block);
    assert!(
        !cells.is_empty(),
        "no interior cell of {block:?} is resident"
    );
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let (mut examined, mut standable, mut aimed) = (0usize, 0usize, 0usize);
    let mut best: Option<(CellId, Vec3, Quat, f32)> = None;
    for i in 0..64u32 {
        let id = CellId((u32::from(block.0) << 16) | (0x0100 + i));
        let Some(g) = LandSource::env_cell(land.as_ref(), id) else {
            continue;
        };
        let Some(cb) = g.cell_bsp.as_ref() else {
            continue;
        };
        examined += 1;
        // Sample the same half-metre XY lattice, here starting at local height 2 m, and settle
        // the actual body before choosing a heading.
        for a in -12i8..=12 {
            for b in -12i8..=12 {
                let local = Vec3::new(f32::from(a) * 0.5, f32::from(b) * 0.5, 2.0);
                if !cb.point_inside_cell_bsp(local) {
                    continue;
                }
                let from = dereth_physics::math::localtoglobal(&g.frame, local);
                let Some(p) = settle(&cells, &body, from) else {
                    continue;
                };
                // Require the settled origin inside some queried cell, not merely free space.
                // On this block the later alcove condition also rejects the sampled outside
                // candidates, so this states the intent early rather than being an independent
                // guard.
                if !body_free_at(&cells, &body, p).1 {
                    continue;
                }
                standable += 1;
                // All 24 fates once, then the arm test is arithmetic rather than 13 more marches.
                let fates: Vec<Fate> = (0..HEADINGS)
                    .map(|k| straight_fate(&cells, &body, p, heading_of(k)))
                    .collect();
                for k in 0..HEADINGS {
                    let Fate::Blocked(d) = fates[k as usize] else {
                        continue;
                    };
                    if !(WALL_AT_LEAST_M..=WALL_WITHIN_M).contains(&d) {
                        continue;
                    }
                    // Every sampled neighbor must hit within the upper band bound. Only the
                    // central heading above is required to have the lower clear-floor bound.
                    let alcove = (0..=2 * ARC).all(|o| {
                        let n = (k + HEADINGS + o - ARC) % HEADINGS;
                        matches!(fates[n as usize], Fate::Blocked(x) if x <= WALL_WITHIN_M)
                    });
                    if !alcove {
                        continue;
                    }
                    aimed += 1;
                    // Maximize qualifying central-wall distance within this cell. The lower
                    // bound avoids a pose too close to its wall to show motion; the later dynamic
                    // positive-motion assertion still verifies actual walking.
                    if best.is_none_or(|(_, _, _, bd)| d > bd) {
                        best = Some((id, p, heading_of(k), d));
                    }
                }
            }
        }
        if let Some((cell, p, _, d)) = best {
            eprintln!(
                "confining room {:#010X}: the body stands at ({:.2}, {:.2}, {:.2}) facing a wall \
                 {d:.2} m away, with every heading within 90 degrees of it blocked too. Chosen \
                 after {examined} resident cell(s), {standable} standing pose(s) and {aimed} \
                 alcove-facing heading(s).",
                cell.0, p.x, p.y, p.z
            );
            return best;
        }
    }
    eprintln!(
        "confining room: {examined} resident interior cell(s) examined, {standable} standing pose(s) \
         tried, {aimed} of them face an alcove -- none confines a body"
    );
    None
}

// ---------------------------------------------------------------------------------------------
// 3. The wall is gone from physics, not merely from a counter
// ---------------------------------------------------------------------------------------------

/// Behaviour: world.streaming.a-departing-block-releases-its-interiors-and-baked-bodies
///
/// Compare collision evidence for the same selected room before and after its block is released.
/// The resident dynamic walk establishes a moving but confined body. After release, repeat the
/// geometric sphere march and inspect saved physics handles. Returning a body to the now-absent
/// cell instead exercises the early missing-cell guard, not an ordinary unobstructed walk.
/// These are complementary checks of geometry release and update lifecycle, not one symmetric
/// movement-distance comparison.
#[test]
fn the_walls_of_a_released_block_no_longer_stop_a_body() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = embodied(&store, &mut gpu, true);
    let home = LandblockId(DEFAULT_LANDBLOCK);

    // A room of the starting block that **confines a body** -- not merely one that contains a
    // point. See the section above for why it is not the control restated.
    let Some((room, inside, heading, wall)) = confining_room(&scene, home) else {
        panic!("no interior cell of Holtburg confines a body of the player's own width");
    };
    assert!(
        wall >= WALL_AT_LEAST_M,
        "the wall {room:?} aims the body at is only {wall:.2} m away; a body with no floor in \
         front of it cannot move, and a body that cannot move satisfies every negative assertion \
         below at once"
    );

    // The same ten seconds of walking forward, from the same point and on the same heading,
    // reported as metres covered.
    let run = |scene: &mut WorldScene| -> f32 {
        {
            let c = scene.character.as_mut().expect("a body");
            c.teleport(Position::new(room, Frame::new(inside, heading)));
        }
        scene.follow_character_now();
        let started = scene.character.as_ref().expect("a body").position();
        let (from, start_cell) = (started.frame.origin, started.cell);
        let input = CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        };
        let step = dereth_physics::globals::MIN_QUANTUM;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a fixed simulation step in seconds, narrowed for the camera's f32 delta.
        let dt = step as f32;
        let mut now = 0.0f64;
        for _ in 0..300 {
            now += step;
            scene.update(
                dereth_client::camera::CameraInput::default(),
                input,
                LocalTime(now),
                dt,
            );
        }
        let end = scene.character.as_ref().expect("a body").position();
        let to = end.frame.origin;
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        // The trace, not only the difference: a displacement on its own cannot say whether the
        // body walked out of the room or was pushed out of it by the insert.
        eprintln!(
            "confining-room run: ({:.2}, {:.2}, {:.2}) in {:?} -> ({:.2}, {:.2}, {:.2}) in {:?}",
            from.x, from.y, from.z, start_cell, to.x, to.y, to.z, end.cell
        );
        dx.mul_add(dx, dy * dy).sqrt()
    };

    // Establish a moving, confined resident control before testing the released geometry.
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    assert!(
        land.env_cell(room).is_some(),
        "{room:?} is not resident before the scroll"
    );
    // The handles themselves, kept across the release. Everything else in this file counts bodies
    // *through* `CellStaticObjects`, and a release that forgot its handles without destroying the
    // objects reads identically there; the arena check on these handles is what tells them apart.
    let handles: Vec<dereth_physics::PhysHandle> = scene.cell_static_handles(room).to_vec();
    assert!(
        !handles.is_empty(),
        "{room:?} bakes no object, so the arena check below is blind"
    );
    assert!(
        handles.iter().all(|h| scene
            .character
            .as_ref()
            .expect("a body")
            .world
            .get(*h)
            .is_some()),
        "a cell static's handle names no live physics object before the release"
    );
    let confined = run(&mut scene);
    assert!(
        confined < 12.0,
        "the body covered {confined:.1} m inside {room:?} with the walls resident, so this room \
         is a walkway rather than a room and the differential below would be vacuous"
    );
    // **The other half of the control.** `< 12.0` is an *absence*, and a body that never moved
    // (wedged in solid geometry at its own start point) satisfies it perfectly. The precondition
    // puts at least `WALL_AT_LEAST_M` of clear floor in front of the body, so a
    // living one covers some of it; this asserts that it did.
    assert!(
        confined > 0.25,
        "the body covered {confined:.2} m inside {room:?} with {wall:.2} m of clear floor in \
         front of it -- it did not walk, it was stuck, and 'it did not get out' is then a fact \
         about the insert rather than about the walls"
    );

    // A direct geometry positive control, because the released dynamic body does not step (see
    // below). Recollect obstacles from the live LandSource and rerun the same
    // sphere march that selected the station; this does not invoke dynamic collision stepping.
    let marched = body_spheres(&scene);
    let before_fate = straight_fate(&block_obstacles(&scene, home), &marched, inside, heading);
    let Fate::Blocked(before_d) = before_fate else {
        panic!(
            "the march that chose {room:?} answers {before_fate:?} with the block still \
             resident, so the negative arm after the release would prove nothing"
        )
    };
    assert!(
        (before_d - wall).abs() < 1e-6,
        "the same march now stops at {before_d:.4} m, not the {wall:.4} m the station was \
         chosen for: this instrument is not looking at what it looked at"
    );

    // Now walk the window away. The body goes with it, so the block it left is released.
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    for d in 1..=5 {
        go_to(&mut scene, &store, &mut gpu, block_at(hx + d, hy + d));
    }
    assert!(
        scene.draw.stats.cells_released > 0,
        "the walk released no cell"
    );
    assert!(
        land.env_cell(room).is_none(),
        "{room:?} is still resident after its block scrolled off the window"
    );
    assert!(
        scene.cell_static_handles(room).is_empty(),
        "{room:?} still holds {} bodies after its block was released",
        scene.cell_static_handles(room).len()
    );
    // Inspect the saved handles in PhysicsWorld, independently of the cell-static lookup table.
    // Forgetting table entries without destroying their objects passes the counter checks; zero
    // live saved handles rejects it.
    let leaked = handles
        .iter()
        .filter(|h| {
            scene
                .character
                .as_ref()
                .expect("a body")
                .world
                .get(**h)
                .is_some()
        })
        .count();
    assert_eq!(
        leaked, 0,
        "{leaked} of {} of {room:?}'s bodies are still live in PhysicsWorld after its block was released: the table forgot them and `world.destroy` was never called",
        handles.len()
    );

    // **The negative arm: the wall is gone from collision, not merely from a counter.** The same
    // march, the same body, the same point and heading, over whatever geometry the block still
    // has. `Blocked` here would mean a released cell whose BSP or whose baked statics are still
    // answering, which is the failure the walk below was written to catch.
    let after_fate = straight_fate(&block_obstacles(&scene, home), &marched, inside, heading);
    assert!(
        !matches!(after_fate, Fate::Blocked(_)),
        "the march from the same point on the same heading still answers {after_fate:?} after \
         {room:?}'s block was released: the wall is gone from the table, not from collision"
    );

    // A body returned into a released cell does not step at all. Object update has three early
    // guards, as retail does: parent present, current cell absent, or FROZEN_PS (0x01000000).
    // Each exits before stepping and clears ACTIVE_TS (0x80). The body's stored position still
    // names the released cell, but its current cell is None, so its update clock stays 0.0 and
    // it neither walks nor falls. (The normal window follows the player, so teleporting back into
    // an already released cell is an artificial station.)
    //
    // This is upstream of refused-transition handling: the transition is never reached. No
    // movement alone would not prove the guard (with no cell there is nothing to insert into
    // either), so the arena checks below also require a missing cell, an inactive state and an
    // unadvanced clock.
    let freed = run(&mut scene);
    let after = scene.character.as_ref().expect("a body").position();
    eprintln!(
        "the same 300-step script from the same point in {room:?}: {confined:.1} m with the \
         block resident, {freed:.4} m after whole-land-block release"
    );
    assert!(
        freed < 1e-3,
        "the body covered {freed:.4} m despite having no resident cell"
    );
    assert!(
        (after.frame.origin.z - inside.z).abs() < 1e-3,
        "the body's vertical displacement was {:.4} m despite having no resident cell",
        inside.z - after.frame.origin.z
    );
    {
        let c = scene.character.as_ref().expect("a body");
        let o = c
            .world
            .get(c.handle)
            .expect("the body is still in the arena");
        assert!(
            o.cell.is_none() && !o.parent.is_some() && !o.state.is_frozen(),
            "the body is not on the missing-cell-only branch -- cell {:?}, parent {}, frozen {} -- so the \
             two assertions above are measuring something else",
            o.cell,
            o.parent.is_some(),
            o.state.is_frozen()
        );
        assert!(
            !o.transient_state.is_active(),
            "ACTIVE_TS is still set ({:#05X}): the early-exit path did not clear it",
            o.transient_state.0
        );
        assert_eq!(
            o.update_time, 0.0,
            "the body's own clock advanced to {}, so update_object stepped it after all",
            o.update_time
        );
    }
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 4. The frame after the round trip
// ---------------------------------------------------------------------------------------------

/// Compare both release settings over the same round trip with LocalTime 0 and zero simulation
/// delta at each teleport. The default degradation multiplier is pinned. These controls make
/// captures comparable; they do not by themselves guarantee identical rebuilt frames.
///
/// Require identical initial home buffers across the independently built arms, plus more than
/// 10,000 changed pixels between home and a distant calibration station in each. Then require
/// equal counts of home pixels changed by the round trip. Returning frames themselves are not
/// compared byte-for-byte, and a zero round-trip change is not required. Equal changed-pixel
/// counts can still describe different changed locations or colors.
#[test]
fn the_round_trip_changes_the_frame_at_home_by_the_same_pixels_in_both_arms() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);

    let diff = |a: &[u8], b: &[u8]| -> usize {
        a.chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(p, q)| p != q)
            .count()
    };

    // Both arms, same walk, same stations, same clock.
    let arm = |release: bool, gpu: &mut Gpu| -> (usize, usize, Vec<u8>) {
        let mut scene = embodied(&store, gpu, release);
        go_to(&mut scene, &store, gpu, block_at(hx, hy));
        let before = shot(&mut scene, gpu);
        assert!(!before.is_empty(), "the capture is empty");
        for (dx, dy) in WALK.iter().skip(1) {
            go_to(&mut scene, &store, gpu, block_at(hx + dx, hy + dy));
        }
        assert_eq!(
            scene.draw.stats.cells_released > 0,
            release,
            "the arm's release counter disagrees with its switch"
        );
        let after = shot(&mut scene, gpu);
        // The calibration, taken inside each arm: a pair this differ is *known* to disagree
        // about. Without it, "0 px" and "my comparison is broken" are the same output.
        go_to(&mut scene, &store, gpu, block_at(hx + 5, hy + 5));
        let far = shot(&mut scene, gpu);
        let out = (diff(&before, &after), diff(&before, &far), before);
        scene.release_textures(gpu);
        out
    };

    let (off_delta, off_control, off_home) = arm(false, &mut gpu);
    let (on_delta, on_control, on_home) = arm(true, &mut gpu);
    let px = off_home.len() / 4;
    eprintln!(
        "of {px} px: the round trip changes the frame at home by {off_delta} px with the release OFF and {on_delta} px with it ON; the differ's calibration against a frame five blocks away reads {off_control} and {on_control} px in the two arms"
    );

    // Positive calibration in each arm; initial-home buffer equality is checked below.
    assert!(
        off_control > 10_000 && on_control > 10_000,
        "the differ cannot see a known difference"
    );

    // A returning block's rebake need not be pixel-stable even with release disabled, so require
    // the release switch to leave the change count equal, with identical starting
    // buffers. This does not assert identical final buffers or prove every restored surface
    // has the same identity; it adds a bounded image comparison to the residency controls.
    assert_eq!(
        off_delta, on_delta,
        "the round trip moves {off_delta} px with the release off and {on_delta} px with it on, so releasing the interiors changed what comes back"
    );
    assert_eq!(
        off_home, on_home,
        "the two arms do not even start from the same frame, so the comparison above is void"
    );
}

// ---------------------------------------------------------------------------------------------
// A departed block releases the objects in its cells
// ---------------------------------------------------------------------------------------------

mod interior_cell_objects {
    //! A landblock that leaves the streaming window releases the objects in its cells, not only their
    //! geometry: an object standing in a departed cell loses its cell, leaves the visible table (whose
    //! update keeps exactly `p.cell.is_some() && !p.is_static()`) and is queued for destruction, and
    //! is parked in the lost-cell table that cell initialization later hands back.
    //!
    //! Every station drives the production path: `WorldScene::update` → `release_block_interiors` →
    //! `WorldScene::sync_objects` → `ObjectStream::release_block_obj_cells` → the game world's
    //! `flush_cells` → `release_obj_cell`. Landblock release first visits the block's land cells and
    //! releases their objects, then its visible interior cells. The release filter refuses an object
    //! with the static bit or a parent, so each gate has its own object in the same frame:
    //!
    //! | object | why it is there | expected |
    //! |---|---|---|
    //! | `SUBJECT` — interior, dynamic, unparented | the subject | **leaves** visibility |
    //! | `STATIC_OBJ` — interior, `STATIC_PS` | the static-state gate | stays |
    //! | `HELD` — interior, parented to `HOLDER` | the parent gate | **not directly released**, but goes with its holder |
    //! | `OUTSIDE` — an outdoor cell of the same block | the land-cell release path | **leaves** |
    //! | `HOLDER` — interior, dynamic, unparented | the denominator: 3 of 6 leave, not 1 | **leaves** |
    //! | `CHILD_OF_SUBJECT` — parented to `SUBJECT` | the visibility-exit child loop | **not directly released**; queued with its parent |
    //!
    //! A held item goes with its holder (the leave-cell path recurses into the holder's children and
    //! visibility exit queues every child for destruction); what the parent gate buys it is that it is
    //! never handed to `leave_visibility` itself, so it is absent from the lost-cell list and from
    //! `objects_left_visibility`. Parent links need an asset-backed `ObjectStream::with_store` and a
    //! setup with a holding-location table, so the objects use setup `0x0200_0001`.
    //!
    //! Fixture: six synthetic creates in Holtburg's block, a body walked five blocks away, on the
    //! retail dats (fails without them) and a software device.

    use super::{block_at, store};
    use std::sync::Arc;

    use dereth_client::character::CharacterInput;
    use dereth_client::objects::ObjectStream;
    use dereth_client::world::{block_xy, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{
        CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
    };
    use dereth_render::device::Gpu;

    // =================================================================================================
    // The constants, as literals
    // =================================================================================================

    /// The `STATIC_PS` state — bit 0, the bit the release filter tests. Written as a literal and compared
    /// against the symbol below, because a test that reads a constant through the same symbol it
    /// writes it through cannot detect a wrong constant.
    const STATIC_PS: u32 = 0x0000_0001;

    /// One of the words the corpus actually carries (`0x0001001C`, a door opened), used wherever this
    /// file needs a realistic non-static word rather than an invented one.
    const DOOR_OPEN: u32 = 0x0001_001C;

    /// An interior cell index. `CellId::is_outdoor` is `0x01..=0x40`, so this is on the other side of
    /// that boundary by a wide margin and is the first interior-cell index.
    const INTERIOR: u16 = 0x0100;

    /// An outdoor cell index of the same block — the control that distinguishes land-cell release
    /// from the interior-cell half of the operation.
    const OUTDOOR: u16 = 0x0001;

    /// An installed setup record whose holding-location table really carries `RIGHT_HAND`. Parent
    /// attachment refuses outright when the holder has no part array, so an object with no setup id
    /// (or a stream with no assets, [`ObjectStream::new`]) could never be parented and the parent gate
    /// would have nothing to bite on.
    const SETUP: u32 = 0x0200_0001;

    /// `ParentLocation::RightHand`, a key that setup's table carries.
    const RIGHT_HAND: u32 = 1;

    const SUBJECT: u32 = 0x7625_0001;
    const STATIC_OBJ: u32 = 0x7625_0002;
    const HELD: u32 = 0x7625_0003;
    const HOLDER: u32 = 0x7625_0004;
    const OUTSIDE: u32 = 0x7625_0005;
    const CHILD_OF_SUBJECT: u32 = 0x7625_0006;

    #[test]
    fn the_static_bit_is_the_number_the_binary_tests() {
        assert_eq!(
            STATIC_PS,
            dereth_client_model::objects::STATIC_PS,
            "bit 0, the release filter's `state & 1` gate"
        );
        assert_eq!(
            DOOR_OPEN & STATIC_PS,
            0,
            "the subject word must not be static, or nothing moves"
        );
        assert!(
            CellId(u32::from(OUTDOOR)).is_outdoor(),
            "0x0001 is one of the 64 generated cells"
        );
        assert!(
            !CellId(u32::from(INTERIOR)).is_outdoor(),
            "0x0100 is an interior cell, which is the scope"
        );
    }

    // =================================================================================================
    // Harness
    // =================================================================================================

    /// A `0xF745 Item_CreateObject` for one object at one cell, with one state word and one parent.
    fn create_body(id: u32, state: u32, cell: CellId, parent: Option<u32>) -> Vec<u8> {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
        let bitfield =
            flags::POSITION | flags::SETUP | if parent.is_some() { flags::PARENT } else { 0 };
        dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: ObjectId(id),
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield,
                    state,
                    setup_id: Some(SETUP),
                    parent: parent.map(|p| (ObjectId(p), RIGHT_HAND)),
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: cell.0,
                        frame: dereth_protocol::types::Frame::default(),
                    }),
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode")
    }

    fn feed(s: &mut ObjectStream, body: Vec<u8>) {
        s.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(0.0),
        );
    }

    /// The six objects of the table in this file's header, all in one landblock.
    ///
    /// The stream is asset-backed ([`ObjectStream::with_store`]): the assetless constructor refuses
    /// every parent link, which would leave the two parent controls measuring nothing. With the dats
    /// open, parent attachment consults the holder setup's holding-location table and the link lands.
    ///
    /// The four unparented objects are handed their cell explicitly. The game world's `apply_physics_desc` method
    /// deliberately does **not** trust a decoded wire cell for an asset-backed object — *"a decoded
    /// wire cell is not a loaded object cell"* — it waits for the physical owner, which in production
    /// is `ObjectStream::publish_physics_cells` after `ObjectPhysics::sync`. This scene never runs
    /// `sync_physics` (`WorldScene::sync_objects` does not; `App::sync_objects` does), so the
    /// publication that says "this body stands in that loaded cell" is made here, through the same
    /// public seam. The two **held** objects are given nothing: attaching them to a holder moves them
    /// into the holder's cell, which is the whole point of the pair.
    fn populate(store: &Arc<RetailDatStore>, block: LandblockId) -> ObjectStream {
        let mut s = ObjectStream::with_store(Arc::clone(store));
        let inside = block.cell(INTERIOR);
        let outside = block.cell(OUTDOOR);
        for (id, state, cell) in [
            (SUBJECT, DOOR_OPEN, inside),
            (STATIC_OBJ, DOOR_OPEN | STATIC_PS, inside),
            (HOLDER, DOOR_OPEN, inside),
            (OUTSIDE, DOOR_OPEN, outside),
        ] {
            feed(&mut s, create_body(id, state, cell, None));
            s.world.publish_physics_cell(ObjectId(id), Some(cell));
        }
        feed(
            &mut s,
            create_body(CHILD_OF_SUBJECT, DOOR_OPEN, inside, Some(SUBJECT)),
        );
        feed(&mut s, create_body(HELD, DOOR_OPEN, inside, Some(HOLDER)));
        // Non-vacuity, at the point of construction: if either link were refused the two parent
        // controls below would be measuring nothing.
        for (child, holder) in [(CHILD_OF_SUBJECT, SUBJECT), (HELD, HOLDER)] {
            assert_eq!(
            s.world.physics_parent(ObjectId(child)),
            Some((ObjectId(holder), RIGHT_HAND)),
            "{child:#010X} must really be attached to {holder:#010X} -- parent attachment must land"
        );
        }
        s
    }

    fn cell_of(s: &ObjectStream, id: u32) -> Option<CellId> {
        s.world
            .physics(ObjectId(id))
            .expect("the create reached the game table")
            .cell
    }

    fn doomed(s: &ObjectStream, id: u32) -> bool {
        s.world.tables.doomed.contains_key(ObjectId(id))
    }

    fn lost_in(s: &ObjectStream, cell: CellId) -> Vec<ObjectId> {
        s.world
            .tables
            .lost_cells
            .get(cell)
            .map(|c| c.objects.clone())
            .unwrap_or_default()
    }

    /// Build the scene with a body in it, settled on its starting block.
    fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
        let cfg = SceneConfig {
            release_interiors: true,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        scene
            .attach_character(store, &region, gpu)
            .expect("the body is created");
        scene
    }

    /// Stand the body in the middle of one landblock, let the window catch up, and run the object
    /// synchronisation — which is the step that drains what the scroll released.
    fn go_to(
        scene: &mut WorldScene,
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        stream: &mut ObjectStream,
        block: LandblockId,
    ) {
        let land = Arc::clone(scene.character.as_ref().expect("a body").land());
        let mid = 96.0f32;
        let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
        {
            let c = scene.character.as_mut().expect("a body");
            c.teleport(Position::new(
                block.cell(1),
                Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
            ));
        }
        scene.follow_character_now();
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(0.0),
            0.0,
        );
        scene.stream(store, gpu).expect("the streamed blocks build");
        scene
            .sync_objects(store, gpu, stream)
            .expect("the objects synchronise");
    }

    // =================================================================================================
    // The acceptance: the production path releases the cell's objects, and only the right ones
    // =================================================================================================

    /// Behaviour: world.streaming.a-departed-block-releases-the-objects-in-its-interior-cells
    ///
    /// **A block that leaves the window takes its interior cells' objects out of visibility.**
    ///
    /// Driven end to end through `WorldScene`, so the thing under test is the wiring rather than
    /// `release_obj_cell` in isolation. The premise is asserted first — six objects in the game table,
    /// all cell-bound, and the block still resident — because a test whose success condition is
    /// "the object is no longer in a cell" scores a stream that never held it as a pass.
    #[test]
    fn a_departed_block_releases_the_objects_in_its_interior_cells() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);

        let home = LandblockId(DEFAULT_LANDBLOCK);
        let inside = home.cell(INTERIOR);
        let outside = home.cell(OUTDOOR);
        let mut stream = populate(&store, home);
        let mut scene = embodied(&store, &mut gpu);

        // The premise, at home: everything the server placed is in a cell, nothing is doomed, and the
        // lost-cell table is empty. Without this the assertions below are satisfied by a stream that
        // never received a create.
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        go_to(&mut scene, &store, &mut gpu, &mut stream, block_at(hx, hy));
        for id in [SUBJECT, CHILD_OF_SUBJECT, STATIC_OBJ, HOLDER, HELD] {
            assert_eq!(
                cell_of(&stream, id),
                Some(inside),
                "{id:#010X} starts in the interior cell"
            );
        }
        assert_eq!(
            cell_of(&stream, OUTSIDE),
            Some(outside),
            "and one starts outdoors"
        );
        assert_eq!(
            stream.stats.objects_left_visibility, 0,
            "nothing has left yet"
        );
        assert!(
            lost_in(&stream, inside).is_empty(),
            "and the lost-cell table starts empty"
        );
        stream.world.update_visible_object_list();
        assert!(
            stream.world.tables.visible.contains(&ObjectId(SUBJECT)),
            "the premise for the whole file: the subject IS in the visible table to begin with"
        );

        // Walk five blocks away, which is well outside the window in both axes.
        go_to(
            &mut scene,
            &store,
            &mut gpu,
            &mut stream,
            block_at(hx + 5, hy + 5),
        );
        assert!(
            scene.draw.stats.blocks_released > 0,
            "the premise: the scroll released blocks"
        );
        assert!(
            stream.stats.blocks_flushed > 0,
            "and the object half was reached at all"
        );

        // 1. The subject left.
        assert_eq!(
            cell_of(&stream, SUBJECT),
            None,
            "clearing the object's cell is the write the visibility sweep reads"
        );
        assert!(
            doomed(&stream, SUBJECT),
            "the object itself is added to the destruction queue"
        );
        assert!(
            doomed(&stream, CHILD_OF_SUBJECT),
            "the subject child is added to the destruction queue"
        );
        assert_eq!(
        lost_in(&stream, inside),
        vec![ObjectId(SUBJECT), ObjectId(HOLDER)],
        "lost-cell insertion parked both objects that left on their former cell, and only those two -- the three objects refused by the direct filter are absent"
    );
        stream.world.update_visible_object_list();
        assert!(
            !stream.world.tables.visible.contains(&ObjectId(SUBJECT)),
            "and the 1 Hz visibility sweep leaves it out"
        );

        // 2. The controls distinguish a static object that stays from parented children that are not
        //    directly released but still leave and enter destruction through their parent's child loop.
        assert_eq!(
            cell_of(&stream, STATIC_OBJ),
            Some(inside),
            "the static-state bit is refused"
        );
        // A parented object is refused by the direct release path, which never calls
        // `leave_visibility` on `HELD`: it is absent from the lost-cell list above and from
        // `objects_left_visibility` below. But it does not stay where it is, because its holder
        // leaves and drags it: the holder's leave-cell path recurses through its child list and clears
        // each child's cell, and visibility-exit preparation adds every child to the destruction queue.
        assert_eq!(
            cell_of(&stream, HELD),
            None,
            "the holder's leave-cell path recursed into its child list"
        );
        assert_eq!(
            stream.world.physics_parent(ObjectId(HELD)),
            Some((ObjectId(HOLDER), RIGHT_HAND)),
            "and nothing on the release path unparents it — the edge outlives the cell"
        );
        assert!(
        !lost_in(&stream, inside).contains(&ObjectId(HELD)),
        "the parent gate kept it out of direct visibility exit, so lost-cell insertion never saw it"
    );
        // Landblock object release is the wider release operation's first step and visits every land cell of the
        // block, so an object standing on the terrain leaves exactly as an interior one does.
        assert_eq!(
            cell_of(&stream, OUTSIDE),
            None,
            "landblock object release visits the block's land cells too"
        );
        assert!(
            doomed(&stream, OUTSIDE),
            "and it is scheduled for destruction like any other"
        );
        assert_eq!(
            lost_in(&stream, outside),
            vec![ObjectId(OUTSIDE)],
            "lost-cell insertion parked it on its outdoor cell rather than the interior one"
        );
        assert!(
            !doomed(&stream, STATIC_OBJ),
            "the STATIC_PS refusal is not scheduled for destruction: nothing it belongs to left"
        );
        assert!(
        doomed(&stream, HELD),
        "the parent-filtered object is still queued through its holder's child loop, exactly as CHILD_OF_SUBJECT is"
    );

        // 3. The denominator. `HOLDER` is unparented and dynamic, so it leaves with the subject; the
        //    count is what separates "the filter is right" from "the filter refused everything".
        assert_eq!(
            cell_of(&stream, HOLDER),
            None,
            "the holder itself is unparented and does leave"
        );
        assert_eq!(
            stream.stats.objects_left_visibility, 3,
            "exactly three of the six: SUBJECT, HOLDER and OUTSIDE"
        );
    }

    /// **`init_obj_cell` is the other end of the same table, and it hands the parked objects back.**
    ///
    /// Cell initialization pops the lost-cell record and re-enters visibility for
    /// everything in it. This asserts the pop against the state the walk above left behind, which is
    /// the only way to know that `goto_lost_cell` wrote something a reader can actually find: a
    /// `lost_cells` entry nothing can retrieve is the same silence as no entry at all.
    #[test]
    fn the_lost_cell_the_release_wrote_is_the_one_init_obj_cell_pops() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);

        let home = LandblockId(DEFAULT_LANDBLOCK);
        let inside = home.cell(INTERIOR);
        let mut stream = populate(&store, home);
        let mut scene = embodied(&store, &mut gpu);
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        go_to(&mut scene, &store, &mut gpu, &mut stream, block_at(hx, hy));
        go_to(
            &mut scene,
            &store,
            &mut gpu,
            &mut stream,
            block_at(hx + 5, hy + 5),
        );

        let parked = lost_in(&stream, inside);
        assert!(
            !parked.is_empty(),
            "the premise: the release parked something"
        );
        let popped = stream.world.init_obj_cell(inside);
        assert_eq!(
            popped, parked,
            "cell initialization hands back exactly what lost-cell insertion parked"
        );
        assert!(
            lost_in(&stream, inside).is_empty(),
            "the lost-cell lookup is empty after cell initialization"
        );
        assert!(
            stream.world.init_obj_cell(inside).is_empty(),
            "a second pop of the same cell answers empty rather than the same list twice"
        );
    }
}
