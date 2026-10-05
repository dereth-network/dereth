//! The outdoor pass in an indoor frame, and the gate on it. Normal rendering branches on the
//! viewer's own cell: an outdoor viewer (low cell word below `0x100`) gets the whole outdoor pass
//! (sky phase 0, every resident block, sky phase 1); an indoor viewer gets the interior, whose
//! portal-cell draw runs the outdoor pass only when the frame's outside view has polygons, which
//! portal clipping collects from each visible portal leading outdoors and view construction
//! zeroes every frame. A dungeon with no outdoor portal in view therefore draws no terrain,
//! scenery or sky, and the black clear stands behind whatever its cells do not cover; without the
//! gate the sky's first phase (always-passing depth, no depth writes) paints every uncovered
//! pixel. Asserted on the outside-view count in both polarities (a sealed training-academy room
//! sees none, Holtburg's house with its windows on screen sees several), taken with the resolved
//! viewer camera, and with the control that the gate leaves an outdoor viewer's frame
//! byte-identical. Fixture: the retail dats on a software device. Fails when the retail dats are
//! absent; skips only without a device.

#![cfg(gpu)]

use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};
use {dereth_world_data::env_cells::physics_geometry, dereth_world_data::env_cells::EnvCellLoader};

const W: u32 = 800;
const H: u32 = 600;

/// `0x8602` is the training academy.
const TRAINING_DUNGEON: u16 = 0x8602;

/// A room of that dungeon whose cells this build actually draws, so that "the interior is still
/// there" is a control with something in it. Chosen by measurement: the `0x0101..0x0120` rooms
/// place their body outside the block's own extent and draw nothing at all, which would make the
/// control vacuous.
const STATION_CELL: u32 = 0x8602_0140;

/// Holtburg — the outdoor control's block.
const HOLTBURG: u16 = 0xA9B4;

/// The stairwell head of Holtburg's house with a cellar: the **positive** arm for
/// `outside_view.view_count`, an indoor viewer with the land visible through the windows.
const HOLTBURG_STAIRWELL: u32 = 0xA9B4_0146;

/// The retail store, or **fail**.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// A standable point inside `cell`, in the cell's own global frame.
///
/// `SceneConfig::start_cell` alone does **not** put the body inside a house: measured, `start_cell: Some(0xA9B40146)` settles the body in `0xA9B40024`, which
/// the low-word cell-ID predicate classifies as outdoors (`0x24 == 36 < 0x100`), and an outdoor viewer is the wrong
/// arm of the branch under test. `Character::teleport` onto a point the cell's own BSP accepts
/// lands in `0xA9B40143`.
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

fn is_black(px: &[u8]) -> bool {
    px[0] == 0 && px[1] == 0 && px[2] == 0
}

fn nonblack(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4).filter(|p| !is_black(p)).count()
}

/// The same, outdoors over Holtburg with no body — the control that says the gate is about the
/// *viewer's cell* and not about the outdoor pass itself.
fn outdoor_frame(store: &Arc<RetailDatStore>, gpu: &mut Gpu, gate: bool) -> Vec<u8> {
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        character: false,
        scenery_radius: 1,
        land_radius: 1,
        time_of_day: Some(0.5),
        particles: false,
        outside_view_gate: gate,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let mut stream = ObjectStream::new();
    let mut rgba = Vec::new();
    for i in 0..4 {
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(f64::from(i) * dereth_client_runtime::platform::clock::HEADLESS_STEP),
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

/// Behaviour: rendering.indoor.a-sealed-room-sees-no-outdoors-and-a-windowed-house-does
/// **The rejecting test: the outside-view count decides the outdoor pass, in both polarities.**
///
/// Portal-cell drawing runs the outdoor pass when `outside_view.view_count != 0`, and that nonzero
/// count is the **entire** condition [`SceneConfig::outside_view_gate`] implements.
/// [`WorldScene::indoor_outside_view_count`] reads it off the same view construction `draw_inside`
/// makes, so this asserts the branch itself:
///
/// * a sealed training-academy room sees **no** outdoor portal, `Some(0)`, so retail draws no
///   terrain, no scenery and no sky there and the gated build must not either;
/// * Holtburg's house at `0xA9B40146`, with its windows on screen, sees **several**, `> 0`, so the
///   pass runs there and the gate is not allowed to suppress it.
///
/// Both polarities, because a build that drew no outdoor pass anywhere would satisfy half of this;
/// and `None` — the viewer is outdoors, or `draw_inside` would bail — is distinguished from `Some(0)`
/// so that an instrument which cannot look cannot report absence. A pixel differential at the
/// academy station reads nothing: with the resolved viewer camera the interior covers the whole
/// frame there, so the claim is taken on the branch and not on pixels.
#[test]
fn a_sealed_dungeon_room_sees_no_outdoors_and_a_house_with_windows_does() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);

    let sealed = outside_view_count_at(&store, &mut gpu, TRAINING_DUNGEON, STATION_CELL);
    let windowed = outside_view_count_at(&store, &mut gpu, HOLTBURG, HOLTBURG_STAIRWELL);
    eprintln!(
        "outside_view.view_count: sealed {STATION_CELL:#010X} -> {sealed:?}; windowed \
         {HOLTBURG_STAIRWELL:#010X} -> {windowed:?}"
    );

    // The instrument could look at all: an indoor frame whose traversal starts. `None` here is
    // `draw_inside` bailing or the viewer standing outdoors, and either would make the zero below
    // meaningless.
    let sealed = sealed.expect(
        "the academy station is not an indoor frame whose traversal starts, so its count says \
         nothing about the gate",
    );
    let windowed = windowed.expect(
        "the Holtburg house station is not an indoor frame whose traversal starts, so its count \
         says nothing about the gate",
    );

    // **The claim.** No portal chain out of this room reaches the outdoors, so portal-cell drawing never
    // takes its first branch and the indoor frame contains no sky, no terrain and no scenery.
    assert_eq!(
        sealed, 0,
        "the sealed academy room {STATION_CELL:#010X} reports {sealed} outdoor view polygons, so \
         the `outside_view.view_count != 0` branch fires there and the gate is not \
         what decides whether this dungeon gets an outdoor pass"
    );
    // **The other polarity**, which is what stops the line above from being satisfied by a build
    // that draws no outdoor pass anywhere.
    assert!(
        windowed > 0,
        "the Holtburg house {HOLTBURG_STAIRWELL:#010X} reports {windowed} outdoor view polygons. \
         Its windows are on screen (about 10,000 px of them), so a zero \
         here means the traversal never reaches an outdoor portal at all and the assertion above is \
         vacuous"
    );
}

/// One settled indoor frame at `cell` in `block`, and the `outside_view.view_count` it draws with.
///
/// The app's own frame order **including `crate::camera::update_viewer`**, so the cell the count is
/// taken from is the resolved viewer's and not the body's. `sweeps > 0` is asserted so that a frame
/// drawn by the debug chase camera cannot be mistaken for a frame drawn by the client.
fn outside_view_count_at(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    block: u16,
    cell: u32,
) -> Option<usize> {
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: block,
        start_cell: Some(CellId(cell)),
        time_of_day: Some(0.35),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    // `start_cell` is not enough on its own -- see [`point_in`]. `Character::teleport` enters the
    // body into the world; initial-frame placement resolves whichever cell actually contains
    // the point, so the settled cell is reported rather than demanded.
    {
        let p = point_in(store, cell).expect("the station cell has a standable point");
        let c = scene.character.as_mut().expect("a body");
        #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
        c.land()
            .load_block_cells(dereth_primitives::LandblockId((cell >> 16) as u16));
        c.teleport(Position::new(CellId(cell), Frame::new(p, Quat::IDENTITY)));
    }
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    for _ in 0..24 {
        t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(t),
            1.0 / 30.0,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(t),
            dereth_client_runtime::platform::clock::HEADLESS_STEP,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        let _ = gpu.capture().expect("capture");
    }
    let c = scene.character.as_ref().expect("a body");
    assert!(
        c.camera.stats.sweeps > 0,
        "no camera sweep ran, so the viewer cell this count is taken from is the body's"
    );
    let pos = c.position();
    eprintln!(
        "outside-view station {cell:#010X} in {block:#06X}: body settled in {:#010X}, viewer cell {:?}, \
         {} sweeps ({} blocked), traversal {:?}",
        pos.cell.0,
        scene.viewer_cell_id().map(|x| format!("{:#010X}", x.0)),
        c.camera.stats.sweeps,
        c.camera.stats.sweeps_blocked,
        scene.indoor_traversal_counts(),
    );
    let n = scene.indoor_outside_view_count();
    scene.release_textures(gpu);
    n
}

/// The control that makes the first test mean what it says: an **outdoor** viewer is untouched.
#[test]
fn an_outdoor_viewer_is_untouched() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let a = outdoor_frame(&store, &mut gpu, false);
    let b = outdoor_frame(&store, &mut gpu, true);
    assert_eq!(a.len(), b.len());
    let changed = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count();
    let painted = nonblack(&a);
    eprintln!(
        "outdoor control over {HOLTBURG:#06X}: {changed} of {} pixels differ; {painted} painted",
        a.len() / 4
    );
    assert!(
        painted > 100_000,
        "the outdoor frame is empty, so 'unchanged' proves nothing"
    );
    assert_eq!(
        changed, 0,
        "the gate changed an outdoor frame; it is keyed on the viewer's cell"
    );
}
