//! From an indoor frame, an outdoor object is selectable only if the frame drew it: from a sealed
//! interior (no outdoor view) an outdoor chest is neither named nor selectable, and from an
//! interior whose opening drew the chest it still is. Fixture: a headless App standing the body
//! inside a sealed building (`0x17460109`) and inside Holtburg's windowed house (`0xA9B40146`),
//! with the player and a chest from `early-inventory-and-casting` applied as local events.
//!
//! # The selection candidate path
//!
//! The retail client selects one of two exclusive render arms from the viewer's own cell. An
//! outdoor viewer draws the landscape; an indoor viewer draws the interior. The indoor arm does
//! not draw the landscape itself, so its only possible outdoor pass comes from the interior portal
//! walk.
//!
//! ### The portal walk's outdoor pass is gated
//!
//! The portal walk tests `outside_view.view_count`. Zero skips the landscape draw, alpha-list
//! flush, environment-cell frame-stamp increment, depth-only clear, and the first loop over the
//! visible outdoor cells' meshes. The unconditional tail still handles portal stamps and dummy
//! cell drawing, but no land-cell object list is submitted. A positive count runs the full outdoor
//! block before that tail.
//!
//! ### Mesh submission is the only offer to the ray
//!
//! Both the portal and non-portal mesh arms run view-cone rejection, offer the surviving geometry
//! to the selection ray, and then perform the internal draw in that order. A mesh that is never
//! submitted is never offered to the ray — **only objects that are actually drawn can be picked**
//! — and a sealed interior submits no outdoor object at all.
//!
//! `WorldScene::draw_object_pass`'s `wanted_light` applies the gate for the **draw**: on an
//! unsplit indoor frame (`ObjectPhase::All`, `viewer_inside`) an outdoor object yields `None` and
//! is not submitted. The pick sweep must follow the same decision, not only restrict interior
//! cells against `PickScene::drawn_cells`. The sealed station asserts all four conditions of
//! that case as premises before the pick is believed.

// This suite drives a real GPU-backed `App`.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::collections::BTreeSet;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::pick::PickScene;
use dereth_client::world::SceneConfig;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui::{ElemHandle, UiDrawCmd, UiSystem};

const SCREEN: (u32, u32) = (800, 600);
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
const SMART_BOX: dereth_ui::ElementId = dereth_ui_screens::hud::world_view::SMART_BOX;

/// A two-cell **sealed** building in a surface landblock: no portal chain out of it carries
/// `other_cell_id == 0xFFFFFFFF`, which produces `outside_view.view_count == 0` and skips the
/// outdoor mesh pass. Its floor sits 1.0 m above the landblock's own ground height, so the
/// terrain outside is at eye level rather than overhead.
///
/// It meets the predicate "a block with real terrain, some interiors that do reach the outdoors
/// and some that do not, and a standable point in one of the latter".
const SEALED_BLOCK: u16 = 0x1746;
const SEALED_CELL: u32 = 0x1746_0109;

/// Holtburg's windowed house at `0xA9B40146`, where `outside_view.view_count > 0`, so the retail
/// client runs the outdoor pass through the opening. The control: what the frame draws must stay
/// selectable.
const OPEN_BLOCK: u16 = 0xA9B4;
const OPEN_CELL: u32 = 0xA9B4_0146;

/// The id the corpus chest is created under, and the base for the control's spread-out attempts.
const CORPUS_CHEST: ObjectId = ObjectId(0x7DA5_5005);
const CONTROL_CHEST: ObjectId = ObjectId(0x7DA5_5010);

fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        assert!(app.frame());
    }
}

fn position(app: &App) -> Position {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
}

/// A point a body could stand at inside `cell`, in the landblock's own metres: inside the cell
/// BSP and outside its solid physics BSP.
fn point_in(store: &dereth_dat::RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let d = dereth_client::env_cells::EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = dereth_client::env_cells::physics_geometry(&d);
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

/// The whole bench: a real headless `App` over `block`'s shipped terrain, with the body standing
/// inside interior `cell` and the corpus player's own create describing him there.
fn setup_in(block: u16, cell: u32) -> App {
    let store = dereth_dat::testing::open_store_or_fail();
    let stand = point_in(&store, cell).expect("the station cell has a standable point");

    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dere-p1-70b-indoor-pick-not-created")
            .join("preferences.ini"),
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        landblock: block,
        start_cell: Some(CellId(cell)),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain/physics scene");

    // `start_cell` is not enough on its own. `Character::teleport` follows the retail enter-world
    // placement role and re-seats the body in whichever cell actually contains the point.
    {
        let mut scene = app.world_scene_mut().expect("a world scene");
        let c = scene.character.as_mut().expect("a body");
        c.land().load_block_cells(LandblockId(block));
        c.teleport(Position::new(
            CellId(cell),
            Frame::new(stand, Quat::IDENTITY),
        ));
    }
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
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("constructed placement"),
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

fn object_pixel(app: &App, id: ObjectId) -> (i32, i32) {
    let scene = app.world_scene().unwrap();
    let f = scene
        .server_object_frame(id)
        .expect("the object has a drawn SceneObject frame");
    aim(app, Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.5))
}

/// An **outdoor** point of `block`, on open ground, that projects inside the 3-D view.
///
/// The search is a ring around the body at the landblock's own ground height: the object's *cell*
/// is what the discriminator is about. `dereth_physics::landdefs::adjust_to_outside` rebases each
/// candidate into its outdoor land cell and supplies that cell ID.
/// Returns `(outdoor cell id, the point in landblock metres)`.
fn an_outdoor_point_in_view(app: &App, block: u16) -> Option<(u32, Vec3)> {
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let here = position(app);
    let land = scene.character.as_ref().expect("a body").land();
    let mut best: Option<(f32, u32, Vec3)> = None;
    for step in 0..14 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let r = 5.0 + step as f32 * 1.5;
        for a in 0..72 {
            #[allow(clippy::cast_precision_loss)] // a small loop counter
            let th = a as f32 * std::f32::consts::TAU / 72.0;
            let (x, y) = (
                here.frame.origin.x + r * math::sinf(th),
                here.frame.origin.y + r * math::cosf(th),
            );
            if !(1.0..191.0).contains(&x) || !(1.0..191.0).contains(&y) {
                continue;
            }
            let Some(g) = land.ground_height(LandblockId(block), x, y) else {
                continue;
            };
            let p = Vec3::new(x, y, g + 0.6);
            let mut cell = LandblockId(block).cell(1);
            let mut origin = p;
            if !dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin) {
                continue;
            }
            let world = Vec3::new(
                viewer.origin.x + (p.x - here.frame.origin.x),
                viewer.origin.y + (p.y - here.frame.origin.y),
                viewer.origin.z + (p.z - here.frame.origin.z),
            );
            let local = dereth_physics::math::globaltolocal(&viewer, world);
            if local.y < 4.0 || local.y > 40.0 {
                continue;
            }
            let (px, py) = pixel_of(local, SCREEN, fov);
            if !(80.0..720.0).contains(&px) || !(80.0..520.0).contains(&py) {
                continue;
            }
            if best.as_ref().is_none_or(|b| local.y < b.0) {
                best = Some((local.y, cell.0, p));
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(_, c, p)| (c, p))
}

/// Several such points, nearest first and at least 4 m apart: "in front of the camera" and
/// "through the opening" are different things indoors. The spread matters: the ring search's
/// neighbours are centimetres apart, so ten adjacent candidates are one candidate.
fn outdoor_points_in_view(app: &App, block: u16) -> Vec<(u32, Vec3)> {
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let here = position(app);
    let land = scene.character.as_ref().expect("a body").land();
    let mut found: Vec<(f32, u32, Vec3)> = Vec::new();
    for step in 0..24 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let r = 5.0 + step as f32 * 1.5;
        for a in 0..144 {
            #[allow(clippy::cast_precision_loss)] // a small loop counter
            let th = a as f32 * std::f32::consts::TAU / 144.0;
            let (x, y) = (
                here.frame.origin.x + r * math::sinf(th),
                here.frame.origin.y + r * math::cosf(th),
            );
            if !(1.0..191.0).contains(&x) || !(1.0..191.0).contains(&y) {
                continue;
            }
            let Some(g) = land.ground_height(LandblockId(block), x, y) else {
                continue;
            };
            let p = Vec3::new(x, y, g + 0.6);
            let mut cell = LandblockId(block).cell(1);
            let mut origin = p;
            if !dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin) {
                continue;
            }
            let world = Vec3::new(
                viewer.origin.x + (p.x - here.frame.origin.x),
                viewer.origin.y + (p.y - here.frame.origin.y),
                viewer.origin.z + (p.z - here.frame.origin.z),
            );
            let local = dereth_physics::math::globaltolocal(&viewer, world);
            if local.y < 4.0 || local.y > 40.0 {
                continue;
            }
            let (px, py) = pixel_of(local, SCREEN, fov);
            if !(80.0..720.0).contains(&px) || !(80.0..520.0).contains(&py) {
                continue;
            }
            if found
                .iter()
                .any(|(_, _, q)| math::hypotf(q.x - p.x, q.y - p.y) < 4.0)
            {
                continue;
            }
            found.push((local.y, cell.0, p));
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, c, p)| (c, p)).collect()
}

/// `early-inventory-and-casting`'s chest, created at `origin` of `cell`: the real decoded-object
/// -> physics placement -> world draw path, with only its wire position changed.
fn place_the_corpus_chest_in(app: &mut App, cell: u32, origin: Vec3) -> (ObjectId, String) {
    place_the_corpus_chest_in_as(app, cell, origin, CORPUS_CHEST)
}

/// The same, under `id`. Two creates built from one recorded blob carry the same
/// `ItemCreateObject::id`, so a station that places several of them is placing one object.
fn place_the_corpus_chest_in_as(
    app: &mut App,
    cell: u32,
    origin: Vec3,
    id: ObjectId,
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
    create.0.id = id;
    let name = create.0.wdesc.name.clone();
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("re-encodes"),
        },
        LocalTime(6.0),
    );
    frames(app, 30);
    (id, name)
}

fn move_pointer_to(app: &mut App, x: i32, y: i32, at: u32) {
    let mut pump = dereth_client::pump::Pump::new();
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

/// Behaviour: selection.pick.an-outdoor-object-a-sealed-interior-frame-never-drew-is-not-selectable
///
/// **The discriminator.** One frame in which, simultaneously:
///
/// 1. the remote object is in `ObjectStream::presences`;
/// 2. `WorldScene::server_object_frame(id)` is `Some`;
/// 3. its geometry lies on the active selection ray;
/// 4. the render frame's post-phase / post-view-cone candidate stage did **not** offer it.
///
/// All four are asserted as premises. The fifth line is the claim: the retail client cannot name
/// an object that its render frame never submitted, because the selection-ray candidate check is
/// reached only while drawing submitted mesh geometry.
#[test]
fn an_outdoor_object_a_sealed_interior_frame_never_drew_is_not_selectable() {
    let mut app = setup_in(SEALED_BLOCK, SEALED_CELL);
    let sbox = world_view(&mut app);

    // ---- the frame really is the gated one ---------------------------------------------------
    {
        let scene = app.world_scene().expect("a world scene");
        let viewer_cell = scene.viewer_cell_id().expect("the viewer has a cell");
        assert!(
            !dereth_physics::landdefs::is_outdoors(viewer_cell),
            "the camera settled in {:#010X}, which is outdoors, so the outdoor render arm runs \
             and this station cannot test the indoor arm",
            viewer_cell.0
        );
        let count = scene.indoor_outside_view_count();
        assert_eq!(
            count,
            Some(0),
            "portal clipping's outside-view count at {:#010X} is {count:?}; zero skips the \
             entire outdoor cell-draw branch and landscape drawing",
            viewer_cell.0
        );
    }

    let (cell, origin) = an_outdoor_point_in_view(&app, SEALED_BLOCK)
        .expect("an outdoor point of the sealed building's block projects into the view");
    assert!(
        dereth_physics::landdefs::is_outdoors(CellId(cell)),
        "{cell:#010X} is an outdoor land cell"
    );
    let (chest, name) = place_the_corpus_chest_in(&mut app, cell, origin);

    // ---- premise 1: the presence is live, and it is outdoors ---------------------------------
    let presence_cell = app
        .objects()
        .presence(chest)
        .and_then(|p| p.position)
        .map(|p| p.cell)
        .expect("the chest is in `ObjectStream::presences` with a position");
    assert!(
        dereth_physics::landdefs::is_outdoors(presence_cell),
        "the chest settled in {:#010X}, which is not an outdoor cell",
        presence_cell.0
    );

    // ---- premise 2: the scene has a render frame for it ---------------------------------------
    let frame = app
        .world_state()
        .unwrap()
        .server_object_frame(chest)
        .expect("`server_object_frame` answers for the chest");

    // ---- premise 3: the ray goes through it ---------------------------------------------------
    let at = object_pixel(&app, chest);
    assert_eq!(
        ui_of(&mut app).hit_test_screen(at.0, at.1),
        Some(sbox),
        "the pointer at {at:?} is over the 3-D view"
    );

    // ---- premise 4: the frame's candidate stage refused it -------------------------------------
    let scene = app.world_scene().expect("a world scene");
    let submitted = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(chest))
        .count();
    assert_eq!(
        submitted, 0,
        "the object pass submitted {submitted} chest parts, so this frame offered the chest to \
         the selection-ray candidate check and there is no divergence to measure here"
    );
    eprintln!(
        "indoor_pick_outdoor_objects sealed station: viewer cell {:#010X}, outside_view_count \
         {:?}, chest {:#010X} in {:#010X} at {:?}, pixel {at:?}, submitted parts {submitted}",
        scene.viewer_cell_id().unwrap().0,
        scene.indoor_outside_view_count(),
        chest.0,
        presence_cell.0,
        (frame.origin.x, frame.origin.y, frame.origin.z),
    );

    // ---- the claim ---------------------------------------------------------------------------
    let before = draw_list(&mut app);
    rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
    let after = draw_list(&mut app);
    let glyphs = glyphs_that_appeared(&before, &after);

    assert_ne!(
        app.interaction().pick.click_object().0,
        chest,
        "the sweep named an outdoor object on a frame whose cell-draw pass never reached \
         landscape drawing — pick stats {:?}",
        app.interaction().pick.stats
    );
    assert!(
        !glyphs.contains(&name),
        "nothing named the chest on screen; drew {glyphs:?}"
    );
    assert_ne!(
        ui_of(&mut app)
            .node(sbox)
            .and_then(|n| n.tooltip_text.clone()),
        Some(name.clone()),
        "the world-view tooltip is not the name of an object the frame never drew"
    );
    assert!(
        app.interaction().pick.stats.objects_not_offered_this_frame >= 1,
        "and the sweep says why it did not: {:?}",
        app.interaction().pick.stats
    );
}

/// **The control that makes the line above mean something.** Holtburg's house at `0xA9B40146`
/// sees the outdoors: its `outside_view.view_count` is positive and its opening covers
/// 10,473 px. The positive count takes the outdoor branch,
/// draws the landscape, and submits the visible land cells' objects to mesh drawing and then the
/// selection ray.
///
/// Without this a build that simply refused every outdoor candidate indoors would be green above.
#[test]
fn an_outdoor_object_the_open_interior_frame_did_draw_is_still_selectable() {
    let mut app = setup_in(OPEN_BLOCK, OPEN_CELL);
    let sbox = world_view(&mut app);

    {
        let scene = app.world_scene().expect("a world scene");
        let viewer_cell = scene.viewer_cell_id().expect("the viewer has a cell");
        assert!(
            !dereth_physics::landdefs::is_outdoors(viewer_cell),
            "the camera settled outdoors in {:#010X}",
            viewer_cell.0
        );
        let count = scene
            .indoor_outside_view_count()
            .expect("an indoor frame that traverses");
        assert!(
            count > 0,
            "the Holtburg house {OPEN_CELL:#010X} reports {count} outdoor view polygons, so \
             landscape drawing does not run and this is not the control it claims to be"
        );
    }

    // The indoor traversal is the retail portal clipper's own, so an outdoor object is
    // submitted only if it falls inside one of the openings' screen polygons: "an outdoor point
    // in front of the camera" is not enough, and the first one the ring search offers is usually
    // beside the window rather than through it. The *premise* of this control is that the frame
    // drew the chest; the *claim* is that the pick then keeps it. So the station looks for a
    // point the frame actually submits and asserts the pick's answer about that one. Which
    // points the cone admits is the portal-clip module's claim, not this one's.
    let mut placed: Option<(ObjectId, String, u32, usize)> = None;
    for (n, (cell, origin)) in outdoor_points_in_view(&app, OPEN_BLOCK)
        .into_iter()
        .take(10)
        .enumerate()
    {
        let id = ObjectId(CONTROL_CHEST.0 + u32::try_from(n).expect("a small loop counter"));
        let (chest, name) = place_the_corpus_chest_in_as(&mut app, cell, origin, id);
        let submitted = app
            .world_scene()
            .expect("a world scene")
            .drawn_part_order()
            .iter()
            .filter(|part| part.object == Some(chest))
            .count();
        if submitted > 0 {
            placed = Some((chest, name, cell, submitted));
            break;
        }
    }
    let Some((chest, name, cell, submitted)) = placed else {
        panic!(
            "none of the ten spread-out outdoor points was submitted by the pass through the \
             openings, so this control cannot say whether the pick keeps what the frame drew"
        )
    };
    let at = object_pixel(&app, chest);
    assert_eq!(ui_of(&mut app).hit_test_screen(at.0, at.1), Some(sbox));
    eprintln!(
        "indoor_pick_outdoor_objects open control: viewer cell {:#010X}, outside_view_count \
         {:?}, chest {:#010X} in {cell:#010X}, pixel {at:?}, submitted parts {submitted}",
        app.world_state().unwrap().viewer_cell_id().unwrap().0,
        app.world_scene().unwrap().indoor_outside_view_count(),
        chest.0,
    );

    rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
    assert_eq!(
        app.interaction().pick.click_object().0,
        chest,
        "an outdoor object the indoor frame *did* draw must stay selectable ({name}); pick stats \
         {:?}",
        app.interaction().pick.stats
    );
}
