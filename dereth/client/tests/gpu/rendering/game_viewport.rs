//! The world is drawn inside the game view rectangle: moving or resizing the smart box moves the
//! viewport and scissor the scene pass installs, and the projection's aspect follows the
//! rectangle rather than the window. The smart box's own screen box is the final device state
//! (its update notice ignores its parameters and rereads the element), so that is the rectangle
//! asserted. Controls: the pixels an inset run leaves untouched are painted with the viewport at
//! the whole window, and a scissor-only build (projection left on the window) is rejected.
//! Fixture: the Holtburg landscape from the retail dats on a software device, and a headless
//! `App` on the gameplay screen. Fails when the retail dats are absent; skips only without a
//! device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_dat::RetailDatStore;
use dereth_render::camera::Viewport;
use dereth_render::device::Gpu;
use dereth_ui::framework::mode;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

const W: u32 = 640;
const H: u32 = 480;

/// The inset rectangle every pixel assertion below is made against -- deliberately off-centre, so
/// that a build that centred the view rather than positioning it would fail.
const INSET: Viewport = Viewport {
    x: 96,
    y: 48,
    width: 320,
    height: 240,
};

/// The retail store, or **fail**.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn cfg() -> SceneConfig {
    SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    }
}

/// One drawn frame's RGBA, with `viewport` installed.
fn frame(store: &Arc<RetailDatStore>, gpu: &mut Gpu, vp: Option<Viewport>) -> Vec<u8> {
    let mut scene = WorldScene::load(store, gpu, cfg()).expect("the landscape loads");
    scene.set_game_viewport(vp);
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

fn px(buf: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

/// Pixels that are inside `INSET`, and pixels that are outside it, sampled far enough from the
/// boundary that a one-pixel inclusive/exclusive disagreement cannot decide the test.
fn inside_points() -> Vec<(u32, u32)> {
    vec![(110, 60), (200, 150), (256, 200), (400, 280)]
}
fn outside_points() -> Vec<(u32, u32)> {
    vec![
        (10, 10),
        (600, 20),
        (20, 460),
        (600, 460),
        (320, 400),
        (500, 150),
    ]
}

// ---------------------------------------------------------------------------------------------
// 1. The rejecting tests.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.viewport.the-scene-renders-inside-the-world-view-rectangle
/// **The scene renders inside the rectangle and nowhere else.** Rejecting test, in pixels.
///
/// `Gpu::set_viewport` installs matching viewport and scissor rectangles. The D3D12 backend
/// uses both `RSSetViewports` and `RSSetScissorRects`, since its viewport does not provide D3D9's
/// clipping behavior. The outside pixels must remain untouched, not merely unprojected.
#[test]
fn the_scene_renders_inside_the_world_view_rectangle() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);

    let full = frame(&store, &mut gpu, None);
    let inset = frame(&store, &mut gpu, Some(INSET));

    // Outside the rect the inset frame must still be whatever `begin_frame` cleared to -- i.e.
    // identical everywhere to a frame in which nothing was drawn at all. Comparing against the
    // *full* frame's own clear colour would be circular, so the reference is the clear itself.
    let cleared = {
        gpu.begin_frame().expect("begin");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    };
    for (x, y) in outside_points() {
        assert_eq!(
            px(&inset, x, y),
            px(&cleared, x, y),
            "({x},{y}) is outside {INSET:?} and must be untouched by the scene pass"
        );
    }

    // Inside the rect it must have drawn, and drawn something *different* from the full-window
    // frame -- the same world through a different projection is a different picture.
    let changed = inside_points()
        .into_iter()
        .filter(|&(x, y)| px(&inset, x, y) != px(&cleared, x, y))
        .count();
    assert!(changed > 0, "the scene must actually draw inside {INSET:?}");
    assert_ne!(
        inside_points()
            .into_iter()
            .map(|(x, y)| px(&inset, x, y))
            .collect::<Vec<_>>(),
        inside_points()
            .into_iter()
            .map(|(x, y)| px(&full, x, y))
            .collect::<Vec<_>>(),
        "the reframed view must differ from the full-window one"
    );
}

/// **Resizing `<SBOX>` moves the rectangle that reaches the renderer.** Rejecting test, through
/// the real `App::frame`.
///
/// Resize the smart box, and the game viewport follows. The path exercised is `App::game_viewport` -> `Renderer::set_game_viewport` ->
/// `WorldScene::set_game_viewport`, all inside `App::frame`'s own `FrameStep::DrawWorld`.
#[test]
fn resizing_the_world_view_moves_the_game_viewport() {
    let Ok(mut app) = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-game-viewport-not-created")
            .join("preferences.ini"),
        ..Config::default()
    }) else {
        panic!("the headless client did not start");
    };
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(mode::GAME_PLAY);
    assert!(app.frame());
    app.probe_mut()
        .load_world(&store(), cfg())
        .expect("a world to draw");
    assert!(app.frame());

    let sbox = {
        let shell = app.ui().unwrap();
        let any: &dyn std::any::Any = shell.flow.current().unwrap();
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        shell
            .ui
            .get_child_recursive(
                screen.root().expect("a root"),
                dereth_ui_screens::hud::world_view::SMART_BOX,
            )
            .expect("<SBOX> is in the tree")
    };

    // `Renderer::game_viewport` is the **effective** rectangle -- `view_params(...).viewport`, the
    // value that reaches `Gpu::set_viewport` -- and not the field `set_game_viewport` stored. A
    // build that took the rect and then ignored it answers the window here, which is what makes
    // this test reject rather than merely observe.
    let before = app
        .renderer()
        .game_viewport(app.world_state())
        .expect("the viewport is installed");
    let box_before = app.ui().unwrap().ui.screen_box(sbox);
    assert_eq!(
        (before.x, before.y, before.width, before.height),
        (
            u32::try_from(box_before.x0).unwrap(),
            u32::try_from(box_before.y0).unwrap(),
            u32::try_from(box_before.width()).unwrap(),
            u32::try_from(box_before.height()).unwrap()
        ),
        "the installed rect is <SBOX>'s screen origin and dimensions"
    );

    // Resize and move the smart box as when dragging its border. In the original path both
    // operations update virtual screen position, which triggers viewport recalculation.
    {
        let shell = app.ui_mut().unwrap();
        shell.ui.resize_to(sbox, 300, 200);
        shell.ui.move_to(sbox, 40, 30);
    }
    assert!(app.frame());

    let after = app
        .renderer()
        .game_viewport(app.world_state())
        .expect("still installed");
    let box_after = app.ui().unwrap().ui.screen_box(sbox);
    assert_ne!(box_before, box_after, "the element itself must have moved");
    assert_ne!(before, after, "and the viewport must have moved with it");
    // The rectangle is read back from the element rather than written here, because `resize_to`
    // applies the original region's minimum-size clamp and the resulting dimensions are the layout's,
    // not this test's. What is asserted is the *identity*: whatever <SBOX> ended up as, that is
    // the viewport.
    assert_eq!(
        (after.x, after.y, after.width, after.height),
        (
            u32::try_from(box_after.x0).unwrap(),
            u32::try_from(box_after.y0).unwrap(),
            u32::try_from(box_after.width()).unwrap(),
            u32::try_from(box_after.height()).unwrap()
        ),
        "the viewport is exactly the element's new box"
    );

    // The control: a build that always set the viewport to the window passes every assertion
    // above except this one.
    let (w, h) = app.renderer().size();
    assert_ne!(
        after,
        Viewport {
            x: 0,
            y: 0,
            width: w,
            height: h
        },
        "the viewport must not simply be the whole window"
    );
    assert!(
        after.width < w || after.height < h,
        "a resized <SBOX> is smaller than the back buffer, so the viewport must be too"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The controls.
// ---------------------------------------------------------------------------------------------

/// **The pixels the inset run leaves untouched really are painted when the viewport is the
/// window.** The control for [`the_scene_renders_inside_the_world_view_rectangle`].
///
/// Without this, "outside the rect matches the clear" passes for a scene that draws nothing, for a
/// device that failed silently, and for a camera pointing at the sky -- and a build that always
/// set the viewport to the whole window would then pass the rejecting test above by drawing
/// nothing at all. This is the assertion that fails in that case.
#[test]
fn the_scene_does_paint_outside_the_rect_when_the_viewport_is_the_window() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let cleared = {
        gpu.begin_frame().expect("begin");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    };
    let full = frame(&store, &mut gpu, None);
    let painted = outside_points()
        .into_iter()
        .filter(|&(x, y)| px(&full, x, y) != px(&cleared, x, y))
        .count();
    assert!(
        painted > 0,
        "with no viewport installed the scene must paint at points outside {INSET:?} -- otherwise
         the rejecting test's 'outside is untouched' is vacuous"
    );
}

/// **The projection's aspect is the viewport's, not the window's.**
///
/// Original viewport installation computes aspect as `(w / h) * display_aspect * 0.75` when
/// raw-aspect mode is disabled. The **display's** aspect comes from the 4:3 or 16:9 preference,
/// falling back to the back-buffer ratio. The two rectangles are different,
/// and only one of them is the viewport.
///
/// This forbids the half-fix: set the scissor, leave the projection alone. That clips the view
/// instead of reframing it, which is a different picture and the wrong one.
#[test]
fn the_aspect_is_the_viewports_and_not_the_windows() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");

    let full = scene.view_params(W, H);
    let window = full.aspect;

    // A 2:1 rectangle inside a 4:3 window. If the aspect followed the window these would be equal.
    scene.set_game_viewport(Some(Viewport {
        x: 0,
        y: 0,
        width: 400,
        height: 200,
    }));
    let wide = scene.view_params(W, H);
    assert_ne!(
        wide.aspect, window,
        "a 2:1 viewport in a 4:3 window is not the window's aspect"
    );
    assert_eq!(
        wide.viewport,
        Viewport {
            x: 0,
            y: 0,
            width: 400,
            height: 200
        }
    );

    // `fov_y_from_preference` preserves the original `pref / (aspect - 0.1)` calculation.
    // The projection, not merely the scissor, therefore follows the rectangle.
    assert_ne!(
        wide.fov_y_rad, full.fov_y_rad,
        "the FOV is derived from the aspect, not fixed"
    );

    // A rectangle with the window's own ratio gives the window's own aspect: the rule is the
    // ratio, not "any viewport at all changes it".
    scene.set_game_viewport(Some(Viewport {
        x: 17,
        y: 23,
        width: W / 2,
        height: H / 2,
    }));
    let same = scene.view_params(W, H);
    assert!(
        (same.aspect - window).abs() < 1e-5,
        "a half-size rectangle of the same shape keeps the aspect ({} vs {window})",
        same.aspect
    );
    // ...and the rectangle itself still moved, so the assertion above is about the ratio and not
    // about the viewport having been ignored.
    assert_eq!(
        same.viewport,
        Viewport {
            x: 17,
            y: 23,
            width: W / 2,
            height: H / 2
        }
    );

    // `None` restores the whole back buffer, matching the original undocked viewport result.
    scene.set_game_viewport(None);
    assert_eq!(
        scene.view_params(W, H).viewport,
        Viewport {
            x: 0,
            y: 0,
            width: W,
            height: H
        }
    );
    assert_eq!(scene.view_params(W, H).aspect, window);
}
