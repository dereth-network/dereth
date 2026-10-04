//! The cell index `cell_draw_order` orders each landblock around is read from the viewpoint's
//! cell **id**, not from its position. Viewpoint update decodes the one id it is handed to global
//! land coordinates and cuts them to block-local `(x & 7, y & 7)`, so the block half and the cell
//! half are one number read twice and cannot disagree; a zero id leaves the last index standing.
//! A position-derived index, `floor(p / 24)` clamped to `0..=7`, agrees only while the viewpoint
//! is inside the centre block; outside it the clamp names a cell the viewer is not in. Indoors the
//! interior id is first converted to the land cell it sees, because `gid_to_lcoord` rejects it.
//! Stations: every outdoor cell and standable room of the home block `0xA9B4`, a block crossing,
//! the load-time flycam 67.2 m south, an off-block room id, an off-world viewpoint and two runs.
//! Fixture: the retail dats on a software device; fails without the dats or a device.
//!
//! Sections of this module:
//! * `cell_source`: the viewer's cell id is its own position read through the container rule.
//! * `load_time`: neither load-time arm re-centres, and a re-centre could not change the index.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::SceneWrites;
use std::sync::Arc;

use dereth_client::camera::CameraInput;
use dereth_client::character::CharacterInput;
use dereth_client::world::{block_xy, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_dat::RetailDatStore;
use dereth_physics::LandSource;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;

/// `BLOCK_LENGTH`.
const BLOCK: f32 = 192.0;
/// `CELL_SIZE` — the side of one of a landblock's 8x8 cells.
const CELL: f32 = 24.0;

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above.
    LandblockId(((x as u16) << 8) | (y as u16))
}

fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let mut scene = WorldScene::load(store, gpu, SceneConfig::default()).expect("the scene loads");
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// One frame's simulation step, in `App::frame`'s own order minus the sweep, which is what puts
/// `recenter` and `update_viewer_cell` in the same call.
fn sim(scene: &mut WorldScene, now: f64) {
    scene.update(
        CameraInput::default(),
        CharacterInput::default(),
        LocalTime(now),
        0.0,
    );
}

/// Stand the body — and therefore the viewpoint — at `origin` of `cell`. An **outdoor** cell is
/// normalized from the placement during `Character::teleport`, just as player creation does; an
/// interior one goes through the indoor placement arm described at [`standable_rooms`].
fn stand(scene: &mut WorldScene, cell: CellId, origin: Vec3) {
    let c = scene.character.as_mut().expect("a body");
    c.teleport(Position::new(cell, Frame::new(origin, Quat::IDENTITY)));
}

// ---------------------------------------------------------------------------------------------
// The two derivations, both built here rather than read out of the code under test
// ---------------------------------------------------------------------------------------------

/// The viewpoint in the renderer's viewer-block-relative space — `WorldScene::viewpoint`, rebuilt
/// from its two public parts so this file's oracles are not the accessor under test.
fn viewpoint(scene: &WorldScene) -> Vec3 {
    match scene.character.as_ref().filter(|c| c.camera.attached()) {
        Some(c) => c.render_frame_of(c.camera.viewer).origin,
        None => scene.camera.position,
    }
}

/// The position-derived SqCoord: the viewpoint's *position*, divided by 24 and clamped to
/// `0..=7`. The comparison arm every station measures the id-derived index against.
fn clamped_from_position(scene: &WorldScene) -> (u8, u8) {
    let p = viewpoint(scene);
    let cx = dereth_primitives::num::floor_to_i32(p.x / CELL).clamp(0, 7);
    let cy = dereth_primitives::num::floor_to_i32(p.y / CELL).clamp(0, 7);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both clamped to 0..=7 on the two lines above. Not a float conversion.
    (cx as u8, cy as u8)
}

/// The id-derived SqCoord: mask the global land coordinates decoded from the id handed to
/// viewpoint update. The id comes from the viewer cell outdoors and from
/// `get_outside_cell_id(&viewer)` indoors.
///
/// `None` is the client's null viewer — no viewpoint this frame, and draw-order calculation
/// returns without touching anything.
fn masked_from_id(scene: &WorldScene) -> Option<(u8, u8)> {
    use dereth_physics::landdefs as ld;
    let id = match scene.character.as_ref().filter(|c| c.camera.attached()) {
        Some(c) => {
            let pos = c.camera.viewer;
            if ld::is_outdoors(pos.cell) {
                pos.cell
            } else {
                ld::get_outside_cell_id(pos.cell, pos.frame.origin)
            }
        }
        None => {
            let (bx, by) = scene.viewer_block()?;
            ld::get_outside_cell_id(ld::lcoord_to_gid(bx * 8, by * 8), scene.camera.position)
        }
    };
    let (x, y) = ld::gid_to_lcoord(id)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: `& 7` bounds both to 0..=7. Not a float conversion.
    Some(((x & 7) as u8, (y & 7) as u8))
}

/// The first interior cell of the home landblock the loaded scene actually has geometry for.
///
/// This does not load another room; it asks only what the landblock path already prefetched — the
/// same question `world::interiors` asks.
fn a_resident_room(scene: &WorldScene) -> Option<CellId> {
    let land = Arc::clone(scene.character.as_ref()?.land());
    (0x100u32..0x200).find_map(|idx| {
        let id = CellId((u32::from(DEFAULT_LANDBLOCK) << 16) | idx);
        LandSource::env_cell(land.as_ref(), id).map(|_| id)
    })
}

/// Every resident room in the queried `0x100..0x200` interior-ID range *with a point that fits
/// the placement probe*, and that point — `WorldScene::standable_point`, which searches the cell
/// volume while excluding its masonry. This establishes a usable point, not that every possible
/// full-body pose remains confined to the room.
///
/// A body stood at arbitrary landblock-local metres under a room's id is not a state the client
/// holds, because `Character::teleport` commits the placement through physics. The indoor
/// placement arm is
///
/// ```text
///   find the interior cell; if absent, return no cell
///   transform the sphere centre using the placement frame before the child-cell lookup
///   find the visible child cell containing that point
///   if none contains it and the room is not marked as seeing outside, return no cell
///   otherwise adjust the position to outside and use the visible land cell it names
/// ```
///
/// so an origin outside the room resolves **out of the room**, onto the land cell the building
/// stands in. A station that wants the viewer indoors has to put the body where the room is, and
/// this is what says where that is.
fn standable_rooms(scene: &WorldScene) -> Vec<(CellId, Vec3)> {
    let land = match scene.character.as_ref() {
        Some(c) => Arc::clone(c.land()),
        None => return Vec::new(),
    };
    (0x100u32..0x200)
        .filter_map(|idx| {
            let id = CellId((u32::from(DEFAULT_LANDBLOCK) << 16) | idx);
            LandSource::env_cell(land.as_ref(), id)?;
            scene.standable_point(id).map(|p| (id, p))
        })
        .collect()
}

/// `floor(p / 24) & 7` of a **landblock-local** metre coordinate — the block-local square used for
/// draw order, derived straight from the two floats
/// instead of through `get_outside_cell_id` + `gid_to_lcoord`. A third, independent reading, so
/// that [`masked_from_id`] and the scene are not the only two answers in the room.
fn sq_of_local(p: Vec3) -> (u8, u8) {
    let cx = dereth_primitives::num::floor_to_i32(p.x / CELL) & 7;
    let cy = dereth_primitives::num::floor_to_i32(p.y / CELL) & 7;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: `& 7` bounds both to 0..=7. Not a float conversion.
    (cx as u8, cy as u8)
}

// ---------------------------------------------------------------------------------------------
// 1. Inside a block: the premise, and where the clamp is inert
// ---------------------------------------------------------------------------------------------

/// Behaviour: world.viewpoint.the-draw-order-cell-is-read-from-the-viewers-cell-id
///
/// **The index steps at every one of the seven interior 24 m boundaries, on both axes.**
///
/// This asserts the premise as well as the difference: a derivation that returned a constant, or
/// that never moved off `(0, 0)`, would satisfy any single-station equality. All 64 cells of the
/// home block are visited and all 64
/// distinct SqCoords are required.
///
/// The clamp is **inert** at every one of these stations, and that is asserted too rather than
/// left implied: inside the centre block `clamp` and `& 7` are the same function, so the two
/// derivations must agree here. If they ever stop agreeing, the fixture has drifted out of the
/// block; the off-block stations below are where the two part.
#[test]
fn the_cell_index_tracks_every_24_m_boundary_inside_a_block() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home = block_at(hx, hy);
    let mut scene = embodied(&store, &mut gpu);

    // Settle the window on the home block before anything is measured.
    stand(&mut scene, home.cell(1), Vec3::new(96.0, 96.0, 0.0));
    for i in 1..=10 {
        sim(&mut scene, f64::from(i) / 30.0);
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut steps_x = 0usize;
    let mut steps_y = 0usize;

    for i in 0..8i32 {
        let mut last_on_row: Option<(u8, u8)> = None;
        for j in 0..8i32 {
            // Just *inside* the far side of each cell: 0.1 m past the boundary that opens it, so
            // the station is a boundary crossing and not a cell centre.
            #[allow(clippy::cast_precision_loss)] // 0..8
            let (x, y) = (i as f32 * CELL + 0.1, j as f32 * CELL + 0.1);
            stand(&mut scene, home.cell(1), Vec3::new(x, y, 0.0));
            sim(&mut scene, 1.0 + f64::from(i * 8 + j) / 30.0);

            let got = scene.viewer_draw_cell();
            let want = masked_from_id(&scene).expect("the viewpoint has a cell id");
            let before = clamped_from_position(&scene);

            assert_eq!(
                got, want,
                "at ({x}, {y}) of {home:?} the scene uses draw-order square {got:?}; the viewer's own cell id says {want:?}"
            );
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: i and j are 0..8.
            let expect = (i as u8, j as u8);
            assert_eq!(
                got, expect,
                "at ({x}, {y}) of {home:?} — 0.1 m inside cell {expect:?} — the index is {got:?}"
            );
            // The clamp is inert inside the block, and the two derivations must agree here.
            assert_eq!(
                before, got,
                "at ({x}, {y}) the clamp answered {before:?} and the mask {got:?}: this station is \
                 supposed to be inside the centre block, where they are the same function"
            );

            seen.insert(got);
            if let Some(p) = last_on_row {
                if p.1 != got.1 {
                    steps_y += 1;
                }
            }
            last_on_row = Some(got);
        }
        if i > 0 {
            steps_x += 1;
        }
    }
    scene.release_textures(&mut gpu);

    eprintln!(
        "inside a block: {} distinct SqCoord(s) over 64 stations; the index stepped at {} \
         of 7 boundaries on x and {} of {} on y",
        seen.len(),
        steps_x,
        steps_y,
        7 * 8
    );
    assert_eq!(
        seen.len(),
        64,
        "the index did not name all 64 cells of the block"
    );
    assert_eq!(
        steps_x, 7,
        "the index did not step at all seven interior x boundaries"
    );
    assert_eq!(
        steps_y,
        7 * 8,
        "the index did not step at every interior y boundary"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Indoors — the arm that has to convert the id at all
// ---------------------------------------------------------------------------------------------

/// **Indoors the id is an interior one, `gid_to_lcoord` rejects it, and the client converts.**
///
/// The stations are the home block's real resident rooms, one per room, with the body at a point
/// **inside the room** ([`standable_rooms`]). This is the state the normal-render indoor arm
/// handles: the viewer is in a room, and landscape draw ordering needs the *land* cell under it.
///
/// The stations are the rooms the block has, not an 8x8 grid: a point no child cell of a room
/// contains resolves the body out of the room and onto the land cell the building stands in, as
/// retail does ([`standable_rooms`]). The claim is that the index is a live reading of the
/// viewer's cell **id**, it moves with the room, and it is `& 7` and not a clamp. Three readings
/// are compared at every station — the
/// scene's [`WorldScene::viewer_draw_cell`], this file's [`masked_from_id`]
/// (`get_outside_cell_id` + `gid_to_lcoord` + `& 7`) and [`sq_of_local`]
/// (`floor(origin / 24) & 7` straight off the viewer's two landblock-local floats).
#[test]
fn the_cell_index_tracks_the_land_cell_under_every_room_of_the_block() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home = block_at(hx, hy);
    let mut scene = embodied(&store, &mut gpu);

    stand(&mut scene, home.cell(1), Vec3::new(96.0, 96.0, 0.0));
    for i in 1..=10 {
        sim(&mut scene, f64::from(i) / 30.0);
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");
    }

    let rooms = standable_rooms(&scene);
    assert!(
        !rooms.is_empty(),
        "no interior cell of {home:?} is resident and standable, so the indoor half has no subject"
    );
    // The premise for the whole file's indoor half: the client's own `gid_to_lcoord` refuses an
    // interior id, so a build that did not convert would be handing draw-order calculation an
    // id it rejects. Asserted over every station's id, not over one.
    for (room, _) in &rooms {
        assert!(
            !dereth_physics::landdefs::is_outdoors(*room),
            "{room:?} is not an interior cell, so this station measures the outdoor arm"
        );
        assert!(
            dereth_physics::landdefs::gid_to_lcoord(*room).is_none(),
            "`gid_to_lcoord` accepted the interior id {room:?}; the indoor conversion arm exists because it does not, and without that this station proves nothing"
        );
    }

    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut indoors = 0usize;
    let mut left_the_room = Vec::new();
    let mut differ = 0usize;

    for (n, (room, inside)) in rooms.iter().enumerate() {
        stand(&mut scene, *room, *inside);
        sim(&mut scene, 2.0 + n as f64 / 30.0);

        let c = scene.character.as_ref().expect("a body");
        let viewer = c.camera.viewer;
        if dereth_physics::landdefs::is_outdoors(viewer.cell) {
            // The camera is a 0.3 m sphere swept 3 m back from the pivot; out of a doorway it can
            // legitimately end in the land cell outside. Counted and named, never silently
            // skipped — the assertion
            // below requires most of the block's rooms to keep the viewer indoors.
            left_the_room.push((*room, viewer.cell));
            continue;
        }
        indoors += 1;

        let got = scene.viewer_draw_cell();
        let want = masked_from_id(&scene).expect("the room converts to a land cell");
        let by_hand = sq_of_local(viewer.frame.origin);
        let before = clamped_from_position(&scene);

        assert_eq!(
            got, want,
            "in {room:?} at {inside:?} the scene uses draw-order square {got:?}; `get_outside_cell_id` says {want:?}"
        );
        assert_eq!(
            got, by_hand,
            "in {room:?} the viewer's landblock-local origin {:?} is `floor / 24 & 7` = \
             {by_hand:?}, and the scene says {got:?}",
            viewer.frame.origin
        );
        // The clamp is inert while the viewpoint is inside the block it is measured against, and
        // that is the mechanism the indoor off-block station rests on — asserted, not assumed.
        if before != got {
            differ += 1;
        }

        if rows.len() < 12 {
            rows.push(format!(
                "{room:?} at ({:6.1},{:6.1}) before {before:?} after {got:?}",
                viewer.frame.origin.x, viewer.frame.origin.y
            ));
        }
        seen.insert(got);
    }
    scene.release_textures(&mut gpu);

    eprintln!(
        "indoors: first {} of {indoors} indoor draw-order station(s) —\n  {}",
        rows.len(),
        rows.join("\n  ")
    );
    eprintln!(
        "indoors: {} standable room(s) of {home:?}; the viewer stayed indoors at {indoors} \
         of them and swept out of the doorway at {} ({:?}...); {} distinct SqCoord(s); the clamp \
         and the mask differ at {differ} of {indoors} — every room's own origin is inside its \
         block, so the two agree here and the difference is structural rather than \
         data-dependent (see the off-block stations)",
        rooms.len(),
        left_the_room.len(),
        left_the_room.iter().take(3).collect::<Vec<_>>(),
        seen.len(),
    );

    // On the shipped dats `0xA9B4` has 57 standable rooms, the viewer stays indoors in all of
    // them, and they sit over 11 distinct land squares. The floors below are envelopes around
    // those numbers rather than the numbers themselves, because the denominator is the block's
    // prefetched residency, which follows the streaming window; the exact counts are printed
    // above so a change is visible either way.
    assert!(
        rooms.len() >= 40,
        "only {} standable room(s) in {home:?} (57 on the shipped dats): the block's \
         interior residency has changed and this station has lost its subject",
        rooms.len()
    );
    assert!(
        left_the_room.is_empty(),
        "the viewer swept out of the room at {} of {} station(s) ({:?}): the 0.3 m viewer sphere is supposed to stop on the room's own walls, so this station would no longer be measuring the indoor arm",
        left_the_room.len(),
        rooms.len(),
        left_the_room,
    );
    assert!(
        seen.len() >= 8,
        "the indoor index named only {} distinct land square(s) over {indoors} rooms (11 on the \
         shipped dats): it is not tracking the room",
        seen.len()
    );
    assert_eq!(
        differ, 0,
        "the clamp and the mask disagree at {differ} indoor station(s), all of which are inside \
         the block the viewpoint is measured against, where `clamp` and `& 7` are the same \
         function"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. At a block boundary — where the clamp hides itself
// ---------------------------------------------------------------------------------------------

/// **Crossing a landblock boundary, the index wraps `7 -> 0` the way `& 7` does.**
///
/// The position-derived clamp does so too: after `recenter()` the viewpoint is inside the centre
/// block by construction, so `clamp` and `& 7` are the same function and **a test that only
/// crosses block boundaries cannot tell them apart**. The agreement is asserted rather than
/// assumed, so that
/// [`the_clamp_reports_a_cell_the_viewer_is_not_in`] is visibly the discriminating case.
#[test]
fn at_a_block_boundary_the_index_wraps_rather_than_saturating() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = embodied(&store, &mut gpu);

    stand(
        &mut scene,
        block_at(hx, hy).cell(1),
        Vec3::new(96.0, 96.0, 0.0),
    );
    for i in 1..=10 {
        sim(&mut scene, f64::from(i) / 30.0);
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");
    }

    // Two stations on each axis, either side of one 192 m boundary: the last cell of the home
    // block and the first cell of its neighbour.
    //
    // **The off-axis coordinate is 100.0 and not 96.0, deliberately.** 96.0 is exactly `4 * 24`,
    // a *cell* boundary, and the body's sphere (radius 0.48) straddles it. `Character::teleport`
    // commits through physics and takes the committed cell from the transition's swept-sphere
    // result rather than recomputing it from the endpoint. At an exact cell boundary, the
    // neighbouring-cell search resolves a straddling sphere to the **lower** cell where
    // `adjust_to_outside`'s `floor(x / 24)` gives the upper one; the two disagree by one index on
    // the tie, which is not what this file is about. `floor(100 / 24) == floor(96 / 24) == 4`, so
    // the expected values are those of the 96.0 line with the tie not stood on.
    let legs: [(&str, LandblockId, Vec3, (u8, u8)); 4] = [
        (
            "north edge",
            block_at(hx, hy),
            Vec3::new(100.0, BLOCK - 0.1, 0.0),
            (4, 7),
        ),
        (
            "north over",
            block_at(hx, hy + 1),
            Vec3::new(100.0, 0.1, 0.0),
            (4, 0),
        ),
        (
            "east edge",
            block_at(hx, hy + 1),
            Vec3::new(BLOCK - 0.1, 100.0, 0.0),
            (7, 4),
        ),
        (
            "east over",
            block_at(hx + 1, hy + 1),
            Vec3::new(0.1, 100.0, 0.0),
            (0, 4),
        ),
    ];

    let mut t = 1.0f64;
    for (name, block, origin, expect) in legs {
        stand(&mut scene, block.cell(1), origin);
        t += 1.0;
        sim(&mut scene, t);
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");

        let got = scene.viewer_draw_cell();
        let want = masked_from_id(&scene).expect("the viewpoint has a cell id");
        let before = clamped_from_position(&scene);
        let p = viewpoint(&scene);
        eprintln!(
            "{name}: viewer block {:?}, viewpoint {:.2},{:.2}; before {before:?} after \
             {got:?} (id says {want:?})",
            scene.viewer_block(),
            p.x,
            p.y
        );

        assert_eq!(
            got, want,
            "{name}: the scene says {got:?}, the viewer's cell id {want:?}"
        );
        assert_eq!(got, expect, "{name}: the index is {got:?}, not {expect:?}");
        // The premise for the load-time station: the window really did re-centre, which is what
        // pulls the viewpoint back inside the centre block and makes the clamp inert.
        assert_eq!(
            scene.viewer_block(),
            Some((block.0 >> 8).into()).map(|x: i32| (x, i32::from(block.0 & 0xFF))),
            "{name}: the window did not re-centre on {block:?}"
        );
        assert_eq!(
            before, got,
            "{name}: the clamp answered {before:?} and the mask {got:?}. That is a real \
             difference, but not the one this test is for — after `recenter` the viewpoint is \
             inside the centre block and the two must agree, which is exactly why crossing block \
             boundaries cannot tell them apart"
        );
    }
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 4. The difference
// ---------------------------------------------------------------------------------------------

/// **The clamp names a cell the viewer is not in, on a path the client takes on every load.**
///
/// `WorldScene::load` places the flycam at `(0.5 * 192, -0.35 * 192)` — **67.2 m south of
/// the centre block** — and calls `update_viewer_cell()` on the next line, with no `recenter`
/// between. `floor(-67.2 / 24)` is `-3`; the clamp turns that into `0` and reports the block's
/// southern edge, while the cell id puts the viewpoint in the block
/// to the south and `& 7` names cell `5` of it. Those are different cells, four apart, and the
/// second is where the flycam actually is.
///
/// The premise is asserted first, in both directions: the viewpoint really is outside the centre
/// block, and the two derivations really do disagree at this station. Without that a green run
/// would mean only that nothing distinguished them.
#[test]
fn the_clamp_reports_a_cell_the_viewer_is_not_in() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    // No body: `WorldScene::load` alone, which is the `--no-character` path and the state every
    // scene passes through before `attach_character` runs.
    let scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");

    let p = viewpoint(&scene);
    let before = clamped_from_position(&scene);
    let after = scene.viewer_draw_cell();
    let want = masked_from_id(&scene).expect("the flycam resolves to a land cell");

    eprintln!(
        "at load: viewer block {:?}, flycam {:.2},{:.2} (that is {:.2} blocks south); draw-order square {before:?} before and {after:?} after",
        scene.viewer_block(),
        p.x,
        p.y,
        p.y / BLOCK
    );

    // Premise 1: the station is what it claims to be.
    assert!(
        p.y < 0.0 || p.y >= BLOCK || p.x < 0.0 || p.x >= BLOCK,
        "the flycam is at {p:?}, inside the centre block: at such a station `clamp` and `& 7` are \
         the same function and this test cannot distinguish them. `WorldScene::load`'s camera \
         placement has moved."
    );
    // Premise 2: the two derivations disagree here, so a change that made the difference
    // unreachable fails here rather than passing the claim below vacuously.
    assert_ne!(
        before, want,
        "the clamp and the mask both answer {before:?} at {p:?}, so this frame cannot tell the two \
         derivations apart"
    );

    // The claim.
    assert_eq!(
        after, want,
        "the scene uses draw-order square {after:?}, not the id-derived {want:?}"
    );
    assert_ne!(
        after, before,
        "the scene still uses the clamped draw-order square {before:?}"
    );

    // And the arithmetic named rather than only compared: 67.2 m south of the block origin is
    // three cells below it, which `& 7` reads as cell 5 of the block to the south.
    let (bx, by) = scene.viewer_block().expect("a viewer block");
    let south = dereth_physics::landdefs::get_outside_cell_id(
        dereth_physics::landdefs::lcoord_to_gid(bx * 8, by * 8),
        p,
    );
    assert!(
        dereth_physics::landdefs::is_outdoors(south) && south.0 != 0,
        "the flycam's position does not resolve to a land cell at all"
    );
    assert_ne!(
        south.0 >> 16,
        u32::from(DEFAULT_LANDBLOCK),
        "the flycam resolves into the centre block {DEFAULT_LANDBLOCK:#06X}, so it is not outside \
         it and the clamp would be inert"
    );
    // The value named rather than only compared: -67.2 / 24 is -2.8, floor is -3, the
    // home block y lcoord is 180 * 8 = 1440, and (1440 - 3) & 7 is 5. Computed here by
    // hand so the expected SqCoord is not a restatement of the code under test.
    assert_eq!(
        after,
        (4, 5),
        "67.2 m south of the home block is its neighbour cell 5"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. The same difference on the INDOOR arm
// ---------------------------------------------------------------------------------------------

/// **An off-block origin under a room's id leaves the room, and the clamp still saturates where
/// the mask wraps.**
///
/// [`the_cell_index_tracks_the_land_cell_under_every_room_of_the_block`] measures a **null**: in
/// every standable room of the block, `clamp(floor(p / 24))` and
/// `gid_to_lcoord(get_outside_cell_id(..)) & 7` give the same SqCoord. The mechanism is not merely
/// that the rooms happen to sit conveniently: here
/// a `Position`'s frame is **landblock-local for an interior cell too**, so
/// both derivations read the same two floats and divide them by the same 24. They *must* agree
/// wherever the clamp is inert, on any data. The only way to part them is a viewpoint whose
/// landblock-local origin is **outside** its own block.
///
/// Asking for `room` at a point 67 m south does not leave the body indoors: placement finds no
/// child cell of the room containing the point, falls back to the room's sees-outside flag, and
/// resolves the body onto the visible **land cell** under it. So the station asserts that the
/// client **refuses** to hold a body in a room it is not in, that the shipped rooms of this block
/// all sit inside `[0, 192)` (reported and asserted, so a dat that changed would say so), and then
/// measures the off-block difference between clamp and mask on the state the client reaches.
/// [`the_clamp_reports_a_cell_the_viewer_is_not_in`] is the same difference reached from the
/// flycam; this one reaches it through an interior id.
#[test]
fn indoors_the_clamp_reports_a_cell_the_viewer_is_not_in() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home = block_at(hx, hy);
    let mut scene = embodied(&store, &mut gpu);

    stand(&mut scene, home.cell(1), Vec3::new(96.0, 96.0, 0.0));
    for i in 1..=10 {
        sim(&mut scene, f64::from(i) / 30.0);
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");
    }

    let room = a_resident_room(&scene)
        .unwrap_or_else(|| panic!("no interior cell is resident in {home:?}, so there is no room"));

    // Half of the mechanism: how far the shipped rooms of this block reach from their own block's
    // origin. A room whose frame sat outside [0, 192) would put the viewpoint off-block **while
    // indoors** with no fabrication at all.
    let mut outside = 0usize;
    let mut total = 0usize;
    {
        let land = Arc::clone(scene.character.as_ref().expect("a body").land());
        for idx in 0x100u32..0x200 {
            let id = CellId((u32::from(DEFAULT_LANDBLOCK) << 16) | idx);
            let Some(g) = LandSource::env_cell(land.as_ref(), id) else {
                continue;
            };
            total += 1;
            let o = g.frame.origin;
            if o.x < 0.0 || o.x >= BLOCK || o.y < 0.0 || o.y >= BLOCK {
                outside += 1;
            }
        }
        eprintln!(
            "indoors: {outside} of {total} resident room(s) of {DEFAULT_LANDBLOCK:#06X} \
             carry a cell frame outside [0, 192) on either axis"
        );
    }
    assert!(
        total > 0,
        "no resident room at all: this station has no subject"
    );

    // The station: 67.2 m south of the block the window is centred on, **named with the room's
    // id**, with no `recenter` between the placement and the read.
    //
    // **`x` is 100.0 and not 96.0 for the reason the block-boundary station gives.** 96.0 is
    // exactly `4 * 24`, a cell boundary, and the body's sphere (radius 0.48) straddles it; the
    // committed cell comes from the neighbouring-cell search, which resolves a straddling sphere
    // to the **lower** cell where endpoint `floor(x / 24)` gives the upper one. At 96.0 the
    // outside cell is `0xA9B3001E`, index 30, i.e. `((30 - 1) >> 3, (30 - 1) & 7) == (3, 5)`
    // against the hand-computed `(4, 5)`. `floor(100 / 24) == floor(96 / 24) == 4`, so the
    // expected values are those of the 96.0 line and the tie is not stood on.
    stand(&mut scene, room, Vec3::new(100.0, -67.2, 0.0));
    scene.follow_character_now();

    let body_cell = scene.character.as_ref().expect("a body").position().cell;
    let viewer_cell = scene.character.as_ref().expect("a body").camera.viewer.cell;
    let p = viewpoint(&scene);
    let before = clamped_from_position(&scene);
    let after = scene.viewer_draw_cell();
    let want = masked_from_id(&scene).expect("the viewpoint has a cell id");

    eprintln!(
        "indoors, off-block: viewer block {:?}, asked for {room:?} at (100.0, -67.2) and the body resolved to {body_cell:?} / viewer {viewer_cell:?} at {:.2},{:.2}; draw-order square {before:?} before and {after:?} after",
        scene.viewer_block(),
        p.x,
        p.y,
    );
    scene.release_textures(&mut gpu);

    // **The producer's own answer.** An origin no child cell of `room` contains falls through to
    // the room's sees-outside path and onto the land cell the building stands in. So the client
    // **refuses** to hold a body in a room 67 m from it, and the state "indoors and off-block" is
    // not reachable with these dats: `outside` above is the only way in, and it is 0.
    assert_eq!(
        outside, 0,
        "{outside} of {total} rooms of {DEFAULT_LANDBLOCK:#06X} do carry an off-block frame, so \
         the indoor-and-off-block state IS reachable here and this station should drive it \
         rather than assert it cannot be reached"
    );
    assert!(
        dereth_physics::landdefs::is_outdoors(body_cell),
        "the body kept {body_cell:?} 67.2 m outside {room:?}; the physics-position outside fallback is supposed to resolve it onto the land cell"
    );
    assert_eq!(
        body_cell.landblock(),
        LandblockId(DEFAULT_LANDBLOCK.wrapping_sub(1)),
        "67.2 m south of {DEFAULT_LANDBLOCK:#06X} is the block below it, and that is the block outdoor-cell normalization should have named"
    );

    // Premise, both directions: the viewpoint really is off-block, so the clamp is not inert.
    assert!(
        p.y < 0.0 || p.y >= BLOCK || p.x < 0.0 || p.x >= BLOCK,
        "the viewpoint {p:?} is inside the centre block, where `clamp` and `& 7` are the same \
         function"
    );
    assert_ne!(
        before, want,
        "the clamp and the mask both answer {before:?} at {p:?}, so this station cannot tell the \
         two derivations apart"
    );

    // The claim, with the expected value computed by hand rather than restated from the code
    // under test: -67.2 / 24 is -2.8, floor is -3, the home block's y lcoord is 180 * 8 = 1440,
    // and (1440 - 3) & 7 is 5.
    assert_eq!(
        after, want,
        "the scene uses draw-order square {after:?}, not the id-derived {want:?}"
    );
    assert_eq!(
        after,
        (4, 5),
        "67.2 m south of {DEFAULT_LANDBLOCK:#06X} is cell 5 of the block below it; the scene says \
         {after:?}"
    );
    assert_eq!(
        before,
        (4, 0),
        "the clamp is expected to saturate to the block's own edge cell"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. A viewpoint with no usable cell id leaves the last one standing
// ---------------------------------------------------------------------------------------------

/// **A zero viewer id changes nothing: draw-order calculation returns without touching the
/// order.**
///
/// `adjust_to_outside` answers `CellId(0)` when the origin it is handed walks off the
/// `0x7F8`-cell world, and `gid_to_lcoord` refuses that id too. So there is a real state in which
/// the viewpoint has no cell, and the client keeps the SqCoord it had; a build that reported
/// `(0, 0)` would swing every block's cell order to the block's south-west corner for as long as
/// the viewpoint stayed off the map. The other stations are all mid-world and hand the derivation
/// a good id, so only this one tells keeping the last value from a `(0, 0)` fallback.
#[test]
fn a_viewpoint_off_the_edge_of_the_world_leaves_the_last_cell_standing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    // No body, so the flycam is the viewpoint and its position is settable from here.
    let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("it loads");

    let settled = scene.viewer_draw_cell();
    // The premise: the last value has to be something a `(0, 0)` fallback would differ from, or
    // this test cannot tell "kept the last one" from "reported the corner".
    assert_ne!(
        settled,
        (0, 0),
        "the scene's settled SqCoord is already (0, 0), so nothing below distinguishes keeping it \
         from falling back to it"
    );

    // 20 km north. The home block's y land coordinate is 180 * 8 = 1440 and the world stops at
    // 0x7F8 = 2040, so 20000 / 24 = 833 cells puts it past the edge.
    let p = scene.camera.position;
    scene.camera.position = Vec3::new(p.x, 20_000.0, p.z);
    // The premise, again and from the other side: the id really does fail to resolve.
    let (bx, by) = scene.viewer_block().expect("a viewer block");
    let off = dereth_physics::landdefs::get_outside_cell_id(
        dereth_physics::landdefs::lcoord_to_gid(bx * 8, by * 8),
        scene.camera.position,
    );
    assert_eq!(
        off.0, 0,
        "20 km north of {DEFAULT_LANDBLOCK:#06X} still resolves to {off:?}, so this station does not reach the zero-id draw-order arm at all"
    );

    scene.follow_character_now();
    let after = scene.viewer_draw_cell();
    eprintln!("off the map: SqCoord {settled:?} -> {after:?} (the id resolved to {off:?})");
    scene.release_textures(&mut gpu);

    assert_eq!(
        after, settled,
        "a viewpoint with no cell id changed the SqCoord from {settled:?} to {after:?}; zero-id draw-order calculation should return without touching the order"
    );
}

// ---------------------------------------------------------------------------------------------
// 7. The shipped path: how often the two derivations part company on a plain walk
// ---------------------------------------------------------------------------------------------

/// One walk's worth of counts.
struct Walk {
    frames: usize,
    /// Frames on which `floor(position / 24)` and the viewer's own cell id named different cells.
    differ: usize,
    /// Frames on which the reported SqCoord changed at all — the premise that it is live.
    cell_changes: usize,
    crossings: usize,
    first: Option<((u8, u8), (u8, u8))>,
}

/// Run north for twenty seconds from `(x, 186)` of the home block, counting the two derivations
/// against each other every frame, in `App::frame`'s order.
fn walk_north(store: &Arc<RetailDatStore>, gpu: &mut Gpu, x: f32) -> Walk {
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = embodied(store, gpu);
    stand(
        &mut scene,
        block_at(hx, hy).cell(1),
        Vec3::new(x, 186.0, 0.0),
    );
    for i in 1..=10 {
        sim(&mut scene, f64::from(i) / 30.0);
        dereth_client::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(f64::from(i) / 30.0),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("the streamed blocks build");
    }

    let mut w = Walk {
        frames: 0,
        differ: 0,
        cell_changes: 0,
        crossings: 0,
        first: None,
    };
    let mut last_block = scene.viewer_block().expect("a viewer block");
    let mut last_cell = scene.viewer_draw_cell();
    let dt = 1.0f32 / 30.0;
    for i in 1..=600u32 {
        let now = f64::from(i + 10) / 30.0;
        let running = CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        };
        scene.update(CameraInput::default(), running, LocalTime(now), dt);

        let after = scene.viewer_draw_cell();
        let before = clamped_from_position(&scene);
        w.frames += 1;
        if before != after {
            w.differ += 1;
            if w.first.is_none() {
                w.first = Some((before, after));
            }
        }
        if after != last_cell {
            w.cell_changes += 1;
            last_cell = after;
        }

        dereth_client::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            f64::from(dt),
        );
        scene.stream(store, gpu).expect("the streamed blocks build");
        let b = scene.viewer_block().expect("a viewer block");
        if b != last_block {
            w.crossings += 1;
            last_block = b;
        }
    }
    scene.release_textures(gpu);
    w
}

/// **The sweep's own cell id and its position are not reconstructions of each other, and two
/// walks say how far apart the answers can be.**
///
/// The stations above measure a **null** wherever the viewpoint is safely inside a cell: the
/// position-derived SqCoord and the id-derived one agree at all 64 outdoor stations and across the
/// selected standable-room stations that keep the camera indoors; rooms whose camera leaves are
/// counted separately. The mechanism is structural: both are `floor(origin / 24)` of the same two
/// floats.
///
/// **The mechanism stops holding on a cell boundary.** The viewer's cell id is not recomputed from
/// the endpoint: it is the swept transition's current cell, whichever candidate last accepted the
/// sphere. On a grid line the point is inside both neighbours because the cell-edge rejection is
/// strict. The candidate loop keeps the **last** matching land cell and visits the `-1` neighbour
/// after the `floor` cell, so the id names the cell *below* the line. Viewpoint update consumes
/// that id, not the endpoint `floor`. It is not a fact about the path: nothing in that chain reads
/// an earlier position, and a boundary station approached from either side gives the same answer.
///
/// **Two walks, because one of them would be the wrong number either way.** A fixture built on
/// exact axis alignment tests the one case where a difference cancels; here exact alignment is
/// the case where it fires *every frame*, the same trap with the opposite sign. So the walk is
/// run at `x = 96.0`, which is exactly the `4 * 24`
/// line, **and** at `x = 100.0`, four metres inside a cell, and both counts are reported. Neither
/// alone is "how often this matters".
///
/// No epsilon: the thresholds are the 24 m cell grid and the 192 m block grid.
#[test]
fn the_id_and_the_position_name_different_cells_on_a_plain_walk() {
    let store = store();

    let mut total_differ = 0usize;
    let mut lines = Vec::new();
    for (name, x) in [("on a 24 m line", 96.0f32), ("4 m inside a cell", 100.0f32)] {
        let mut gpu = crate::common::test_gpu(800, 600);
        let w = walk_north(&store, &mut gpu, x);
        lines.push(format!(
            "x = {x:.1} ({name}): {} of {} frame(s) differ; the SqCoord changed on {}; {} \
             crossing(s){}",
            w.differ,
            w.frames,
            w.cell_changes,
            w.crossings,
            w.first.map_or(String::new(), |(b, a)| format!(
                "; first disagreement {b:?} before, {a:?} after"
            ))
        ));
        // Premise, per walk: the body walked and the SqCoord is live, so "they agree" could have
        // been "they differ" on this walk.
        assert!(
            w.crossings >= 1,
            "the {name} walk crossed no landblock boundary"
        );
        assert!(
            w.cell_changes >= 2,
            "the {name} walk's SqCoord changed on {} frame(s), so it crossed no cell line",
            w.cell_changes
        );
        total_differ += w.differ;
    }
    eprintln!("walks:\n  {}", lines.join("\n  "));

    // The claim. A null across both walks would mean the id had been a harmless restatement of
    // the position on the shipped path, which is a different result and would need saying.
    assert!(
        total_differ > 0,
        "over both walks the viewer's own cell id and a `floor` of its position never named \
         different cells: {}",
        lines.join(" | ")
    );
}

// ---------------------------------------------------------------------------------------------
// Where the viewer's cell id comes from
// ---------------------------------------------------------------------------------------------

mod cell_source {
    //! The viewer's cell id is a pure function of where the viewer sphere stops, not a memory of the
    //! path: on a 24 m line the tie is broken by the container search's **last match wins** rule over
    //! the sphere's list of outside cells. Measured on the viewer of `WorldObjects` on Holtburg's own
    //! terrain (`0xA9B4`), over runs driven in `App::frame`'s order: four walks crossing the `x = 96`
    //! and `y = 96` lines from opposite sides, and one ride exactly along `x = 96`. Every frame's id is
    //! compared with an independent container oracle built on the closed 24 m square, which is itself
    //! checked against `find_terrain_poly` on the shipped terrain in both directions. Fixture: the
    //! retail dats on a software device; fails when the dats or a device are absent.

    use super::{block_at, store, CELL};
    use dereth_client::world::SceneWrites;
    use std::sync::Arc;

    use dereth_client::camera::CameraInput;
    use dereth_client::character::CharacterInput;
    use dereth_client::world::{block_xy, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
    use dereth_dat::RetailDatStore;
    use dereth_physics::landdefs;
    use dereth_primitives::num::math;
    use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
    use dereth_render::device::Gpu;

    /// The viewer sphere's radius, which is what sets the window in which a neighbouring cell is
    /// added to the sphere's cell list.
    const R: f32 = dereth_physics::globals::VIEWER_SPHERE_RADIUS;

    fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
        let mut scene =
            WorldScene::load(store, gpu, SceneConfig::default()).expect("the scene loads");
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        scene
            .attach_character(store, &region, gpu)
            .expect("the body is created");
        scene
    }

    /// A yaw about z. The body faces `+y` at the identity, so `+90 degrees` faces west and
    /// `-90 degrees` faces east — asserted as a premise in each walk rather than assumed.
    fn yaw(degrees: f32) -> Quat {
        let h = degrees.to_radians() * 0.5;
        Quat {
            w: math::cosf(h),
            x: 0.0,
            y: 0.0,
            z: math::sinf(h),
        }
    }

    fn stand(scene: &mut WorldScene, cell: CellId, origin: Vec3, rot: Quat) {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(cell, Frame::new(origin, rot)));
    }

    // =================================================================================================
    // The oracle: the cell-list search's container arm, with a closed-square containment test
    // =================================================================================================

    /// Retail's land-cell containment rule, represented as the closed 24 m square.
    ///
    /// The cell's two terrain triangles tile the square and the 2-D polygon test rejects on `v > 0`, so
    /// a point on any edge — including the shared diagonal — is inside. Checked against the shipped
    /// `find_terrain_poly` in [`the_closed_square_is_point_in_cell_on_retail_terrain`].
    fn point_in_closed_square(cell: CellId, p: Vec3) -> bool {
        let Some((gx, gy)) = landdefs::gid_to_lcoord(cell) else {
            return false;
        };
        #[allow(clippy::cast_precision_loss)]
        let (x0, y0) = (f64::from(gx & 7) * 24.0, f64::from(gy & 7) * 24.0);
        let (px, py) = (f64::from(p.x), f64::from(p.y));
        px >= x0 && px <= x0 + 24.0 && py >= y0 && py <= y0 + 24.0
    }

    /// Collect the outside cells touched by one sphere, then apply the container loop: the **last**
    /// land cell of the array
    /// that contains the point.
    ///
    /// A second, deliberate copy of the rule the physics crate's boundary-cell test uses, written
    /// against a different containment test so that the two are independent readings rather than one
    /// reading called twice.
    fn container_cell(base: CellId, center: Vec3, radius: f32) -> Option<CellId> {
        let mut id = base;
        let mut o = center;
        if !landdefs::adjust_to_outside(&mut id, &mut o) {
            return None;
        }
        let (cx, cy) = (landdefs::cell_of(o.x), landdefs::cell_of(o.y));
        #[allow(clippy::cast_precision_loss)]
        let local = Vec3::new(o.x - cx as f32 * CELL, o.y - cy as f32 * CELL, 0.0);
        let (lo, hi) = (CELL - radius, radius);
        let (gx, gy) = landdefs::gid_to_lcoord(id)?;

        let mut arr: Vec<CellId> = Vec::new();
        let push = |x: i32, y: i32, arr: &mut Vec<CellId>| {
            if !(0..0x7F8).contains(&x) || !(0..0x7F8).contains(&y) {
                return;
            }
            let cid = landdefs::lcoord_to_gid(x, y);
            if !arr.contains(&cid) {
                arr.push(cid);
            }
        };
        push(gx, gy, &mut arr);
        if lo < local.x {
            push(gx + 1, gy, &mut arr);
            if lo < local.y {
                push(gx + 1, gy + 1, &mut arr);
            }
            if local.y < hi {
                push(gx + 1, gy - 1, &mut arr);
            }
        }
        if local.x < hi {
            push(gx - 1, gy, &mut arr);
            if lo < local.y {
                push(gx - 1, gy + 1, &mut arr);
            }
            if local.y < hi {
                push(gx - 1, gy - 1, &mut arr);
            }
        }
        if lo < local.y {
            push(gx, gy + 1, &mut arr);
        }
        if local.y < hi {
            push(gx, gy - 1, &mut arr);
        }

        let mut container = None;
        for cid in arr {
            let p = center.sub_v(landdefs::get_block_offset(base, cid));
            if point_in_closed_square(cid, p) {
                container = Some(cid);
            }
        }
        container
    }

    /// The one vector operation this file needs, written out so the file does not depend on a physics
    /// trait for it.
    trait SubV {
        fn sub_v(self, o: Vec3) -> Vec3;
    }
    impl SubV for Vec3 {
        fn sub_v(self, o: Vec3) -> Vec3 {
            Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
        }
    }

    /// The viewer's position in **global** metres, so that a landblock re-base cannot make the walk
    /// look like a jump.
    ///
    /// `viewer.frame.origin` is landblock-relative, so a walk sampled across a block boundary would
    /// read as a jump of a block's width, and the travel premises would pass on the wrong thing.
    fn global_xy(pos: &Position) -> (f64, f64) {
        let (bx, by) = landdefs::blockid_to_lcoord(pos.cell).expect("an outdoor cell id");
        (
            f64::from(bx) * f64::from(CELL) + f64::from(pos.frame.origin.x),
            f64::from(by) * f64::from(CELL) + f64::from(pos.frame.origin.y),
        )
    }

    // =================================================================================================
    // 1. The two containment rules are the same rule, on the shipped terrain
    // =================================================================================================

    /// **The cross-check between this file's oracle and the physics-side one**, on retail geometry
    /// rather than on a flat synthetic block, and in both directions: a point one millimetre inside a
    /// cell must be in it and a point one millimetre outside must not, and the closed-square rule must
    /// agree with `find_terrain_poly` at every one of the 4,096 probes.
    #[test]
    fn the_closed_square_is_point_in_cell_on_retail_terrain() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);
        let scene = embodied(&store, &mut gpu);
        let land = scene.character.as_ref().expect("a body").world.land();
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let block = land
            .landblock(block_at(hx, hy))
            .expect("the home block is resident");

        let mut probes = 0_usize;
        let mut inside = 0_usize;
        let mut disagreements = 0_usize;
        for ci in 0..64_u16 {
            let cell = block_at(hx, hy).cell(ci + 1);
            for k in 0..64_u32 {
                // A ring of probes around the cell: corners, edge midpoints, the centre, and points a
                // millimetre either side of each edge, so both answers occur.
                #[allow(clippy::cast_precision_loss)]
                let t = f32::from(u16::try_from(k).expect("small")) / 63.0;
                let (gx, gy) = landdefs::gid_to_lcoord(cell).expect("outdoor");
                #[allow(clippy::cast_precision_loss)]
                let (x0, y0) = ((gx & 7) as f32 * CELL, (gy & 7) as f32 * CELL);
                let p = match k % 4 {
                    0 => Vec3::new(x0 + t * CELL, y0, 0.0), // the south edge
                    1 => Vec3::new(x0 + t * CELL, y0 + CELL, 0.0), // the north edge
                    2 => Vec3::new(x0 - 0.001, y0 + t * CELL, 0.0), // a millimetre west
                    _ => Vec3::new(x0 + t * CELL, y0 + CELL + 0.001, 0.0), // a millimetre north
                };
                let want = block.find_terrain_poly(cell.index(), p).is_some();
                let got = point_in_closed_square(cell, p);
                probes += 1;
                if want {
                    inside += 1;
                }
                if want != got {
                    disagreements += 1;
                }
            }
        }
        eprintln!(
            "containment: {probes} probe(s) over 64 cells of {:#06X}; {inside} inside by \
         `find_terrain_poly`, {disagreements} disagreement(s) with the closed square",
            DEFAULT_LANDBLOCK
        );
        assert_eq!(probes, 4096, "the denominator");
        assert!(
            inside > 0,
            "some probes must be inside, or the check only proves it can say no"
        );
        assert!(
            inside < probes,
            "and some must be outside, or it only proves it can say yes"
        );
        assert_eq!(
            disagreements, 0,
            "the closed square is `point_in_cell` for a land cell"
        );
        scene.world.character.expect("a body");
    }

    // =================================================================================================
    // 2. The walks
    // =================================================================================================

    struct Crossing {
        frames: usize,
        /// Frames on which the viewer's own id and the container rule named different cells.
        against_oracle: usize,
        /// Frames on which the viewer's own id and `floor(origin / 24)` named different cells.
        against_floor: usize,
        /// Frames on which the viewer stood within one sphere radius of the `x = 96` (or `y = 96`)
        /// line, i.e. where the array carries two cells and the tie-break is live.
        near_the_line: usize,
        /// Frames exactly on the line.
        on_the_line: usize,
        first: f64,
        last: f64,
        /// The largest single-frame move along the walk's axis, in metres. A settled follow camera at
        /// a run is about 0.17 m per frame; anything near a metre is a snap, not a walk.
        max_step: f64,
        /// How far the walk travelled along the **other** axis, which is what says a ride *along* a
        /// line was a walk at all.
        other_travel: f64,
    }

    /// Drive `frames` frames of a run in `App::frame`'s order from `(x, y)` facing `heading`, sampling
    /// the viewer's own position and cell id every frame.
    fn crossing(
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        start: Vec3,
        heading: f32,
        axis_x: bool,
        frames: u32,
    ) -> Crossing {
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let mut scene = embodied(store, gpu);
        stand(&mut scene, block_at(hx, hy).cell(1), start, yaw(heading));
        // Settle: let the camera reach its steady offset behind the body before anything is counted.
        // **Forty frames, not ten**: with ten, the camera has not caught up with the teleport and the
        // first counted sample sits tens of metres from the body. The per-frame ceiling asserted by
        // the caller catches an unsettled start.
        for i in 1..=40 {
            let now = f64::from(i) / 30.0;
            scene.update(
                CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
            dereth_client::camera::update_viewer(
                &mut scene,
                CameraInput::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
            scene.stream(store, gpu).expect("the streamed blocks build");
        }

        let mut out = Crossing {
            frames: 0,
            against_oracle: 0,
            against_floor: 0,
            near_the_line: 0,
            on_the_line: 0,
            first: 0.0,
            last: 0.0,
            max_step: 0.0,
            other_travel: 0.0,
        };
        let mut first_other = 0.0_f64;
        let dt = 1.0_f32 / 30.0;
        for i in 1..=frames {
            let now = f64::from(i + 40) / 30.0;
            let running = CharacterInput {
                forward: true,
                run: true,
                ..CharacterInput::default()
            };
            scene.update(CameraInput::default(), running, LocalTime(now), dt);
            dereth_client::camera::update_viewer(
                &mut scene,
                CameraInput::default(),
                LocalTime(now),
                f64::from(dt),
            );
            scene.stream(store, gpu).expect("the streamed blocks build");

            let c = scene.character.as_ref().expect("a body");
            let v = c.camera.viewer;
            if !landdefs::is_outdoors(v.cell) {
                continue;
            }
            let o = v.frame.origin;
            let (gx, gy) = global_xy(&v);
            let coord = if axis_x { gx } else { gy };
            let other = if axis_x { gy } else { gx };
            if out.frames == 0 {
                first_other = other;
            }
            out.other_travel = (other - first_other).abs();
            if out.frames == 0 {
                out.first = coord;
            } else {
                out.max_step = out.max_step.max((coord - out.last).abs());
            }
            out.last = coord;
            out.frames += 1;

            let want = container_cell(v.cell, o, R);
            if want != Some(v.cell) {
                out.against_oracle += 1;
            }
            if landdefs::get_outside_cell_id(v.cell, o) != v.cell {
                out.against_floor += 1;
            }
            let d = (coord / 24.0 - (coord / 24.0).round()).abs() * 24.0;
            if d <= f64::from(R) {
                out.near_the_line += 1;
            }
            if d == 0.0 {
                out.on_the_line += 1;
            }
        }
        scene.release_textures(gpu);
        out
    }

    /// Behaviour: world.viewpoint.the-viewers-cell-is-its-position-under-the-container-rule
    ///
    /// **The viewer's cell id is the container rule applied to its own position, whichever side it
    /// crosses from.** Four walks on the shipped block: east and west across `x = 96.0`, north and
    /// south across `y = 96.0`.
    ///
    /// The premise of each is asserted before its result is read — the walk must actually have moved
    /// in the declared direction and must actually have crossed the line, or it is not a crossing and
    /// a clean answer from it means nothing.
    #[test]
    fn the_viewers_cell_is_its_own_position_read_through_the_container_rule() {
        let store = store();

        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let home_sw =
            landdefs::blockid_to_lcoord(block_at(hx, hy).cell(1)).expect("the home block");

        let mut lines = Vec::new();
        let mut total_frames = 0_usize;
        let mut total_oracle = 0_usize;
        let mut total_floor = 0_usize;
        let mut total_near = 0_usize;

        // The **viewer** trails the body by about nine metres, so the body's own crossing is not the
        // one that matters and a walk long enough for the body is not long enough for the camera: a
        // 60-frame run does not carry the viewer to the line. 150 frames at ~4 m/s is five seconds
        // and about twenty metres, which carries the viewer itself across; the premise assertions
        // below check that it did.
        let walks = [
            (
                "east across x = 96",
                Vec3::new(90.0, 100.0, 0.0),
                -90.0_f32,
                true,
                1.0_f32,
            ),
            (
                "west across x = 96",
                Vec3::new(102.0, 100.0, 0.0),
                90.0,
                true,
                -1.0,
            ),
            (
                "north across y = 96",
                Vec3::new(100.0, 90.0, 0.0),
                0.0,
                false,
                1.0,
            ),
            (
                "south across y = 96",
                Vec3::new(100.0, 102.0, 0.0),
                180.0,
                false,
                -1.0,
            ),
        ];
        for (name, start, heading, axis_x, sign) in walks {
            let mut gpu = crate::common::test_gpu(800, 600);
            let c = crossing(&store, &mut gpu, start, heading, axis_x, 150);

            // Premise 1: the body moved, and it moved the way this walk says it did.
            let travelled = c.last - c.first;
            assert!(
                travelled * f64::from(sign) > 1.0,
                "the {name} walk moved {travelled:.3} m along its axis, which is not a run in the \
             declared direction"
            );
            // Premise 1b: it walked rather than snapped. See [`Crossing::max_step`].
            assert!(
                c.max_step < 1.0,
                "the {name} walk moved {:.3} m in one frame, which is a camera snap and not a walk",
                c.max_step
            );
            // Premise 2: it really crossed the line, so the tie-break was reachable. The line is the
            // home block's own `96 m`, expressed globally.
            let line = f64::from(if axis_x { home_sw.0 } else { home_sw.1 }) * 24.0 + 96.0;
            assert!(
                (c.first - line) * (c.last - line) < 0.0,
                "the {name} walk ran from {:.3} to {:.3} and never crossed {line:.3}",
                c.first,
                c.last
            );
            assert!(
            c.near_the_line > 0,
            "the {name} walk never came within {R} m of the line, so the array never carried two \
             cells and this walk cannot see the tie-break"
        );

            lines.push(format!(
            "{name}: {} frame(s) from {:.3} to {:.3}; {} within {R} m of the line ({} exactly on \
             it); {} differ from the container rule, {} from a floor",
            c.frames,
            c.first,
            c.last,
            c.near_the_line,
            c.on_the_line,
            c.against_oracle,
            c.against_floor
        ));
            total_frames += c.frames;
            total_oracle += c.against_oracle;
            total_floor += c.against_floor;
            total_near += c.near_the_line;
            let _ = total_floor;
        }

        eprintln!("crossings:\n  {}", lines.join("\n  "));

        assert!(
            total_frames >= 200,
            "the denominator: {total_frames} frames over four walks"
        );
        assert!(
        total_near > 0,
        "no frame of any walk stood within a sphere radius of the line, so nothing here tested \
         the tie-break"
    );
        // The claim: the viewer's id is what the container rule says about the viewer's own
        // position, on every frame of every walk, whichever side it came from.
        assert_eq!(
            total_oracle,
            0,
            "the viewer's own cell id disagreed with the cell-list container on {total_oracle} \
         of {total_frames} frame(s): {}",
            lines.join(" | ")
        );
    }

    /// **Riding exactly along a 24 m line, the viewer's id follows the container rule and not a
    /// `floor`.**
    ///
    /// No frame of any crossing lands *exactly* on a 24 m line, so on all four walks the container
    /// rule and a plain `floor` give the same answer — and "0 disagreements with the container rule"
    /// is then satisfied by a build that used a `floor` instead. A differential test is blind when
    /// the subject is absent from both arms.
    ///
    /// This is the station where they part. The body runs **north along `x = 96.0`**, so the viewer
    /// trails it exactly on the `4 * 24` line for the whole walk: the container rule says the cell
    /// **west** of the line, a `floor` says the cell east of it, and the viewer's own cell id must
    /// follow the first on every frame. It is `world::viewer_cell`'s on-the-line walk asked as a
    /// question about the *mechanism* rather than about the disagreement's size.
    ///
    /// **The start point is `world::viewer_cell`'s own, and that is not arbitrary.** The same ride
    /// begun at `(96.0, 84.0)` drifts **2.3e-5 m** off the line over 150 frames — the terrain there is
    /// not flat and the body slides a hair in `x` — and then no frame is on the line. That is why
    /// `on_the_line` and `against_floor` are asserted as *premises*.
    #[test]
    fn riding_the_line_is_where_the_container_rule_and_a_floor_part() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);
        let c = crossing(
            &store,
            &mut gpu,
            Vec3::new(96.0, 186.0, 0.0),
            0.0,
            true,
            150,
        );

        eprintln!(
            "ride: {} frame(s), {:.3} m north, x held to within {:.6} m of the line; {} frames \
         exactly on it; {} differ from the container rule, {} from a floor",
            c.frames,
            c.other_travel,
            (c.last - c.first).abs(),
            c.on_the_line,
            c.against_oracle,
            c.against_floor
        );

        // Premises: it is a walk, and it really is on the line.
        assert!(
            c.other_travel > 10.0,
            "the ride only travelled {:.3} m north",
            c.other_travel
        );
        assert!(
        c.on_the_line >= c.frames - 2,
        "only {} of {} frame(s) sat exactly on the line, so this station is not the one it claims \
         to be",
        c.on_the_line,
        c.frames
    );

        // The discriminator: here a `floor` is a *different* answer, on essentially every frame...
        assert!(
        c.against_floor >= c.frames - 2,
        "the container rule and a floor agreed on {} of {} frame(s) of a ride along the line, so \
         this station cannot tell the two derivations apart either",
        c.frames - c.against_floor,
        c.frames
    );
        // ...and the viewer's own id follows the container rule, not the floor.
        assert_eq!(
            c.against_oracle, 0,
            "the viewer's id disagreed with the cell-list container on {} of {} frame(s)",
            c.against_oracle, c.frames
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The load-time viewpoint does not re-centre
// ---------------------------------------------------------------------------------------------

mod load_time {
    //! The two arms that set the viewer's cell outside the frame loop — `WorldScene::load`'s bodiless
    //! flycam and `WorldScene::follow_character_now` — do not re-centre the window first, and a
    //! re-centre could not change their answer. The cell index is block-independent: the outside-cell
    //! id is `blockid_to_lcoord(id) + floor(origin / 24)` per axis, and a re-centre adds `8d` to the
    //! first term and takes `8d` from the second; with a body the viewpoint is a cell id and a
    //! cell-local frame the re-centre never touches. A re-centre inside `load` would scroll the window
    //! off the block `load` was asked for, and the next `WorldScene::update` recomputes the same index
    //! from the re-centred viewpoint before anything is drawn. Stations: 1,089 flycam points on a 12 m
    //! grid that includes the exact 24 m and 192 m lines, and body placements across a block boundary.
    //! Fixture: the retail dats on a software device; fails without the dats or a device.

    use super::{block_at, sim, stand, store, BLOCK, CELL};
    use dereth_client::world::SceneWrites;
    use std::sync::Arc;

    use dereth_client::world::{block_xy, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
    use dereth_dat::RetailDatStore;
    use dereth_primitives::Vec3;
    use dereth_render::device::Gpu;

    fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
        let mut scene =
            WorldScene::load(store, gpu, SceneConfig::default()).expect("the scene loads");
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        scene
            .attach_character(store, &region, gpu)
            .expect("the body is created");
        scene
    }

    // ---------------------------------------------------------------------------------------------
    // The counterfactual: what `update_viewer_cell` would answer if `recenter` had run first
    // ---------------------------------------------------------------------------------------------

    /// **`recenter`'s two lines, transcribed here**, applied to the bodiless flycam, followed by
    /// `viewpoint_cell_id`'s own `None` arm and draw-order calculation's `& 7`.
    ///
    /// The block moves by `floor(p / 192)` and the camera position moves by the same shift in metres,
    /// which is exactly what `WorldScene::recenter` does. Built in this file rather than by calling
    /// the private function, so that the comparison below is between two derivations and not between
    /// a function and itself.
    fn masked_after_a_hypothetical_recentre(scene: &WorldScene) -> Option<(u8, u8)> {
        use dereth_physics::landdefs as ld;
        let (bx, by) = scene.viewer_block()?;
        let p = scene.camera.position;
        let dx = dereth_primitives::num::floor_to_i32(p.x / BLOCK);
        let dy = dereth_primitives::num::floor_to_i32(p.y / BLOCK);
        #[allow(clippy::cast_precision_loss)] // a block shift, at most 255
        let shifted = Vec3::new(p.x - dx as f32 * BLOCK, p.y - dy as f32 * BLOCK, p.z);
        let id = ld::get_outside_cell_id(ld::lcoord_to_gid((bx + dx) * 8, (by + dy) * 8), shifted);
        let (x, y) = ld::gid_to_lcoord(id)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: `& 7` bounds both to 0..=7. Not a float conversion.
        Some(((x & 7) as u8, (y & 7) as u8))
    }

    // ---------------------------------------------------------------------------------------------
    // 1. The re-centre cannot change the answer, bodiless: the flycam arm, over a grid that reaches
    //    the boundaries
    // ---------------------------------------------------------------------------------------------

    /// **1,089 stations, and the two derivations agree on every one.**
    ///
    /// The grid is 33 x 33 positions from two blocks south-west of the window's own block to two
    /// blocks north-east of it, stepped by 12 m — half a cell — so it lands **exactly** on every 24 m
    /// cell line and every 192 m block line in that range as well as between them. Both matter, and
    /// for opposite reasons: the exact lines are where a `floor` is
    /// one ULP from flipping and the shift `p - 192 d` could have moved it, and the interior points
    /// are where an implementation that *always* saturated would still agree.
    ///
    /// The premise is asserted rather than implied: the stations must actually leave the centre block
    /// — otherwise the re-centre is a no-op at every one of them and the agreement means nothing — and
    /// the reported index must actually move around, not sit on one value.
    #[test]
    fn a_re_centre_cannot_change_the_flycams_cell_index() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        // No body: `WorldScene::load` alone, which is the `--no-character` path and the state every
        // scene passes through before `attach_character` runs.
        let mut scene =
            WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
        let home = scene
            .viewer_block()
            .expect("a loaded scene has a viewer block");

        let (mut stations, mut outside, mut on_a_cell_line, mut on_a_block_line) = (0, 0, 0, 0);
        let mut seen = std::collections::BTreeSet::new();
        let z = scene.camera.position.z;
        for ix in 0..33i32 {
            for iy in 0..33i32 {
                #[allow(clippy::cast_precision_loss)] // 0..=32
                let p = Vec3::new(
                    (ix as f32).mul_add(CELL * 0.5, -2.0 * BLOCK),
                    (iy as f32).mul_add(CELL * 0.5, -2.0 * BLOCK),
                    z,
                );
                scene.camera.position = p;
                // `follow_character_now` on a bodiless scene *is* `update_viewer_cell`:
                // `follow_character` returns at its first line with no `Character`.
                scene.follow_character_now();
                let got = scene.viewer_draw_cell();
                let want = masked_after_a_hypothetical_recentre(&scene)
                    .expect("every station on this grid resolves to a land cell");
                assert_eq!(
                got, want,
                "at {p:?} the un-recentred arm says {got:?} and a re-centre first would have said \
                 {want:?}: the cell index is NOT block-independent"
            );
                stations += 1;
                seen.insert(got);
                if p.x < 0.0 || p.x >= BLOCK || p.y < 0.0 || p.y >= BLOCK {
                    outside += 1;
                }
                if (p.x / CELL).fract() == 0.0 && (p.y / CELL).fract() == 0.0 {
                    on_a_cell_line += 1;
                }
                if (p.x / BLOCK).fract() == 0.0 && (p.y / BLOCK).fract() == 0.0 {
                    on_a_block_line += 1;
                }
                // The window must not have moved: this arm does not re-centre, which is the pin
                // `neither_load_time_arm_re_centres` states and what makes the counterfactual one.
                assert_eq!(
                    scene.viewer_block(),
                    Some(home),
                    "the flycam arm re-centred the window"
                );
            }
        }
        scene.release_textures(&mut gpu);
        eprintln!(
        "grid: {stations} flycam stations, {outside} outside the centre block, {on_a_cell_line} \
         exactly on a 24 m line, {on_a_block_line} exactly on a 192 m line; {} distinct SqCoords; \
         the two derivations agree at every one",
        seen.len()
    );
        assert_eq!(stations, 1_089, "the grid");
        // The premise, in three parts.
        assert!(
            outside > 900,
            "only {outside} stations left the centre block"
        );
        assert_eq!(
            on_a_cell_line, 289,
            "17 x 17 of the grid's points sit exactly on a 24 m line"
        );
        assert_eq!(
            on_a_block_line, 9,
            "3 x 3 of them sit exactly on a 192 m line"
        );
        assert_eq!(
            seen.len(),
            64,
            "the index must range over all 64 SqCoords, not sit on one"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // 2. The re-centre cannot change the answer, embodied: the arm that has no block in it at all
    // ---------------------------------------------------------------------------------------------

    /// **With a body the derivation does not read the viewer block, so the re-centre is not even in
    /// the expression.**
    ///
    /// `viewpoint_cell_id`'s `Some(c)` arm takes `Character::camera.viewer` — a `Position`, a cell id
    /// and a cell-local frame — and `recenter` writes `self.camera.position`, `self.window` and
    /// `Character::viewer_block`, none of which appears in it. This asserts that structurally, on a
    /// frame that really does re-centre: the viewpoint `Position` is **bit-identical** across the
    /// crossing and the SqCoord it produces is unchanged by it.
    ///
    /// The premise is the crossing itself. Without it the test would pass on a frame where `recenter`
    /// did nothing, which is the shape of a differential test blind to the
    /// subject being absent from both arms.
    #[test]
    fn the_bodys_viewpoint_is_untouched_by_the_re_centre() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let mut scene = embodied(&store, &mut gpu);

        // Stand the body one block north of the window's block, so the *next* `update` must re-centre.
        stand(
            &mut scene,
            block_at(hx, hy).cell(1),
            Vec3::new(96.0, 96.0, 0.0),
        );
        for i in 1..=10 {
            sim(&mut scene, f64::from(i) / 30.0);
            scene
                .stream(&store, &mut gpu)
                .expect("the streamed blocks build");
        }
        let settled = scene.viewer_block().expect("a viewer block");
        stand(
            &mut scene,
            block_at(hx, hy + 1).cell(1),
            Vec3::new(96.0, 96.0, 0.0),
        );

        let viewer_before = scene.character.as_ref().expect("a body").camera.viewer;
        scene.follow_character_now();
        let cell_before = scene.viewer_draw_cell();
        let block_before = scene.viewer_block().expect("a viewer block");

        sim(&mut scene, 11.0 / 30.0);
        let viewer_after = scene.character.as_ref().expect("a body").camera.viewer;
        let cell_after = scene.viewer_draw_cell();
        let block_after = scene.viewer_block().expect("a viewer block");
        scene.release_textures(&mut gpu);

        eprintln!(
            "embodied: block {block_before:?} -> {block_after:?} (settled {settled:?}); viewer \
         cell id {:#010X} -> {:#010X}; SqCoord {cell_before:?} -> {cell_after:?}",
            viewer_before.cell.0, viewer_after.cell.0
        );

        // The premise: the update really did re-centre.
        assert_ne!(
            block_before, block_after,
            "the window did not move, so this frame does not exercise the re-centre at all"
        );
        // The mechanism: `recenter` writes none of the fields this derivation reads.
        assert_eq!(
            viewer_before.cell, viewer_after.cell,
            "the re-centre changed the viewpoint's cell id, which it has no business touching"
        );
        assert_eq!(
            viewer_before.frame.origin, viewer_after.frame.origin,
            "the re-centre moved the viewpoint's cell-local origin"
        );
        // ...and therefore the answer.
        assert_eq!(
            cell_before, cell_after,
            "the SqCoord moved across a re-centre that did not move the viewpoint"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // 3. What a re-centre inside `WorldScene::load` would actually cost
    // ---------------------------------------------------------------------------------------------

    /// **A re-centre on `load`'s `update_viewer_cell` line would scroll the window one block south, off
    /// the block `load` was asked to build.**
    ///
    /// `WorldScene::load` places the flycam at `(0.5 * 192, -0.35 * 192)` — 67.2 m **south** of
    /// the centre block, deliberately, so the whole block is in frame — and `floor(-67.2 / 192)` is
    /// `-1`. So `recenter` there would choose `(bx, by - 1)`, discarding the block this function has
    /// just streamed and just checked `WorldError::NoSuchLandblock(cfg.landblock)` against, and would
    /// hand its caller a scene centred on a landblock the caller did not ask for.
    ///
    /// That is a change to what `load` *is*, in exchange for a cell index that
    /// [`a_re_centre_cannot_change_the_flycams_cell_index`] measures as identical either way. The
    /// premise and the cost are both asserted, so this reads as a measurement of the trade rather than
    /// as an argument for one side of it.
    #[test]
    fn a_re_centre_at_load_would_scroll_off_the_block_load_was_asked_for() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let cfg = SceneConfig::default();
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");

        let asked_for = block_xy(cfg.landblock);
        let centred_on = scene.viewer_block().expect("a viewer block");
        let p = scene.camera.position;
        let dx = dereth_primitives::num::floor_to_i32(p.x / BLOCK);
        let dy = dereth_primitives::num::floor_to_i32(p.y / BLOCK);
        let would_choose = (centred_on.0 + dx, centred_on.1 + dy);
        let cell = scene.viewer_draw_cell();
        eprintln!(
            "at load: asked for {asked_for:?}, centred on {centred_on:?}, flycam at \
         ({:.2}, {:.2}) = {dx},{dy} blocks; a recenter here would choose {would_choose:?}; the \
         SqCoord is {cell:?} either way",
            p.x, p.y
        );

        // The premise: `load` centres on the block it was asked for, and the flycam is outside it.
        assert_eq!(
            centred_on, asked_for,
            "load did not centre on cfg.landblock"
        );
        assert_eq!(
            (dx, dy),
            (0, -1),
            "load's camera placement has moved; re-derive this station"
        );
        // The cost.
        assert_ne!(
            would_choose, asked_for,
            "a re-centre at load would leave the window on the block load was asked for, so it \
         costs nothing and this test is the wrong shape"
        );
        // ...and the benefit, measured: none. The index is the same on either side of the shift.
        assert_eq!(
            Some(cell),
            masked_after_a_hypothetical_recentre(&scene),
            "the re-centre would have changed the index after all"
        );
        scene.release_textures(&mut gpu);
    }

    // ---------------------------------------------------------------------------------------------
    // 4. The pin itself, and the supersession
    // ---------------------------------------------------------------------------------------------

    /// Behaviour: world.viewpoint.the-load-time-arms-do-not-recentre
    ///
    /// **Neither load-time arm re-centres.** This is the pin, stated so it is falsifiable: a later
    /// change that quietly makes either of them re-centre reddens here rather than passing unnoticed.
    #[test]
    fn neither_load_time_arm_re_centres() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);

        // Arm 1 — `WorldScene::load`. Its flycam is a block south and the window stays put.
        let mut scene =
            WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
        assert_eq!(
            scene.viewer_block(),
            Some(block_xy(DEFAULT_LANDBLOCK)),
            "WorldScene::load re-centred the window off cfg.landblock"
        );
        scene.release_textures(&mut gpu);

        // Arm 2 — `follow_character_now`, with the body a whole block away from the window's block.
        let mut scene = embodied(&store, &mut gpu);
        for i in 1..=10 {
            sim(&mut scene, f64::from(i) / 30.0);
            scene
                .stream(&store, &mut gpu)
                .expect("the streamed blocks build");
        }
        let before = scene.viewer_block().expect("a viewer block");
        stand(
            &mut scene,
            block_at(hx, hy + 1).cell(1),
            Vec3::new(96.0, 96.0, 0.0),
        );
        scene.follow_character_now();
        let after = scene.viewer_block().expect("a viewer block");
        // The premise: the body really is outside the window's block, so a re-centring
        // `follow_character_now` would have moved it.
        let p = scene
            .character
            .as_ref()
            .expect("a body")
            .viewer_render_frame()
            .expect("a viewpoint");
        assert!(
        p.origin.y >= BLOCK || p.origin.y < 0.0 || p.origin.x >= BLOCK || p.origin.x < 0.0,
        "the body is at {:?}, inside the window's block: a re-centre would be a no-op here and \
         this pin proves nothing",
        p.origin
    );
        assert_eq!(after, before, "follow_character_now re-centred the window");
        // And the next `update` is what does re-centre, so the pin is about *this call*, not about
        // the frame loop.
        sim(&mut scene, 11.0 / 30.0);
        assert_ne!(
        scene.viewer_block(),
        Some(before),
        "the frame loop's own recenter did not run either -- this is no longer a statement about \
         follow_character_now"
    );
        scene.release_textures(&mut gpu);
    }

    /// **The load-time value is recomputed before anything is drawn — and comes out the same.**
    ///
    /// `App::frame` runs `player_teleport_use_time` and `load_pending_scene` inside its
    /// `WorldViewStep` step and `world.update` later in that same step, and `DrawWorld` is a
    /// later step still (`dereth_client::frame::FrameStep::ORDER`, asserted by the frame module's unit
    /// tests). So
    /// whatever a load-time arm leaves in `viewer_cell` is overwritten by the ordered pair
    /// `recenter(); ..; update_viewer_cell()` before the frame is drawn.
    ///
    /// That alone would only say the question does not matter *because nobody reads the answer*. The
    /// second assertion is the claim: the ordered pair recomputes the **same index**. So the two arms
    /// are not merely harmless — they agree with the frame loop, which is the invariance observed on
    /// the shipped path rather than on a constructed grid.
    #[test]
    fn the_next_update_recomputes_the_same_index_from_the_re_centred_viewpoint() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let mut scene = embodied(&store, &mut gpu);
        for i in 1..=10 {
            sim(&mut scene, f64::from(i) / 30.0);
            scene
                .stream(&store, &mut gpu)
                .expect("the streamed blocks build");
        }

        // Four destinations, in and out of the window's block, on and off the 24 m lines — the same
        // pairing the grid above uses.
        let mut checked = 0usize;
        let mut crossings = 0usize;
        for (n, (bx, by, o)) in [
            (hx, hy + 1, Vec3::new(96.0, 96.0, 0.0)),
            (hx, hy, Vec3::new(96.0, -67.2, 0.0)),
            (hx + 1, hy, Vec3::new(4.0, 100.0, 0.0)),
            (hx, hy, Vec3::new(96.0, 96.0, 0.0)),
        ]
        .into_iter()
        .enumerate()
        {
            stand(&mut scene, block_at(bx, by).cell(1), o);
            let block_before = scene.viewer_block().expect("a viewer block");
            // The load-time arm, exactly as `App::load_pending_scene` and
            // `App::player_teleport_use_time` call it.
            scene.follow_character_now();
            let load_time = scene.viewer_draw_cell();
            // The frame loop's ordered pair, later in the same `App::frame` step.
            sim(&mut scene, (11.0 + n as f64) / 30.0);
            scene
                .stream(&store, &mut gpu)
                .expect("the streamed blocks build");
            let drawn = scene.viewer_draw_cell();
            let block_after = scene.viewer_block().expect("a viewer block");
            eprintln!(
                "station {n}: block {block_before:?} -> {block_after:?}; load-time SqCoord \
             {load_time:?}, drawn {drawn:?}"
            );
            assert_eq!(
            load_time, drawn,
            "station {n}: the load-time arm said {load_time:?} and the frame loop's own ordered \
             pair said {drawn:?}"
        );
            if block_before != block_after {
                crossings += 1;
            }
            checked += 1;
        }
        scene.release_textures(&mut gpu);
        assert_eq!(checked, 4, "four stations");
        // The premise: at least one of them actually made the frame loop re-centre, or every station
        // compared two calls that had nothing to disagree about.
        assert!(
            crossings >= 2,
            "only {crossings} of the four stations re-centred the window"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // 5. The premise the file rests on: what the frame loop's re-centre actually does
    // ---------------------------------------------------------------------------------------------

    /// **`recenter` normalises the viewpoint into the centre block, and the flycam scene reaches it on
    /// its very first `update`.**
    ///
    /// `WorldScene::update` runs `recenter` on the line before `update_viewer_cell` reads the
    /// viewpoint, so every drawn frame reads a viewpoint inside the centre block. A sign error in
    /// `recenter`'s camera shift is invisible to every other station here and to
    /// `world::landblock_recentre`: with a body `follow_character` rewrites `self.camera` from the
    /// body above the re-centre and `crate::camera::update_viewer` overwrites it again below
    /// `world.update`, so the **bodiless flycam is the only thing that can see it**.
    ///
    /// The station puts the flycam a whole block west *and* a fraction of a block south, so both
    /// axes shift and by different amounts; a sign error on either is then a position outside
    /// `[0, 192)`.
    ///
    /// It also records the other side of the load-time cost: **the `--no-character` scene
    /// re-centres one block south on frame 1 regardless**, because `WorldScene::load` deliberately
    /// parks the camera south of the block it was asked to build. So the window `load` leaves is not
    /// the window the first drawn frame uses — and by
    /// [`a_re_centre_cannot_change_the_flycams_cell_index`] the SqCoord is the same across that move.
    #[test]
    fn the_frame_loops_re_centre_normalises_the_flycam_into_the_centre_block() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let mut scene =
            WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
        let home = scene.viewer_block().expect("a viewer block");

        // A whole block west and 0.35 of a block south of the block the window is centred on.
        let p0 = Vec3::new(96.0 - BLOCK, -0.35 * BLOCK, scene.camera.position.z);
        scene.camera.position = p0;
        let cell_before = {
            scene.follow_character_now();
            scene.viewer_draw_cell()
        };
        // The premise: both axes are outside the centre block, and by different whole numbers of
        // blocks, so a sign error on either one is visible and they cannot mask each other.
        let dx = dereth_primitives::num::floor_to_i32(p0.x / BLOCK);
        let dy = dereth_primitives::num::floor_to_i32(p0.y / BLOCK);
        assert_eq!(
            (dx, dy),
            (-1, -1),
            "the station does not straddle both axes"
        );

        sim(&mut scene, 1.0 / 30.0);
        let p1 = scene.camera.position;
        let block = scene.viewer_block().expect("a viewer block");
        let cell_after = scene.viewer_draw_cell();
        scene.release_textures(&mut gpu);
        eprintln!(
            "re-centre: block {home:?} -> {block:?}; flycam ({:.2}, {:.2}) -> ({:.2}, {:.2}); \
         SqCoord {cell_before:?} -> {cell_after:?}",
            p0.x, p0.y, p1.x, p1.y
        );

        // The window followed the viewpoint.
        assert_eq!(
            block,
            (home.0 + dx, home.1 + dy),
            "the window did not follow the flycam"
        );
        // **And the viewpoint is now inside the centre block**, which is the sentence being pinned.
        // Exact values, not just a range: the shift is a whole number of blocks and `192 k` is exactly
        // representable, so `96 - 192 + 192` is `96` and `-67.2 + 192` is the float nearest `124.8`.
        assert_eq!(
            p1.x, 96.0,
            "the x shift did not put the flycam back in the centre block"
        );
        assert_eq!(
            p1.y,
            (-0.35f32).mul_add(BLOCK, BLOCK),
            "the y shift is wrong"
        );
        assert!(
            (0.0..BLOCK).contains(&p1.x) && (0.0..BLOCK).contains(&p1.y),
            "the re-centred viewpoint {p1:?} is still outside [0, 192)"
        );
        // ...and the index the draw order is built from did not move: the invariance again, on the
        // shipped path rather than on the grid.
        assert_eq!(
            cell_before, cell_after,
            "the re-centre moved the SqCoord after all"
        );
    }
}
