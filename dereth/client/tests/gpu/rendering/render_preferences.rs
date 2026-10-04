//! The profile's `Render.*` preferences reach what they drive (the projection, device gamma,
//! texture scales, alpha lists, degrade globals), and a change on the shipped options page applies
//! on the next frame without a restart. Field of view is stored in radians; aspect choice 1 is 4:3
//! and 2 is 16:9; gamma clamps to [-0.2, 1.0]. Texture detail and draw distance are polled once a
//! frame against shadow copies; *Multiple Pass Alpha* is read per mesh, and its enabled arm sends a
//! clip-mapped subset to the device twice, which is how the preference is seen (the list choice is
//! the same on both arms).
//! Fixture: synthetic profiles, the shipped options pages, and a headless `App` or scene at
//! Holtburg over the retail dats on a software device. No socket is opened; missing dats fail.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::app::App;
use dereth_client::config::{Config, Preferences};
use dereth_client::render_prefs::RenderPreferences;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::{AssetSource, DataId};
use dereth_render::device::Gpu;
use dereth_ui::framework::{DidMapperResolver, Screen};
use dereth_ui::msg::Delivery;
use dereth_ui::UiSystem;
use dereth_ui_screens::options::page::PlayerOptionPage;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::{PrefValue, UiRequest};
use std::rc::Rc;
use std::sync::Arc;

const W: u32 = 800;
const H: u32 = 600;

/// A synthetic `UserPreferences.ini` in the original save layout — a section
/// per category and the bare key after the registered name's last dot — with **every** value
/// different from that preference's registered default, so no assertion below can pass by
/// agreement. It uses the original formatting: `%.2f`
/// for a float, `True`/`False` for a bool, and the choice **label** for an enumeration.
const FIXTURE: &str = "[Render]\r\n\
     TextureFiltering=Anisotropic\r\n\
     LandscapeDetailTextures=True\r\n\
     BuildingDetailTextures=False\r\n\
     MultiPassAlpha=True\r\n\
     LandscapeTextureDetail=VeryHigh\r\n\
     EnvironmentTextureDetail=VeryLow\r\n\
     SceneryDrawDistance=High\r\n\
     LandscapeDrawDistance=Extreme\r\n\
     ScreenBrightness=0.50\r\n\
     AspectRatio=Wide\r\n\
     FieldOfView=120.00\r\n\
     AutomaticDegrades=False\r\n\
     GraphicsPerformance=0.30\r\n\
     DegradeDistance=75.00\r\n";

/// The retail store, or **fail**.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn config_from(text: &str) -> (Preferences, Config) {
    let prefs = Preferences::parse(text);
    let argv = ["--no-connect".to_string()];
    let cfg = Config::from_args_and_prefs_with(&argv, &prefs).expect("the profile parses");
    (prefs, cfg)
}

/// The Holtburg yard used for the scene measurements. This configuration does not attach a body.
fn scene_config(cfg: &Config) -> SceneConfig {
    SceneConfig {
        landblock: 0xA9B4,
        render: cfg.render,
        ..SceneConfig::default()
    }
}

// ---------------------------------------------------------------------------------------------
// 1. the profile reaches every consumer
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.preferences.render-options-reach-projection-and-device
/// The values below are each preference's *retail* mapping of the fixture's string, not this
/// build's reading of it.
#[test]
fn the_profile_s_render_preferences_reach_the_projection_and_the_device() {
    let (_, cfg) = config_from(FIXTURE);
    let r = cfg.render;

    // The original choice arm maps the label to its registered value. The two detail scales run
    // backwards (`VeryHigh` = 0), while the landscape draw-distance table jumps to 25 for
    // `Extreme`.
    assert_eq!(r.texture_filtering, 3, "Anisotropic");
    assert_eq!(r.landscape_texture_detail, 0, "VeryHigh is 0, not 4");
    assert_eq!(r.environment_texture_detail, 4, "VeryLow is 4");
    assert_eq!(
        r.scenery_draw_distance, 2,
        "High, no value array, so the index is the value"
    );
    assert_eq!(r.landscape_draw_distance, 25, "Extreme");
    assert_eq!(r.aspect_ratio, 2, "Wide");
    assert!(
        !r.environment_detail_textures,
        "Render.BuildingDetailTextures=False"
    );
    assert!(
        r.landscape_detail_textures,
        "registered and round-trips; the landscape detail texture's switch"
    );
    assert!(r.multi_pass_alpha);
    assert!(!r.automatic_degrades);
    assert!((r.field_of_view - 120.0).abs() < 1e-6);
    assert!((r.screen_brightness - 0.5).abs() < 1e-6);
    assert!((r.graphics_performance - 0.30).abs() < 1e-6);
    assert!((r.degrade_distance - 75.0).abs() < 1e-6);
    // The terrain window's configuration field shares the preference and must not have drifted
    // from it. `scene_config` below copies only `cfg.render`; this assertion does not claim that its
    // `cfg.land_radius` value reaches the loaded scene.
    assert_eq!(
        cfg.land_radius, 25,
        "Render.LandscapeDrawDistance still reaches the terrain window radius"
    );
    // The original field-of-view conversion multiplies degrees by the radians-per-degree constant.
    assert!((r.game_fov_rad() - 120.0 * 0.017_453_292).abs() < 1e-6);
    // The original image-scale branch computes `max(detail, 1) - 1`.
    assert_eq!(RenderPreferences::image_scale(0), 0);
    assert_eq!(RenderPreferences::image_scale(1), 0);
    assert_eq!(RenderPreferences::image_scale(4), 3);

    let mut gpu = crate::common::test_gpu(W, H);
    let store = store();
    let scene = WorldScene::load(&store, &mut gpu, scene_config(&cfg)).expect("the scene loads");

    // --- the projection. The vertical field is the game FOV divided by
    // `(viewport_aspect - 0.1)`, where viewport aspect is
    // `(w/h) * display_aspect * 0.75`. The profile selects 16:9 instead of 4:3.
    let v = scene.view_params(W, H);
    let display = 16.0f32 / 9.0;
    let aspect = (W as f32 / H as f32) * display * dereth_render::camera::ASPECT_FACTOR;
    let want = (120.0 * dereth_render::camera::DEG_TO_RAD)
        / (aspect - dereth_render::camera::FOV_ASPECT_BIAS);
    assert!(
        (v.aspect - aspect).abs() < 1e-6,
        "Render.AspectRatio=Wide did not reach the display aspect: {} vs {aspect}",
        v.aspect
    );
    assert!(
        (v.fov_y_rad - want).abs() < 1e-6,
        "Render.FieldOfView=120 did not reach the projection: {} vs {want}",
        v.fov_y_rad
    );
    // The default profile is the other half of the same claim: 90 degrees and 4:3, which is the
    // field of 1.2736190 rad measured in the retail process.
    let (_, plain) = config_from("");
    let base = WorldScene::load(&store, &mut gpu, scene_config(&plain)).expect("loads");
    let bv = base.view_params(W, H);
    assert!((bv.aspect - (W as f32 / H as f32) * (4.0 / 3.0) * 0.75).abs() < 1e-6);
    assert!(
        (bv.fov_y_rad - 1.273_619).abs() < 1e-5,
        "the retail default: {}",
        bv.fov_y_rad
    );
    assert!(bv.fov_y_rad != v.fov_y_rad, "the profile changed nothing");
    // **The two preferences pull opposite ways and the fixture moves both**, which is why the
    // exact formula above is the assertion and not "wider": at a fixed viewport a *wider*
    // display aspect divides the game FOV by more, so `AspectRatio=Wide` narrows the
    // vertical field even as `FieldOfView=120` widens it. Isolated, each moves the way its label
    // says.
    let fov_only = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            render: RenderPreferences {
                field_of_view: 120.0,
                ..RenderPreferences::default()
            },
            ..scene_config(&plain)
        },
    )
    .expect("loads");
    assert!(
        fov_only.view_params(W, H).fov_y_rad > bv.fov_y_rad,
        "120 degrees must be a wider projection than 90 at the same aspect"
    );
    let wide_only = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            render: RenderPreferences {
                aspect_ratio: 2,
                ..RenderPreferences::default()
            },
            ..scene_config(&plain)
        },
    )
    .expect("loads");
    assert!(
        wide_only.view_params(W, H).fov_y_rad < bv.fov_y_rad,
        "16:9 must narrow the vertical field at the same viewport"
    );

    // --- the two degrade globals consumed by object degradation.
    let g = scene.degrade_globals();
    assert!(
        (g.degrade_distance - 75.0).abs() < 1e-6,
        "Render.DegradeDistance did not reach the scene degradation distance"
    );
    assert!(
        (g.user_bias - 0.30).abs() < 1e-6,
        "Render.GraphicsPerformance did not reach the scene user-supplied degradation bias: {}",
        g.user_bias
    );
    assert!(
        (base.degrade_globals().degrade_distance - 50.0).abs() < 1e-6,
        "the .data default"
    );
    assert_eq!(base.degrade_globals().user_bias, 0.0);

    // --- the land texture shift, through the current
    // `dereth_world_render::land::merge::land_texture_scale_shift` helper.
    assert_eq!(
        dereth_world_render::land::merge::land_texture_scale_shift(r.landscape_texture_detail),
        0,
        "VeryHigh is full resolution"
    );
    assert_eq!(
        dereth_world_render::land::merge::land_texture_scale_shift(2),
        1,
        "and Medium, the default, halves it -- so the two are distinguishable"
    );
    assert_eq!(
        scene.draw.cfg.render.landscape_texture_detail, 0,
        "the scene took the profile's value"
    );

    // --- the device. This station calls `Gpu::set_gamma` directly with the parsed
    // `Render.ScreenBrightness`; it does not prove an automatic configuration-to-device path.
    gpu.set_gamma(r.screen_brightness);
    assert!((gpu.gamma() - 0.5).abs() < 1e-6);
    // The clamp is `[-0.2, 1.0]`, not the slider's declared `[-1.0, 1.0]`.
    gpu.set_gamma(-1.0);
    assert!(
        (gpu.gamma() - (-0.2)).abs() < 1e-6,
        "the gamma lower clamp is -0.2"
    );
    gpu.set_gamma(5.0);
    assert!(
        (gpu.gamma() - 1.0).abs() < 1e-6,
        "the gamma upper clamp is 1.0"
    );

    // --- the ramp itself, entry by entry, against the original arithmetic above.
    let ramp = dereth_render::gamma_ramp(0.0);
    for (i, e) in ramp.iter().enumerate() {
        assert_eq!(usize::from(*e), i * 255, "v = 0 is the 255*i ramp");
    }
    assert_eq!(
        dereth_render::gamma_ramp(0.5)[100],
        100 * 255 + 100 * 255,
        "255i + 510iv at v = 0.5"
    );
    assert_eq!(dereth_render::gamma_ramp(-0.2)[255], 39015, "0.6 of 65025");
    assert_eq!(dereth_render::gamma_ramp(1.0)[255], 65535, "saturated");
}

// ---------------------------------------------------------------------------------------------
// 2. the gamma is pixels, not a number in a struct
// ---------------------------------------------------------------------------------------------

/// The mean luminance of the same headless frame at three brightnesses.
fn mean_luma(gpu: &mut Gpu, store: &Arc<RetailDatStore>, scene: &mut WorldScene, v: f32) -> f64 {
    gpu.set_gamma(v);
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let rgba = gpu.capture().expect("capture").to_rgba();
    let sum: f64 = rgba
        .chunks_exact(4)
        .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
        .sum();
    sum / (rgba.len() / 4) as f64
}

/// Behaviour: presentation.brightness.it-changes-the-games-own-picture-and-not-the-display
/// **`Render.ScreenBrightness` changes the picture.** The ramp is linear in the pixel value —
/// `ramp[i] = 255i(1 + 2v)`, saturating — so relative to `v = 0` the frame is scaled by `1 + 2v`.
/// The software rendering path applies that factor in its finish shader; the observable is the
/// full captured frame's weighted mean luminance, using coefficients
/// 0.299/0.587/0.114. No player body is attached to this scene.
#[test]
fn a_brighter_profile_draws_a_brighter_frame() {
    let mut gpu = crate::common::test_gpu(W, H);
    let store = store();
    let (_, cfg) = config_from("");
    let mut scene = WorldScene::load(&store, &mut gpu, scene_config(&cfg)).expect("loads");

    let dark = mean_luma(&mut gpu, &store, &mut scene, -0.2);
    let flat = mean_luma(&mut gpu, &store, &mut scene, 0.0);
    let bright = mean_luma(&mut gpu, &store, &mut scene, 0.5);
    eprintln!(
        "render preferences: mean luminance: v=-0.2 {dark:.3}  v=0 {flat:.3}  v=+0.5 {bright:.3}"
    );

    assert!(
        flat > 1.0,
        "the frame is not black to begin with, or there is nothing to measure"
    );
    assert!(
        dark < flat,
        "v = -0.2 must darken the frame: {dark:.3} vs {flat:.3}"
    );
    assert!(
        bright > flat,
        "v = +0.5 must brighten it: {bright:.3} vs {flat:.3}"
    );
    // `1 + 2v` is 0.6 and 2.0. Saturation and the alpha-blended sky keep the measured ratio below
    // the ideal on the bright side and on it on the dark side, so the bounds are one-sided.
    let dim_ratio = dark / flat;
    assert!(
        (0.55..=0.70).contains(&dim_ratio),
        "v = -0.2 should scale the frame by about 0.6, measured {dim_ratio:.3}"
    );
    assert!(
        bright / flat > 1.2,
        "v = +0.5 should be visibly brighter, measured {:.3}",
        bright / flat
    );
}

// ---------------------------------------------------------------------------------------------
// 3. the options page's Apply
// ---------------------------------------------------------------------------------------------

fn ui_env() -> UiSystem {
    let dir = dereth_dat::testing::dat_dir();
    let store = RetailDatStore::open_dir(&dir).expect("the retail dats open");
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("MasterProperty decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    let store = Rc::new(store);
    let resolver =
        Rc::new(DidMapperResolver::load_via_master(store.as_ref()).expect("the DidMapper loads"));
    dereth_ui_screens::env::install(&mut ui, store, resolver);
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    ui
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
        }
    }
}

/// Press the slider row for `preference` a given fraction along its bar and return the value the
/// page wrote. The slider computes `lower + frac * (upper - lower)` and the current page emits a
/// preference request. The press is a `mouse_down`/`mouse_up` pair on the element the shipped
/// layout built, so a row that
/// is not hit-testable fails here rather than passing.
fn drag(ui: &mut UiSystem, preference: &str, frac: f32) -> f32 {
    // A screen of its own per drag. The shipped Client Options page stacks its six sections in
    // one column and only one is expanded at a time; forcing two of them visible at once puts a
    // row of the first over a row of the second, and the press then lands on the wrong element.
    // That is a property of the layout, not of the preference wiring, so each press gets a
    // freshly built page in the state a player's would be in.
    let mut screen = GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(ui))
        .expect("the gameplay screen builds");
    let s = &mut screen;
    let i = s
        .config_page
        .options
        .iter()
        .position(|o| o.preference == preference)
        .unwrap_or_else(|| panic!("{preference} is not on the built page"));
    let bar = s.config_page.options[i].element;
    let mut h = bar;
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    // The rows live in the client-options list box, which clips: a row below the box's
    // viewport is laid out but not hit-testable, exactly as it is for a player who has not
    // scrolled. `scroll_to_view` brings the selected row into view before the click.
    if let Some(list) = s.config_page.option_box.as_mut() {
        let mut h = bar;
        let idx = loop {
            if let Some(i) = list.index_of(h) {
                break Some(i);
            }
            match ui.parent(h) {
                Some(p) => h = p,
                None => break None,
            }
        };
        if let Some(i) = idx {
            list.scroll_to_view(ui, i);
        }
    }
    ui.drain_outbox();
    let b = ui.node(bar).expect("the bar").region.box_;
    let (ox, oy) = ui.screen_origin(bar);
    let y = oy + b.height() / 2;
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let x = ox + (b.width() as f32 * frac) as i32;
    eprintln!(
        "render preferences: drag {preference}: bar {}x{} at ({ox}, {oy}), press ({x}, {y})",
        b.width(),
        b.height()
    );
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump(ui, s);
    match dereth_ui_screens::options::store::inq_value(preference) {
        Some(PrefValue::Float(v)) => v,
        other => panic!("{preference} did not land in the store: {other:?}"),
    }
}

/// **The options pages change the picture live.** Click the Field of View and Brightness slider
/// rows at 75% on freshly built shipped Client Options pages. The emitted requests are applied directly, after
/// which `WorldScene::view_params` and `Gpu::gamma` expose the updated values without a restart or
/// second read of the profile. This station does not render post-click pixels.
///
/// The element outbox is drained into each screen, then the global requests are taken exactly as
/// the application dispatch takes them and handed to the same owner it uses,
/// (`dereth_client::render_prefs::apply_preference_requests`).
#[test]
fn the_options_pages_apply_changes_the_picture_live() {
    let gpu = crate::common::test_gpu(W, H);
    drop(gpu); // the renderer below creates its own device; this was only the availability probe.
    let store = store();
    let mut renderer =
        dereth_client::gpu::Renderer::new(None, W, H).expect("a headless GPU renderer");
    let (_, cfg) = config_from("");
    let mut world = None;
    renderer
        .load_world(&store, scene_config(&cfg), &mut world)
        .expect("the world loads");
    let ws = world.as_ref().expect("a world state");

    let before = renderer
        .world()
        .expect("a world")
        .view_params(ws, W, H)
        .fov_y_rad;
    let gamma_before = renderer.gamma();
    assert!(
        (gamma_before - 0.0).abs() < 1e-6,
        "the default profile is the identity ramp"
    );

    let mut ui = ui_env();
    ui.requests.clear();

    // Three quarters along a 10..160 slider is 122.5 degrees; three quarters along a -1..1 one is
    // +0.5, which the gamma ceiling leaves alone.
    let fov = drag(&mut ui, "Render.FieldOfView", 0.75);
    let bright = drag(&mut ui, "Render.ScreenBrightness", 0.75);
    assert!(
        fov > 100.0,
        "the Field of View drag wrote {fov}, which is not three quarters along"
    );
    assert!(bright > 0.1, "the Brightness drag wrote {bright}");

    let requests = ui.requests.take();
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, UiRequest::SetPreference(n, _) if *n == "Render.FieldOfView")),
        "the page emitted no Render.FieldOfView write: {requests:?}"
    );
    let left = dereth_client::render_prefs::apply_preference_requests(&mut renderer, requests);
    for r in &left {
        if let UiRequest::SetPreference(n, _) = r {
            assert!(
                !n.eq_ignore_ascii_case("Render.FieldOfView")
                    && !n.eq_ignore_ascii_case("Render.ScreenBrightness"),
                "{n} came back unowned"
            );
        }
    }

    let after = renderer
        .world()
        .expect("a world")
        .view_params(ws, W, H)
        .fov_y_rad;
    let aspect = renderer
        .world()
        .expect("a world")
        .view_params(ws, W, H)
        .aspect;
    let want = (fov * dereth_render::camera::DEG_TO_RAD)
        / (aspect - dereth_render::camera::FOV_ASPECT_BIAS);
    assert!(
        (after - want).abs() < 1e-5,
        "the projection did not follow the slider: {after} vs {want} (was {before})"
    );
    assert!(
        after > before,
        "122.5 degrees is a wider projection than 90"
    );
    assert!(
        (renderer.gamma() - bright).abs() < 1e-6,
        "the device gamma did not follow the Brightness slider: {} vs {bright}",
        renderer.gamma()
    );
}

/// The Holtburg yard the camera and landscape tests measure at.
const HOLTBURG: u16 = 0xA9B4;

// ---------------------------------------------------------------------------------------------
// The shipped options page, driven the way a player drives it.
// ---------------------------------------------------------------------------------------------

/// Drain the UI outbox into the gameplay screen.
fn pump_live(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..8 {
        let out = ui.drain_outbox();
        if out.is_empty() {
            break;
        }
        for d in out {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

fn index_of(p: &PlayerOptionPage, pref: &str) -> usize {
    p.options
        .iter()
        .position(|o| o.preference == pref)
        .unwrap_or_else(|| panic!("{pref}"))
}

/// Show every ancestor of an element and scroll its list-box row into view.
///
/// The options page is one of sixteen stacked pages and starts hidden. Its list box clips rows
/// below the viewport: a row is laid out but cannot be hit until the reveal path scrolls it into
/// view, matching a player scrolling the page.
fn reveal(ui: &mut UiSystem, s: &mut GamePlayScreen, h: dereth_ui::ElemHandle) {
    let mut a = h;
    loop {
        ui.set_visible(a, true);
        match ui.parent(a) {
            Some(p) => a = p,
            None => break,
        }
    }
    if let Some(list) = s.config_page.option_box.as_mut() {
        let mut a = h;
        let idx = loop {
            if let Some(i) = list.index_of(a) {
                break Some(i);
            }
            match ui.parent(a) {
                Some(p) => a = p,
                None => break None,
            }
        };
        if let Some(i) = idx {
            list.scroll_to_view(ui, i);
        }
    }
    ui.drain_outbox();
}

/// A real `mouse_down`/`mouse_up` at the centre of an element, asserted to land on it first.
fn press(ui: &mut UiSystem, s: &mut GamePlayScreen, h: dereth_ui::ElemHandle, what: &str) {
    let b = ui.screen_box(h);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(
        ui.hit_test_screen(cx, cy),
        Some(h),
        "the press does not land on {what}"
    );
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
    pump_live(ui, s);
}

/// Choose row `k` of one of the page's drop-downs with two presses: one opens the closed menu and
/// one selects the row. The row selection handles element message 7 and applies the option.
///
/// Returns the `UiRequest` the page emitted.
fn choose(pref: &'static str, row: usize) -> UiRequest {
    let mut ui = ui_env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let menu = s.config_page.options[index_of(&s.config_page, pref)].element;
    reveal(&mut ui, &mut s, menu);
    press(&mut ui, &mut s, menu, pref);
    let popup =
        dereth_ui::widgets::menu::popup_handle(&ui, menu).expect("the drop-down popup exists");
    assert!(
        ui.node(popup).expect("live").region.flags.visible,
        "{pref}: the drop-down did not open"
    );
    let item = dereth_ui::widgets::menu::get_item(&ui, menu, row)
        .unwrap_or_else(|| panic!("{pref} has no row {row}"));
    ui.requests.clear();
    press(&mut ui, &mut s, item, "the drop-down row");
    let mut reqs = ui.requests.take();
    reqs.retain(|r| matches!(r, UiRequest::SetPreference(n, _) if *n == pref));
    assert_eq!(reqs.len(), 1, "{pref}: the page emitted {reqs:?}");
    reqs.pop().expect("one")
}

/// Tick one of the page's check boxes and return the `UiRequest` it emitted.
///
/// The button owns its `0x0E` (`ATTR_CHECKED`) and raises element message 1 once it has flipped the
/// attribute; the option handler then reads it back and applies the change. The attribute is set
/// here and the message raised on the element, which is the same pair used for the sound-option
/// check boxes. This build's
/// `UiSystem` does not own a button's checked state, so a bare press would flip nothing.
fn tick(pref: &'static str, on: bool) -> UiRequest {
    let mut ui = ui_env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let cb = s.config_page.options[index_of(&s.config_page, pref)].element;
    reveal(&mut ui, &mut s, cb);
    ui.set_attribute_bool(cb, dereth_ui_screens::options::page::ATTR_CHECKED, on);
    ui.drain_outbox();
    ui.requests.clear();
    ui.broadcast_element_message(cb, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    pump_live(&mut ui, &mut s);
    let mut reqs = ui.requests.take();
    reqs.retain(|r| matches!(r, UiRequest::SetPreference(n, _) if *n == pref));
    assert_eq!(reqs.len(), 1, "{pref}: the page emitted {reqs:?}");
    reqs.pop().expect("one")
}

// ---------------------------------------------------------------------------------------------
// 1. The page really writes all five of them
// ---------------------------------------------------------------------------------------------

/// **The instrument's calibration.** Every App test below feeds the renderer a `UiRequest` it
/// claims the options page produces; this is where that claim is checked, by building the
/// shipped page and driving each of the five controls the way a player does.
#[test]
fn the_shipped_options_page_emits_all_five_render_preferences() {
    // The two detail lists are `[VeryLow, Low, Medium, High, VeryHigh]` against the value array
    // `[4, 3, 2, 1, 0]`, so the **scale runs backwards**: row 0
    // is `VeryLow` = 4 and row 4 is `VeryHigh` = 0. The draw distance's is `[3, 5, 8, 11, 15, 25]`
    // against `[VeryLow .. Extreme]`, and runs forwards.
    assert_eq!(
        choose("Render.LandscapeTextureDetail", 4),
        UiRequest::SetPreference("Render.LandscapeTextureDetail", PrefValue::Int(0)),
        "row 4 of the Landscape Texture Detail drop-down is VeryHigh, whose value is 0"
    );
    assert_eq!(
        choose("Render.EnvironmentTextureDetail", 1),
        UiRequest::SetPreference("Render.EnvironmentTextureDetail", PrefValue::Int(3)),
        "row 1 is Low, whose value is 3"
    );
    assert_eq!(
        choose("Render.LandscapeDrawDistance", 0),
        UiRequest::SetPreference("Render.LandscapeDrawDistance", PrefValue::Int(3)),
        "row 0 of the Landscape Draw Distance drop-down is VeryLow = 3"
    );
    assert_eq!(
        tick("Render.MultiPassAlpha", true),
        UiRequest::SetPreference("Render.MultiPassAlpha", PrefValue::Bool(true))
    );
    assert_eq!(
        tick("Render.BuildingDetailTextures", false),
        UiRequest::SetPreference("Render.BuildingDetailTextures", PrefValue::Bool(false)),
        "the page's `Environment Detail Textures` row is `Render.BuildingDetailTextures`"
    );
}

// ---------------------------------------------------------------------------------------------
// The App harness — the real `App::frame`.
// ---------------------------------------------------------------------------------------------

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

fn app() -> Option<App> {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir()
            .join("dereth-live-preferences-not-created/prefs.ini"),
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
    // Settle. The first frames stream the window and the next few finish assembling the body's
    // batches and its degrade levels, so the part pass is still growing; measuring a before and an
    // after across that ramp compares two different scenes. This runs until two consecutive frames
    // draw the same part pass; without that gate the multi-pass arm can read `clip 8 -> 10` on a
    // preference that cannot move a subset between lists.
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

/// Hand the page's own request to the running client and let the client's own frame do the rest.
///
/// **Two frames, and the split is the point.** `App::frame` runs `stream_world`, which
/// contains the renderer-preference poll, before `interaction_use_time`, where the frame's
/// `UiRequest`s reach their owners. The frame carrying the option updates the live render
/// preferences, and the **next** frame is the one whose poll sees it. This is the original
/// client's observed order too: device preparation polls before the options-page handler runs.
/// That one-frame delay is exactly what "no restart" means here.
fn change(app: &mut App, r: UiRequest) -> u64 {
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(r);
    assert!(app.frame(), "the frame that carries the option write");
    let before = app.render_pref_applies();
    assert!(app.frame(), "the frame whose poll sees it");
    app.render_pref_applies() - before
}

// ---------------------------------------------------------------------------------------------
// 2. Multiple Pass Alpha
// ---------------------------------------------------------------------------------------------

/// `(clip-mapped subsets deferred, of those, how many were ALSO drawn in place)` for the frame
/// the scene last drew — **one frame, both halves**, from `WorldScene::drawn_part_order`, which is
/// the trace of every subset that went on the device in the order it went.
///
/// This is frame-local on purpose. Totals from two different frames do not hold still: the body
/// idles, and view-cone culling chooses a different set of its parts every frame, so `clip` can
/// wander 8 -> 10 -> 15 across a run on a preference that cannot move a subset between lists.
/// Two numbers taken inside one frame cannot drift against each other.
fn second_pass_census(app: &App) -> (usize, usize) {
    use std::collections::BTreeSet;
    let trace = app.world_scene().expect("a world").drawn_part_order();
    let key = |d: &dereth_client::world::PartSubsetDraw| (d.object.map(|o| o.0), d.part, d.subset);
    let deferred: BTreeSet<_> = trace
        .iter()
        .filter(|d| d.list == Some(dereth_world_render::objects::alpha::AlphaList::Clip))
        .map(key)
        .collect();
    let in_place: BTreeSet<_> = trace.iter().filter(|d| d.list.is_none()).map(key).collect();
    (deferred.len(), deferred.intersection(&in_place).count())
}

/// **Rejecting.** The original mesh-draw multipass arm defers each clip-mapped subset *and* draws
/// it immediately, so every such subset reaches the device twice. The ordinary delay-mask arm
/// defers the subset and jumps to the loop tail, producing one pass.
///
/// **The lists cannot measure this.** The alpha-delay mask `0x0E` makes both arms choose
/// `AlphaList::Clip` for a clip-mapped subset (`0x0E & 8 != 0`), so `clip` and `blend` are the
/// same numbers on both settings; a preference wired only to the list choice does nothing at all.
#[test]
fn multiple_pass_alpha_adds_a_second_device_pass_on_the_next_frame() {
    let Some(mut app) = app() else { return };
    let (deferred0, both0) = second_pass_census(&app);
    eprintln!("live preferences: MultiPassAlpha off: {deferred0} clip-mapped subset(s), {both0} drawn twice");
    assert!(
        deferred0 > 0,
        "nothing on this station has a clip-mapped subset, so the instrument cannot see the \
         preference at all -- an empty result is not a negative result"
    );
    assert_eq!(
        both0, 0,
        "with the option off, the immediate subset draw is skipped and no deferred \
         subset may appear in the trace a second time"
    );
    assert!(
        !app.world_scene()
            .expect("a world")
            .draw
            .cfg
            .render
            .multi_pass_alpha
    );

    // **This one takes no poll arm and must not.** `Render.MultiPassAlpha` has no shadow copy in
    // the original client. Its mesh draw reads the preference live, per mesh, per frame. A build
    // that had to flush or rebuild for it would be doing something the original client does not.
    assert_eq!(
        change(&mut app, tick("Render.MultiPassAlpha", true)),
        0,
        "the poll acted on a preference the client's poll does not look at"
    );

    assert!(
        app.world_scene()
            .expect("a world")
            .draw
            .cfg
            .render
            .multi_pass_alpha,
        "the option did not reach the live render preferences"
    );
    let (deferred1, both1) = second_pass_census(&app);
    eprintln!("live preferences: MultiPassAlpha on:  {deferred1} clip-mapped subset(s), {both1} drawn twice");
    assert!(
        deferred1 > 0,
        "the clip list emptied, which the preference cannot do"
    );
    assert_eq!(
        both1,
        deferred1,
        "every clip-mapped subset must now ALSO be drawn in place, and {} of {deferred1} were not",
        deferred1 - both1
    );
    assert_eq!(app.stream_failures(), 0);
}

/// Behaviour: rendering.preferences.multiple-pass-alpha-blends-the-edges-the-cut-out-drops
/// **Rejecting.** The option's second pass is what softens the edges: the alpha-list flush draws
/// each entry the option queued with surface setup's force alpha, blended and without the alpha
/// test. Drawn like any other clip-list entry, alpha-tested and unblended, the two passes are the
/// same hard cut-out twice and the option changes no pixel. Scenery and buildings get the same
/// second pass at the frame's alpha flush; with the option off nothing does.
#[test]
fn multiple_pass_alpha_s_second_pass_is_blended_and_not_alpha_tested() {
    use dereth_world_render::objects::alpha::AlphaList;
    let Some(mut app) = app() else { return };
    let forced = |app: &App| {
        let s = app.world_scene().expect("a world");
        let trace = s.drawn_part_order();
        let clip8 = trace
            .iter()
            .filter(|d| d.list == Some(AlphaList::Clip) && d.mask == 8)
            .count();
        let forced = trace.iter().filter(|d| d.force_alpha).count();
        let forced_clip8 = trace
            .iter()
            .filter(|d| d.force_alpha && d.list == Some(AlphaList::Clip) && d.mask == 8)
            .count();
        (
            clip8,
            forced,
            forced_clip8,
            s.drawn_alpha_lists().multipass,
            s.drawn_landscape_alpha(),
        )
    };

    let (clip0, forced0, _, parts0, land0) = forced(&app);
    eprintln!(
        "MultiPassAlpha off: {clip0} clip-mapped part subset(s) flushed, {forced0} forced; \
         landscape {} alpha-tested batch(es), {} second pass(es)",
        land0.clip, land0.multipass
    );
    assert!(
        clip0 > 0,
        "nothing on this station has a clip-mapped part subset, so the instrument cannot see the \
         second pass at all"
    );
    assert!(
        land0.clip > 0,
        "the landscape has no alpha-tested batch here, so its second pass cannot be seen"
    );
    assert_eq!(forced0, 0, "with the option off no draw is forced alpha");
    assert_eq!(parts0, 0);
    assert_eq!(
        land0.multipass, 0,
        "with the option off the landscape has one pass"
    );

    change(&mut app, tick("Render.MultiPassAlpha", true));
    let (clip1, forced1, forced_clip1, parts1, land1) = forced(&app);
    eprintln!(
        "MultiPassAlpha on:  {clip1} clip-mapped part subset(s) flushed, {forced1} forced; \
         landscape {} alpha-tested batch(es), {} second pass(es)",
        land1.clip, land1.multipass
    );
    assert!(
        clip1 > 0,
        "the clip list emptied, which the option cannot do"
    );
    assert_eq!(
        forced_clip1, clip1,
        "every clip-mapped entry the flush drew must be its blended second pass"
    );
    assert_eq!(
        forced1, clip1,
        "only those entries are forced: no in-place draw and no blend-list entry"
    );
    assert_eq!(parts1, clip1, "the census agrees with the trace");
    assert!(
        land1.multipass > 0 && land1.multipass <= land1.clip,
        "the landscape's alpha-tested batches get a second pass at the frame's flush \
         ({} of {})",
        land1.multipass,
        land1.clip
    );
    assert_eq!(app.stream_failures(), 0);
}

// ---------------------------------------------------------------------------------------------
// 3. Landscape Draw Distance
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.preferences.a-render-option-applies-on-the-next-frame
/// **Rejecting.** Changing the mid radius releases the whole landblock ring and repopulates it
/// around the viewer at the new radius on the next frame.
#[test]
fn the_landscape_draw_distance_rebuilds_the_ring_on_the_next_frame() {
    let Some(mut app) = app() else { return };
    let (r0, n0) = {
        let s = app.world_scene().expect("a world");
        (s.mid_radius(), s.resident_blocks())
    };
    assert_eq!(r0, 2, "the scene was built at land_radius 2");
    assert!(
        n0 > 0,
        "nothing is resident, so a block count cannot measure anything"
    );

    // Row 1 of the drop-down is `Low` = 5.
    assert_eq!(
        change(&mut app, choose("Render.LandscapeDrawDistance", 1)),
        1,
        "the poll did not act on the next frame"
    );

    let s = app.world_scene().expect("a world");
    let work = app.last_render_pref_work();
    eprintln!(
        "live preferences: LandscapeDrawDistance: mid_radius {r0} -> {}, blocks {n0} -> {} ({work:?})",
        s.mid_radius(),
        s.resident_blocks()
    );
    assert!(
        work.mid_radius_changed,
        "the poll did not take the set_mid_radius arm"
    );
    assert_eq!(
        s.mid_radius(),
        5,
        "the landscape middle radius did not follow the option"
    );
    assert_eq!(s.render_shadow().landscape_draw_distance, 5);
    assert!(
        s.resident_blocks() > n0,
        "an 11x11 ring must hold more blocks than a 5x5 one: {} vs {n0}",
        s.resident_blocks()
    );
    assert_eq!(app.stream_failures(), 0, "the rebuild failed somewhere");
}

// ---------------------------------------------------------------------------------------------
// 4. Landscape Texture Detail
// ---------------------------------------------------------------------------------------------

/// **Rejecting.** The landscape texture scale is `max(v, 1) - 1`; a change then flushes graphics
/// resources and composites every resident cell again at the new size. This happens on the next
/// frame, rather than waiting for the next scene load.
#[test]
fn the_landscape_texture_detail_recomposites_the_terrain_on_the_next_frame() {
    let Some(mut app) = app() else { return };
    let (extent0, built0) = {
        let s = app.world_scene().expect("a world");
        (
            s.last_terrain_surface_built()
                .expect("the load composited at least one surface"),
            s.draw.stats.terrain_surfaces_built,
        )
    };
    assert_eq!(
        extent0.2, 1,
        "Render.LandscapeTextureDetail's default `Medium` (2) is shift 1"
    );

    // Row 4 is `VeryHigh` = 0. `land_texture_scale_shift` indexes `{0, 1, 2, 4, 8}` with
    // `max(v, 1) - 1`, so 0 is a shift of 0 — full resolution, and the
    // merged surface doubles on a side against the default's shift of 1.
    assert_eq!(
        change(&mut app, choose("Render.LandscapeTextureDetail", 4)),
        1,
        "the poll did not act on the next frame"
    );

    let s = app.world_scene().expect("a world");
    let work = app.last_render_pref_work();
    let extent1 = s
        .last_terrain_surface_built()
        .expect("the rebuild composited a surface");
    eprintln!(
        "live preferences: LandscapeTextureDetail: {}x{} shift {} -> {}x{} shift {}, surfaces built {built0} \
         -> {} ({work:?})",
        extent0.0,
        extent0.1,
        extent0.2,
        extent1.0,
        extent1.1,
        extent1.2,
        s.draw.stats.terrain_surfaces_built
    );
    assert!(
        work.flushed,
        "the poll did not take the graphics-resource flush arm"
    );
    assert_eq!(
        extent1.2, 0,
        "the landscape texture scale did not follow the option"
    );
    assert!(
        s.draw.stats.terrain_surfaces_built > built0,
        "nothing was re-composited, so the flush did not reach texture recomposition"
    );
    assert_eq!(
        (extent1.0, extent1.1),
        (extent0.0 * 2, extent0.1 * 2),
        "the merged terrain texture is not twice the size it was: {}x{} vs {}x{}",
        extent1.0,
        extent1.1,
        extent0.0,
        extent0.1
    );
    assert_eq!(app.stream_failures(), 0, "the rebuild failed somewhere");
}

// ---------------------------------------------------------------------------------------------
// 5. Environment Texture Detail
// ---------------------------------------------------------------------------------------------

/// **Rejecting.** Environment texture detail updates the clip-map, RGBA, and indexed texture
/// scales and then performs the same flush. Every object texture is created again at the new
/// current texture scale, which supplies the input expected by
/// `dereth_render::texture::scaled_dimensions`.
///
/// **The observable is a population, not a sample.** The last surface a rebuild happens to reach
/// can be block-compressed, and `dereth_render::texture::scale_surface` returns those at their
/// source extent by design (see its declared departure), so `last_object_texture_built` alone can
/// read "full resolution" on a run that scaled everything else. The texels over the whole rebuild
/// cannot do that.
#[test]
fn the_environment_texture_detail_reuploads_object_textures_on_the_next_frame() {
    let Some(mut app) = app() else { return };
    let (u0, up0, src0, d0, scale0) = {
        let s = app.world_scene().expect("a world");
        let (u, up, src, d) = s.object_texture_census();
        (
            u,
            up,
            src,
            d,
            s.last_object_texture_built()
                .expect("the load uploaded one")
                .2,
        )
    };
    assert_eq!(
        scale0, 0,
        "Render.EnvironmentTextureDetail's default (1) is FULL_RES"
    );
    assert!(
        u0 > 0,
        "no object texture was uploaded at all, so nothing here can measure anything"
    );
    assert_eq!(d0, 0, "something was already being downscaled at FULL_RES");
    assert_eq!(
        up0, src0,
        "at FULL_RES every image reaches the device at its source extent"
    );

    // Row 1 is `Low` = 3, i.e. `max(3,1) - 1` = 2 = `ImageScale::QuarterRes`.
    assert_eq!(
        change(&mut app, choose("Render.EnvironmentTextureDetail", 1)),
        1,
        "the poll did not act on the next frame"
    );

    let s = app.world_scene().expect("a world");
    let work = app.last_render_pref_work();
    let (u1, up1, src1, d1) = s.object_texture_census();
    let (du, dup, dsrc, dd) = (u1 - u0, up1 - up0, src1 - src0, d1 - d0);
    eprintln!(
        "live preferences: EnvironmentTextureDetail: at FULL_RES {u0} upload(s), {up0} texel(s) of {src0} \
         source texel(s), {d0} resampled; on the rebuild {du} upload(s), {dup} of {dsrc}, {dd} \
         resampled; scale now {:?} ({work:?})",
        s.last_object_texture_built().map(|b| b.2)
    );
    assert!(
        work.flushed,
        "the poll did not take the graphics-resource flush arm"
    );
    assert_eq!(
        s.last_object_texture_built()
            .expect("the rebuild uploaded one")
            .2,
        2,
        "the clip-map texture scale did not follow the option"
    );
    assert!(
        du > 0,
        "the flush released nothing, so nothing was created again"
    );
    assert!(
        dd > 0,
        "{du} texture(s) were uploaded again and not one of them was resampled -- \
         scale_surface is not reaching the texture-upload path"
    );
    // The **pair**, not the mean: which surfaces a rebuild happens to touch is not under this
    // test's control, but "what went to the device against what the dat holds" is a ratio of two
    // sums over the *same* population and is 1.0 at FULL_RES by construction.
    assert!(
        dup * 2 < dsrc,
        "the device is still being handed most of the image it was: {dup} of {dsrc} source texels"
    );
    assert_eq!(app.stream_failures(), 0, "the rebuild failed somewhere");
}

// ---------------------------------------------------------------------------------------------
// 6. Environment Detail Textures
// ---------------------------------------------------------------------------------------------

/// **The poll's own arm for Environment Detail Textures, measured rather than assumed.**
///
/// The chain passes `(false, v)` from world objects into landscape detail texturing as
/// `(false, v, v, 0)`. It cleans up and generates detail surfaces, invokes the four surface and
/// tiling setters, and supplies the resulting surface as a second texture stage during mesh
/// drawing; that surface also suppresses alpha-list deferral. The landscape half is dead in retail
/// too: all three callers pass a literal 0 for `landscape` and `object`, so the option's live
/// effect is buildings and environment cells. This test records the poll's own arm; the
/// subsystem is asserted in `rendering::detail_textures`.
///
/// The point of asserting it is that "the preference is unreachable" and "the preference is not
/// being polled" would otherwise print alike.
#[test]
fn environment_detail_textures_is_polled_and_reaches_no_subsystem_in_this_build() {
    // The poll's own flag and what the subsystem did are two separate measurements, so
    // "unreachable" and "not polled" cannot print alike. The subsystem is asserted in
    // `rendering/detail_textures.rs`; this keeps the poll half.
    assert!(
        dereth_client::world::DETAIL_TEXTURE_PASS,
        "the detail-texture pass has been removed again; `Render.BuildingDetailTextures` has \
         nowhere to go and `rendering/detail_textures.rs` is the file to reconcile with"
    );
    let Some(mut app) = app() else { return };
    let (_, both0) = second_pass_census(&app);
    let built0 = app.world_scene().expect("a world").object_texture_census();

    assert_eq!(
        change(&mut app, tick("Render.BuildingDetailTextures", false)),
        1,
        "the poll did not notice the change at all"
    );

    let work = app.last_render_pref_work();
    let s = app.world_scene().expect("a world");
    assert!(
        work.detail_texturing_changed,
        "the poll did not notice the change at all"
    );
    assert!(
        !work.flushed && !work.mid_radius_changed,
        "it took an arm that is not its own"
    );
    assert_eq!(work.blocks_queued, 0, "it rebuilt something");
    // The arm reaches the landscape detail-texture update. Turning the option **off** cleans up the detail surfaces, so zero
    // generated surfaces is the measurement.
    assert_eq!(
        work.detail_surfaces, 0,
        "the off arm left a detail surface installed"
    );
    assert_eq!(s.detail_texturing().generated(), 0);
    assert!(
        !s.draw.cfg.render.environment_detail_textures,
        "the option did not reach the live render preferences"
    );
    assert!(
        !s.render_shadow().environment_detail_textures,
        "the shadow did not move"
    );
    // Frame-local, for the reason [`second_pass_census`] gives: the totals wander with the
    // viewcone and only a within-frame invariant is comparable across a preference change.
    let (_, both1) = second_pass_census(&app);
    assert_eq!(both0, 0, "the control arm is not a control");
    assert_eq!(
        both1, 0,
        "the draw grew a second pass, which this preference cannot do"
    );
    assert_eq!(
        s.object_texture_census(),
        built0,
        "a texture was created again, which is the flush arm and not this one"
    );
    eprintln!(
        "live preferences: BuildingDetailTextures: polled, recorded, and acted on -- \
         the landscape detail-texture update released both detail surfaces"
    );
}
