//! The object modes: the world's objects drawn with another era's look, on any world and switched
//! while it is drawn.
//!
//! An object keeps the world's setup (its parts and how the world's motion data moves them), and
//! each part takes the other era's model, surfaces, pictures and palettes where the other era's
//! model is the same object (a bare body part takes that era's bare part); the landscape's
//! scenery, buildings and statics are drawn whole from one era each. So the end-of-retail
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

/// What the February 2005 look draws for the end-of-retail body's first 17 parts: the same models,
/// except that the bare arms and hands are that era's bare arms and hands (the end-of-retail bare
/// arm is the older files' armoured one).
const FEBRUARY_2005_LOOK_OF_THE_LATER_BODY: [dereth_primitives::DataId; 17] = ids([
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
    0x0100_0497,
    0x0100_0495,
    0x0100_0076,
    0x0100_04AD,
    0x0100_0496,
    0x0100_0077,
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
/// every block (its buildings, statics and scenery take the older look where the older files hold
/// the same objects) and the body, which keeps the world's 34-part setup and draws its parts
/// with the older files' records: the same models where they are the same (with the same finer
/// nearest meshes, which the older files reach through the records their ids reach), and the
/// older bare arms and hands for the later bare ones; the frame moves.
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
    // The body keeps the world's 34 parts. What each draws is the older look's: the older files
    // reach the same finer nearest meshes for the first ten through the records their ids
    // reach, and the bare arms and hands are the older bare ones, which are their own nearest.
    let older_body = scene.character_built_from().to_vec();
    assert_eq!(older_body.len(), 34, "the body keeps the world's setup");
    assert_eq!(&older_body[..10], &LATER_BODY_NEAREST[..10]);
    assert_eq!(
        &older_body[10..17],
        &FEBRUARY_2005_LOOK_OF_THE_LATER_BODY[10..]
    );
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
    // The February 2005 body, its first sixteen parts at the nearest level of the records their
    // ids reach (the same meshes the later files name), the head its own.
    let own_body = own.character_built_from().to_vec();
    assert_eq!(
        &own_body[..16],
        &LATER_BODY_NEAREST[..],
        "the February 2005 body"
    );
    assert_eq!(own_body[16], FEBRUARY_2005_BODY[16]);
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

/// Behaviour: rendering.degrade.the-degrade-distance-applies-on-an-older-world
/// The February 2005 world takes the Degrade Distance setting as the end-of-retail world does,
/// whichever era's look its objects are drawn with: 0, the default 50 and 100 each reach the
/// detail choice as they are.
#[test]
fn the_february_2005_world_takes_the_degrade_distance_setting_with_either_look() {
    let store = older_world();
    let mut gpu = crate::common::software_gpu(640, 480);
    for objects in [None, Some(RegionStyle::Modern)] {
        for setting in [0.0f32, 50.0, 100.0] {
            let mut c = cfg(objects);
            c.render.degrade_distance = setting;
            let scene = load(&store, &mut gpu, c);
            assert_eq!(
                scene.degrade_globals().degrade_distance,
                setting,
                "the February 2005 world with the {objects:?} look"
            );
        }
    }
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

/// Behaviour: rendering.objects.the-paper-doll-wears-the-bodys-look
/// The paper doll is the player's setup dressed in the player's description in a preview space
/// of its own. With the end-of-retail world drawn in the February 2005 look, the doll is built
/// from the same models the body in the world draws, the older bare arms and hands among them;
/// with the world's own look it is built from the world's.
#[test]
fn the_paper_doll_wears_the_look_the_body_wears_in_the_world() {
    use dereth_client::anim_assets::DatAnimAssets;
    use dereth_client::preview::PreviewSpace;
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::software_gpu(320, 240);
    let scene = load(&store, &mut gpu, cfg(Some(RegionStyle::LegacyHardware)));
    assert!(scene.draw.objects_from_other_files());
    let body = scene.character_built_from().to_vec();
    assert_eq!(&body[..10], &LATER_BODY_NEAREST[..10]);
    assert_eq!(&body[10..17], &FEBRUARY_2005_LOOK_OF_THE_LATER_BODY[10..]);
    let look = scene.draw.object_look().expect("the older look");
    let assets = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let mut doll = PreviewSpace::new(Arc::clone(&assets));
    let i = doll
        .add_object_dressed_in_look(
            &store,
            &mut gpu,
            dereth_primitives::DataId(0x0200_0001),
            None,
            Some((&look.0, &look.1)),
        )
        .expect("the doll bakes")
        .expect("the setup loads");
    assert_eq!(
        &doll.object(i).expect("the doll").built_from()[..17],
        &body[..17],
        "the doll wears what the body wears"
    );
    let mut own = PreviewSpace::new(assets);
    let j = own
        .add_object_dressed(
            &store,
            &mut gpu,
            dereth_primitives::DataId(0x0200_0001),
            None,
        )
        .expect("the doll bakes")
        .expect("the setup loads");
    assert_eq!(
        &own.object(j).expect("the doll").built_from()[..16],
        &LATER_BODY_NEAREST[..],
        "the world's own doll"
    );
}

/// Holtburg's resident interior cells, and whether each draws the other era's room.
fn holtburg_rooms(scene: &WorldScene) -> Vec<(u32, bool)> {
    scene
        .draw
        .interior_looks()
        .into_iter()
        .filter(|(c, _)| c.0 >> 16 == 0xA9B4)
        .map(|(c, l)| (c.0, l))
        .collect()
}

/// Behaviour: rendering.objects.interiors-follow-their-building-in-another-eras-look
/// Holtburg's rooms are drawn with their buildings. The end-of-retail world with its own look draws
/// every room from its own cell file; switched to the February 2005 look, every room the older
/// cell file holds as the same room in the same place is drawn from the older record of it (all
/// but `0xA9B40123`), and switching back draws them all from the world's again. The February 2005
/// world with the end-of-retail look draws the later rooms except the two cottages' (`0xA9B40180`
/// to `0xA9B40189`) the later files hold elsewhere.
#[test]
fn holtburgs_rooms_take_the_other_eras_look_with_their_buildings_and_go_back_with_them() {
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::software_gpu(640, 480);
    let mut scene = load(&store, &mut gpu, cfg(None));
    draw(&mut scene, &store, &mut gpu);
    let own = holtburg_rooms(&scene);
    assert_eq!(own.len(), 0x7B, "Holtburg's interior cells");
    assert!(own.iter().all(|(_, l)| !l), "the world's own look");

    scene.draw.cfg.render.objects = Some(RegionStyle::LegacyHardware);
    let work = scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll applies the objects' look");
    assert!(work.objects_changed);
    draw(&mut scene, &store, &mut gpu);
    let older = holtburg_rooms(&scene);
    assert_eq!(older.len(), own.len());
    let kept: Vec<u32> = older.iter().filter(|(_, l)| !l).map(|(c, _)| *c).collect();
    assert_eq!(kept, [0xA9B4_0123], "the rooms kept in the world's look");

    scene.draw.cfg.render.objects = None;
    scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll restores the world's own look");
    draw(&mut scene, &store, &mut gpu);
    assert_eq!(holtburg_rooms(&scene), own, "switching back");
    drop(scene);

    let store = older_world();
    let mut later = load(&store, &mut gpu, cfg(Some(RegionStyle::Modern)));
    draw(&mut later, &store, &mut gpu);
    let rooms = holtburg_rooms(&later);
    let kept: Vec<u32> = rooms.iter().filter(|(_, l)| !l).map(|(c, _)| *c).collect();
    let mut expected = vec![0xA9B4_0123];
    expected.extend(0xA9B4_0180..=0xA9B4_0189);
    assert_eq!(kept, expected, "the rooms the later files hold elsewhere");
    assert!(rooms.len() > kept.len() + 100, "{} rooms", rooms.len());
}

/// The verdicts for `store`'s world and the other era's files beside it, as the application
/// works them out.
fn identity_of(
    store: &RetailDatStore,
) -> Arc<dereth_client_runtime::object_identity::ObjectIdentity> {
    use dereth_client_runtime::object_identity::{Budget, IdentityBuild};
    let mut build = IdentityBuild::for_store(store, None).expect("the other era is beside");
    build.step(Budget::All).expect("all at once")
}

/// Behaviour: rendering.objects.an-object-mode-asked-for-before-its-verdicts-are-ready-is-drawn-when-they-arrive
/// A scene whose application works the verdicts out in the background keeps the look it has
/// while they are not ready. Loaded with the Legacy look asked for, the end-of-retail world is
/// drawn with its own objects and the preference poll says it is waiting; a live switch back and
/// forth while waiting changes nothing. The frame the verdicts are handed over, the next poll
/// draws the older look, exactly as a scene that had them from the start draws it, and a later
/// switch needs nothing more.
#[test]
fn an_object_mode_asked_for_before_its_verdicts_are_ready_keeps_the_look_until_they_arrive() {
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::software_gpu(640, 480);
    let background = SceneConfig {
        object_identity_budget: Some(std::time::Duration::from_millis(3)),
        ..cfg(Some(RegionStyle::LegacyHardware))
    };
    let mut scene = load(&store, &mut gpu, background);
    assert!(!scene.draw.objects_from_other_files());
    let own_px = draw(&mut scene, &store, &mut gpu);
    for _ in 0..2 {
        let work = scene
            .update_from_preferences(&store, &mut gpu)
            .expect("the poll runs");
        assert!(work.objects_waiting, "{work:?}");
        assert!(!work.objects_changed);
        assert_eq!(work.objects_refused, None);
        assert!(!scene.draw.objects_from_other_files());
    }
    assert_eq!(
        moved(&own_px, &draw(&mut scene, &store, &mut gpu)),
        0,
        "the world's own look moved while waiting"
    );
    // Back to the world's own and to Legacy again, still waiting: nothing to do.
    scene.draw.cfg.render.objects = None;
    let work = scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll runs");
    assert!(!work.objects_waiting && !work.objects_changed, "{work:?}");
    scene.draw.cfg.render.objects = Some(RegionStyle::LegacyHardware);
    let work = scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll runs");
    assert!(work.objects_waiting, "{work:?}");

    scene.draw.offer_object_identity(identity_of(&store));
    let work = scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll applies the look");
    assert!(work.objects_changed && !work.objects_waiting, "{work:?}");
    assert!(scene.draw.objects_from_other_files());
    let legacy_px = draw(&mut scene, &store, &mut gpu);
    drop(scene);

    let mut at_once = load(&store, &mut gpu, cfg(Some(RegionStyle::LegacyHardware)));
    assert!(at_once.draw.objects_from_other_files());
    assert_eq!(
        moved(&legacy_px, &draw(&mut at_once, &store, &mut gpu)),
        0,
        "the look drawn once the verdicts arrived is not the look drawn with them from the start"
    );
}

/// Behaviour: rendering.objects.an-object-mode-asked-for-before-its-verdicts-are-ready-is-drawn-when-they-arrive
/// The running client starts working the verdicts out the frame it starts, a few milliseconds
/// a frame, its cache kept in the host's own store. Choosing the Legacy object mode before they
/// are ready keeps the world's own objects, tells the player once in the chat window that the
/// look is still being prepared, and gives the work more of each frame; the frame the verdicts
/// are ready, the objects are drawn with the older look. No frame gave the work much more than
/// its share.
#[test]
fn the_client_prepares_the_verdicts_from_start_up_and_draws_a_look_asked_for_early_when_ready() {
    use dereth_client_contract::options::store;
    store::init();
    // The cache goes to a store in memory on this thread, never the player's own folder.
    dereth_client_runtime::platform::files::install(memory_files::HOST);
    let legacy = dereth_dat::testing::pre_tod_dat_dir()
        .unwrap_or_else(|| panic!("{:?}", dereth_dat::testing::pre_tod_shortfall()));
    let config = dereth_client::config::Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dereth-object-modes-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        legacy_dat_dir: Some(legacy),
        set_at: vec![(3, "Render.Objects=Legacy".to_string())],
        object_identity_ms: Some(3),
        ..dereth_client::config::Config::default()
    };
    let mut app = dereth_client::app::App::new(config)
        .unwrap_or_else(|e| panic!("the headless client did not start: {e}"));
    let units_at_start = app
        .object_identity
        .as_ref()
        .expect("the verdicts start with the client")
        .build
        .units();
    assert_eq!(units_at_start, 0);
    let background_cfg = SceneConfig {
        object_identity_budget: Some(std::time::Duration::from_millis(3)),
        ..cfg(None)
    };
    app.load_static_scene(background_cfg)
        .unwrap_or_else(|e| panic!("the static scene did not load: {e}"));
    let before = app.objects().world.scroll.added;
    for _ in 0..4 {
        assert!(app.frame());
    }
    let work = app.last_render_pref_work();
    assert!(work.objects_waiting, "{work:?}");
    assert!(!app
        .world_scene()
        .expect("a world")
        .draw
        .objects_from_other_files());
    assert_eq!(app.objects().world.scroll.added, before + 1);
    let notice = "The objects' look is still being prepared; it is drawn as soon as it is ready.";
    assert!(app
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .any(|l| l.body == notice));
    let mut frames = 4;
    while !app.object_identity.as_ref().is_some_and(|p| p.offered) {
        assert!(
            frames < 20_000,
            "the verdicts were not ready in {frames} frames"
        );
        assert!(app.frame());
        frames += 1;
    }
    let work = app.last_render_pref_work();
    assert!(work.objects_changed, "{work:?}");
    assert!(app
        .world_scene()
        .expect("a world")
        .draw
        .objects_from_other_files());
    assert_eq!(
        app.objects().world.scroll.added,
        before + 1,
        "the player was told more than once"
    );
    let prep = app.object_identity.as_ref().expect("the verdicts");
    let source = prep.build.source().expect("ready");
    assert!(!source.from_cache, "{source:?}");
    assert_eq!(source.files_hashed, 4);
    eprintln!(
        "ready after {frames} frames, {} steps, {} units, the longest frame's share {:.1} ms",
        prep.build.steps(),
        prep.build.units(),
        prep.longest_step.as_secs_f64() * 1000.0
    );
    assert!(
        prep.longest_step < std::time::Duration::from_millis(40),
        "a frame gave the work {:?}",
        prep.longest_step
    );
}

/// A store of the client's own files held in memory on this thread, as a host without a disk
/// keeps them.
mod memory_files {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    use dereth_client_runtime::platform::files::FileHost;

    thread_local! {
        static FILES: RefCell<BTreeMap<PathBuf, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    }

    pub const HOST: FileHost = FileHost {
        read: |p| {
            FILES
                .with(|m| m.borrow().get(p).cloned())
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        },
        write: |p, bytes| {
            FILES.with(|m| m.borrow_mut().insert(p.to_path_buf(), bytes.to_vec()));
            Ok(())
        },
        list: |dir| {
            Ok(FILES.with(|m| {
                m.borrow()
                    .keys()
                    .filter(|p| p.parent() == Some(dir))
                    .cloned()
                    .collect()
            }))
        },
        exists: |p| FILES.with(|m| m.borrow().contains_key(p)),
        read_only: |_| Ok(false),
        make_dirs: |_| Ok(()),
        cache_dir: || Some(Path::new("/memory/cache").to_path_buf()),
    };
}
