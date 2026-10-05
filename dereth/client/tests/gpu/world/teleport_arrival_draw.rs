//! A server teleport into a dungeon from outside draws what logging in there draws: the same
//! streaming window, the same interior traversal and the same painted pixel count. Driven through
//! a real headless `App`: the server's `Movement_PositionEvent 0xF748` with an advanced teleport
//! timestamp is fed in, and `App::player_teleport_use_time` -> `complete_player_teleport_at` ->
//! `apply_player_teleport_at` moves the body with the destination block prefetched. That is the one
//! path that relocates the player on portal travel; a later `Item_CreateObject 0xF745` for the
//! player moves nothing, and `Effects_PlayerTeleport 0xF751` carries no position. Station: Drudge
//! Hideout `0x019E_0114`, teleported into from Holtburg `0xA9B4_0021` and logged into directly.
//! Fixture: the retail dats; fails without them or a headless device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::app::App;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::scene::SceneConfig;
use dereth_dat::RetailDatStore;
use dereth_primitives::{CellId, LocalTime, ObjectId, Vec3};
use dereth_protocol::movement::{MovementPositionEvent, PositionPack};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::{physicsdesc, Origin, PositionWire};
use dereth_protocol::Opcode;
use dereth_scene::world_scene::SceneReads;
use std::sync::Arc;
use {dereth_world_data::env_cells::physics_geometry, dereth_world_data::env_cells::EnvCellLoader};

const W: u32 = 400;
const H: u32 = 300;

/// **Drudge Hideout**, `weenie 2068`'s portal destination in the ACE world database.
const DRUDGE_BLOCK: u16 = 0x019E;
const DRUDGE_CELL: u32 = 0x019E_0114;

/// The outdoor block the client is standing on when the portal is used. Any block but the
/// dungeon's does; this one is the usual Holtburg station.
const SURFACE: u16 = 0xA9B4;
/// A landcell of [`SURFACE`], so the "before" body is genuinely outdoors.
const SURFACE_CELL: u32 = 0xA9B4_0021;

const PLAYER: ObjectId = ObjectId(0x5000_0F30);

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn app() -> Option<App> {
    App::new(Config {
        headless: true,
        sound: false,
        ui: false,
        width: W,
        height: H,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dere-teleport-arrival-draw-not-created")
            .join("preferences.ini"),
        ..Config::default()
    })
    .map(Some)
    .unwrap_or_else(|e| panic!("the gpu tier needs a headless device and none opened: {e}"))
}

fn scene(landblock: u16, start_cell: Option<u32>) -> SceneConfig {
    SceneConfig {
        landblock,
        start_cell: start_cell.map(CellId),
        time_of_day: Some(0.35),
        character: true,
        ..SceneConfig::default()
    }
}

/// A non-solid point inside `cell`. The same search `world::dungeon_landblock_window::teleport_into_a_dungeon` uses, and for
/// the same reason: a dungeon's cells share frame origins that `WorldScene::standable_point`'s
/// +/-6 m box does not cover.
fn point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let d = EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = physics_geometry(&d);
    let bsp = g.cell_bsp.as_ref()?;
    for zi in -24i32..=24 {
        for i in -40i32..=40 {
            for j in -40i32..=40 {
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

/// `Item_CreateObject 0xF745` for the player, at `cell`. Establishes the `Presence` that
/// `ObjectStream::received_position` needs and the timestamps its gates are measured against.
fn create_player_at(cell: u32, origin: Vec3) -> SessionEvent {
    let mut p = ObjectCreatePayload {
        id: PLAYER,
        ..ObjectCreatePayload::default()
    };
    p.physicsdesc.bitfield |= physicsdesc::flags::SETUP | physicsdesc::flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.position = Some(PositionWire {
        objcell_id: cell,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: origin.x,
                y: origin.y,
                z: origin.z,
            },
            orientation: dereth_protocol::types::Quat::default(),
        },
    });
    p.physicsdesc.timestamps.instance = 1;
    let body = dereth_protocol::write_blob(&ItemCreateObject(p)).expect("the create encodes");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_CREATE_OBJECT,
        body: body[4..].to_vec(),
    }
}

/// `Movement_PositionEvent 0xF748` about the player, with `TELEPORT_TS` advanced. Retail responds
/// by relocating the player through its simple-position path. This is the message ACE sends for
/// portal travel and the server-teleport path exercised here.
fn server_teleport(cell: u32, origin: Vec3, teleport_ts: u16) -> SessionEvent {
    let m = MovementPositionEvent {
        id: PLAYER,
        position: PositionPack {
            flags: 0,
            origin: Origin {
                objcell_id: cell,
                origin: dereth_protocol::types::Vec3 {
                    x: origin.x,
                    y: origin.y,
                    z: origin.z,
                },
            },
            orientation: dereth_protocol::types::Quat::default(),
            velocity: None,
            placement_id: None,
            instance_timestamp: 1,
            position_timestamp: teleport_ts,
            teleport_timestamp: teleport_ts,
            force_position_timestamp: 0,
        },
    };
    let body = dereth_protocol::write_blob(&m).expect("the position event encodes");
    SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_POSITION_EVENT,
        body: body[4..].to_vec(),
    }
}

/// What an arrival looks like once the picture has settled.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Arrival {
    painted: usize,
    window: Option<(i32, i32)>,
    settled: u32,
    traversal: (usize, usize),
    origin: Vec3,
}

fn drive(app: &mut App, frames: u32) {
    for _ in 0..frames {
        assert!(app.frame(), "the app's own frame loop ended early");
    }
}

/// Run until the painted total stabilizes or 64 frames elapse, then read the arrival from `App`.
fn settle(app: &mut App) -> Arrival {
    let mut last = usize::MAX;
    let mut stable = 0;
    let mut trace = Vec::new();
    for i in 0..64 {
        assert!(app.frame());
        let p = app.renderer_mut().lit_pixel_count().expect("a capture");
        trace.push(p);
        if p == last {
            stable += 1;
            if stable >= 6 && i >= 8 {
                break;
            }
        } else {
            stable = 0;
        }
        last = p;
    }
    if std::env::var("DERETH_TEST_TELEPORT_ARRIVAL_TRACE").is_ok() {
        eprintln!("teleport arrival trace: {trace:?}");
    }
    let painted = app.renderer_mut().lit_pixel_count().expect("a capture");
    // A PNG for eyeballing, when a human is driving the suite.
    if let Ok(dir) = std::env::var("DERETH_TEST_TELEPORT_ARRIVAL_DUMP") {
        let n = std::path::Path::new(&dir)
            .read_dir()
            .map_or(0, Iterator::count);
        app.renderer_mut()
            .capture_png(&std::path::Path::new(&dir).join(format!("teleport_arrival_{n}.png")))
            .expect("dump");
    }
    let world = app.world_scene().expect("a world");
    let c = world.character.as_ref().expect("a body");
    Arrival {
        painted,
        window: world.viewer_block(),
        settled: c.position().cell.0,
        traversal: world.indoor_traversal_counts(),
        origin: c.position().frame.origin,
    }
}

/// **Teleport in from outside**, as a player taking a portal does: the scene is standing on an
/// outdoor landblock, the server sends one `0xF748` whose `TELEPORT_TS` advances, and nothing else
/// in the test touches the body.
fn teleport_in(store: &Arc<RetailDatStore>, point: Vec3) -> Option<Arrival> {
    let mut app = app()?;
    app.probe_mut()
        .load_world(store, scene(SURFACE, None))
        .expect("the surface loads");
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(0.0));
    app.probe_mut().objects_mut().apply_event(
        &create_player_at(SURFACE_CELL, Vec3::new(96.0, 96.0, 0.0)),
        LocalTime(0.0),
    );
    drive(&mut app, 4);
    app.probe_mut()
        .objects_mut()
        .apply_event(&server_teleport(DRUDGE_CELL, point, 1), LocalTime(1.0));
    Some(settle(&mut app))
}

/// **Log in inside**, the control: the scene is built on the dungeon's own block and the
/// body is placed in the destination cell before the first frame. The same `0xF748` then arrives,
/// so the arms compare different initial scene/body placements followed by the same server move.
fn log_in(store: &Arc<RetailDatStore>, point: Vec3) -> Option<Arrival> {
    let mut app = app()?;
    app.probe_mut()
        .load_world(store, scene(DRUDGE_BLOCK, Some(DRUDGE_CELL)))
        .expect("the dungeon loads");
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(0.0));
    app.probe_mut()
        .objects_mut()
        .apply_event(&create_player_at(DRUDGE_CELL, point), LocalTime(0.0));
    drive(&mut app, 4);
    app.probe_mut()
        .objects_mut()
        .apply_event(&server_teleport(DRUDGE_CELL, point, 1), LocalTime(1.0));
    Some(settle(&mut app))
}

/// Behaviour: world.teleport.a-teleport-into-a-dungeon-draws-what-logging-in-there-draws
///
/// **Both arrivals end in the same cell at the same point by the same message, and draw the same
/// dungeon.**
#[test]
fn a_server_teleport_into_a_dungeon_draws_what_logging_in_there_draws() {
    let store = store();
    let point = point_in(&store, DRUDGE_CELL).expect("the destination cell has a standable point");
    let Some(teleported) = teleport_in(&store, point) else {
        return;
    };
    let Some(login) = log_in(&store, point) else {
        return;
    };
    eprintln!(
        "drudge {DRUDGE_CELL:#010X} ({} pixels):
           teleport from {SURFACE:#06X}: settled {:#010X} window {:?} traversal {:?} painted {} origin {:?}
           login inside:                 settled {:#010X} window {:?} traversal {:?} painted {} origin {:?}",
        (W * H) as usize,
        teleported.settled, teleported.window, teleported.traversal, teleported.painted,
        teleported.origin,
        login.settled, login.window, login.traversal, login.painted, login.origin,
    );
    // The premise: the server's message really did move the body into the dungeon on both arms.
    for (what, a) in [("teleported", &teleported), ("login", &login)] {
        assert_eq!(
            a.settled, DRUDGE_CELL,
            "the {what} arm's body settled in {:#010X}, not in the destination cell",
            a.settled
        );
    }
    // Non-vacuity: the control really is drawing an interior.
    assert!(
        login.traversal.0 > 1 && login.traversal.1 == login.traversal.0 && login.painted > 0,
        "the login control reached {:?} cells and {} pixels -- it is not drawing an interior",
        login.traversal,
        login.painted
    );
    assert_eq!(
        teleported.window, login.window,
        "the teleported arrival's streaming window is {:?} and the login arrival's is {:?}",
        teleported.window, login.window
    );
    assert_eq!(
        teleported.traversal, login.traversal,
        "the teleported arrival traverses {:?} interior cells and the login arrival {:?} -- the \
         dungeon that draws on login does not draw on a teleport",
        teleported.traversal, login.traversal
    );
    assert_eq!(
        teleported.painted, login.painted,
        "the teleported arrival paints {} pixels and the login arrival {}",
        teleported.painted, login.painted
    );
}
