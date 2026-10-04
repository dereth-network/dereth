//! Corpse decay: when a decayed corpse leaves the screen, and why it can stay on screen when the
//! server stops talking to the observer.
//!
//! In range, the recorded `0xF755 { corpse, Destroy = 89 }` followed 1.016608 s later by the
//! recorded `0xF747 { corpse, instance 0 }` retires the corpse's model, presence, collision body,
//! rendered body, opened-corpse marker and selection in one frame. A corpse stays on screen only
//! in a sequence where the observer never receives that `0xF747`; this module enumerates those
//! sequences.
//!
//! Fixture: long-solo-play's recorded corpse (create, decay script and delete), fed through a
//! socket-free replay endpoint into a real `App`, plus a `WorldScene` walked across landblocks.
//!
//! # What ACE does
//!
//! * A corpse's decay broadcasts the `Destroy` play-script, waits **1.0 s**, then destroys the
//!   object: exactly the recorded pair.
//! * Time to rot is **5 minutes** for a corpse with loot, **15 s** once its inventory is empty,
//!   `max(3600, level*300)` for a player corpse.
//! * Destroying the object removes it from its landblock, which (outside an adjacency move)
//!   broadcasts the delete `0xF747` to the corpse's own **known-players set**. A player who is not
//!   in it at that instant is told nothing, ever.
//! * Outdoor visibility is Chebyshev landblock distance `<= 1`: a **3x3 landblock** window.
//! * An object that leaves that window is queued for removal; after **25 s** the server drops
//!   it from the player's known objects and the player from the object's known players. **No
//!   `0xF747` is sent**: the client is expected to cull its own copy.
//! * The corpse never re-adds the player by itself (a corpse on the ground enters no cells); only
//!   the observer's own next visibility entry re-adds him.
//!
//! So the ACE timeline for observer **A** and corpse **C**, with A leaving C's 3x3 block window
//! at `t0` and the corpse's decay firing at `t1`:
//!
//! ```text
//! t0            A's visibility update queues C   (no message)
//! t0 + 25 s     A's removal pass drops C, and drops A from C's KnownPlayers   (no message)
//! t1            C decays: 0xF755 Destroy, +1.0 s, Destroy() -> 0xF747
//!               ... to C's KnownPlayers, which no longer contains A
//! t2            A returns. C is gone from the landblock, so nothing is sent for it at all.
//! ```
//!
//! **A is never told, in any ordering where `t1 > t0 + 25 s` and A has not come back first.**
//! For a looted-empty corpse `t1 - t_death` is 15 s, but for a corpse with loot still in it it is
//! **5 minutes**, which is far longer than it takes to walk out of a 3x3 block window.
//!
//! # What the retail client does with that silence
//!
//! Nothing. The client queues an object for its 25 s destruction only for a create it could not
//! place, placeholders for a blob about an unknown ID, a container's contents, and preparation to
//! leave visibility. The last is reached from the object-cell release chain, which starts with a
//! **cell or landblock release**, and from two internal-placement null-cell arms. None of those
//! is a distance test.
//!
//! The client's landscape window has a registered radius of **8**
//! (`Render.LandscapeDrawDistance`), and its smallest shipped preset is **3** (the five quality
//! presets are 3, 5, 8, 11, and 15). A scrolling window releases only the blocks pushed off an
//! edge.
//!
//! So ACE stops talking about an object at Chebyshev block distance **2**, and the retail client
//! cannot forget it until block distance **landscape radius + 1**, i.e. 4 at the cheapest setting
//! and 9 at the default. In the band between, a retail client on an ACE server holds a corpse
//! the server has already destroyed, for as long as the player stays in the band. The ghost is
//! **parity** with retail, and the tests below measure that this client sits at exactly the same
//! boundaries.
//!
//! Socket-free: every blob is admitted through the encrypted replay endpoint and the ordinary
//! `App::frame` order, and the 25 s deadline is crossed with a real `TimeSync` optional header
//! that advances the application clock, never by a model call.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::{retail_store, test_gpu};
use dereth_client::world::{SceneReads, SceneWrites};
use std::sync::Arc;

use dereth_client::app::App;
use dereth_client::character::CharacterInput;
use dereth_client::config::Config;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{block_xy, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_dat::RetailDatStore;
use dereth_primitives::{
    CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, ServerTime, Vec3,
};
use dereth_protocol::{
    movement::{position_flags, MovementPositionEvent, PositionPack},
    objects::{ItemCreateObject, ItemSetState, LoginCreatePlayer, ObjectCreatePayload},
    types::{weeniedesc::header, PhysicsEventStamp, PositionWire, PublicWeenieDesc},
    Message,
};
use dereth_render::device::Gpu;
use dereth_ui_screens::mapradar::radar::{inq_showable_on_radar, radar_enum};

/// long-solo-play's recorded corpse.
const CORPSE: ObjectId = ObjectId(0x8000_0A3C);
/// The observing character, so the `0xF748`s below take the real local-teleport path.
const LOCAL: ObjectId = ObjectId(0x5000_1A90);

/// The client's destruction queue records the current time plus 25.0 seconds. ACE's destruction
/// time is also 25 seconds, which is why ACE can leave the cull to the client at all.
const DESTRUCTION_TIME: f64 = 25.0;
/// ACE's default time to rot: 5 minutes for a corpse that still has loot in it.
const DEFAULT_TIME_TO_ROT: f64 = 300.0;
/// ACE's outdoor visibility: Chebyshev landblock distance `<= 1`.
const ACE_PVS_BLOCK_RADIUS: i32 = 1;
/// The cheapest shipped landscape-radius preset.
const SMALLEST_SHIPPED_MID_RADIUS: i32 = 3;

const VISIBLE: u32 = 0x0040_0408;
const PLAYER_BIT: u32 = 0x0000_0008;

// =================================================================================================
// Socket-free peer, which can put a real `TimeSync` header on a datagram
// =================================================================================================

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn attach(app: &mut App) -> Self {
        let mut net = dereth_client::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "corpse-decay",
            "unused",
            0,
        )
        .unwrap();
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        app.attach_replay_network(net).unwrap();
        Self {
            crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
            sequence: 1,
            blob: 0,
        }
    }

    fn send<M: Message>(&mut self, app: &mut App, message: &M) {
        self.send_batch(app, &[dereth_protocol::write_blob(message).unwrap()], None);
    }

    /// A recorded server blob, byte for byte.
    fn send_recorded(&mut self, app: &mut App, row: &CorpusBlob) {
        assert_eq!(row.dir, Direction::ServerToClient);
        assert_eq!(row.queue, dereth_primitives::NetQueue::WorldObjects);
        self.send_batch(app, &[row.payload.clone()], None);
    }

    fn send_batch(&mut self, app: &mut App, blobs: &[Vec<u8>], server_time: Option<f64>) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        if let Some(t) = server_time {
            packet
                .add_optional_header(
                    dereth_transport::PacketFlags::TIME_SYNC,
                    t.to_le_bytes().to_vec(),
                )
                .unwrap();
        }
        for blob in blobs {
            self.blob += 1;
            packet
                .add_fragment(dereth_transport::Fragment::new(
                    dereth_transport::FragmentHeader {
                        blob_id_low: self.blob,
                        blob_id_high: 0x8000_0000,
                        num_frags: 1,
                        blob_frag_size: 0,
                        blob_num: 0,
                        queue_id: 10,
                    },
                    blob.clone(),
                ))
                .unwrap();
        }
        let raw = packet.serialize(Some(self.crypto.next())).unwrap();
        app.replay_network_mut()
            .unwrap()
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .unwrap();
    }

    /// Push the server clock forward and change nothing else. The blob beside the header is a
    /// state repeat that the state-timestamp gate refuses.
    fn silence_until(&mut self, app: &mut App, t: f64) {
        let repeat = dereth_protocol::write_blob(&ItemSetState {
            id: LOCAL,
            state: VISIBLE,
            timestamps: PhysicsEventStamp {
                instance: 1,
                event: 0,
            },
        })
        .unwrap();
        self.send_batch(app, &[repeat], Some(t));
        frames(app, 3);
    }
}

// =================================================================================================
// The recorded corpse
// =================================================================================================

struct Recorded {
    create: ObjectCreatePayload,
    destroy: CorpusBlob,
    delete: CorpusBlob,
}

/// long-solo-play rows 862 / 1099 / 1114, with their own assertions on what they are, so a corpus
/// change fails here rather than silently changing the subject.
fn recorded() -> Recorded {
    let corpus = Corpus::load("long-solo-play")
        .unwrap()
        .expect("long-solo-play fixture");
    let row = |idx| {
        corpus
            .blobs
            .iter()
            .find(|r| r.idx == idx)
            .expect("recorded row")
            .clone()
    };
    let (create, destroy, delete) = (row(862), row(1099), row(1114));
    assert_eq!(
        (create.opcode, destroy.opcode, delete.opcode),
        (0xF745, 0xF755, 0xF747)
    );
    assert_eq!(
        destroy.payload.as_slice(),
        &[0x55, 0xF7, 0, 0, 0x3C, 0x0A, 0, 0x80, 0x59, 0, 0, 0, 0, 0, 0x80, 0x3F],
        "the effect is script type Destroy (89), intensity 1"
    );
    assert_eq!(
        delete.payload.as_slice(),
        &[0x47, 0xF7, 0, 0, 0x3C, 0x0A, 0, 0x80, 0, 0, 0, 0],
        "the recorded delete names instance 0"
    );
    assert_eq!(
        delete.t_rel_micros - destroy.t_rel_micros,
        1_016_608,
        "ACE's delay between the two is 1.0 s"
    );

    let payload = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&create.payload[4..]))
        .unwrap()
        .0;
    assert_eq!(payload.id, CORPSE);
    assert_eq!(payload.physicsdesc.timestamps.instance, 0);
    assert_ne!(
        payload.wdesc.bitfield & dereth_client_model::weenie::bitfield::CORPSE,
        0,
        "the recorded weenie really is a corpse"
    );
    // **Where the recorded corpse actually stood, and why the tests do not stand there.**
    // `0x7F03023A` is an *interior* cell (index >= 0x100) of a dungeon landblock: that block has
    // no walkable terrain to place an outdoor body on (`DatLandSource::ground_height` answers
    // `None` and the offline body sits at z = 0), so a create in its land cells never places and
    // every arm below would be measuring the object-creation unplaceable tail instead of the
    // decay sequence. The tests therefore put this same corpse on Holtburg's terrain; the only
    // field they change is the position.
    let objcell_id = payload
        .physicsdesc
        .position
        .as_ref()
        .expect("world corpse position")
        .objcell_id;
    assert_eq!(
        objcell_id, 0x7F03_023A,
        "long-solo-play's corpse stood in an interior cell"
    );
    Recorded {
        create: payload,
        destroy,
        delete,
    }
}

/// The recorded corpse create with its **position** moved to `at` and nothing else touched, so
/// the body lands in front of the observer's camera and the draw pass is a real oracle. Every
/// other field -- the corpse bitfield, the setup, the object description, instance 0 -- is the
/// wire's.
fn corpse_create_at(recorded: &Recorded, at: Position) -> ItemCreateObject {
    let mut payload = recorded.create.clone();
    payload.physicsdesc.position = Some(PositionWire {
        objcell_id: at.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: at.frame.origin.into(),
            orientation: at.frame.rotation.into(),
        },
    });
    ItemCreateObject(payload)
}

/// The observing player's own description needs `RADAR_ENUM` before it can appear on the radar.
fn observer_wdesc() -> PublicWeenieDesc {
    PublicWeenieDesc {
        header: header::RADAR_ENUM,
        name: "Observer".to_string(),
        wcid: 1,
        icon_id: 0x0600_1036,
        obj_type: 0,
        bitfield: PLAYER_BIT,
        radar_enum: Some(radar_enum::SHOW_ALWAYS),
        ..PublicWeenieDesc::default()
    }
}

fn observer_create(at: Position) -> ItemCreateObject {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PhysicsTimestamps};
    ItemCreateObject(ObjectCreatePayload {
        id: LOCAL,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            state: VISIBLE,
            setup_id: Some(dereth_client::character::ALUVIAN_MALE_SETUP.0),
            position: Some(PositionWire {
                objcell_id: at.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            timestamps: PhysicsTimestamps {
                instance: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc: observer_wdesc(),
    })
}

fn position_message(
    id: ObjectId,
    at: Position,
    stamp: u16,
    teleport: u16,
) -> MovementPositionEvent {
    let flags = position_flags::IS_GROUNDED
        | position_flags::ORIENTATION_HAS_NO_W
        | position_flags::ORIENTATION_HAS_NO_X
        | position_flags::ORIENTATION_HAS_NO_Y
        | position_flags::ORIENTATION_HAS_NO_Z;
    MovementPositionEvent {
        id,
        position: PositionPack {
            flags,
            origin: dereth_protocol::types::Origin {
                objcell_id: at.cell.0,
                origin: dereth_protocol::types::Vec3 {
                    x: at.frame.origin.x,
                    y: at.frame.origin.y,
                    z: at.frame.origin.z,
                },
            },
            instance_timestamp: 1,
            position_timestamp: stamp,
            teleport_timestamp: teleport,
            ..PositionPack::default()
        },
    }
}

// =================================================================================================
// Observers -- production consumers, not test mirrors
// =================================================================================================

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

fn drawn(app: &App, id: ObjectId) -> usize {
    app.world_scene()
        .unwrap()
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(id))
        .count()
}

/// The renderer's own server-object table: "the client still has a body for this id".
fn rendered(app: &App, id: ObjectId) -> bool {
    app.world_state().unwrap().server_object_frame(id).is_some()
}

fn doomed_at(app: &App, id: ObjectId) -> Option<f64> {
    app.objects().world.tables.doomed.get(id).map(|t| t.0)
}

fn held(app: &App, id: ObjectId) -> bool {
    app.objects().presence(id).is_some()
}

fn radar_dot(app: &App, id: ObjectId) -> bool {
    app.hud()
        .radar
        .iter()
        .any(|b| b.id == id && inq_showable_on_radar(b))
}

fn app_at(landblock: u16) -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("dere-corpse-decay-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        landblock,
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: true,
        release_interiors: true,
        ..Default::default()
    })
    .unwrap();
    assert!(app.frame());
    app
}

/// A grounded spot `dx`/`dy` metres from the local body, with the land cell index and the ground
/// height both recomputed from the coordinates.
///
/// Both halves are needed: keeping the body's `cell` while stepping 2 m puts the create in a cell
/// its coordinates are not in when the body stands near a 24 m boundary (index 36 against a
/// computed 37), and keeping the body's `z` is only right while the body is actually on the
/// ground.
fn near_player(app: &App, dx: f32, dy: f32) -> Position {
    let scene = app.world_scene().unwrap();
    let character = scene.character.as_ref().unwrap();
    let origin = character.world.get(character.handle).unwrap().position;
    on_terrain(
        app,
        origin.cell.landblock(),
        (origin.frame.origin.x + dx).clamp(1.0, 190.0),
        (origin.frame.origin.y + dy).clamp(1.0, 190.0),
    )
}

fn home_block(app: &App) -> LandblockId {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
        .cell
        .landblock()
}

fn on_terrain(app: &App, block: LandblockId, x: f32, y: f32) -> Position {
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    let z = land.ground_height(block, x, y).unwrap_or(0.0) + 0.5;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: bounded to 0..8 by the callers' coordinates, which are inside one block.
    let index = ((x / 24.0) as u16) * 8 + ((y / 24.0) as u16) + 1;
    Position::new(
        block.cell(index),
        Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
    )
}

/// **The login tunnel hides the whole 3D world**: `0xF746` installs the player with
/// `position_update_complete == 0`, which is `teleport_in_progress()`, and the scene draw takes
/// its `world_hidden` early return until the player's use-time update clears it. A test that
/// measures the draw pass has to wait it out.
fn wait_until_drawn(app: &mut App, ids: &[ObjectId], budget: usize) -> usize {
    for n in 0..budget {
        if ids.iter().all(|id| drawn(app, *id) > 0) {
            return n;
        }
        assert!(app.frame());
    }
    panic!("{ids:?} never reached the draw pass in {budget} frames");
}

/// The observer standing in the corpse's landblock, with the recorded corpse drawn in front of
/// him. Returns the corpse's position and the number of frames the login tunnel lasted.
fn observer_and_corpse(app: &mut App, peer: &mut Peer, recorded: &Recorded) -> (Position, usize) {
    let start = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    peer.send(app, &LoginCreatePlayer { player_id: LOCAL });
    peer.send(app, &observer_create(start));
    frames(app, 4);
    assert_eq!(
        app.objects().player(),
        Some(LOCAL),
        "the local player id is the server's"
    );

    let spot = near_player(app, 2.0, 2.0);
    peer.send(app, &corpse_create_at(recorded, spot));
    frames(app, 6);
    assert_eq!(
        app.objects()
            .world
            .physics(CORPSE)
            .expect("the create reached the game table")
            .cell,
        Some(spot.cell),
        "the premise for every arm below: the corpse **placed**. A failed placement queues the \
         create immediately for the 25 s lifecycle deadline, which is a different story from \
         the decay sequence. spot = {spot:?}"
    );
    let tunnel = wait_until_drawn(app, &[CORPSE], 2000);
    frames(app, 8);

    assert!(held(app, CORPSE), "the stream owns the recorded instance 0");
    assert!(
        app.objects().physics.handle(CORPSE).is_some(),
        "a real collision body exists"
    );
    assert!(rendered(app, CORPSE), "and a real rendered body");
    assert!(
        doomed_at(app, CORPSE).is_none(),
        "the corpse placed, so the failed-placement route did not queue it; a quiet server \
         changes nothing"
    );
    (spot, tunnel)
}

// =================================================================================================
// 1. The control: the observer is still in range, and the recorded pair still retires the corpse
// =================================================================================================

/// Behaviour: objects.corpse.a-decayed-corpse-is-culled-at-the-lifecycle-deadline
///
/// The in-range decay, run **behind a real `0xF746` login** so that the login tunnel, the local
/// body and the draw pass are all in the picture. If the observer is inside ACE's 3x3
/// block window when the decay fires, both messages arrive and nothing is left on screen.
#[test]
fn the_recorded_decay_pair_retires_a_corpse_the_observer_can_still_see() {
    let recorded = recorded();
    let mut app = app_at(DEFAULT_LANDBLOCK);
    let mut peer = Peer::attach(&mut app);
    let (_spot, tunnel) = observer_and_corpse(&mut app, &mut peer, &recorded);

    // The player-visible state a live corpse owns.
    app.objects_mut().world.opened_corpses.insert(CORPSE);
    app.objects_mut().world.set_selected_object(
        Some(CORPSE),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );

    peer.send_recorded(&mut app, &recorded.destroy);
    frames(&mut app, 2);
    assert_eq!(
        app.objects().stats.script_type_events,
        1,
        "the recorded Destroy reached 0xF755"
    );
    assert!(held(&app, CORPSE), "0xF755 is an effect, not a deletion");
    assert!(
        rendered(&app, CORPSE),
        "so the body is still there for the 1.0 s ACE waits"
    );

    peer.send_recorded(&mut app, &recorded.delete);
    frames(&mut app, 2);
    assert_eq!(
        app.objects().stats.removes,
        1,
        "the recorded instance-0 0xF747 was accepted"
    );
    assert!(!held(&app, CORPSE), "presence retired");
    assert!(
        app.objects().physics.handle(CORPSE).is_none(),
        "collision body retired"
    );
    assert!(!rendered(&app, CORPSE), "rendered body retired");
    assert_eq!(
        drawn(&app, CORPSE),
        0,
        "and nothing of it reaches the draw pass"
    );
    assert!(
        !app.objects().world.opened_corpses.contains(&CORPSE),
        "opened-corpse state retired"
    );
    assert_eq!(app.objects().world.selected, None, "selection retired");
    assert!(!radar_dot(&app, CORPSE), "and the radar blip is gone");

    eprintln!(
        "corpse decay, in-range control: login tunnel {tunnel} frame(s), corpse fully retired"
    );
    app.shutdown();
}

// =================================================================================================
// 2. The incident: the server stops talking, the block stays resident, and nothing culls it
// =================================================================================================

/// **A decayed corpse stays on screen for an observer the server stopped talking to, as in
/// retail.**
///
/// Once the observer has left the corpse's 3x3 landblock window for 25 s, ACE removes him from
/// the corpse's known players and the later delete is broadcast to a set he is not in. From the
/// client's point of view the entire decay is **silence**.
///
/// The client's answer to silence is nothing at all, and that is the retail rule: the only
/// destruction-queue route a stationary, placed, still-resident object can reach prepares it to
/// leave visibility after its **cell** is released.
///
/// The clock here is pushed past both deadlines that could plausibly matter (the client's 25 s
/// destruction time and ACE's 5-minute time to rot) through a real `TimeSync` header, so the
/// lifecycle deadline sweep really does run with a clock that is minutes later.
#[test]
fn an_observer_the_server_stopped_talking_to_keeps_the_decayed_corpse() {
    let recorded = recorded();
    let mut app = app_at(DEFAULT_LANDBLOCK);
    let mut peer = Peer::attach(&mut app);
    let (spot, _) = observer_and_corpse(&mut app, &mut peer, &recorded);
    let block = home_block(&app);
    assert_eq!(
        spot.cell.landblock(),
        block,
        "the corpse is in the observer's own landblock"
    );

    let start = app.clock().cur_time;
    for step in 1..=8 {
        let t = start + (DEFAULT_TIME_TO_ROT + DESTRUCTION_TIME + 30.0) * f64::from(step) / 8.0;
        peer.silence_until(&mut app, t);
    }
    let elapsed = app.clock().cur_time - start;
    assert!(
        elapsed > DEFAULT_TIME_TO_ROT + DESTRUCTION_TIME,
        "the TimeSync headers really advanced the application clock: {elapsed:.1} s"
    );
    frames(&mut app, 8);

    assert!(
        doomed_at(&app, CORPSE).is_none(),
        "silence never queues a placed body -- failed placement and preparation after cell \
         release are the insertion routes exercised here, and neither occurred"
    );
    assert!(
        held(&app, CORPSE),
        "so the corpse is still tracked {elapsed:.0} s later"
    );
    assert!(
        rendered(&app, CORPSE),
        "and the renderer still has a body for it"
    );
    assert!(
        drawn(&app, CORPSE) > 0,
        "and it is still on screen. **This ghost is what the retail client does too**: ACE \
         dropped the observer from the corpse's KnownPlayers at PVS + 25 s (leaving the cull to \
         the client), the corpse's landblock is still inside the client's landscape window, \
         and the client has no visibility-range cull to reach. If this assertion ever fails, \
         the client has grown a local TTL retail does not have and will start deleting objects \
         the server never deleted."
    );
    assert_eq!(
        app.objects().stats.removes,
        0,
        "nothing was removed, because nothing was sent"
    );

    eprintln!(
        "corpse decay, silent-server arm: {elapsed:.0} s of silence, corpse still drawn ({} parts), \
         doomed = {:?}",
        drawn(&app, CORPSE),
        doomed_at(&app, CORPSE)
    );
    app.shutdown();
}

// =================================================================================================
// 3. and 4. Leaving the client's own window: the cull, and the rescue that defeats it
// =================================================================================================

fn teleport_local(app: &mut App, peer: &mut Peer, to: Position, stamp: &mut u16) {
    *stamp = stamp.wrapping_add(1);
    let applied = app.player_teleports_applied();
    peer.send(app, &position_message(LOCAL, to, *stamp, *stamp));
    frames(app, 14);
    assert_eq!(
        app.player_teleports_applied(),
        applied + 1,
        "the local 0xF748 to {:#010X} was admitted",
        to.cell.0
    );
}

/// Behaviour: objects.corpse.a-decayed-corpse-is-culled-at-the-lifecycle-deadline
///
/// **The one ordering with no ghost.** If the observer leaves the corpse's landblock window and
/// stays away past the client's own 25 s, the lifecycle deadline sweep culls the corpse
/// exactly as ACE assumes it will, and the return is clean: the server has destroyed the corpse
/// and sends nothing, and the client has nothing left to draw.
///
/// It also pins the player-visible state that has to come off with it: an opened corpse's marker
/// and the selection, which the recorded `0xF747` clears in test 1 and which the **local** cull
/// has to clear through the same `ObjectDeleted` notice.
#[test]
fn leaving_the_window_past_the_deadline_culls_the_corpse_and_the_return_is_clean() {
    let recorded = recorded();
    let mut app = app_at(DEFAULT_LANDBLOCK);
    let mut peer = Peer::attach(&mut app);
    let (_spot, _) = observer_and_corpse(&mut app, &mut peer, &recorded);

    app.objects_mut().world.opened_corpses.insert(CORPSE);
    app.objects_mut().world.set_selected_object(
        Some(CORPSE),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );

    let block = home_block(&app);
    let far = LandblockId((u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()));
    let away = on_terrain(&app, far, 96.0, 96.0);
    let home = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    let mut stamp: u16 = 1;

    teleport_local(&mut app, &mut peer, away, &mut stamp);
    let deadline = doomed_at(&app, CORPSE).expect(
        "the corpse's landblock was released, so object-cell release queued it for lifecycle \
         destruction",
    );
    let now = app.clock().cur_time;
    assert!(
        (deadline - now - DESTRUCTION_TIME).abs() < 2.0,
        "the deadline is current time + 25.0: {deadline:.2} vs {now:.2}"
    );

    peer.silence_until(&mut app, now + DESTRUCTION_TIME + 5.0);
    frames(&mut app, 4);
    assert!(
        !held(&app, CORPSE),
        "the lifecycle deadline sweep culled it at its deadline"
    );
    assert!(
        !rendered(&app, CORPSE),
        "and the renderer body went with it"
    );
    assert!(
        !app.objects().world.opened_corpses.contains(&CORPSE),
        "the local cull clears the opened-corpse marker through the same ObjectDeleted notice \
         the recorded 0xF747 raises"
    );
    assert_eq!(app.objects().world.selected, None, "and the selection");

    // The return. ACE destroyed the corpse while he was away and it is not on the landblock any
    // more, so its visibility update finds nothing to track and **no message is sent at all**.
    teleport_local(&mut app, &mut peer, home, &mut stamp);
    frames(&mut app, 40);
    assert!(
        !held(&app, CORPSE),
        "nothing brought it back, because nothing was sent"
    );
    assert!(!rendered(&app, CORPSE), "and the screen is clean");
    assert_eq!(drawn(&app, CORPSE), 0);

    eprintln!(
        "corpse decay, out-of-window cull: deadline {deadline:.1}, corpse culled and return clean"
    );
    app.shutdown();
}

/// Behaviour: objects.corpse.a-decayed-corpse-is-culled-at-the-lifecycle-deadline
///
/// **The rescue that turns the cull back into a ghost.** A return *inside* the 25 s cancels the
/// deadline. Reinitializing the cell's objects re-enters visibility and removes the pending
/// destruction deadline. The other re-entry site is the live viewer update.
///
/// Retail does exactly this, so a player who ducks out and back inside 25 s keeps a corpse ACE
/// may already have destroyed. Asserting that the corpse **survives** is therefore the correct
/// oracle: a client that culled it here would be deleting an object on a timer of its own.
#[test]
fn a_return_inside_the_deadline_rescues_a_corpse_the_server_may_already_have_destroyed() {
    let recorded = recorded();
    let mut app = app_at(DEFAULT_LANDBLOCK);
    let mut peer = Peer::attach(&mut app);
    let (_spot, _) = observer_and_corpse(&mut app, &mut peer, &recorded);
    let handle = app.objects().physics.handle(CORPSE).expect("a placed body");

    let block = home_block(&app);
    let far = LandblockId((u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()));
    let away = on_terrain(&app, far, 96.0, 96.0);
    let home = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    let mut stamp: u16 = 1;

    teleport_local(&mut app, &mut peer, away, &mut stamp);
    assert!(doomed_at(&app, CORPSE).is_some(), "queued on the way out");
    teleport_local(&mut app, &mut peer, home, &mut stamp);
    frames(&mut app, 40);

    assert!(
        doomed_at(&app, CORPSE).is_none(),
        "cell re-entry cancelled the pending destruction deadline"
    );
    assert!(held(&app, CORPSE), "and the corpse is still tracked");
    assert_eq!(
        app.objects().physics.handle(CORPSE),
        Some(handle),
        "on the same physical body, not a rebuild"
    );

    // And now the clock runs past the deadline it would have had, with the server silent.
    let now = app.clock().cur_time;
    peer.silence_until(&mut app, now + DESTRUCTION_TIME + 10.0);
    frames(&mut app, 8);
    assert!(
        rendered(&app, CORPSE),
        "the rescued corpse remains after its former deadline; re-entry removed the pending \
         deadline"
    );

    eprintln!("corpse decay, rescue arm: deadline cancelled on re-entry, corpse retained");
    app.shutdown();
}

// =================================================================================================
// 5. The band itself: ACE's 3x3 window against the client's landscape window
// =================================================================================================

fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above. Not a float conversion.
    LandblockId(((x as u16) << 8) | (y as u16))
}

fn walk_to(
    scene: &mut WorldScene,
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    stream: &mut ObjectStream,
    block: LandblockId,
    now: f64,
) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let mid = 96.0f32;
    let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            block.cell(1),
            Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
        ));
    }
    scene.follow_character_now();
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(now),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("the streamed blocks build");
    scene
        .sync_objects(store, gpu, stream)
        .expect("the objects synchronise");
}

/// **The measurement that makes the incident a parity statement rather than a guess.**
///
/// ACE stops sending a player anything about an object at Chebyshev landblock distance **2**
/// (outdoor visibility is distance `<= 1`). The client cannot forget that object until its
/// landblock leaves the landscape window, i.e. distance **landscape radius + 1**: 4 at the
/// cheapest shipped preset and 9 at `Render.LandscapeDrawDistance`'s registered default of 8.
///
/// This scrolls the window through its four edge releases, rather than the whole-array release
/// used by a teleport, to both sides of that boundary and reads the production verdict.
/// `SceneConfig::default()`'s `land_radius` is 3, the smallest shipped preset, so the band measured
/// here -- distance 2 and 3 -- is the **narrowest** a retail player can configure.
#[test]
fn the_ghost_band_is_the_gap_between_aces_pvs_and_the_clients_landblock_window() {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc};

    let store = retail_store();
    let mut gpu = test_gpu(800, 600);

    let recorded = recorded();
    let home = LandblockId(DEFAULT_LANDBLOCK);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);

    // The recorded corpse, standing on the terrain of `home`, fed through the same
    // `ObjectStream::apply_event` seam the network takes.
    let cell: CellId = home.cell(0x0001);
    let payload = ObjectCreatePayload {
        id: CORPSE,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: recorded.create.physicsdesc.setup_id,
            position: Some(PositionWire {
                objcell_id: cell.0,
                frame: dereth_protocol::types::Frame::default(),
            }),
            ..PhysicsDesc::default()
        },
        // The recorded weenie, so this is the recorded corpse and not a generic body: the `CORPSE`
        // bit and the name come off the wire.
        wdesc: recorded.create.wdesc.clone(),
    };
    let body = dereth_protocol::write_body(&ItemCreateObject(payload)).expect("encode");

    let mut stream = ObjectStream::new();
    stream.apply_event(
        &dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(0.0),
    );

    let cfg = SceneConfig {
        release_interiors: true,
        ..SceneConfig::default()
    };
    let radius = i32::try_from(cfg.land_radius).unwrap();
    assert_eq!(
        radius, SMALLEST_SHIPPED_MID_RADIUS,
        "this arm is calibrated against the cheapest shipped landscape-radius preset"
    );
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    walk_to(
        &mut scene,
        &store,
        &mut gpu,
        &mut stream,
        block_at(hx, hy),
        0.0,
    );
    assert_eq!(
        stream
            .world
            .physics(CORPSE)
            .expect("the create reached the game table")
            .cell,
        Some(cell),
        "the premise: the corpse is standing in an outdoor cell of its own block"
    );
    assert!(
        scene.server_object_frame(CORPSE).is_some(),
        "and the scene is drawing it"
    );

    // --- inside the band: ACE has stopped talking, the client has not released anything -------
    let mut evidence = Vec::new();
    for distance in (ACE_PVS_BLOCK_RADIUS + 1)..=radius {
        walk_to(
            &mut scene,
            &store,
            &mut gpu,
            &mut stream,
            block_at(hx + distance, hy),
            1.0,
        );
        let cell_now = stream.world.physics(CORPSE).unwrap().cell;
        let queued = stream.world.tables.doomed.contains_key(CORPSE);
        assert_eq!(
            cell_now,
            Some(cell),
            "at block distance {distance} the corpse's block is still inside the window \
         (landscape radius {radius}), so landscape block refresh released nothing and the \
         corpse keeps its cell -- while ACE stopped sending anything about it at distance 2"
        );
        assert!(
            !queued,
            "and nothing queued it for destruction at distance {distance}"
        );
        assert!(
            scene.server_object_frame(CORPSE).is_some(),
            "and it is still drawn at distance {distance}: the ghost band"
        );
        evidence.push(format!(
            "distance {distance}: cell kept, doomed false, drawn true"
        ));
        walk_to(
            &mut scene,
            &store,
            &mut gpu,
            &mut stream,
            block_at(hx, hy),
            1.0,
        );
    }

    // --- past it: the window scroll releases the block and the client culls on its own --------
    let out = radius + 1;
    walk_to(
        &mut scene,
        &store,
        &mut gpu,
        &mut stream,
        block_at(hx + out, hy),
        1.0,
    );
    assert_eq!(
        stream.world.physics(CORPSE).unwrap().cell,
        None,
        "at block distance {out} the block left the window: whole-landblock release reached \
         object-cell release and queued the corpse for destruction"
    );
    let deadline = stream
        .world
        .tables
        .doomed
        .get(CORPSE)
        .expect("queued by lifecycle insertion")
        .0;
    assert!(
        (deadline - 1.0 - DESTRUCTION_TIME).abs() < 0.01,
        "now + 25.0, from a clock this test set to 1.0: {deadline}"
    );
    let dead = 1.0 + DESTRUCTION_TIME + 5.0;
    stream.use_time::<dereth_client_net::client_session::testing::MockTransport>(
        ServerTime(dead),
        None,
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the removal drains");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(dead),
        1.0 / 30.0,
    );
    assert!(
        scene.server_object_frame(CORPSE).is_none(),
        "and 25 s later it is off the screen: the cull ACE relies on"
    );
    evidence.push(format!(
        "distance {out}: cell released, doomed at {deadline:.1}, culled"
    ));

    eprintln!("corpse decay, ghost band (ACE PVS radius 1, client landscape radius {radius}):");
    for line in &evidence {
        eprintln!("  {line}");
    }
}
