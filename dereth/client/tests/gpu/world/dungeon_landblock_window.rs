//! An indoor viewer whose cell has not seen the outdoors does not move the landblock window. Retail
//! updates the landscape viewpoint from the viewer's cell id outdoors, from its adjusted outside
//! cell id when the interior cell has `seen_outside`, and otherwise keeps the viewer cell and block
//! draw list it had. A dungeon cell's position is in the dungeon's own layout space and routinely
//! leaves the block's `[0, 192]²` (87 % of the dat's interior cells; block `0x8602` runs from
//! `y = -250` to `0`), so a window re-derived from `floor(origin / 192)` walks off the block whose
//! cells are being drawn and the rooms draw nothing. Each station is a two-arm differential on
//! [`SceneConfig::indoor_viewpoint_gate`]: three training-academy rooms (`0x8602_0102`,
//! `0x8602_0110`, `0x8602_0120`), an outdoor crossing, and the `seen_outside` cell `0x2E6C_0381`.
//! Fixture: the retail dats and a census of their cells; fails without them or a device.
//!
//! Sections of this module:
//! * `teleport_into_a_dungeon`: a teleport into a dungeon from another landblock brings the window
//!   to the dungeon's own block.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_assets::{Decode, EnvCell};
use dereth_client::character::CharacterInput;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;

const W: u32 = 400;
const H: u32 = 300;

/// The training academy: a dungeon block whose cells sit south of the block's own origin.
const ACADEMY: u16 = 0x8602;

/// Three of the academy's rooms that draw nothing when the window re-centres. All three place
/// their cells near `y = -230`, two landblocks south of their own block's origin in raw
/// coordinates.
const DARK_ROOMS: [u32; 3] = [0x8602_0102, 0x8602_0110, 0x8602_0120];

/// A cell that is **both** outside its block's box and `seen_outside` — one of only 222 in the
/// whole dat, and the control that says the gate reads `seen_outside` and not "indoors".
/// Origin `(-8.0, 97.0, 36.4)`, so `floor(x / 192) == -1` and the ungated re-centre fires.
const SEEN_OUTSIDE_CELL: u32 = 0x2E6C_0381;
const SEEN_OUTSIDE_BLOCK: u16 = 0x2E6C;

fn warp() -> Gpu {
    crate::common::software_gpu(W, H)
}

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn block_xy(b: u16) -> (i32, i32) {
    (i32::from(b >> 8), i32::from(b & 0xFF))
}

/// One station: start in `cell`, update and stream the scene, draw it, and report the painted
/// pixel count and final window block. This harness does not run the swept-camera update.
fn station(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    landblock: u16,
    cell: u32,
    gate: bool,
) -> (usize, Option<(i32, i32)>) {
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock,
        start_cell: Some(CellId(cell)),
        time_of_day: Some(0.1),
        indoor_viewpoint_gate: gate,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    let mut stream = ObjectStream::new();
    let mut painted = 0;
    for i in 0..6 {
        let t = f64::from(i + 1) * dereth_client::app::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(t),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        let rgba = gpu.capture().expect("capture").to_rgba();
        painted = rgba
            .chunks_exact(4)
            .filter(|p| p[0] | p[1] | p[2] != 0)
            .count();
    }
    (painted, scene.viewer_block())
}

/// **A dungeon's layout space is authored outside its block's box, and `seen_outside` cells
/// almost never are.** A census over every interior cell of the retail cell dat.
#[test]
fn a_dungeons_layout_space_is_authored_outside_its_block() {
    let store = store();
    let ids = store.ids_of(DbType::Cell);
    let (mut interior, mut outside, mut seen, mut both) = (0u32, 0u32, 0u32, 0u32);
    // The two populations' worst overhang past the block box, kept apart: the whole claim is that
    // "a dungeon's layout space" and "a cell that has seen the outdoors" are different things.
    let (mut worst_seen, mut worst_unseen) = (0f32, 0f32);
    let mut blocks_outside = std::collections::BTreeSet::new();
    for id in &ids {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        let Ok(bytes) = store.read_typed(DbType::Cell, *id) else {
            continue;
        };
        let Ok(c) = EnvCell::decode_payload(*id, &bytes) else {
            continue;
        };
        interior += 1;
        let o = c.frame.origin;
        let so = c.flags & 1 != 0;
        let out = o.x < 0.0 || o.x > 192.0 || o.y < 0.0 || o.y > 192.0;
        if so {
            seen += 1;
        }
        if out {
            outside += 1;
            #[allow(clippy::cast_possible_truncation)]
            blocks_outside.insert((id.0 >> 16) as u16);
            // How far outside the box this one actually is.
            let d = (-o.x).max(o.x - 192.0).max((-o.y).max(o.y - 192.0));
            if so {
                both += 1;
                worst_seen = worst_seen.max(d);
            } else {
                worst_unseen = worst_unseen.max(d);
            }
        }
    }
    eprintln!(
        "census: {interior} interior cells; {outside} placed outside their block's [0,192]^2 \
         over {} landblocks; seen_outside set on {seen}; both outside AND seen_outside: {both}. \
         Worst overhang past the box: {worst_unseen:.1} m without seen_outside, \
         {worst_seen:.1} m with it",
        blocks_outside.len()
    );
    // The premise: dungeons really are authored this way, and it is the norm rather than a fault.
    assert!(
        interior > 500_000,
        "only {interior} interior cells -- the dat did not open"
    );
    assert!(
        outside * 2 > interior,
        "only {outside} of {interior} interior cells leave the block box; the claim that this is \
         normal authored layout does not hold and the placement should be suspected instead"
    );
    assert!(
        blocks_outside.len() > 1000,
        "only {} such landblocks",
        blocks_outside.len()
    );
    // The other direction: `seen_outside` and "outside the box" are all but disjoint, which is
    // what makes seen_outside the rendering predicate to key the re-centre on.
    assert!(
        seen > 0,
        "no cell has seen_outside, so the gate's other arm is untested everywhere"
    );
    assert!(
        both * 1000 < outside,
        "{both} of {outside} out-of-box cells are also seen_outside -- more than a rounding \
         fraction, so freezing the window on the gate would strand them"
    );
    // And they are a different *kind* of thing, measured rather than asserted at a magic number:
    // a layout space runs far further out of the box than any cell that has seen the outdoors.
    assert!(
        worst_seen * 2.0 < worst_unseen,
        "the worst seen_outside overhang is {worst_seen:.1} m against {worst_unseen:.1} m without \
         it -- the two populations are not separable, and keying the re-centre on `seen_outside` \
         would need a different argument"
    );
}

/// Behaviour: world.window.an-indoor-viewer-that-never-sees-outside-keeps-the-window-on-its-block
///
/// **The academy's rooms draw, and the window stays on their own block.** The ungated arm walks
/// the window off the block and paints almost nothing; the gated arm keeps it and fills the frame.
#[test]
fn the_academy_rooms_draw_and_the_window_stays_on_their_own_block() {
    let store = store();
    let mut gpu = warp();
    let own = block_xy(ACADEMY);
    for cell in DARK_ROOMS {
        let (before, wb_before) = station(&store, &mut gpu, ACADEMY, cell, false);
        let (after, wb_after) = station(&store, &mut gpu, ACADEMY, cell, true);
        eprintln!(
            "{cell:#010X}: painted {before} -> {after} of {}; window {wb_before:?} -> \
             {wb_after:?} (own block {own:?})",
            (W * H) as usize
        );
        // The premise: the ungated arm really does lose the block, and really does draw nothing.
        assert_ne!(
            wb_before,
            Some(own),
            "the ungated arm already kept the window on {own:?}, so this station does not \
             reach the off-block window"
        );
        assert!(
            before * 20 < (W * H) as usize,
            "the ungated arm painted {before} pixels; the room was not blank, so the differential \
             is measuring something else"
        );
        // The claim, in both halves: the window is the cell's own block, and the room is on screen.
        assert_eq!(
            wb_after,
            Some(own),
            "the gated arm still walked the window off {own:?}"
        );
        assert!(
            after * 4 > (W * H) as usize,
            "the gated arm painted only {after} pixels; the window is right but the room is not \
             being drawn"
        );
    }
}

/// The control that keeps the gate honest outdoors: an **outdoor** viewer crossing a landblock
/// boundary still re-centres, identically on both arms.
#[test]
fn an_outdoor_viewer_still_re_centres() {
    let store = store();
    let mut gpu = warp();
    let mut run = |gate: bool| -> (Option<(i32, i32)>, Option<(i32, i32)>) {
        let region = dereth_client::world::load_region(&store).expect("the region decodes");
        let cfg = SceneConfig {
            indoor_viewpoint_gate: gate,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        let home = dereth_client::world::DEFAULT_LANDBLOCK;
        let (hx, hy) = block_xy(home);
        let sim = |scene: &mut WorldScene, n: u32| {
            for i in 0..n {
                scene.update(
                    dereth_client::camera::CameraInput::default(),
                    CharacterInput::default(),
                    LocalTime(f64::from(i + 1) / 30.0),
                    0.0,
                );
            }
        };
        sim(&mut scene, 10);
        let start = scene.viewer_block();
        // The body's own landblock, one block north — an ordinary outdoor position, taken through
        // `Character::teleport`'s outside-cell coordinate adjustment, matching retail's
        // player-create handling of the server position.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let north = CellId((u32::from(((hx as u16) << 8) | ((hy + 1) as u16)) << 16) | 1);
        scene
            .character
            .as_mut()
            .expect("a body")
            .teleport(Position::new(
                north,
                Frame::new(Vec3::new(96.0, 96.0, 0.0), Quat::IDENTITY),
            ));
        sim(&mut scene, 10);
        (start, scene.viewer_block())
    };
    let (a0, a1) = run(false);
    let (b0, b1) = run(true);
    eprintln!("outdoor control: ungated {a0:?} -> {a1:?}; gated {b0:?} -> {b1:?}");
    // Non-vacuity: the window really did move, so "the two agree" is not two frozen windows.
    assert_ne!(
        a0, a1,
        "the ungated arm did not re-centre outdoors, so this control is vacuous"
    );
    assert_eq!(
        (a0, a1),
        (b0, b1),
        "the gate changed an outdoor viewer's window"
    );
}

/// The control that says the gate reads **`seen_outside`**, not "indoors": an interior cell that
/// *has* seen the outdoors still re-centres, exactly as the ungated arm does.
#[test]
fn the_gate_is_seen_outside_and_not_merely_indoors() {
    let store = store();
    let mut gpu = warp();
    // The premise, off the dat: this cell is interior, outside its block's box, and seen_outside.
    let id = dereth_primitives::DataId(SEEN_OUTSIDE_CELL);
    let bytes = store
        .read_typed(DbType::Cell, id)
        .expect("the cell is in the dat");
    let cell = EnvCell::decode_payload(id, &bytes).expect("it decodes");
    let o = cell.frame.origin;
    eprintln!(
        "seen_outside station {SEEN_OUTSIDE_CELL:#010X}: origin ({:.1},{:.1},{:.1}), \
         flags {:#x}, seen_outside {}",
        o.x,
        o.y,
        o.z,
        cell.flags,
        cell.flags & 1 != 0
    );
    assert!(
        cell.flags & 1 != 0,
        "the station's whole point is that seen_outside is set"
    );
    assert!(
        o.x < 0.0 || o.x > 192.0 || o.y < 0.0 || o.y > 192.0,
        "and that it leaves the box"
    );

    let mut run = |gate: bool| -> Option<(i32, i32)> {
        let region = dereth_client::world::load_region(&store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: SEEN_OUTSIDE_BLOCK,
            start_cell: Some(CellId(SEEN_OUTSIDE_CELL)),
            indoor_viewpoint_gate: gate,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        for i in 0..8 {
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(f64::from(i + 1) / 30.0),
                0.0,
            );
        }
        scene.viewer_block()
    };
    let before = run(false);
    let after = run(true);
    let own = block_xy(SEEN_OUTSIDE_BLOCK);
    eprintln!("seen_outside control: ungated {before:?}; gated {after:?} (own block {own:?})");
    // Non-vacuity: the ungated re-centre really fires at this station.
    assert_ne!(
        before,
        Some(own),
        "the ungated arm did not re-centre here; the control is vacuous"
    );
    assert_eq!(
        after, before,
        "the gate froze the window for a `seen_outside` cell -- it is keyed on 'indoors' rather \
         than on whether that interior cell has seen the outdoors"
    );
}

// ---------------------------------------------------------------------------------------------
// A teleport into a dungeon brings the window with it
// ---------------------------------------------------------------------------------------------

mod teleport_into_a_dungeon {
    //! Teleporting into a dungeon from another landblock brings the streaming window to the
    //! destination cell's own landblock. Retail's landscape viewpoint update takes a **cell id** and
    //! treats a change in its top 16 bits as the landblock change; a window derived from the origin,
    //! `floor(origin / 192)`, puts a dungeon authored outside its block's box one or more blocks away,
    //! and past `SceneConfig::land_radius` the dungeon's own block leaves the window entirely. Station:
    //! Drudge Hideout `0x019E_0114`, entered by teleport from Holtburg `0xA9B4`, as a two-arm
    //! differential on [`SceneConfig::indoor_viewpoint_gate`] checked against a login inside it, with
    //! the training academy's rooms as the control.
    //! Fixture: the retail dats on a software device; fails without the dats or a device.

    use super::{block_xy, store, warp, H, W};
    use dereth_assets::Decode;
    use dereth_client::character::CharacterInput;
    use dereth_client::env_cells::{physics_geometry, EnvCellLoader};
    use dereth_client::objects::ObjectStream;
    use dereth_client::world::{SceneConfig, WorldScene};
    use dereth_dat::{DbType, RetailDatStore};
    use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
    use dereth_render::device::Gpu;
    use std::sync::Arc;

    /// **Drudge Hideout.** `weenie 2068`'s destination in the ACE world database is
    /// `0x019E0114 @ (10, -40, 0)` -- a dungeon, on its own landblock, authored at `y = -40` and so
    /// one of the 642,001 interior cells outside their block's `[0, 192]^2`.
    const DRUDGE_BLOCK: u16 = 0x019E;
    const DRUDGE_CELL: u32 = 0x019E_0114;

    /// The block the scene is *loaded* on, standing in for wherever the portal was used from. Any
    /// outdoor block other than the dungeon's does; this one is the usual Holtburg station.
    const SURFACE: u16 = 0xA9B4;

    /// The training academy rooms `world::dungeon_landblock_window` measures. They are the
    /// **control**: a blanket "always re-centre from the origin" loses their correct cell-derived
    /// window as well.
    const ACADEMY_BLOCK: u16 = 0x8602;
    const ACADEMY_ROOMS: [u32; 3] = [0x8602_0102, 0x8602_0110, 0x8602_0120];

    fn painted(rgba: &[u8]) -> usize {
        rgba.chunks_exact(4)
            .filter(|p| p[0] | p[1] | p[2] != 0)
            .count()
    }

    /// A non-solid point inside `cell`, searched over the cell's own extent. The same search
    /// `rendering::indoor_depth_clear` uses, and for the same reason: a dungeon's cells share frame
    /// origins that `WorldScene::standable_point`'s +/-6 m box does not cover.
    fn point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
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

    /// What one arrival looks like: paint the frame, and say where the window ended up.
    struct Arrival {
        painted: usize,
        window: Option<(i32, i32)>,
        settled: u32,
        /// `WorldScene::env_cell_counts` -- interior cells in the resident blocks, and meshes in the
        /// viewer's own cell. `(n, 0)` is `draw_inside` bailing at `cells.contains_key(&start.0)`.
        cells: (usize, usize),
        /// `WorldScene::indoor_traversal_counts` -- cells reached, and of those, cells with a mesh.
        traversal: (usize, usize),
        /// Where the body actually came to rest, in its cell's own frame, and the render-space
        /// viewpoint `traversal_cells` clips the portals against.
        origin: Vec3,
        eye: Vec3,
    }

    /// **Teleport in.** Load the scene on `from`, run the app's own frame order for three frames so
    /// the window is established there, then teleport the body into `cell` and run until the painted
    /// count stabilizes, subject to the 48-frame ceiling below.
    ///
    /// The indoor stations of `world::dungeon_landblock_window`, `rendering::indoor_depth_clear` and
    /// `world::interiors` name the destination's own landblock in `SceneConfig::landblock`, so the
    /// window is already correct before the first frame and `recenter` has nothing to do; this is the
    /// arrival shape where it does.
    fn arrive(
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        from: u16,
        cell: u32,
        gate: bool,
    ) -> Arrival {
        arrive_with(store, gpu, from, None, cell, gate)
    }

    /// **Log in inside.** With `SceneConfig::start_cell`, the scene is built on the destination's
    /// own landblock and the body is placed in it before the
    /// first frame. The control arrival: logging in inside the dungeon draws it.
    fn log_in(store: &Arc<RetailDatStore>, gpu: &mut Gpu, cell: u32) -> Arrival {
        #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
        let block = (cell >> 16) as u16;
        arrive_with(store, gpu, block, Some(cell), cell, true)
    }

    fn arrive_with(
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        from: u16,
        start_cell: Option<u32>,
        cell: u32,
        gate: bool,
    ) -> Arrival {
        arrive_cfg(
            store,
            gpu,
            from,
            start_cell,
            cell,
            SceneConfig {
                indoor_viewpoint_gate: gate,
                ..SceneConfig::default()
            },
        )
    }

    fn arrive_cfg(
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        from: u16,
        start_cell: Option<u32>,
        cell: u32,
        base: SceneConfig,
    ) -> Arrival {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: from,
            start_cell: start_cell.map(CellId),
            time_of_day: Some(0.35),
            ..base
        };
        let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, gpu)
            .expect("the body is created");
        let mut stream = ObjectStream::new();
        let mut now = 0.0f64;
        for _ in 0..3 {
            step(store, &mut scene, gpu, &mut stream, &mut now);
        }
        let p = point_in(store, cell).expect("the destination cell has a standable point");
        {
            // `App::apply_player_teleport_at`'s own sequence, not a bare `Character::teleport`: the
            // client's land source is prefetched per block, so the destination cell's physics geometry
            // must be present before the body's placement transition. Otherwise position validation
            // fails and the body keeps the raw origin it was handed.
            let c = scene.character.as_mut().expect("a body");
            #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
            c.land()
                .load_block_cells(dereth_primitives::LandblockId((cell >> 16) as u16));
            c.teleport(Position::new(CellId(cell), Frame::new(p, Quat::IDENTITY)));
        }
        // The window's streaming is incremental, so the frames after an arrival are counted until the
        // painted total settles rather than at a magic number. 48 is a ceiling, not a schedule.
        let mut rgba = Vec::new();
        let mut last = usize::MAX;
        let mut stable = 0;
        let mut trace: Vec<usize> = Vec::new();
        for i in 0..48 {
            rgba = step(store, &mut scene, gpu, &mut stream, &mut now);
            let p = painted(&rgba);
            trace.push(p);
            if p == last {
                stable += 1;
                if stable >= 6 && i >= 8 {
                    break;
                }
            } else {
                stable = 0;
            }
            last = p;
        }
        if std::env::var("DERETH_TEST_DUNGEON_WINDOW_TRACE").is_ok() {
            eprintln!("dungeon window trace {from:04X} start={start_cell:?}: {trace:?}");
        }
        // A raw RGBA dump for eyeballing, when a human is driving the suite.
        if let Ok(dir) = std::env::var("DERETH_TEST_DUNGEON_WINDOW_DUMP") {
            let tag = format!("{from:04X}_{}", start_cell.unwrap_or(0));
            std::fs::write(format!("{dir}/dungeon_window_{tag}.raw"), &rgba).expect("dump");
        }
        let settled = scene.character.as_ref().expect("a body").position().cell.0;
        Arrival {
            painted: painted(&rgba),
            window: scene.viewer_block(),
            settled,
            cells: scene.env_cell_counts(),
            traversal: scene.indoor_traversal_counts(),
            origin: scene
                .character
                .as_ref()
                .expect("a body")
                .position()
                .frame
                .origin,
            eye: scene.camera_position(),
        }
    }

    /// The app's own frame order -- `sync_objects`, `update`, camera update, `stream`, `reserve_upload_arena`,
    /// `begin_frame`, `draw`, `end_frame` -- so what is captured is the production path.
    fn step(
        store: &Arc<RetailDatStore>,
        scene: &mut WorldScene,
        gpu: &mut Gpu,
        stream: &mut ObjectStream,
        now: &mut f64,
    ) -> Vec<u8> {
        *now += dereth_client::app::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(*now),
            1.0 / 30.0,
        );
        // **`App::frame`'s next stage.** The camera update sweeps its sphere and sets
        // `CameraControl::viewer_cell`. Without it `camera.stats.sweeps == 0`, scene viewer-cell
        // selection falls back to the body, and `Arrival::eye` reports the debug chase camera instead.
        // This file tests the normal-mode branch on the **viewer's** cell; the body's cell would be
        // the wrong input.
        dereth_client::camera::update_viewer(
            scene,
            dereth_client::camera::CameraInput::default(),
            LocalTime(*now),
            dereth_client::app::HEADLESS_STEP,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    }

    /// The premise, with no GPU in it: Drudge Hideout is a real dungeon in the retail dats, its
    /// destination cell is authored outside the block box, and it has not `seen_outside` -- so
    /// retail's normal-mode branch for an indoor viewer without outside visibility applies.
    #[test]
    fn drudge_hideout_is_an_unseen_outside_dungeon_authored_outside_its_block() {
        let store = store();
        let cells = EnvCellLoader::new().load_block(&store, DRUDGE_BLOCK);
        assert!(
            !cells.is_empty(),
            "block {DRUDGE_BLOCK:#06X} has no interior cells; wrong dungeon"
        );
        let target = cells
            .iter()
            .find(|d| d.id.0 == DRUDGE_CELL)
            .expect("the portal's destination cell is in the dat");
        let o = target.cell.frame.origin;
        assert!(
        o.x < 0.0 || o.x > 192.0 || o.y < 0.0 || o.y > 192.0,
        "{DRUDGE_CELL:#010X} sits at ({:.1}, {:.1}) inside the block box, so the origin-derived \
         re-centre would land on the right block by accident and this station proves nothing",
        o.x,
        o.y
    );
        let bytes = store
            .read_typed(DbType::Cell, dereth_primitives::DataId(DRUDGE_CELL))
            .expect("cell read");
        let raw =
            dereth_assets::EnvCell::decode_payload(dereth_primitives::DataId(DRUDGE_CELL), &bytes)
                .expect("the cell decodes");
        assert_eq!(
            raw.flags & 1,
            0,
            "{DRUDGE_CELL:#010X} has seen_outside set, so the third arm is not the arm taken here"
        );
        eprintln!(
        "premise: {DRUDGE_CELL:#010X} at ({:.1}, {:.1}, {:.1}), {} cells in {DRUDGE_BLOCK:#06X}",
        o.x,
        o.y,
        o.z,
        cells.len()
    );
        // How far the origin-derived re-centre can throw the window inside this one dungeon:
        // `floor(origin / 192)` per cell, against `SceneConfig::land_radius` of 3.
        let mut worst = (0i32, 0i32, 0u32);
        for d in &cells {
            let o = d.cell.frame.origin;
            let (bx, by) = (
                dereth_primitives::num::floor_to_i32(o.x / 192.0),
                dereth_primitives::num::floor_to_i32(o.y / 192.0),
            );
            if bx.abs().max(by.abs()) > worst.0.abs().max(worst.1.abs()) {
                worst = (bx, by, d.id.0);
            }
        }
        eprintln!(
            "premise: worst origin-derived block shift inside {DRUDGE_BLOCK:#06X} is ({}, {}) at \
         {:#010X} -- land_radius is 3, so anything past 3 leaves the window entirely",
            worst.0, worst.1, worst.2
        );
    }

    /// Behaviour: world.window.a-teleport-into-a-dungeon-brings-the-landblock-window-with-it
    ///
    /// **Teleport into Drudge Hideout from another landblock and the streaming window follows the
    /// *cell id*, not the origin.**
    ///
    /// A two-arm differential on [`SceneConfig::indoor_viewpoint_gate`], the switch between the two
    /// *derivations*: cleared, the window's block is `floor(origin / 192)`; set, it is the landblock
    /// selected from the viewer's cell id, following retail's normal-mode landscape viewpoint update.
    /// The login arrival is the reference both halves are compared with.
    ///
    /// Drudge's own layout reaches `floor(origin / 192) == (0, -1)` at worst, and
    /// `SceneConfig::land_radius` is 3, so even the wrong window keeps `0x019E` resident and the
    /// dungeon still draws -- both arms reach the same 16 cells. The cost of the wrong window shows
    /// in the landblocks whose interior cells run up to **1,590 m** out of the box: past 3 blocks the
    /// dungeon's own block leaves the window and `draw_inside` stops at `cells.contains_key(&start.0)`.
    #[test]
    fn a_teleport_into_a_dungeon_brings_the_window_with_it() {
        let store = store();
        let mut gpu = warp();
        let own = block_xy(DRUDGE_BLOCK);
        let before = arrive(&store, &mut gpu, SURFACE, DRUDGE_CELL, false);
        let after = arrive(&store, &mut gpu, SURFACE, DRUDGE_CELL, true);
        // **The control**: the same cell entered by login, which draws the dungeon. Retail uses
        // separate entry and teleport paths, with placement flags 0x11 and 0x1012 respectively. This
        // harness uses `Character::teleport` for both arrivals.
        let login = log_in(&store, &mut gpu, DRUDGE_CELL);
        eprintln!(
        "drudge (own block {own:?}, {} pixels):
           teleport, origin-derived:  settled {:#010X} window {:?} cells {:?} traversal {:?}          painted {} origin {:?} eye {:?}
           teleport, cell-id-derived: settled {:#010X} window {:?} cells {:?} traversal {:?}          painted {} origin {:?} eye {:?}
           login:                     settled {:#010X} window {:?} cells {:?} traversal {:?}          painted {} origin {:?} eye {:?}",
        (W * H) as usize,
        before.settled, before.window, before.cells, before.traversal, before.painted,
        before.origin, before.eye,
        after.settled, after.window, after.cells, after.traversal, after.painted,
        after.origin, after.eye,
        login.settled, login.window, login.cells, login.traversal, login.painted,
        login.origin, login.eye,
    );
        // The premise: the body really did arrive in the dungeon on every arm.
        for (what, a) in [
            ("origin-derived", &before),
            ("cell-id-derived", &after),
            ("login", &login),
        ] {
            assert_eq!(
                a.settled >> 16,
                u32::from(DRUDGE_BLOCK),
                "the {what} arm's body settled in {:#010X}, not in the dungeon",
                a.settled
            );
            assert!(
                !dereth_physics::landdefs::is_outdoors(CellId(a.settled)),
                "the {what} arm's body settled outdoors in {:#010X}",
                a.settled
            );
        }
        // The premise: `floor(-40 / 192) == -1` puts the origin-derived window one block *south* of
        // the dungeon rather than on its own block. The radius-3 window still includes this dungeon;
        // this assertion detects incorrect centering, not a missing interior.
        assert_ne!(
            before.window,
            Some(own),
            "the origin-derived arm already kept the window on {own:?}, so this station does not \
         reach the off-block window"
        );
        // The claim, in both halves: the window is the cell's own block, and it is the block login
        // chooses, login being the arrival that draws the dungeon.
        assert_eq!(
            after.window,
            Some(own),
            "the cell-id-derived arm put the window on {:?}",
            after.window
        );
        assert_eq!(
            (after.window, after.cells, after.traversal),
            (login.window, login.cells, login.traversal),
            "the teleport arrival and the login arrival do not agree on the window, the resident \
         cells or the traversal"
        );
        // Non-vacuity: the traversal really reaches the viewer's own cell and really has geometry, so
        // "the two agree" is not two empty frames.
        assert!(
            after.traversal.0 > 1 && after.traversal.1 == after.traversal.0,
            "the traversal reached {:?} cells/meshed -- the station is not drawing an interior",
            after.traversal
        );
    }

    /// **The control that fails on a blanket change.** The academy rooms still draw.
    ///
    /// With unconditional origin-derived centering, block `0x8602`'s range from `y = -250` to `y = 0`
    /// moves the window as far as two blocks south of its cells. Keep the correct block and require
    /// a nontrivial painted image at each academy room; a blanket reversion is not a valid fix.
    #[test]
    fn the_academy_rooms_still_draw_when_the_scene_starts_on_their_block() {
        let store = store();
        let mut gpu = warp();
        let own = block_xy(ACADEMY_BLOCK);
        for cell in ACADEMY_ROOMS {
            let a = arrive(&store, &mut gpu, ACADEMY_BLOCK, cell, true);
            eprintln!(
                "academy {cell:#010X}: settled {:#010X}; window {:?} (own {own:?}); painted {}",
                a.settled, a.window, a.painted
            );
            assert_eq!(
                a.window,
                Some(own),
                "the window left the academy's own block for {:?} -- the gate is not holding it",
                a.window
            );
            assert!(
                a.painted * 20 > (W * H) as usize,
                "the academy room {cell:#010X} painted only {} pixels",
                a.painted
            );
        }
    }
}
