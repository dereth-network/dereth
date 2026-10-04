//! Clicking your own body selects you: the pick ray through the local body's chest answers the
//! player's id, and a click two metres beside him answers nothing. Fixture: a headless App with
//! the UI shell on a real terrain scene, the player's id and assets from
//! early-inventory-and-casting's recorded create (relocated onto the terrain body), and a
//! constructed visible-state update; events go straight to the object stream.
//!
//! # How the client picks its own body
//!
//! Every drawn part with a nonzero object id takes part in the mesh ray test, and the player's
//! parts carry the player's id. A found object is rejected only when it is missing or carries the
//! UI-hidden flag 0x80; the selection setter has no self exclusion. The local player has no
//! server scene object, so the pick must also sweep the local body. Clicks go through
//! `wrapper_mouse` then `App::frame` (PRIMARY_CLICK, start=true, over=None; no mouse-up).

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::app::{frames, position};
use dereth_client::world::SceneReads;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::interaction;
use dereth_client::pick::PickScene;
use dereth_client::ui::UiMouseEvent;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};

const SCREEN: (u32, u32) = (800, 600);
/// Visible-state word applied after the hidden login create; the test constructs the update and
/// advances its event stamp.
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;

/// Construct a local terrain body, with the UI shell enabled so the direct wrapper click uses
/// the actual world-view rectangle. An App that cannot be built, and any shell, scene or
/// recording failure after it, fails the test.
fn setup() -> (App, ObjectId) {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-select-self-not-created")
            .join("preferences.ini"),
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain/physics scene");
    frames(&mut app, 60);

    let corpus = Corpus::shared("early-inventory-and-casting");
    let player_row = corpus
        .blobs
        .iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0)
        .expect("recorded player identity");
    let id = ObjectId(u32::from_le_bytes(
        player_row.payload[4..8].try_into().unwrap(),
    ));
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == id.0
        })
        .expect("recorded player assets");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("recorded 0xF745");
    let here = position(&app);
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: here.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: here.frame.origin.x,
                y: here.frame.origin.y,
                z: here.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: here.frame.rotation.w,
                x: here.frame.rotation.x,
                y: here.frame.rotation.y,
                z: here.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("constructed terrain placement"),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 90);
    // A login create arrives hidden (state 0x00404410) and a later state update clears hidden;
    // no-draw parts skip the ray test. Here the update is constructed: the current event stamp
    // plus one (wrapping), instance zero and the visible-state word, then 60 frames, which cover
    // the 0.75 s visibility ramp. No elapsed time or recorded state-event sequence is asserted.
    {
        let state_ts = app
            .objects()
            .presence(id)
            .expect("the player's presence")
            .state_ts;
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemSetState::OPCODE,
                body: dereth_protocol::write_body(&ItemSetState {
                    id,
                    state: TELEPORT_UNHIDE_STATE,
                    timestamps: PhysicsEventStamp {
                        instance: 0,
                        event: state_ts.wrapping_add(1),
                    },
                })
                .expect("encodes"),
            },
            LocalTime(4.0),
        );
    }
    frames(&mut app, 60);
    {
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(c.on_ground(), "real terrain must support the local body");
        assert_eq!(c.object_id(), id, "the body adopted the server's id");
        let d = c.driver();
        let drawn = d.part_array.parts.iter().filter(|p| !p.no_draw()).count();
        assert_eq!(
            drawn,
            d.part_array.parts.len(),
            "every body part has its no-draw flag cleared"
        );
    }
    (app, id)
}

fn click(x: i32, y: i32) -> UiMouseEvent {
    UiMouseEvent {
        action: dereth_ui::focus::action::PRIMARY_CLICK,
        start: true,
        x,
        y,
        // `is_world_click(None)`: the pointer is over the 3D view and not over a HUD element.
        over: None,
    }
}

/// Invert the viewer projection for local coordinates (+x right, +y forward, +z up), using the
/// client's half-viewport and tangent formula for the pixel whose ray passes through them.
fn pixel_of(local: Vec3, viewport: (u32, u32), fov_y_rad: f32) -> (f32, f32) {
    let half_w = (viewport.0 as f32 - 1.0) * 0.5;
    let half_h = (viewport.1 as f32 - 1.0) * 0.5;
    // `vdst = ty / tan(fov/2)`, in the same units as `ty`.
    let vdst = half_h / math::tanf(fov_y_rad * 0.5);
    let k = vdst / local.y;
    (half_w + local.x * k, half_h - local.z * k)
}

/// Project a world point and check its unrounded pixel with the production `selection_ray`:
/// cosine >0.9999 is measured before returning rounded integer click coordinates.
fn aim(app: &App, world: Vec3) -> (i32, i32) {
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let local = dereth_physics::math::globaltolocal(&viewer, world);
    assert!(
        local.y > 0.5,
        "the point is in front of the chase camera: {local:?}"
    );
    let (px, py) = pixel_of(local, SCREEN, fov);
    let ray = selection_ray(&viewer, px, py, SCREEN, fov);
    let to = Vec3::new(
        world.x - viewer.origin.x,
        world.y - viewer.origin.y,
        world.z - viewer.origin.z,
    );
    let len = (to.x * to.x + to.y * to.y + to.z * to.z).sqrt();
    let cos = (ray.x * to.x + ray.y * to.y + ray.z * to.z) / len;
    assert!(
        cos > 0.9999,
        "the pick ray through ({px}, {py}) points at the point: cos = {cos}"
    );
    (px.round() as i32, py.round() as i32)
}

fn pick_at(app: &mut App, at: (i32, i32)) -> ObjectId {
    let e = click(at.0, at.1);
    let armed = app
        .interaction_mut()
        .wrapper_mouse(e, SCREEN, interaction::is_world_click(e.over));
    assert!(
        armed,
        "({}, {}) is inside the 3D view and arms a Select pick",
        at.0, at.1
    );
    assert!(app.frame(), "the frame that answers the pick");
    assert!(
        !app.interaction().pick.looking_for_object(),
        "the pick was answered this frame"
    );
    app.interaction().pick.click_object().0
}

/// Behaviour: selection.pick.clicking-your-own-body-selects-you
///
/// A direct wrapper click on the chest selects the player; the control point is two metres
/// camera-right at the same depth, where the answered object ID must be zero.
#[test]
fn clicking_the_local_body_selects_the_player_and_beside_him_selects_nothing() {
    let (mut app, player) = setup();
    let scene = app.world_scene().unwrap();
    assert!(
        scene.server_object_frame(player).is_none(),
        "the premise: the player has no SceneObject, so the presence sweep alone cannot find him"
    );
    let (root, _parts) = scene.character_frames().expect("the local body's frames");
    let viewer = PickScene::viewer(&scene);
    let d = Vec3::new(
        root.origin.x - viewer.origin.x,
        root.origin.y - viewer.origin.y,
        root.origin.z - viewer.origin.z,
    );
    let dist = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    assert!(
        (1.0..20.0).contains(&dist),
        "the camera-to-body distance is within 1..20 m: {dist} m"
    );
    // The body's origin is at its feet; a metre up is the torso of the Aluvian male setup.
    let chest = Vec3::new(root.origin.x, root.origin.y, root.origin.z + 1.0);
    // Two metres camera-right at the same depth. Terrain does not take part in the mesh-object
    // ray test, so the answer at this point is zero.
    let local = dereth_physics::math::globaltolocal(&viewer, chest);
    let beside =
        dereth_physics::math::localtoglobal(&viewer, Vec3::new(local.x + 2.0, local.y, local.z));
    let body_px = aim(&app, chest);
    let beside_px = aim(&app, beside);
    assert_ne!(body_px, beside_px);
    {
        let b = PickScene::local_body(&scene).expect("the scene offers its local body");
        let hidden = b.parts.iter().filter(|p| p.no_draw).count();
        eprintln!(
            "select_self: viewer {:?} root {:?} dist {dist} chest px {body_px:?} beside px {beside_px:?}",
            viewer.origin, root.origin
        );
        eprintln!(
            "select_self: body {:?} setup {:?} parts {} hidden {} part0 {:?} scale0 {:?}",
            b.id,
            b.setup,
            b.parts.len(),
            hidden,
            b.parts.first().map(|p| p.pos.origin),
            b.parts.first().map(|p| p.gfxobj_scale)
        );
    }

    // **The control first**, while nothing is selected: the zero answer is delivered as a zero
    // and changes no selection.
    assert_eq!(
        app.objects().world.selected,
        None,
        "nothing is selected to begin with"
    );
    let miss = pick_at(&mut app, beside_px);
    assert_eq!(miss, ObjectId(0), "beside the body is empty space");
    assert_eq!(
        app.objects().world.selected,
        None,
        "and a zero answer selects nothing"
    );

    // **The rejecting half.** A presence sweep alone answers 0 here: it skips the one object
    // with no `SceneObject`, and the click falls through him.
    let hit = pick_at(&mut app, body_px);
    assert_eq!(
        hit, player,
        "the pick ray through the body's chest answers the player's id"
    );
    assert_eq!(
        app.objects().world.selected,
        Some(player),
        "the world selection records the player returned by the body pick"
    );
    assert!(
        app.interaction().pick.stats.found >= 1 && app.interaction().pick.stats.missed >= 1,
        "both arms ran through the real sweep: {:?}",
        app.interaction().pick.stats
    );

    // The selection lookup clears the in-view observation and a selected part drawn inside the
    // view cone restores it. Player parts carry the player id, so a selected self needs the
    // body's draw to report visibility too, or the next range check clears the selection. Both
    // the selection and the body-draw report are checked.
    //
    // Read the scene's per-frame report here, not the folded world latch: a hover does one extra
    // lookup per frame and clears the report before the range checks fold it (the fold without
    // hover is covered by the selection-persistence tests). Three extra frames give one frame of
    // margin over the two-frame draw delay. Assertions below inspect the final selection, watched
    // id, local-body draw entries and scene report; they do not establish continuous selection, a
    // pixel target box or the folded latch value.
    frames(&mut app, 3);
    assert_eq!(app.objects().world.selected, Some(player), "still selected");
    assert_eq!(
        app.objects().world.viewcone_check_object_id,
        Some(player),
        "the view-cone check tracks the selected player"
    );
    let scene = app.world_scene().unwrap();
    let body_parts_drawn = scene
        .drawn_part_order()
        .iter()
        .filter(|d| d.object.is_none())
        .count();
    assert!(
        body_parts_drawn > 0,
        "the final frame contains local-body part submissions"
    );
    assert!(
        scene.take_selected_part_drawn(),
        "the final frame reports a selected-body part drawn inside the view cone"
    );
}
