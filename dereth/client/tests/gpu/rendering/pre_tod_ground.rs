//! The ground of a world from before Throne of Destiny: Classic Blend (its hardware region),
//! Palette Shift (its software region) and Modern Blend (the end-of-retail ground).
//!
//! An older dat set carries two regions, and the clients of the time took the texture-merge one
//! when they drew with 3D hardware and the palette-shift one in software. This client draws with
//! hardware and takes the first by default, with its detail textures; `[Render] Ground =
//! PaletteShift` takes the second, which names no detail texture; `[Render] Ground = ModernBlend`
//! draws the cells with the end-of-retail files' texture-merge land surface and landscape detail
//! texture while the scenery stays the world's own. The end-of-retail world alone, with no older
//! files beside it, has no older ground to take.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`), Holtburg with no body at a pinned time of day, and a graphics
//! device; a missing input fails.

#![cfg(gpu)]

use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::render_prefs::RegionStyle;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime};
use dereth_render::device::Gpu;
use dereth_world_render::detail::DetailClass;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// The February 2005 world with the end-of-retail files beside it, or a failed test.
fn older_world() -> Arc<RetailDatStore> {
    if let Some(msg) = dereth_dat::testing::classic_shortfall() {
        panic!("{msg}");
    }
    let world = dereth_dat::testing::classic_dat_dir().unwrap_or_default();
    Arc::new(
        RetailDatStore::open_classic_with_modern(&world, &dereth_dat::testing::dat_dir())
            .unwrap_or_else(|e| panic!("the February 2005 world did not open: {e}")),
    )
}

/// Holtburg with no body, no particles and a pinned clock, with the landscape detail texture on.
fn cfg(ground: Option<RegionStyle>) -> SceneConfig {
    SceneConfig {
        landblock: 0xA9B4,
        character: false,
        particles: false,
        land_radius: 2,
        scenery_radius: 1,
        time_of_day: Some(0.5),
        game_time: Some(0.0),
        camera_height: 0.0,
        render: dereth_client_runtime::render_prefs::RenderPreferences {
            landscape_detail_textures: true,
            ground,
            ..SceneConfig::default().render
        },
        ..SceneConfig::default()
    }
}

/// Load the scene and draw a few frames from over the block; the last frame's RGBA.
fn load_and_draw(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    cfg: SceneConfig,
) -> (WorldScene, Vec<u8>) {
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    // Over the middle of the block, forty metres above its highest ground, looking down across it.
    let p = scene.camera.position;
    scene.camera.position = dereth_primitives::Vec3::new(p.x, p.y + 0.85 * 192.0, p.z + 40.0);
    scene.camera.yaw = 0.0;
    scene.camera.pitch = -0.7;
    let mut rgba = Vec::new();
    for i in 0..4 {
        let mut stream = ObjectStream::new();
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(f64::from(i) / 30.0),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    (scene, rgba)
}

/// Behaviour: rendering.terrain.an-older-world-draws-the-ground-its-hardware-client-drew
/// The February 2005 world is drawn with its hardware region: texture-merged ground, the landscape
/// detail texture (`0x05001786`, eight times a cell) and the building and environment one
/// (`0x05001787`, three times). With Palette Shift its software region's land surface
/// palette-shifts the ground and names no detail texture, so the detail textures come from the
/// world's hardware region, and the ground is a different picture.
#[test]
fn an_older_world_draws_the_hardware_ground_and_on_request_the_software_one() {
    let store = older_world();
    let mut gpu = crate::common::test_gpu(640, 480);
    let (hardware, hardware_px) = load_and_draw(&store, &mut gpu, cfg(None));
    assert!(
        !hardware.draw.ground_palette_shifts(),
        "the hardware region texture-merges"
    );
    assert!(hardware.draw.ground_is_worlds_own());
    let d = hardware.detail_texturing();
    let land = d
        .surface(DetailClass::Landscape)
        .expect("a landscape detail texture");
    assert_eq!(land.texture, DataId(0x0500_1786));
    assert!((land.tiling - 8.0).abs() < f32::EPSILON, "{}", land.tiling);
    let building = d
        .surface(DetailClass::Building)
        .expect("a building detail texture");
    assert_eq!(building.texture, DataId(0x0500_1787));
    assert!(
        (building.tiling - 3.0).abs() < f32::EPSILON,
        "{}",
        building.tiling
    );
    let hardware_census = dereth_client_runtime::present::Scene::census(&hardware);
    drop(hardware);

    let (software, software_px) =
        load_and_draw(&store, &mut gpu, cfg(Some(RegionStyle::LegacySoftware)));
    assert!(software.draw.ground_palette_shifts());
    // The palette-shift land surface names no detail texture, so with the preference on the
    // ground still gets one: the world's own hardware region's, eight times a cell.
    let land = software
        .detail_texturing()
        .surface(DetailClass::Landscape)
        .expect("the world's landscape detail texture under Palette Shift");
    assert_eq!(land.texture, DataId(0x0500_1786));
    assert!((land.tiling - 8.0).abs() < f32::EPSILON, "{}", land.tiling);
    assert_eq!(
        software.draw.detail_source_used(),
        dereth_scene::world_scene::DetailSource::World
    );
    assert!(
        !software.terrain_composites().is_empty(),
        "no ground surfaces"
    );
    let software_census = dereth_client_runtime::present::Scene::census(&software);
    assert_eq!(
        (software_census.scenery_objects, software_census.buildings),
        (hardware_census.scenery_objects, hardware_census.buildings),
        "the two regions place the same scenery"
    );
    let moved = hardware_px
        .iter()
        .zip(&software_px)
        .filter(|(a, b)| a != b)
        .count();
    assert!(moved > 10_000, "the two grounds look alike: {moved} bytes");
    eprintln!("hardware against software region: {moved} bytes differ");
}

/// Behaviour: rendering.terrain.the-later-ground-draws-an-older-world-with-the-later-land-surface
/// With Modern Blend asked for, the February 2005 cells are drawn with the end-of-retail region's
/// texture-merge land surface and its landscape detail texture, the ground's pixels change, and
/// the scenery, buildings and objects are exactly the world's own. Over the end-of-retail world
/// with no older files beside it, an older ground cannot be drawn and the world's own is.
#[test]
fn the_later_ground_draws_an_older_world_with_the_later_land_surface_under_its_own_scenery() {
    let store = older_world();
    let mut gpu = crate::common::test_gpu(640, 480);
    let (own, own_px) = load_and_draw(&store, &mut gpu, cfg(None));
    let own_census = dereth_client_runtime::present::Scene::census(&own);
    drop(own);
    let (later, later_px) = load_and_draw(&store, &mut gpu, cfg(Some(RegionStyle::Late)));
    assert!(later.draw.ground_from_other_files());
    assert!(
        !later.draw.ground_palette_shifts(),
        "the later land surface texture-merges"
    );
    let land = later
        .detail_texturing()
        .surface(DetailClass::Landscape)
        .expect("the later region's landscape detail texture");
    assert_eq!(land.texture, DataId(0x0500_1786));
    assert!(
        (land.tiling - 4.0).abs() < f32::EPSILON,
        "the later region's tiling, not the world's eight: {}",
        land.tiling
    );
    let building = later
        .detail_texturing()
        .surface(DetailClass::Building)
        .expect("the later region's building detail texture");
    assert!(
        (building.tiling - 4.0).abs() < f32::EPSILON,
        "the building detail texture follows the drawn style: {}",
        building.tiling
    );
    assert_eq!(
        later.draw.detail_source_used(),
        dereth_scene::world_scene::DetailSource::Ground
    );
    let later_census = dereth_client_runtime::present::Scene::census(&later);
    assert_eq!(
        (
            later_census.scenery_objects,
            later_census.buildings,
            later_census.static_objects
        ),
        (
            own_census.scenery_objects,
            own_census.buildings,
            own_census.static_objects
        ),
        "the world's own scenery, buildings and objects"
    );
    let moved = own_px.iter().zip(&later_px).filter(|(a, b)| a != b).count();
    assert!(moved > 10_000, "the ground did not change: {moved} bytes");
    eprintln!(
        "later ground: {moved} of {} bytes moved; {} scenery, {} buildings, {} statics",
        own_px.len(),
        later_census.scenery_objects,
        later_census.buildings,
        later_census.static_objects
    );
    drop(later);

    // The end-of-retail world alone has no older files beside it: an older ground is refused at
    // load and the world's own is drawn.
    let eor = crate::common::dats();
    let (scene, _) = load_and_draw(&eor, &mut gpu, cfg(Some(RegionStyle::LegacySoftware)));
    assert!(scene.draw.ground_is_worlds_own());
    assert!(!scene.draw.ground_palette_shifts());
}
