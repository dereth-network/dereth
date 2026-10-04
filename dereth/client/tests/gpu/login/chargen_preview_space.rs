//! The `CreatureMode` preview space, shared by the char-gen appearance and summary pages and the
//! teleport tunnel's portal swirl. The portal ids resolve as enum keys `0x10000001` and
//! `0x10000002` through group **7, `UIASSET`** (`portalspace_background`, `portalspace_animation`);
//! the sequence frame counter is driven by elapsed seconds, so the same duration reaches the same
//! frame at any frame rate; a preview draws only inside its viewport; and the preview's own
//! controls (rotate arrows, zoom, gender) change the camera, heading and setup it is built from.
//! Fixture: the retail dats, bare preview spaces on a WARP renderer, and offline headless `App`s
//! (char-gen wizard, or Holtburg with the gameplay UI) captured in paired frames with the space's
//! object present and absent; no link, FINISH never pressed.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use std::sync::Arc;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::gpu::PreviewId;
use dereth_client::preview::{
    ENUM_PORTALSPACE_ANIMATION, ENUM_PORTALSPACE_BACKGROUND, UIASSET_GROUP,
};
use dereth_primitives::{AssetSource as _, DataId};
use dereth_ui::framework::mode;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::chargen::{appearance, CharGenScreen, EcgProgress};
use dereth_ui_screens::screens::teleport::{in_exit_window, portal_space, timing};

/// Every fixture path is an `expect`: a missing dat or device fails the test.
fn store() -> Arc<dereth_dat::RetailDatStore> {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    Arc::new(dereth_client::assets::open_data_files(&d).expect("the retail dats open"))
}

/// Broadcast a button-click message as `(element, 1, 7, 0)`.
fn click(app: &mut App, id: u32) {
    let shell = app.ui_mut().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("element {id:#010X} is in the shipped layout"));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

// =================================================================================================
// 1. The two ids
// =================================================================================================

/// Oracle: the two enum-key lookups portal-preview initialization makes, and the shipped
/// `DidMapper` chain `0x25000000` -> group 7 -> `0x25000010`.
///
/// The values are asserted because they pin the *group*: 7 is the only one of the thirteen whose
/// `0x10000001` is a setup record (`0x02` id space) and whose `0x10000002` is an animation
/// (`0x03`), which is what object creation and sequence-animation setup consume. Group 5 answers
/// `0x21000000`, a `LayoutDesc`; group 9 answers a `Font`. Getting the group wrong makes the
/// portal space quietly draw nothing.
#[test]
fn the_portal_space_ids_resolve_through_the_uiasset_group() {
    let store = store();
    let assets: &dyn dereth_primitives::AssetSource = &*store;

    let obj = dereth_client::assets::enum_did(assets, UIASSET_GROUP, ENUM_PORTALSPACE_BACKGROUND)
        .expect("UIASSET 0x10000001 (portalspace_background) resolves");
    let anim = dereth_client::assets::enum_did(assets, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
        .expect("UIASSET 0x10000002 (portalspace_animation) resolves");

    assert_eq!(
        obj,
        DataId(0x0200_0306),
        "portalspace_background is a setup record"
    );
    assert_eq!(
        anim,
        DataId(0x0300_05AC),
        "portalspace_animation is an Animation"
    );
    assert_eq!(obj.0 >> 24, 0x02, "object creation wants a setup record");
    assert_eq!(
        anim.0 >> 24,
        0x03,
        "set_sequence_animation wants an Animation"
    );
    assert!(
        store.exists(obj) && store.exists(anim),
        "both are in this dat build"
    );

    // The paper-doll and char-gen animation enums resolve too, and to the right id spaces.
    for (name, e, space) in [
        (
            "PaperDollAnimation",
            dereth_client::preview::ENUM_PAPERDOLL_ANIMATION,
            0x03,
        ),
        (
            "CharGenAnimation",
            dereth_client::preview::ENUM_CHARGEN_ANIMATION,
            0x03,
        ),
    ] {
        let id = dereth_client::assets::enum_did(assets, UIASSET_GROUP, e)
            .unwrap_or_else(|| panic!("UIASSET {e:#010X} ({name}) resolves"));
        assert_eq!(id.0 >> 24, space, "{name} is in the {space:#04X} id space");
    }
}

/// Oracle: portal initialization's camera-position and light-direction arguments, with the same
/// camera reapplied by its per-frame update.
///
/// The z is asserted as its **bit pattern** as well as its value: the update stores `0x3F6147AE`,
/// which is 0.88, not 0.87.
#[test]
fn the_portal_camera_constants_are_the_floats_in_post_init() {
    assert_eq!(portal_space::CAMERA_POSITION, (0.24, -2.7, 0.88));
    assert_eq!(
        portal_space::CAMERA_POSITION.2.to_bits(),
        0x3F61_47AE,
        "the camera-height bit pattern is 0x3F6147AE, which is 0.88 and not 0.87"
    );
    assert_eq!(portal_space::LIGHT_DIRECTION, (0.3, -1.9, 0.65));
    assert_eq!(portal_space::LIGHT_INTENSITY, 2.0);
    // The light added is `(DISTANT_LIGHT, 2.0)`; DISTANT_LIGHT is 1 in its enum.
    assert_eq!(
        dereth_world_render::lighting::LightType::Directional as u32,
        1
    );
}

// =================================================================================================
// 2. The frame counter
// =================================================================================================

/// A preview space holding the portal object with its sequence started, exactly as portal
/// initialization and its per-frame update build it.
fn portal_renderer(store: &Arc<dereth_dat::RetailDatStore>) -> dereth_client::gpu::Renderer {
    let mut r = dereth_client::gpu::Renderer::new(None, 64, 64).expect("a WARP device comes up");
    let assets = Arc::new(dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(
        store,
    )));
    assert!(
        r.ensure_preview(PreviewId::Portal, &assets),
        "the space is created once"
    );
    let a: &dyn dereth_primitives::AssetSource = &**store;
    let obj = dereth_client::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_BACKGROUND)
        .expect("portalspace_background resolves");
    let i = r
        .add_preview_object(PreviewId::Portal, store, obj)
        .expect("the object is built")
        .expect("the setup loads");
    assert_eq!(i, 0, "the first appended preview object has index zero");
    r
}

/// Behaviour: chargen.preview.the-portal-space-plays-at-forty-frames-a-second-inside-its-viewport
///
/// Oracle: the portal update starts the resolved `0x10000002` animation on object zero at
/// low frame 1, clearing prior animations and running at 40 frames per second; the current frame
/// reader supplies the observation.
///
/// The assertion is against **elapsed seconds**, not against a frame count: the sequence update
/// advances `frame_number` by `framerate * dt`.
#[test]
fn the_portal_sequence_advances_at_forty_frames_a_second_of_elapsed_time() {
    let store = store();
    let mut r = portal_renderer(&store);
    let a: &dyn dereth_primitives::AssetSource = &*store;
    let anim = dereth_client::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
        .expect("portalspace_animation resolves");

    let space = r
        .preview_mut(PreviewId::Portal)
        .expect("the space is there");
    // `low_frame = 1`, `framerate = 40.0`, `clear = 1` -- the portal update's arguments.
    assert!(
        space.set_sequence_animation(0, anim, true, 1, timing::PORTAL_FRAMERATE as f32),
        "the sequence really started -- an animation the dat does not hold is dropped silently"
    );
    assert_eq!(
        space.curr_frame_number(0),
        1,
        "set_sequence_animation starts at low_frame"
    );

    // Half a second of elapsed time is twenty frames at 40 fps, from frame 1.
    for _ in 0..30 {
        space.use_time(0.5 / 30.0);
    }
    let after_half = space.curr_frame_number(0);
    assert_eq!(
        after_half, 21,
        "1 + 0.5 s * 40 fps = frame 21, got {after_half}"
    );
}

/// **Frame-rate independence.**
///
/// The same wall-clock duration, stepped at 30 Hz and at 144 Hz, must reach the same animation
/// frame. A player that accumulated a per-frame increment instead would drift.
#[test]
fn the_same_duration_at_two_frame_rates_reaches_the_same_frame() {
    let store = store();
    let a: &dyn dereth_primitives::AssetSource = &*store;
    let anim = dereth_client::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
        .expect("portalspace_animation resolves");

    let run = |steps: u32, seconds: f64| {
        let mut r = portal_renderer(&store);
        let space = r
            .preview_mut(PreviewId::Portal)
            .expect("the space is there");
        assert!(space.set_sequence_animation(0, anim, true, 1, timing::PORTAL_FRAMERATE as f32));
        let dt = seconds / f64::from(steps);
        for _ in 0..steps {
            space.use_time(dt);
        }
        space.curr_frame_number(0)
    };

    // 2.5 seconds is a hundred frames at 40 fps -- past the animation's own length, so the wrap is
    // exercised as well as the advance.
    let slow = run(75, 2.5); // 30 Hz
    let fast = run(360, 2.5); // 144 Hz
    assert_eq!(
        slow, fast,
        "2.5 s of elapsed time is 2.5 s of animation at any frame rate"
    );
    // ... and it really moved, so an accumulator stuck at its start frame cannot pass.
    assert!(slow > 1, "the sequence advanced past its low frame: {slow}");
}

/// Oracle: the portal update's tunnel-continue exit opens when
/// `(0x78 - current_animation_frame) / 40.0` is in `(1.1, 1.3)`.
///
/// **This is the test that fails when the mechanism under it is removed.** A space whose sequence
/// animation was never started reports frame 0 for ever, `(120 - 0)/40 = 3.0` is outside the
/// window, and the tunnel can only ever leave on its five-second cap. Delete the
/// `set_sequence_animation` call below and the second half of this test fails.
#[test]
fn the_exit_window_is_reachable_only_once_the_sequence_animation_has_started() {
    let store = store();
    let a: &dyn dereth_primitives::AssetSource = &*store;
    let anim = dereth_client::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
        .expect("portalspace_animation resolves");

    // Without the sequence: the counter never leaves 0 and the window is never open.
    let mut r = portal_renderer(&store);
    {
        let space = r
            .preview_mut(PreviewId::Portal)
            .expect("the space is there");
        space.clear_sequence_anims(0);
        let mut opened = false;
        for _ in 0..400 {
            space.use_time(0.025);
            opened |= in_exit_window(space.curr_frame_number(0));
        }
        assert_eq!(space.curr_frame_number(0), 0, "no animation, no frames");
        assert!(
            !opened,
            "the exit window never opens without a sequence animation"
        );
    }

    // With it: the window opens, and it opens where `(120 - frame)/40` says it should.
    let space = r
        .preview_mut(PreviewId::Portal)
        .expect("the space is there");
    assert!(space.set_sequence_animation(0, anim, true, 1, timing::PORTAL_FRAMERATE as f32));
    let mut opened_at = Vec::new();
    for _ in 0..400 {
        space.use_time(0.025);
        let f = space.curr_frame_number(0);
        if in_exit_window(f) {
            opened_at.push(f);
        }
    }
    assert!(
        !opened_at.is_empty(),
        "the exit window opens once the sequence is running"
    );
    for f in &opened_at {
        let remaining = (timing::PORTAL_EXIT_FRAME - f) as f32 / timing::PORTAL_FRAMERATE as f32;
        assert!(
            remaining > timing::PORTAL_EXIT_WINDOW.0 && remaining < timing::PORTAL_EXIT_WINDOW.1,
            "frame {f} claims to be in the window at {remaining} s remaining"
        );
    }
}

// =================================================================================================
// 3. The pixels
// =================================================================================================

/// The wizard, offline and headless, parked on a page whose viewport is showing.
fn wizard_on(page: EcgProgress) -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(mode::CHAR_GEN);
    for _ in 0..8 {
        app.frame();
    }
    // A heritage and a town, so the state names a real setup: reset leaves the heritage group at
    // zero, and the character-preview update does no work until
    // `if (heritage != 0 && gender != 0)`.
    click(
        &mut app,
        dereth_ui_screens::screens::chargen::HERITAGE_BUTTONS[0]
            .0
             .0,
    );
    for _ in 0..2 {
        app.frame();
    }
    click(
        &mut app,
        dereth_ui_screens::screens::chargen::TOWN_BUTTONS[0].0 .0,
    );
    for _ in 0..2 {
        app.frame();
    }
    click(
        &mut app,
        page.select_button().expect("a real page has a tab").0,
    );
    for _ in 0..4 {
        app.frame();
    }
    app
}

/// The viewport element's screen box, as the host reads it.
fn viewport_box(app: &mut App, id: ElementId) -> dereth_ui::Box2D {
    let shell = app.ui_mut().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, id)
        .expect("the viewport is in the layout");
    shell.ui.screen_clip_box(h)
}

/// **The paired frame, inside one process.**
///
/// Viewport rendering runs only when the space holds an object, so emptying the
/// space is exactly what the client does on a page with no preview -- and the difference between
/// the two frames is the pass, with nothing else changed. The acceptance criterion is that the
/// changed pixels are **confined to the viewport**: a preview that leaked outside its rectangle,
/// or that clobbered the panel art by clearing colour, would fail here.
#[test]
fn the_preview_draws_inside_its_viewport_and_nowhere_else() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let area = viewport_box(&mut app, appearance::VIEWPORT);
    assert!(area.is_valid(), "the appearance page's viewport has a box");

    app.frame();
    let (w, h, with) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    let drawn = app.renderer_mut().ui_stats.previews_drawn;
    assert!(drawn > 0, "the preview pass ran at least once");

    // The same frame with the space emptied. `preview_use_time` rebuilds only when the *setup*
    // changes, so this stays empty and the panel is left exactly as the UI blitted it.
    app.renderer_mut()
        .preview_mut(PreviewId::CharGen)
        .expect("the space exists")
        .remove_all_objects();
    app.frame();
    let (w2, h2, without) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    assert_eq!((w, h), (w2, h2));
    assert!(
        app.renderer_mut().ui_stats.previews_empty > 0,
        "an empty preview space must be counted as declined rather than drawn"
    );

    let mut changed = 0u32;
    let mut outside = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if with[i..i + 3] != without[i..i + 3] {
                changed += 1;
                let (px, py) = (x as i32, y as i32);
                if px < area.x0 || px > area.x1 || py < area.y0 || py > area.y1 {
                    outside += 1;
                }
            }
        }
    }
    let box_area = ((area.x1 - area.x0 + 1) * (area.y1 - area.y0 + 1)) as u32;
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the viewport"
    );
    assert!(
        changed > box_area / 20,
        "the preview covers a real part of the panel: {changed} of {box_area} pixels"
    );
    app.shutdown();
}

/// Physics-part drawing selects material the same way
/// whether its part array belongs to the world or a preview scene. Exercise that common rule on
/// the wizard's actual preview object:
/// opaque, a half-transparent material clone, full `NoDraw`, then restored.
#[test]
fn preview_part_translucency_reaches_the_draw_and_preserves_the_panel() {
    struct Arm {
        frame: Vec<u8>,
        pose: Vec<dereth_primitives::Frame>,
        materials: usize,
        nodraw: usize,
    }

    fn arm(app: &mut App, t: f32) -> Arm {
        {
            let space = app
                .renderer_mut()
                .preview_mut(PreviewId::CharGen)
                .expect("the character preview exists");
            space
                .part_array_mut(0)
                .expect("the heritage body is preview object zero")
                .set_translucency_internal(t, false);
        }
        app.frame();
        let (frame, pose, materials, nodraw) = {
            let renderer = app.renderer_mut();
            let (_, _, frame) = renderer.capture_bgra().expect("the preview frame captures");
            let object = renderer
                .preview(PreviewId::CharGen)
                .expect("the character preview remains")
                .object(0)
                .expect("the heritage body remains");
            let pose = object.part_array.parts.iter().map(|p| p.pos).collect();
            let materials = object
                .part_array
                .parts
                .iter()
                .filter(|p| p.material.is_some())
                .count();
            let nodraw = object
                .part_array
                .parts
                .iter()
                .filter(|p| p.no_draw())
                .count();
            (frame, pose, materials, nodraw)
        };
        Arm {
            frame,
            pose,
            materials,
            nodraw,
        }
    }

    let mut app = wizard_on(EcgProgress::Appearance);
    let area = viewport_box(&mut app, appearance::VIEWPORT);
    let opaque = arm(&mut app, 0.0);
    let half = arm(&mut app, 0.5);
    let gone = arm(&mut app, 1.0);
    let restored = arm(&mut app, 0.0);

    assert_eq!(
        opaque.pose, half.pose,
        "the preview moved between opaque and half alpha"
    );
    assert_eq!(
        opaque.pose, gone.pose,
        "the preview moved before the NoDraw control"
    );
    assert_eq!(
        opaque.pose, restored.pose,
        "the preview moved before restoration"
    );
    assert_eq!(
        opaque.materials, 0,
        "the opaque body starts with a cloned material"
    );
    assert!(
        half.materials > 10,
        "only {} preview parts cloned a material",
        half.materials
    );
    assert_eq!(
        gone.nodraw,
        opaque.pose.len(),
        "full translucency did not hide every body part"
    );
    assert_eq!(
        restored.materials, 0,
        "restoring t=0 left cloned preview materials"
    );
    assert_eq!(
        restored.nodraw, 0,
        "restoring t=0 left preview parts NoDraw"
    );
    assert_eq!(
        opaque.frame, restored.frame,
        "the opaque preview did not return byte-for-byte"
    );

    let mut silhouette = 0usize;
    let mut half_changed = 0usize;
    let mut outside_silhouette = 0usize;
    let mut outside_viewport = 0usize;
    for (i, ((o, h), g)) in opaque
        .frame
        .as_chunks::<4>()
        .0
        .iter()
        .zip(half.frame.as_chunks::<4>().0)
        .zip(gone.frame.as_chunks::<4>().0)
        .enumerate()
    {
        let body = o[..3] != g[..3];
        let changed = o[..3] != h[..3];
        silhouette += usize::from(body);
        half_changed += usize::from(changed);
        outside_silhouette += usize::from(changed && !body);
        let x = (i % 800) as i32;
        let y = (i / 800) as i32;
        outside_viewport +=
            usize::from(changed && (x < area.x0 || x > area.x1 || y < area.y0 || y > area.y1));
    }
    eprintln!(
        "preview translucency: {} cloned parts; silhouette {silhouette} px, half alpha moved \
         {half_changed} px ({outside_silhouette} outside the silhouette, {outside_viewport} \
         outside the viewport)",
        half.materials,
    );
    assert!(
        silhouette > 500,
        "the full-NoDraw control sees only {silhouette} body pixels"
    );
    assert!(
        half_changed > 200,
        "the half-transparent material moved only {half_changed} pixels"
    );
    assert_eq!(
        outside_silhouette, 0,
        "half alpha changed the preview background"
    );
    assert_eq!(
        outside_viewport, 0,
        "half alpha escaped the preview viewport"
    );
    app.shutdown();
}

/// The summary page is the second consumer of the **same** space: both pages draw through one
/// preview space, not two.
#[test]
fn the_summary_page_draws_through_the_same_space() {
    let mut app = wizard_on(EcgProgress::Summary);
    let area = viewport_box(
        &mut app,
        dereth_ui_screens::screens::chargen::SUMMARY_VIEWPORT,
    );
    assert!(area.is_valid(), "the summary page's viewport has a box");
    app.frame();
    assert!(
        app.renderer_mut().ui_stats.previews_drawn > 0,
        "the summary preview drew"
    );
    let space = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the same space");
    // **Two** objects, not one per page: the heritage model and the background room it
    // stands in.
    assert_eq!(
        space.object_count(),
        2,
        "the model and its background, not one set per page"
    );
    let o = space.object(0).expect("the heritage model");
    assert!(
        o.drawn_parts() > 10,
        "a body is more than a handful of parts: {}",
        o.drawn_parts()
    );
    // Summary-page initialization ends by starting animation, so this one is running.
    let before = space.object(0).expect("the model").setup;
    app.frame();
    let space = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the same space");
    assert_eq!(
        space.object(0).expect("the model").setup,
        before,
        "no rebuild between frames"
    );
    app.shutdown();
}

/// The wizard's own 3D-view state, checked against both pages' initialization rather than
/// against what the preview happens to render.
#[test]
fn the_wizard_asks_for_the_camera_and_heading_both_post_inits_write() {
    let mut app = wizard_on(EcgProgress::Summary);
    let shell = app.ui_mut().expect("the shell is up");
    let screen = shell.flow.current().expect("a screen is up");
    let any: &dyn std::any::Any = screen;
    let w = any
        .downcast_ref::<CharGenScreen>()
        .expect("the wizard is up");
    // Summary-page initialization sets camera `(0, -2.5, 0.95)` toward `(0, 0, 0)`,
    // sets player heading 180, and starts animation.
    assert_eq!(w.view3d.camera_position, [0.0, -2.5, 0.95]);
    assert_eq!(w.view3d.camera_direction, [0.0, 0.0, 0.0]);
    assert_eq!(
        w.view3d.heading, 180.0,
        "a heading of 0 shows the model's back"
    );
    assert!(w.view3d.animating, "the summary page starts the animation");
    app.shutdown();
}

// =================================================================================================
// 4. The portal swirl -- the teleport tunnel's consumer of the preview mechanism
// =================================================================================================

/// The application with Holtburg loaded and the gameplay UI up, which is the state a player is in
/// when a portal fires.
fn app_in_world() -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("Holtburg loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    app
}

/// **The tunnel is a swirl and not a hidden world.**
///
/// The paired frame is the same tunnel with the space's object present and absent -- viewport
/// rendering draws only when object zero exists -- so the difference is the swirl and nothing
/// else.
#[test]
fn the_teleport_tunnel_draws_the_portal_object() {
    let mut app = app_in_world();
    // The player object's existence is the condition for the login tunnel;
    // `Teleport::teleport_in_progress()` is the corresponding predicate.
    app.probe_mut().teleport_mut().apply_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerCreated(
            dereth_primitives::ObjectId(0x5000_0001),
        ),
    ]);
    // Into `TAS_TUNNEL` and a little way past the first fade, so the projection is open again.
    for _ in 0..40 {
        app.frame();
    }
    assert!(app.teleport().world_hidden(), "the tunnel hides the world");
    assert!(
        app.teleport().anim.state.is_tunnel(),
        "still in the tunnel: {:?}",
        app.teleport().anim.state
    );
    let drawn = app.renderer_mut().ui_stats.previews_drawn;
    assert!(drawn > 0, "the portal space drew at least once");

    // The frame counter is the real animation sequence's, and it moves.
    let f0 = app
        .teleport()
        .portal_anim_frame
        .expect("the space reports a real frame number");
    for _ in 0..10 {
        app.frame();
    }
    let f1 = app.teleport().portal_anim_frame.expect("still reporting");
    assert_ne!(
        f0, f1,
        "the portal sequence is running, not parked on one frame"
    );

    let (w, h, with) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    // The same tunnel with nothing in the preview space: viewport rendering declines it.
    app.renderer_mut()
        .preview_mut(PreviewId::Portal)
        .expect("the space")
        .remove_all_objects();
    app.frame();
    let (w2, h2, without) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame captures");
    assert_eq!((w, h), (w2, h2));

    let changed = with
        .as_chunks::<4>()
        .0
        .iter()
        .zip(without.as_chunks::<4>().0)
        .filter(|(a, b)| a[0..3] != b[0..3])
        .count();
    assert!(
        changed > 500,
        "the swirl is on the screen: only {changed} pixels differ between a tunnel with the \
         portal object and one without"
    );
    eprintln!("portal swirl: the tunnel frame differs from an empty one in {changed} pixels");
    app.shutdown();
}

/// The portal space's own constants reach it, and it is the space portal initialization describes
/// rather than the char-gen one.
#[test]
fn the_portal_space_is_built_the_way_post_init_builds_it() {
    let mut app = app_in_world();
    app.probe_mut().teleport_mut().apply_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerCreated(
            dereth_primitives::ObjectId(0x5000_0001),
        ),
    ]);
    for _ in 0..20 {
        app.frame();
    }
    let space = app
        .renderer_mut()
        .preview(PreviewId::Portal)
        .expect("the portal space exists");
    assert_eq!(space.object_count(), 1, "one object: the portal object");
    assert!(
        space.mode.use_world_fov,
        "the world field of view is what makes the tunnel's projection collapse reach the swirl"
    );
    assert!(
        !space.mode.use_sharp_mode,
        "sharp mode belongs to the character-generation 3D view, not this preview"
    );
    assert_eq!(space.mode.lights.len(), 1);
    let l = space.mode.lights[0];
    assert_eq!(l.intensity, portal_space::LIGHT_INTENSITY);
    assert_eq!(
        (l.direction.x, l.direction.y, l.direction.z),
        portal_space::LIGHT_DIRECTION,
        "the portal light points at negative y; the character-generation 3D view's points at positive y"
    );
    let o = space.mode.view_frame.origin;
    assert_eq!((o.x, o.y, o.z), portal_space::CAMERA_POSITION);
    // The two spaces are distinct: one *mechanism*, one instance per viewport widget.
    assert!(
        app.renderer_mut().preview(PreviewId::CharGen).is_none(),
        "the char-gen space is not built by a login"
    );
    app.shutdown();
}

// =================================================================================================
// 5. The preview's own controls
// =================================================================================================

/// Read the wizard's 3D-view state.
fn view3d(app: &mut App) -> dereth_ui_screens::screens::chargen::Cg3dView {
    let shell = app.ui_mut().expect("the shell is up");
    let screen = shell.flow.current().expect("a screen is up");
    let any: &dyn std::any::Any = screen;
    any.downcast_ref::<CharGenScreen>()
        .expect("the wizard is up")
        .view3d
        .clone()
}

/// **The rotate arrows turn the model.**
///
/// Oracle: the appearance page's message-3 arm rotates only while its rotating flag is
/// set, using `delta = dt / rotation_period * 360`.
///
/// The per-frame tick is what moves the heading while the rotating flag is set. Delete the
/// `tick_preview` call in `App::preview_use_time` and this test fails.
#[test]
fn the_rotate_arrows_turn_the_model_and_the_second_press_stops_it() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let start = view3d(&mut app).heading;
    assert_eq!(
        start, 180.0,
        "the page opens at the character-appearance page's post-init heading"
    );

    click(&mut app, appearance::ROTATE_CW.0);
    app.frame();
    assert!(view3d(&mut app).rotating, "the arrow armed the turntable");
    for _ in 0..10 {
        app.frame();
    }
    let turned = view3d(&mut app).heading;
    assert_ne!(
        turned, start,
        "ten frames of a running turntable moved the heading"
    );

    // Rotation stops when the already-spinning arrow is pressed again.
    click(&mut app, appearance::ROTATE_CW.0);
    app.frame();
    assert!(!view3d(&mut app).rotating, "the second press stopped it");
    let stopped = view3d(&mut app).heading;
    for _ in 0..10 {
        app.frame();
    }
    assert_eq!(
        view3d(&mut app).heading,
        stopped,
        "a stopped turntable does not drift"
    );

    // And the other arrow reverses rather than stopping.
    click(&mut app, appearance::ROTATE_CCW.0);
    for _ in 0..10 {
        app.frame();
    }
    let back = view3d(&mut app).heading;
    assert_ne!(back, stopped, "the counter-clockwise arrow turns it too");
    app.shutdown();
}

/// **The Face and Clothes tabs set the zoom, and the + / - buttons agree with them.**
///
/// Oracle: the appearance-choice handler zooms in for Face and out for Clothes, using
/// each heritage's corresponding target distances.
///
/// The page **opens on Face**, so it opens zoomed in: the retail frame of this page is a head,
/// not a whole body. The tab moves the camera as well as the radios and the two choice groups.
#[test]
fn the_face_tab_zooms_in_and_the_clothes_tab_zooms_out() {
    use dereth_ui_screens::screens::chargen::{zoomed_in_camera, zoomed_out_camera};
    let mut app = wizard_on(EcgProgress::Appearance);
    let v = view3d(&mut app);
    assert!(v.zoomed_in, "choosing the face page ends in a zoom-in");
    assert_eq!(
        v.camera_position,
        zoomed_in_camera(1),
        "Aluvian's zoom-in camera"
    );

    click(&mut app, appearance::TAB_CLOTHES.0);
    for _ in 0..2 {
        app.frame();
    }
    let v = view3d(&mut app);
    assert!(!v.zoomed_in, "choosing the clothes page ends in a zoom-out");
    assert_eq!(v.camera_position, zoomed_out_camera(1));

    click(&mut app, appearance::TAB_FACE.0);
    for _ in 0..2 {
        app.frame();
    }
    let v = view3d(&mut app);
    assert!(v.zoomed_in, "and back");
    assert_eq!(v.camera_position, zoomed_in_camera(1));

    // The `-` button agrees with the tab rather than fighting it.
    click(&mut app, appearance::ZOOM_OUT.0);
    for _ in 0..2 {
        app.frame();
    }
    assert!(
        !view3d(&mut app).zoomed_in,
        "the minus button zooms out from the Face tab"
    );
    app.shutdown();
}

/// **Choosing a gender changes the preview's body.**
///
/// Oracle: setup selection reads the selected heritage and gender's setup (or the hair style's
/// alternate setup) whenever both are chosen, and the stored setup id only when one of them is 0.
///
/// The heritage group's generic setup field is `0x02000054` -- the generic human -- for all
/// thirteen heritages; the shipped table holds **18 distinct** sex-specific setups.
#[test]
fn choosing_a_gender_changes_the_setup_the_preview_is_built_from() {
    let mut app = wizard_on(EcgProgress::Appearance);
    let male = view3d(&mut app).setup;
    assert_eq!(
        male,
        DataId(0x0200_0001),
        "Aluvian male is the gender's setup, not the heritage's"
    );
    assert_ne!(
        male,
        DataId(0x0200_0054),
        "0x02000054 is the heritage group's generic setup for every one"
    );
    assert_eq!(
        app.renderer_mut()
            .preview(PreviewId::CharGen)
            .expect("the space")
            .object(0)
            .expect("a model")
            .setup,
        male,
        "the drawn object is the one the state names"
    );

    click(&mut app, appearance::GENDER_FEMALE.0);
    for _ in 0..3 {
        app.frame();
    }
    let female = view3d(&mut app).setup;
    assert_eq!(female, DataId(0x0200_004E), "Aluvian female");
    assert_ne!(female, male, "the model is a different body");
    let space = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space");
    assert_eq!(
        space.object_count(),
        2,
        "the old model was removed, not stacked on -- two objects is the model plus the \
         background object, and a third would mean the rebuild leaked one"
    );
    assert_eq!(
        space.object(0).expect("a model").setup,
        female,
        "the character-generation 3D view update rebuilt it when the requested setup differed"
    );

    click(&mut app, appearance::GENDER_MALE.0);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(view3d(&mut app).setup, male, "and back");
    app.shutdown();
}
