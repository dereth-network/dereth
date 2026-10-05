//! An interior cell's baked objects are drawn and are in the way: standing in the training dungeon
//! shows its furniture, and walking into a table is stopped by it (or, within the step height,
//! climbs onto it). Both halves are asserted **differentially**, against the same frame and the
//! same walk with `SceneConfig::cell_statics` clear, because "the frame changed" and "the frame
//! changed because the furniture is in it" are different claims, and so are "the body stopped" and
//! "the body stopped at the table".
//!
//! The training dungeon is landblock `0x8602`: the character-generation data (`0x0E000002`) names
//! starter area 0 "Holtburg"'s five instantiation cells as `0x860201AD`, `0x860301AD`,
//! `0x860401AD`, `0x870201AD` and `0x870301AD`, and ACE's `PlayerFactory.Create` places a new
//! character from that table. A dungeon door is not a cell static but a server weenie.
//!
//! Fixture: `client_cell_1.dat` and `client_portal.dat` (the environment-cell and setup decoders),
//! the training dungeon and Holtburg's dining rooms, on a software device. Fails without the dats.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::{Decode, Setup};
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::V3;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {
    dereth_world_data::env_cells::cell_statics, dereth_world_data::env_cells::physics_geometry,
    dereth_world_data::env_cells::EnvCellLoader,
};

/// The first instantiation cell of character-generation starter area 0, "Holtburg": the academy
/// a new Holtburg character wakes up in.
const TRAINING_DUNGEON: u16 = 0x8602;

/// One of Holtburg's dining rooms: it holds a `dinnertable` and a `chair`, both of them
/// environment-cell statics, and it is in the block every other test in this crate already builds.
const HOLTBURG_DINING_ROOM: u32 = 0xA9B4_0108;

/// `0x0200011F`. Named by ACE's `weenie` table, which has a `dinnertable` weenie whose
/// `PropertyDataId.Setup` is this id; the cell dat has no names of its own.
const DINNERTABLE: u32 = 0x0200_011F;

/// The Holtburg room the low static below stands in.
const HOLTBURG_LOW_STATIC_ROOM: u32 = 0xA9B4_0100;

/// `0x02000120` -- the static a player's step height says *can* be climbed. Its collision half
/// reaches **0.300 m** above its own origin against the dinnertable's **0.762 m**, so the two
/// straddle the player's 0.600 m step height, which is what makes them a pair.
const LOW_STATIC: u32 = 0x0200_0120;

/// The baseline player setup used before a server-provided replacement; the player's
/// step-up height is read directly from this DAT record.
const PLAYER_SETUP: u32 = 0x0200_0001;

/// How far the body's origin may drift upwards over a walk before the walk counts as a **climb**
/// rather than as settling. A settling body lands within the first ten frames and then holds z to
/// within a millimetre; 0.05 m is two orders above that and below the shortest static (0.143 m).
const CLIMB: f32 = 0.05;

/// One walk's reading, horizontal and vertical: without the **z** half a body that climbed a
/// static and one that walked through it are the same reading.
#[derive(Debug, Clone, Copy)]
struct Walk {
    /// The closest the body's origin came to a collision sphere's surface, in the horizontal
    /// plane.
    closest: f32,
    /// The furthest the body got from its start, in the horizontal plane.
    moved: f32,
    /// The lowest z the body held: the floor it settled onto.
    floor: f32,
    /// The highest z it reached **after** it had settled.
    peak: f32,
    /// How far the body fell from where `teleport` put it to the floor it settled on. It is not
    /// a claim about the world; it is the sampler proving it looked, because a z reading that is
    /// constant for a reason other than the body is a zero nothing can interpret.
    drop: f32,
    /// Where it ended.
    end_z: f32,
    /// The largest single-frame rise after the body had settled: one step-up lift.
    /// The object's step-up-height budget is spent **per step**, by passing it to one
    /// downward collision probe. A multi-frame climb is legal and an oversized single lift
    /// is not. Without this the two are the same number and the law below cannot be stated.
    max_lift: f32,
}

impl Walk {
    /// How far above the floor it settled onto the body ever got. A body stopped by a static
    /// reads ~0; a body that climbed onto one reads that static's own height.
    fn rise(self) -> f32 {
        self.peak - self.floor
    }

    /// What the body did, in the two dimensions this test now measures.
    fn verdict(self) -> &'static str {
        match (self.closest < 0.0, self.rise() > CLIMB) {
            (true, true) => "CLIMBED onto it",
            (true, false) => "PASSED THROUGH it at floor level",
            (false, true) => "rose without ever reaching it",
            (false, false) => "was stopped short of it",
        }
    }

    /// Did the static answer at all? Either the body was stopped short of it or the body was
    /// lifted onto it; the one thing a registered static must never do is nothing.
    ///
    /// In retail a player can walk onto a chair and from there onto a table (plain stairs are
    /// built the same way), so a climb is retail's behaviour. What is a defect is a body that goes
    /// **through** a static at floor level, which is the control's reading.
    fn answered(self) -> bool {
        self.closest > 0.0 || self.rise() > CLIMB
    }
}

/// One captured frame: its pixels, its size, the screen boxes of every cell-static triangle it
/// would issue, and how many batches those are.
type Shot = (Vec<u8>, u32, u32, Vec<(f32, f32, f32, f32)>, usize);

/// The retail store, or a failed test.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Drive the app's own per-frame order for `frames` frames and hand back the last capture.
///
/// Exercise the production scene/device calls in their frame order: `sync_objects`, `update`,
/// `stream`, then the frame bracket, as in the companion particle tests. This is not a full
/// application replay; the assertions observe the resulting scene, collision and pixels rather
/// than treating injected keystrokes or the existence of a test loop as evidence.
fn run(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    input: CharacterInput,
    frames: usize,
) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let mut rgba = Vec::new();
    for _ in 0..frames {
        t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            input,
            LocalTime(t),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    rgba
}

fn setup(store: &RetailDatStore, id: DataId) -> Option<Setup> {
    let b = store.read_typed(DbType::Setup, id).ok()?;
    Setup::decode_payload(id, &b).ok()
}

/// Where one static of a cell stands, from the dat.
fn placement(store: &RetailDatStore, cell: u32, want: u32) -> Option<Frame> {
    let mut loader = EnvCellLoader::new();
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    loader
        .load_block(store, block)
        .iter()
        .filter(|d| d.id.0 == cell)
        .flat_map(cell_statics)
        .find(|s| s.id.0 == want)
        .map(|s| s.frame)
}

// ---------------------------------------------------------------------------------------------
// 1. The data: what the cells bake in
// ---------------------------------------------------------------------------------------------

/// Oracle: `client_cell_1.dat`, through the environment-cell decoder, whose own gate is that all
/// 805,347 cell records consume their payload exactly.
///
/// The training dungeon's cells name several hundred baked objects. This asserts the two
/// carriers agree —
/// [`cell_statics`] and `EnvCellGeometry::static_objects` are two views of the same field and a
/// drift between them is how a consumer ends up reading the wrong one.
#[test]
fn the_training_dungeon_names_the_furniture_that_was_missing() {
    let store = store();
    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    assert_eq!(
        loader.stats.undecodable, 0,
        "an environment cell of the training dungeon would not decode"
    );
    assert_eq!(
        loader.stats.no_environment, 0,
        "a training-dungeon cell's environment record is missing"
    );
    assert!(
        cells.len() > 100,
        "only {} cells in the training dungeon",
        cells.len()
    );

    let mut placements = 0usize;
    let mut scripted = 0usize;
    let mut with_spheres = 0usize;
    for d in &cells {
        let statics = cell_statics(d);
        // The two carriers of the same field.
        let geom = physics_geometry(d);
        assert_eq!(
            statics.len(),
            geom.static_objects
                .iter()
                .filter(|(id, _)| id.0 != 0)
                .count(),
            "cell {:#010X}: cell_statics and EnvCellGeometry::static_objects disagree",
            d.id.0
        );
        placements += statics.len();
        for s in &statics {
            assert_eq!(s.cell, d.id);
            if s.id.0 >> 24 != 0x02 {
                continue;
            }
            let Some(setup) = setup(&store, s.id) else {
                continue;
            };
            if setup.default_script_id != DataId(0) {
                scripted += 1;
            }
            if !setup.spheres.is_empty() {
                with_spheres += 1;
            }
        }
    }
    eprintln!(
        "training dungeon {TRAINING_DUNGEON:#06X}: {} cells, {placements} placements, \
         {scripted} with a setup record's default script, {with_spheres} with collision spheres",
        cells.len()
    );
    assert!(
        placements > 500,
        "only {placements} baked objects in the whole dungeon"
    );
    assert!(
        scripted > 50,
        "only {scripted} of them run a default script"
    );
    assert!(
        with_spheres > 100,
        "only {with_spheres} of them could ever be solid"
    );
}

/// Oracle: ACE's `landblock_instance` and `weenie` tables for landblock `0x8602`, and the retail
/// setup records those weenies name.
///
/// A dungeon door is not one of these cell statics but a server **weenie**. The world database
/// places ten
/// `WeenieType.Door (19)` instances in `0x8602` — `door` (278), `doorprison` (568), `doorolthoi`
/// (4451) x4, `doormetalcave` (4453), `dooracademya` (12705),
/// `doornewbieacademypracticearea` (29329) and `doornewbieacademylibrary` (30998) — and their
/// `PropertyDataId.Setup` values are `0x0200024F`, `0x02000281`, `0x020005F2`, `0x020005F1` and
/// `0x020005DA`. None of the five is a static of any cell of the block, which is what this
/// asserts from the dat alone.
///
/// Door collision therefore belongs to the streamed-object path, not cell-static registration.
/// These door setups carry **no collision spheres** and select part physics meshes through
/// `HAS_PHYSICS_BSP_PS (0x10000)`, so only the part-mesh branch can make them solid.
#[test]
fn a_dungeon_door_is_a_server_weenie_and_not_one_of_these_statics() {
    let store = store();
    // The five setups ACE's door weenies for this block name.
    const DOORS: [u32; 5] = [
        0x0200_024F,
        0x0200_0281,
        0x0200_05F2,
        0x0200_05F1,
        0x0200_05DA,
    ];

    let mut loader = EnvCellLoader::new();
    let placed: std::collections::BTreeSet<u32> = loader
        .load_block(&store, TRAINING_DUNGEON)
        .iter()
        .flat_map(cell_statics)
        .map(|s| s.id.0)
        .collect();
    for d in DOORS {
        assert!(
            !placed.contains(&d),
            "{d:#010X} is a cell static after all, so a door IS cell-static registration's"
        );
    }

    // And each of them carries no collision sphere, so the sphere branch cannot make it solid.
    let mut wooden = 0usize;
    for d in DOORS {
        let s = setup(&store, DataId(d)).expect("a door setup decodes");
        assert!(
            s.spheres.is_empty(),
            "{d:#010X} has {} collision sphere(s); the sphere branch would already stop the player",
            s.spheres.len()
        );
        assert!(
            s.has_physics_bsp,
            "{d:#010X} does not even carry a physics BSP"
        );
        if s.cylspheres.is_empty() {
            // No spheres or cylinder-spheres: neither primitive collision branch can stop a body.
            wooden += 1;
        }
    }
    assert!(
        wooden >= 2,
        "expected the plain wooden doors (0x0200024F, 0x02000281) to have neither spheres nor \
         cylspheres, which is why they are not solid at all rather than solid and mis-shaped"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The draw
// ---------------------------------------------------------------------------------------------

/// Behaviour: camera.view.slides-through-a-creature-standing-between-you-and-the-camera
///
/// Oracle: retail's cell-drawing pass issues the cell's placed objects, and the rendered
/// frame must show their geometry.
///
/// The differential is the whole evidence: the same scene, the same viewer, one frame with
/// the cell-static triangles issued and one without. Every changed pixel must lie
/// inside the projected outline of a cell-static triangle, which is what
/// [`WorldScene::cell_static_screen_boxes`] gives.
#[test]
fn standing_in_the_training_dungeon_draws_its_furniture() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");

    // Where the body stands, and which of that cell's baked objects it faces. Both come out of
    // the dat: `standable_point` asks the cell's own `cell_bsp`, and the target is the furthest
    // of the cell's own statics, so it is fully in frame rather than clipped by the near plane.
    let start = CellId(0x8602_0102);
    let mut loader = EnvCellLoader::new();
    let placements: Vec<_> = loader
        .load_block(&store, TRAINING_DUNGEON)
        .iter()
        .filter(|d| d.id == start)
        .flat_map(cell_statics)
        .collect();
    assert!(
        !placements.is_empty(),
        "the start cell bakes in nothing to look at"
    );

    // A frame from the same station, with and without the cells' objects.
    let shot = |gpu: &mut Gpu, on: bool| -> Shot {
        let cfg = SceneConfig {
            landblock: TRAINING_DUNGEON,
            start_cell: Some(start),
            cell_statics: on,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, gpu, cfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, gpu)
            .expect("the body is created");
        let stand = scene
            .standable_point(start)
            .expect("the start cell has a standable point");
        // Face the furthest of this cell's own objects. `follow_character` takes the camera's yaw
        // from the body's heading, so aiming the body aims the
        // camera.
        let target = placements
            .iter()
            .max_by(|a, b| {
                let d = |f: &Frame| math::hypotf(f.origin.x - stand.x, f.origin.y - stand.y);
                d(&a.frame).total_cmp(&d(&b.frame))
            })
            .expect("just checked");
        let (dx, dy) = (
            target.frame.origin.x - stand.x,
            target.frame.origin.y - stand.y,
        );
        let t = math::atan2f(-dx, dy);
        let heading = Quat::new(math::cosf(t * 0.5), 0.0, 0.0, math::sinf(t * 0.5));
        {
            let c = scene.character.as_mut().expect("a body");
            c.teleport(Position::new(start, Frame::new(stand, heading)));
        }
        scene.follow_character_now();
        let body = scene.character.as_ref().expect("a body").render_frame();
        scene.camera.position = Vec3::new(body.origin.x, body.origin.y, body.origin.z + 1.2);
        let (w, h) = gpu.size();
        let boxes = scene.cell_static_screen_boxes(w, h);
        // The camera is hand-placed and the simulation is not run, as in the companion
        // interiors fixture: the camera's swept sphere would itself be stopped by
        // a table, so a run that let the camera settle would compare two different viewpoints and
        // the differential would be of the whole frame instead of the furniture. With no step, the
        // sweep has never run and no viewer cell has been resolved. `viewer_cell()` falls back to
        // the body's cell, the camera update's last resort, identically in both runs.
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        let image = gpu.capture().expect("capture");
        let batches = scene.draw.stats.cell_static_batches;
        (image.to_rgba(), image.width, image.height, boxes, batches)
    };

    let (off, w, h, _, off_batches) = shot(&mut gpu, false);
    let (on, _, _, boxes, on_batches) = shot(&mut gpu, true);
    // The switch is taken at the bake, so the control is the same scene with no cell statics in
    // it: no triangles, no emitters, no bodies, no furniture textures.
    assert_eq!(
        off_batches, 0,
        "the control frame baked cell-static batches after all"
    );
    assert!(
        on_batches > 0,
        "the dungeon's cells produced no draw batches at all"
    );
    assert!(
        !boxes.is_empty(),
        "the traversal reached no cell with a baked object in it"
    );

    let mut changed = 0usize;
    let mut outside = 0usize;
    for (i, (a, b)) in off
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if a == b {
            continue;
        }
        changed += 1;
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        // LINT-OK: a pixel index in an 800x600 image. Not a float-to-int conversion.
        let (px, py) = (((i as u32) % w) as f32 + 0.5, ((i as u32) / w) as f32 + 0.5);
        if !boxes
            .iter()
            .any(|(x0, y0, x1, y1)| px >= *x0 && px <= *x1 && py >= *y0 && py <= *y1)
        {
            outside += 1;
        }
    }
    eprintln!(
        "training dungeon: {on_batches} cell-static batches, {} triangle boxes, \
         {changed} px changed, {outside} outside every box",
        boxes.len()
    );
    assert!(changed > 500, "only {changed} of {} pixels changed", w * h);
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels are outside every drawn triangle's own outline"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The collision
// ---------------------------------------------------------------------------------------------

/// Oracle: object collision against the retail setup spheres, run **differentially**:
/// the same walk twice, once with cell-static registration and once without.
///
/// The table is `0x0200011F`, ACE's `dinnertable` setup, standing in a Holtburg dining room. Its
/// collision half is two spheres of radius 0.381 at `x = +-0.3625, z = 0.381`, and the claim is
/// that a body walking at it ends **outside** their union in one run and **inside** it in the
/// other: "the body stopped" is not evidence that it stopped at the table.
///
/// The walk reads z as well as the horizontal gap, because a body that **climbed onto** the table
/// and a body that **walked through** it have the same horizontal reading. A walk reports both,
/// and [`Walk::verdict`] names which.
fn dinner_walk(
    store: &Arc<RetailDatStore>,
    region: &dereth_assets::Region,
    gpu: &mut Gpu,
    room: CellId,
    target: &Frame,
    solid: &[(Vec3, f32)],
    statics: bool,
) -> Walk {
    let cfg = SceneConfig {
        cell_statics: statics,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, region, gpu)
        .expect("the body is created");
    assert_eq!(
        scene.cell_static_handles(room).is_empty(),
        !statics,
        "the room's statics were registered when they should not have been, or not when they \
         should"
    );

    // Start on the floor of the same room, two metres from the target, heading at it. The start
    // point is the cell's own `cell_bsp` containment result, not a
    // guess: `standable_point` scans it.
    let stand = scene
        .standable_point(room)
        .expect("the room has a standable point");
    let (dx, dy) = (target.origin.x - stand.x, target.origin.y - stand.y);
    let d = math::hypotf(dx, dy);
    assert!(
        d > 1.0,
        "the standable point is {d:.2} m from the target, which is not a walk"
    );
    // Yaw so that the body's local +y, its forward axis, points at the
    // target: rotating (0, 1) by t gives (-sin t, cos t).
    let t = math::atan2f(-dx, dy);
    let heading = Quat::new(math::cosf(t * 0.5), 0.0, 0.0, math::sinf(t * 0.5));
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(room, Frame::new(stand, heading)));
    }
    scene.follow_character_now();

    let input = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };
    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a fixed simulation step in seconds, narrowed for the debug camera's f32 delta.
    let dt = step as f32;
    let mut now = 0.0f64;
    let mut closest = f32::MAX;
    let mut moved = 0.0f32;
    let mut zs: Vec<f32> = Vec::with_capacity(120);
    for _ in 0..120 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            input,
            LocalTime(now),
            dt,
        );
        let c = scene.character.as_ref().expect("a body");
        let p = c.position().frame.origin;
        moved = moved.max(math::hypotf(p.x - stand.x, p.y - stand.y));
        zs.push(p.z);
        for (centre, r) in solid {
            // The body is a vertical capsule; measure in the plane, which is the axis a walking
            // body can be blocked along.
            let gap = math::hypotf(p.x - centre.x, p.y - centre.y) - r;
            closest = closest.min(gap);
        }
    }

    let (floor, peak, max_lift) = floor_peak_and_lift(&zs);
    Walk {
        closest,
        moved,
        floor,
        peak,
        drop: zs[0] - floor,
        end_z: *zs.last().expect("frames"),
        max_lift,
    }
}

/// The floor a run settled onto and the highest it ever got after settling.
///
/// The metric must distinguish a fall after `teleport` from a climb. The lowest z the run ever
/// held is the surface it came to rest on, and everything from the frame it first touches that
/// surface onwards is the walk -- a body that climbs raises z **after** that frame, which is the
/// only reading this calls a climb.
///
/// The tolerance is the load-bearing part. `<= floor` exactly picks the frame **after** a climb,
/// because a body that climbs a static and walks back off it settles a ten-thousandth of a metre
/// lower than it started (66.00490 against 66.00500 at the Holtburg dinnertable) -- and then
/// reports a rise of zero.
///
/// The third return is the largest single-frame rise after the body settled. A climb can take
/// many step-up frames, so the total rise says nothing about what any one step did and the
/// per-frame maximum says everything. Both are calibrated in both directions in
/// [`the_rise_metric_reads_a_climb_and_not_the_drop_teleport_leaves`].
fn floor_peak_and_lift(zs: &[f32]) -> (f32, f32, f32) {
    let floor = zs.iter().copied().fold(f32::MAX, f32::min);
    let landed = zs.iter().position(|z| *z <= floor + 0.01).unwrap_or(0);
    let walk = &zs[landed..];
    let peak = walk.iter().copied().fold(f32::MIN, f32::max);
    let lift = walk.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
    (floor, peak, lift)
}

/// Oracle: a measured dinnertable walk's z series, in both of the shapes it produces -- a settle,
/// and a settle followed by a climb and a descent. The rise metric reports **zero** for a body
/// that only ever falls, and it has to report the climb for a body that climbs, or every zero it
/// prints is uninterpretable.
///
/// The climbing series was recorded frame for frame: the body lands
/// at 66.005, is lifted in three steps of 0.525, 0.395 and 0.207 m -- each inside the 0.600 m its
/// own setup allows -- rests on the table top at 66.925, and comes back down to **66.0049**, a
/// ten-thousandth *below* where it landed.
#[test]
fn the_rise_metric_reads_a_climb_and_not_the_drop_teleport_leaves() {
    // A fall onto the floor and a flat walk: no climb.
    let settle = [
        66.500, 66.418, 66.239, 66.064, 66.005, 66.005, 66.005, 66.005,
    ];
    let (floor, peak, lift) = floor_peak_and_lift(&settle);
    assert!((floor - 66.005).abs() < 1e-3, "{floor}");
    assert!(
        peak - floor <= CLIMB,
        "the half-metre drop `teleport` leaves reads as a climb of {:.3} m",
        peak - floor
    );
    assert!(
        lift <= 1e-4,
        "a body that only ever falls was lifted {lift:.4} m"
    );

    // The same fall, then the climb, then off the table and a hair below where it landed.
    let climb = [
        66.500, 66.418, 66.239, 66.064, 66.005, 66.005, 66.530, 66.925, 67.131, 66.925, 66.925,
        66.658, 66.0049,
    ];
    let (floor, peak, lift) = floor_peak_and_lift(&climb);
    assert!(
        peak - floor > CLIMB,
        "a body that stood 1.13 m up on a table read as a rise of {:.3} m, so this metric cannot \
         tell a climb from a walk-through and neither can any assertion built on it",
        peak - floor
    );
    assert!(
        (peak - 67.131).abs() < 1e-3,
        "the peak is the top of the climb, not {peak}"
    );
    // **The per-step lift, calibrated through the same reduction the two walks are judged by.**
    // `max_lift` is what says a climb was stair-stepping rather than a body teleporting up a
    // wall; blanking it in [`floor_peak_and_lift`] must redden this. The recorded climb's
    // largest single frame is 66.530 -> 66.925 = **0.395 m**; the largest lift of any kind in the
    // series is 66.005 -> 66.530 = **0.525 m**, and both are inside the 0.600 m the player's setup
    // allows.
    let lifts: Vec<f32> = climb
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.0)
        .collect();
    assert!(
        (lift - 0.525).abs() < 1e-3,
        "the shared reduction read the largest single lift of the recorded climb as {lift:.4} m, \
         not the 0.525 m the series contains ({lifts:?}) -- every `max_lift` assertion in this \
         file is measured with it",
    );
    assert!(
        lifts.iter().copied().fold(0.0f32, f32::max) < 0.6,
        "no single lift in the recorded climb is larger than the 0.600 m step height: {lifts:?}"
    );
}

/// The collision spheres of one baked static, in the block's own space. Object collision
/// transforms them by the object's frame and scale (1 here), which is what this repeats.
fn solid_spheres(store: &RetailDatStore, frame: &Frame, id: u32) -> Vec<(Vec3, f32)> {
    let su = setup(store, DataId(id)).expect("the setup decodes");
    let m = dereth_physics::math::l2g(frame.rotation);
    su.spheres
        .iter()
        .map(|s| {
            (
                dereth_physics::math::localtoglobalvec(m, s.center).add(frame.origin),
                s.radius,
            )
        })
        .collect()
}

/// How far above its own origin a static's collision half reaches, over **both** representations
/// the collision path can consult: the setup's own spheres and, because
/// object collision takes the part-mesh arm whenever `HAS_PHYSICS_BSP_PS`
/// is set and never looks at the spheres then, the placed parts' physics meshes.
///
/// The two must be read together or the answer is about the wrong geometry: every static in this
/// pair carries a part BSP, so the mesh is what a body actually meets, while the spheres are what
/// the setup declares. Both are reported and the taller decides.
fn collision_top(store: &RetailDatStore, frame: &Frame, id: u32) -> (f32, f32) {
    let su = setup(store, DataId(id)).expect("the setup decodes");
    let spheres = su
        .spheres
        .iter()
        .map(|s| s.center.z + s.radius)
        .fold(f32::MIN, f32::max);

    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    let g = dereth_world_data::setup::setup_geometry_with_parts(store, &su, &mut stats);
    let pos = Position::new(CellId(0), *frame);
    let mut mesh = f32::MIN;
    for i in 0..g.parts.len() {
        let Some(part) = g.placed_part(i, &pos, 1.0) else {
            continue;
        };
        let Some(tree) = part.physics_bsp.as_ref() else {
            continue;
        };
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        for poly in &tree.polygons {
            for v in &poly.vertices {
                let w = dereth_physics::math::localtoglobalvec(m, v.mul(part.gfxobj_scale))
                    .add(part.pos.frame.origin);
                mesh = mesh.max(w.z - frame.origin.z);
            }
        }
    }
    (spheres, mesh)
}

/// Behaviour: world.cell-statics.an-interior-cells-baked-objects-are-drawn-and-collide
///
/// A registered dinnertable **answers** a body walking into it: it stops the body, or lifts it.
/// In retail a player can walk onto a chair and from there onto a table (plain stairs are built
/// that way), so a climb is retail behaviour. What the static must never do is nothing, which is
/// exactly the control's reading.
///
/// The climb is still bounded, and by retail's own budget rather than by a number chosen here:
/// no single frame may lift the body further than its setup's step-up height, because
/// each step-up attempt passes that budget to one downward collision probe. A
/// 0.920 m table reached in lifts of 0.525, 0.395 and 0.207 m is stair-stepping; one 0.920 m
/// lift would be the budget not being read at all.
#[test]
fn a_holtburg_dinnertable_answers_a_body_walking_into_it_and_an_unregistered_one_does_not() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");

    let room = CellId(HOLTBURG_DINING_ROOM);
    let table = placement(&store, HOLTBURG_DINING_ROOM, DINNERTABLE)
        .expect("the Holtburg dining room bakes in a dinnertable");
    let table_setup = setup(&store, DataId(DINNERTABLE)).expect("the dinnertable setup decodes");
    assert_eq!(
        table_setup.spheres.len(),
        2,
        "the dinnertable's collision half is two spheres"
    );
    let solid = solid_spheres(&store, &table, DINNERTABLE);

    let blocked = dinner_walk(&store, &region, &mut gpu, room, &table, &solid, true);
    let through = dinner_walk(&store, &region, &mut gpu, room, &table, &solid, false);
    eprintln!(
        "dinnertable at ({:.2}, {:.2}): with the statics registered the body came \
         within {:.3} m of a table sphere's surface, travelled {:.2} m and rose {:.3} m in \
         single lifts of at most {:.3} m — it {}; without them {:.3} m, {:.2} m and {:.3} m — \
         it {}",
        table.origin.x,
        table.origin.y,
        blocked.closest,
        blocked.moved,
        blocked.rise(),
        blocked.max_lift,
        blocked.verdict(),
        through.closest,
        through.moved,
        through.rise(),
        through.verdict(),
    );

    // The control has to actually reach the table, or the comparison is about nothing — and it
    // has to reach it *through* the table's own volume rather than over it.
    assert!(
        through.closest < 0.0,
        "the control walk never overlapped the table (closest {:.3} m), so it is not a walk into \
         the table",
        through.closest
    );
    // Placement commits in an interior cell: its downward probe puts the body on the floor at
    // the instant of `teleport` (the BSP insertion path answers `OK_TS` for a sphere it has not
    // touched, as retail does), so both walks drop less than 0.05 m. A placement that was
    // refused would leave the body half a metre up, falling. That the z sampler follows the body
    // is shown by the positive `blocked.rise()` check below.
    assert!(
        through.drop < 0.05 && blocked.drop < 0.05,
        "the body fell {:.3} m and {:.3} m from where `teleport` put it; both drops must remain below 0.05 m",
        through.drop,
        blocked.drop,
    );
    assert!(
        through.rise() <= CLIMB,
        "the control walk rose {:.3} m with nothing registered to climb, so the z reading is \
         measuring something other than a climb",
        through.rise()
    );
    // And with the statics registered the table **answers**: it either stops the body or lifts
    // it onto itself. Both are retail (see this test's doc comment), and the one reading that is
    // a defect is the control's, a body that
    // crosses the table's own volume at floor level as though nothing were there.
    assert!(
        blocked.answered(),
        "the body {} (closest {:.3} m, rose {:.3} m from a floor at {:.3}, ending at {:.3}): the \
         registered dinnertable neither stopped it nor lifted it, which is the control's reading \
         and means the static contributed nothing",
        blocked.verdict(),
        blocked.closest,
        blocked.rise(),
        blocked.floor,
        blocked.end_z,
    );
    // If it lifted the body, it did so one legal step at a time: no single frame lifted it
    // further than the 0.600 m step-up height the player's own setup declares.
    // A **total** rise larger than the budget is stair-stepping and is retail; a **single frame**
    // larger than it would not be, and only a per-frame reading can tell the two apart.
    //
    // This describes the climb; it does not prove the budget is read, because the lifts this
    // fixture produces are set by where the dinnertable's mesh offers footing. The budget's own
    // reading is asserted in `dereth-physics --lib`'s
    // `step_up_probes_with_the_objects_own_budget_only_while_it_is_on_a_walkable`.
    let player = setup(&store, DataId(PLAYER_SETUP)).expect("the Aluvian male setup decodes");
    assert!(
        blocked.max_lift <= player.step_up_height + 1e-3,
        "one frame lifted the body {:.3} m, more than the {:.3} m `step_up_height` its own setup \
         declares -- the transition step-up routine is not spending its per-step budget",
        blocked.max_lift,
        player.step_up_height,
    );
}

/// Behaviour: world.cell-statics.step-height-decides-which-static-can-be-climbed
///
/// Oracle: the mover setup's step-up height and the downward collision probe used by each
/// step-up attempt. The probe budget starts at 0.04 and is replaced by the object's step-up
/// height when `ON_WALKABLE_TS` is set; the probe also receives the walkable allowance.
///
/// The pair, from the shipped block:
///
/// | setup | collision top | against the player's 0.600 m |
/// |---|---|---|
/// | `0x0200011F`, ACE's `dinnertable` | 0.762 m | **above** the single-step budget |
/// | `0x02000120`, its companion | 0.300 m | below the single-step budget |
///
/// Several legal lifts can climb the taller object, so the step budget does not cap the final
/// height above the floor. The test requires the shorter object to be climbable, rejects climbing
/// only the taller one, and checks the shorter climb's largest single-frame lift against the setup
/// budget; the dinnertable test checks the taller climb's per-frame lift. The physics unit test,
/// not this fixture, proves the budget is read.
///
/// The rise metric is calibrated against a known positive in
/// [`the_rise_metric_reads_a_climb_and_not_the_drop_teleport_leaves`]: without it, "the body did
/// not climb" and "the instrument cannot see a climb" are the same zero.
#[test]
fn the_setups_step_height_says_which_of_two_statics_can_be_climbed() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");

    // 1. The step height, read from the player's own setup rather than assumed.
    let player = setup(&store, DataId(PLAYER_SETUP)).expect("the Aluvian male setup decodes");
    let step_up = player.step_up_height;
    assert!(
        (step_up - 0.6).abs() < 1e-6,
        "the player's setup carries step_up_height {step_up}, not 0.6 — the step-height pair is \
         transposed or the wrong setup was read"
    );

    // 2. The two statics, straddling it.
    let room = CellId(HOLTBURG_DINING_ROOM);
    let table = placement(&store, HOLTBURG_DINING_ROOM, DINNERTABLE)
        .expect("the Holtburg dining room bakes in a dinnertable");
    let low_room = CellId(HOLTBURG_LOW_STATIC_ROOM);
    let low = placement(&store, HOLTBURG_LOW_STATIC_ROOM, LOW_STATIC)
        .expect("that room bakes in the low static");
    let (table_spheres, table_mesh) = collision_top(&store, &table, DINNERTABLE);
    let (low_spheres, low_mesh) = collision_top(&store, &low, LOW_STATIC);
    let table_top = table_spheres.max(table_mesh);
    let low_top = low_spheres.max(low_mesh);
    eprintln!(
        "the step-height pair: dinnertable {DINNERTABLE:#010X} spheres reach {table_spheres:.3} m and \
         its mesh {table_mesh:.3} m; {LOW_STATIC:#010X} spheres reach {low_spheres:.3} m and its \
         mesh {low_mesh:.3} m; the player's own step height is {step_up:.3} m"
    );
    assert!(
        table_top > step_up,
        "the dinnertable reaches {table_top:.3} m, which is not above the {step_up:.3} m step \
         height, so it is not the 'cannot be climbed' half of this pair"
    );
    // The two representations of each static's collision half must agree about which side of
    // the budget it falls on, or the pair is ambiguous and nothing below means anything. They do:
    // 0.762 and 0.920 are both above 0.600, and 0.300 and 0.507 are both below it.
    assert_eq!(
        table_spheres > step_up,
        table_mesh > step_up,
        "the dinnertable's spheres reach {table_spheres:.3} m and its mesh {table_mesh:.3} m, which fall on opposite sides of the {step_up:.3} m step height -- whether a body can climb it depends on which representation the collision path consults"
    );
    assert_eq!(
        low_spheres > step_up,
        low_mesh > step_up,
        "{LOW_STATIC:#010X}'s spheres reach {low_spheres:.3} m and its mesh {low_mesh:.3} m, which fall on opposite sides of the {step_up:.3} m step height"
    );
    assert!(
        low_top > 0.0 && low_top < step_up,
        "{LOW_STATIC:#010X} reaches {low_top:.3} m, which is not below the {step_up:.3} m step \
         height, so it is not the 'can be climbed' half of this pair"
    );

    // 3. The two walks. `ground` is the same dinnertable walk
    //    measured above; the metric's own
    //    calibration is [`the_rise_metric_reads_a_climb_and_not_the_drop_teleport_leaves`],
    //    which is where a body that only falls and a body that climbs are told apart.
    let solid = solid_spheres(&store, &table, DINNERTABLE);
    let ground = dinner_walk(&store, &region, &mut gpu, room, &table, &solid, true);

    // 4. The paired comparison rejects a build that climbs the **taller** static while
    //    refusing the shorter one. The implication does not prohibit a multi-step climb
    //    onto the taller object; the positive short-climb and per-frame checks remain separate.
    let low_solid = solid_spheres(&store, &low, LOW_STATIC);
    let w = dinner_walk(&store, &region, &mut gpu, low_room, &low, &low_solid, true);
    eprintln!(
        "the step-height pair: at the dinnertable (top {table_top:.3} m, above the \
         {step_up:.3} m budget) the body came within {:.3} m, travelled {:.2} m and rose \
         {:.3} m -- it {}; at {LOW_STATIC:#010X} (top {low_top:.3} m, below it) {:.3} m, \
         {:.2} m and {:.3} m -- it {}",
        ground.closest,
        ground.moved,
        ground.rise(),
        ground.verdict(),
        w.closest,
        w.moved,
        w.rise(),
        w.verdict(),
    );
    let climbed_tall = ground.rise() > CLIMB;
    let climbed_short = w.rise() > CLIMB;

    // A step the player can take is taken without a jump. `0x02000120`'s collision half reaches
    // `low_top` above its own origin, which is **below** the `step_up_height` the player's own
    // setup carries, so a player walks onto it; a body stopped short of it (rising 0.000 m) would
    // be refusing a step the client itself declares it can take.
    //
    // It is a positive case rather than a pin: it says the body *can* climb, so it cannot be
    // satisfied by a build that lifts nothing, and the law below still forbids lifting the
    // taller of the pair while refusing this one.
    assert!(
        climbed_short,
        "the body did not get onto {LOW_STATIC:#010X} (rose {:.3} m, closest {:.3} m, travelled \
         {:.2} m -- it {}), whose {low_top:.3} m top is below the {step_up:.3} m its own setup \
         allows: a step the player's own `step_up_height` says is steppable, that \
         the body can only get onto by jumping. The transition step-up routine is not \
         reached from the BSP sphere-step routine",
        w.rise(),
        w.closest,
        w.moved,
        w.verdict(),
    );
    // And the climb obeys the per-step budget, which is what makes it stair-stepping rather than
    // teleporting up a wall. See the same law in
    // [`a_holtburg_dinnertable_answers_a_body_walking_into_it_and_an_unregistered_one_does_not`].
    assert!(
        w.max_lift <= step_up + 1e-3,
        "one frame lifted the body {:.3} m onto {LOW_STATIC:#010X}, more than the {step_up:.3} m \
         `step_up_height` its own setup declares",
        w.max_lift,
    );

    assert!(
        !climbed_tall || climbed_short,
        "the body climbed the dinnertable, whose {table_top:.3} m top is above the \
         {step_up:.3} m its own setup allows (rose {:.3} m), and did not climb \
         {LOW_STATIC:#010X}, whose {low_top:.3} m top is below it (rose {:.3} m) -- \
         the transition step-up routine is reading its budget backwards",
        ground.rise(),
        w.rise(),
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The scripts
// ---------------------------------------------------------------------------------------------

/// Oracle: setup default-script IDs in `client_portal.dat` and `dereth_animation`'s script runner.
///
/// Object construction queues the setup's default script, which for a brazier or candle is
/// a physics script full of particle-creation hooks. Interior statics are constructed through the
/// same path as outdoor ones, so a dungeon's braziers burn: this asserts that the dungeon's
/// scripted statics reach that path and that the scripts create emitters.
#[test]
fn the_training_dungeons_braziers_run_their_default_scripts() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: TRAINING_DUNGEON,
        start_cell: Some(CellId(0x8602_0102)),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    // A second of the app's own frame order, so the scripts' `CreateParticleHook`s have run and
    // the emitters have emitted.
    run(&store, &mut gpu, &mut scene, CharacterInput::default(), 30);
    eprintln!(
        "training dungeon: {} emitter hosts, {} emitters, {} live particles",
        scene.draw.stats.emitter_hosts,
        scene.draw.stats.particles.emitters,
        scene.draw.stats.particles.live
    );
    assert!(
        scene.draw.stats.emitter_hosts > 0,
        "no interior static got a live object, so no default script ran"
    );
    assert!(
        scene.draw.stats.particles.live > 0,
        "the dungeon's scripted statics emit nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. The counters
// ---------------------------------------------------------------------------------------------

/// Oracle: registration of placed objects from the retail cell and portal DATs.
///
/// Registration failures are counted rather than silently ignored. This asserts the two
/// counters that would mean a wiring bug are
/// zero over a whole dungeon, and reports the two that are known gaps in collision coverage.
#[test]
fn every_training_dungeon_placement_is_accounted_for() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: TRAINING_DUNGEON,
        start_cell: Some(CellId(0x8602_0102)),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let s = scene.cell_static_stats();
    eprintln!(
        "cell statics: {} placements, {} bodies, {} intangible, {} simple setups, \
         {} undecodable, {} unplaced",
        s.placements, s.created, s.intangible, s.simple_setup, s.undecodable, s.unplaced
    );
    assert!(s.placements > 0, "no cell static was offered at all");
    assert_eq!(s.unplaced, 0, "a baked cell was not given to physics");
    assert_eq!(
        s.undecodable, 0,
        "a setup record named by a cell would not decode"
    );
    assert_eq!(
        s.created,
        s.placements - s.simple_setup,
        "every placement with a setup-record id got a body"
    );
    assert!(
        s.created - s.intangible > 0,
        "not one of the dungeon's objects is solid"
    );

    // The bodies are in the cell's own list, which object collision walks.
    let mut with_bodies = 0usize;
    let mut loader = EnvCellLoader::new();
    for d in loader.load_block(&store, TRAINING_DUNGEON) {
        if cell_statics(&d).iter().any(|x| x.id.0 >> 24 == 0x02)
            && !scene.cell_static_handles(d.id).is_empty()
        {
            with_bodies += 1;
        }
    }
    assert!(with_bodies > 50, "only {with_bodies} cells hold a body");
}
