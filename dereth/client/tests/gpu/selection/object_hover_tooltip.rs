//! Hovering a world object names it in the 3-D view's tooltip, moving off it takes the name away,
//! and with the ShowTooltips option off the pick still finds the object but no name is shown.
//! Fixture: a headless App on retail terrain with the shipped gameplay layout; the player and a
//! named chest come from decoded `early-inventory-and-casting` creates, placed in front of the
//! camera and applied as local events (no datagram leaves the process).
//!
//! # How a hover becomes a tooltip
//!
//! Mouse motion and the frame loop arm a geometric pick; the draw-time answer arrives through
//! an object-found notice. On a changed object, the tooltip branch clears text and its enabled
//! flag for a zero ID or disabled ShowTooltips option. Otherwise it queries the appropriate
//! object name with final argument 0. An empty name leaves the tooltip unchanged; a nonempty
//! name sets the viewport's text and tooltip flag (element flag bit 5).
//!
//! With no drag in progress, as tested here, the generic hover path creates the window after
//! pointer rest. Its no-preferences defaults are enabled tooltips, a 0.25 s delay and 10.0 s
//! duration. The hover path also checks the enable preference; per-element property
//! `0x50` can override the rest delay. During a drag, retail instead resets and
//! starts the tooltip directly when the pointer is inside the viewport. The start call's
//! duration argument 0.0 leaves the configured duration unchanged; it is not a delay argument.
//! This file does not exercise the drag or empty-name branches.
//!
//! # The fixture
//!
//! The player's create is relocated and a constructed state update uses the recorded
//! visible-state mask with a fresh stamp. The chest's create keeps its descriptor and name but
//! has its position changed and POSITION flag ensured. A missing device fails the test, as
//! do scene and corpus failures.
//!
//! Pointer motion uses normalized pump messages through the input manager, not OS injection.
//! The test does not directly call the object-found callback or tooltip setters. It checks pick
//! results, tooltip state, window bounds and glyphs in the recording draw backend. The glyph
//! helper selects newly appearing draw-element handles, not all changed text or GPU pixels.
//! Twenty frames are allowed at each station; this is not an elapsed-time measurement of the
//! 0.25 s delay. The test checks the configured default delay separately.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::app::{frames, position};

use std::collections::BTreeSet;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::pick::PickScene;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui::{ElemHandle, UiDrawCmd, UiSystem};
use {
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
};

const SCREEN: (u32, u32) = (800, 600);
/// Recorded visible-state mask that un-hides the player at login; applied here with a fresh stamp.
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
/// Shipped 3-D viewport element that receives the picked object's tooltip text.
const SMART_BOX: dereth_ui::ElementId = dereth_ui_screens::hud::world_view::SMART_BOX;
/// Public ShowTooltips option, represented by mask `0x100` in the first option word.
const SHOW_TOOLTIPS: usize = dereth_client_model::player::option::SHOW_TOOLTIPS;

// =================================================================================================
// The bench: the player on real terrain, with early-inventory-and-casting's chest in front
// =================================================================================================

fn setup() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-object-hover-tooltip-not-created")
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
    {
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(c.on_ground(), "real terrain must support the local body");
        assert_eq!(c.object_id(), id, "the body adopted the server's id");
    }
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

/// Decode early-inventory-and-casting's first named Chest create with a setup. Preserve its
/// descriptor, name and other decoded fields while replacing the position and ensuring its
/// presence flag, then re-encode and apply it locally. This is fixture-derived input, not an
/// unchanged wire packet.
fn place_the_corpus_chest(app: &mut App) -> (ObjectId, String) {
    let corpus = Corpus::shared("early-inventory-and-casting");
    let mut names = Vec::new();
    let mut chosen = None;
    for r in &corpus.blobs {
        if r.dir != Direction::ServerToClient || r.opcode != Opcode::ITEM_CREATE_OBJECT.0 {
            continue;
        }
        let Ok(c) = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..]))
        else {
            continue;
        };
        if !c.0.wdesc.name.is_empty() {
            names.push(c.0.wdesc.name.clone());
        }
        if c.0.wdesc.name.contains("Chest")
            && c.0.physicsdesc.bitfield & dereth_protocol::types::physicsdesc::flags::SETUP != 0
            && chosen.is_none()
        {
            chosen = Some(c);
        }
    }
    // **The instrument has to be able to look.** An empty search is only a negative result once
    // the census proves the search ran over real creates.
    assert!(
        names.len() > 5,
        "early-inventory-and-casting decoded only {} named creates: {names:?}",
        names.len()
    );
    let mut create = chosen.unwrap_or_else(|| {
        panic!("early-inventory-and-casting has no named chest with a SETUP among {names:?}")
    });

    let here = position(app);
    // Four metres straight ahead, on the ground the body is standing on: inside the chase
    // camera's cone and well clear of the body itself.
    let origin = dereth_physics::math::localtoglobal(&here.frame, Vec3::new(0.0, 4.0, 0.0));
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
    eprintln!("object_hover_tooltip: chest {id:?} {name:?} at {origin:?}");
    assert_eq!(
        app.objects()
            .world
            .weenie(id)
            .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate)),
        Some(name.clone()),
        "appropriate-name lookup returns the fixture's chest name"
    );
    (id, name)
}

// =================================================================================================
// Aiming: invert the selection-ray projection
// =================================================================================================

fn pixel_of(local: Vec3, viewport: (u32, u32), fov_y_rad: f32) -> (f32, f32) {
    let half_w = (viewport.0 as f32 - 1.0) * 0.5;
    let half_h = (viewport.1 as f32 - 1.0) * 0.5;
    let vdst = half_h / math::tanf(fov_y_rad * 0.5);
    let k = vdst / local.y;
    (half_w + local.x * k, half_h - local.z * k)
}

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

/// Aim half a metre above the chest's scene-frame origin to intersect its body rather than
/// the ground. A scene frame alone is not evidence that a part was submitted or a pixel drawn.
fn chest_pixel(app: &App, chest: ObjectId) -> (i32, i32) {
    let scene = app.world_scene().unwrap();
    let f = scene
        .server_object_frame(chest)
        .expect("the chest has a SceneObject — its `0xF745` carried a SETUP");
    aim(app, Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.5))
}

// =================================================================================================
// The pointer, and what it draws
// =================================================================================================

/// Send a normalized pointer-move message through the pump and input manager, without
/// synthesizing a button press or directly invoking the hover/tooltip helpers.
fn move_pointer_to(app: &mut App, x: i32, y: i32, at: u32) {
    let mut pump = dereth_desktop::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let message = pump.mouse_move_message(f64::from(x), f64::from(y), at);
    pump.dispatch(message);
    app.input_manager_mut()
        .expect("real input maps")
        .on_message(message);
}

/// Move once and advance the requested number of frames without further pointer motion.
/// Picking, notice routing and delayed hover run through production updates, which take several
/// frames; a frame count is not a fixed wall-clock duration.
fn rest_pointer_at(app: &mut App, x: i32, y: i32, at: u32, frames_after: usize) {
    move_pointer_to(app, x, y, at);
    frames(app, frames_after);
}

fn ui_of(app: &mut App) -> &mut UiSystem {
    &mut app.ui_mut().expect("the shell is up").ui
}

fn world_view(app: &mut App) -> ElemHandle {
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
        .get_child_recursive(root, SMART_BOX)
        .expect("<SBOX> is in the shipped layout")
}

fn draw_list(app: &mut App) -> Vec<UiDrawCmd> {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui_of(app).draw(&mut back);
    back.calls
}

/// Glyphs from draw commands whose element handle was absent from the previous command list.
/// Existing-element text changes are excluded. This records submitted UI glyph data, not pixels.
fn glyphs_that_appeared(before: &[UiDrawCmd], after: &[UiDrawCmd]) -> String {
    let known: BTreeSet<ElemHandle> = before.iter().map(|c| c.who).collect();
    after
        .iter()
        .filter(|c| !known.contains(&c.who))
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        })
        .collect()
}

// =================================================================================================
// The acceptance
// =================================================================================================

/// Behaviour: selection.tooltip.hovering-an-object-names-it-and-leaving-clears-it
///
/// Hover the chest, move away, then disable ShowTooltips and hover it again. Require the name
/// in new-element glyph commands only in the enabled case, verify window placement and removal,
/// and keep the viewport mouse-visible after clearing. Disabling the option must leave the pick
/// working while suppressing additional tooltip text updates and newly appearing name glyphs.
#[test]
fn hovering_a_chest_names_it_and_moving_off_takes_the_name_away() {
    let mut app = setup();
    let (chest, name) = place_the_corpus_chest(&mut app);
    let sbox = world_view(&mut app);

    // ---- premises --------------------------------------------------------------------------
    assert!(
        app.objects().world.player_system.options.get(SHOW_TOOLTIPS),
        "ShowTooltips is default-on (mask 0x100 in the default character options)"
    );
    assert_eq!(
        ui_of(&mut app).tooltip_delay(),
        0.25,
        "the no-preferences tooltip delay is 0.25 seconds"
    );
    assert!(
        !ui_of(&mut app).tooltip_on(sbox),
        "the viewport starts with its tooltip flag clear"
    );

    let at = chest_pixel(&app, chest);
    let away = {
        // Three metres to the camera's right at the same depth; the later pick must find no object.
        let scene = app.world_scene().unwrap();
        let viewer = PickScene::viewer(&scene);
        let f = scene
            .server_object_frame(chest)
            .expect("the chest is in the scene");
        let world = Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.5);
        let local = dereth_physics::math::globaltolocal(&viewer, world);
        let beside = dereth_physics::math::localtoglobal(
            &viewer,
            Vec3::new(local.x + 3.0, local.y, local.z),
        );
        aim(&app, beside)
    };
    assert_ne!(at, away, "the two pointer positions are different pixels");

    // Verify the chest aim actually hits the shipped viewport rather than a HUD panel before
    // attributing any tooltip result to world-hover routing.
    {
        let hit = ui_of(&mut app).hit_test_screen(at.0, at.1);
        assert_eq!(
            hit,
            Some(sbox),
            "the pointer at {at:?} lands on <SBOX>, not on a HUD panel"
        );
    }

    // ---- 1. hover the chest ------------------------------------------------------------------
    let before = draw_list(&mut app);
    rest_pointer_at(&mut app, at.0, at.1, 1_000, 20);

    assert_eq!(
        app.interaction().pick.click_object().0,
        chest,
        "the geometric pick under the pointer answered with the chest"
    );
    // **The rejecting assertion.** A notice that sets no tooltip leaves object_tooltips_set at
    // zero and the viewport tooltip flag clear.
    assert!(
        app.interaction().stats.object_tooltips_set >= 1,
        "the object-found notice produced tooltip text: {:?}",
        app.interaction().stats.object_tooltips_set
    );
    assert!(
        ui_of(&mut app).tooltip_on(sbox),
        "the viewport tooltip flag is enabled"
    );
    assert_eq!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        Some(name.clone()),
        "the viewport tooltip text is the chest name"
    );

    // ---- 2. the name appears in new draw commands and the window has valid bounds ------------
    let after = draw_list(&mut app);
    let drawn = glyphs_that_appeared(&before, &after);
    assert!(
        drawn.contains(&name),
        "new draw-command glyphs contain the chest's name; new glyphs were {drawn:?}"
    );
    let tip = ui_of(&mut app)
        .tooltip_element()
        .expect("the tooltip window exists");
    {
        let b = ui_of(&mut app).screen_box(tip);
        assert!(
            b.is_valid() && b.width() > 1 && b.height() > 1,
            "the tooltip has a rectangle: {b:?}"
        );
        // Retail positioning adds 0x20 to the pointer and clamps into the display. This
        // station checks only a valid nontrivial rectangle down-and-right of the pointer.
        assert!(
            b.x0 >= at.0 && b.y0 >= at.1,
            "the tooltip sits down-and-right of the pointer at {at:?}: {b:?}"
        );
    }

    // ---- 3. move off the chest ---------------------------------------------------------------
    let cleared_before = app.interaction().stats.object_tooltips_cleared;
    rest_pointer_at(&mut app, away.0, away.1, 4_000, 20);
    assert_eq!(
        app.interaction().pick.click_object().0,
        ObjectId(0),
        "the pick beside the chest finds no object"
    );
    assert!(
        app.interaction().stats.object_tooltips_cleared > cleared_before,
        "the tooltip-cleared counter increased after leaving the chest"
    );
    assert!(
        !ui_of(&mut app).tooltip_on(sbox),
        "leaving the chest cleared the tooltip flag"
    );
    assert_eq!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        None
    );
    assert_eq!(
        ui_of(&mut app).tooltip_element(),
        None,
        "the tooltip window is gone"
    );

    // Clearing tooltip text recomputes mouse visibility. If the viewport depended only on
    // its tooltip for mouse hits, clearing would make world clicks fall through. Recheck the
    // same chest-aim hit to guard that regression; this does not send an actual click.
    {
        let hit = ui_of(&mut app).hit_test_screen(at.0, at.1);
        assert_eq!(
            hit,
            Some(sbox),
            "the viewport still catches the pointer after tooltip clearing"
        );
    }

    // ---- 4. the option off -------------------------------------------------------------------
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .options
        .set(SHOW_TOOLTIPS, false);
    let before = draw_list(&mut app);
    let set_before = app.interaction().stats.object_tooltips_set;
    rest_pointer_at(&mut app, at.0, at.1, 7_000, 20);
    assert_eq!(
        app.interaction().pick.click_object().0,
        chest,
        "the pick still finds the chest — `ShowTooltips` gates the naming, not the search"
    );
    assert_eq!(
        app.interaction().stats.object_tooltips_set,
        set_before,
        "the disabled option produced no additional tooltip-text update"
    );
    assert!(
        !ui_of(&mut app).tooltip_on(sbox),
        "no tooltip flag with the option off"
    );
    let after = draw_list(&mut app);
    let drawn = glyphs_that_appeared(&before, &after);
    assert!(
        !drawn.contains(&name),
        "with ShowTooltips off no new draw-command glyphs name the chest; drew {drawn:?}"
    );
}
