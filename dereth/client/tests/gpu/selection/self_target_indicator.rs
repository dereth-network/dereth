//! Selecting yourself hides the in-world target indicator: the on-screen brackets and the
//! off-screen arrow both go down and no corner command stays in the draw list, while the player
//! stays selected; reselecting another object draws all four corners again. Fixture: a headless
//! App with the UI shell on `DEFAULT_LANDBLOCK` terrain, the local body given
//! early-inventory-and-casting's recorded identity, a constructed control creature, and pointer
//! presses as synthesized Win32 messages through the input manager. No datagram is sent.
//!
//! # How the client decides
//!
//! * **The indicator's selection setter is the single gate, and its first act is the self
//!   comparison.** It loads the current player id (zero when no world controller exists),
//!   compares it with the incoming selection id, and replaces a match with zero.
//!
//! * Three later refusals collapse to the same zero: no game object, ownership by the player, and
//!   `current_state == IN_CONTAINER` (state 2).
//!
//! * **The player is refused twice.** The ownership predicate returns owned immediately when the
//!   object's id equals the proposed owner, so the ownership refusal rejects the player even
//!   without the direct player-id comparison; removing only one guard changes nothing observable.
//!
//! * **Zero is neither an arrow nor a box.** The setter stores zero as the target-object id.
//!   World rendering needs both a nonzero target id and a target callback before looking up a
//!   bounding box, so it never requests the player's box. When the indicator is enabled, the
//!   zero-selection arm explicitly hides both the on-screen and off-screen display elements.
//!
//! * The draw callback would refuse it a third time: after checking enabled and display-on state,
//!   it rejects id zero. Only `status == 1` takes the bracket arm and only `status == 2` takes the
//!   arrow arm.
//!
//! * The selection-changed notice loads the selected id, calls the selection setter, and returns.
//!   Display-state updates, enabling, player-description notices, player-option notices,
//!   radar-look notices and quality changes all funnel through the same setter.
//!
//! * **The bounding-box lookup carries no self exclusion.** It returns object-not-found only for a
//!   missing object and never converts to player space. The selected-object-in-view predicate
//!   serves the object-range exit only and never reaches the target indicator.
//!
//! So selecting yourself hides the indicator, by the same rule that hides it for an item you own
//! or an item in a container.
//!
//! The control object is selected **first**, so the on-screen brackets are visible when the self
//! click arrives. Drawing a box for the player instead would leave the *previous* target's
//! brackets on screen, because `target::draw` returns early on a `None` projection (the local body
//! has no `SceneObject`) without hiding anything: with both self refusals removed from the
//! selection setter, the self press leaves `on-screen true, corners 4`. That is the state this
//! test refuses.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::app::{frames, position};

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::pick::PickScene;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::hud::target::Projection;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

const SCREEN: (u32, u32) = (800, 600);
/// The visible-state word that un-hides a login's hidden create.
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
/// The Aluvian male body setup.
const MONSTER_SETUP: u32 = 0x0200_0001;
/// The control object — someone who is not me.
const OTHER: ObjectId = ObjectId(0x8000_3101);
/// `PlayerOption::VividTargetingIndicator`, `hud.rs`'s index 14. A player-option notice re-evaluates
/// this value to decide whether indicator display is enabled.
const OPTION_VIVID_TARGETING_INDICATOR: usize = 14;

/// UI shell up, real `DEFAULT_LANDBLOCK` terrain, the local body given
/// early-inventory-and-casting's recorded identity and un-hidden as a login does, plus the
/// character option the indicator reads.
fn setup() -> (App, ObjectId) {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-self-target-indicator-not-created")
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
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("constructed terrain placement"),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 90);
    // A login creates the player hidden; every part of a hidden body has `NoDraw`, so drawing
    // returns before any selection-ray test.
    {
        let state_ts = app
            .objects()
            .presence(id)
            .expect("the player's presence")
            .state_ts;
        app.probe_mut().objects_mut().apply_event(
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
            "the un-hidden body draws every part"
        );
    }
    // Enable indicator display. Without it the draw callback hides both elements for everyone,
    // and the control leg could not distinguish a self refusal from an entirely disabled display.
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .apply_player_module(&dereth_protocol::login::PlayerModule::default());
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .set_option(
            OPTION_VIVID_TARGETING_INDICATOR,
            true,
            dereth_primitives::ServerTime(0.0),
        );
    frames(&mut app, 2);
    (app, id)
}

/// Create a monster at a **player-space** offset through the client's own `0xF745` and require
/// that it is drawn.
fn place(app: &mut App, id: ObjectId, offset: (f32, f32, f32), now: f64) {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    let here = position(app);
    let origin =
        dereth_physics::math::localtoglobal(&here.frame, Vec3::new(offset.0, offset.1, offset.2));
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: Some(MONSTER_SETUP),
            state: 0,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: here.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: here.frame.rotation.into(),
                },
            }),
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&ItemCreateObject(payload)).expect("encode");
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    frames(app, 30);
    let scene = app.world_scene().unwrap();
    assert!(
        scene.server_object_frame(id).is_some(),
        "{id:?} has a drawn SceneObject: the frame answers for it"
    );
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

fn visible(app: &App, id: ElementId) -> bool {
    app.ui()
        .unwrap()
        .ui
        .node(handle(app, id))
        .unwrap()
        .region
        .flags
        .visible
}

/// How many of the four bracket corners reached this frame's draw list.
fn corners_drawn(app: &App) -> usize {
    (1..=4)
        .filter(|i| {
            let h = handle(app, ElementId(0x1000_0038 + i));
            app.ui_draw_list().iter().any(|c| c.who == h)
        })
        .count()
}

fn report(app: &App, tag: &str) -> String {
    format!(
        "{tag}: selected {:?} player {:?} on-screen {} off-screen {} corners {}",
        app.objects().world.selected,
        app.objects().world.player,
        visible(app, window::TARGET_ON_SCREEN),
        visible(app, window::TARGET_OFF_SCREEN),
        corners_drawn(app)
    )
}

/// Both display elements are hidden, with no bracket corners left in the frame's draw list.
fn assert_indicator_hidden(app: &App, why: &str) {
    assert!(
        !visible(app, window::TARGET_ON_SCREEN),
        "{why}: the on-screen brackets are hidden"
    );
    assert!(
        !visible(app, window::TARGET_OFF_SCREEN),
        "{why}: the off-screen arrow is hidden"
    );
    assert_eq!(
        corners_drawn(app),
        0,
        "{why}: no stale corner command survives in the draw list"
    );
}

/// The inverse of the render pick-ray projection for a point already in viewer space.
fn pixel_of(local: Vec3, viewport: (u32, u32), fov_y_rad: f32) -> (f32, f32) {
    let half_w = (viewport.0 as f32 - 1.0) * 0.5;
    let half_h = (viewport.1 as f32 - 1.0) * 0.5;
    let vdst = half_h / math::tanf(fov_y_rad * 0.5);
    let k = vdst / local.y;
    (half_w + local.x * k, half_h - local.z * k)
}

/// A world point's viewport pixel, checked against the pick's own `selection_ray`.
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

/// A synthesized Win32 pointer press at a screen point (move, button down, button up), through
/// the headless input manager.
fn press_at(app: &mut App, x: i32, y: i32, at: u32) {
    let mut pump = dereth_client::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let messages = [
        pump.mouse_move_message(f64::from(x), f64::from(y), at),
        pump.mouse_button_message(winit::event::MouseButton::Left, true, at + 10)
            .unwrap(),
        pump.mouse_button_message(winit::event::MouseButton::Left, false, at + 20)
            .unwrap(),
    ];
    for message in messages {
        pump.dispatch(message);
        app.input_manager_mut()
            .expect("real input maps")
            .on_message(message);
    }
    for _ in 0..2 {
        assert!(app.frame());
    }
}

/// Behaviour: selection.indicator.selecting-yourself-hides-the-target-indicator
///
/// A press through the input path on someone else draws the brackets; one on your own body
/// selects you and takes the brackets down, because the indicator's selection setter compares
/// the incoming id against the player id and turns a match into no target at all.
#[test]
fn selecting_yourself_takes_the_vivid_target_indicator_down_as_retail_does() {
    let (mut app, player) = setup();
    // Four metres ahead of the body and two to its right: fully on screen, and clear of the
    // body's own silhouette. Directly ahead does not work — the body is *nearer* the chase
    // camera, and the pick takes the nearest hit, so the click lands on me instead.
    place(&mut app, OTHER, (2.2, 4.0, 0.0), 10.0);
    frames(&mut app, 10);

    assert_eq!(
        app.objects().world.player,
        Some(player),
        "the client knows who it is"
    );
    assert_eq!(
        app.objects().world.selected,
        None,
        "nothing is selected to begin with"
    );
    assert_indicator_hidden(&app, "before anything is selected");

    // Where the two clicks go. The body's origin is at its feet; a metre up is the torso of the
    // Aluvian male setup.
    let (chest_px, other_px) = {
        let scene = app.world_scene().unwrap();
        let (root, _parts) = scene.character_frames().expect("the local body's frames");
        let chest = Vec3::new(root.origin.x, root.origin.y, root.origin.z + 1.0);
        let chest_px = aim(&app, chest);
        let r = match app.renderer().target_projection(OTHER, app.world_state()) {
            Some(Projection::OnScreen(r)) => r,
            other => panic!("the control object is on screen: {other:?}"),
        };
        let other_px = ((r.0 + r.2) / 2, (r.1 + r.3) / 2);
        assert_ne!(
            chest_px, other_px,
            "the two clicks land on different pixels"
        );
        eprintln!("self_target_indicator: chest px {chest_px:?} other px {other_px:?} rect {r:?}");
        (chest_px, other_px)
    };

    // ---------------------------------------------------------------------------------------
    // The control: someone who is not me. This is what "draws as today" means, and it is also
    // what arms the self leg — the brackets must be up when the self click arrives.
    // ---------------------------------------------------------------------------------------
    press_at(&mut app, other_px.0, other_px.1, 5_000);
    eprintln!(
        "self_target_indicator: {}",
        report(&app, "after the control press")
    );
    assert_eq!(
        app.objects().world.selected,
        Some(OTHER),
        "the pointer press at {other_px:?} reached world-object lookup and selected the control"
    );
    assert!(
        visible(&app, window::TARGET_ON_SCREEN),
        "a non-self selection keeps the control id and draws the on-screen brackets"
    );
    assert!(
        !visible(&app, window::TARGET_OFF_SCREEN),
        "the control is on screen, not off it"
    );
    assert_eq!(
        corners_drawn(&app),
        4,
        "all four DAT corner brackets reached the draw list"
    );

    // ---------------------------------------------------------------------------------------
    // The rejecting leg: me.
    // ---------------------------------------------------------------------------------------
    press_at(&mut app, chest_px.0, chest_px.1, 6_000);
    eprintln!(
        "self_target_indicator: {}",
        report(&app, "after the self press")
    );
    assert_eq!(
        app.objects().world.selected,
        Some(player),
        "a press through the input path still selects the player and the toolbar strip shows \
         him; world selection has no self exclusion"
    );
    assert_indicator_hidden(
        &app,
        "selecting yourself: the direct player-id comparison and ownership test both reduce the \
         indicator target to zero; zero bypasses bounding-box lookup and explicitly hides both \
         display elements",
    );
    // The double refusal, on our own live world: both inputs that `draw_world_target` supplies to
    // the indicator selection setter say "not a target".
    assert!(
        app.objects().world.is_the_player(player),
        "the clicked id matches the world controller's player id"
    );
    assert!(
        app.objects().world.is_owned_by_player(player),
        "and the ownership identity rule refuses the player a second time"
    );

    // A frame later it is still down — the hide is the steady state, not a one-frame blip.
    frames(&mut app, 5);
    assert_eq!(app.objects().world.selected, Some(player), "still selected");
    assert_indicator_hidden(&app, "five frames after selecting yourself");

    // ---------------------------------------------------------------------------------------
    // And it comes back for someone else: the hide belongs to the self rule, not to a latch
    // that the first self selection broke.
    // ---------------------------------------------------------------------------------------
    press_at(&mut app, other_px.0, other_px.1, 7_000);
    eprintln!(
        "self_target_indicator: {}",
        report(&app, "after re-selecting the control")
    );
    assert_eq!(
        app.objects().world.selected,
        Some(OTHER),
        "re-selected the control"
    );
    assert!(
        visible(&app, window::TARGET_ON_SCREEN),
        "the brackets return for a non-self target after a self selection"
    );
    assert_eq!(
        corners_drawn(&app),
        4,
        "and all four corners are drawn again"
    );
}
