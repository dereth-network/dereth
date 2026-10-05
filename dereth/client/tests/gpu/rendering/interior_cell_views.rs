//! Interior cell views: each cell reached through a doorway is drawn through its own
//! portal-derived views, not the whole screen. The views have three consumers: portal depth
//! stamping clips each opening against its owning cell's views; cell meshes draw once per view
//! polygon; and the object pass classifies each part's drawing sphere against the cell's views, so
//! a part outside them is neither drawn nor offered to the selection ray even though its cell is
//! reached; a part of an object that reaches into several cells is drawn when any of those
//! cells' views holds it. Fixture: a headless App over the retail dats at a Holtburg house cell, with the body
//! and a chest taken from the `early-inventory-and-casting` recording; the stations read the
//! published view polygons, object submission and pick results, and the stamp counters, not pixels.

// The application test is available with Vulkan or Windows D3D12.
#![cfg(gpu)]

use crate::common::app::{frames, position};
use dereth_scene::world_scene::SceneReads;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::pick::PickScene;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_client_runtime::scene::SceneConfig;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemDeleteObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui::{ElemHandle, UiDrawCmd, UiSystem};

const SCREEN: (u32, u32) = (800, 600);
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
const SMART_BOX: dereth_ui::ElementId = dereth_ui_screens::hud::world_view::SMART_BOX;

/// A Holtburg house cell whose portal graph reaches further cells through successive doorways
/// (123 interior cells, none sealed; not recounted here).
const HOLTBURG: u16 = 0xA9B4;
const HOUSE_CELL: u32 = 0xA9B4_0146;
/// A room of the same house from which a neighbouring room's doorway leaves some of that room on
/// screen but outside the doorway: from the first station every point outside a reached cell's
/// view was also within about one sphere radius of it, or off screen (measured over 58 placements).
const DOORWAY_CELL: u32 = 0xA9B4_013F;

const CHEST: ObjectId = ObjectId(0x7DA5_5060);

/// The frames a bearing needs before its views are the settled camera's.
const SWEEP_SETTLE: usize = 20;
/// The frames a station needs before it may believe a **pick** — `Character::teleport` leaves the
/// viewer on the pivot, so a pick read too early answers with the body's own parts.
const PICK_SETTLE: usize = 90;

/// Sample points inside the cell BSP and outside its solid physics BSP, transformed to
/// block-local metres. It does not prove support under a body. Actual object placement and submission are checked later.
///
/// A pure function of the dat, so each cell is sampled once per process and kept.
fn points_in(store: &dereth_dat::RetailDatStore, cell: u32) -> Vec<Vec3> {
    static SAMPLED: OnceLock<Mutex<HashMap<u32, Vec<Vec3>>>> = OnceLock::new();
    let sampled = SAMPLED.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(p) = sampled
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&cell)
    {
        return p.clone();
    }
    let p = sample_points_in(store, cell);
    sampled
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(cell, p.clone());
    p
}

fn sample_points_in(store: &dereth_dat::RetailDatStore, cell: u32) -> Vec<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let Some(d) = dereth_world_data::env_cells::EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)
    else {
        return Vec::new();
    };
    let g = dereth_world_data::env_cells::physics_geometry(&d);
    let Some(bsp) = g.cell_bsp.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for zi in 0i32..=2 {
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
                out.push(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    out
}

fn yaw_quat(yaw: f32) -> Quat {
    let half = yaw * 0.5;
    Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half))
}

/// `early-inventory-and-casting`, decoded once per process: the body and the chest both come out of
/// it, and the placement search below creates a chest many times over.
fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(|| {
        Corpus::load("early-inventory-and-casting")
            .expect("corpus decodes")
            .expect("early-inventory-and-casting")
    })
}

/// A real headless `App` over Holtburg with the body standing in `cell`.
fn setup_in(block: u16, cell: u32, portal_clip: bool) -> Option<App> {
    let store = dereth_dat::testing::open_store_or_fail();
    let stand = *points_in(&store, cell)
        .first()
        .expect("the station cell has a standable point");

    let Ok(mut app) = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-interior-cell-views-not-created")
            .join("preferences.ini"),
        ..Config::default()
    }) else {
        panic!("the headless client did not start");
    };
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

    let corpus = corpus();
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
    Some(app)
}

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

/// The shoelace area of a screen polygon, in px².
fn poly_area(poly: &[(f32, f32)]) -> f32 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0f32;
    for i in 0..n {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % n];
        a += x1 * y2 - x2 * y1;
    }
    (a * 0.5).abs()
}

/// `early-inventory-and-casting`'s first named chest with a setup.
fn corpus_chest() -> ItemCreateObject {
    corpus()
        .blobs
        .iter()
        .filter(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
        .filter_map(|r| {
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..])).ok()
        })
        .find(|c| {
            c.0.wdesc.name.contains("Chest")
                && c.0.physicsdesc.bitfield & dereth_protocol::types::physicsdesc::flags::SETUP != 0
        })
        .expect("early-inventory-and-casting has a named chest with a SETUP")
}

/// `early-inventory-and-casting`'s chest, created at `origin` of `cell` under `id`, with no frame
/// run: the caller settles it.
fn create_the_corpus_chest_in(
    app: &mut App,
    cell: u32,
    origin: Vec3,
    at: f64,
    id: ObjectId,
) -> (ObjectId, String) {
    let mut create = corpus_chest();
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
    (id, name)
}

/// Remove the chest `id` from the world, as the server's delete would.
fn delete_the_corpus_chest(app: &mut App, id: ObjectId, at: f64) {
    let instance_sequence = corpus_chest().0.physicsdesc.timestamps.instance;
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_DELETE_OBJECT,
            body: dereth_protocol::write_body(&ItemDeleteObject {
                id,
                instance_sequence,
            })
            .expect("encodes"),
        },
        LocalTime(at),
    );
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

/// Reached interior cells other than the body's current cell, with their published view
/// polygons and summed areas, largest sum first. Overlapping polygons are not unioned.
fn reached_cells(app: &App) -> Vec<(u32, Vec<Vec<(f32, f32)>>, f32)> {
    let scene = app.world_scene().expect("a world scene");
    let here = scene.viewer_cell_id().map_or(0, |c| c.0);
    let mut out: Vec<(u32, Vec<Vec<(f32, f32)>>, f32)> = SceneReads::drawn_cells(&scene)
        .unwrap_or_default()
        .iter()
        .filter(|id| **id != here && !dereth_physics::landdefs::is_outdoors(CellId(**id)))
        .map(|id| {
            let polys = scene.cell_view_polys(CellId(*id));
            let area = polys.iter().map(|p| poly_area(p)).sum::<f32>();
            (*id, polys, area)
        })
        .collect();
    out.sort_by(|a, b| b.2.total_cmp(&a.2));
    out
}

// =================================================================================================

/// Behaviour: rendering.interior.each-reached-cell-draws-through-its-own-portal-view
/// The starting cell uses a full-screen view; other reached cells inherit clipped doorway
/// polygons. Choose among sixteen bearings by reached-cell count, then largest cell-area sum.
/// Require the starting cell's total published area to match the screen within one square pixel
/// and every other reached cell to have polygons whose summed area is smaller than the screen.
///
/// Publishing the full screen for all cells would fail this comparison. It does not measure
/// portal-graph depth, prove a particular polygon shape, or deduplicate overlapping view areas.
#[test]
fn a_cell_reached_through_a_doorway_has_a_view_smaller_than_the_screen() {
    let Some(mut app) = setup_in(HOLTBURG, HOUSE_CELL, true) else {
        return;
    };
    // Prefer more reached cells, breaking ties by the largest single cell's summed view area.
    let mut best = (0.0f32, 0.0f32, 0usize);
    for i in 0..16 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let yaw = i as f32 * std::f32::consts::TAU / 16.0;
        face(&mut app, yaw, SWEEP_SETTLE);
        let cells = reached_cells(&app);
        let area = cells.first().map_or(0.0, |c| c.2);
        if cells.len() > best.2 || (cells.len() == best.2 && area > best.1) {
            best = (yaw, area, cells.len());
        }
    }
    face(&mut app, best.0, SWEEP_SETTLE);

    let scene = app.world_scene().expect("a world scene");
    let here = scene.viewer_cell_id().expect("an indoor viewer");
    let own = scene.cell_view_polys(here);
    let cells = reached_cells(&app);
    #[allow(clippy::cast_precision_loss)] // the back buffer's extent
    let screen_area = SCREEN.0 as f32 * SCREEN.1 as f32;
    eprintln!(
        "interior views at {:.0} deg from {:#010X}: own {:?} px^2, reached {:?}",
        best.0.to_degrees(),
        here.0,
        own.iter().map(|p| poly_area(p)).collect::<Vec<_>>(),
        cells
            .iter()
            .map(|(id, p, a)| format!("{id:#010X}:{}poly/{a:.0}px^2", p.len()))
            .collect::<Vec<_>>()
    );

    assert!(
        !own.is_empty(),
        "the viewer's own cell has no published view, so the walk published nothing at all"
    );
    assert!(
        (own.iter().map(|p| poly_area(p)).sum::<f32>() - screen_area).abs() < 1.0,
        "the viewer's own cell has full-screen total view area; got {:?}",
        own.iter().map(|p| poly_area(p)).collect::<Vec<_>>()
    );
    assert!(
        !cells.is_empty(),
        "no cell beyond the viewer's own was reached at any of the sixteen bearings, so this \
         station has no doorway to measure"
    );
    for (id, polys, area) in &cells {
        assert!(
            !polys.is_empty(),
            "reached cell {id:#010X} is in drawn_cells with no published view"
        );
        assert!(
            *area < screen_area,
            "reached cell {id:#010X} is published with {area:.0} px^2 of view against a {screen_area:.0} px^2 screen; its total view area must be smaller"
        );
    }
}

/// The station's 0.8 m drawing-sphere bound, projected at chest distance. Sphere culling needs a
/// distance-dependent pixel margin; the test does not decode an independent bound.
const CHEST_RADIUS_M: f32 = 0.8;

/// What one chest's own cell view says at one bearing.
#[derive(Debug)]
struct Bearing {
    yaw: f32,
    reached: bool,
    inside: bool,
    clear: f32,
    proj_r: f32,
    parts: usize,
    pixel: (f32, f32),
    /// How far the chest's pixel is inside the screen's own edges; negative is off screen.
    on_screen: f32,
}

/// Thirty-two bearings, each chest of `chests` (with the cell it is drawn in) read at every one.
fn sweep_bearings(app: &mut App, chests: &[(ObjectId, u32)]) -> Vec<Vec<Bearing>> {
    let mut out: Vec<Vec<Bearing>> = chests.iter().map(|_| Vec::new()).collect();
    for i in 0..32 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let yaw = i as f32 * std::f32::consts::TAU / 32.0;
        face(app, yaw, SWEEP_SETTLE);
        let scene = app.world_scene().expect("a world scene");
        let viewer = PickScene::viewer(&scene);
        let fov = PickScene::fov_y_rad(&scene, SCREEN);
        let drawn = SceneReads::drawn_cells(&scene).unwrap_or_default();
        for ((chest, cell), sweep) in chests.iter().zip(out.iter_mut()) {
            let polys = scene.cell_view_polys(CellId(*cell));
            let parts = scene
                .drawn_part_order()
                .iter()
                .filter(|q| q.object == Some(*chest))
                .count();
            let Some(f) = scene.server_object_frame(*chest) else {
                continue;
            };
            let local = dereth_physics::math::globaltolocal(
                &viewer,
                Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.5),
            );
            if local.y < 1.0 {
                continue;
            }
            let px = pixel_of(local, SCREEN, fov);
            #[allow(clippy::cast_precision_loss)] // the back buffer's extent
            let vdst = (SCREEN.1 as f32 - 1.0) * 0.5 / math::tanf(fov * 0.5);
            let proj_r = vdst * CHEST_RADIUS_M / local.y;
            let clear = polys
                .iter()
                .map(|q| distance_to_poly(q, px))
                .fold(f32::MAX, f32::min);
            #[allow(clippy::cast_precision_loss)] // the back buffer's extent
            let on_screen =
                px.0.min(SCREEN.0 as f32 - 1.0 - px.0)
                    .min(px.1)
                    .min(SCREEN.1 as f32 - 1.0 - px.1);
            sweep.push(Bearing {
                yaw,
                reached: drawn.contains(cell),
                inside: polys.iter().any(|q| inside_poly(q, px)),
                clear,
                proj_r,
                parts,
                pixel: px,
                on_screen,
            });
        }
    }
    out
}

/// The bearing that puts the chest clearest inside its own cell's view.
fn seen_bearing(sweep: &[Bearing]) -> Option<&Bearing> {
    sweep
        .iter()
        .filter(|b| b.reached && b.inside && b.clear >= b.proj_r)
        .max_by(|a, b| a.clear.total_cmp(&b.clear))
}

/// The bearing that reaches the chest's cell with the chest on screen and clearest outside the
/// cell's view.
fn hidden_bearing(sweep: &[Bearing]) -> Option<&Bearing> {
    sweep
        .iter()
        .filter(|b| b.reached && !b.inside && b.clear >= 1.25 * b.proj_r && b.on_screen >= 0.0)
        .max_by(|a, b| a.clear.total_cmp(&b.clear))
}

/// The object pass installs the cell's views before testing parts and offering surviving meshes to
/// the selection ray. This station holds a selected chest placement fixed
/// while rotating the body/camera bearing, seeking one view containing its projected bound and
/// another that still reaches the cell but excludes that bound.
///
/// Most floor points lie inside the doorway view and the excluded ones tend to be near the
/// ceiling, where placement fails, so the search tries several placements and then chooses one
/// submitted chest in a reached cell other than the body's own. A cell's candidate placements are
/// created together, one chest each, and settled by one run of frames; the drawn ones are swept
/// together, and the first in candidate order with both bearings is kept and the others are
/// deleted. Only the bearing changes for the comparison.
///
/// **The excluding bearing has the chest on screen**, so the full-screen cone would draw it and
/// only the cell's own view refuses it: falling back to the full-screen cone for a cell's views
/// makes this red. An off-screen bearing could not tell the two apart.
///
/// Thirty-two sampled bearings supply the draw controls. The later hidden-bearing hover check
/// runs only if the settled pointer hits `<SBOX>`; a HUD-covered aim skips that arm. Thus the
/// unconditional evidence is part submission, while pick and new-element glyph checks are
/// conditional. No visible-bearing pick positive control or GPU pixel comparison runs here.
#[test]
fn the_same_object_is_drawn_and_named_only_from_bearings_its_cell_sees_it_through() {
    /// A view a chest can stand clear inside at all.
    const MIN_VIEW_AREA: f32 = 5_000.0;

    let Some(mut app) = setup_in(HOLTBURG, DOORWAY_CELL, true) else {
        return;
    };
    let sbox = world_view(&mut app);
    let store = dereth_dat::testing::open_store_or_fail();

    // ---- chests on the floor of a cell the walk reaches, and the bearings each is seen from ----
    //
    // A cell's candidates are created together and settled by one run of frames; the ones the
    // frame draws in a reached cell other than the body's own are swept together over the
    // thirty-two bearings, and the first, in candidate order, that has both a bearing its cell's
    // view contains and an on-screen bearing it excludes is kept. The rest are deleted.
    let mut placed: Option<(ObjectId, String, u32, Vec<Bearing>)> = None;
    let mut made: Vec<ObjectId> = Vec::new();
    let mut attempt = 0u32;
    let mut misses: Vec<String> = Vec::new();
    'place: for i in 0..16 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let yaw = i as f32 * std::f32::consts::TAU / 16.0;
        face(&mut app, yaw, SWEEP_SETTLE);
        for (id, _, area) in reached_cells(&app) {
            if area < MIN_VIEW_AREA {
                continue;
            }
            let base = CHEST.0 + attempt * 12;
            attempt += 1;
            // Spread over the whole cell rather than its first corner: a chest the screen shows
            // outside the doorway stands behind the wall beside it.
            let points = points_in(&store, id);
            let stride = (points.len() / 12).max(1);
            let candidates: Vec<(ObjectId, String)> = points
                .into_iter()
                .step_by(stride)
                .take(12)
                .enumerate()
                .map(|(n, p)| {
                    let oid = ObjectId(base + u32::try_from(n).expect("a small loop counter"));
                    #[allow(clippy::cast_precision_loss)] // a small loop counter
                    create_the_corpus_chest_in(&mut app, id, p, 6.0 + n as f64, oid)
                })
                .collect();
            made.extend(candidates.iter().map(|(c, _)| *c));
            frames(&mut app, 30);
            let eligible: Vec<(ObjectId, String, u32)> = {
                let scene = app.world_scene().expect("a world scene");
                let here = scene.viewer_cell_id().map_or(0, |c| c.0);
                let reached = SceneReads::drawn_cells(&scene).unwrap_or_default();
                // Placement can change the effective draw cell (a chest created in 0xA9B40146
                // can settle in the body's own 0xA9B40143, whose full-screen view makes the
                // comparison meaningless). Require an actual submitted part and
                // server_object_draw_cell in a reached cell other than the body's own.
                candidates
                    .iter()
                    .filter_map(|(chest, name)| {
                        let drawn_in = scene.server_object_draw_cell(*chest)?;
                        (scene.server_object_frame(*chest).is_some()
                            && scene
                                .drawn_part_order()
                                .iter()
                                .any(|q| q.object == Some(*chest))
                            && drawn_in.0 != here
                            && reached.contains(&drawn_in.0))
                        .then(|| (*chest, name.clone(), drawn_in.0))
                    })
                    .collect()
            };
            if eligible.is_empty() {
                continue;
            }
            let sweeps = sweep_bearings(
                &mut app,
                &eligible
                    .iter()
                    .map(|(c, _, cell)| (*c, *cell))
                    .collect::<Vec<_>>(),
            );
            for ((chest, name, cell), sweep) in eligible.into_iter().zip(sweeps) {
                let best = sweep
                    .iter()
                    .filter(|b| b.reached && !b.inside && b.on_screen >= 0.0)
                    .map(|b| b.clear / b.proj_r)
                    .fold(0.0f32, f32::max);
                misses.push(format!(
                    "{:#010X} in {cell:#010X}: seen {}, best on-screen outside clearance {best:.2} r",
                    chest.0,
                    seen_bearing(&sweep).is_some()
                ));
                if seen_bearing(&sweep).is_some() && hidden_bearing(&sweep).is_some() {
                    placed = Some((chest, name, cell, sweep));
                    break 'place;
                }
            }
        }
    }
    let Some((chest, name, cell, sweep)) = placed else {
        panic!(
            "no reached cell of {DOORWAY_CELL:#010X} admitted a chest the frame draws, seen through \
             its cell's view from one bearing and on screen outside it from another: {misses:#?}"
        )
    };
    // The chest under test is the only one left standing, so nothing else in the world wears its
    // name.
    let others: Vec<ObjectId> = made
        .into_iter()
        .filter(|other| *other != chest && app.objects().presence(*other).is_some())
        .collect();
    for other in &others {
        delete_the_corpus_chest(&mut app, *other, 40.0);
    }
    frames(&mut app, 5);
    assert!(
        others
            .iter()
            .all(|other| app.objects().presence(*other).is_none()),
        "a candidate chest outlived its delete"
    );
    eprintln!(
        "interior sweep of {name} {:#010X} in draw cell {cell:#010X}: {:?}",
        chest.0,
        sweep
            .iter()
            .map(|b| format!(
                "{:.0}deg r{} i{} c{:.0}/{:.0} s{:.0} p{}",
                b.yaw.to_degrees(),
                u8::from(b.reached),
                u8::from(b.inside),
                b.clear,
                b.proj_r,
                b.on_screen,
                b.parts
            ))
            .collect::<Vec<_>>()
    );

    // ---- the control: a bearing whose cell view contains it ------------------------------------
    let seen = seen_bearing(&sweep).expect("chosen for having one");
    assert!(
        seen.parts > 0,
        "at {:.0} deg the chest is {:.0} px inside cell {cell:#010X}'s own view (sphere {:.0} px) and the object pass submitted nothing",
        seen.yaw.to_degrees(),
        seen.clear,
        seen.proj_r
    );

    // ---- the claim: a bearing that still reaches the cell but no longer sees it there ----------
    //
    // The chest's pixel is on screen there: the whole screen would admit it, so only the cell's
    // own view can refuse it. A bearing that put it off screen would be refused by the
    // full-screen cone as well, and could not tell the two apart.
    let hidden = hidden_bearing(&sweep).expect("chosen for having one");
    assert_eq!(
        hidden.parts, 0,
        "at {:.0} deg cell {cell:#010X} is still on the draw list but the chest is {:.0} px outside the polygon it is seen through (sphere {:.0} px, pixel {:?}, {:.0} px inside the screen); this build submitted {} subsets",
        hidden.yaw.to_degrees(),
        hidden.clear,
        hidden.proj_r,
        hidden.pixel,
        hidden.on_screen,
        hidden.parts
    );

    // ---- and the pick agrees with the draw ------------------------------------------------------
    face(&mut app, hidden.yaw, PICK_SETTLE);
    let at = {
        let scene = app.world_scene().expect("a world scene");
        let f = scene.server_object_frame(chest).expect("a frame");
        aim(&app, Vec3::new(f.origin.x, f.origin.y, f.origin.z + 0.5))
    };
    if ui_of(&mut app).hit_test_screen(at.0, at.1) == Some(sbox) {
        let before = draw_list(&mut app);
        rest_pointer_at(&mut app, at.0, at.1, 2_000, 20);
        let after = draw_list(&mut app);
        let glyphs = glyphs_that_appeared(&before, &after);
        eprintln!(
            "interior hover at {at:?} from {:.0} deg named {:#010X}",
            hidden.yaw.to_degrees(),
            app.interaction().pick.click_object().0 .0
        );
        assert_ne!(
            app.interaction().pick.click_object().0,
            chest,
            "hovering an object its own cell's view does not contain named it ({name}); pick \
             stats {:?}",
            app.interaction().pick.stats
        );
        assert!(
            !glyphs.contains(&name),
            "no new draw-command glyphs named it; drew {glyphs:?}"
        );
    }
}

/// Portal stamping clips each opening against the currently installed cell view. An opening
/// outside that view contributes no stamp for that opening/view pair.
///
/// The disabled-clipping control may empty openings too: a reached cell's opening can project off
/// screen and be emptied by the viewport clip.
///
/// The current predicates require positive issued and emptied counts with clipping enabled,
/// positive issued count with it disabled, and fewer total evaluated pairs in the enabled run.
/// They distinguish the traversal modes in this scene but do not independently attribute each
/// rejection to a cell boundary rather than a screen boundary, or compare stamp pixels.
#[test]
fn the_indoor_stamps_are_clipped_to_their_own_cells_views() {
    let Some(mut app) = setup_in(HOLTBURG, HOUSE_CELL, true) else {
        return;
    };
    let mut clipped_total = 0u64;
    let mut issued_total = 0u64;
    for i in 0..16 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let yaw = i as f32 * std::f32::consts::TAU / 16.0;
        face(&mut app, yaw, SWEEP_SETTLE);
        let (issued, clipped) = app
            .world_scene()
            .expect("a world scene")
            .drawn_portal_stamps();
        issued_total += issued;
        clipped_total += clipped;
    }
    eprintln!(
        "interior stamps, clipped build: {issued_total} issued, {clipped_total} clipped away over \
         sixteen bearings"
    );
    assert!(
        issued_total > 0,
        "no indoor stamp was issued at any bearing, so the clipping has nothing to act on and the \
         count below would be vacuous"
    );
    assert!(
        clipped_total > 0,
        "no opening/view pair was clipped away in this enabled-clipping sweep"
    );

    let Some(mut app) = setup_in(HOLTBURG, HOUSE_CELL, false) else {
        return;
    };
    let mut unclipped_clipped = 0u64;
    let mut unclipped_issued = 0u64;
    for i in 0..16 {
        #[allow(clippy::cast_precision_loss)] // a small loop counter
        let yaw = i as f32 * std::f32::consts::TAU / 16.0;
        face(&mut app, yaw, SWEEP_SETTLE);
        let (issued, clipped) = app
            .world_scene()
            .expect("a world scene")
            .drawn_portal_stamps();
        unclipped_issued += issued;
        unclipped_clipped += clipped;
    }
    eprintln!(
        "interior stamps, unclipped build: {unclipped_issued} issued, {unclipped_clipped} clipped away"
    );
    assert!(
        unclipped_issued > 0,
        "the unclipped build issued no stamp either, so the comparison is between two zeroes"
    );
    assert!(
        issued_total + clipped_total < unclipped_issued + unclipped_clipped,
        "the clipped build evaluated {} (opening, view) pairs against the unclipped build's {}; the enabled-clipping sweep must evaluate fewer",
        issued_total + clipped_total,
        unclipped_issued + unclipped_clipped
    );
}

/// The local body's drawn parts this frame, by part index.
fn body_parts_drawn(app: &App) -> BTreeSet<usize> {
    app.world_scene()
        .expect("a world scene")
        .drawn_part_order()
        .iter()
        .filter(|q| q.object.is_none())
        .map(|q| q.part)
        .collect()
}

/// A body is drawn by every reached cell it overlaps, each under its own view: standing on the
/// cellar stairs with the camera in the room above, the body's own cell is seen only through the
/// stairwell, which leaves the upper body outside it, but the room the camera is in sees the
/// whole screen, so every part on screen is drawn. Testing the parts against the body's own cell
/// alone draws the legs and drops the rest.
///
/// The station is a placement on the ground floor over the stairwell that settles onto the stairs
/// in the cellar's top cell, facing north-east, which puts the production chase camera in the
/// room above. The oracle is the same station with portal clipping off, where every cell is
/// seen through the whole screen: the clipped frame must draw exactly the body parts it draws.
/// Behaviour: rendering.interior.a-body-in-several-cells-is-drawn-by-each-of-them
#[test]
fn a_body_on_the_cellar_stairs_is_drawn_whole_from_the_room_above() {
    let clipped = cellar_stairs_station(true);
    let unclipped = cellar_stairs_station(false);
    assert!(
        unclipped.len() > 10,
        "the unclipped frame draws the body: {} parts",
        unclipped.len()
    );
    let missing: Vec<usize> = unclipped.difference(&clipped).copied().collect();
    assert!(
        missing.is_empty(),
        "every part of the body on screen is drawn from the room above; {} of {} drawn, missing {missing:?}",
        clipped.len(),
        unclipped.len()
    );
}

/// The body's drawn parts at the cellar-stairs station, with portal clipping on or off.
fn cellar_stairs_station(portal_clip: bool) -> BTreeSet<usize> {
    const GROUND_FLOOR: u32 = 0xA9B4_0143;
    const CELLAR_TOP: u32 = 0xA9B4_0147;
    let mut app = setup_in(HOLTBURG, HOUSE_CELL, portal_clip).expect("the station app");
    {
        let mut scene = app.world_scene_mut().expect("a world scene");
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            CellId(GROUND_FLOOR),
            Frame::new(
                Vec3::new(141.5, 7.5, 94.5),
                yaw_quat(std::f32::consts::FRAC_PI_4),
            ),
        ));
    }
    frames(&mut app, SWEEP_SETTLE);
    let (body_cell, camera_cell, shadows) = {
        let scene = app.world_scene().expect("a world scene");
        let c = scene.character.as_ref().expect("a body");
        let shadows: Vec<CellId> = c
            .world
            .get(c.handle)
            .map(|b| b.shadow_objects.iter().map(|s| s.cell_id).collect())
            .unwrap_or_default();
        (c.position().cell, c.camera.viewer_cell, shadows)
    };
    assert_eq!(
        body_cell,
        CellId(CELLAR_TOP),
        "the body settles on the stairs in the cellar's top cell"
    );
    assert_eq!(
        camera_cell,
        Some(CellId(GROUND_FLOOR)),
        "the camera is in the room above"
    );
    assert!(
        shadows.contains(&CellId(GROUND_FLOOR)),
        "the body reaches up into the room above: {shadows:?}"
    );
    body_parts_drawn(&app)
}
