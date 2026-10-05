//! A press in the open 3-D view selects the object behind it; the transparent padding
//! ("gutters") of the inventory panel hands the pointer to the world view as retail does, rather
//! than the whole HUD being opaque to clicks; and the game viewport is the whole window.
//! Fixture: a headless App on `DEFAULT_LANDBLOCK` terrain with the shipped gameplay layout, the
//! player and a chest from `early-inventory-and-casting` applied as local events, and normalized
//! pointer messages through the input manager.
//!
//! # Layout and hit-test behaviour
//!
//! The retail viewport calculation begins with display width/height and visits visible UI
//! objects. A valid edge value from 1 through 4 shrinks one edge per object: 1 top, 2 bottom,
//! 3 left, 4 right. The edge setter is fed by element property 0x52. In the shipped layouts that
//! property appears only on roots of older docked layouts; the root children of
//! `classic_gameplay` inherit from `classic_floaty*` instead and arm no clamp edge. This file
//! does not rescan the layouts for it.
//!
//! Mouse hit testing walks visible children tail-to-head, checks their bounds and rebases
//! coordinates. If no child accepts, an element accepts only when mouse-visible or configured
//! to block clicks. Floating-panel padding has neither condition, allowing the earlier sibling
//! `<SBOX>` to receive the pointer. Once that viewport wrapper receives a selection action, its
//! object lookup can answer a UI item synchronously or request a world pick; it does not add a
//! second panel-under-pointer test. This does not remove the picker's own bounds/search guards.
//!
//! `App` supplies the pick/render viewport from `<SBOX>`'s own client rectangle. The
//! full-buffer rectangle and sampled gutter hit tests below pin observable outcomes, not the
//! retail property's setter provenance or a complete layout census. Making these sampled
//! gutters mouse-opaque would fail their positive selection controls.
//!
//! # Fixture and input boundaries
//!
//! Scenery, cell statics, mesh collision and particles are disabled. The player's
//! identity/create assets are decoded, the create relocated and its POSITION flag set, then an
//! unhide state update is constructed with a fresh event stamp. The chest also comes from a
//! decoded create with position and POSITION flag changed. These are direct decoded
//! object-stream events, not whole network replay or socket/session decoding.
//!
//! Pointer motion and left-button down/up use normalized `Pump` messages and the input manager,
//! followed by frames through the actual UI hit-test/selection path. There are no OS-injected
//! mouse events or direct selection-success calls; direct selection clearing is only a reset.
//! Chest coordinates come from geometry projection and a ray-direction check, not pixel capture.
//! Three active tests cover open-view selection, three gutter samples and viewport bounds. The
//! ignored fourth test reports a larger hit-test map without asserting its shape. A missing
//! device fails the tests. No datagram leaves this process.

#![cfg(gpu)]

use crate::common::app::{frames, position};

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::pick::PickScene;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemDeleteObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui::{ElemHandle, UiSystem};
use winit::event::MouseButton;
use {dereth_client_runtime::scene::SceneConfig, dereth_world_data::landblock::DEFAULT_LANDBLOCK};

const SCREEN: (u32, u32) = (800, 600);
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
const SMART_BOX: dereth_ui::ElementId = dereth_ui_screens::hud::world_view::SMART_BOX;
const PANEL_STACK: dereth_ui::ElementId = dereth_ui_screens::screens::gameplay::window::PANEL_STACK;

/// Three inventory gutter samples: `(column, row, containing column run)`.
/// The runs are 495..509, 718..739 and 776..794 on rows 400..450: the left margin, the
/// paper-doll/backpack region and the strip right of the item list, with neighbouring control
/// bounds paper-doll right 728 and list left 740. They are samples, not assertions over every
/// row. The ignored probe reports runs at four rows; the active test checks row 407, then the
/// chest's separately projected point in each run.
const GUTTERS: [(i32, i32, (i32, i32)); 3] = [
    (500, 407, (495, 509)),
    (725, 407, (718, 739)),
    (785, 407, (776, 794)),
];

fn setup() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-viewport-click-passthrough-not-created")
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
        .expect("recorded F745");
    let here = position(&app);
    create.0.physicsdesc.position = Some(wire_position(here.cell.0, here.frame.origin, &here));
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
    app
}

fn wire_position(
    cell: u32,
    origin: Vec3,
    frame: &Position,
) -> dereth_protocol::types::PositionWire {
    dereth_protocol::types::PositionWire {
        objcell_id: cell,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: origin.x,
                y: origin.y,
                z: origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: frame.frame.rotation.w,
                x: frame.frame.rotation.x,
                y: frame.frame.rotation.y,
                z: frame.frame.rotation.z,
            },
        },
    }
}

fn place_the_corpus_chest_at(app: &mut App, lateral: f32) -> (ObjectId, String) {
    let corpus = Corpus::shared("early-inventory-and-casting");
    let mut chosen = None;
    for r in &corpus.blobs {
        if r.dir != Direction::ServerToClient || r.opcode != Opcode::ITEM_CREATE_OBJECT.0 {
            continue;
        }
        let Ok(c) = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..]))
        else {
            continue;
        };
        if c.0.wdesc.name.contains("Chest")
            && c.0.physicsdesc.bitfield & dereth_protocol::types::physicsdesc::flags::SETUP != 0
        {
            chosen = Some(c);
            break;
        }
    }
    let mut create = chosen.expect("early-inventory-and-casting has a named chest with a SETUP");
    let here = position(app);
    let origin = dereth_physics::math::localtoglobal(&here.frame, Vec3::new(lateral, 4.0, 0.0));
    create.0.physicsdesc.position = Some(wire_position(here.cell.0, origin, &here));
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    let id = create.0.id;
    let name = create.0.wdesc.name.clone();
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("re-encodes"),
        },
        LocalTime(6.0),
    );
    frames(app, 30);
    (id, name)
}

fn delete_the_chest(app: &mut App, id: ObjectId) {
    let instance = app
        .objects()
        .presence(id)
        .expect("the chest is live")
        .instance;
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemDeleteObject::OPCODE,
            body: dereth_protocol::write_body(&ItemDeleteObject {
                id,
                instance_sequence: instance,
            })
            .expect("encodes"),
        },
        LocalTime(7.0),
    );
    frames(app, 5);
}

/// Put the chest's projected sample point inside a gutter run with a two-pixel horizontal
/// margin. The initial solve can land about 10% long after terrain settling under the chase
/// camera. Re-measure the object frame, correct lateral offset by wanted/reached distance
/// from screen center, and delete each failed placement before trying again (at most six tries).
/// This measures geometric projection, not rendered pixels; the returned row need not be 407.
fn place_chest_in_run(
    app: &mut App,
    target: i32,
    run: (i32, i32),
) -> (ObjectId, String, (i32, i32)) {
    #[allow(clippy::cast_precision_loss)]
    let cx = (SCREEN.0 as f32 - 1.0) * 0.5;
    let mut lateral = lateral_for_column(app, target);
    let mut last: Option<(ObjectId, String, (i32, i32))> = None;
    for _ in 0..6 {
        if let Some((id, _, _)) = last.take() {
            delete_the_chest(app, id);
        }
        let (id, name) = place_the_corpus_chest_at(app, lateral);
        let at = chest_pixel(app, id);
        if run.0 + 2 <= at.0 && at.0 <= run.1 - 2 {
            return (id, name, at);
        }
        #[allow(clippy::cast_precision_loss)]
        let reached = at.0 as f32 - cx;
        #[allow(clippy::cast_precision_loss)]
        let want = target as f32 - cx;
        assert!(
            reached.abs() > 1.0,
            "the projected chest is too close to the center line for this correction"
        );
        lateral *= want / reached;
        last = Some((id, name, at));
    }
    let (id, name, at) = last.expect("at least one attempt");
    panic!(
        "could not put the chest inside {run:?}: last attempt projected to {at:?} ({id:?} {name})"
    )
}

#[allow(clippy::cast_precision_loss)]
fn pixel_of(local: Vec3, viewport: (u32, u32), fov_y_rad: f32) -> (f32, f32) {
    let half_w = (viewport.0 as f32 - 1.0) * 0.5;
    let half_h = (viewport.1 as f32 - 1.0) * 0.5;
    let vdst = half_h / math::tanf(fov_y_rad * 0.5);
    let k = vdst / local.y;
    (half_w + local.x * k, half_h - local.z * k)
}

#[allow(clippy::cast_possible_truncation)]
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

fn chest_pixel(app: &App, chest: ObjectId) -> (i32, i32) {
    let scene = app.world_scene().unwrap();
    let f = scene
        .server_object_frame(chest)
        .expect("the chest has a SceneObject");
    aim(app, Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.5))
}

/// The lateral offset that puts a point 4 m ahead of the body nearest to screen column `target_x`.
#[allow(clippy::cast_precision_loss)]
fn lateral_for_column(app: &App, target_x: i32) -> f32 {
    let here = position(app);
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let mut best = (f32::MAX, 0.0f32);
    for i in -200i32..=200 {
        let dx = i as f32 * 0.05;
        let p = dereth_physics::math::localtoglobal(&here.frame, Vec3::new(dx, 4.0, 0.5));
        let local = dereth_physics::math::globaltolocal(&viewer, p);
        if local.y <= 0.5 {
            continue;
        }
        let (px, _) = pixel_of(local, SCREEN, fov);
        let err = (px - target_x as f32).abs();
        if err < best.0 {
            best = (err, dx);
        }
    }
    assert!(
        best.0 < 8.0,
        "no offset 4 m ahead reaches column {target_x}: best error {}",
        best.0
    );
    best.1
}

fn ui_of(app: &mut App) -> &mut UiSystem {
    &mut app.ui_mut().expect("the shell is up").ui
}

fn element_of(app: &mut App, id: dereth_ui::ElementId) -> ElemHandle {
    let shell = app.ui_mut().expect("the shell is up");
    let root = *shell
        .flow
        .current()
        .expect("a screen")
        .roots()
        .first()
        .expect("a root");
    shell
        .ui
        .get_child_recursive(root, id)
        .expect("in the shipped layout")
}

fn world_view(app: &mut App) -> ElemHandle {
    element_of(app, SMART_BOX)
}

fn seed_inventory(app: &mut App) {
    let w = &mut app.probe_mut().objects_mut().world;
    let player = w.player.expect("the bench's body has the server's id");
    w.tables.inventories.insert(
        player,
        dereth_client_model::objects::ObjectInventory::new(player),
    );
    for i in 0..3usize {
        let id = ObjectId(0x7E00_0100 + u32::try_from(i).unwrap());
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = format!("bag item {i}");
        wn.pwd.container_id = Some(player);
        w.tables.weenies.insert(id, wn);
        w.tables
            .inventories
            .get_mut(player)
            .expect("seeded")
            .add_content(id, false, i);
    }
    if let Some(p) = w.tables.weenies.get_mut(player) {
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
    }
}

fn show_panel(app: &mut App, panel_id: u32, visible: bool) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen");
    screen.recv_set_panel_visibility(ui, panel_id, visible);
}

fn inventory_panel(app: &mut App) -> u32 {
    let shell = app.ui_mut().expect("shell");
    let screen = shell.flow.current_mut().expect("screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen");
    screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == dereth_ui_screens::screens::gameplay::window::INVENTORY_PAGE)
        .expect("the inventory page is in the shipped panel stack")
        .panel_id
}

/// Raise the inventory page and settle the tree.
fn open_the_inventory_page(app: &mut App) {
    seed_inventory(app);
    let panel = inventory_panel(app);
    show_panel(app, panel, true);
    frames(app, 3);
}

/// Send normalized motion/down/up messages through Pump and the input manager, then allow
/// three frames for UI dispatch and selection. This is not OS mouse injection.
fn press_at(app: &mut App, x: i32, y: i32, stamp: u32) {
    let mut pump = dereth_desktop::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let messages = [
        pump.mouse_move_message(f64::from(x), f64::from(y), stamp),
        pump.mouse_button_message(MouseButton::Left, true, stamp + 10)
            .expect("WM_LBUTTONDOWN"),
        pump.mouse_button_message(MouseButton::Left, false, stamp + 20)
            .expect("WM_LBUTTONUP"),
    ];
    for m in messages {
        pump.dispatch(m);
        app.input_manager_mut()
            .expect("real input maps")
            .on_message(m);
    }
    frames(app, 3);
}

fn selected(app: &App) -> Option<ObjectId> {
    app.objects().world.selected
}

fn clear_selection(app: &mut App) {
    app.probe_mut().objects_mut().world.set_selected_object(
        None,
        false,
        &mut dereth_client_model::NullSink,
    );
}

// =================================================================================================
// The control: the open 3-D view selects
// =================================================================================================

/// Positive selection control: the projected chest point hit-tests to `<SBOX>`, selection is
/// cleared, and normalized input selects that chest. This makes later pass-through observations
/// meaningful rather than relying only on a lack of selection.
///
/// In the retail route, the manager hit-tests and updates mouse-over, then delivers mouse down
/// to the accepted element. The viewport wrapper makes itself mouse-visible during setup;
/// selection action 7 sets the selection-search reason and starts object lookup. The test
/// drives that input boundary rather than calling the wrapper or object-found handler.
#[test]
fn a_press_in_the_open_view_selects_the_object_behind_it() {
    let mut app = setup();
    let lateral = lateral_for_column(&app, 200);
    let (chest, name) = place_the_corpus_chest_at(&mut app, lateral);
    let at = chest_pixel(&app, chest);
    let sbox = world_view(&mut app);
    assert_eq!(
        ui_of(&mut app).hit_test_screen(at.0, at.1),
        Some(sbox),
        "the open view at {at:?} hit-tests to <SBOX>"
    );
    clear_selection(&mut app);
    assert_eq!(selected(&app), None, "nothing is selected before the press");
    press_at(&mut app, at.0, at.1, 5_000);
    assert_eq!(
        selected(&app),
        Some(chest),
        "a press on {name:?} at {at:?} selects it: {:?}",
        app.interaction().pick.stats
    );
}

// =================================================================================================
// The gutters, and *why* they are gutters
// =================================================================================================

/// Behaviour: selection.viewport.transparent-panel-gutters-pass-clicks-to-the-world
///
/// Inventory padding passes through to the world, as observed in the reference layout.
/// First check each fixed row-407 sample is inside `<PANS>` and hit-tests to `<SBOX>`. Then put
/// the chest's projected point inside the associated horizontal run, check its hit target and
/// press there. The actual chest press can be on a different row from the fixed sample.
///
/// The retail recursion visits children from tail to head, accepting the first eligible
/// descendant or the element itself when mouse-visible or blocking clicks. Effective mouse
/// visibility combines an explicit request with the behaviour's answer; the base answer is
/// true for a context menu or valid tooltip. The client's widgets may also request mouse
/// visibility. The floating panel, inventory page and three subpanels do not accept their
/// padding. In the shipped layouts BLOCK_CLICKS appears only on hidden `classic_keyboard`
/// 0x100004A8; this test does not rescan every element for that property.
///
/// Adding opacity or a special gutter guard would change these positive hit/selection results.
/// A viewport-clamp change would be caught only if it changes the measured rectangle or picks;
/// this is not an independent audit of every possible clamp configuration.
#[test]
fn the_panel_gutters_hand_the_pointer_to_sbox_exactly_as_retail_does() {
    let mut app = setup();
    open_the_inventory_page(&mut app);
    let sbox = world_view(&mut app);
    let pans = element_of(&mut app, PANEL_STACK);
    let window = ui_of(&mut app).screen_box(pans);

    for (i, (x, y, run)) in GUTTERS.into_iter().enumerate() {
        // ---- the premise: the pixel is inside the panel window at all -------------------------
        assert!(
            window.x0 <= x && x <= window.x1 && window.y0 <= y && y <= window.y1,
            "({x}, {y}) is inside <PANS> {window:?}"
        );
        // ---- the measurement: the hit test hands it to <SBOX> ---------------------------------
        assert_eq!(
            ui_of(&mut app).hit_test_screen(x, y),
            Some(sbox),
            "the hit test at ({x}, {y}) returns <SBOX> through the panel gutter"
        );

        // ---- and a press there really does reach the world ------------------------------------
        let (chest, name, at) = place_chest_in_run(&mut app, x, run);
        assert_eq!(
            ui_of(&mut app).hit_test_screen(at.0, at.1),
            Some(sbox),
            "the chest {name:?} settled at {at:?}, which must still be inside the gutter {run:?}"
        );
        clear_selection(&mut app);
        press_at(
            &mut app,
            at.0,
            at.1,
            6_000 + u32::try_from(i).unwrap() * 500,
        );
        assert_eq!(
            selected(&app),
            Some(chest),
            "a press in the gutter at {at:?} reaches the world, because the hit test said <SBOX>"
        );
        delete_the_chest(&mut app, chest);
    }
}

// =================================================================================================
// The rectangle, in the client and in the dats
// =================================================================================================

/// `<SBOX>` covers the full 800x600 buffer in this layout. The retail viewport walk considers
/// visible objects with edge values 1..4, populated through property 0x52; none of this
/// layout's elements carries one. App derives its effective viewport directly from the
/// viewport element's client rectangle.
///
/// Assert that element's inclusive bounds and panel containment. A normalized top-left press
/// must not increase outside-viewport refusals; that counter equality alone does not prove a
/// new world search began at (0, 0). The other stations provide positive selection controls.
#[test]
fn the_game_viewport_is_the_whole_window_because_no_element_arms_the_clamp_edge() {
    let mut app = setup();
    open_the_inventory_page(&mut app);
    let sbox = world_view(&mut app);
    let b = ui_of(&mut app).screen_box(sbox);
    assert_eq!(
        (b.x0, b.y0, b.x1, b.y1),
        (
            0,
            0,
            i32::try_from(SCREEN.0).unwrap() - 1,
            i32::try_from(SCREEN.1).unwrap() - 1
        ),
        "<SBOX> occupies the whole back buffer in this layout"
    );

    // The retail pick bounds use unsigned coordinates relative to viewport origin and
    // compare them with width/height. Here observe only that the top-left press causes no
    // additional outside-viewport refusal; do not infer a new search from that counter alone.
    let before = app.interaction().pick.stats.outside_viewport;
    press_at(&mut app, 0, 0, 9_000);
    assert_eq!(
        app.interaction().pick.stats.outside_viewport,
        before,
        "the top-left press caused no additional outside-viewport rejection"
    );

    // The panel rectangle lies inside the viewport rectangle. Hit testing determines which
    // element gets a pointer there; this containment check does not inspect other pick gates.
    let pans = element_of(&mut app, PANEL_STACK);
    let w = ui_of(&mut app).screen_box(pans);
    assert!(
        b.x0 <= w.x0 && b.y0 <= w.y0 && w.x1 <= b.x1 && w.y1 <= b.y1,
        "<PANS> {w:?} lies inside <SBOX> {b:?}"
    );
}
