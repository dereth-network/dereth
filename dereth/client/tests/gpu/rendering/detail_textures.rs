//! The detail-texture pass and its two preferences. Environment Detail Textures
//! (`Render.BuildingDetailTextures`) generates the region's building and environment detail
//! surfaces (never an object one), the building and environment-cell draws bind the detail
//! surface in texture stage 1, and with the preference off the frame is byte-identical between
//! independent clients while an on client moves pixels. Landscape Detail Textures
//! (`Render.LandscapeDetailTextures`, off by default) generates the landscape one, drawn over the
//! near ground in a second pass and faded out by 50 m. The preference is still polled and counted, so "the poll noticed" and "the
//! subsystem acted" stay two separate measurements. Fixture: the Holtburg yard from the retail
//! dats in a headless `App` (live, and a still scene with no body, particles or UI). No datagram
//! leaves the process; a station returns with a printed reason when the device or the scene
//! cannot be created.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::SceneConfig;
use dereth_client::{app::App, config::Config};
use dereth_ui_screens::{PrefValue, UiRequest};
use dereth_world_render::detail::DetailClass;

/// The Holtburg yard used by the other camera and landscape stations. Its landblock contains the
/// building content that this preference actually reaches.
const HOLTBURG: u16 = 0xA9B4;

fn scene() -> SceneConfig {
    SceneConfig {
        landblock: HOLTBURG,
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 2,
        scenery_radius: 1,
        particles: false,
        ..SceneConfig::default()
    }
}

/// A running client after the alpha-list counters reach one adjacent stable pair with at least one
/// submitted part. The first frames stream the window and assemble body batches, so a comparison
/// taken across that ramp would use different workloads. This condition does not assert stable
/// pixels; the live body remains animated.
fn app() -> Option<App> {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("dereth-detail-textures-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    };
    let mut app = match App::new(cfg) {
        Ok(a) => a,
        Err(e) => panic!("the headless client did not start: {e}"),
    };
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    if let Err(e) = app.load_static_scene(scene()) {
        panic!("the static scene did not load: {e}");
    }
    for _ in 0..40 {
        assert!(app.frame());
        let a = app.world_scene().expect("a world").drawn_alpha_lists();
        assert!(app.frame());
        let b = app.world_scene().expect("a world").drawn_alpha_lists();
        if (a.parts, a.clip, a.blend, a.immediate) == (b.parts, b.clip, b.blend, b.immediate)
            && a.parts > 0
        {
            return Some(app);
        }
    }
    panic!("the part pass never settled, so no before/after here would be comparable");
}

/// The options page's `UiRequest` for the Environment Detail Textures check box; the options-page
/// station calibrates this exact key against the built page.
fn tick(on: bool) -> UiRequest {
    UiRequest::SetPreference("Render.BuildingDetailTextures", PrefValue::Bool(on))
}

/// Emit the request directly to the running client and let two frames carry and apply it.
///
/// World streaming polls the current preferences before the interaction phase applies that frame's
/// `UiRequest`s. The first frame writes the option and the second frame's poll acts on it. This is
/// direct request emission, not a simulated pointer click.
fn change(app: &mut App, on: bool) -> u64 {
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(tick(on));
    assert!(app.frame(), "the frame that carries the option write");
    let before = app.render_pref_applies();
    assert!(app.frame(), "the frame whose poll sees it");
    app.render_pref_applies() - before
}

/// `(detail binds this frame, the frame's pixels)`, both taken from the same frame.
fn frame_census(app: &mut App) -> (u64, Vec<u8>) {
    app.renderer().clear_stage1_binds();
    assert!(app.frame());
    let binds = app.renderer().stage1_binds();
    let (_, _, bgra) = app
        .renderer_mut()
        .capture_bgra()
        .expect("a headless frame reads back");
    (binds, bgra)
}

// ---------------------------------------------------------------------------------------------
// 1. The generation half
// ---------------------------------------------------------------------------------------------

/// Region detail selection generates a surface from each enabled class's detail-texture ID in the
/// terrain descriptors that the client already decoded.
#[test]
fn the_preference_generates_the_region_s_two_live_detail_surfaces() {
    let Some(mut app) = app() else { return };

    // `Render.BuildingDetailTextures` ships **on** (`RenderPreferences::default`), so the region
    // install has already generated the two. Turning it off is what must release them.
    let d = app.world_scene().expect("a world").detail_texturing();
    assert_eq!(
        d.applies(),
        1,
        "the landscape region install did not reach it"
    );
    assert_eq!(
        d.generated(),
        2,
        "the shipped default is on and generated nothing"
    );

    assert_eq!(
        change(&mut app, false),
        1,
        "the poll did not act on the next frame"
    );
    let d = app.world_scene().expect("a world").detail_texturing();
    assert_eq!(
        d.applies(),
        2,
        "the poll did not reach landscape detail-texture selection"
    );
    assert_eq!(
        d.generated(),
        0,
        "turning the preference off did not release its detail surfaces"
    );
    assert_eq!(d.current(DetailClass::Building), None);

    assert_eq!(
        change(&mut app, true),
        1,
        "the poll did not act on the next frame"
    );
    let d = app.world_scene().expect("a world").detail_texturing();
    assert_eq!(
        d.applies(),
        3,
        "the poll did not reach landscape detail-texture selection"
    );
    assert_eq!(
        d.generated(),
        2,
        "the shipped call shape is (0, v, v, 0): exactly building and environment"
    );
    assert_eq!(
        d.surface(DetailClass::Landscape),
        None,
        "the landscape surface is absent because its selection flag is zero"
    );
    assert_eq!(
        d.surface(DetailClass::Object),
        None,
        "the object surface is absent because its selection flag is zero"
    );
    // In the shipped portal-data region, the building and environment terrain-descriptor rows name
    // `SurfaceTexture 0x05001787` at tiling 4.
    let b = d.surface(DetailClass::Building).expect("building");
    let e = d.surface(DetailClass::Environment).expect("environment");
    assert_eq!(
        b.texture,
        dereth_primitives::DataId(0x0500_1787),
        "the building terrain descriptor's detail texture"
    );
    assert_eq!(
        e.texture,
        dereth_primitives::DataId(0x0500_1787),
        "the environment terrain descriptor's detail texture"
    );
    assert!(
        b.detail_bit,
        "the building surface did not gain the detail-texture bit 0x20000"
    );
    assert!((d.tiling(DetailClass::Building) - 4.0).abs() < f32::EPSILON);
    assert!((d.tiling(DetailClass::Environment) - 4.0).abs() < f32::EPSILON);
    // Tiling assignment precedes the four surface gates. This station checks building,
    // environment, and the disabled landscape class; it does not assert object tiling.
    assert!((d.tiling(DetailClass::Landscape) - 4.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------------------------
// 2. The draw half — REJECTING
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.detail-textures.the-preference-binds-a-second-stage-and-moves-pixels
/// **Rejecting.** The building draw installs the building detail surface before drawing the shell.
/// Mesh submission enables detail use from that surface, binds it in stage 1 with a WRAP/LINEAR
/// sampler, and changes the stage-operation chain to `MODULATE` / `PREMODULATE` plus
/// `BLENDCURRENTALPHA`.
///
/// The observable here is **the bind count**, taken inside one frame of the live client, from a
/// measured zero. It is deliberately *not* a pixel comparison; see the note in the body.
#[test]
fn the_building_pass_binds_a_second_texture_stage_and_the_pixels_move() {
    let Some(mut app) = app() else { return };
    // The preference ships on, so the control arm is a frame after it is turned **off**.
    assert_eq!(
        change(&mut app, false),
        1,
        "the poll did not act on the next frame"
    );
    let (binds0, px0) = frame_census(&mut app);
    assert_eq!(
        binds0, 0,
        "the control arm is not a control: something already binds stage 1"
    );

    assert_eq!(
        change(&mut app, true),
        1,
        "the poll did not act on the next frame"
    );
    let (binds1, px1) = frame_census(&mut app);

    assert!(
        binds1 > 0,
        "the preference generated {} detail surface(s), but no building or environment draw bound one in stage 1",
        app.world_scene().expect("a world").detail_texturing().generated()
    );
    assert_eq!(px0.len(), px1.len(), "the two frames are different sizes");
    // Recorded, not asserted: over an animated scene this number is the BODY. With the bind left
    // in place and `key_detail` never selected -- the detail texture bound to a stage no shader
    // samples -- a pixel diff here still read 1,181,426 of 1,920,000 bytes changed, because the
    // character idles and a different set of its parts is culled every frame. The pixel claim
    // belongs to the still-scene test below, which *does* fail under that mutation. Printed so
    // both figures appear in one run.
    let moved = px0.iter().zip(&px1).filter(|(a, b)| a != b).count();
    eprintln!(
        "detail textures, live scene: {binds1} stage-1 binds, {moved} of {} bytes differ",
        px0.len()
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The off state draws no detail
// ---------------------------------------------------------------------------------------------

/// The off state: two independent clients loaded with the preference off must render
/// the same still frame byte for byte, while a third independently loaded client with it on must
/// bind the detail stage and move at least one compared byte.
///
/// Off leaves every current detail surface absent, so no draw selects the detail key or binds stage
/// 1, and `PerDrawConstants::detail_params` remains zero. The vertex shader therefore reduces
/// `uv1 = uv0 * lerp(1, tiling, 0)` to `uv1 = uv0`.
#[test]
fn with_the_preference_off_the_frame_is_byte_identical() {
    // **A body-less, UI-less scene with a pinned clock, and that is not tidying.** `app()`'s
    // Holtburg carries the character and the shipped HUD; the body idles and the HUD has a clock
    // and a radar, so two consecutive frames of it are not the same pixels and a byte-for-byte
    // claim taken across them would fail whatever the pass did. Each frame here comes from its
    // own freshly loaded client with the preference set at load. Nothing is toggled within a
    // client, and the two off clients independently calibrate reproducibility.
    let Some((binds_a, px_a)) = still_frame(false) else {
        return;
    };
    let Some((binds_b, px_b)) = still_frame(false) else {
        return;
    };
    let Some((binds_c, px_c)) = still_frame(true) else {
        return;
    };

    assert_eq!(
        binds_a, 0,
        "a draw bound a detail texture with the preference off"
    );
    assert_eq!(binds_b, 0);
    // The calibration: two independent runs of the *same* configuration must agree, or "no bytes
    // moved" below measures nothing. An instrument that cannot see reports absence.
    assert_eq!(
        px_a.iter().zip(&px_b).filter(|(x, y)| x != y).count(),
        0,
        "two runs of the same off configuration already differ, so this test cannot see"
    );
    assert!(
        binds_c > 0,
        "the on run bound nothing, so the off run proves nothing"
    );
    let moved = px_a.iter().zip(&px_c).filter(|(x, y)| x != y).count();
    assert!(moved > 0, "the on run moved no pixel in this scene");
    eprintln!(
        "detail textures, still frame: {binds_c} stage-1 binds, {moved} of {} bytes moved",
        px_a.len()
    );
}

/// One frame of a **still** Holtburg -- no character, no particles, no UI overlay, a pinned day
/// fraction and game time -- with `Render.BuildingDetailTextures` set at load. After eight warm-up
/// frames, it clears the bind counter and captures the ninth. Returns `(stage-1 binds, the frame's
/// BGRA)`; `None` when application/device initialization or static-scene loading fails.
fn still_frame(on: bool) -> Option<(u64, Vec<u8>)> {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dereth-detail-textures-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    };
    let mut app = match App::new(cfg) {
        Ok(a) => a,
        Err(e) => panic!("the headless client did not start: {e}"),
    };
    let base = scene();
    let s = SceneConfig {
        character: false,
        time_of_day: Some(0.5),
        game_time: Some(0.0),
        render: dereth_client::render_prefs::RenderPreferences {
            environment_detail_textures: on,
            ..base.render
        },
        ..base
    };
    if let Err(e) = app.load_static_scene(s) {
        panic!("the static scene did not load: {e}");
    }
    for _ in 0..8 {
        assert!(app.frame());
    }
    app.renderer().clear_stage1_binds();
    assert!(app.frame());
    let binds = app.renderer().stage1_binds();
    let (_, _, bgra) = app
        .renderer_mut()
        .capture_bgra()
        .expect("a headless frame reads back");
    Some((binds, bgra))
}

// ---------------------------------------------------------------------------------------------
// 4. The preference is polled and counted
// ---------------------------------------------------------------------------------------------

/// "The poll noticed" and "the subsystem did something" are two separate measurements, and both
/// move when the preference changes.
#[test]
fn the_preference_is_still_polled_and_counted() {
    assert!(
        dereth_client::world::DETAIL_TEXTURE_PASS,
        "the detail-texture pass exists; if this is false the pass has been removed"
    );
    let Some(mut app) = app() else { return };
    // Off first: the preference ships on, so `on` is not a change from the settled state.
    assert_eq!(change(&mut app, false), 1);
    assert_eq!(change(&mut app, true), 1);
    let work = app.last_render_pref_work();
    assert!(
        work.detail_texturing_changed,
        "the poll did not notice the change at all"
    );
    assert!(
        !work.flushed && !work.mid_radius_changed,
        "it took an arm that is not its own"
    );
    assert_eq!(
        work.blocks_queued, 0,
        "it rebuilt the ring, which is another arm's work"
    );
    assert_eq!(
        work.detail_surfaces, 2,
        "the poll noticed and the subsystem did nothing"
    );
    let s = app.world_scene().expect("a world");
    assert!(
        s.draw.cfg.render.environment_detail_textures,
        "the option did not reach the current render preferences"
    );
    assert!(
        s.render_shadow().environment_detail_textures,
        "the shadow did not move"
    );

    // And the negative half: off is polled, counted, and generates nothing.
    assert_eq!(change(&mut app, false), 1);
    let work = app.last_render_pref_work();
    assert!(work.detail_texturing_changed);
    assert_eq!(work.detail_surfaces, 0);
}

// ---------------------------------------------------------------------------------------------
// 5. The landscape detail texture
// ---------------------------------------------------------------------------------------------

/// One frame of a still Holtburg (no body, no particles, a pinned clock) with only
/// `Render.LandscapeDetailTextures` set as asked and the building detail texture off, from a
/// camera over the middle of the block, `height` metres above its highest ground and looking
/// steeply down at it. Returns `(the frame's draws, its RGBA, the landscape detail surface)`.
fn landscape_frame(
    on: bool,
    height: f32,
) -> (
    u64,
    Vec<u8>,
    Option<dereth_world_render::detail::DetailSurface>,
) {
    use dereth_client::objects::ObjectStream;
    use dereth_client::world::WorldScene;
    use dereth_primitives::LocalTime;
    let store = crate::common::dats();
    let mut gpu = crate::common::test_gpu(640, 480);
    let base = scene();
    let cfg = SceneConfig {
        character: false,
        particles: false,
        time_of_day: Some(0.5),
        game_time: Some(0.0),
        camera_height: 0.0,
        render: dereth_client::render_prefs::RenderPreferences {
            landscape_detail_textures: on,
            environment_detail_textures: false,
            ..base.render
        },
        ..base
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    let surface = scene.detail_texturing().surface(DetailClass::Landscape);
    // The load camera stands at the block's highest ground, a third of a block south of it; this
    // one stands over the block's middle and looks down at it.
    let p = scene.camera.position;
    scene.camera.position = dereth_primitives::Vec3::new(p.x, p.y + 0.85 * 192.0, p.z + height);
    scene.camera.yaw = 0.0;
    scene.camera.pitch = -1.2;
    let mut out = (0, Vec::new());
    for i in 0..4 {
        let mut stream = ObjectStream::new();
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(f64::from(i) / 30.0),
            1.0 / 30.0,
        );
        scene.stream(&store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        let before = gpu.draw_calls();
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        out = (
            gpu.draw_calls() - before,
            gpu.capture().expect("capture").to_rgba(),
        );
    }
    (out.0, out.1, surface)
}

/// Behaviour: rendering.landscape.the-landscape-detail-preference-draws-a-fading-detail-texture
/// With `Render.LandscapeDetailTextures` on, the region's landscape detail texture
/// (`SurfaceTexture 0x05001786`, tiling 4) is drawn over the ground in a second pass of every
/// full-detail cell: near the eye it moves the ground's pixels, and from 50 m on it fades to
/// nothing, so a camera high over the land draws the extra pass and changes not one pixel. Off,
/// two independent loads draw the same frame byte for byte.
#[test]
fn the_landscape_detail_texture_covers_the_near_ground_and_fades_out_by_fifty_metres() {
    // Near: the camera fifteen metres over the block's highest ground.
    let (draws_off, px_off, none) = landscape_frame(false, 15.0);
    let (_, px_off2, _) = landscape_frame(false, 15.0);
    let (draws_on, px_on, surface) = landscape_frame(true, 15.0);
    assert_eq!(none, None, "off generates no landscape detail surface");
    let surface = surface.expect("on generates the landscape detail surface");
    assert_eq!(surface.texture, dereth_primitives::DataId(0x0500_1786));
    assert!((surface.tiling - 4.0).abs() < f32::EPSILON);
    assert_eq!(
        px_off.iter().zip(&px_off2).filter(|(a, b)| a != b).count(),
        0,
        "two loads of the same off configuration already differ, so this test cannot see"
    );
    assert!(
        draws_on > draws_off,
        "the detail pass drew nothing: {draws_on} draws on, {draws_off} off"
    );
    let near_moved = px_off.iter().zip(&px_on).filter(|(a, b)| a != b).count();
    assert!(
        near_moved > 1000,
        "only {near_moved} bytes moved under a camera fifteen metres up"
    );

    // Far: three hundred metres up, every piece of ground is beyond the fade.
    let (far_draws_off, far_off, _) = landscape_frame(false, 300.0);
    let (far_draws_on, far_on, _) = landscape_frame(true, 300.0);
    assert!(
        far_draws_on > far_draws_off,
        "the pass must still be drawn for the identical frame to mean anything"
    );
    let far_moved = far_off.iter().zip(&far_on).filter(|(a, b)| a != b).count();
    assert_eq!(far_moved, 0, "the detail texture shows beyond 50 m");
    eprintln!(
        "landscape detail: near {near_moved} bytes moved ({draws_off} -> {draws_on} draws), \
         far {far_moved} ({far_draws_off} -> {far_draws_on} draws)"
    );
}
