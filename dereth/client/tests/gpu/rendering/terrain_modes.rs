//! The ground and sky styles: Palette Shift, Legacy Blend and Modern Blend, and the three skies,
//! on any world and switched while it is drawn.
//!
//! The region alone decides how the ground and the sky look; the cells give heights, terrain
//! types and road bits numbered the same in every era. So the end-of-retail world is drawn here
//! with the February 2005 regions (from that set's `portal.dat`, beside it for presentation only)
//! and with its own, and a change of style rebuilds the ground or the sky in place. A style whose
//! files are not present is refused and the world keeps what it had.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`), Holtburg with no body at a pinned time of day, and a graphics
//! device; a missing input fails.

#![cfg(gpu)]

use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;
use dereth_render::device::Gpu;
use std::sync::Arc;
use {
    dereth_client_runtime::render_prefs::RegionStyle,
    dereth_client_runtime::render_prefs::RequiredFiles,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

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

/// Holtburg with no body, no particles and a pinned clock, drawn with `ground` and `sky`.
fn cfg(ground: Option<RegionStyle>, sky: Option<RegionStyle>) -> SceneConfig {
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
            ground,
            sky,
            ..SceneConfig::default().render
        },
        ..SceneConfig::default()
    }
}

/// Load the scene and point the camera over the block, looking across it toward the horizon.
fn load(store: &Arc<RetailDatStore>, gpu: &mut Gpu, cfg: SceneConfig) -> WorldScene {
    look(
        WorldScene::load(store, gpu, cfg).expect("the scene loads"),
        -0.25,
    )
}

/// Load the scene with the camera looking steeply down at the block, so the frame is ground and
/// objects alone, and the weather off: a still view, whose frame is the same however many times
/// it is drawn. The sky's clouds scroll with every frame drawn, so a frame with sky in it is
/// compared only with another scene's first frames.
fn load_ground(store: &Arc<RetailDatStore>, gpu: &mut Gpu, cfg: SceneConfig) -> WorldScene {
    let mut scene = look(
        WorldScene::load(store, gpu, cfg).expect("the scene loads"),
        -0.9,
    );
    scene.set_weather_enabled(false);
    scene
}

fn look(mut scene: WorldScene, pitch: f32) -> WorldScene {
    let p = scene.camera.position;
    scene.camera.position = dereth_primitives::Vec3::new(p.x, p.y + 0.85 * 192.0, p.z + 40.0);
    scene.camera.yaw = 0.0;
    scene.camera.pitch = pitch;
    scene
}

/// Draw a few frames at the pinned clock; the last frame's RGBA.
fn draw(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> Vec<u8> {
    let mut rgba = Vec::new();
    for i in 0..3 {
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
    rgba
}

fn moved(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// Behaviour: rendering.terrain.a-ground-style-switch-rebuilds-the-ground-while-the-world-is-drawn
/// The end-of-retail world, drawn with its own ground, is switched to Palette Shift, then Legacy
/// Blend, then back to its own, through the same per-frame preference poll the options page
/// reaches. Each switch rebuilds every resident block with the new land surface (palette shift
/// composes on the CPU, the other two texture-merge) and the ground's pixels change; switching
/// back draws exactly the frame the world was first drawn with, so nothing of an earlier ground
/// survives a switch. The scenery and buildings never change.
#[test]
fn a_ground_switch_rebuilds_every_resident_block_and_switching_back_restores_the_frame() {
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = load_ground(&store, &mut gpu, cfg(None, None));
    assert!(scene.draw.ground_is_worlds_own());
    assert!(!scene.draw.ground_palette_shifts());
    let own_px = draw(&mut scene, &store, &mut gpu);
    // The calibration: the still view draws the same frame again, or "restored" below measures
    // nothing.
    assert_eq!(
        moved(&own_px, &draw(&mut scene, &store, &mut gpu)),
        0,
        "the view is not still, so this test cannot see"
    );
    let blocks = scene.draw.stats.blocks_meshed;
    let census = dereth_client_runtime::present::Scene::census(&scene);

    let switch = |scene: &mut WorldScene, gpu: &mut Gpu, style: Option<RegionStyle>| {
        scene.draw.cfg.render.ground = style;
        let work = scene
            .update_from_preferences(&store, gpu)
            .expect("the poll applies the ground");
        assert!(
            work.ground_changed,
            "{style:?}: the poll did not change the ground"
        );
        assert_eq!(work.ground_refused, None);
        assert!(work.blocks_rebuilt > 0, "{style:?}: nothing was rebuilt");
        assert_eq!(scene.draw.ground_style(), style);
        draw(scene, &store, gpu)
    };

    let palette_px = switch(&mut scene, &mut gpu, Some(RegionStyle::LegacySoftware));
    assert!(scene.draw.ground_palette_shifts());
    assert!(scene.draw.ground_from_other_files());
    assert!(
        !scene.terrain_composites().is_empty(),
        "the palette-shift ground composes its cells"
    );
    let palette_moved = moved(&own_px, &palette_px);
    assert!(
        palette_moved > 10_000,
        "Palette Shift looks like the world's own: {palette_moved}"
    );

    let legacy_px = switch(&mut scene, &mut gpu, Some(RegionStyle::LegacyHardware));
    assert!(!scene.draw.ground_palette_shifts());
    assert!(scene.draw.ground_from_other_files());
    let legacy_moved = moved(&own_px, &legacy_px);
    assert!(
        legacy_moved > 10_000,
        "Legacy Blend looks like the world's own: {legacy_moved}"
    );
    assert!(moved(&palette_px, &legacy_px) > 10_000);

    let back_px = switch(&mut scene, &mut gpu, None);
    assert!(scene.draw.ground_is_worlds_own());
    assert!(!scene.draw.ground_from_other_files());
    assert_eq!(
        moved(&own_px, &back_px),
        0,
        "switching back to the world's own ground did not restore its frame"
    );
    assert_eq!(scene.draw.stats.blocks_meshed, blocks);
    let after = dereth_client_runtime::present::Scene::census(&scene);
    assert_eq!(
        (after.scenery_objects, after.buildings, after.static_objects),
        (
            census.scenery_objects,
            census.buildings,
            census.static_objects
        ),
        "a ground switch changed the scenery"
    );
    eprintln!(
        "ground switch: Palette Shift moved {palette_moved}, Legacy Blend {legacy_moved} of {} \
         bytes; back to the world's own moved 0",
        own_px.len()
    );
}

/// Behaviour: rendering.terrain.every-ground-and-sky-style-draws-the-end-of-retail-world
/// Loaded with each of the three grounds and each of the three skies, the end-of-retail world is
/// drawn every time with its own scenery, buildings and objects. Modern Blend and the Modern sky
/// are the world's own and draw its own frame; Palette Shift palette-shifts and Legacy Blend
/// texture-merges from the February 2005 files. The February 2005 hardware sky carries the same
/// light as the end-of-retail sky, so the ground is lit alike under the two, and the software sky
/// carries different light.
#[test]
fn every_ground_and_sky_style_draws_the_end_of_retail_world() {
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut own = load(&store, &mut gpu, cfg(None, None));
    let own_px = draw(&mut own, &store, &mut gpu);
    let own_census = dereth_client_runtime::present::Scene::census(&own);
    let own_light = own.landscape_lighting();
    drop(own);

    let mut grounds = Vec::new();
    for style in RegionStyle::ALL {
        let mut scene = load(&store, &mut gpu, cfg(Some(style), None));
        let px = draw(&mut scene, &store, &mut gpu);
        assert_eq!(
            scene.draw.ground_palette_shifts(),
            style == RegionStyle::LegacySoftware,
            "{style:?}"
        );
        assert_eq!(
            scene.draw.ground_is_worlds_own(),
            style == RegionStyle::Modern,
            "{style:?}: only Modern Blend is the end-of-retail world's own ground"
        );
        let c = dereth_client_runtime::present::Scene::census(&scene);
        assert_eq!(
            (c.scenery_objects, c.buildings, c.static_objects),
            (
                own_census.scenery_objects,
                own_census.buildings,
                own_census.static_objects
            ),
            "{style:?} changed the scenery"
        );
        let m = moved(&own_px, &px);
        if style == RegionStyle::Modern {
            assert_eq!(m, 0, "Modern Blend is the world's own frame");
        } else {
            assert!(m > 10_000, "{style:?} looks like the world's own: {m}");
        }
        grounds.push((style, m));
    }

    let mut skies = Vec::new();
    for style in RegionStyle::ALL {
        let mut scene = load(&store, &mut gpu, cfg(None, Some(style)));
        let px = draw(&mut scene, &store, &mut gpu);
        assert_eq!(
            scene.draw.sky_is_worlds_own(),
            style == RegionStyle::Modern,
            "{style:?}"
        );
        assert_eq!(
            scene.draw.sky_from_other_files(),
            style != RegionStyle::Modern
        );
        assert_eq!(
            scene
                .draw
                .sky_region()
                .sky_info
                .as_ref()
                .map(|s| s.day_groups.len()),
            Some(20)
        );
        let light = scene.landscape_lighting();
        match style {
            RegionStyle::LegacySoftware => assert_ne!(light, own_light, "the software sky's light"),
            _ => assert_eq!(
                light, own_light,
                "{style:?}: the same light as the world's own"
            ),
        }
        let m = moved(&own_px, &px);
        if style == RegionStyle::Modern {
            assert_eq!(m, 0, "the Modern sky is the world's own frame");
        } else {
            assert!(m > 1_000, "{style:?}'s sky looks like the world's own: {m}");
        }
        skies.push((style, m));
    }
    eprintln!(
        "grounds moved {grounds:?}; skies moved {skies:?} of {} bytes",
        own_px.len()
    );
}

/// Behaviour: rendering.terrain.a-style-without-its-files-is-refused-and-the-world-keeps-its-own
/// With no older files beside the end-of-retail world, choosing Legacy Blend, Palette Shift or an
/// older sky is refused: the poll reports which files were wanted, the option goes back to the
/// world's own, and the frame is the frame it was. A later world's own Modern Blend is always
/// there.
#[test]
fn a_style_without_its_files_is_refused_and_the_world_keeps_its_own() {
    let store = crate::common::dats();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = load_ground(&store, &mut gpu, cfg(None, None));
    let before = draw(&mut scene, &store, &mut gpu);
    assert_eq!(
        moved(&before, &draw(&mut scene, &store, &mut gpu)),
        0,
        "the view is not still, so this test cannot see"
    );
    for style in [RegionStyle::LegacyHardware, RegionStyle::LegacySoftware] {
        scene.draw.cfg.render.ground = Some(style);
        scene.draw.cfg.render.sky = Some(style);
        let work = scene
            .update_from_preferences(&store, &mut gpu)
            .expect("a refusal is not an error");
        assert_eq!(
            work.ground_refused,
            Some((RequiredFiles::Legacy, None)),
            "{style:?}"
        );
        assert_eq!(
            work.sky_refused,
            Some((RequiredFiles::Legacy, None)),
            "{style:?}"
        );
        assert!(!work.ground_changed && !work.sky_changed);
        assert_eq!(scene.draw.cfg.render.ground, None, "the option went back");
        assert_eq!(scene.draw.cfg.render.sky, None);
        assert!(scene.draw.ground_is_worlds_own() && scene.draw.sky_is_worlds_own());
        let after = draw(&mut scene, &store, &mut gpu);
        assert_eq!(
            moved(&before, &after),
            0,
            "a refused style changed the frame"
        );
    }
    // The world's own style by name is no change at all.
    scene.draw.cfg.render.ground = Some(RegionStyle::Modern);
    let work = scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll");
    assert_eq!(work.ground_refused, None);
    assert!(scene.draw.ground_is_worlds_own());
    assert_eq!(moved(&before, &draw(&mut scene, &store, &mut gpu)), 0);
}

/// Behaviour: rendering.terrain.a-terrain-type-the-region-does-not-name-takes-its-neighbours-ground
/// The end-of-retail world uses Desolate Lands (type 31) at three vertices, two of them in block
/// `0xF930`. The end-of-retail region names it; the February 2005 regions name 31 types and draw
/// any other with a filler. With Modern Blend the block's ground is the region's own for every
/// type; with Legacy Blend type 31 is not among the drawn types and those vertices are drawn as
/// their neighbours (Forest Floor and Barren Rock) are.
#[test]
fn a_terrain_type_the_ground_region_does_not_name_takes_its_neighbours_ground() {
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::test_gpu(640, 480);
    let at = |ground| SceneConfig {
        landblock: 0xF930,
        ..cfg(ground, None)
    };
    let mut modern = load(&store, &mut gpu, at(None));
    assert_ne!(
        modern.draw.ground_drawn_types() & (1 << 31),
        0,
        "the world names type 31"
    );
    let modern_px = draw(&mut modern, &store, &mut gpu);
    drop(modern);
    let mut legacy = load(&store, &mut gpu, at(Some(RegionStyle::LegacyHardware)));
    let drawn = legacy.draw.ground_drawn_types();
    assert_eq!(
        drawn & (1 << 31),
        0,
        "the February 2005 region does not name type 31"
    );
    assert_eq!(drawn, (1 << 31) - 1, "and names every type below it");
    let legacy_px = draw(&mut legacy, &store, &mut gpu);
    assert!(moved(&modern_px, &legacy_px) > 10_000);
    drop(legacy);
    // The fill itself, over the block's own cells.
    let lb_bytes = store
        .read_cell(dereth_primitives::DataId(0xF930_FFFF))
        .expect("block 0xF930");
    let lb = <dereth_assets::world::CellLandblock as dereth_assets::Decode>::decode_payload_in(
        store.era(),
        dereth_primitives::DataId(0xF930_FFFF),
        &lb_bytes,
    )
    .expect("decodes");
    let filled = dereth_terrain::land::fill::fill_undrawn_terrain(&lb, drawn)
        .expect("block 0xF930 has type 31 to fill");
    let mut seen = Vec::new();
    for x in 0..9 {
        for y in 0..9 {
            if lb.terrain_type(x, y) == 31 {
                let t = filled.terrain_type(x, y);
                assert!(
                    t == 0 || t == 21,
                    "({x}, {y}) took {t}, not Barren Rock or Forest Floor"
                );
                seen.push((x, y, t));
            } else {
                assert_eq!(filled.terrain[x * 9 + y], lb.terrain[x * 9 + y]);
            }
        }
    }
    assert_eq!(seen.len(), 2, "{seen:?}");
    eprintln!("0xF930 type 31 vertices filled: {seen:?}");
}

/// A headless client over the end-of-retail world at Holtburg (no body, no UI, a pinned clock),
/// its data files from the one folder `dat_dir` (the older set beside the later one when it holds
/// both) and `set_at` settings made part way through, as `--dat-dir` and `--set-at` make them.
fn client(dat_dir: std::path::PathBuf, set_at: Vec<(u64, String)>) -> dereth_client::app::App {
    let config = dereth_client_runtime::config::Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dereth-terrain-modes-not-created/prefs.ini"),
        dat_dir,
        set_at,
        ..dereth_client_runtime::config::Config::default()
    };
    let mut app = dereth_client::app::App::new(config)
        .unwrap_or_else(|e| panic!("the headless client did not start: {e}"));
    let scene = SceneConfig {
        character: false,
        particles: false,
        land_radius: 2,
        scenery_radius: 1,
        time_of_day: Some(0.5),
        game_time: Some(0.0),
        ..cfg(None, None)
    };
    app.load_static_scene(scene)
        .unwrap_or_else(|e| panic!("the static scene did not load: {e}"));
    app
}

/// Behaviour: rendering.terrain.a-style-without-its-files-is-refused-and-the-world-keeps-its-own
/// In the running client, choosing Legacy Blend and the Legacy Software sky on the end-of-retail
/// world with no older files beside it leaves the world's own ground and sky, puts the two options
/// back to the world's own, and tells the player in the chat window: "This terrain mode requires
/// legacy DATs" and "This sky requires legacy DATs".
#[test]
fn the_client_says_which_files_a_refused_style_needs_and_puts_the_option_back() {
    use dereth_client_contract::options::{landscape, store};
    store::init();
    let mut app = client(
        dereth_dat::testing::dat_dir(),
        vec![
            (3, "Render.Ground=Legacy Blend".to_string()),
            (3, "Render.Sky=LegacySoftware".to_string()),
        ],
    );
    let before = app.objects().world.scroll.added;
    // Set as frame 3 begins, handed to the scene in that frame's request drain and applied by the
    // next frame's preference poll.
    for _ in 0..4 {
        assert!(app.frame());
    }
    let work = app.probe().last_render_pref_work();
    assert_eq!(work.ground_refused, Some((RequiredFiles::Legacy, None)));
    assert_eq!(work.sky_refused, Some((RequiredFiles::Legacy, None)));
    let s = app.world_scene().expect("a world");
    assert!(s.draw.ground_is_worlds_own() && s.draw.sky_is_worlds_own());
    assert_eq!(
        store::inq_value(landscape::GROUND),
        Some(dereth_ui_screens::PrefValue::Int(landscape::WORLD_DEFAULT)),
        "the ground option went back to the world's own"
    );
    assert_eq!(
        store::inq_value(landscape::SKY),
        Some(dereth_ui_screens::PrefValue::Int(landscape::WORLD_DEFAULT))
    );
    assert_eq!(app.objects().world.scroll.added, before + 2);
    let lines: Vec<(String, u32)> = app
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .map(|l| (l.body.clone(), l.chat_type))
        .collect();
    for want in [
        "This terrain mode requires legacy DATs",
        "This sky requires legacy DATs",
    ] {
        assert!(
            lines
                .iter()
                .any(|(b, t)| b == want && *t == dereth_client_model::scroll::LOCAL_ERROR_TYPE),
            "{want:?} not in {lines:?}"
        );
    }
}

/// Behaviour: rendering.terrain.a-ground-style-switch-rebuilds-the-ground-while-the-world-is-drawn
/// In the running client, with the February 2005 files beside the end-of-retail world, an option
/// set to Palette Shift part way through the run reaches the scene through the options store and
/// the preference request, and the next frame's preference poll palette-shifts the ground.
#[test]
fn the_client_switches_the_ground_when_the_option_changes() {
    use dereth_client_contract::options::{landscape, store};
    store::init();
    let mut app = client(
        dereth_dat::testing::both_sets_dir(),
        vec![(3, "Render.Ground=PaletteShift".to_string())],
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(!app
        .world_scene()
        .expect("a world")
        .draw
        .ground_palette_shifts());
    assert!(app.frame());
    let work = app.probe().last_render_pref_work();
    assert!(work.ground_changed, "{work:?}");
    assert!(work.blocks_rebuilt > 0);
    let s = app.world_scene().expect("a world");
    assert!(s.draw.ground_palette_shifts());
    assert_eq!(s.draw.ground_style(), Some(RegionStyle::LegacySoftware));
    assert_eq!(
        store::inq_value(landscape::GROUND),
        Some(dereth_ui_screens::PrefValue::Int(
            RegionStyle::LegacySoftware.value()
        ))
    );
}

/// Behaviour: rendering.terrain.the-detail-textures-follow-the-drawn-ground-style
/// With the landscape and environment detail textures on, every ground style draws them. They are
/// taken from the drawn style's region when it names them (Legacy Blend: the February 2005
/// hardware region's, eight times a cell for the ground and three for buildings; Modern Blend: the
/// end-of-retail region's, four and four), and from the world's own region when it does not
/// (Palette Shift on the end-of-retail world: four and four). Palette Shift's ground moves when
/// the landscape detail texture is turned on.
#[test]
fn the_detail_textures_follow_the_drawn_ground_style_and_palette_shift_draws_them_too() {
    use dereth_scene::world_scene::DetailSource;
    use dereth_world_render::detail::DetailClass;
    let store = end_of_retail_with_legacy_files();
    let mut gpu = crate::common::test_gpu(640, 480);
    let with_detail = |ground: Option<RegionStyle>, on: bool| {
        let mut c = cfg(ground, None);
        c.render.landscape_detail_textures = on;
        c.render.environment_detail_textures = true;
        c
    };
    let tilings = |scene: &WorldScene| {
        let d = scene.detail_texturing();
        let land = d
            .surface(DetailClass::Landscape)
            .expect("a landscape detail texture");
        let building = d
            .surface(DetailClass::Building)
            .expect("a building detail texture");
        (
            land.texture.0,
            land.tiling,
            building.tiling,
            scene.draw.detail_source_used(),
        )
    };
    let mut seen = Vec::new();
    for (style, want) in [
        (None, (0x0500_1786, 4.0, 4.0, DetailSource::World)),
        (
            Some(RegionStyle::LegacySoftware),
            (0x0500_1786, 4.0, 4.0, DetailSource::World),
        ),
        (
            Some(RegionStyle::LegacyHardware),
            (0x0500_1786, 8.0, 3.0, DetailSource::Ground),
        ),
        (
            Some(RegionStyle::Modern),
            (0x0500_1786, 4.0, 4.0, DetailSource::World),
        ),
    ] {
        let scene = load_ground(&store, &mut gpu, with_detail(style, true));
        let got = tilings(&scene);
        assert_eq!(got, want, "{style:?}");
        seen.push((style, got));
    }
    // Switched live, the detail textures follow the new style.
    let mut scene = load_ground(&store, &mut gpu, with_detail(None, true));
    scene.draw.cfg.render.ground = Some(RegionStyle::LegacyHardware);
    scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the poll");
    assert_eq!(
        tilings(&scene),
        (0x0500_1786, 8.0, 3.0, DetailSource::Ground)
    );
    drop(scene);
    // Palette Shift's ground with the landscape detail texture on and off.
    let mut off = load_ground(
        &store,
        &mut gpu,
        with_detail(Some(RegionStyle::LegacySoftware), false),
    );
    let off_px = draw(&mut off, &store, &mut gpu);
    drop(off);
    let mut on = load_ground(
        &store,
        &mut gpu,
        with_detail(Some(RegionStyle::LegacySoftware), true),
    );
    let on_px = draw(&mut on, &store, &mut gpu);
    let m = moved(&off_px, &on_px);
    assert!(
        m > 1_000,
        "the detail texture did not reach the palette-shift ground: {m}"
    );
    eprintln!("detail sources {seen:?}; Palette Shift detail on vs off moved {m} bytes");
}
