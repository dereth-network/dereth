//! Repeated leave/return cycles: a remote player, his held wand and nearby NPCs stay one drawn
//! body each across many departures and returns.
//!
//! Three lifecycle questions, each run over several cycles (a held child is culled with its
//! teleported wielder, see `objects::remote_leave_return`):
//!
//! * ACE's asymmetric visibility expiry: updates stop while a player stands in view, and
//!   creates resume on the next visibility entry;
//! * whether an interior destination resolves locally, allowing placement or queuing destruction;
//! * fresh dynamic GUIDs after a landblock reload as a possible source of same-name NPC duplicates.
//!
//! The first test runs ten timing/stamp combinations, the second runs each of three interior
//! arms twice, and the third runs four sequential NPC variants. Repetition can expose retained
//! deadlines, stale placement state or lost child links. This is not a full combination matrix.
//!
//! # ACE server rules behind the input sequences
//!
//! These motivate the inputs; no server code runs in this socket-free client fixture.
//!
//! * An **occluded** object is added to the destruction queue and is *not* removed from the
//!   visible set.
//! * A **re-visible** object is only un-queued when it becomes *newly* visible, which is false
//!   for an id still in the visible set.
//! * So the 25 s expiry fires on a player standing in view, and the removal sends **no**
//!   `0xF747`; the survivor also stops being one of his known players.
//! * The re-track on his next visibility entry is a create for the player, then one per wielded
//!   child-location item; the player's create carries the children list.
//! * A landblock unloads after 5 idle minutes and recreates its creatures with **new dynamic
//!   guids** and a fresh face. Nothing is sent to a player already expired from their lists.
//!
//! # Client behavior
//!
//! * Position application first prepares a cell-less body to enter the world, then adjusts its
//!   position. If no cell is found, the object and its children leave visibility, the position
//!   is stored, the object enters lost-cell tracking and its active timestamp is cleared.
//! * An equal-instance create merges visual description, received position, state and vectors.
//!   Its final placement result cancels destruction when a cell exists or queues destruction
//!   when none exists. A newer instance deletes and rebuilds the object.
//! * Advancing time past a queued deadline deletes the object.
//! * Radar insertion rejects rows that fail the radar-eligibility predicate. Here `radar_dot`
//!   inspects that predicate on the HUD row; it does not read rendered radar pixels.
//!
//! Fixture: synthetic messages encoded and admitted through the encrypted replay endpoint of a
//! headless `App`, in ordinary `App::frame` order. A real `TimeSync` optional header advances the
//! server clock across the 25 s deadline; the tests do not directly invoke the model's
//! destruction routine.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::SceneReads;
use dereth_client::{app::App, config::Config, world::SceneConfig};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::{
    movement::{position_flags, MovementPositionEvent, PositionPack},
    objects::{ItemCreateObject, ItemSetState, LoginCreatePlayer, ObjectCreatePayload},
    types::{
        physicsdesc::{flags, ChildLink},
        weeniedesc::header,
        ObjDesc, PhysicsDesc, PhysicsEventStamp, PhysicsTimestamps, PositionWire, PublicWeenieDesc,
    },
    Message,
};
use dereth_ui_screens::mapradar::radar::{inq_showable_on_radar, radar_enum};

const REMOTE: ObjectId = ObjectId(0x5000_1C95);
const WAND: ObjectId = ObjectId(0x5000_1C96);
const LOCAL: ObjectId = ObjectId(0x5000_1C01);

const PLAYER_SETUP: u32 = 0x0200_0001;
const PLAYER_MTABLE: u32 = 0x0900_0001;
const PLAYER_PSCRIPT_TABLE: u32 = 0x3400_0004;
const WAND_SETUP: u32 = 0x0200_1713;
const RIGHT_HAND: u32 = 1;

/// The corpus's player teleport words: `HIDDEN_PS | IGNORE_COLLISIONS_PS` on, and the visible
/// `0x00400408` ACE answers the login-complete action with.
const HIDDEN: u32 = 0x0040_4410;
const VISIBLE: u32 = 0x0040_0408;
/// `ObjectDescriptionFlag::Player`.
const PLAYER_BIT: u32 = 0x0000_0008;
/// Destruction is queued for current server time plus 25.0 seconds.
const DESTRUCTION_TIME: f64 = 25.0;
/// The visible-state transition's 0.75 s ramp plus slack, in headless frames.
const RAMP_FRAMES: usize = 40;

// =================================================================================================
// Socket-free peer
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
            "leave-return-churn",
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

    /// One datagram carrying every blob in order, optionally with a `TimeSync` header.
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

    /// Advance server time with a state-repeat blob whose STATE_TS is refused. The repeated
    /// state is not applied, but normal frame processing can still advance lifecycle work.
    fn advance_to(
        &mut self,
        app: &mut App,
        id: ObjectId,
        state: u32,
        stamp: u16,
        instance: u16,
        t: f64,
    ) {
        let repeat = dereth_protocol::write_blob(&set_state(id, state, stamp, instance)).unwrap();
        self.send_batch(app, &[repeat], Some(t));
    }
}

// =================================================================================================
// Wire shapes
// =================================================================================================

/// A descriptor eligible for radar-row admission: the accepted radar modes include
/// ShowMovement, ShowAttacking and ShowAlways. This fixture chooses ShowAlways;
/// ObjectDescriptionFlag::Player supplies the player classification used for blip color.
fn seen_wdesc(name: &str, player: bool) -> PublicWeenieDesc {
    PublicWeenieDesc {
        header: header::RADAR_ENUM,
        name: name.to_string(),
        wcid: 1,
        icon_id: 0x0600_1036,
        obj_type: if player { 0 } else { 0x10 },
        bitfield: if player { PLAYER_BIT } else { 0 },
        radar_enum: Some(radar_enum::SHOW_ALWAYS),
        ..PublicWeenieDesc::default()
    }
}

/// A creature's `0xF745`. `children` is ACE's physics-description children arm: every create of
/// a creature wielding a child-location item carries the held-item list, so a returning player's
/// create names his wand.
#[allow(clippy::too_many_arguments)]
fn create(
    id: ObjectId,
    wdesc: PublicWeenieDesc,
    at: Position,
    instance: u16,
    position_stamp: u16,
    state_stamp: u16,
    state: u32,
    children: Option<ObjectId>,
) -> ItemCreateObject {
    let mut bitfield = flags::POSITION | flags::SETUP | flags::MTABLE | flags::PETABLE;
    if children.is_some() {
        bitfield |= flags::CHILDREN;
    }
    ItemCreateObject(ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield,
            state,
            setup_id: Some(PLAYER_SETUP),
            mtable_id: Some(PLAYER_MTABLE),
            phstable_id: Some(PLAYER_PSCRIPT_TABLE),
            position: Some(PositionWire {
                objcell_id: at.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            children: children.map(|c| {
                vec![ChildLink {
                    child_id: c,
                    location_id: RIGHT_HAND,
                }]
            }),
            timestamps: PhysicsTimestamps {
                instance,
                position: position_stamp,
                state: state_stamp,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc,
    })
}

/// The wand's 0xF745 uses its own sequences, matching ACE's tracking of an equipped item.
/// Within an unchanged instance it is byte-identical across re-tracks and POSITION_TS stays
/// zero. The newer-instance cases below deliberately advance the supplied instance too.
fn wand(parent: ObjectId, instance: u16) -> ItemCreateObject {
    ItemCreateObject(ObjectCreatePayload {
        id: WAND,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::PARENT | flags::ANIMFRAME,
            setup_id: Some(WAND_SETUP),
            parent: Some((parent, RIGHT_HAND)),
            animframe_id: Some(1),
            timestamps: PhysicsTimestamps {
                instance,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc: seen_wdesc("Sceptre", false),
    })
}

/// A remote player's 0xF748 advances POSITION_TS while leaving TELEPORT_TS unchanged,
/// following ACE's non-admin teleport position update.
fn remote_position(id: ObjectId, at: Position, stamp: u16) -> MovementPositionEvent {
    position_message(id, at, 1, stamp, 0)
}

fn position_message(
    id: ObjectId,
    at: Position,
    instance: u16,
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
            instance_timestamp: instance,
            position_timestamp: stamp,
            teleport_timestamp: teleport,
            ..PositionPack::default()
        },
    }
}

/// 0xF74B state application requires an event strictly newer than the stored STATE_TS.
/// Repeating the stamp seeded by the create is refused, so that word does not reach the body.
fn set_state(id: ObjectId, state: u32, event: u16, instance: u16) -> ItemSetState {
    ItemSetState {
        id,
        state,
        timestamps: PhysicsEventStamp { instance, event },
    }
}

// =================================================================================================
// Observers -- every one of them reads a production consumer, not a test mirror
// =================================================================================================

fn drawn(app: &App, id: ObjectId) -> usize {
    app.world_scene()
        .unwrap()
        .drawn_part_order()
        .iter()
        .filter(|part| part.object == Some(id))
        .count()
}

fn doomed(app: &App, id: ObjectId) -> Option<f64> {
    app.objects().world.tables.doomed.get(id).map(|t| t.0)
}

fn body_cell(app: &App, id: ObjectId) -> Option<Option<CellId>> {
    let handle = app.objects().physics.handle(id)?;
    let scene = app.world_scene().unwrap();
    scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .map(|body| body.cell)
}

fn pose(app: &App, id: ObjectId) -> Option<Vec3> {
    let handle = app.objects().physics.handle(id)?;
    let scene = app.world_scene().unwrap();
    scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .map(|b| b.position.frame.origin)
}

/// Apply the production radar-row filter to the row Hud::sync built this frame, not pixels.
fn radar_dot(app: &App, id: ObjectId) -> bool {
    app.hud()
        .radar
        .iter()
        .any(|b| b.id == id && inq_showable_on_radar(b))
}

/// Count distinct handles returned by ObjectPhysics's index and the physics world's object-ID
/// index, requiring the former to resolve to a live body. ObjectPhysics::sync prevents index
/// collisions. This observer does not enumerate the arena or exclude unindexed orphan bodies.
fn bodies(app: &App, id: ObjectId) -> usize {
    let scene = app.world_scene().unwrap();
    let world = &scene.character.as_ref().unwrap().world;
    let tracked = app
        .objects()
        .physics
        .handle(id)
        .filter(|h| world.get(*h).is_some());
    let arena = world.by_object_id(id);
    usize::from(tracked.is_some()) + usize::from(arena.is_some() && arena != tracked)
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

fn apart(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// An unchanged TELEPORT_TS interpolates a remote body toward the received position rather
/// than snapping it. Sample every eight frames until convergence, up to fourteen samples;
/// return the trajectory for diagnostics. This does not require monotonic progress.
fn settle_near(app: &mut App, id: ObjectId, target: Vec3, tolerance: f32) -> (bool, Vec<Vec3>) {
    let mut trajectory = vec![pose(app, id).unwrap_or(Vec3::ZERO)];
    for _ in 0..14 {
        frames(app, 8);
        let now = pose(app, id).unwrap_or(Vec3::ZERO);
        trajectory.push(now);
        if apart(now, target) < tolerance {
            return (true, trajectory);
        }
    }
    (false, trajectory)
}

// =================================================================================================
// The App
// =================================================================================================

fn app() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        // Hud::sync runs inside App::ui_use_time's shell block. The radar-row eligibility
        // observation therefore needs the real shell even though it does not inspect pixels.
        ui: true,
        preferences_file: std::env::temp_dir()
            .join("dere-leave-return-churn-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
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

/// A spot in front of the actual camera, `right` metres to the side.
fn in_view(app: &App, right: f32) -> Position {
    let scene = app.world_scene().unwrap();
    let character = scene.character.as_ref().unwrap();
    let mut position = character.world.get(character.handle).unwrap().position;
    position.frame.origin =
        dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(right, 10.0, 0.0));
    position
}

/// Offset the placed body's own pose by dx along world X, retaining its current height and
/// cell. This avoids using a later camera sample as the reference position.
fn beside(app: &App, id: ObjectId, dx: f32) -> Position {
    let handle = app.objects().physics.handle(id).expect("placed body");
    let scene = app.world_scene().unwrap();
    let mut position = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .position;
    position.frame.origin.x += dx;
    position
}

/// Offset the local body's pose by dx/dy and use terrain height plus 0.5 where available.
/// This supplies a grounded placement candidate, rather than assuming camera height is ground.
fn near_player(app: &App, dx: f32, dy: f32) -> Position {
    let scene = app.world_scene().unwrap();
    let character = scene.character.as_ref().unwrap();
    let mut position = character.world.get(character.handle).unwrap().position;
    position.frame.origin.x += dx;
    position.frame.origin.y += dy;
    let land = character.land().clone();
    let block = position.cell.landblock();
    if let Some(z) = land.ground_height(block, position.frame.origin.x, position.frame.origin.y) {
        position.frame.origin.z = z + 0.5;
    }
    position
}

/// Wait out the login tunnel before relying on the world draw trace.
///
/// Renderer::draw_scene returns early while world_hidden is set. LoginCreatePlayer (0xF746)
/// reaches Teleport::events as SessionEvent::PlayerCreated. With a player present but position
/// completion still pending, teleport animation hides the world. The normal frame path clears
/// that state after the body reaches a loaded scene and cell loading no longer blocks it.
///
/// A create sent before 0xF746 draws (23 parts), but a byte-identical create immediately after it
/// draws none until the tunnel comes down. While the tunnel hides the world, drawn_part_order
/// retains the last completed draw rather than reporting a fresh empty draw. The local body is
/// hidden too; this fixture boundary is not an object-only behaviour.
///
/// Require a positive draw of an ID created after the tunnel began. Checking only whether the
/// retained trace is empty could pass against stale pre-tunnel output.
fn wait_until_drawn(app: &mut App, ids: &[ObjectId], budget: usize) -> usize {
    for n in 0..budget {
        if ids.iter().all(|id| drawn(app, *id) > 0) {
            return n;
        }
        assert!(app.frame());
    }
    panic!(
        "{ids:?} never reached the draw pass in {budget} frames -- if the world is still hidden \
         the login tunnel never came down"
    );
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

/// A point on the terrain of `block`, with the land cell index the coordinates fall in.
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

/// Player and wand on screen, through the actual draw pass.
fn seen(app: &mut App, peer: &mut Peer) -> Position {
    let home = in_view(app, 0.0);
    peer.send(
        app,
        &create(
            REMOTE,
            seen_wdesc("Returning Player", true),
            home,
            1,
            0,
            0,
            VISIBLE,
            Some(WAND),
        ),
    );
    peer.send(app, &wand(REMOTE, 1));
    frames(app, 31);
    assert!(
        drawn(app, REMOTE) > 0,
        "the remote body starts through the actual draw pass"
    );
    assert!(
        drawn(app, WAND) > 0,
        "its wand starts through the same pass"
    );
    assert!(radar_dot(app, REMOTE), "and it starts with a radar blip");
    home
}

// =================================================================================================
// 1. ACE's asymmetric visibility expiry, ten cycles
// =================================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stamp {
    /// What ACE actually sends: `ObjectPosition` only ever advances.
    Newer,
    /// A wielded item's re-track is byte-identical, and a player who has not moved since the
    /// last broadcast re-tracks with the sequence he already had.
    Equal,
    /// A stale create loses to an already applied 0xF748. The equal-instance merge's strict
    /// position-sequence gate refuses that position, so the body should retain its prior pose.
    Older,
}

#[derive(Clone, Copy)]
struct Cycle {
    /// Silence before re-track. ACE's 25 s expiry motivates the 26/60 s cases. The 5/24 s creates
    /// are additional client robustness inputs, not a claim that this server expiry sequence would
    /// resend before its deadline.
    silence: f64,
    stamp: Stamp,
    /// ACE does not bump a player's instance on re-track; landblock reload instead uses fresh
    /// GUIDs. A newer instance on the same ID separately exercises the delete/rebuild branch.
    /// Handles are reported below, but their change is not itself an asserted predicate.
    new_instance: bool,
}

/// Behaviour: objects.lifecycle.repeated-leave-return-keeps-one-drawn-body-per-object
///
/// Server visibility-expiry question. ACE queues an occluded object for destruction **without**
/// removing it from the visible set, and adding an id already in that set does not count as
/// newly visible, so the un-queue never runs for a player who came back through his own cell
/// entry. He is expired at 25 s **while standing in A's view**, silently (no `0xF747`), and his
/// next visibility entry re-sends him: a fresh create with the children list, one create per
/// wielded item, and the login-complete `0xF74B`.
///
/// Ten cycles can expose state retained across earlier cycles: deadlines, placement state,
/// indexed handles or child links. Each cycle checks index agreement, resident placement,
/// convergence, visible state, absence of a deadline, drawn part submissions, radar-row
/// eligibility and a parented/drawn wand. These are connected production observations, not
/// a claim to inspect every visible pixel or enumerate every arena allocation.
#[test]
fn ten_asymmetric_expiry_cycles_keep_one_drawn_body_and_its_wand() {
    let mut app = app();
    let mut peer = Peer::attach(&mut app);
    seen(&mut app, &mut peer);
    let block = home_block(&app);

    let cycles = [
        Cycle {
            silence: 5.0,
            stamp: Stamp::Newer,
            new_instance: false,
        },
        Cycle {
            silence: 24.0,
            stamp: Stamp::Newer,
            new_instance: false,
        },
        Cycle {
            silence: 26.0,
            stamp: Stamp::Newer,
            new_instance: false,
        },
        Cycle {
            silence: 60.0,
            stamp: Stamp::Newer,
            new_instance: false,
        },
        Cycle {
            silence: 5.0,
            stamp: Stamp::Equal,
            new_instance: false,
        },
        Cycle {
            silence: 24.0,
            stamp: Stamp::Equal,
            new_instance: false,
        },
        Cycle {
            silence: 26.0,
            stamp: Stamp::Older,
            new_instance: false,
        },
        Cycle {
            silence: 60.0,
            stamp: Stamp::Older,
            new_instance: false,
        },
        Cycle {
            silence: 26.0,
            stamp: Stamp::Newer,
            new_instance: true,
        },
        Cycle {
            silence: 60.0,
            stamp: Stamp::Newer,
            new_instance: true,
        },
    ];

    let mut stamp: u16 = 1;
    let mut instance: u16 = 1;
    let mut evidence: Vec<String> = Vec::new();

    for (n, c) in cycles.iter().enumerate() {
        // Silence input: only a refused state repeat accompanies the clock header.
        let server_now = app.clock().cur_time + c.silence;
        peer.advance_to(&mut app, REMOTE, VISIBLE, 0, instance, server_now);
        frames(&mut app, 3);
        assert!(
            drawn(&app, REMOTE) > 0,
            "cycle {n}: the retained body is still drawn where he stood after {} s of silence",
            c.silence
        );
        assert!(
            doomed(&app, REMOTE).is_none(),
            "cycle {n}: a placed body is on no destruction queue -- the 25 s deadline is the \
             client's answer to a failed placement, not to silence"
        );

        // --- the re-track, in ACE's order ----------------------------------------------------
        let before = pose(&app, REMOTE).expect("cycle precondition: a placed body");
        let handle_before = app.objects().physics.handle(REMOTE);
        let held_ts = app
            .objects()
            .presence(REMOTE)
            .expect("a known remote")
            .position_ts;
        let unplaced_before = app.objects().physics.stats.unplaced;
        // Two stamps per cycle: the create seeds STATE_TS and the login-complete 0xF74B
        // must be strictly newer to apply.
        stamp = stamp.wrapping_add(2);
        let position_stamp = match c.stamp {
            Stamp::Newer => stamp,
            Stamp::Equal => held_ts,
            Stamp::Older => held_ts.wrapping_sub(1),
        };
        if c.new_instance {
            instance = instance.wrapping_add(1);
        }
        let back = beside(&app, REMOTE, if n % 2 == 0 { 3.0 } else { -3.0 });
        peer.send(
            &mut app,
            &create(
                REMOTE,
                seen_wdesc("Returning Player", true),
                back,
                instance,
                position_stamp,
                stamp,
                HIDDEN,
                Some(WAND),
            ),
        );
        peer.send(&mut app, &wand(REMOTE, instance));
        frames(&mut app, 2);
        assert_eq!(
            drawn(&app, REMOTE),
            0,
            "cycle {n}: the create's hidden-state path suppressed the returned body's part submissions"
        );
        assert_eq!(
            app.objects().physics.stats.unplaced - unplaced_before,
            0,
            "cycle {n}: the grounded return position was accepted by placement"
        );

        // The login-complete `0xF74B` closes the re-track.
        peer.send(
            &mut app,
            &set_state(REMOTE, VISIBLE, stamp.wrapping_add(1), instance),
        );

        // --- production state, draw trace and radar-row observations -------------------------
        let target = match c.stamp {
            Stamp::Newer => back.frame.origin,
            // Equal or older position stamps are refused; retain the body's prior position.
            Stamp::Equal | Stamp::Older => before,
        };
        let (converged, trajectory) = settle_near(&mut app, REMOTE, target, 2.5);
        frames(&mut app, RAMP_FRAMES);

        let handle_after = app.objects().physics.handle(REMOTE);
        assert_eq!(
            bodies(&app, REMOTE),
            1,
            "cycle {n}: exactly one physics body answers to the id"
        );
        let cell = body_cell(&app, REMOTE)
            .expect("the returned player has a body")
            .expect("and it is in a cell");
        assert_eq!(
            cell.landblock(),
            block,
            "cycle {n}: and it stands in the resident block"
        );
        assert!(
            converged,
            "cycle {n} ({:?}, {} s): the body is not where the sequence put it: target \
             {target:?}, pose every 8 frames {trajectory:?}",
            c.stamp, c.silence
        );
        assert_eq!(
            app.objects().physics_state(REMOTE),
            Some(VISIBLE),
            "cycle {n}: the login-complete 0xF74B un-hid him"
        );
        assert!(
            doomed(&app, REMOTE).is_none(),
            "cycle {n}: and left him on no destruction queue"
        );
        assert!(
            drawn(&app, REMOTE) > 0,
            "cycle {n}: the visible-state update returned his parts to the draw pass"
        );
        assert!(
            radar_dot(&app, REMOTE),
            "cycle {n}: and the radar-row filter still admits him"
        );
        assert_eq!(
            app.objects().presence(WAND).and_then(|p| p.parent),
            Some((REMOTE, RIGHT_HAND)),
            "cycle {n}: the wand is attached to the returned player"
        );
        assert!(
            drawn(&app, WAND) > 0,
            "cycle {n}: and the wand draws attached to him"
        );
        assert_eq!(
            bodies(&app, WAND),
            0,
            "cycle {n}: a held child owns no independent body"
        );

        evidence.push(format!(
            "cycle {n:>2}: silence {:>4.0} s  {:<5}  instance {instance}  handle {}  drift {:.2} m  \
             parts {}  wand {}  radar {}",
            c.silence,
            format!("{:?}", c.stamp),
            if handle_before == handle_after { "same" } else { "new " },
            apart(pose(&app, REMOTE).unwrap(), target),
            drawn(&app, REMOTE),
            drawn(&app, WAND),
            radar_dot(&app, REMOTE),
        ));
    }

    eprintln!(
        "leave/return churn -- asymmetric expiry, {} cycles:",
        cycles.len()
    );
    for line in &evidence {
        eprintln!("  {line}");
    }
    app.shutdown();
}

// =================================================================================================
// 2. The housing-dungeon / interior-cell departure, and the return after it
// =================================================================================================

/// Query the current physics source's cached geometry for one interior cell without loading.
/// DatLandSource::load_block_cells populates the table; LandSource::env_cell only looks it up.
fn env_cell_known(app: &App, cell: CellId) -> bool {
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    dereth_physics::LandSource::env_cell(&*land, cell).is_some()
}

fn landblock_resident(app: &App, block: LandblockId) -> bool {
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    dereth_physics::LandSource::landblock_resident(&*land, block)
}

/// The centre of land cell `index` of `block`, on the terrain.
fn land_cell_centre(app: &App, block: LandblockId, index: u16) -> Vec3 {
    let i = f32::from(index - 1);
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (((i / 8.0).floor()) * 24.0 + 12.0, (i % 8.0) * 24.0 + 12.0);
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    Vec3::new(x, y, land.ground_height(block, x, y).unwrap_or(0.0) + 1.0)
}

/// Find a decoded interior referenced by a building in this block. building_cells indexes
/// buildings by the land cell containing their frame origin. The land-cell centre plus terrain
/// height is only an approximate position candidate: it does not prove containment in that
/// particular interior. The test observes placement and deadline results rather than forcing
/// them.
fn decoded_interior(app: &App, block: LandblockId) -> Option<(CellId, Vec3)> {
    let land = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .land()
        .clone();
    for index in 1..=64u16 {
        let interiors = dereth_physics::LandSource::building_cells(&*land, block.cell(index));
        if let Some(cell) = interiors.into_iter().find(|c| !c.is_outdoor()) {
            return Some((cell, land_cell_centre(app, block, index)));
        }
    }
    None
}

/// An interior cell id of `block` that the client does **not** hold geometry for.
fn undecoded_interior(app: &App, block: LandblockId) -> CellId {
    (0x0100..0x0200u16)
        .map(|i| block.cell(i))
        .find(|c| !env_cell_known(app, *c))
        .expect("an interior index the block does not carry")
}

struct EnvArm {
    label: &'static str,
    cell: CellId,
    origin: Vec3,
    /// Record the prefetched-block latch separately from this specific cell's decoded geometry.
    /// Loading a block sets the latch and fills available cells, but does not make every possible
    /// interior index valid; the loaded-but-missing-index arm explicitly checks that distinction.
    resident: bool,
    decoded: bool,
}

/// Behaviour: objects.lifecycle.repeated-leave-return-keeps-one-drawn-body-per-object
///
/// Interior-departure question, motivated by a player returning from a housing dungeon without
/// reappearing to the survivor. The remote 0xF748 names an interior cell; LandSource::env_cell
/// resolves it from cached geometry without loading. Exercise a loaded block's missing index,
/// an unloaded block and a decoded interior, twice each. Observe whether placement queues a
/// deadline, require culling to agree with that verdict, and require every return to draw.
#[test]
fn an_interior_cell_departure_still_lets_the_return_draw() {
    let mut app = app();
    let mut peer = Peer::attach(&mut app);
    seen(&mut app, &mut peer);
    let block = home_block(&app);

    // load_block_cells inserts the prefetched latch and decodes available geometry. Confirm
    // current residency, then find a missing interior index for the loaded-but-unresolved arm.
    // This does not assert that every prefetched block has every possible interior decoded.
    assert!(
        landblock_resident(&app, block),
        "the block under the viewer is prefetched"
    );
    let (decoded_cell, decoded_origin) = decoded_interior(&app, block)
        .expect("the viewer's landblock carries at least one building interior");
    assert!(
        env_cell_known(&app, decoded_cell),
        "and load_block_cells decoded it"
    );

    let far = LandblockId((u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()));
    assert!(
        !landblock_resident(&app, far),
        "the far block is outside the streamed window"
    );

    let arms = [
        EnvArm {
            label: "(a) resident block, an interior index it does not carry",
            cell: undecoded_interior(&app, block),
            origin: decoded_origin,
            resident: true,
            decoded: false,
        },
        EnvArm {
            label: "(b) a landblock this client has never loaded",
            cell: far.cell(0x0100),
            origin: Vec3::new(12.0, 12.0, 0.0),
            resident: false,
            decoded: false,
        },
        EnvArm {
            label: "(c) the viewer's own block, a decoded building interior",
            cell: decoded_cell,
            origin: decoded_origin,
            resident: true,
            decoded: true,
        },
    ];

    let mut stamp: u16 = 1;
    let mut evidence: Vec<String> = Vec::new();

    for arm in &arms {
        assert_eq!(
            landblock_resident(&app, arm.cell.landblock()),
            arm.resident,
            "{}",
            arm.label
        );
        assert_eq!(env_cell_known(&app, arm.cell), arm.decoded, "{}", arm.label);
        assert!(
            !arm.cell.is_outdoor(),
            "{}: an env cell index, not one of the 64 land cells",
            arm.label
        );

        // Repeat each arm to expose state that might persist from the preceding departure,
        // cull or rescue; a single pass can still reveal immediate defects.
        for cycle in 0..2u32 {
            let departure = Position::new(arm.cell, Frame::new(arm.origin, Quat::IDENTITY));
            stamp = stamp.wrapping_add(1);
            peer.send(&mut app, &remote_position(REMOTE, departure, stamp));
            peer.send(&mut app, &set_state(REMOTE, HIDDEN, stamp, 1));
            frames(&mut app, 3);

            let placed = body_cell(&app, REMOTE).flatten();
            let deadline = doomed(&app, REMOTE);
            let wand_deadline = doomed(&app, WAND);
            assert_eq!(
                drawn(&app, REMOTE),
                0,
                "{} cycle {cycle}: the departed player is not drawn while HIDDEN_PS",
                arm.label
            );

            // Cross the destruction deadline through a real TimeSync, whatever the verdict was.
            let after = app.clock().cur_time + DESTRUCTION_TIME + 5.0;
            peer.advance_to(&mut app, REMOTE, HIDDEN, stamp, 1, after);
            frames(&mut app, 3);
            let culled = app.objects().presence(REMOTE).is_none();
            assert_eq!(
                culled,
                deadline.is_some(),
                "{} cycle {cycle}: the 25 s cull follows the placement verdict and nothing else",
                arm.label
            );
            if culled {
                // Held children leave visibility and are culled with their wielder.
                assert!(
                    app.objects().presence(WAND).is_none(),
                    "{} cycle {cycle}: the held wand was culled with its wielder",
                    arm.label
                );
            }

            // --- the return, in ACE's order ---------------------------------------------------
            stamp = stamp.wrapping_add(1);
            // Whether departure culled or retained the object, supply the return position in
            // ACE's create order. Use a camera-relative candidate and assert the resulting draw
            // below rather than assuming visibility from the chosen coordinates alone.
            let back = in_view(&app, 2.0);
            peer.send(
                &mut app,
                &create(
                    REMOTE,
                    seen_wdesc("Returning Player", true),
                    back,
                    1,
                    stamp,
                    stamp,
                    HIDDEN,
                    Some(WAND),
                ),
            );
            peer.send(&mut app, &wand(REMOTE, 1));
            frames(&mut app, 2);
            peer.send(
                &mut app,
                &set_state(REMOTE, VISIBLE, stamp.wrapping_add(1), 1),
            );
            stamp = stamp.wrapping_add(1);
            let (converged, trajectory) = settle_near(&mut app, REMOTE, back.frame.origin, 2.5);
            frames(&mut app, RAMP_FRAMES);

            assert_eq!(
                bodies(&app, REMOTE),
                1,
                "{} cycle {cycle}: one body for the id",
                arm.label
            );
            assert!(
                converged,
                "{} cycle {cycle}: the returned body is not at the return position {:?}; \
                 pose every 8 frames {trajectory:?}",
                arm.label, back.frame.origin
            );
            assert_eq!(
                body_cell(&app, REMOTE).flatten().map(CellId::landblock),
                Some(block),
                "{} cycle {cycle}: and it stands in the resident block",
                arm.label
            );
            assert_eq!(
                app.objects().physics_state(REMOTE),
                Some(VISIBLE),
                "{} cycle {cycle}: the login-complete 0xF74B un-hid him",
                arm.label
            );
            assert!(
                doomed(&app, REMOTE).is_none(),
                "{} cycle {cycle}: on no queue",
                arm.label
            );
            assert!(
                drawn(&app, REMOTE) > 0,
                "{} cycle {cycle}: he draws",
                arm.label
            );
            assert!(
                radar_dot(&app, REMOTE),
                "{} cycle {cycle}: with a radar blip",
                arm.label
            );
            assert_eq!(
                app.objects().presence(WAND).and_then(|p| p.parent),
                Some((REMOTE, RIGHT_HAND)),
                "{} cycle {cycle}: with the wand parented",
                arm.label
            );
            assert!(
                drawn(&app, WAND) > 0,
                "{} cycle {cycle}: and the wand drawn",
                arm.label
            );

            evidence.push(format!(
                "{:<52} cycle {cycle}: departure cell {:#010X} placed {:?} doomed {} wand-doomed {} \
                 culled {culled} -> return parts {} wand {} radar {}",
                arm.label,
                arm.cell.0,
                placed,
                deadline.is_some(),
                wand_deadline.is_some(),
                drawn(&app, REMOTE),
                drawn(&app, WAND),
                radar_dot(&app, REMOTE),
            ));
        }
    }

    eprintln!("leave/return churn -- interior-cell departures:");
    for line in &evidence {
        eprintln!("  {line}");
    }
    app.shutdown();
}

// =================================================================================================
// 3. The NPC duplicate shape: A leaves Holtburg's landblock and comes back
// =================================================================================================

/// Count distinct presence IDs with this name that submitted parts in the draw trace.
/// Same-name counts distinguish fresh-ID duplicates from reuse of an existing ID. This does
/// not enumerate physical arena allocations or measure the bodies' separation in pixels.
fn drawn_named(app: &App, name: &str) -> usize {
    app.objects()
        .presences()
        .filter(|(id, _)| {
            app.objects()
                .world
                .weenie(*id)
                .is_some_and(|w| w.pwd.name == name)
                && drawn(app, *id) > 0
        })
        .count()
}

fn known(app: &App, id: ObjectId) -> bool {
    app.objects().presence(id).is_some()
}

/// Send the local player's 0xF748 with an advanced TELEPORT_TS. The local teleport path
/// completes the player-position update and releases the resident landblock window; the
/// subsequent frame checks observe destruction deadlines on objects left behind.
fn teleport_local(app: &mut App, peer: &mut Peer, to: Position, stamp: &mut u16) {
    *stamp = stamp.wrapping_add(1);
    let applied = app.probe().player_teleports_applied();
    peer.send(app, &position_message(LOCAL, to, 1, *stamp, *stamp));
    frames(app, 14);
    assert_eq!(
        app.probe().player_teleports_applied(),
        applied + 1,
        "the local 0xF748 to {:#010X} was admitted",
        to.cell.0
    );
}

const NPC_NAMES: [&str; 3] = ["Aun Tigrana", "Baron Nerine", "Town Crier"];

/// Behaviour: objects.lifecycle.repeated-leave-return-keeps-one-drawn-body-per-object
///
/// NPC-reload question. ACE unloads idle landblocks after five minutes and recreates creatures
/// with fresh dynamic GUIDs and a fresh face, and relies on client culling for objects the server
/// forgets. This motivates fresh-ID, same-name inputs.
///
/// Four sequential variants test: expiry then same-ID creates; return before expiry with no
/// creates; expiry then new-ID creates; and return before expiry followed by new-ID creates.
/// The fixture does not run ACE's reload or face generation: it uses fixed descriptions and
/// synthetic IDs. The final variant demonstrates a possible duplicate sequence, not proof
/// that a server sends that sequence.
#[test]
fn leaving_and_returning_to_the_npc_landblock_leaves_one_body_per_npc() {
    let mut app = app();
    let mut peer = Peer::attach(&mut app);

    // The local player, so the `0xF748` below is the real teleport path and not a remote move.
    let start = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    peer.send(&mut app, &LoginCreatePlayer { player_id: LOCAL });
    let mut me = create(
        LOCAL,
        seen_wdesc("Survivor", true),
        start,
        1,
        0,
        0,
        VISIBLE,
        None,
    );
    me.0.physicsdesc.setup_id = Some(dereth_client::character::ALUVIAN_MALE_SETUP.0);
    peer.send(&mut app, &me);
    frames(&mut app, 4);
    assert_eq!(
        app.objects().player(),
        Some(LOCAL),
        "the local player id is the server's"
    );

    let block = home_block(&app);
    let far = LandblockId((u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()));
    let away = on_terrain(&app, far, 96.0, 96.0);

    // Three NPCs standing in front of the survivor, each with its own id and name.
    let npcs: Vec<(ObjectId, &str, Position)> = NPC_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let spot = near_player(&app, (i as f32 - 1.0) * 3.0, 8.0);
            (ObjectId(0x5000_1D00 + i as u32), *name, spot)
        })
        .collect();
    let mut stamp: u16 = 1;
    for (id, name, spot) in &npcs {
        peer.send(
            &mut app,
            &create(*id, seen_wdesc(name, false), *spot, 1, 0, 0, VISIBLE, None),
        );
    }
    let ids: Vec<ObjectId> = npcs.iter().map(|(id, ..)| *id).collect();
    let tunnel_frames = wait_until_drawn(&mut app, &ids, 2000);
    frames(&mut app, 31);
    for (id, name, _) in &npcs {
        assert!(
            doomed(&app, *id).is_none(),
            "{name} was placed, so it is on no queue"
        );
        assert_eq!(
            drawn_named(&app, name),
            1,
            "one {name} on screen to begin with"
        );
    }

    let mut evidence: Vec<String> = vec![format!(
        "0  the login tunnel hid the world for {tunnel_frames} frame(s)"
    )];

    // ---------------------------------------------------------------------------------------
    // A. Away past the deadline; the server re-tracks the **same** ids on return.
    // ---------------------------------------------------------------------------------------
    teleport_local(&mut app, &mut peer, away, &mut stamp);
    for (id, name, _) in &npcs {
        assert!(
            doomed(&app, *id).is_some(),
            "A: {name}'s landblock was released and the object has a destruction deadline"
        );
    }
    let after = app.clock().cur_time + DESTRUCTION_TIME + 5.0;
    peer.advance_to(&mut app, LOCAL, VISIBLE, 0, 1, after);
    frames(&mut app, 3);
    for (id, name, _) in &npcs {
        assert!(
            !known(&app, *id),
            "A: the lifecycle deadline sweep culled {name} at its deadline"
        );
        assert_eq!(
            drawn_named(&app, name),
            0,
            "A: and nothing named {name} is drawn"
        );
    }

    teleport_local(&mut app, &mut peer, start, &mut stamp);
    stamp = stamp.wrapping_add(1);
    for (id, name, spot) in &npcs {
        let mut moved = *spot;
        moved.frame.origin.x += 2.0;
        peer.send(
            &mut app,
            &create(
                *id,
                seen_wdesc(name, false),
                moved,
                1,
                stamp,
                stamp,
                VISIBLE,
                None,
            ),
        );
    }
    frames(&mut app, RAMP_FRAMES);
    for (id, name, _) in &npcs {
        assert_eq!(
            bodies(&app, *id),
            1,
            "A: exactly one body answers to {name}'s id"
        );
        assert!(
            drawn(&app, *id) > 0,
            "A: {name} draws again after the return"
        );
        assert!(
            doomed(&app, *id).is_none(),
            "A: and is on no destruction queue"
        );
        assert!(radar_dot(&app, *id), "A: with a radar blip");
        assert_eq!(
            drawn_named(&app, name),
            1,
            "A: exactly one {name} on screen"
        );
    }
    evidence.push(format!(
        "A  away {DESTRUCTION_TIME:.0}+ s, same ids : per-name drawn {:?}, ids known {:?}",
        NPC_NAMES.map(|n| drawn_named(&app, n)),
        npcs.iter()
            .map(|(id, ..)| known(&app, *id))
            .collect::<Vec<_>>()
    ));

    // ---------------------------------------------------------------------------------------
    // B. Return before the deadline with no fresh creates. Re-entering the resident cells
    // rescues the existing objects and cancels their destruction deadlines; handle identity
    // is asserted in this arm as well as draw and same-name counts.
    // ---------------------------------------------------------------------------------------
    let handles_before: Vec<_> = npcs
        .iter()
        .map(|(id, ..)| app.objects().physics.handle(*id))
        .collect();
    teleport_local(&mut app, &mut peer, away, &mut stamp);
    for (id, name, _) in &npcs {
        assert!(
            doomed(&app, *id).is_some(),
            "B: {name} is queued on the way out"
        );
    }
    teleport_local(&mut app, &mut peer, start, &mut stamp);
    frames(&mut app, RAMP_FRAMES);
    for ((id, name, _), before) in npcs.iter().zip(&handles_before) {
        assert_eq!(bodies(&app, *id), 1, "B: one body for {name}");
        assert_eq!(
            app.objects().physics.handle(*id),
            *before,
            "B: {name} re-entered on the same physical body"
        );
        assert!(
            doomed(&app, *id).is_none(),
            "B: and left the destruction queue on re-entry"
        );
        assert!(drawn(&app, *id) > 0, "B: {name} draws after the round trip");
        assert_eq!(
            drawn_named(&app, name),
            1,
            "B: exactly one {name} on screen"
        );
    }
    evidence.push(format!(
        "B  away < {DESTRUCTION_TIME:.0} s, nothing sent: per-name drawn {:?}, same handles {}",
        NPC_NAMES.map(|n| drawn_named(&app, n)),
        npcs.iter()
            .zip(&handles_before)
            .all(|((id, ..), h)| app.objects().physics.handle(*id) == *h)
    ));

    // ---------------------------------------------------------------------------------------
    // C. Expire the old IDs, then supply fresh GUIDs with the same descriptions. This models
    // reload identity churn without executing server reload or generating different faces.
    // ---------------------------------------------------------------------------------------
    teleport_local(&mut app, &mut peer, away, &mut stamp);
    let after = app.clock().cur_time + DESTRUCTION_TIME + 5.0;
    peer.advance_to(&mut app, LOCAL, VISIBLE, 0, 1, after);
    frames(&mut app, 3);
    for (id, name, _) in &npcs {
        assert!(
            !known(&app, *id),
            "C: {name}'s old id was culled at its deadline"
        );
    }
    teleport_local(&mut app, &mut peer, start, &mut stamp);
    stamp = stamp.wrapping_add(1);
    let reloaded: Vec<(ObjectId, &str, Position)> = npcs
        .iter()
        .enumerate()
        .map(|(i, (_, name, spot))| {
            let mut moved = *spot;
            moved.frame.origin.x -= 2.0;
            #[allow(clippy::cast_possible_truncation)]
            (ObjectId(0x5000_1E00 + i as u32), *name, moved)
        })
        .collect();
    for (id, name, spot) in &reloaded {
        peer.send(
            &mut app,
            &create(
                *id,
                seen_wdesc(name, false),
                *spot,
                1,
                stamp,
                stamp,
                VISIBLE,
                None,
            ),
        );
    }
    frames(&mut app, RAMP_FRAMES);
    for ((old, name, _), (new, ..)) in npcs.iter().zip(&reloaded) {
        assert!(
            !known(&app, *old),
            "C: the guid the server dropped stays dropped"
        );
        assert_eq!(bodies(&app, *old), 0, "C: and owns no body");
        assert_eq!(
            bodies(&app, *new),
            1,
            "C: {name}'s new guid owns exactly one"
        );
        assert!(drawn(&app, *new) > 0, "C: {name} draws under his new guid");
        assert_eq!(
            drawn_named(&app, name),
            1,
            "C: exactly one {name} on screen -- a landblock reload after a full cull does not \
             double him"
        );
    }
    evidence.push(format!(
        "C  away {DESTRUCTION_TIME:.0}+ s, new guids : per-name drawn {:?}, old ids known {:?}",
        NPC_NAMES.map(|n| drawn_named(&app, n)),
        npcs.iter()
            .map(|(id, ..)| known(&app, *id))
            .collect::<Vec<_>>()
    ));

    // ---------------------------------------------------------------------------------------
    // D. Return before 25 s, allowing visibility re-entry to rescue existing IDs, then supply
    // fresh IDs for the same names. No deletion arrives for the rescued objects. With successful
    // placement there is no new failed-placement deadline to remove them automatically.
    // Require two drawn IDs per name ([2,2,2]), while each individual ID has one indexed body.
    // This demonstrates the duplicate shape under these synthetic inputs; it does not establish
    // the server's actual message history or prove spatial separation through a pixel measurement.
    // ---------------------------------------------------------------------------------------
    let live: Vec<(ObjectId, &str, Position)> = reloaded.clone();
    teleport_local(&mut app, &mut peer, away, &mut stamp);
    teleport_local(&mut app, &mut peer, start, &mut stamp);
    frames(&mut app, 8);
    for (id, name, _) in &live {
        assert!(
            doomed(&app, *id).is_none(),
            "D: {name} was rescued by the return, as retail does"
        );
        assert!(known(&app, *id), "D: and is still tracked");
    }
    stamp = stamp.wrapping_add(1);
    let third: Vec<(ObjectId, &str, Position)> = live
        .iter()
        .enumerate()
        .map(|(i, (_, name, spot))| {
            let mut moved = *spot;
            moved.frame.origin.y += 4.0;
            #[allow(clippy::cast_possible_truncation)]
            (ObjectId(0x5000_1F00 + i as u32), *name, moved)
        })
        .collect();
    for (id, name, spot) in &third {
        peer.send(
            &mut app,
            &create(
                *id,
                seen_wdesc(name, false),
                *spot,
                1,
                stamp,
                stamp,
                VISIBLE,
                None,
            ),
        );
    }
    frames(&mut app, RAMP_FRAMES);
    let doubled: Vec<usize> = NPC_NAMES.iter().map(|n| drawn_named(&app, n)).collect();
    for ((old, name, _), (new, ..)) in live.iter().zip(&third) {
        assert_eq!(
            bodies(&app, *old),
            1,
            "D: the rescued id still owns exactly one body"
        );
        assert_eq!(
            bodies(&app, *new),
            1,
            "D: and the new guid owns exactly one of its own"
        );
        assert!(drawn(&app, *new) > 0, "D: {name}'s new guid draws");
    }
    assert_eq!(
        doubled,
        vec![2, 2, 2],
        "D: two bodies per name is what this sequence *must* produce -- the client was never \
         told the first set was gone and a resident block gives it no cull trigger. If this ever \
         reads 1, something started deleting objects the server never deleted."
    );
    evidence.push(format!(
        "D  away < {DESTRUCTION_TIME:.0} s, new guids : per-name drawn {doubled:?} \
         (server-shaped duplicate; no client-side id is doubled)"
    ));

    eprintln!("leave/return churn -- Holtburg leave/return:");
    for line in &evidence {
        eprintln!("  {line}");
    }
    app.shutdown();
}
