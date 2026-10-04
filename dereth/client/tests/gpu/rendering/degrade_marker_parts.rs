//! Level-designer markers are never drawn: a graphics object whose degrade record is itself at a
//! zero-width band followed by the empty terminator draws nothing at any distance, so the red cone
//! placed in the Holtburg training academy's library (cell `0x860201AD`, setup `0x02000C39`, part
//! `0x010028CA`) never appears, while every other placement in the room still draws. This is the
//! drawn half of the claim; which objects and placements the guard refuses is the `dat` tier's
//! `rendering::degrade_marker_parts`.
//!
//! Fixture: `client_cell_1.dat` and `client_portal.dat` from `$DERETH_TEST_DAT_DIR`, and a rendered
//! frame of the library with the guard on and off. Every fixture is an `expect`, so a missing
//! input fails rather than skips.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::character::CharacterInput;
use dereth_client::env_cells::{cell_statics, EnvCellLoader};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, LocalTime};
use dereth_render::device::Gpu;

/// The first Holtburg starter area's first instantiation cell and its landblock -- the
/// room a new Holtburg character wakes up in, and the room the marker stands in.
const TRAINING_DUNGEON: u16 = 0x8602;
const LIBRARY: u32 = 0x8602_01AD;

/// The library's marker placement: a setup whose one part is the cone.
const MARKER_SETUP: u32 = 0x0200_0C39;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Drive the app's own per-frame order and hand back the last capture as RGBA.
///
/// Every call below is one `App::frame` makes every frame, so the frame is the binary's and not a
/// bespoke loop.
fn run(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    frames: usize,
) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let mut rgba = Vec::new();
    for _ in 0..frames {
        t += dereth_client::app::HEADLESS_STEP;
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
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    rgba
}

/// The marker's own colours, as a pixel predicate. Its red ramp is `0xFFCE0000`..`0xFFF71010` and
/// its green ramp `0xFF006F07`..`0xFF00BE0B`, both far outside anything the room's own textures
/// produce -- the rug is red but not this red, and nothing in the room is this green.
fn marker_red(p: &[u8; 4]) -> bool {
    p[0] >= 150 && p[1] <= 60 && p[2] <= 60
}
fn marker_green(p: &[u8; 4]) -> bool {
    p[1] >= 110 && p[0] <= 60 && p[2] <= 60
}

/// Behaviour: rendering.markers.marker-placements-in-the-dat-are-never-drawn
/// Oracle: the rendered frame of the library with the guard on and off.
///
/// The differential is the whole evidence: the same scene, the same viewer, one frame with
/// the physics-part degrade guard honored and one without ([`SceneConfig::part_degrades`],
/// which exists for exactly this). The guarded frame must lose the marker's pixels and keep
/// everything else, so the change is bounded from **both** sides.
#[test]
fn the_library_draws_no_red_primitive() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let start = CellId(LIBRARY);

    // Stand where the offline client stands and face the marker, so that a marker that is drawn is
    // certainly in frame. Both points come out of the dat, not from taste.
    let marker = EnvCellLoader::new()
        .load_block(&store, TRAINING_DUNGEON)
        .iter()
        .filter(|d| d.id == start)
        .flat_map(cell_statics)
        .find(|s| s.id.0 == MARKER_SETUP)
        .expect("the library places the marker")
        .frame
        .origin;

    let shot = |gpu: &mut Gpu, guard: bool| -> (Vec<u8>, u32) {
        let cfg = SceneConfig {
            landblock: TRAINING_DUNGEON,
            start_cell: Some(start),
            part_degrades: guard,
            // `part_degrade_levels` is gated by `part_degrades`, so leaving it at its default
            // would vary two things between the arms at once: the near-band guard, which is this
            // test's subject, and the *meshes*, because part loading bakes the
            // record's level 0 rather than the part's own id and 16 of the body's 34 parts name
            // a record whose level 0 differs. Measured with it left on: 5,347 changed pixels
            // against a marker covering 1,601, i.e. the body itself moved. Pinned off in both
            // arms.
            part_degrade_levels: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, gpu, cfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, gpu)
            .expect("the body is created");
        // `SceneConfig::start_cell` already stands the body on the cell's own standable point and
        // aims it at the furthest of that cell's statics, which is what `--start-cell` does in the
        // binary, with the red cone in frame, and neither arm may move it, or the control would
        // not be one.
        let stand = scene
            .standable_point(start)
            .expect("the library has a standable point");
        assert!(
            math::hypotf(marker.x - stand.x, marker.y - stand.y) > 1.0,
            "the standable point is on top of the marker"
        );
        let rgba = run(&store, gpu, &mut scene, 4);
        (rgba, scene.draw.stats.parts_not_drawn)
    };

    let (unguarded, none_refused) = shot(&mut gpu, false);
    let (guarded, refused) = shot(&mut gpu, true);
    assert_eq!(
        none_refused, 0,
        "the control arm refused parts, so it is not the unguarded draw"
    );
    assert!(
        refused > 0,
        "the guard refused nothing, so the frame proves nothing"
    );
    eprintln!("parts refused by the degrade guard over the baked window: {refused}");

    let (u, g) = (unguarded.as_chunks::<4>().0, guarded.as_chunks::<4>().0);
    let ur = u.iter().filter(|p| marker_red(p)).count();
    let ug = u.iter().filter(|p| marker_green(p)).count();
    let gr = g.iter().filter(|p| marker_red(p)).count();
    let gg = g.iter().filter(|p| marker_green(p)).count();
    eprintln!("marker-coloured pixels: unguarded red {ur} green {ug}; guarded red {gr} green {gg}");
    assert!(
        ug > 0,
        "the control frame has no marker green, so the marker was never in frame"
    );
    assert!(
        ur > gr,
        "the guarded frame is not less red than the unguarded one"
    );
    assert_eq!(gg, 0, "{gg} marker-green pixels survive the guard");

    // Nothing else moved: every changed pixel is one the marker covered. A guard that hid the
    // furniture as well would pass the two counts above and fail here.
    let changed = u.iter().zip(g.iter()).filter(|(a, b)| a != b).count();
    let covered = u
        .iter()
        .filter(|p| marker_red(p) || marker_green(p))
        .count();
    eprintln!(
        "{changed} of {} pixels changed; the marker covered {covered}",
        u.len()
    );
    assert!(changed > 0, "the guard changed no pixel at all");
    assert!(
        changed <= covered + covered / 4,
        "{changed} pixels changed but the marker only covered {covered}"
    );
}
