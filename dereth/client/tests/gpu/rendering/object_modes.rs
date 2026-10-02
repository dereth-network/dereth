//! The object modes: the world's objects drawn with another era's look, on any world and switched
//! while it is drawn.
//!
//! An object keeps the world's setup (its parts and how the world's motion data moves them) and
//! takes the other era's models, surfaces, pictures and palettes for its parts; the landscape's
//! scenery, buildings and statics are drawn whole from the other era's files. So the end-of-retail
//! world is drawn here with the February 2005 look (that set's `portal.dat` beside it for
//! presentation only) and the February 2005 world with the end-of-retail look (the later files
//! beside it), and a change of mode rebuilds the town and the body in place. A mode whose files
//! are not present is refused and the world keeps what it had.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`), Holtburg with the offline body at a pinned time of day, and a
//! graphics device; a missing input fails.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::objects::ObjectStream;
use dereth_client::render_prefs::{RegionStyle, RequiredFiles};
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;
use dereth_render::device::Gpu;
use std::sync::Arc;

/// The end-of-retail world with the February 2005 `portal.dat` beside it for presentation, or a
/// failed test.
fn end_of_retail_with_legacy_files() -> Arc<RetailDatStore> {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let legacy = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    Arc::new(
        dereth_dat::testing::open_store_or_fail()
            .with_legacy_portal(&legacy)
            .unwrap_or_else(|e| panic!("the February 2005 portal did not attach: {e}")),
    )
}

/// The February 2005 world with the end-of-retail files beside it, or a failed test.
fn older_world() -> Arc<RetailDatStore> {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let world = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    Arc::new(
        RetailDatStore::open_pre_tod_with_later(&world, &dereth_dat::testing::dat_dir())
            .unwrap_or_else(|e| panic!("the February 2005 world did not open: {e}")),
    )
}

/// Holtburg with no particles, the weather off and a pinned clock, drawn with `objects`.
fn cfg(objects: Option<RegionStyle>) -> SceneConfig {
    SceneConfig {
        landblock: 0xA9B4,
        character: false,
        particles: false,
        land_radius: 2,
        scenery_radius: 1,
        time_of_day: Some(0.5),
        game_time: Some(0.0),
        camera_height: 0.0,
        render: dereth_client::render_prefs::RenderPreferences {
            objects,
            ..SceneConfig::default().render
        },
        ..SceneConfig::default()
    }
}

/// Load the scene, attach the offline body and look down across the town at it, the weather off:
/// a still view of buildings, statics, scenery and the body.
fn load(store: &Arc<RetailDatStore>, gpu: &mut Gpu, cfg: SceneConfig) -> WorldScene {
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene.set_weather_enabled(false);
    let p = scene.camera.position;
    scene.camera.position = dereth_primitives::Vec3::new(p.x, p.y + 0.85 * 192.0, p.z + 40.0);
    scene.camera.yaw = 0.0;
    scene.camera.pitch = -0.9;
    scene
}

/// Draw a few frames at the pinned clock; the last frame's RGBA.
fn draw(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> Vec<u8> {
    let mut rgba = Vec::new();
    for _ in 0..3 {
        let mut stream = ObjectStream::new();
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(0.0),
            0.0,
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

/// The human male setup's 17 parts in February 2005 (the later setup's first 17 are the same).
const FEBRUARY_2005_BODY: [dereth_primitives::DataId; 17] = ids([
    0x0100_004E,
    0x0100_004F,
    0x0100_004D,
    0x0100_004C,
    0x0100_004B,
    0x0100_0053,
    0x0100_0051,
    0x0100_0050,
    0x0100_0052,
    0x0100_0054,
    0x0100_0055,
    0x0100_0056,
    0x0100_0058,
    0x0100_0057,
    0x0100_0059,
    0x0100_005B,
    0x0100_005A,
]);

/// What the later files draw for the first sixteen of those at the nearest degrade level.
const LATER_BODY_NEAREST: [dereth_primitives::DataId; 16] = ids([
    0x0100_1787,
    0x0100_1793,
    0x0100_178D,
    0x0100_1789,
    0x0100_1791,
    0x0100_1794,
    0x0100_178E,
    0x0100_178A,
    0x0100_1792,
    0x0100_1788,
    0x0100_1795,
    0x0100_178F,
    0x0100_178B,
    0x0100_1796,
    0x0100_1790,
    0x0100_178C,
]);

const fn ids<const N: usize>(raw: [u32; N]) -> [dereth_primitives::DataId; N] {
    let mut out = [dereth_primitives::DataId(0); N];
    let mut i = 0;
    while i < N {
        out[i] = dereth_primitives::DataId(raw[i]);
        i += 1;
    }
    out
}

fn moved(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// Behaviour: rendering.objects.an-object-mode-switch-redraws-the-town-and-the-body-while-the-world-is-drawn
/// The end-of-retail world, drawn with its own objects, is switched to the February 2005 look and
/// back through the same per-frame preference poll the options page reaches. The switch rebuilds
/// every block (its buildings, statics and scenery take the older look) and the body, which keeps
/// the world's 34-part setup and draws its parts with the older files' records (the older parts
/// themselves where the later files draw finer meshes through their degrade records); the frame
/// moves.
/// Switching back draws exactly the frame the world was first drawn with. The scenery, building
/// and static counts never change.
#[test]
fn an_object_mode_switch_rebuilds_the_town_and_the_body_and_switching_back_restores_the_frame() {
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::software_gpu(640, 480);
    let mut scene = load(&store, &mut gpu, cfg(None));
    assert!(!scene.draw.objects_from_other_files());
    let own_px = draw(&mut scene, &store, &mut gpu);
    assert_eq!(
        moved(&own_px, &draw(&mut scene, &store, &mut gpu)),
        0,
        "the view is not still, so this test cannot see"
    );
    let body = scene.character_built_from().to_vec();
    assert_eq!(body.len(), 34, "the end-of-retail body");
    assert_eq!(&body[..16], &LATER_BODY_NEAREST[..]);
    let census = dereth_client_runtime::present::Scene::census(&scene);

    let switch = |scene: &mut WorldScene, gpu: &mut Gpu, style: Option<RegionStyle>| {
        scene.draw.cfg.render.objects = style;
        let work = scene
            .update_from_preferences(&store, gpu)
            .expect("the poll applies the objects' look");
        assert!(
            work.objects_changed,
            "{style:?}: the poll did not change the objects"
        );
        assert_eq!(work.objects_refused, None);
        assert!(work.blocks_rebuilt > 0, "{style:?}: nothing was rebuilt");
        assert_eq!(scene.draw.objects_style(), style);
        draw(scene, &store, gpu)
    };

    let looks_before = scene.draw.stats.object_appearances_from_look;
    let legacy_px = switch(&mut scene, &mut gpu, Some(RegionStyle::LegacyHardware));
    assert!(scene.draw.objects_from_other_files());
    assert!(
        scene.draw.stats.object_appearances_from_look > looks_before,
        "the body was not built with the older look"
    );
    // The body keeps the world's 34 parts. What each draws is the older look's: the later files
    // give the first sixteen a degrade record whose nearest level is a finer mesh, the older
    // files have no such record, so the parts draw themselves.
    let older_body = scene.character_built_from().to_vec();
    assert_eq!(older_body.len(), 34, "the body keeps the world's setup");
    assert_eq!(&older_body[..17], &FEBRUARY_2005_BODY[..]);
    assert_eq!(&older_body[17..], &body[17..]);
    let legacy_moved = moved(&own_px, &legacy_px);
    assert!(
        legacy_moved > 10_000,
        "the older look draws like the world's own: {legacy_moved}"
    );
    let after = dereth_client_runtime::present::Scene::census(&scene);
    assert_eq!(
        (after.scenery_objects, after.buildings, after.static_objects),
        (
            census.scenery_objects,
            census.buildings,
            census.static_objects
        ),
        "an object mode changed what the landscape places"
    );

    let back_px = switch(&mut scene, &mut gpu, None);
    assert!(!scene.draw.objects_from_other_files());
    assert_eq!(
        moved(&own_px, &back_px),
        0,
        "switching back to the world's own objects did not restore its frame"
    );
    eprintln!(
        "object mode switch: the older look moved {legacy_moved} of {} bytes; back to the \
         world's own moved 0",
        own_px.len()
    );
}

/// Behaviour: rendering.objects.an-older-world-draws-its-objects-with-the-later-look
/// The February 2005 world loaded with the end-of-retail look draws its town and its body with the
/// later files' records: the frame differs from the world's own, and the body keeps the world's
/// 17-part setup (the later setup has 34), its first sixteen parts drawn as the later files draw
/// them, through their degrade records' finer meshes.
#[test]
fn an_older_world_draws_its_objects_with_the_later_look_and_keeps_its_own_setups() {
    let store = older_world();
    let mut gpu = crate::common::software_gpu(640, 480);
    let mut own = load(&store, &mut gpu, cfg(None));
    let own_px = draw(&mut own, &store, &mut gpu);
    let own_body = own.character_built_from().to_vec();
    assert_eq!(own_body, FEBRUARY_2005_BODY, "the February 2005 body");
    assert!(!own.draw.objects_from_other_files());
    drop(own);

    let mut later = load(&store, &mut gpu, cfg(Some(RegionStyle::Modern)));
    assert!(later.draw.objects_from_other_files());
    assert_eq!(later.draw.objects_style(), Some(RegionStyle::Modern));
    let later_px = draw(&mut later, &store, &mut gpu);
    // The world's 17 parts, each drawn as the later files draw it: the first sixteen through
    // their later degrade record's nearest mesh.
    let later_body = later.character_built_from().to_vec();
    assert_eq!(later_body.len(), 17, "the body keeps the world's setup");
    assert_eq!(&later_body[..16], &LATER_BODY_NEAREST[..]);
    assert_eq!(later_body[16], own_body[16]);
    assert!(later.draw.stats.object_appearances_from_look > 0);
    assert_eq!(later.draw.stats.object_appearances_from_world, 0);
    let n = moved(&own_px, &later_px);
    assert!(n > 10_000, "the later look draws like the world's own: {n}");
    eprintln!(
        "February 2005 world with the later look: {n} of {} bytes moved",
        own_px.len()
    );
}

/// Behaviour: rendering.objects.an-object-mode-without-its-files-is-refused
/// With no older files beside the end-of-retail world, asking for the older look is refused: the
/// poll reports the files it needed and the look kept, the objects stay the world's, and the
/// frame does not move.
#[test]
fn an_object_mode_without_its_files_is_refused_and_the_world_keeps_its_own() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut gpu = crate::common::software_gpu(640, 480);
    let mut scene = load(&store, &mut gpu, cfg(None));
    let own_px = draw(&mut scene, &store, &mut gpu);
    scene.draw.cfg.render.objects = Some(RegionStyle::LegacyHardware);
    let work = scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll runs");
    assert_eq!(work.objects_refused, Some((RequiredFiles::Legacy, None)));
    assert!(!work.objects_changed);
    assert_eq!(scene.draw.objects_style(), None);
    assert!(!scene.draw.objects_from_other_files());
    assert_eq!(moved(&own_px, &draw(&mut scene, &store, &mut gpu)), 0);
}

/// A headless client over the end-of-retail world at Holtburg (no body, no UI, a pinned clock),
/// with `legacy` beside it for presentation and `set_at` settings made part way through, as
/// `--legacy-dat-dir` and `--set-at` make them.
fn client(
    legacy: Option<std::path::PathBuf>,
    set_at: Vec<(u64, String)>,
) -> dereth_client::app::App {
    let config = dereth_client::config::Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dereth-object-modes-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        legacy_dat_dir: legacy,
        set_at,
        ..dereth_client::config::Config::default()
    };
    let mut app = dereth_client::app::App::new(config)
        .unwrap_or_else(|e| panic!("the headless client did not start: {e}"));
    app.load_static_scene(cfg(None))
        .unwrap_or_else(|e| panic!("the static scene did not load: {e}"));
    app
}

/// Behaviour: rendering.objects.an-object-mode-without-its-files-is-refused
/// In the running client, choosing the Legacy object mode on the end-of-retail world with no
/// older files beside it leaves the world's own objects, puts the option back to World Default,
/// and tells the player in the chat window: "This object mode requires legacy DATs". With the
/// older files beside it, the same setting draws the objects with the older look on the next
/// frame.
#[test]
fn the_client_says_which_files_a_refused_object_mode_needs_and_switches_when_they_are_there() {
    use dereth_client_contract::options::{landscape, store};
    store::init();
    let mut app = client(None, vec![(3, "Render.Objects=Legacy".to_string())]);
    let before = app.objects().world.scroll.added;
    for _ in 0..4 {
        assert!(app.frame());
    }
    let work = app.last_render_pref_work();
    assert_eq!(work.objects_refused, Some((RequiredFiles::Legacy, None)));
    assert!(!app
        .world_scene()
        .expect("a world")
        .draw
        .objects_from_other_files());
    assert_eq!(
        store::inq_value(landscape::OBJECTS),
        Some(dereth_ui_screens::PrefValue::Int(landscape::WORLD_DEFAULT)),
        "the option went back to the world's own"
    );
    assert_eq!(app.objects().world.scroll.added, before + 1);
    assert!(app
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .any(|l| l.body == "This object mode requires legacy DATs"
            && l.chat_type == dereth_client_model::scroll::LOCAL_ERROR_TYPE));
    drop(app);

    store::init();
    let legacy = dereth_dat::testing::pre_tod_dat_dir()
        .unwrap_or_else(|| panic!("{:?}", dereth_dat::testing::pre_tod_shortfall()));
    let mut app = client(Some(legacy), vec![(3, "Render.Objects=Legacy".to_string())]);
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(!app
        .world_scene()
        .expect("a world")
        .draw
        .objects_from_other_files());
    assert!(app.frame());
    let work = app.last_render_pref_work();
    assert!(work.objects_changed, "{work:?}");
    let s = app.world_scene().expect("a world");
    assert!(s.draw.objects_from_other_files());
    assert_eq!(s.draw.objects_style(), Some(RegionStyle::LegacyHardware));
}
