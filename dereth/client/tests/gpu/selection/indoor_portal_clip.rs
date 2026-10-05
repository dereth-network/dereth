//! Indoors, the clipped outlines of the portals that reach the outdoors decide whether there is an
//! outdoor pass at all and what it may draw: facing a wall the outside-view count is zero and no
//! outdoor object is drawn or named; through a window only objects inside the opening's cone are
//! drawn and named. With the clip switched off every portal counts, and the wall hover names the
//! chest outside. Fixture: a headless `App` over a Holtburg house cell (0xA9B40146) from the retail
//! dats, the chest from `early-inventory-and-casting` and the creature from `long-solo-play`; the
//! arms differ only in **which way the body is facing**.
//!
//! # An opening contributes only while it is on screen
//!
//! Portal clipping first computes the portal's screen-space outline. If nothing remains, the portal
//! contributes nothing. An outdoor portal has `other_cell_id == 0xFFFFFFFF`. The clipping-mode
//! flag is **1** in shipped data, so the shipped arm copies the opening's own outline into
//! `outside_view` instead of installing a full-screen default.
//!
//! The resulting count gates the entire outdoor block. An interior viewer **facing a wall** has
//! `view_count == 0` and draws no terrain, sky, or outdoor object.
//!
//! # Surviving objects are cone-tested against each opening
//!
//! The indoor cell-draw path installs `outside_view` before landscape drawing, so its outdoor pass
//! uses the portal arm. For each surviving outline, mesh drawing installs that view, checks the
//! object's geometry against its cone, skips to the next outline on rejection, and otherwise offers
//! the geometry to the selection ray before drawing it. An outdoor object is therefore submitted
//! only if it falls inside at least one opening polygon.

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
use dereth_client_runtime::scene::SceneConfig;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui::{ElemHandle, UiDrawCmd, UiSystem};

const SCREEN: (u32, u32) = (800, 600);
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
const SMART_BOX: dereth_ui::ElementId = dereth_ui_screens::hud::world_view::SMART_BOX;

/// Holtburg, and a house there with windows onto the land. None of Holtburg's interior cells is
/// sealed, so without the clip every frame in it takes the outdoor pass.
const HOLTBURG: u16 = 0xA9B4;
const HOUSE_CELL: u32 = 0xA9B4_0146;

/// Two distinct server ids for the two placed chests. `ItemCreateObject::id` is the key the whole
/// client uses, so two creates built from one recorded blob are one object.
const CHEST_A: ObjectId = ObjectId(0x7DA5_5005);
const CHEST_B: ObjectId = ObjectId(0x7DA5_5006);
/// The base id for the placed creature attempts — long-solo-play's own `0x800009D2` is reused as a
/// template and re-keyed, so several attempts are several objects.
const CREATURE: ObjectId = ObjectId(0x7DA5_5030);

/// A bearing at which this house's portal chain reaches the outdoors but **no opening is on
/// screen** — `turning_to_face_a_wall_takes_the_outside_view_count_to_zero` asserts this exact
/// value reports zero, so the unclipped control below can reach it without re-deriving it from a
/// build that cannot tell the difference.
const WALL_YAW: f32 = 202.5 * std::f32::consts::PI / 180.0;

/// A point a body could stand at inside `cell`, in the landblock's own metres.
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

/// `Quat` for a yaw about the client's +z, matching `viewpoint_source.rs`, so the current heading
/// calculation and `follow_character` see the body turn.
fn yaw_quat(yaw: f32) -> Quat {
    let half = yaw * 0.5;
    Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half))
}

/// The bench: a real headless `App` over Holtburg with the body standing in `cell`.
///
/// `portal_clip` is `SceneConfig`'s own switch: on is the shipped client's behaviour, and off
/// treats every portal as visible and copies its view successfully.
fn setup_in(block: u16, cell: u32, portal_clip: bool) -> App {
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
            .join("dereth-indoor-portal-clip-not-created")
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
        portal_clip,
        ..Default::default()
    })
    .expect("real terrain/physics scene");
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
        .expect("recorded 0xF745");
    let here = position(&app);
    create.0.physicsdesc.position = Some(wire_position(here.cell.0, here.frame.origin, &here));
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
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

/// Turn the body on the spot and let the chase camera settle, without leaving the cell.
///
/// `settle` matters: `Character::teleport` follows enter-world placement and viewer update,
/// which leaves the viewer **on the pivot**, inside the body. A station that reads a pick
/// before the camera sweep has walked the eye back out is reading the body's own parts, not the
/// world's.
fn face(app: &mut App, yaw: f32, settle: usize) {
    let here = position(app);
    {
        let mut scene = app.world_scene_mut().expect("a world scene");
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            here.cell,
            Frame::new(here.frame.origin, yaw_quat(yaw)),
        ));
    }
    frames(app, settle);
}

/// The frames a bearing needs before its `outside_view` is the settled camera's.
const SWEEP_SETTLE: usize = 20;
/// The frames a station needs before it may believe a **pick**: the eye has to be out of the body.
const PICK_SETTLE: usize = 90;

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

/// Even–odd point-in-polygon, on `WorldScene::outside_view_polys`' own screen outlines.
fn inside_poly(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut hit = false;
    let n = poly.len();
    for i in 0..n {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % n];
        if (y1 > p.1) != (y2 > p.1) {
            let t = (p.1 - y1) / (y2 - y1);
            if p.0 < x1 + t * (x2 - x1) {
                hit = !hit;
            }
        }
    }
    hit
}

/// The shortest distance from `p` to any edge of `poly`, in pixels. The stations use it as a
/// margin: the cone tests an object's **drawing sphere**, not its centre pixel, so a candidate
/// sitting on the opening's edge is a coin toss rather than a statement.
fn distance_to_poly(poly: &[(f32, f32)], p: (f32, f32)) -> f32 {
    let mut best = f32::MAX;
    let n = poly.len();
    for i in 0..n {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % n];
        let (ex, ey) = (x2 - x1, y2 - y1);
        let len2 = ex * ex + ey * ey;
        let t = if len2 > 0.0 {
            (((p.0 - x1) * ex + (p.1 - y1) * ey) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (qx, qy) = (x1 + t * ex, y1 + t * ey);
        best = best.min(((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt());
    }
    best
}

/// Outdoor points of `block`, on open ground, that project into the 3-D view — with the pixel each
/// one projects to, so a station can classify them against the openings.
fn outdoor_candidates(app: &App, block: u16) -> Vec<(u32, Vec3, (f32, f32))> {
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let here = position(app);
    let land = scene.character.as_ref().expect("a body").land();
    let mut out = Vec::new();
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
            let px = pixel_of(local, SCREEN, fov);
            if !(80.0..720.0).contains(&px.0) || !(80.0..520.0).contains(&px.1) {
                continue;
            }
            out.push((cell.0, p, px));
        }
    }
    out
}

/// `early-inventory-and-casting`'s chest, created at `origin` of `cell` under `id`.
///
/// **The id is a parameter and not the corpus's own**: two creates
/// from the same recorded blob carry the same `ItemCreateObject::id`, so the second one *replaces*
/// the first in `ObjectStream` and both halves of a two-object comparison end up measuring one
/// object. `0xF745`'s id is the server's and the client keys everything on it.
fn place_the_corpus_chest_in(
    app: &mut App,
    cell: u32,
    origin: Vec3,
    at: f64,
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
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("re-encodes"),
        },
        LocalTime(at),
    );
    frames(app, 30);
    (id, name)
}

/// `long-solo-play`'s recorded remote **creature** `0x800009D2`, placed at `origin` of `cell` under
/// `id`.
///
/// A creature is not a chest: it carries a `MotionDriver`, its parts move every frame, and the
/// per-frame part update is what fills the frames the sweep places by. A station that only ever
/// places a static chest cannot see a defect that lives in the animated path.
fn place_the_corpus_creature_in(
    app: &mut App,
    cell: u32,
    origin: Vec3,
    at: f64,
    id: ObjectId,
) -> (ObjectId, String) {
    let corpus = Corpus::shared("long-solo-play");
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && r.payload.get(4..8) == Some(&0x8000_09D2_u32.to_le_bytes())
        })
        .expect("long-solo-play carries the remote creature create");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("the recorded 0xF745 decodes");
    let here = position(app);
    create.0.physicsdesc.position = Some(wire_position(cell, origin, &here));
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    create.0.id = id;
    let name = create.0.wdesc.name.clone();
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("re-encodes"),
        },
        LocalTime(at),
    );
    frames(app, 30);
    (id, name)
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

/// `outside_view.view_count` and the openings' outlines at each of `n` bearings, all in one scene.
fn sweep_bearings(app: &mut App, n: usize) -> Vec<(f32, usize, usize)> {
    let mut out = Vec::new();
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let yaw = i as f32 * std::f32::consts::TAU / n as f32;
        face(app, yaw, SWEEP_SETTLE);
        let scene = app.world_scene().expect("a world scene");
        out.push((
            yaw,
            scene.indoor_outside_view_count().unwrap_or(usize::MAX),
            scene.outside_view_polys().len(),
        ));
    }
    out
}

// =================================================================================================

/// **The gate itself.** The cell-draw path keys the whole outdoor block on
/// `outside_view.view_count`, and portal clipping only counts an opening while its screen-space
/// clip leaves something visible. So turning on the spot inside one house must take the count
/// through zero.
///
/// An *unclipped* count would be every exterior portal the chain reached, at every bearing, so
/// landscape drawing would run on every frame of every Holtburg interior, and the pick, which
/// offers exactly what the frame submitted, would offer the whole outdoors.
#[test]
fn turning_to_face_a_wall_takes_the_outside_view_count_to_zero() {
    let mut app = setup_in(HOLTBURG, HOUSE_CELL, true);
    let bearings = sweep_bearings(&mut app, 16);
    let zero = bearings.iter().filter(|b| b.1 == 0).count();
    let some = bearings
        .iter()
        .filter(|b| b.1 > 0 && b.1 != usize::MAX)
        .count();
    eprintln!(
        "bearings at {HOUSE_CELL:#010X}: {:?}",
        bearings
            .iter()
            .map(|(y, c, p)| format!("{:.0}deg->{c}/{p}", y.to_degrees()))
            .collect::<Vec<_>>()
    );
    assert!(
        zero > 0,
        "no bearing of this house reports outside_view.view_count == 0, so the outdoor-block \
         zero-count exit never runs and the outdoor pass runs with the camera facing a wall"
    );
    assert!(
        some > 0,
        "every bearing reports zero — the traversal is not finding the house's windows at all, \
         and the assertion above would be satisfied by a build that drew no outdoor pass anywhere"
    );
    // The polygons and the count are one statement: a counted opening must have an outline, and a
    // bearing with no opening must publish none.
    for (yaw, count, polys) in &bearings {
        assert_eq!(
            count,
            polys,
            "at {:.0} degrees the portal traversal counted {count} openings and published \
             {polys} outlines; each count must produce one outline",
            yaw.to_degrees()
        );
    }
    // The named bearing the unclipped control uses. Pinned here so that geometry drift breaks
    // *this* test rather than silently turning the control into a second window station.
    face(&mut app, WALL_YAW, SWEEP_SETTLE);
    assert_eq!(
        app.world_scene()
            .expect("a world scene")
            .indoor_outside_view_count(),
        Some(0),
        "`WALL_YAW` ({:.1} deg) is no longer a wall bearing of {HOUSE_CELL:#010X}",
        WALL_YAW.to_degrees()
    );
}

/// One creature, placed outdoors where this build's own frame can draw it, hovered.
///
/// Returns `(parts submitted, in `drawn_objects`, the pick named it, its name, the opening count)`.
/// The three answers are kept apart on purpose: "the frame drew it but did not publish it" and
/// "the frame published it but the sweep dropped it" are different defects with different fixes,
/// and a bare "the hover named nothing" cannot tell them apart.
fn creature_through_an_opening(portal_clip: bool) -> (usize, bool, bool, String, usize) {
    let mut app = setup_in(HOLTBURG, HOUSE_CELL, portal_clip);
    let sbox = world_view(&mut app);
    let bearings = sweep_bearings(&mut app, 16);
    let &(yaw, count, _) = bearings
        .iter()
        .filter(|b| b.1 > 0 && b.1 != usize::MAX)
        .max_by_key(|b| b.1)
        .expect("some bearing of this house sees an opening");
    face(&mut app, yaw, PICK_SETTLE);

    // Where the frame can draw it. With the clip on that is inside an opening's own outline; with
    // it off the outdoor pass is full-screen and any point in view will do.
    let polys = app
        .world_scene()
        .expect("a world scene")
        .outside_view_polys();
    let candidates = outdoor_candidates(&app, HOLTBURG);
    let mut ranked: Vec<(f32, u32, Vec3, (f32, f32))> = candidates
        .iter()
        .filter_map(|&(cell, origin, px)| {
            if polys.is_empty() {
                return Some((0.0, cell, origin, px));
            }
            polys
                .iter()
                .filter(|p| inside_poly(p, px))
                .map(|p| distance_to_poly(p, px))
                .fold(None, |acc: Option<f32>, d| {
                    Some(acc.map_or(d, |a| a.max(d)))
                })
                .map(|d| (d, cell, origin, px))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));

    let body_id = app
        .world_state()
        .and_then(|s| s.character.as_ref().map(|c| c.object_id()))
        .unwrap_or(ObjectId(0));

    // **A pixel that is not on your own character.** Inside a room the chase camera's sweep is
    // blocked and the viewer ends up on the body's own pivot, so most of the screen is the player's
    // own parts, and mesh drawing offers those to the ray like anyone else's (a click on yourself
    // selects you). A hover that names the body is therefore *correct* and simply means the
    // pointer is on the body, so the search moves on rather than concluding anything.
    // `body_blocked` is reported so that "every candidate was on the player" cannot be mistaken
    // for "the creature was refused".
    let mut tried: Vec<Vec3> = Vec::new();
    let mut body_blocked = 0usize;
    let mut live: Vec<ObjectId> = Vec::new();
    let mut answer: Option<(usize, bool, bool, String, (f32, f32), (i32, i32), ObjectId)> = None;
    for &(_, cell, origin, px) in &ranked {
        if tried
            .iter()
            .any(|p| math::hypotf(p.x - origin.x, p.y - origin.y) < 4.0)
        {
            continue;
        }
        if tried.len() >= 12 {
            break;
        }
        let id = ObjectId(CREATURE.0 + u32::try_from(tried.len()).expect("a small loop counter"));
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let stamp = 6.0 + tried.len() as f64;
        tried.push(origin);
        let (placed, name) = place_the_corpus_creature_in(&mut app, cell, origin, stamp, id);
        live.push(placed);
        face(&mut app, yaw, PICK_SETTLE);
        let scene = app.world_scene().expect("a world scene");
        if scene.server_object_frame(placed).is_none()
            || !scene
                .drawn_part_order()
                .iter()
                .any(|p| p.object == Some(placed))
        {
            eprintln!("creature: candidate {px:?} not placed-and-drawn; next");
            continue;
        }
        let parts = scene
            .drawn_part_order()
            .iter()
            .filter(|p| p.object == Some(placed))
            .count();
        let published =
            SceneReads::drawn_objects(&scene).is_some_and(|seen| seen.contains(&placed));
        let at = object_pixel(&app, placed);
        if ui_of(&mut app).hit_test_screen(at.0, at.1) != Some(sbox) {
            continue;
        }
        rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
        let hit = app.interaction().pick.click_object().0;
        if hit == body_id && hit != placed {
            body_blocked += 1;
            // **The measurement this skip exists to record.** When the camera is inside the head,
            // the camera update sets the player's hierarchical translucency to 1.0. The part
            // setter compares that value with the shipped 1.0 no-draw threshold and sets the
            // part's `NoDraw` bit. Part drawing tests that bit first, so a near-camera body is
            // offered to neither drawing nor the selection ray. Print this build's latch
            // and part count so that "the body won the ray" is a reading rather than an inference.
            let scene = app.world_scene().expect("a world scene");
            let (translucency, drawable) = scene.character.as_ref().map_or((-1.0, 0), |c| {
                (
                    c.camera.player_translucency,
                    c.driver()
                        .part_array
                        .parts
                        .iter()
                        .filter(|p| !p.no_draw())
                        .count(),
                )
            });
            eprintln!(
                "creature: aim {at:?} is on the player's own body; next \
                 candidate \
                 (player_translucency {translucency}, {drawable} body part(s) not NoDraw)"
            );
            continue;
        }
        // A rejected attempt stays in the world — there is no removal on this bench — and the
        // attempts are all the same recorded creature standing outdoors within one opening's cone,
        // so the ray legitimately answers with whichever of them is nearest. The claim is "the
        // hover names a creature this frame drew outdoors", and that is what is checked; the exact
        // id is printed so the reader can see which one.
        answer = Some((parts, published, live.contains(&hit), name, px, at, hit));
        break;
    }
    let Some((parts, published, picked, name, px, at, hit)) = answer else {
        panic!(
            "no outdoor point gave a creature this frame draws at a pixel clear of the player's \
             own body ({} tried, {body_blocked} on the body, {} ranked, {} outlines)",
            tried.len(),
            ranked.len(),
            polys.len()
        )
    };
    eprintln!(
        "creature (portal_clip {portal_clip}): yaw {:.0} deg, {count} opening(s), \
         {name} at {px:?} -> {parts} parts, published {published}, picked {picked}; aim {at:?}, \
         hover named {:#010X} (the body is {:#010X}), {body_blocked} candidate(s) on the body, \
         pick stats {:?}",
        yaw.to_degrees(),
        hit.0,
        body_id.0,
        app.interaction().pick.stats
    );
    (parts, published, picked, name, count)
}

/// **A creature seen through an opening can be picked.** With the clip on, a creature standing
/// outdoors inside an opening's own cone is drawn by this frame, so the mesh path offers it to the
/// selection ray and the hover must name it.
#[test]
fn a_creature_the_frame_draws_through_an_opening_is_named() {
    let (parts, published, picked, name, count) = creature_through_an_opening(true);
    assert!(
        parts > 0,
        "the station's own premise: the frame submitted {name}'s parts"
    );
    assert!(
        published,
        "{name} was submitted by the object pass but is not in `WorldScene::drawn_objects` — the \
         frame drew what the pick was then told it had not"
    );
    assert!(
        picked,
        "hovering a creature the frame drew through one of {count} opening(s) named something \
         else"
    );
}

/// The same with `portal_clip` **off**: the outdoor pass runs full-screen, so the creature is
/// certainly drawn, and whether the hover names it is a question about the publication of drawn
/// objects to the pick alone, independent of the portal traversal.
#[test]
fn a_creature_the_unclipped_frame_draws_is_named() {
    let (parts, published, picked, name, _) = creature_through_an_opening(false);
    assert!(
        parts > 0,
        "the unclipped outdoor pass submitted {name}'s parts"
    );
    assert!(
        published,
        "{name} was submitted but not published into `drawn_objects`"
    );
    assert!(
        picked,
        "hovering a creature the unclipped frame drew named something else"
    );
}

/// **The unclipped build, kept as a control.**
///
/// With `SceneConfig::portal_clip` off every portal is treated as visible and every view copy
/// succeeds. Then **no** bearing of this house ever reports zero. The outdoor-block gate never
/// takes its zero-count exit, the pass runs on every frame with the full screen as its view, and
/// the outdoor chest behind the wall is drawn *and* named. Turn the switch back on and the two
/// stations above hold.
#[test]
fn without_the_clip_every_bearing_runs_the_outdoor_pass_and_the_wall_hover_names_the_chest() {
    let mut app = setup_in(HOLTBURG, HOUSE_CELL, false);
    let sbox = world_view(&mut app);
    let bearings = sweep_bearings(&mut app, 16);
    eprintln!(
        "unclipped bearings: {:?}",
        bearings
            .iter()
            .map(|(y, c, p)| format!("{:.0}deg->{c}/{p}", y.to_degrees()))
            .collect::<Vec<_>>()
    );
    assert!(
        bearings.iter().all(|b| b.1 > 0),
        "with the clip off some bearing already reported zero, so the clipped build's zero is not \
         the clip's doing: {bearings:?}"
    );
    assert!(
        bearings.iter().all(|b| b.2 == 0),
        "the unclipped traversal has no polygons to publish: it counts openings without \
         outlines and the outdoor pass must stay full-screen there: {bearings:?}"
    );

    face(&mut app, WALL_YAW, PICK_SETTLE);
    let candidates = outdoor_candidates(&app, HOLTBURG);
    assert!(
        !candidates.is_empty(),
        "no outdoor ground point projects into the view"
    );
    let (cell, origin, _) = candidates[0];
    let (chest, name) = place_the_corpus_chest_in(&mut app, cell, origin, 6.0, CHEST_A);
    face(&mut app, WALL_YAW, PICK_SETTLE);

    let scene = app.world_scene().expect("a world scene");
    let submitted = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(chest))
        .count();
    eprintln!(
        "unclipped wall arm: count {:?}, chest {:#010X} -> {submitted} parts",
        scene.indoor_outside_view_count(),
        chest.0,
    );
    assert!(
        submitted > 0,
        "even unclipped the wall-facing frame submitted no outdoor chest part, so the two \
         stations above are not measuring what this control says they are"
    );
    let at = object_pixel(&app, chest);
    assert_eq!(ui_of(&mut app).hit_test_screen(at.0, at.1), Some(sbox));
    rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
    assert_eq!(
        app.interaction().pick.click_object().0,
        chest,
        "the unclipped build named {name} through the wall; pick stats {:?}",
        app.interaction().pick.stats
    );
}

/// Facing an interior wall, the frame submits **no** outdoor object part
/// and the pick offers none because the cell-draw path never reaches landscape drawing and no
/// outdoor geometry reaches the selection-ray candidate check.
#[test]
fn facing_a_wall_no_outdoor_object_is_drawn_or_named() {
    let mut app = setup_in(HOLTBURG, HOUSE_CELL, true);
    let sbox = world_view(&mut app);
    let bearings = sweep_bearings(&mut app, 16);
    let Some(&(wall_yaw, _, _)) = bearings.iter().find(|b| b.1 == 0) else {
        panic!("no bearing of {HOUSE_CELL:#010X} faces a wall: {bearings:?}")
    };
    face(&mut app, wall_yaw, PICK_SETTLE);

    let candidates = outdoor_candidates(&app, HOLTBURG);
    assert!(
        !candidates.is_empty(),
        "no outdoor ground point projects into the view"
    );
    let (cell, origin, _) = candidates[0];
    let (chest, name) = place_the_corpus_chest_in(&mut app, cell, origin, 6.0, CHEST_A);
    face(&mut app, wall_yaw, PICK_SETTLE); // the create's own settle must not have moved the camera

    let scene = app.world_scene().expect("a world scene");
    assert_eq!(
        scene.indoor_outside_view_count(),
        Some(0),
        "the bearing stopped being a wall bearing once the chest was placed"
    );
    assert!(
        scene.outside_view_polys().is_empty(),
        "and no opening survived the screen-space clip"
    );
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

    let scene = app.world_scene().expect("a world scene");
    let submitted = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(chest))
        .count();
    assert_eq!(
        submitted, 0,
        "facing a wall the object pass submitted {submitted} outdoor chest parts; retail's \
         cell-draw pass never reached landscape drawing at all"
    );
    assert!(
        SceneReads::drawn_objects(&scene).is_some_and(|seen| !seen.contains(&chest)),
        "and the frame's offered-object set must not contain it"
    );

    let at = object_pixel(&app, chest);
    assert_eq!(ui_of(&mut app).hit_test_screen(at.0, at.1), Some(sbox));
    eprintln!(
        "wall arm: yaw {:.0} deg, viewer cell {:#010X}, count {:?}, chest {:#010X} in \
         {:#010X}, pixel {at:?}, submitted {submitted}",
        wall_yaw.to_degrees(),
        app.world_state().unwrap().viewer_cell_id().unwrap().0,
        app.world_scene().unwrap().indoor_outside_view_count(),
        chest.0,
        presence_cell.0,
    );

    let before = draw_list(&mut app);
    rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
    let after = draw_list(&mut app);
    let glyphs = glyphs_that_appeared(&before, &after);
    assert_ne!(
        app.interaction().pick.click_object().0,
        chest,
        "hovering the wall named the chest outside it; pick stats {:?}",
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
        "the world-view tooltip is not the name of an object behind the wall"
    );
}

/// Behaviour: selection.pick.indoors-only-what-a-clipped-opening-shows-is-drawn-and-named
///
/// **The other half, and the one that stops the first from being satisfied by a blanket refusal.**
///
/// Facing an opening, `outside_view` is the opening's own outline and the portal mesh path
/// cone-tests every outdoor mesh against it. So of two outdoor chests both on screen, the one
/// inside the window's polygon must be drawn **and** selectable, and the one outside it must be
/// neither.
///
/// The two positions are chosen from `WorldScene::outside_view_polys` — the frame's own published
/// `outside_view`, which is the *input* to the cone — and the assertions read the draw's and the
/// pick's answers, which are the *output*. A margin of 40 px from the outline is required on both
/// sides because the view-cone check tests the part's **drawing sphere**, not its centre pixel.
#[test]
fn facing_a_window_only_what_is_inside_its_cone_is_drawn_and_named() {
    // A chest's drawing sphere is about half a metre; at this station's 5–40 m that is ~12 px at
    // the far end and ~30 px at the near one, so the two floors are asymmetric on purpose. The
    // inside candidate has a whole opening's width to be central in and only ~70 px of height, and
    // the outside one has the rest of the screen.
    const IN_FLOOR: f32 = 10.0;
    const OUT_FLOOR: f32 = 60.0;

    let mut app = setup_in(HOLTBURG, HOUSE_CELL, true);
    let sbox = world_view(&mut app);
    let bearings = sweep_bearings(&mut app, 16);
    // The widest opening, so both an inside and an outside candidate exist at all.
    let Some(&(win_yaw, _, _)) = bearings.iter().filter(|b| b.1 > 0).max_by_key(|b| b.1) else {
        panic!("no bearing of {HOUSE_CELL:#010X} sees an opening: {bearings:?}")
    };
    face(&mut app, win_yaw, PICK_SETTLE);

    let polys = app
        .world_scene()
        .expect("a world scene")
        .outside_view_polys();
    assert!(
        !polys.is_empty(),
        "the window bearing publishes its openings' outlines"
    );
    let candidates = outdoor_candidates(&app, HOLTBURG);
    // The **most central** candidate of each kind, rather than the first: a point on the outline is
    // a coin toss and the station has no business making one.
    let margin_of = |px: (f32, f32)| -> (bool, f32) {
        let inside = polys.iter().any(|p| inside_poly(p, px));
        let d = polys
            .iter()
            .map(|p| distance_to_poly(p, px))
            .fold(f32::MAX, f32::min);
        (inside, d)
    };
    let ranked = |want_inside: bool| {
        let mut v: Vec<(u32, Vec3, (f32, f32), f32)> = candidates
            .iter()
            .filter_map(|c| {
                let (inside, d) = margin_of(c.2);
                (inside == want_inside).then_some((c.0, c.1, c.2, d))
            })
            .collect();
        v.sort_by(|a, b| b.3.total_cmp(&a.3));
        v
    };
    let in_candidates = ranked(true);
    let out_candidates = ranked(false);
    let (Some(&(in_cell, in_origin, in_px, in_margin)), Some(&(_, _, _, out_best_margin))) =
        (in_candidates.first(), out_candidates.first())
    else {
        panic!(
            "the opening at yaw {:.0} deg does not admit both an inside and an outside ground \
             point at all: {} candidates, outlines {polys:?}",
            win_yaw.to_degrees(),
            candidates.len()
        )
    };
    eprintln!(
        "window arm: through-the-window candidate {in_px:?} is {in_margin:.1} px inside \
         its opening; {} wall-side candidates, the clearest {out_best_margin:.1} px",
        out_candidates.len()
    );
    assert!(
        in_margin >= IN_FLOOR,
        "the widest opening's most central ground point is only {in_margin:.1} px from its \
         outline, which is inside a chest's own drawing sphere — this station cannot tell the \
         cone's answer from rounding. Outlines {polys:?}"
    );
    assert!(
        out_best_margin >= OUT_FLOOR,
        "the most clear-of-the-openings ground point is only {out_best_margin:.1} px away; same \
         reason"
    );

    let (through, through_name) =
        place_the_corpus_chest_in(&mut app, in_cell, in_origin, 6.0, CHEST_A);
    face(&mut app, win_yaw, PICK_SETTLE);
    let scene = app.world_scene().expect("a world scene");
    let through_parts = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(through))
        .count();
    eprintln!(
        "window arm: yaw {:.0} deg, count {:?}, through-the-window chest {:#010X} at \
         {in_px:?} -> {through_parts} parts; outlines {polys:?}",
        win_yaw.to_degrees(),
        scene.indoor_outside_view_count(),
        through.0,
    );
    assert!(
        through_parts > 0,
        "an outdoor chest inside the opening's own polygon was not submitted — the cone is \
         rejecting what retail draws through the window"
    );
    let at = object_pixel(&app, through);
    assert_eq!(ui_of(&mut app).hit_test_screen(at.0, at.1), Some(sbox));
    rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
    assert_eq!(
        app.interaction().pick.click_object().0,
        through,
        "an outdoor object drawn through the window must stay selectable ({through_name}); pick \
         stats {:?}",
        app.interaction().pick.stats
    );

    // The same frame, the other chest.
    //
    // **The candidates are tried in order and the first one that actually *places* wins.** The ring
    // search knows the ground height at a point; it does not know whether that point is inside a
    // neighbouring house's masonry, and `ObjectPhysics::place` refuses those. A chest with no
    // `SceneObject` is already excluded by the pick's `server_object_frame` gate, so believing one
    // here would measure the wrong thing.
    let mut attempt: Option<(ObjectId, String, (f32, f32), f32)> = None;
    let mut tried: Vec<Vec3> = Vec::new();
    for &(cell, origin, px, margin) in &out_candidates {
        // Spread the attempts out. The ranking is by margin, and the clearest points of one
        // bearing are all within a metre of each other — eight tries at one spot is one try.
        if tried
            .iter()
            .any(|p| math::hypotf(p.x - origin.x, p.y - origin.y) < 4.0)
        {
            continue;
        }
        if tried.len() >= 12 {
            break;
        }
        let id = ObjectId(CHEST_B.0 + u32::try_from(tried.len()).expect("a small loop counter"));
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let at = 9.0 + tried.len() as f64;
        tried.push(origin);
        let (placed, name) = place_the_corpus_chest_in(&mut app, cell, origin, at, id);
        if app
            .world_state()
            .expect("a world scene")
            .server_object_frame(placed)
            .is_some()
        {
            attempt = Some((placed, name, px, margin));
            break;
        }
        eprintln!("window arm: wall-side candidate {px:?} would not place");
    }
    let Some((beyond, beyond_name, out_px, out_margin)) = attempt else {
        panic!(
            "none of {} spread-out wall-side candidates placed; {} were offered",
            tried.len(),
            out_candidates.len()
        )
    };
    face(&mut app, win_yaw, PICK_SETTLE);
    let scene = app.world_scene().expect("a world scene");
    let beyond_parts = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(beyond))
        .count();
    let still_through = scene
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(through))
        .count();
    eprintln!(
        "window arm: wall-side chest {:#010X} at {out_px:?} -> {beyond_parts} parts \
         (through-the-window chest still {still_through}); stages {:?}, cone {:?}",
        beyond.0,
        scene
            .drawn_part_order()
            .iter()
            .filter(|p| p.object == Some(beyond))
            .map(|p| (p.part, p.before_depth_clear, p.outdoors))
            .collect::<Vec<_>>(),
        scene.drawn_object_cone(),
    );
    assert!(
        still_through > 0,
        "the second create disturbed the control: the through-the-window chest stopped drawing"
    );
    assert_eq!(
        beyond_parts, 0,
        "an outdoor chest {out_margin:.1} px clear of every opening was submitted anyway — \
         the portal mesh path must reject geometry outside every opening, and the mesh view \
         list must report the mesh outside the view cone when every result is outside"
    );
    // **The premise that makes the pick half of this arm mean anything.** If the wall-side chest
    // had no `SceneObject` at all, the pick's `server_object_frame` gate would already keep
    // it out of the sweep and this station would be measuring that instead of the cone.
    let scene = app.world_scene().expect("a world scene");
    eprintln!(
        "window arm premises: presence {:?}, draw cell {:?}, frame {:?}",
        app.objects()
            .presence(beyond)
            .and_then(|p| p.position)
            .map(|p| p.cell.0),
        scene.server_object_draw_cell(beyond).map(|c| c.0),
        scene
            .server_object_frame(beyond)
            .map(|f| (f.origin.x, f.origin.y, f.origin.z)),
    );
    assert!(
        scene.server_object_frame(beyond).is_some(),
        "the wall-side chest has no render frame, so the pick's gate excludes it before the cone \
         ever runs and this arm says nothing about `outside_view`"
    );
    let at = object_pixel(&app, beyond);
    assert_eq!(ui_of(&mut app).hit_test_screen(at.0, at.1), Some(sbox));
    let before = draw_list(&mut app);
    rest_pointer_at(&mut app, at.0, at.1, 4_000, 20);
    let after = draw_list(&mut app);
    let glyphs = glyphs_that_appeared(&before, &after);
    assert_ne!(
        app.interaction().pick.click_object().0,
        beyond,
        "hovering the wall beside the window named an outdoor object the frame never submitted; \
         pick stats {:?}",
        app.interaction().pick.stats
    );
    assert!(
        !glyphs.contains(&beyond_name),
        "nothing named it on screen; drew {glyphs:?}"
    );
}
