//! **Client lifecycle when a previously seen remote player leaves and returns.**
//!
//! The modeled message schedules follow ACE:
//! * Expired known objects are removed without sending a delete (`0xF747`); the 25-second queue
//!   relies on the client culling its own copy.
//! * Adjacency moves suppress the delete broadcast. A teleport sends the position update naming
//!   the destination, then broadcasts the hidden/ignore-collisions state only when those values
//!   or report-collisions changed.
//! * An occluded known-visible object is queued without leaving the visible set, and destruction
//!   is cancelled only when it becomes newly visible. A return while it remains visible can leave
//!   the expiry queued; a later cell entry after expiry can produce a new create for a client
//!   that kept its body.
//! These tests construct those schedules; they do not run the server or prove every teleport
//! emits only this sequence of messages.
//!
//! The client's placement into no cell queues both the object and each child for destruction.
//! Expiry removes physics and object data and unparents children. An equal-instance create
//! synchronously applies its received position, then cancels destruction if the body has a cell
//! or queues a fresh deadline if it does not. So an already-culled object can be created afresh,
//! while a placed merge can survive its old deadline. The two orderings are exercised below as
//! separate fresh-create and same-frame-merge cases, not every possible interleaving. A remote
//! whose cell the observer's own teleport released, and who then returns in the equal-instance
//! form, is drawn again with the wand he holds on the same retained body.
//!
//! Fixture: synthetic messages serialized into replay datagrams with the configured crypto
//! state, fed directly to the transport of a headless `App` and delivered by ordinary
//! `App::frame` calls. A `TimeSync` header advances the application clock alongside its blobs
//! before maintenance and object sync. No socket is used. Draw counts observe submitted parts,
//! not a pixel-perfect image or exact final position.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::SceneReads;
use dereth_client::{app::App, config::Config, net::ClientNetwork, world::SceneConfig};
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::{
    movement::{position_flags, MovementPositionEvent, PositionPack},
    objects::{ItemCreateObject, ItemSetState, ObjectCreatePayload},
    types::{
        physicsdesc::{flags, ChildLink},
        PhysicsDesc, PhysicsEventStamp, PhysicsTimestamps, PositionWire, PublicWeenieDesc,
    },
    Message,
};

const REMOTE: ObjectId = ObjectId(0x5000_1F95);
const WAND: ObjectId = ObjectId(0x5000_1F96);
const PLAYER_SETUP: u32 = 0x0200_0001;
const PLAYER_MTABLE: u32 = 0x0900_0001;
const PLAYER_PSCRIPT_TABLE: u32 = 0x3400_0004;
const WAND_SETUP: u32 = 0x0200_1713;
const RIGHT_HAND: u32 = 1;
/// The corpus's player teleport words: `HIDDEN_PS | IGNORE_COLLISIONS_PS` on, and the visible
/// `0x00400408` ACE answers the login-complete action with.
const HIDDEN: u32 = 0x0040_4410;
const VISIBLE: u32 = 0x0040_0408;
/// The client's destruction grace period: queue at the current clock plus 25 seconds.
const DESTRUCTION_TIME: f64 = 25.0;
/// Allow 40 headless frames for the visible-state 0.75 s ramp plus slack; no exact ramp timing
/// is asserted by the later positive draw-count checks.
const RAMP_FRAMES: usize = 40;

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "remote-leave-return", "unused", 0)
                .unwrap();
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
            },
            net,
        )
    }

    fn send<M: Message>(&mut self, app: &mut App, message: &M) {
        self.send_batch(app, &[dereth_protocol::write_blob(message).unwrap()], None);
    }

    /// Put blobs in order in one synthetic datagram, optionally with TimeSync. Feed it to the
    /// replay transport without polling a socket. The following App::frame consumes the clock
    /// update and messages before expiry maintenance, which supplies the tested ordering seam.
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
}

fn app() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir()
            .join("dere-remote-leave-return-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.load_static_scene(SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: true,
        ..Default::default()
    })
    .unwrap();
    app
}

/// Synthetic player create. ACE sets the physics-description children flag for a nonempty child
/// list; `with_children` models that serialization arm with the wand in the right-hand location.
/// Position/state stamps vary while instance stays 1.
fn remote(
    at: Position,
    position_stamp: u16,
    state_stamp: u16,
    state: u32,
    with_children: bool,
) -> ItemCreateObject {
    let mut bitfield = flags::POSITION | flags::SETUP | flags::MTABLE | flags::PETABLE;
    if with_children {
        bitfield |= flags::CHILDREN;
    }
    ItemCreateObject(ObjectCreatePayload {
        id: REMOTE,
        objdesc: Default::default(),
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
            children: with_children.then(|| {
                vec![ChildLink {
                    child_id: WAND,
                    location_id: RIGHT_HAND,
                }]
            }),
            timestamps: PhysicsTimestamps {
                instance: 1,
                position: position_stamp,
                state: state_stamp,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    })
}

/// Identical wand create on every synthetic track: setup, parent/location and animation-frame
/// fields with instance 1 and default position stamp. This models ACE's tracking of an equipped
/// item with the item's sequences, for a held item whose position sequence has not advanced; the
/// test
/// constructs that equality rather than asserting all server-held items keep the same stamp.
fn wand() -> ItemCreateObject {
    ItemCreateObject(ObjectCreatePayload {
        id: WAND,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::PARENT | flags::ANIMFRAME,
            setup_id: Some(WAND_SETUP),
            parent: Some((REMOTE, RIGHT_HAND)),
            animframe_id: Some(1),
            timestamps: PhysicsTimestamps {
                instance: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    })
}

/// Synthetic position update following ACE's teleport sequence convention (as in
/// `objects::death_teleport_effects`): advance the position stamp while leaving teleport stamp 0
/// and instance 1. The fixture explicitly
/// supplies grounded/omitted-orientation flags; it is not a captured teleport packet.
fn remote_position(at: Position, position_stamp: u16) -> MovementPositionEvent {
    let flags = position_flags::IS_GROUNDED
        | position_flags::ORIENTATION_HAS_NO_W
        | position_flags::ORIENTATION_HAS_NO_X
        | position_flags::ORIENTATION_HAS_NO_Y
        | position_flags::ORIENTATION_HAS_NO_Z;
    MovementPositionEvent {
        id: REMOTE,
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
            position_timestamp: position_stamp,
            teleport_timestamp: 0,
            ..PositionPack::default()
        },
    }
}

fn state(word: u32, event: u16) -> ItemSetState {
    ItemSetState {
        id: REMOTE,
        state: word,
        timestamps: PhysicsEventStamp { instance: 1, event },
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

fn doomed(app: &App, id: ObjectId) -> Option<f64> {
    app.objects().world.tables.doomed.get(id).map(|t| t.0)
}

fn body_cell(app: &App, id: ObjectId) -> Option<Option<dereth_primitives::CellId>> {
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

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

/// Place a candidate 10 m forward in the current camera frame with the requested lateral offset,
/// retaining the local body's cell. Later positive draw-count predicates check the selected
/// placements; this helper alone does not prove visibility or ground contact.
fn in_view(app: &App, right: f32) -> Position {
    let scene = app.world_scene().unwrap();
    let character = scene.character.as_ref().unwrap();
    let mut position = character.world.get(character.handle).unwrap().position;
    position.frame.origin =
        dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(right, 10.0, 0.0));
    position
}

/// Copy the placed body's cell/pose and offset world x by 6 m. This preserves its z and cell ID,
/// but does not independently prove terrain support or exact acceptance at the requested point.
fn beside(app: &App, id: ObjectId) -> Position {
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
    position.frame.origin.x += 6.0;
    position
}

fn unloaded_cell(app: &App) -> Position {
    let mut far = in_view(app, 0.0);
    let block = far.cell.landblock();
    let far_block = dereth_primitives::LandblockId(
        (u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()),
    );
    far.cell = far_block.cell(1);
    far
}

/// Start with positive body/wand draw counts, then send the modeled position-before-hidden
/// sequence to a block five x indices away. After two frames require no remote draw and an
/// actual queued deadline; subsequent tests inspect expiry or return behavior.
fn seen_then_departed(app: &mut App, peer: &mut Peer) -> f64 {
    let home = in_view(app, 0.0);
    peer.send(app, &remote(home, 0, 0, VISIBLE, true));
    peer.send(app, &wand());
    frames(app, 31);
    assert!(
        drawn(app, REMOTE) > 0,
        "the remote body starts through the actual draw pass"
    );
    assert!(
        drawn(app, WAND) > 0,
        "its wand starts through the same pass"
    );

    let far = unloaded_cell(app);
    peer.send(app, &remote_position(far, 1));
    peer.send(app, &state(HIDDEN, 1));
    frames(app, 2);
    assert_eq!(
        drawn(app, REMOTE),
        0,
        "the departed player is not drawn while HIDDEN_PS"
    );
    doomed(app, REMOTE).expect("departure to an unloaded cell queued the body for destruction")
}

/// Behaviour: objects.lifecycle.a-remote-that-leaves-and-returns-rematerializes-with-its-held-child
///
/// Cross the departed body's deadline through TimeSync plus a repeated state stamp. After
/// three frames require the remote object/physics handle gone, neither object drawn and the
/// wand object absent. This checks eventual joint removal, not exact equality of child deadlines.
#[test]
fn a_remote_teleporting_to_an_unloaded_landblock_is_culled_with_its_held_child() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();

    let deadline = seen_then_departed(&mut app, &mut peer);
    assert!(
        (deadline - (app.clock().cur_time + DESTRUCTION_TIME)).abs() < 1.0,
        "the deadline is within one second of the current clock plus 25.0, got {deadline} vs {}",
        app.clock().cur_time
    );

    // TimeSync crosses the deadline. The repeated hidden-state stamp supplies no newer state
    // update; normal frame simulation and expiry maintenance still run.
    let repeat = dereth_protocol::write_blob(&state(HIDDEN, 1)).unwrap();
    peer.send_batch(&mut app, &[repeat], Some(deadline + 0.5));
    frames(&mut app, 3);

    assert!(
        app.objects().presence(REMOTE).is_none(),
        "the culled player left the object table"
    );
    assert!(
        app.objects().physics.handle(REMOTE).is_none(),
        "and his body was destroyed"
    );
    assert_eq!(drawn(&app, REMOTE), 0);
    assert_eq!(drawn(&app, WAND), 0, "an orphaned wand is not drawn");

    // The client's departure handling queues held children too. Check the wand's object removal;
    // its exact deadline and physics handle are not separately compared in this test.
    assert!(
        app.objects().presence(WAND).is_none(),
        "the held wand was culled with its departed wielder"
    );
}

/// Behaviour: objects.lifecycle.a-remote-that-leaves-and-returns-rematerializes-with-its-held-child
///
/// After confirmed culling, model ACE re-tracking with a hidden player create listing children,
/// the wand create, then a visible-state message. Require the parent link, a body cell in the
/// requested landblock and positive draw counts after the ramp allowance. No exact target pose
/// or per-frame fade curve is asserted.
#[test]
fn a_culled_remote_returning_as_a_fresh_create_materializes_body_and_wand() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();

    let deadline = seen_then_departed(&mut app, &mut peer);
    let repeat = dereth_protocol::write_blob(&state(HIDDEN, 1)).unwrap();
    peer.send_batch(&mut app, &[repeat], Some(deadline + 0.5));
    frames(&mut app, 3);
    assert!(
        app.objects().presence(REMOTE).is_none(),
        "precondition: the player was culled"
    );

    let back = in_view(&app, 2.0);
    peer.send(&mut app, &remote(back, 2, 2, HIDDEN, true));
    peer.send(&mut app, &wand());
    frames(&mut app, 2);
    assert_eq!(
        drawn(&app, REMOTE),
        0,
        "a hidden arrival is not drawn before its unhide"
    );
    peer.send(&mut app, &state(VISIBLE, 3));
    frames(&mut app, RAMP_FRAMES);

    assert_eq!(
        app.objects().presence(WAND).unwrap().parent,
        Some((REMOTE, RIGHT_HAND)),
        "the wand is attached to the returned player"
    );
    let cell = body_cell(&app, REMOTE).expect("the returned player has a body");
    assert_eq!(
        cell.map(|c| c.landblock()),
        Some(back.cell.landblock()),
        "and it stands in the resident block"
    );
    assert!(
        drawn(&app, REMOTE) > 0,
        "the visible-state update returned the player's parts to the draw pass"
    );
    assert!(drawn(&app, WAND) > 0, "and the wand draws attached to him");
}

/// Behaviour: objects.lifecycle.a-remote-that-leaves-and-returns-rematerializes-with-its-held-child
///
/// Deliver a return create, wand create and visible-state message with TimeSync past the old
/// deadline in one datagram. One App::frame must leave the remote known, and later frames must
/// draw both parts with the parent link intact. This checks the deliver-before-maintenance race;
/// the preceding test covers creating after culling has already happened.
///
/// Equal-instance placement cancels the old deadline when the body finds a cell and renews it
/// otherwise. The modeled server considers its new create sufficient; this test
/// supplies no additional recovery create after the raced batch.
#[test]
fn a_return_create_landing_on_the_cull_frame_keeps_the_remote() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();

    let deadline = seen_then_departed(&mut app, &mut peer);
    let back = in_view(&app, 2.0);
    let batch = [
        dereth_protocol::write_blob(&remote(back, 2, 2, HIDDEN, true)).unwrap(),
        dereth_protocol::write_blob(&wand()).unwrap(),
        dereth_protocol::write_blob(&state(VISIBLE, 3)).unwrap(),
    ];
    peer.send_batch(&mut app, &batch, Some(deadline + 0.5));
    assert!(app.frame());

    assert!(
        app.objects().presence(REMOTE).is_some(),
        "the returning player survives the frame that crosses its old cull deadline"
    );
    frames(&mut app, RAMP_FRAMES);
    assert!(
        app.objects().presence(REMOTE).is_some(),
        "the player is still known after the ramp"
    );
    assert!(drawn(&app, REMOTE) > 0, "and drawn at the return position");
    assert_eq!(
        app.objects().presence(WAND).unwrap().parent,
        Some((REMOTE, RIGHT_HAND))
    );
    assert!(drawn(&app, WAND) > 0, "with his wand");
}

/// Behaviour: objects.lifecycle.an-equal-instance-return-reattaches-the-retained-body-and-held-item
///
/// Model the ACE retained-visible case: the observer keeps a placed body while server-side
/// tracking expires silently. Advance time by 50 seconds with a repeated visible-state stamp;
/// do not run a server or claim a period with no synthetic traffic. Then send equal-instance
/// player/wand creates, with a newer player position/state and the same wand position stamp.
///
/// Require the original player handle, hidden body/wand draws, no new unplaced count or queued
/// deadline, then visible parts and the wand parent link. Sample every ten frames up to 150,
/// accepting distance < 2.5 m from the target only after at least 40 frames. This is bounded
/// approach to the target, not exact placement, a monotonic trajectory or a measured fade
/// duration.
#[test]
fn a_silently_dropped_visible_remote_rematerializes_from_the_equal_instance_create() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();

    let home = in_view(&app, 0.0);
    peer.send(&mut app, &remote(home, 0, 0, VISIBLE, true));
    peer.send(&mut app, &wand());
    frames(&mut app, 31);
    assert!(drawn(&app, REMOTE) > 0);
    assert!(drawn(&app, WAND) > 0);
    let handle = app.objects().physics.handle(REMOTE).expect("placed body");
    assert!(
        doomed(&app, REMOTE).is_none(),
        "a placed body is on no destruction queue"
    );

    // Model elapsed time without a new tracking create; a repeated state accompanies TimeSync.
    let repeat = dereth_protocol::write_blob(&state(VISIBLE, 0)).unwrap();
    let later = app.clock().cur_time + 2.0 * DESTRUCTION_TIME;
    peer.send_batch(&mut app, &[repeat], Some(later));
    frames(&mut app, 3);
    assert!(
        drawn(&app, REMOTE) > 0,
        "the retained body is still drawn where he stood"
    );

    let back = beside(&app, REMOTE);
    let unplaced_before = app.objects().physics.stats.unplaced;
    peer.send(&mut app, &remote(back, 2, 2, HIDDEN, true));
    peer.send(&mut app, &wand());
    frames(&mut app, 2);
    assert_eq!(
        app.objects().physics.handle(REMOTE),
        Some(handle),
        "the merge kept the one body"
    );
    assert_eq!(
        drawn(&app, REMOTE),
        0,
        "the create merge hid the retained body"
    );
    assert_eq!(
        drawn(&app, WAND),
        0,
        "the held wand has no drawn parts while its parent is hidden"
    );
    assert_eq!(
        app.objects().physics.stats.unplaced - unplaced_before,
        0,
        "the return position six metres away did not increase the unplaced counter"
    );
    assert!(
        doomed(&app, REMOTE).is_none(),
        "the retained body has no destruction deadline after the merge"
    );
    peer.send(&mut app, &state(VISIBLE, 3));

    // An unchanged teleport stamp uses interpolation rather than an immediate snap. Keep the
    // sampled trajectory for diagnostics; success below is a 2.5 m proximity threshold.
    let pose = |app: &App| -> Vec3 {
        let scene = app.world_scene().unwrap();
        scene
            .character
            .as_ref()
            .unwrap()
            .world
            .get(handle)
            .unwrap()
            .position
            .frame
            .origin
    };
    let target = back.frame.origin;
    let apart = |a: Vec3| {
        ((a.x - target.x).powi(2) + (a.y - target.y).powi(2) + (a.z - target.z).powi(2)).sqrt()
    };
    let mut trajectory = vec![pose(&app)];
    let mut converged = false;
    for step in 0..15 {
        frames(&mut app, 10);
        let now = pose(&app);
        trajectory.push(now);
        if step >= 3 && apart(now) < 2.5 {
            converged = true;
            break;
        }
    }

    assert_eq!(app.objects().physics.handle(REMOTE), Some(handle));
    let scene = app.world_scene().unwrap();
    let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
    assert!(body.cell.is_some(), "the merged body has a cell");
    assert!(
        converged,
        "the body must get within 2.5 m of the return target after at least 40 frames: target {target:?}, pose every 10 frames after the merge {trajectory:?}"
    );
    assert!(
        drawn(&app, REMOTE) > 0,
        "the visible-state update returned the merged player to the draw pass"
    );
    assert_eq!(
        app.objects().presence(WAND).unwrap().parent,
        Some((REMOTE, RIGHT_HAND))
    );
    assert!(drawn(&app, WAND) > 0, "and his wand draws again");
}

/// Behaviour: objects.lifecycle.an-equal-instance-return-reattaches-the-retained-body-and-held-item
///
/// A player already known to this client leaves with a departing block, then returns in the
/// equal-instance `0xF745` form. The held wand distinguishes attachment materialization from
/// merely retaining the player's model row.
///
/// The placement/update path reactivates the retained body and completes the unhide even though
/// the merge lacks the `position_entries` marker a standalone `0xF748` leaves.
#[test]
fn equal_instance_return_create_materializes_remote_body_and_held_item() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();

    let home = {
        let scene = app.world_scene().unwrap();
        let mut position = scene
            .character
            .as_ref()
            .unwrap()
            .world
            .get(scene.character.as_ref().unwrap().handle)
            .unwrap()
            .position;
        position.frame.origin =
            dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(0.0, 10.0, 0.0));
        position
    };
    let nearby = home;
    peer.send(&mut app, &remote(nearby, 0, 0, VISIBLE, false));
    peer.send(&mut app, &wand());
    assert!(app.frame());
    for _ in 0..30 {
        assert!(app.frame());
    }
    assert!(
        drawn(&app, REMOTE) > 0,
        "remote body starts through the actual object draw pass"
    );
    assert!(
        drawn(&app, WAND) > 0,
        "its attached wand starts through the same pass"
    );

    let far_block = {
        let block = nearby.cell.landblock();
        dereth_primitives::LandblockId(
            (u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()),
        )
    };
    let mut far = nearby;
    far.cell = far_block.cell(1);
    app.probe_mut()
        .world_state_mut()
        .unwrap()
        .character
        .as_mut()
        .unwrap()
        .teleport(far);
    for _ in 0..30 {
        assert!(app.frame());
        let handle = app.objects().physics.handle(REMOTE).expect("retained body");
        if app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .world
            .get(handle)
            .unwrap()
            .cell
            .is_none()
        {
            break;
        }
    }
    let handle = app.objects().physics.handle(REMOTE).expect("retained body");
    {
        let scene = app.world_scene().unwrap();
        let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
        assert_eq!(
            body.cell, None,
            "the observer's block departure released the retained body's cell"
        );
    }

    // The server reintroduces the same physical instance with a newer POSITION_TS. This is the
    // exact merge arm, not a fresh body and not a test-only physics poke.
    peer.send(&mut app, &remote(far, 2, 1, HIDDEN, false));
    assert!(app.frame());
    assert_eq!(
        drawn(&app, REMOTE),
        0,
        "the merge's hidden state made the body transparent"
    );
    peer.send(
        &mut app,
        &ItemSetState {
            id: REMOTE,
            state: VISIBLE,
            timestamps: PhysicsEventStamp {
                instance: 1,
                event: 2,
            },
        },
    );
    assert!(app.frame());
    for _ in 0..30 {
        assert!(app.frame());
    }

    let scene = app.world_scene().unwrap();
    assert_eq!(
        app.objects().physics.handle(REMOTE),
        Some(handle),
        "merge retained the same body"
    );
    assert_eq!(
        app.objects().presence(WAND).unwrap().parent,
        Some((REMOTE, RIGHT_HAND)),
        "the authoritative held-child edge survived the holder merge",
    );
    let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
    assert_eq!(
        body.cell.map(|cell| cell.landblock()),
        Some(far.cell.landblock()),
        "same retained body re-entered the resident landblock after native cell normalization",
    );
    assert!(
        body.transient_state.is_active(),
        "the normal nearby update reactivated the body"
    );
    assert!(
        drawn(&app, REMOTE) > 0,
        "the unhide script returned the remote player's parts to drawing"
    );
    assert!(
        drawn(&app, WAND) > 0,
        "the authoritative held child remained attached and drawable"
    );
}
