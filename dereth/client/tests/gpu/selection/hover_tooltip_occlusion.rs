//! A 3-D hover names only what the frame drew: a hover names a chest with no prior click, an
//! object behind an interior wall or under an opaque panel never names itself, a door placed in
//! a non-visible building cell is not submitted, and a hover during a drag names the object at
//! once over the view and never over the panel. Fixture: a headless App on `DEFAULT_LANDBLOCK`
//! terrain with the shipped gameplay layout, and the player and a chest from
//! `early-inventory-and-casting` applied as local events.
//!
//! # The fixture
//!
//! `early-inventory-and-casting` supplies the player's identity and decoded create, relocated
//! onto this terrain; a constructed `0xF74B` applies the recorded visible-state mask with a fresh
//! stamp. A named chest create is also relocated; the door station additionally replaces its
//! setup. A missing device fails the test.
//!
//! The tooltip-content stations inspect glyphs on newly appearing element handles in a
//! recording draw backend, alongside tooltip state and pick results. This is draw-command
//! evidence, not GPU pixel readback. The door station counts submitted parts; the timing station
//! compares tooltip-window existence after two frames, without reading glyphs or elapsed time.
//!
//! Hover coordinates arrive as normalized pump mouse-move messages through the input manager.
//! Starting an inventory drag also uses direct UI mouse calls. Neither path injects OS input,
//! and the tests do not directly invoke the tooltip setters or object-found callback. Corpus
//! messages are decoded and applied as local events; no datagram leaves the process.

#![cfg(gpu)]

use crate::common::app::{frames, position};
use dereth_scene::world_scene::SceneReads;

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
use {dereth_client_runtime::scene::SceneConfig, dereth_world_data::landblock::DEFAULT_LANDBLOCK};

const SCREEN: (u32, u32) = (800, 600);
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
const SMART_BOX: dereth_ui::ElementId = dereth_ui_screens::hud::world_view::SMART_BOX;
const SHOW_TOOLTIPS: usize = dereth_client_model::player::option::SHOW_TOOLTIPS;

fn setup() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-hover-tooltip-occlusion-not-created")
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

fn place_the_corpus_chest(app: &mut App) -> (ObjectId, String) {
    place_the_corpus_chest_at(app, 0.0)
}

fn place_the_corpus_chest_at(app: &mut App, lateral: f32) -> (ObjectId, String) {
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
    assert!(
        names.len() > 5,
        "early-inventory-and-casting decoded only {} named creates: {names:?}",
        names.len()
    );
    let mut create = chosen.unwrap_or_else(|| {
        panic!("early-inventory-and-casting has no named chest with a SETUP among {names:?}")
    });
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
// A hover needs no focus click
// =================================================================================================

/// **No focus click needed.** An out-of-viewport pointer must not disable later hover.
/// Captured mouse motion can provide such coordinates; a docked viewport can also leave part
/// of the client area outside the 3-D rectangle.
///
/// The retail loop checks for a search reason at least 1, then sets reason 1 before requesting
/// a hover pick. The object-found notice unconditionally clears that reason. A refused unsigned
/// viewport check instead clears the selection cursor and returns without arming a search, so
/// no notice arrives and the reason can stay latched at 1. Later loop iterations then return.
///
/// In retail, a world click recovers because its gate allows reasons below examine, including
/// 1; hovering a UI item recovers through a synchronous found-object notice. The client's hover
/// caller restores its previous reason when no search is armed. This station checks that
/// restoration and then finds the chest without either recovery gesture.
#[test]
fn a_hover_names_a_chest_with_no_prior_click() {
    let mut app = setup();
    let (chest, name) = place_the_corpus_chest(&mut app);
    let sbox = world_view(&mut app);
    let at = chest_pixel(&app, chest);

    // The instrument has to be able to look: the pointer must reach the viewport at all.
    assert_eq!(
        ui_of(&mut app).hit_test_screen(at.0, at.1),
        Some(sbox),
        "the pointer at {at:?} lands on <SBOX>"
    );
    assert!(
        app.objects().world.player_system.options.get(SHOW_TOOLTIPS),
        "`ShowTooltips` is default-on"
    );

    // ---- the pointer visits a coordinate outside the 3-D viewport ---------------------------
    // Retail mouse-move decoding sign-extends both packed 16-bit coordinates and stores
    // them without clamping. The normalized messages below provide the same negative-coordinate
    // case that captured pointer motion outside the window can produce.
    let rect = {
        let b = ui_of(&mut app).screen_box(sbox);
        (b.x0, b.y0, b.x1, b.y1)
    };
    assert!(
        rect.0 <= 0 && rect.1 <= 0,
        "<SBOX> starts at the window's corner in the shipped layout: {rect:?}"
    );
    rest_pointer_at(&mut app, -5, -7, 1_000, 6);
    assert!(
        app.interaction().pick.stats.outside_viewport >= 1,
        "the object lookup rejected an out-of-viewport search: {:?}",
        app.interaction().pick.stats
    );

    // **The rejecting assertion.** The refused search must not have latched the gate.
    assert_eq!(
        app.interaction().search_reason(),
        dereth_client_runtime::interaction::SearchReason::None,
        "a search that armed nothing leaves no gesture in flight"
    );

    // ---- and now the chest, with no click anywhere in this test ------------------------------
    let before = draw_list(&mut app);
    rest_pointer_at(&mut app, at.0, at.1, 3_000, 20);
    assert_eq!(
        app.interaction().pick.click_object().0,
        chest,
        "the hover pick runs again without a prior click"
    );
    assert!(
        ui_of(&mut app).tooltip_on(sbox),
        "the object-found notice enabled the viewport tooltip"
    );
    assert_eq!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        Some(name.clone()),
        "the viewport tooltip text is the chest name"
    );
    let after = draw_list(&mut app);
    let drawn = glyphs_that_appeared(&before, &after);
    assert!(
        drawn.contains(&name),
        "the chest's name appears in new draw-command glyphs with no prior click; new glyphs were {drawn:?}"
    );
}

// =================================================================================================
// Objects behind interior walls
// =================================================================================================

/// Find a sampled point inside the cell BSP and outside its solid physics BSP, then transform
/// it to block-local metres. A Holtburg house shares one frame origin across eighteen cells,
/// so the search samples each cell's own volume. The sample is an interior-volume test, not
/// proof of ground support.
fn point_in(store: &dereth_dat::RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let d = dereth_world_data::env_cells::EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = dereth_world_data::env_cells::physics_geometry(&d);
    let bsp = g.cell_bsp.as_ref()?;
    for zi in -8i32..=16 {
        for i in -24i32..=24 {
            for j in -24i32..=24 {
                #[allow(clippy::cast_precision_loss)] // small loop counters
                let local = Vec3::new(i as f32 * 0.5, j as f32 * 0.5, zi as f32 * 0.5);
                if !bsp.point_inside_cell_bsp(local) {
                    continue;
                }
                if g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(local))
                {
                    continue;
                }
                return Some(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    None
}

/// Choose a loaded interior cell not reached by the outdoor portal walk, with a sampled point
/// projected inside the camera margins at depth 3..40 m. The wall scenario is identified by
/// cell traversal, not an independent camera-to-point wall-intersection measurement.
fn a_cell_behind_a_wall(app: &App, store: &dereth_dat::RetailDatStore) -> Option<(u32, Vec3)> {
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let here = position(app);
    // Reached cells do not exercise this exclusion, even if some points inside them could
    // still be occluded. Exclude them from the candidate set.
    let seen = SceneReads::drawn_cells(&scene).expect("the frame's cell walk has an answer");
    let mut best: Option<(f32, u32, Vec3)> = None;
    for d in dereth_world_data::env_cells::EnvCellLoader::new().load_block(store, DEFAULT_LANDBLOCK)
    {
        if seen.contains(&d.id.0) {
            continue;
        }
        let Some(p) = point_in(store, d.id.0) else {
            continue;
        };
        // Use the block-local offset from the body to estimate the point relative to the
        // viewer before creating the object. The final hover aim uses its actual render frame.
        let world = Vec3::new(
            viewer.origin.x + (p.x - here.frame.origin.x),
            viewer.origin.y + (p.y - here.frame.origin.y),
            viewer.origin.z + (p.z - here.frame.origin.z),
        );
        let local = dereth_physics::math::globaltolocal(&viewer, world);
        if local.y < 3.0 || local.y > 40.0 {
            continue;
        }
        let (px, py) = pixel_of(local, SCREEN, fov);
        #[allow(clippy::cast_precision_loss)]
        if px < 60.0 || px > 740.0 || py < 60.0 || py > 540.0 {
            continue;
        }
        // Prefer the smallest projected forward depth among eligible samples.
        if best.as_ref().is_none_or(|b| local.y < b.0) {
            best = Some((local.y, d.id.0, p));
        }
    }
    best.map(|(_, c, p)| (c, p))
}

/// `early-inventory-and-casting`'s chest, placed in `cell` at `origin` (the block's own metres)
/// instead of on the ground in front of the body.
fn place_the_corpus_item_in(
    app: &mut App,
    cell: u32,
    origin: Vec3,
    setup: Option<u32>,
) -> (ObjectId, String) {
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
    create.0.physicsdesc.position = Some(wire_position(cell, origin, &here));
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    if let Some(setup) = setup {
        create.0.physicsdesc.setup_id = Some(setup);
    }
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

fn place_the_corpus_chest_in(app: &mut App, cell: u32, origin: Vec3) -> (ObjectId, String) {
    place_the_corpus_item_in(app, cell, origin, None)
}

/// **Submission control.** Retail building rendering visits leaf cells within the shell
/// BSP traversal, and interior-object rendering visits the reached cell list. It does not add
/// a final global pass over every interior object. An object registered only in an unreached
/// interior cell must therefore contribute no submitted parts.
///
/// The recorded chest create is given the reported door setup and a sampled interior position,
/// then follows decoded-object placement and world drawing. The test requires an actual body in
/// exactly that hidden cell, with exactly one present shadow registration, before checking zero
/// submissions. This distinguishes a culling result from failed placement; it makes no pixel claim.
#[test]
fn a_placed_door_in_a_non_visible_building_cell_is_not_submitted_after_the_shell() {
    const DOOR_SETUP: u32 = 0x0200_024F;

    let mut app = setup();
    let store = dereth_dat::testing::open_store_or_fail();
    let Some((cell, origin)) = a_cell_behind_a_wall(&app, &store) else {
        panic!("the loaded Holtburg window has no interior cell behind a wall")
    };
    let (door, _) = place_the_corpus_item_in(&mut app, cell, origin, Some(DOOR_SETUP));

    let scene = app.world_scene().expect("a world scene");
    let drawn = SceneReads::drawn_cells(&scene).expect("the frame's cell walk has an answer");
    assert!(
        !drawn.contains(&cell),
        "the outdoor building-portal walk reached {cell:#010X}, so this is not an unreached-cell control"
    );
    let physics = &scene.character.as_ref().expect("the local body").world;
    let door_body = physics
        .by_object_id(door)
        .and_then(|h| physics.get(h))
        .expect("the placed door has a collision body");
    let physics_cell = door_body.cell.map(|cell| cell.0);
    assert_eq!(
        physics_cell,
        Some(cell),
        "the door did not enter the interior cell, so a missing draw would only prove failed placement"
    );
    assert_eq!(
        door_body
            .shadow_objects
            .iter()
            .filter(|shadow| shadow.cell_present)
            .map(|shadow| shadow.cell_id.0)
            .collect::<Vec<_>>(),
        vec![cell],
        "the door is wholly registered in the hidden cell; this is not the separate boundary-crossing case"
    );

    let submitted = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(door))
        .count();
    assert_eq!(
        submitted, 0,
        "{submitted} door parts from non-visible cell {cell:#010X} were submitted"
    );

    // Retail boundary-crossing objects register in every overlapped cell; a reached entry
    // can offer their parts for drawing. Add one such registration directly while keeping this
    // door and origin fixed. This tests consumption of the physics overlap set, not automatic
    // boundary discovery. The retail renderer also clips a shadow to that cell's planes; this
    // submission-count control does not establish clipped pixel coverage.
    let visible_cell = drawn
        .iter()
        .copied()
        .find(|cell| !dereth_physics::landdefs::is_outdoors(dereth_primitives::CellId(*cell)))
        .expect("the outdoor frame reached an interior cell through a building opening");
    {
        let mut scene = app.world_scene_mut().expect("a world scene");
        let physics = &mut scene.character.as_mut().expect("the local body").world;
        let handle = physics
            .by_object_id(door)
            .expect("the placed door has a collision body");
        physics
            .get_mut(handle)
            .expect("the door body remains live")
            .shadow_objects
            .push(dereth_physics::obj::ShadowObj {
                cell_id: dereth_primitives::CellId(visible_cell),
                cell_present: true,
            });
    }
    frames(&mut app, 1);
    let submitted = app
        .world_scene()
        .expect("a world scene")
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(door))
        .count();
    assert!(
        submitted > 0,
        "an object shadowing reached cell {visible_cell:#010X} was culled only because its origin cell {cell:#010X} was hidden"
    );
}

/// Behaviour: selection.tooltip.a-hover-names-only-what-the-frame-drew
///
/// **Interior walls hide names.** In the retail renderer, the reached cell list drives
/// each cell's object traversal, which offers object parts to the selection-ray test. An object
/// confined to a cell absent from that traversal is never offered. Eligibility is a draw-path
/// decision, not proof that the GPU produced a visible pixel for every eligible part.
///
/// A sweep that considers every positioned presence names a chest through the house wall.
/// This station selects an unreached interior cell, aims at the relocated
/// chest, and requires a rejected pick, the undrawn-cell counter, no chest tooltip text and no
/// new draw-command glyphs naming it. The cell list is shared production evidence, not an
/// independent wall raycast.
#[test]
fn an_object_behind_an_interior_wall_never_names_itself() {
    let mut app = setup();
    let store = dereth_dat::testing::open_store_or_fail();
    let sbox = world_view(&mut app);
    let Some((cell, origin)) = a_cell_behind_a_wall(&app, &store) else {
        panic!("the loaded Holtburg window has no interior cell in front of the camera")
    };
    assert!(
        !dereth_physics::landdefs::is_outdoors(dereth_primitives::CellId(cell)),
        "{cell:#010X} is an interior cell"
    );
    let (chest, name) = place_the_corpus_chest_in(&mut app, cell, origin);
    assert_eq!(
        app.objects()
            .presence(chest)
            .and_then(|p| p.position)
            .map(|p| p.cell.0),
        Some(cell),
        "the chest stayed in the interior cell it was created in"
    );

    // ---- premises: the chest cell is unreached and the aim ray points at the chest ------------
    let drawn = SceneReads::drawn_cells(&app.world_scene().unwrap());
    assert!(
        drawn.is_some(),
        "the frame's cell walk has an answer at all"
    );
    assert!(
        !drawn.as_ref().unwrap().contains(&cell),
        "the frame's cell walk did not reach {cell:#010X}. Walk was {:?}",
        drawn.as_ref().unwrap()
    );
    let at = chest_pixel(&app, chest);
    assert_eq!(
        ui_of(&mut app).hit_test_screen(at.0, at.1),
        Some(sbox),
        "the pointer at {at:?} hits the 3-D view"
    );

    // ---- the hover ---------------------------------------------------------------------------
    let before = draw_list(&mut app);
    rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);

    // **The rejecting assertions.** A sweep that ignores the drawn cells reaches the chest
    // through the wall and the object-found notice supplies its name.
    let after = draw_list(&mut app);
    let glyphs = glyphs_that_appeared(&before, &after);
    assert!(
        !glyphs.contains(&name),
        "no newly appearing draw-command glyphs named the chest; drew {glyphs:?}"
    );
    assert_ne!(
        app.interaction().pick.click_object().0,
        chest,
        "the pick did not select the chest in an unreached interior cell"
    );
    assert!(
        app.interaction().pick.stats.objects_in_undrawn_cells >= 1,
        "and the sweep says why it did not: {:?}",
        app.interaction().pick.stats
    );
    assert_ne!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        Some(name.clone()),
        "the viewport tooltip does not name the chest in the unreached cell"
    );
}

// =================================================================================================
// Objects behind the HUD
// =================================================================================================

/// Seed three synthetic loose items in this player's pack so the inventory grid has a drag
/// source. The test starts a grid-icon drag, which exercises the reported shared drag state;
/// it does not pick an equipped mesh directly from the paper doll.
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

/// The shipped gameplay screen's panel stack: every page shares one docked rectangle on the right.
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

/// The inventory page ID used to expose the grid drag source.
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

/// The screen box of the inventory page — the opaque sibling this station hides the chest behind.
fn inventory_page_box(app: &mut App) -> dereth_ui::Box2D {
    let h = {
        let shell = app.ui_mut().expect("shell");
        let root = *shell
            .flow
            .current()
            .expect("screen")
            .roots()
            .first()
            .expect("root");
        shell
            .ui
            .get_child_recursive(
                root,
                dereth_ui_screens::screens::gameplay::window::INVENTORY_PAGE,
            )
            .expect("the inventory page is in the shipped layout")
    };
    ui_of(app).screen_box(h)
}

/// The centre of inventory grid slot `n`, in screen coordinates.
fn grid_slot_centre(app: &mut App, n: usize) -> (i32, i32) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen");
    let grid = screen
        .inventory
        .item_list
        .as_ref()
        .expect("the inventory grid");
    let h = grid.slots.get(n).expect("a slot").handle;
    let (ox, oy) = ui.screen_origin(h);
    let b = ui.node(h).expect("alive").region.box_;
    (ox + b.width() / 2, oy + b.height() / 2)
}

/// Press an inventory grid icon and move eight pixels in each axis past the drag threshold.
/// Require a live drag element before relying on the drag-specific tooltip branch.
fn pick_up_an_icon(app: &mut App) {
    let from = grid_slot_centre(app, 0);
    {
        let ui = ui_of(app);
        ui.mouse_move(LocalTime(1.0), from.0, from.1);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, from.0, from.1);
        ui.mouse_move(LocalTime(1.05), from.0 + 8, from.1 + 8);
    }
    frames(app, 1);
    assert!(
        ui_of(app).drag_state().element.is_some(),
        "the icon must be in flight or this station measures nothing"
    );
}

/// The lateral offset, in the player's own frame, that puts a point 4 m ahead of him nearest to
/// screen column `target_x`. Solved against the live camera before anything is placed, so the
/// chest is created once and in the right place.
#[allow(clippy::cast_precision_loss)]
fn lateral_for_column(app: &App, target_x: i32) -> f32 {
    let here = position(app);
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let mut best = (f32::MAX, 0.0f32);
    for i in -160i32..=160 {
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

/// **The HUD hides names.** The retail drag-tooltip branch requires an active drag,
/// subtracts the viewport origin from the mouse coordinates and unsigned-compares both axes
/// against the viewport dimensions before starting a zero-delay tooltip.
///
/// Retail viewport construction begins with display dimensions and shrinks one of four edges
/// for each docked object before installing the device rectangle. Here the shipped `<SBOX>`
/// retains the full 800x600 box and its layout does not arm clamp-edge property `0x52`.
/// `App::pointer_over_game_view` uses hit-test ancestry to supply the HUD exclusion: the panel
/// is a later sibling of `<SBOX>` under `0x10000495` and wins the hit.
///
/// The station verifies the projected chest falls in the panel rectangle and does not hit the
/// viewport, then drags a grid icon across that point. New glyphs, tooltip text and the refusal
/// counter establish the exclusion without a pixel readback or a paper-doll mesh gesture.
#[test]
fn an_object_under_an_opaque_panel_never_names_itself() {
    let mut app = setup();
    // Column 785 in the pre-placement solve lands the settled chest at ~716, inside the panel.
    let dx = lateral_for_column(&app, 785);
    let (chest, name) = place_the_corpus_chest_at(&mut app, dx);
    let sbox = world_view(&mut app);
    let at = chest_pixel(&app, chest);

    seed_inventory(&mut app);
    let panel = inventory_panel(&mut app);
    show_panel(&mut app, panel, true);
    frames(&mut app, 3);

    // ---- the premises: the chest really is behind the panel ----------------------------------
    let page = inventory_page_box(&mut app);
    assert!(
        page.x0 <= at.0 && at.0 <= page.x1 && page.y0 <= at.1 && at.1 <= page.y1,
        "the chest at {at:?} is behind the inventory page {page:?}"
    );
    assert_ne!(
        ui_of(&mut app).hit_test_screen(at.0, at.1),
        Some(sbox),
        "and the pointer there lands on the panel, not on <SBOX>"
    );
    pick_up_an_icon(&mut app);

    // ---- the hover, with the drag in flight --------------------------------------------------
    let before = draw_list(&mut app);
    {
        let ui = ui_of(&mut app);
        ui.mouse_move(LocalTime(1.2), at.0, at.1);
    }
    move_pointer_to(&mut app, at.0, at.1, 2_000);
    frames(&mut app, 6);

    // **The rejecting assertions.** Without the HUD exclusion the chest's name becomes
    // `<SBOX>`'s tooltip text and its glyphs draw at the pointer, over the panel.
    let after = draw_list(&mut app);
    let drawn = glyphs_that_appeared(&before, &after);
    assert!(
        !drawn.contains(&name),
        "no newly appearing draw-command glyphs named the chest; new glyphs were {drawn:?}"
    );
    assert_ne!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        Some(name.clone()),
        "the viewport tooltip must not name an object covered by the panel"
    );
    assert!(
        app.interaction().stats.hover_searches_under_the_hud >= 1,
        "the hover search was refused for a pointer outside the game view: {} refusals",
        app.interaction().stats.hover_searches_under_the_hud
    );
}

// =================================================================================================
// Immediate drag tooltip versus delayed ordinary hover
// =================================================================================================

/// **Does dragging suppress hover entirely?** No: the retail active-drag branch resets and
/// starts the tooltip directly when the pointer is inside the viewport. Its duration argument
/// 0.0 leaves the configured duration unchanged; the direct call bypasses ordinary hover's
/// default 0.25 s rest delay. The client keeps that immediate drag branch.
///
/// At the same chest position this station observes no tooltip window after two frames without
/// a drag, then an existing window after two frames with a drag. It verifies text state but does
/// not measure elapsed seconds, exact first-frame creation or glyph output. The separate HUD
/// station covers the panel exclusion named in this test's function name.
#[test]
fn a_hover_during_a_drag_names_the_object_at_once_over_the_view_and_never_over_the_panel() {
    let mut app = setup();
    let (chest, name) = place_the_corpus_chest(&mut app);
    let sbox = world_view(&mut app);
    let at = chest_pixel(&app, chest);
    seed_inventory(&mut app);
    let panel = inventory_panel(&mut app);
    show_panel(&mut app, panel, true);
    frames(&mut app, 3);
    assert_eq!(
        ui_of(&mut app).hit_test_screen(at.0, at.1),
        Some(sbox),
        "this chest is in the open 3-D view at {at:?}, not behind the panel"
    );

    // ---- control: no drag, so ordinary hover must wait for its 0.25 s delay -----------------
    rest_pointer_at(&mut app, at.0, at.1, 1_000, 2);
    assert!(
        ui_of(&mut app).tooltip_on(sbox),
        "the first pick enables the viewport tooltip even without a drag"
    );
    assert_eq!(
        ui_of(&mut app).tooltip_element(),
        None,
        "with no drag the tooltip window still waits after two frames"
    );

    // ---- the drag: the same two frames, and the window is already up -------------------------
    pick_up_an_icon(&mut app);
    // Move away and back so the found-object ID changes and the tooltip update runs again
    // with a drag in progress, rather than taking its unchanged-object early exit.
    {
        let ui = ui_of(&mut app);
        ui.mouse_move(LocalTime(2.0), 8, 560);
    }
    move_pointer_to(&mut app, 8, 560, 3_000);
    frames(&mut app, 2);
    {
        let ui = ui_of(&mut app);
        ui.mouse_move(LocalTime(2.1), at.0, at.1);
    }
    move_pointer_to(&mut app, at.0, at.1, 3_100);
    frames(&mut app, 2);
    assert_eq!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        Some(name.clone()),
        "the drag's own notice named the chest"
    );
    assert!(
        ui_of(&mut app).tooltip_element().is_some(),
        "with a drag in flight the tooltip window exists after the same two-frame wait"
    );
}
