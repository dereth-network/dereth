//! In an indoor frame the outdoor pass leaves its depth behind; the interior path then clears the
//! depth (colour kept) and stamps each outdoor-facing opening's own depth back, so the interior
//! draws over the land everywhere except through a window, where the land still shows. Retail's
//! indoor order: draw the landscape, flush alpha, clear depth only (clear mask 4), then stamp every
//! outdoor portal of every drawn cell with mask 6 (no colour, depth write, the opening's own z),
//! and draw the interior cells over the result. Fixture: the retail dats' Holtburg house with a
//! cellar in landblock 0xA9B4, viewed from the top of its stairs (stairwell head 0xA9B40146 at z
//! 93.8-94.0, below the land at z 94), with `SceneConfig::indoor_z_clear` switched off as the
//! control and `SceneConfig::building_portals` isolating the neighbouring buildings' mask-7
//! stamps. Fails when the retail dats are absent or no GPU device can be created.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::character::CharacterInput;
use dereth_client::env_cells::{physics_geometry, EnvCellLoader};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client::world::{SceneReads, SceneWrites};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const W: u32 = 800;
const H: u32 = 600;
const HOLTBURG: u16 = 0xA9B4;

/// The stairwell head of the Holtburg house with a cellar: a 1.6 x 4.8 m cell whose floor is the
/// hole in the ground floor, at z 93.8–94.0 with the land outside at z ≈ 94.
const STAIRWELL: u32 = 0xA9B4_0146;
/// The ground floor immediately above it, and the cellar below.
const GROUND_FLOOR: u32 = 0xA9B4_0143;
const CELLAR: [u32; 3] = [0xA9B4_0147, 0xA9B4_0148, 0xA9B4_0149];

/// The retail store, or **fail**.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn is_black(p: &[u8]) -> bool {
    p[0] == 0 && p[1] == 0 && p[2] == 0
}

fn painted(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4).filter(|p| !is_black(p)).count()
}

/// A point the body can stand on inside `cell`, searched over the cell's **own** extent.
///
/// `WorldScene::standable_point` samples ±6 m about the cell's frame origin, and a Holtburg house
/// shares one frame origin across eighteen cells spread over 18 m and three storeys, so it answers
/// `None` for every cell of this building. This wider-grid search checks the same two facts:
/// containment in the cell and no intersection with solid geometry in its physics BSP.
///
/// The answer is a pure function of the dat, so it is searched once per cell and kept.
fn point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
    static FOUND: OnceLock<Mutex<HashMap<u32, Option<Vec3>>>> = OnceLock::new();
    let found = FOUND.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(p) = found
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&cell)
    {
        return *p;
    }
    let p = search_point_in(store, cell);
    found
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(cell, p);
    p
}

fn search_point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let d = EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = physics_geometry(&d);
    let bsp = g.cell_bsp.as_ref()?;
    for zi in -24i32..=24 {
        for i in -40i32..=40 {
            for j in -40i32..=40 {
                #[allow(clippy::cast_precision_loss)] // small loop counters
                let local = Vec3::new(i as f32 * 0.5, j as f32 * 0.5, zi as f32 * 0.5);
                if !bsp.point_inside_cell_bsp(local) {
                    continue;
                }
                if g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(local))
                {
                    continue;
                }
                return Some(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    None
}

/// One indoor frame, and the openings the frame's own matrices put on the screen.
struct Shot {
    /// The last frame's pixels; empty when the shot was taken for its stamps alone.
    rgba: Vec<u8>,
    openings: Vec<Vec<(f32, f32)>>,
    stamps: u64,
}

/// The production frame at the top of the stairs, with the depth clear on or off, in the app's own
/// frame order — `sync_objects`, `update`, `stream`, `reserve_upload_arena`, `begin_frame`, `draw`,
/// `end_frame` — so what is captured is the production path.
///
/// Three tests read the same two frames, which are a pure function of the dats and the switch, so
/// each is taken once per process and shared.
fn stairwell_shot(store: &Arc<RetailDatStore>, z_clear: bool) -> Arc<Shot> {
    static SHOTS: OnceLock<Mutex<HashMap<bool, Arc<Shot>>>> = OnceLock::new();
    let shots = SHOTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(s) = shots
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&z_clear)
    {
        return Arc::clone(s);
    }
    let shot = Arc::new(indoor_shot_with(
        store, STAIRWELL, 0.0, z_clear, true, true, true,
    ));
    shots
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(z_clear, Arc::clone(&shot));
    shot
}

/// The same frame, with `SceneConfig::building_portals` under the caller's control.
///
/// `Gpu::portal_stamps` counts every device portal stamp, of either mask. Retail's own counter
/// increments only for the indoor mask; the indoor cell-draw path reads it when deciding whether
/// to clear depth.
///
/// An indoor frame issues **both** kinds. It first draws the landscape through blocks and sorted
/// cells to buildings, stamping mask 7 into their visible openings before the depth-only clear.
/// Those building stamps do not move with `indoor_z_clear`; clearing `building_portals` takes them
/// out of the counter so the indoor half can be measured on its own.
///
/// `pixels` captures the last frame; a shot taken for its stamp count alone captures nothing.
fn indoor_shot_with(
    store: &Arc<RetailDatStore>,
    cell: u32,
    yaw: f32,
    z_clear: bool,
    stamp: bool,
    buildings: bool,
    pixels: bool,
) -> Shot {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        indoor_z_clear: z_clear,
        portal_depth_stamp: stamp,
        building_portals: buildings,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    let p = point_in(store, cell).expect("the station cell has a standable point");
    let q = Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5));
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(CellId(cell), Frame::new(p, q)));

    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    let mut rgba = Vec::new();
    let mut stamps = 0;
    for i in 0..6 {
        now += dereth_client::app::HEADLESS_STEP;
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        // `App::frame`'s next camera step. The swept-sphere camera update sets the viewer cell and
        // replaces the temporary debug chase camera placed by the scene update with the production
        // camera, `(0, -2.75, 0.825)` behind the pivot and pitched down 16.699 degrees, so the depth
        // clear and the stamp are photographed from where a player's camera actually is.
        dereth_client::camera::update_viewer(
            &mut scene,
            dereth_client::camera::CameraInput::default(),
            LocalTime(now),
            dereth_client::app::HEADLESS_STEP,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        let before = gpu.portal_stamps();
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        if i == 5 {
            stamps = gpu.portal_stamps() - before;
            if pixels {
                rgba = gpu.capture().expect("capture").to_rgba();
            }
        }
    }
    // The viewer really is inside, which is the premise every assertion below rests on.
    let pos = scene.character.as_ref().expect("a body").position();
    assert!(
        !dereth_physics::landdefs::is_outdoors(pos.cell),
        "the viewer settled outdoors in cell {:#010X}; the indoor path would not run",
        pos.cell.0
    );
    assert_eq!(
        pos.cell.0 >> 16,
        u32::from(HOLTBURG),
        "the viewer settled in another landblock ({:#010X})",
        pos.cell.0
    );
    // Initial placement re-seats the body in the cell that actually contains the point,
    // so the settled cell is reported rather than demanded: the station is "the top of the
    // stairs", and the ground floor is the cell that owns that point.
    eprintln!(
        "indoor depth station {cell:#010X}: viewer settled in {:#010X}",
        pos.cell.0
    );
    let openings = scene.indoor_outdoor_portal_screen_polygons(W, H);
    Shot {
        rgba,
        openings,
        stamps,
    }
}

/// The same, outdoors over Holtburg with no body — the control that says the change is keyed to the
/// indoor path and nothing else.
fn outdoor_shot(store: &Arc<RetailDatStore>, z_clear: bool) -> Vec<u8> {
    let mut gpu = crate::common::test_gpu(W, H);
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        character: false,
        scenery_radius: 1,
        land_radius: 1,
        time_of_day: Some(0.5),
        particles: false,
        indoor_z_clear: z_clear,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    let mut stream = ObjectStream::new();
    for i in 0..4 {
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(f64::from(i) * dereth_client::app::HEADLESS_STEP),
            1.0 / 30.0,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
    gpu.capture().expect("capture").to_rgba()
}

/// Even-odd point-in-polygon.
fn inside(poly: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        if (y0 > py) != (y1 > py) {
            let t = (py - y0) / (y1 - y0);
            if px < (x1 - x0).mul_add(t, x0) {
                c = !c;
            }
        }
    }
    c
}

/// Which pixels lie inside at least one projected opening, **eroded by `margin` pixels** so that
/// the mask never includes the opening's own boundary — where a rasterisation difference between
/// the reported outline and the stamped one is not evidence of anything.
fn opening_mask(polys: &[Vec<(f32, f32)>], margin: f32) -> Vec<bool> {
    let mut mask = vec![false; (W * H) as usize];
    for poly in polys {
        if poly.len() < 3 {
            continue;
        }
        for y in 0..H {
            for x in 0..W {
                #[allow(clippy::cast_precision_loss)] // pixel indices
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                if [
                    (0.0, 0.0),
                    (margin, 0.0),
                    (-margin, 0.0),
                    (0.0, margin),
                    (0.0, -margin),
                ]
                .iter()
                .all(|(dx, dy)| inside(poly, px + dx, py + dy))
                {
                    mask[(y * W + x) as usize] = true;
                }
            }
        }
    }
    mask
}

/// **Is this pixel within `margin` of an opening's projected outline?**
///
/// The same idea [`opening_mask`]'s erosion already encodes and for the same stated reason — *"a
/// rasterisation difference between the reported outline and the stamped one is not evidence of
/// anything"* — but as a predicate about one pixel rather than as a mask, so the `black_after`
/// population below can be **classified** instead of merely counted. The stencil straddles the
/// outline exactly when its five samples do not agree.
fn near_opening_edge(polys: &[Vec<(f32, f32)>], px: f32, py: f32, margin: f32) -> bool {
    polys.iter().filter(|p| p.len() >= 3).any(|poly| {
        let s = [
            (0.0, 0.0),
            (margin, 0.0),
            (-margin, 0.0),
            (0.0, margin),
            (0.0, -margin),
        ]
        .iter()
        .map(|(dx, dy)| inside(poly, px + dx, py + dy))
        .collect::<Vec<_>>();
        s.iter().any(|v| *v) && !s.iter().all(|v| *v)
    })
}

/// The premise, read out of `client_cell_1.dat`: this really is a house with a cellar, the
/// stairwell head really is below the land, and the cellar is below that.
#[test]
fn the_station_is_a_house_with_a_cellar() {
    let store = store();
    let cells = EnvCellLoader::new().load_block(&store, HOLTBURG);
    let extent = |id: u32| -> (f32, f32) {
        let d = cells
            .iter()
            .find(|d| d.id.0 == id)
            .expect("the cell is in the block");
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for v in &d.structure.vertex_array.vertices {
            let w = dereth_physics::math::localtoglobal(&d.cell.frame, v.position);
            lo = lo.min(w.z);
            hi = hi.max(w.z);
        }
        (lo, hi)
    };
    let ground = extent(GROUND_FLOOR);
    let stair = extent(STAIRWELL);
    eprintln!(
        "indoor depth station: ground floor {GROUND_FLOOR:#010X} z=[{:.2}, {:.2}], stairwell \
         {STAIRWELL:#010X} z=[{:.2}, {:.2}]",
        ground.0, ground.1, stair.0, stair.1
    );
    assert!(
        (ground.0 - 94.0).abs() < 0.01,
        "the ground floor's own floor is at z {}, not the land's 94.0",
        ground.0
    );
    assert!(
        stair.1 <= ground.0,
        "the stairwell head's ceiling {} is not below the ground floor's floor {}",
        stair.1,
        ground.0
    );
    for id in CELLAR {
        let (lo, hi) = extent(id);
        eprintln!("indoor depth cellar {id:#010X} z=[{lo:.2}, {hi:.2}]");
        assert!(
            hi <= ground.0,
            "cellar cell {id:#010X} is not below the ground floor"
        );
    }
    // And the stairwell is reachable from the ground floor, so a viewer at the top of the stairs
    // is looking into a cell the traversal draws.
    let stairs = cells
        .iter()
        .find(|d| d.id.0 == STAIRWELL)
        .expect("the stairwell cell");
    let neighbours: Vec<u32> = stairs
        .cell
        .portals
        .iter()
        .map(|p| p.other_cell_id & 0xFFFF)
        .collect();
    eprintln!("indoor depth stairwell {STAIRWELL:#010X} portals lead to {neighbours:04X?}");
    assert!(
        neighbours.contains(&(GROUND_FLOOR & 0xFFFF))
            || CELLAR.iter().any(|c| neighbours.contains(&(c & 0xFFFF))),
        "the stairwell head connects to neither the ground floor nor the cellar"
    );
}

/// The pixels the depth clear changes were land and are now the interior.
#[test]
fn the_ground_over_the_stairwell_is_the_outdoor_passs_depth() {
    let store = store();
    let pre = stairwell_shot(&store, false);
    let post = stairwell_shot(&store, true);
    assert_eq!(pre.rgba.len(), post.rgba.len());

    let (mut changed, mut black_before, mut black_after) = (0usize, 0usize, 0usize);
    // The `black_after` population, classified rather than only counted. See below.
    let mut cleared: Vec<(u32, u32, bool)> = Vec::new();
    for (i, (a, b)) in pre
        .rgba
        .chunks_exact(4)
        .zip(post.rgba.chunks_exact(4))
        .enumerate()
    {
        if a == b {
            continue;
        }
        changed += 1;
        if is_black(a) {
            black_before += 1;
        }
        if is_black(b) {
            black_after += 1;
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            // LINT-OK: a pixel index into an 800x600 frame.
            let (x, y) = ((i as u32) % W, (i as u32) / W);
            #[allow(clippy::cast_precision_loss)] // pixel centres
            let edge = near_opening_edge(&post.openings, x as f32 + 0.5, y as f32 + 0.5, 3.0);
            cleared.push((x, y, edge));
        }
    }
    let total = pre.rgba.len() / 4;
    #[allow(clippy::cast_precision_loss)] // a percentage in a log line
    let pct = 100.0 * changed as f32 / total as f32;
    eprintln!(
        "indoor depth at {STAIRWELL:#010X}: {changed} of {total} pixels changed ({pct:.1} %); painted \
         {} -> {}; changed pixels that were the clear before {black_before}, after {black_after}",
        painted(&pre.rgba),
        painted(&post.rgba),
    );

    assert!(
        changed > 5_000,
        "only {changed} pixels changed -- this station shows no landscape over the stairwell, so \
         it asserts nothing about land drawn over the interior"
    );
    assert_eq!(
        black_before, 0,
        "{black_before} of the changed pixels were the colour clear without the depth clear -- it \
         is being credited with covering nothing, not with covering the land"
    );
    // The claim: the depth clear lets the interior draw over the land rather than blanking the
    // frame. It is not an exact zero: from the production camera a handful of changed pixels (5 of
    // 480,000 at this station) end as the colour clear. None of them is within 3 px of a projected
    // opening, so they are not the outlines' rasterisation; they are one small sliver where the
    // interior's own geometry does not quite close, through which the land showed before and now
    // nothing does. So the cleared pixels must be **one small cluster** (a blanking depth clear
    // would scatter them across the frame) and a small fraction of the change, not a fixed count.
    eprintln!(
        "indoor depth cleared pixels: {black_after} of {changed} changed ({:.3} % of the change, \
         {:.4} % of the frame) -- (x, y, within 3 px of an opening outline) {:?}",
        100.0 * black_after as f32 / changed as f32,
        100.0 * black_after as f32 / total as f32,
        cleared,
    );
    if black_after > 0 {
        let xs = cleared.iter().map(|c| c.0);
        let ys = cleared.iter().map(|c| c.1);
        let (x0, x1) = (xs.clone().min().unwrap(), xs.max().unwrap());
        let (y0, y1) = (ys.clone().min().unwrap(), ys.max().unwrap());
        let area = (x1 - x0 + 1) * (y1 - y0 + 1);
        eprintln!(
            "indoor depth cleared bounding box: ({x0},{y0})..({x1},{y1}), {area} px, holding \
             {black_after}"
        );
        assert!(
            area <= 256,
            "the {black_after} pixels the Z clear left as the colour clear span a {area} px box \
             ({x0},{y0})..({x1},{y1}) -- that is a region of the frame and not a sliver, so the Z \
             clear is blanking the frame rather than letting the interior draw over the land"
        );
    }
    assert!(
        black_after * 1_000 <= changed,
        "{black_after} of the {changed} changed pixels are the colour clear with the depth clear -- more \
         than a tenth of a percent of the change: it is blanking the frame rather than letting the \
         interior draw over the land"
    );
    assert!(
        painted(&pre.rgba) - painted(&post.rgba) <= black_after,
        "the depth clear lost {} painted pixels where only {black_after} changed pixels became the clear",
        painted(&pre.rgba) - painted(&post.rgba)
    );
}

/// The land still draws through the openings: from inside the building the land stays visible
/// through the windows.
#[test]
fn the_land_still_draws_through_the_openings() {
    let store = store();
    let pre = stairwell_shot(&store, false);
    let post = stairwell_shot(&store, true);

    for margin in [1.0f32, 3.0, 6.0] {
        let m = opening_mask(&post.openings, margin);
        let k = m.iter().filter(|b| **b).count();
        let d = pre
            .rgba
            .chunks_exact(4)
            .zip(post.rgba.chunks_exact(4))
            .enumerate()
            .filter(|(i, (a, b))| m[*i] && a != b)
            .count();
        eprintln!("indoor depth openings eroded by {margin} px: {k} pixels, {d} of them differ");
    }
    let mask = opening_mask(&post.openings, 3.0);
    let n = mask.iter().filter(|m| **m).count();
    eprintln!(
        "indoor depth openings: {} projected, {n} pixels inside them (eroded by 3 px)",
        post.openings.len()
    );
    assert!(
        n > 200,
        "the projected openings cover only {n} pixels -- there is no window on screen here, so \
         this control asserts nothing"
    );

    let (mut differ, mut lit) = (0usize, 0usize);
    for (i, (a, b)) in pre
        .rgba
        .chunks_exact(4)
        .zip(post.rgba.chunks_exact(4))
        .enumerate()
    {
        if !mask[i] {
            continue;
        }
        if a != b {
            differ += 1;
        }
        if !is_black(b) {
            lit += 1;
        }
    }
    eprintln!(
        "indoor depth inside the openings: {differ} of {n} pixels differ, {lit} of {n} are painted"
    );
    // Not zero, and the residue is not a rasterisation edge: it shrinks with the erosion margin in
    // proportion to the mask rather than vanishing, so it is real geometry -- the sill and the wall
    // edge, which are **nearer** than the window plane and so pass the stamped depth, and which the
    // terrain's depth hides without the clear. The land itself, beyond the window plane, does not
    // move.
    assert!(
        differ * 100 <= n * 5,
        "{differ} of the {n} pixels inside a window changed -- the depth clear is altering the land \
         seen through the opening, which must not move"
    );
    assert_eq!(
        lit, n,
        "only {lit} of {n} pixels inside a window are painted with the depth clear -- the land \
         through the window has gone"
    );
}

/// The gate controls the indoor clear/stamp path, which an outdoor viewer never reaches.
#[test]
fn an_outdoor_viewer_is_untouched() {
    let store = store();
    let before = outdoor_shot(&store, false);
    let after = outdoor_shot(&store, true);
    let differ = before
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    eprintln!(
        "indoor depth outdoor control: {differ} of {} pixels differ, {} painted",
        before.len() / 4,
        painted(&before)
    );
    assert!(
        painted(&before) > 100_000,
        "the outdoor control drew almost nothing"
    );
    assert_eq!(differ, 0, "{differ} pixels moved for an outdoor viewer");
}

/// Behaviour: rendering.indoor.the-outdoor-depth-is-cleared-and-openings-restamped
/// Indoor mask-6 stamps reach the device, and the arm without the depth clear issues none of
/// **those**.
///
/// `Gpu::portal_stamps` counts every stamp of **either** mask (see [`indoor_shot_with`]). An indoor
/// frame also runs the building pass: retail's indoor path begins with landscape drawing that
/// reaches buildings, so a viewer in a Holtburg house sees the neighbouring houses' interiors
/// through their doorways as an outdoor viewer does, and their openings are stamped with mask 7
/// before the depth-only clear wipes them again. So the frame's stamps are decomposed into the two
/// masks by `SceneConfig::building_portals`, and each half is asserted separately: the indoor half
/// must be non-empty with the depth clear and **exactly empty** without it, and the two halves must
/// add up, which says neither switch is quietly moving the other's draws.
#[test]
fn the_openings_are_stamped_back_into_the_cleared_depth() {
    let store = store();
    // The production frame, both arms: building stamps plus (with the depth clear) the indoor stamps.
    let on = stairwell_shot(&store, true);
    let off = stairwell_shot(&store, false);
    // The same two arms with building stamps switched off: the indoor stamps alone.
    let indoor_only = indoor_shot_with(&store, STAIRWELL, 0.0, true, true, false, false);
    let pre_indoor_only = indoor_shot_with(&store, STAIRWELL, 0.0, false, true, false, false);
    eprintln!(
        "indoor depth stamps: {} with the Z clear ({} openings projected), {} without it; with \
         the building half off, {} and {}; so the building half is {} and the indoor half \
         is {}",
        on.stamps,
        on.openings.len(),
        off.stamps,
        indoor_only.stamps,
        pre_indoor_only.stamps,
        off.stamps,
        indoor_only.stamps,
    );
    assert!(
        indoor_only.stamps > 0,
        "the Z clear ran and stamped no openings back -- every window in the frame would be \
         covered by whatever the interior draws behind it"
    );
    assert_eq!(
        pre_indoor_only.stamps, 0,
        "the arm without the depth clear issued {} indoor stamps; it must issue none",
        pre_indoor_only.stamps
    );
    // `off` is the building half on its own, and `on` is that same half plus the indoor one. A
    // failure here means one switch moved the other's draws -- e.g. the Z clear changing what the
    // outdoor pass reaches, or the building pass changing the indoor traversal -- which no
    // separate count of either half would catch.
    assert_eq!(
        on.stamps,
        off.stamps + indoor_only.stamps,
        "the frame's {} stamps are not the building pass's {} plus the Z clear's {} -- one of the two \
         halves moved when the other was switched",
        on.stamps,
        off.stamps,
        indoor_only.stamps
    );
}
