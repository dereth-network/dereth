//! The App frame draws the shipped target-indicator brackets around a selected world creature or
//! corpse, sized from the object's selection sphere, and clears them on deselection; ownership,
//! the indicator option, destruction and off-screen targets leave no brackets; a gameplay screen
//! rebuilt at another display size reprojects the corners; the brackets follow the world fade and
//! hide during the portal tunnel. Fixture: a headless `App` with the retail dats' gameplay layout
//! and world, objects created from synthetic object-create messages.
#![cfg(gpu)]

use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{DataId, LocalTime, ObjectId, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use dereth_scene::world_scene::SceneReads;
use dereth_ui::{framework::mode, ElemHandle, ElementId};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use {
    dereth_client::app::App, dereth_client_runtime::config::Config,
    dereth_client_runtime::scene::SceneConfig,
};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

const CREATURE: ObjectId = ObjectId(0x8300_0F01);
const CORPSE: ObjectId = ObjectId(0x8300_0F02);

fn setup() -> App {
    setup_at((800, 600))
}

fn setup_at(size: (u32, u32)) -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: size.0,
        height: size.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("retail DATs and headless device");
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        character: false,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("DAT world");
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .apply_player_module(&dereth_protocol::login::PlayerModule::default());
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .set_option(14, true, dereth_primitives::ServerTime(0.0));
    for _ in 0..3 {
        assert!(app.frame());
    }
    app
}

fn handle(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().unwrap();
    let any: &dyn std::any::Any = shell.flow.current().unwrap();
    let screen = any.downcast_ref::<GamePlayScreen>().unwrap();
    shell
        .ui
        .get_child_recursive(screen.root().unwrap(), id)
        .expect("shipped target element")
}

fn place(app: &mut App, id: ObjectId, camera_offset: Vec3, corpse: bool) {
    let scene = app.world_scene().unwrap();
    let camera = scene.camera.frame();
    let origin = dereth_physics::math::localtoglobal(&camera, camera_offset);
    let block = scene.viewer_block().unwrap();
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            // The retail Aluvian setup carries a real selection sphere. Corpse appearance
            // uses the same body setup; the weenie's type/flags change, not the bracket API.
            setup_id: Some(0x0200_0001),
            state: 0,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: ((block.0 as u32) << 24) | ((block.1 as u32) << 16) | 1,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc {
            obj_type: if corpse {
                item_type::CONTAINER
            } else {
                item_type::CREATURE
            },
            bitfield: if corpse {
                bitfield::CORPSE
            } else {
                bitfield::ATTACKABLE
            },
            ..PublicWeenieDesc::default()
        },
    };
    let body =
        dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload)).unwrap();
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(1.0),
    );
}

fn select(app: &mut App, id: Option<ObjectId>) {
    app.probe_mut().objects_mut().world.set_selected_object(
        id,
        false,
        &mut dereth_client_model::NullSink,
    );
}

fn assert_hidden(app: &App) {
    for id in [window::TARGET_ON_SCREEN, window::TARGET_OFF_SCREEN] {
        let h = handle(app, id);
        assert!(!app.ui().unwrap().ui.node(h).unwrap().region.flags.visible);
        assert!(!app.ui_draw_list().iter().any(|c| c.who == h));
    }
    for index in 1..=4 {
        let h = handle(app, ElementId(0x1000_0038 + index));
        assert!(
            !app.ui_draw_list().iter().any(|c| c.who == h),
            "no stale corner command"
        );
    }
}

/// Independent camera-space sphere oracle, not target_projection/on_screen_box. These
/// stations are wholly inside the viewport, so no clamp is required. The real DAT sphere
/// must produce the expected box around its projected centre, including its nonzero Z.
fn assert_sphere_geometry(app: &App, id: ObjectId) -> dereth_ui::region::Box2D {
    use dereth_assets::Decode;
    let store = dereth_dat::testing::open_store().unwrap();
    let did = DataId(0x0200_0001);
    let setup = dereth_assets::Setup::decode_payload(
        did,
        &store.read_typed(dereth_dat::DbType::Setup, did).unwrap(),
    )
    .unwrap();
    assert!(setup.selection_sphere.radius > 0.0 && setup.selection_sphere.center.z > 0.0);
    let scene = app.world_scene().unwrap();
    let frame = scene.server_object_frame(id).expect("actual drawn setup");
    let center = dereth_physics::math::localtoglobal(&frame, setup.selection_sphere.center);
    let local = dereth_physics::math::globaltolocal(&scene.camera.frame(), center);
    let size = app.renderer().size();
    // The independent scalar oracle must use the world's current fade projection too.
    // Production applies this override in Renderer, not in WorldScene's ordinary reader.
    let projection =
        dereth_render::camera::view_distance_override::with(app.teleport().view_distance(), || {
            dereth_render::camera::projection(&scene.view_params(size.0, size.1)).to_cols_array()
        });
    let (w, h) = (size.0 as f32, size.1 as f32);
    let r = setup.selection_sphere.radius;
    let left =
        dereth_primitives::num::to_i32((local.x - r) * projection[0] / local.y * w * 0.5 + w * 0.5);
    let right =
        dereth_primitives::num::to_i32((local.x + r) * projection[0] / local.y * w * 0.5 + w * 0.5);
    let top =
        dereth_primitives::num::to_i32(h * 0.5 - (local.z + r) * projection[5] / local.y * h * 0.5);
    let bottom =
        dereth_primitives::num::to_i32(h * 0.5 - (local.z - r) * projection[5] / local.y * h * 0.5);
    let ui = &app.ui().unwrap().ui;
    let corner = ui
        .node(handle(app, ElementId(0x1000_0039)))
        .unwrap()
        .region
        .box_;
    let b = ui
        .node(handle(app, window::TARGET_ON_SCREEN))
        .unwrap()
        .region
        .box_;
    for (got, expected) in [
        (b.x0, left - corner.width()),
        (b.y0, top - corner.height()),
        (b.width(), right - left + 2 * corner.width()),
        (b.height(), bottom - top + 2 * corner.height()),
    ] {
        assert!((got-expected).abs() <= 1, "actual DAT bracket geometry {b:?}, expected scalar sphere result {expected}, got {got}");
    }
    // Resizing the parent must really re-anchor the corners, not merely
    // appear as four images at their pristine layout positions.
    let origin = ui.screen_box(handle(app, window::TARGET_ON_SCREEN));
    for index in 1..=4 {
        let cmd = app
            .ui_draw_list()
            .iter()
            .find(|c| c.who == handle(app, ElementId(0x1000_0038 + index)))
            .unwrap();
        assert!(
            cmd.screen.x0 == origin.x0 || cmd.screen.x1 == origin.x1,
            "corner horizontally anchored"
        );
        assert!(
            cmd.screen.y0 == origin.y0 || cmd.screen.y1 == origin.y1,
            "corner vertically anchored"
        );
    }
    b
}

/// Behaviour: selection.brackets.a-selected-object-draws-the-shipped-corner-brackets
#[test]
fn app_draws_dat_brackets_for_world_creature_and_corpse_and_clears_deselection() {
    let mut app = setup();
    place(&mut app, CREATURE, Vec3::new(-2.0, 18.0, -1.0), false);
    place(&mut app, CORPSE, Vec3::new(2.0, 18.0, -1.0), true);
    for (id, color) in [(CREATURE, 0xFFFF_AB00u32), (CORPSE, 0xFFFF_FFFF)] {
        select(&mut app, Some(id));
        assert!(app.frame());
        let on = handle(&app, window::TARGET_ON_SCREEN);
        let off = handle(&app, window::TARGET_OFF_SCREEN);
        assert!(
            app.ui().unwrap().ui.node(on).unwrap().region.flags.visible,
            "selected {id:?} must reach the actual DAT on-screen indicator"
        );
        assert!(!app.ui().unwrap().ui.node(off).unwrap().region.flags.visible);
        for index in 1..=4 {
            let corner = handle(&app, ElementId(0x1000_0038 + index));
            let command = app
                .ui_draw_list()
                .iter()
                .find(|c| c.who == corner)
                .expect("corner drawn");
            assert_eq!(
                command.image,
                dereth_ui_screens::env::did_by_enum(&app.ui().unwrap().ui, 0x1000_0009, index)
            );
            assert_eq!(
                command.image_op,
                Some(dereth_ui::region::SurfaceOp::Colorize(color))
            );
            assert_eq!(
                command.color, 0xFFFF_FFFF,
                "tint is carried by the image operation; the material color remains white"
            );
        }
        assert_sphere_geometry(&app, id);
    }
    // Camera movement happens after ui_use_time; this catches a one-frame-old HUD projection.
    let before = assert_sphere_geometry(&app, CORPSE);
    app.flycam_key(dereth_input::keys::Key::SPACE, true);
    assert!(
        app.probe().camera_input().up,
        "real residual flycam input accepted"
    );
    assert!(app.frame());
    app.flycam_key(dereth_input::keys::Key::SPACE, false);
    let after = assert_sphere_geometry(&app, CORPSE);
    assert_ne!(
        before.y0, after.y0,
        "input moved this frame's camera and brackets together"
    );
    select(&mut app, None);
    assert!(app.frame());
    assert_hidden(&app);
    assert_eq!(app.renderer().ui_stats.decode_failures, 0);
    app.shutdown();
}

#[test]
fn actual_selection_option_ownership_destroy_and_offscreen_consumers_do_not_leave_brackets() {
    let mut app = setup();
    place(&mut app, CREATURE, Vec3::new(0.0, 18.0, -1.0), false);
    select(&mut app, Some(CREATURE));
    assert!(app.frame());
    assert_sphere_geometry(&app, CREATURE);

    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .set_option(14, false, dereth_primitives::ServerTime(1.0));
    assert!(app.frame());
    assert_hidden(&app);
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .set_option(14, true, dereth_primitives::ServerTime(2.0));
    assert!(app.frame());
    assert_sphere_geometry(&app, CREATURE);

    let player = ObjectId(0x5000_0F01);
    app.probe_mut().objects_mut().world.player = Some(player);
    {
        let w = app
            .probe_mut()
            .objects_mut()
            .world
            .tables
            .weenies
            .get_mut(CREATURE)
            .unwrap();
        w.pwd.wielder_id = Some(player);
        w.pwd.location = Some(1);
        w.determine_position_state();
    }
    assert!(app
        .probe_mut()
        .objects_mut()
        .world
        .is_owned_by_player(CREATURE));
    assert!(app.frame());
    assert_hidden(&app);
    {
        let w = app
            .probe_mut()
            .objects_mut()
            .world
            .tables
            .weenies
            .get_mut(CREATURE)
            .unwrap();
        w.pwd.wielder_id = None;
        w.pwd.location = None;
        w.pwd.container_id = Some(ObjectId(0x5000_0F02)); // a foreign container, not ownership
        w.determine_position_state();
    }
    assert!(!app
        .probe_mut()
        .objects_mut()
        .world
        .is_owned_by_player(CREATURE));
    assert!(app.frame());
    assert_hidden(&app);
    {
        let w = app
            .probe_mut()
            .objects_mut()
            .world
            .tables
            .weenies
            .get_mut(CREATURE)
            .unwrap();
        w.pwd.container_id = None;
        w.determine_position_state();
    }
    app.probe_mut().objects_mut().world.player = Some(CREATURE); // selection of self
    assert!(app.frame());
    assert_hidden(&app);
    app.probe_mut().objects_mut().world.player = None;
    assert!(app.frame());
    assert_sphere_geometry(&app, CREATURE);

    // Behind-camera elevation is ignored: these are exact east/west edge arrows.
    for (n, x, arrow) in [(3, 30.0, 9), (4, -30.0, 8)] {
        let id = ObjectId(0x8300_0F00 + n);
        place(&mut app, id, Vec3::new(x, -20.0, 15.0), true);
        select(&mut app, Some(id));
        assert!(app.frame());
        let ui = &app.ui().unwrap().ui;
        assert!(
            !ui.node(handle(&app, window::TARGET_ON_SCREEN))
                .unwrap()
                .region
                .flags
                .visible
        );
        let off = handle(&app, window::TARGET_OFF_SCREEN);
        let cmd = app
            .ui_draw_list()
            .iter()
            .find(|c| c.who == off)
            .expect("DAT offscreen arrow drawn");
        assert_eq!(
            cmd.image,
            dereth_ui_screens::env::did_by_enum(ui, 0x1000_0009, arrow)
        );
        let b = ui.node(off).unwrap().region.box_;
        let size = app.renderer().size();
        assert_eq!(
            b.x0,
            if x > 0.0 {
                size.0 as i32 - b.width() - 8
            } else {
                8
            }
        );
        assert_eq!(b.y0, (size.1 as i32 - b.height()) / 2);
        for i in 1..=4 {
            assert!(!app
                .ui_draw_list()
                .iter()
                .any(|c| c.who == handle(&app, ElementId(0x1000_0038 + i))));
        }
    }
    select(&mut app, Some(CREATURE));
    assert!(app.frame());
    assert_sphere_geometry(&app, CREATURE);
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemDeleteObject {
        id: CREATURE,
        instance_sequence: 0,
    })
    .unwrap();
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_DELETE_OBJECT,
            body,
        },
        LocalTime(3.0),
    );
    assert!(app.frame());
    assert_hidden(&app);
    assert!(app
        .world_state()
        .unwrap()
        .server_object_frame(CREATURE)
        .is_none());
    app.shutdown();
}

#[test]
fn rebuilt_gameplay_at_a_different_display_extent_reprojects_real_dat_corners() {
    // The App has no runtime resize API (retail's ordinary WM_SIZE is also ignored).
    // Drive both supported device extents rather than pretend changing UI alone resizes GPU.
    let mut extents = Vec::new();
    for size in [(800, 600), (1024, 768)] {
        let mut app = setup_at(size);
        place(&mut app, CREATURE, Vec3::new(2.0, 18.0, -1.0), false);
        select(&mut app, Some(CREATURE));
        assert!(app.frame());
        let b = assert_sphere_geometry(&app, CREATURE);
        let old = handle(&app, window::TARGET_ON_SCREEN);
        app.queue_ui_mode(mode::GAME_PLAY);
        assert!(app.frame());
        assert!(
            app.ui().unwrap().ui.node(old).is_none(),
            "old DAT generation destroyed"
        );
        assert_sphere_geometry(&app, CREATURE);
        extents.push(b);
        app.shutdown();
    }
    assert!(
        extents[1].width() > extents[0].width(),
        "sphere pixel radius grows with the display"
    );
    assert!(extents[1].x0 > extents[0].x0 && extents[1].y0 > extents[0].y0);
}

/// Behaviour: selection.brackets.the-brackets-hide-during-the-portal-tunnel
///
/// Synthetic transition input, not a captured portal/logout journey: arm the production logoff
/// timer, then signal the animation's completion. App owns every intervening
/// clock/state/render/UI step. This checks the target indicator against that transition model;
/// it does not validate the model's documented use of 0xF748 for the physics state.
#[test]
fn selected_target_tracks_world_fade_projection_and_hides_during_the_tunnel() {
    use dereth_client_runtime::teleport::TeleportAnimState;
    let mut app = setup();
    place(&mut app, CREATURE, Vec3::new(2.0, 18.0, -1.0), false);
    select(&mut app, Some(CREATURE));
    assert!(app.frame());
    let normal = assert_sphere_geometry(&app, CREATURE);

    // Make the existing three-second timer due; do not set the resulting projection or flags.
    let now = app.ui().unwrap().ui.now.0;
    app.probe_mut()
        .teleport_mut()
        .request_log_off(now - 3.1, false);
    for _ in 0..25 {
        assert!(app.frame());
    }
    assert_eq!(app.teleport().anim.state, TeleportAnimState::WorldFadeOut);
    assert!(!app.renderer().world_hidden());
    assert!(app.teleport().view_distance().is_some());
    let faded = assert_sphere_geometry(&app, CREATURE);
    assert!(
        faded.width() < normal.width(),
        "brackets use the collapsing world projection"
    );

    for _ in 0..90 {
        assert!(app.frame());
        if app.renderer().world_hidden() {
            break;
        }
    }
    assert!(app.renderer().world_hidden(), "App entered the tunnel");
    assert_hidden(&app);
    assert_eq!(
        app.objects().world.selected,
        Some(CREATURE),
        "hidden is not deselected"
    );

    let now = app.ui().unwrap().ui.now.0;
    app.probe_mut().teleport_mut().anim.end(now);
    let mut visible_fade = false;
    let mut finished = false;
    for _ in 0..480 {
        assert!(app.frame());
        if app.renderer().world_hidden() {
            assert_hidden(&app);
        } else if app.teleport().anim.vivid_target_indicator {
            assert_sphere_geometry(&app, CREATURE);
            visible_fade |= app.teleport().view_distance().is_some();
        }
        if app.teleport().anim.state == TeleportAnimState::Off {
            finished = true;
            break;
        }
    }
    assert!(
        visible_fade,
        "selection resumes during the visible world fade-in"
    );
    assert!(finished, "animation completes through actual App frames");
    assert!(app.teleport().view_distance().is_none());
    assert_eq!(assert_sphere_geometry(&app, CREATURE), normal);
    app.shutdown();
}
