//! A teleport releases every landscape block, and nothing of the origin survives it, including
//! traffic that names the origin after the window has moved. A teleport applies the same per-block
//! release as a ring shift (land-cell objects, visible interior cells, the block) to every slot of
//! the window rather than only to the slots scrolled off an edge; each released object leaves
//! visibility and is queued for destruction 25 s out. Landscape lookup refuses a block that is not
//! loaded, so a create naming a departed block gets no cell and joins the destruction queue, and
//! the per-frame placement retry cannot take a released block's cell back. A returning parent's
//! re-entry clears its own and its held children's deadlines. ACE makes the late traffic real: it
//! sends the client a position at the destination before moving the player out of the origin.
//! Fixture: the retail dats on a WARP device, Holtburg's default landblock; no datagram leaves.
//!
//! Sections of this module:
//! * `outdoor_objects`: objects in a departed block's outdoor cells are released by the scroll.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{
    CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, ServerTime, Vec3,
};
use dereth_protocol::Message;
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::landblock::block_xy,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene, dereth_scene::world_scene::WorldSceneRef,
};

/// A realistic non-static state word, the one the corpus carries for an opened door; the same
/// literal `world::landblock_object_release::outdoor_objects` uses, so the two modules share one station.
const DOOR_OPEN: u32 = 0x0001_001C;
/// Retail's nonvisible-object destruction interval, added to the current time when an object
/// enters the destruction queue.
const DESTRUCTION_TIME: f64 = 25.0;
/// A real door setup out of the dats, so the scene builds a `SceneObject` and "left drawn" can be
/// measured rather than assumed.
const SETUP: u32 = 0x0200_024F;

/// The subject: a portal standing on the landscape at the origin.
const PORTAL: u32 = 0x7168_0001;
/// A second one, in a different land cell of the same block.
const PORTAL_2: u32 = 0x7168_0002;
/// The one that arrives **after** the teleport, naming the origin the client has already left.
const LATE: u32 = 0x7168_0003;
/// The control that proves the residency refusal is not a blanket refusal: it names the
/// **destination**, which is resident, and must place and stay.
const ARRIVED: u32 = 0x7168_0004;
/// A returned outdoor parent and its authored holding-location child. Their destruction deadlines
/// distinguish a full world re-entry from a parent-only placement retry.
const RETURNED_PARENT: u32 = 0x7168_0010;
const RETURNED_CHILD: u32 = 0x7168_0011;
/// Local identity for the actual `App::frame` short-teleport station.
const PLAYER: u32 = 0x7168_00F0;
/// A physical child of the local body. Its independent destruction deadline proves that local
/// re-entry clears the children's deadlines too rather than only placing the player.
const PLAYER_CHILD: u32 = 0x7168_00F1;
/// Destination player and held item for the same-frame rapid-teleport arrival station.
const RAPID_REMOTE: u32 = 0x7168_0100;
const RAPID_CHILD: u32 = 0x7168_0101;
const HIDDEN: u32 = 0x0040_4410;
const VISIBLE: u32 = 0x0040_0408;
/// Setup `0x0200_0001` carries holding location 1, so a child can be held at location 1.
const HOLDER_SETUP: u32 = 0x0200_0001;

// =================================================================================================
// Harness: `world::landblock_object_release::outdoor_objects`'s, plus the physics sync it does not run.
// =================================================================================================

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above. Not a float conversion.
    LandblockId(((x as u16) << 8) | (y as u16))
}

fn create_body(id: u32, cell: CellId, origin: Vec3) -> Vec<u8> {
    create_body_with_setup(id, cell, origin, SETUP)
}

fn create_body_with_setup(id: u32, cell: CellId, origin: Vec3, setup: u32) -> Vec<u8> {
    dereth_protocol::write_body(&create_message_with_setup(id, cell, origin, setup))
        .expect("encode")
}

fn create_message_with_setup(
    id: u32,
    cell: CellId,
    origin: Vec3,
    setup: u32,
) -> dereth_protocol::objects::ItemCreateObject {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    let bitfield = flags::POSITION | flags::SETUP;
    dereth_protocol::objects::ItemCreateObject(dereth_protocol::objects::ObjectCreatePayload {
        id: ObjectId(id),
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield,
            state: DOOR_OPEN,
            setup_id: Some(setup),
            parent: None,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_protocol::types::Vec3 {
                        x: origin.x,
                        y: origin.y,
                        z: origin.z,
                    },
                    ..dereth_protocol::types::Frame::default()
                },
            }),
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc::default(),
    })
}

struct ReplayPeer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}

impl ReplayPeer {
    fn new() -> (Self, dereth_client_runtime::net::ClientNetwork) {
        let mut net = dereth_client_runtime::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "rapid-teleport-station",
            "unused",
            0,
        )
        .expect("socket-free client net");
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

    fn send<M: Message>(&mut self, app: &mut dereth_client::app::App, message: &M) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
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
                dereth_protocol::write_blob(message).expect("encoded replay message"),
            ))
            .expect("one-fragment packet");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("encrypted replay packet");
        app.replay_network_mut()
            .expect("attached replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("feed replay packet");
    }
}

fn position_event(id: ObjectId, position: Position, instance: u16, stamp: u16) -> SessionEvent {
    SessionEvent::WorldObject {
        opcode: dereth_protocol::Opcode::MOVEMENT_POSITION_EVENT,
        body: dereth_protocol::write_body(&position_message(id, position, instance, stamp))
            .expect("encode player position"),
    }
}

fn position_message(
    id: ObjectId,
    position: Position,
    instance: u16,
    stamp: u16,
) -> dereth_protocol::movement::MovementPositionEvent {
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
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
                objcell_id: position.cell.0,
                origin: dereth_protocol::types::Vec3 {
                    x: position.frame.origin.x,
                    y: position.frame.origin.y,
                    z: position.frame.origin.z,
                },
            },
            instance_timestamp: instance,
            position_timestamp: stamp,
            teleport_timestamp: stamp,
            ..PositionPack::default()
        },
    }
}

fn create_return_pair(cell: CellId, origin: Vec3) -> [Vec<u8>; 2] {
    use dereth_protocol::types::{
        physicsdesc::{flags, ChildLink},
        ObjDesc, PhysicsDesc, PublicWeenieDesc,
    };

    let child = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(RETURNED_CHILD),
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP,
                setup_id: Some(HOLDER_SETUP),
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode returned child");
    let parent = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(RETURNED_PARENT),
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP | flags::CHILDREN,
                state: DOOR_OPEN,
                setup_id: Some(HOLDER_SETUP),
                children: Some(vec![ChildLink {
                    child_id: ObjectId(RETURNED_CHILD),
                    location_id: 1,
                }]),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: dereth_protocol::types::Vec3 {
                            x: origin.x,
                            y: origin.y,
                            z: origin.z,
                        },
                        ..dereth_protocol::types::Frame::default()
                    },
                }),
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode returned parent");
    [child, parent]
}

fn create_held_child(id: u32, parent: u32) -> Vec<u8> {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(id),
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::PARENT,
                setup_id: Some(HOLDER_SETUP),
                parent: Some((ObjectId(parent), 1)),
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode held child")
}

fn feed(s: &mut ObjectStream, body: Vec<u8>) {
    s.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(0.0),
    );
}

/// The object's `PhysicsPresence::cell`, or `None` when it has no cell; `in_table` says whether
/// it is in the table at all, so "retains a table entry" and "retains a cell" can be told apart.
fn cell_of(s: &ObjectStream, id: u32) -> Option<CellId> {
    s.world.physics(ObjectId(id)).and_then(|p| p.cell)
}

fn in_table(s: &ObjectStream, id: u32) -> bool {
    s.world.physics(ObjectId(id)).is_some()
}

fn doomed(s: &ObjectStream, id: u32) -> bool {
    s.world.tables.doomed.contains_key(ObjectId(id))
}

fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let cfg = SceneConfig {
        release_interiors: true,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// `App::sync_objects`' own `self.objects.sync_physics_at(..)` step, which runs
/// `ObjectPhysics::sync` and therefore placement.
///
/// **`WorldScene::sync_objects` does not hold the physics world**, so a scene-level harness that
/// omits this call measures a placement that never happened: every object keeps the cell the
/// **wire** named and the residency refusal under test is never reached.
/// `world::landblock_object_release::outdoor_objects` omits it because its subject is the release itself, which is
/// `dereth_client_model`'s; this module needs the placement half as well.
fn sync_physics(
    scene: &mut WorldScene,
    store: &Arc<RetailDatStore>,
    stream: &mut ObjectStream,
    t: f64,
) {
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics_at(store, &mut c.world, LocalTime(t));
    }
}

/// Put the body in `block` and let the window, the streamer and the object stream settle.
///
/// The body move is `Character::teleport`, the teleport's plain position update.
/// `follow_character_now` plus `update` stands for the accepted player position update that moves
/// the landscape window according to its teleport flag.
fn go_to(
    scene: &mut WorldScene,
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    stream: &mut ObjectStream,
    block: LandblockId,
    t: f64,
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
    settle(scene, store, gpu, stream, t);
}

fn settle(
    scene: &mut WorldScene,
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    stream: &mut ObjectStream,
    t: f64,
) {
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("the streamed blocks build");
    scene
        .sync_objects(store, gpu, stream)
        .expect("the objects synchronise");
    sync_physics(scene, store, stream, t);
}

/// A point on the terrain of `block`, so the create is a placement that can succeed rather than a
/// burial the placement clamp has to lift (the lift is unbounded, but a control that has to
/// exercise it is a worse control).
fn on_terrain(scene: WorldSceneRef<'_>, block: LandblockId, x: f32, y: f32) -> (CellId, Vec3) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let z = land.ground_height(block, x, y).unwrap_or(0.0) + 0.5;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: bounded to 0..8 by the callers' coordinates, which are inside one block.
    let index = ((x / 24.0) as u16) * 8 + ((y / 24.0) as u16) + 1;
    (block.cell(index), Vec3::new(x, y, z))
}

// =================================================================================================
// A long teleport
// =================================================================================================

/// Behaviour: world.teleport.a-teleport-releases-the-whole-origin-window-including-late-arrivals
///
/// **After a teleport across a region-scale distance, nothing of the origin survives, including
/// what the server says about it afterwards.**
///
/// Driven through `WorldScene::update` -> `queue` -> `release_block_interiors` -> `sync_objects`
/// -> `ObjectStream::release_block_obj_cells_with_physics`, plus `App`'s own `sync_physics_at`,
/// which is the production pair. A create naming the destination is the control that the
/// refusal is about residency and not a blanket one.
#[test]
fn a_teleport_leaves_nothing_of_the_origin_behind_not_even_what_arrives_after_it() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home = block_at(hx, hy);
    // Far enough that no slot of the 7-wide window survives: the landscape update takes its
    // full-reload arm when `|dx| >= mid_width`.
    //
    // **Twenty blocks and not forty**: at `(hx + 40, hy + 40)` the shipped terrain is block
    // `0xD1DC`, whose height is `-0.0` at every probe (open ocean), and walkable-position
    // validation refuses a control object standing on the sea for a reason unrelated to this
    // test. `0xBDC8` is hill country at 186..198 m.
    let away = block_at(hx + 20, hy + 20);

    let mut stream = ObjectStream::new();
    let mut scene = embodied(&store, &mut gpu);
    go_to(&mut scene, &store, &mut gpu, &mut stream, home, 0.0);

    let (c1, p1) = on_terrain(scene.view(), home, 96.0, 96.0);
    let (c2, p2) = on_terrain(scene.view(), home, 24.0, 24.0);
    feed(&mut stream, create_body(PORTAL, c1, p1));
    feed(&mut stream, create_body(PORTAL_2, c2, p2));
    settle(&mut scene, &store, &mut gpu, &mut stream, 0.5);

    // --- the premise, at home ---------------------------------------------------------------
    for id in [PORTAL, PORTAL_2] {
        assert_eq!(
            cell_of(&stream, id).map(CellId::landblock),
            Some(home),
            "the premise: {id:#X} is standing in the origin landblock to begin with"
        );
        assert!(
            !doomed(&stream, id),
            "and nothing is queued for destruction yet"
        );
        assert!(
            scene.server_object_frame(ObjectId(id)).is_some(),
            "and the scene is drawing it -- 'left drawn' needs a 'was drawn' to mean anything"
        );
    }
    stream.world.update_visible_object_list();
    assert!(
        stream.world.tables.visible.contains(&ObjectId(PORTAL)),
        "and updating the visible-object list has it in the visible table"
    );

    // --- the teleport ------------------------------------------------------------------------
    go_to(&mut scene, &store, &mut gpu, &mut stream, away, 1.0);
    assert!(
        scene.draw.stats.blocks_released > 0,
        "the premise: the teleport released blocks"
    );
    assert!(
        stream.stats.blocks_flushed > 0,
        "and the object half was reached at all"
    );

    for id in [PORTAL, PORTAL_2] {
        assert_eq!(
            cell_of(&stream, id),
            None,
            "{id:#X} kept its cell across the teleport -- the landscape loop must apply per-slot \
             landblock release across the complete block array"
        );
        assert!(
            doomed(&stream, id),
            "leaving visibility must queue destruction for now + 25 s"
        );
    }
    stream.world.update_visible_object_list();
    assert!(
        !stream.world.tables.visible.contains(&ObjectId(PORTAL)),
        "and the 1 Hz sweep now leaves it out"
    );

    // --- traffic that arrives AFTER the origin has gone -----------------------------------------
    //
    // A create names the origin landblock, and a control names the destination. Retail refuses
    // the first because the block is no longer in the landscape array and queues it for
    // destruction; it accepts the second because that block IS resident.
    let (c3, p3) = on_terrain(scene.view(), home, 60.0, 60.0);
    let (c4, p4) = on_terrain(scene.view(), away, 96.0, 120.0);
    feed(&mut stream, create_body(LATE, c3, p3));
    feed(&mut stream, create_body(ARRIVED, c4, p4));
    settle(&mut scene, &store, &mut gpu, &mut stream, 2.0);

    assert_eq!(
        cell_of(&stream, LATE),
        None,
        "a create for a landblock the client has already left took a cell anyway. \
         Visible-cell lookup goes through outdoor land-cell and landscape lookup, and landscape \
         lookup must refuse a block that is not loaded rather than read it off disk"
    );
    assert!(
        doomed(&stream, LATE),
        "and a create that found no cell must join the destruction queue; otherwise this \
         object is drawn at a frame nothing updates for the rest of the session"
    );
    assert_eq!(
        cell_of(&stream, ARRIVED).map(CellId::landblock),
        Some(away),
        "the control: a create for a RESIDENT block still places. Without this the refusal above \
         could be a blanket one and every object would die 25 s after it arrived."
    );
    assert!(
        !doomed(&stream, ARRIVED),
        "and a placed object is not queued"
    );

    // --- and 25 s later the origin is gone from the tables and from the screen -----------------
    let dead = 2.0 + DESTRUCTION_TIME + 5.0;
    stream.use_time::<dereth_client_net::client_session::testing::MockTransport>(
        dereth_primitives::ServerTime(dead),
        None,
    );
    settle(&mut scene, &store, &mut gpu, &mut stream, dead);
    for id in [PORTAL, PORTAL_2, LATE] {
        assert!(
            !in_table(&stream, id),
            "{id:#X} still has a table entry {dead:.0} s after the teleport"
        );
        assert!(
            scene.server_object_frame(ObjectId(id)).is_none(),
            "{id:#X} is still on screen {dead:.0} s after the teleport -- a stale object \
             floating where the origin used to be"
        );
    }
    assert!(
        in_table(&stream, ARRIVED),
        "and the destination's own object is untouched"
    );
    assert!(
        scene.server_object_frame(ObjectId(ARRIVED)).is_some(),
        "and still drawn"
    );
}

/// **A ring shift releases the departed block's objects with the placement running too.**
///
/// `world::landblock_object_release::outdoor_objects` drives the release without `sync_physics_at`, so it measures
/// `dereth_client_model`'s half only. This is the same scroll with `ObjectPhysics::sync` in the
/// loop, which could otherwise place the object back, plus the same resident-block control.
#[test]
fn the_ring_shift_still_releases_and_a_resident_block_still_places() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home = block_at(hx, hy);
    // Five blocks: outside the 7-wide window in both axes, but inside `|dx| < mid_width`, so this
    // is `update_block`'s **scroll**, not its full reload.
    let next = block_at(hx + 5, hy + 5);

    let mut stream = ObjectStream::new();
    let mut scene = embodied(&store, &mut gpu);
    go_to(&mut scene, &store, &mut gpu, &mut stream, home, 0.0);
    let (c1, p1) = on_terrain(scene.view(), home, 96.0, 96.0);
    feed(&mut stream, create_body(PORTAL, c1, p1));
    settle(&mut scene, &store, &mut gpu, &mut stream, 0.5);
    assert_eq!(
        cell_of(&stream, PORTAL).map(CellId::landblock),
        Some(home),
        "the premise"
    );

    go_to(&mut scene, &store, &mut gpu, &mut stream, next, 1.0);
    assert_eq!(
        cell_of(&stream, PORTAL),
        None,
        "ring shift: the departed block's object left"
    );
    assert!(
        doomed(&stream, PORTAL),
        "and it is on the destruction queue"
    );

    // The control, at the new station: an object created in a block that IS resident places.
    let (c2, p2) = on_terrain(scene.view(), next, 48.0, 48.0);
    feed(&mut stream, create_body(ARRIVED, c2, p2));
    settle(&mut scene, &store, &mut gpu, &mut stream, 1.5);
    assert_eq!(cell_of(&stream, ARRIVED).map(CellId::landblock), Some(next));
    assert!(!doomed(&stream, ARRIVED));
}

/// Behaviour: world.landblock-release.a-returning-parent-rescues-its-held-child
///
/// A released outdoor object returning with its landblock takes the complete cell
/// initialization, visibility re-entry, and world-entry cleanup, including its children.
///
/// The ordinary outdoor placement retry already restores the parent's body. The discriminator is
/// everything after that apparent success: re-entering the world removes the lost-cell entry and
/// the destruction deadline for both the parent and every physical child. A parent-only
/// cancellation looks correct until the original 25-second deadline deletes its held child.
#[test]
fn an_outdoor_parent_returning_with_its_block_rescues_its_held_child_from_the_old_deadline() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home = block_at(hx, hy);
    let away = block_at(hx + 5, hy + 5);
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    let mut scene = embodied(&store, &mut gpu);
    go_to(&mut scene, &store, &mut gpu, &mut stream, home, 0.0);

    let (cell, origin) = on_terrain(scene.view(), home, 96.0, 96.0);
    for body in create_return_pair(cell, origin) {
        feed(&mut stream, body);
    }
    settle(&mut scene, &store, &mut gpu, &mut stream, 0.5);
    let handle = stream
        .physics
        .handle(ObjectId(RETURNED_PARENT))
        .expect("physical parent");
    let placed_cell = cell_of(&stream, RETURNED_PARENT).expect("the parent placed in a real cell");
    assert_eq!(placed_cell.landblock(), home);
    assert!(
        placed_cell.is_outdoor(),
        "the return path under test is an outdoor land cell"
    );
    assert_eq!(
        stream
            .presence(ObjectId(RETURNED_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(RETURNED_PARENT), 1)),
        "the encoded holding-location relation is live before departure"
    );

    go_to(&mut scene, &store, &mut gpu, &mut stream, away, 1.0);
    assert_eq!(
        cell_of(&stream, RETURNED_PARENT),
        None,
        "the outdoor parent left with the block"
    );
    assert!(doomed(&stream, RETURNED_PARENT));
    assert!(
        doomed(&stream, RETURNED_CHILD),
        "leaving visibility queues every child"
    );
    assert!(
        stream
            .world
            .tables
            .lost_cells
            .get(placed_cell)
            .is_some_and(|lost| lost.objects.contains(&ObjectId(RETURNED_PARENT))),
        "lost-cell handling parked the parent under its resolved outdoor cell"
    );

    // Return before the original deadline and send no new object-position message. Loading the
    // landblock itself drives cell initialization.
    go_to(&mut scene, &store, &mut gpu, &mut stream, home, 2.0);
    assert_eq!(
        stream.physics.handle(ObjectId(RETURNED_PARENT)),
        Some(handle),
        "re-entry preserves the surviving physical body"
    );
    assert_eq!(
        cell_of(&stream, RETURNED_PARENT).map(CellId::landblock),
        Some(home)
    );
    assert_eq!(
        stream
            .presence(ObjectId(RETURNED_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(RETURNED_PARENT), 1)),
        "the held child remains attached after its parent re-enters"
    );
    assert!(
        stream
            .world
            .tables
            .lost_cells
            .iter()
            .all(|(_, lost)| !lost.objects.contains(&ObjectId(RETURNED_PARENT))),
        "world re-entry consumes the returned parent's lost-cell membership"
    );
    assert!(
        !doomed(&stream, RETURNED_PARENT),
        "the returned parent leaves the destruction queue"
    );
    assert!(
        !doomed(&stream, RETURNED_CHILD),
        "the returned parent's child leaves it too"
    );

    stream.world.tables.visible.clear();
    stream.use_time::<dereth_client_net::client_session::testing::MockTransport>(
        ServerTime(27.001),
        None,
    );
    assert!(
        in_table(&stream, RETURNED_PARENT),
        "the returned parent survives the old deadline"
    );
    assert!(
        in_table(&stream, RETURNED_CHILD),
        "its held child survives the old deadline"
    );

    // A resident parent with no lost/re-entry edge must not become a general child-doom clearer.
    stream
        .world
        .schedule_destroy(ObjectId(RETURNED_CHILD), ServerTime(28.0));
    settle(&mut scene, &store, &mut gpu, &mut stream, 28.1);
    assert!(
        doomed(&stream, RETURNED_CHILD),
        "an ordinary resident sync does not cancel an independently scheduled child deadline"
    );
}

/// The accepted player-position edge, not a scene-test teleport helper, releases the whole
/// landscape when it crosses even one land cell inside the same resident block.
///
/// Position change has an exact-cell fast path, so the first accepted teleport is the control: it
/// changes the frame inside the current cell and releases nothing. The second changes only the
/// outdoor cell index. Both the old and destination landblocks remain inside the same 3x3 window,
/// which makes any release in that arm attributable to the teleport flag rather than the ordinary
/// ring shift. A third, with the body out of its cell, shows the fast path needs a live cell.
#[test]
fn an_actual_short_player_teleport_releases_and_rebuilds_the_whole_window() {
    use dereth_client::app::App;
    use dereth_client_runtime::character::ALUVIAN_MALE_SETUP;
    use dereth_client_runtime::config::Config;

    let mut app = App::new(Config {
        headless: true,
        frames: None,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("headless WARP App");
    app.start_shell().expect("input shell");
    app.load_static_scene(SceneConfig {
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        release_interiors: true,
        ..SceneConfig::default()
    })
    .expect("actual App world");

    let home = {
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        c.position().cell.landblock()
    };
    let (start_cell, start_origin) = on_terrain(app.world_scene().unwrap(), home, 84.0, 84.0);
    let start = Position::new(start_cell, Frame::new(start_origin, Quat::IDENTITY));
    {
        let mut world = app.world_scene_mut().unwrap();
        world.character.as_mut().unwrap().teleport(start);
        world.follow_character_now();
    }

    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::PlayerCreated(ObjectId(PLAYER)),
        LocalTime(0.0),
    );
    feed(
        app.probe_mut().objects_mut(),
        create_body_with_setup(PLAYER, start_cell, start_origin, ALUVIAN_MALE_SETUP.0),
    );
    feed(
        app.probe_mut().objects_mut(),
        create_held_child(PLAYER_CHILD, PLAYER),
    );
    assert!(
        app.frame(),
        "the player identity and initial placement settle"
    );
    assert_eq!(app.objects().player(), Some(ObjectId(PLAYER)));
    assert_eq!(
        app.objects()
            .presence(ObjectId(PLAYER_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(PLAYER), 1)),
        "the local held-child cleanup control is physically authored before the teleport"
    );

    let (object_cell, object_origin) = on_terrain(app.world_scene().unwrap(), home, 60.0, 60.0);
    for body in create_return_pair(object_cell, object_origin) {
        feed(app.probe_mut().objects_mut(), body);
    }
    assert!(
        app.frame(),
        "the parent and held child enter the actual App scene"
    );
    let parent_handle = app
        .objects()
        .physics
        .handle(ObjectId(RETURNED_PARENT))
        .expect("physical parent");
    assert_eq!(cell_of(app.objects(), RETURNED_PARENT), Some(object_cell));
    assert_eq!(
        app.objects()
            .presence(ObjectId(RETURNED_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(RETURNED_PARENT), 1))
    );

    let resident = app.world_scene().unwrap().resident_blocks();
    assert_eq!(
        resident, 9,
        "the 3x3 window is fully resident before either teleport"
    );
    let released = app.world_scene().unwrap().draw.stats.blocks_released;
    let flushed = app.objects().stats.blocks_flushed;
    let instance = app.objects().presence(ObjectId(PLAYER)).unwrap().instance;

    let same_cell = Position::new(
        start_cell,
        Frame::new(
            Vec3::new(start_origin.x + 1.0, start_origin.y, start_origin.z),
            Quat::IDENTITY,
        ),
    );
    app.probe_mut().objects_mut().apply_event(
        &position_event(ObjectId(PLAYER), same_cell, instance, 1),
        LocalTime(1.0),
    );
    assert!(app.frame(), "same-cell accepted teleport");
    assert_eq!(
        app.probe().player_teleports_applied(),
        1,
        "the control really was accepted"
    );
    assert_eq!(
        app.world_scene().unwrap().draw.stats.blocks_released,
        released,
        "the accepted same-cell position-change path must not release the landscape"
    );
    assert_eq!(app.objects().stats.blocks_flushed, flushed);

    let (destination_cell, destination_origin) =
        on_terrain(app.world_scene().unwrap(), home, 108.0, 84.0);
    assert_ne!(
        destination_cell, start_cell,
        "the rejecting arm crosses one land cell"
    );
    assert_eq!(
        destination_cell.landblock(),
        home,
        "but remains inside the same landblock"
    );
    let destination = Position::new(
        destination_cell,
        Frame::new(destination_origin, Quat::IDENTITY),
    );
    app.probe_mut().objects_mut().apply_event(
        &position_event(ObjectId(PLAYER), destination, instance, 2),
        LocalTime(2.0),
    );
    assert!(app.frame(), "different-cell short teleport");
    assert_eq!(app.probe().player_teleports_applied(), 2);
    let world = app.world_scene().unwrap();
    assert_eq!(world.viewer_block(), Some(block_xy(home.0)));
    assert_eq!(
        world.resident_blocks(),
        resident,
        "the same 3x3 destination window rebuilt"
    );
    assert_eq!(
        world.draw.stats.blocks_released - released,
        resident as u64,
        "retail's release of every block visits each resident slot though none left the window"
    );
    assert_eq!(
        app.objects().stats.blocks_flushed - flushed,
        resident as u64,
        "the matching object-cell teardown ran before the rebuilt arrivals"
    );
    assert_eq!(cell_of(app.objects(), RETURNED_PARENT), None);
    assert!(doomed(app.objects(), RETURNED_PARENT));
    assert!(doomed(app.objects(), RETURNED_CHILD));
    assert!(
        doomed(app.objects(), PLAYER),
        "the released local body enters the same destruction queue"
    );
    assert!(
        doomed(app.objects(), PLAYER_CHILD),
        "leaving visibility schedules the local body's held child too"
    );
    assert!(
        app.objects()
            .world
            .tables
            .lost_cells
            .get(destination_cell)
            .is_some_and(|lost| lost.objects.contains(&ObjectId(PLAYER))),
        "the local body is parked under its accepted outdoor cell"
    );
    assert!(
        app.frame(),
        "the queued placement retry runs after the rebuilt cells arrive"
    );
    assert_eq!(
        app.objects().physics.handle(ObjectId(RETURNED_PARENT)),
        Some(parent_handle),
        "the surviving parent re-enters with the same physical body"
    );
    assert_eq!(cell_of(app.objects(), RETURNED_PARENT), Some(object_cell));
    assert_eq!(
        app.objects()
            .presence(ObjectId(RETURNED_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(RETURNED_PARENT), 1)),
        "the authored holding relation survives release and re-entry"
    );
    assert!(!doomed(app.objects(), RETURNED_PARENT));
    assert!(!doomed(app.objects(), RETURNED_CHILD));
    assert!(
        app.objects()
            .world
            .tables
            .lost_cells
            .iter()
            .all(|(_, lost)| !lost.objects.contains(&ObjectId(PLAYER))),
        "local re-entry consumes the actual player lost-cell membership"
    );
    assert!(
        !doomed(app.objects(), PLAYER),
        "the local body leaves the destruction queue"
    );
    assert!(
        !doomed(app.objects(), PLAYER_CHILD),
        "its held child leaves the queue too"
    );

    let before_walk = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
        .frame
        .origin;
    let movement = |start| dereth_input::InputEvent {
        action: dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        input_map: dereth_input::InputMapId(4),
        toggle: dereth_input::ToggleType::Hold,
        extent: 1.0,
        start,
        repeat_delta: 0,
        repeat_total: 0,
        from_key_down: start,
    };
    app.input_manager_mut()
        .expect("input manager")
        .inject_action(movement(true));
    assert!(
        app.frame(),
        "ordinary movement resumes after the release/re-entry cycle"
    );
    assert!(app.probe().char_input().forward);
    app.input_manager_mut()
        .unwrap()
        .inject_action(movement(false));
    assert!(app.frame(), "ordinary movement release");
    assert!(!app.probe().char_input().forward);
    let character = app.world_state().unwrap().character.as_ref().unwrap();
    assert_eq!(
        character
            .world
            .get(character.handle)
            .and_then(|body| body.cell),
        Some(character.position().cell),
        "the local body retains a live physical cell after the landscape reload"
    );
    let after_walk = character.position().frame.origin;
    assert!(
        math::hypotf(after_walk.x - before_walk.x, after_walk.y - before_walk.y) > 0.001,
        "a real forward press must move the returned local body"
    );

    // The same-cell fast path also requires the body to be in a cell now. Model that transient
    // directly: the stored position still names the destination cell, but the physical body has
    // left it. An accepted same-cell packet must now release every block rather than being
    // mistaken for the live-cell control above.
    let released = app.world_scene().unwrap().draw.stats.blocks_released;
    let flushed = app.objects().stats.blocks_flushed;
    {
        let character = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        character.world.leave_cell(character.handle);
    }
    let same_id_without_a_cell = Position::new(
        destination_cell,
        Frame::new(
            Vec3::new(
                destination_origin.x + 1.0,
                destination_origin.y,
                destination_origin.z,
            ),
            Quat::IDENTITY,
        ),
    );
    app.probe_mut().objects_mut().apply_event(
        &position_event(ObjectId(PLAYER), same_id_without_a_cell, instance, 3),
        LocalTime(3.0),
    );
    assert!(
        app.frame(),
        "same-id teleport with no current physical cell"
    );
    assert_eq!(app.probe().player_teleports_applied(), 3);
    assert_eq!(
        app.world_scene().unwrap().draw.stats.blocks_released - released,
        resident as u64,
        "a matching stored cell id does not take the same-cell fast path when the body has no cell"
    );
    assert_eq!(
        app.objects().stats.blocks_flushed - flushed,
        resident as u64
    );
    assert!(doomed(app.objects(), PLAYER));
    assert!(doomed(app.objects(), PLAYER_CHILD));
    assert!(
        app.frame(),
        "the no-current-cell release gets the same ordinary re-entry frame"
    );
    assert!(!doomed(app.objects(), PLAYER));
    assert!(!doomed(app.objects(), PLAYER_CHILD));

    // Advance the model past the deadlines stamped by both full releases. Re-entry must have
    // removed the exact local body and held-child timers, not merely made the player movable.
    app.probe_mut().objects_mut().world.tables.visible.clear();
    app.probe_mut()
        .objects_mut()
        .use_time::<dereth_client_net::client_session::testing::MockTransport>(
            ServerTime(2.0 * DESTRUCTION_TIME + 10.0),
            None,
        );
    assert!(
        in_table(app.objects(), PLAYER),
        "the local body survives the old deadline"
    );
    assert!(
        in_table(app.objects(), PLAYER_CHILD),
        "its held child survives the old deadline"
    );

    app.shutdown();
}

/// Two accepted local teleports and the final destination's remote arrivals can share one
/// received packet batch. Each `Movement_PositionEvent 0xF748` must complete its release before
/// the next blob, while the final `Item_CreateObject 0xF745` / `Item_SetState 0xF74B` arrivals
/// must survive the resulting unloaded-cell interval and re-enter on the next ordinary frame with
/// the authored held link intact.
///
/// This is the production replay path, not `ObjectStream::apply_event`: every message is encoded,
/// admitted by the session, and consumed by `App::deliver_session_events` in one `App::frame`.
#[test]
fn rapid_teleports_preserve_the_final_local_and_remote_arrivals() {
    use dereth_protocol::{
        objects::{ItemCreateObject, ItemSetState, LoginCreatePlayer, ObjectCreatePayload},
        types::{
            physicsdesc::flags, ObjDesc, PhysicsDesc, PhysicsEventStamp, PhysicsTimestamps,
            PublicWeenieDesc,
        },
    };
    use {
        dereth_client::app::App, dereth_client_runtime::character::ALUVIAN_MALE_SETUP,
        dereth_client_runtime::config::Config,
    };

    let mut app = App::new(Config {
        headless: true,
        frames: None,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("headless WARP App");
    app.load_static_scene(SceneConfig {
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        release_interiors: true,
        ..SceneConfig::default()
    })
    .expect("actual App world");

    let home = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
        .cell
        .landblock();
    let (start_cell, start_origin) = on_terrain(app.world_scene().unwrap(), home, 60.0, 60.0);
    let (middle_cell, middle_origin) = on_terrain(app.world_scene().unwrap(), home, 84.0, 60.0);
    let (final_cell, final_origin) = on_terrain(app.world_scene().unwrap(), home, 108.0, 84.0);
    assert_ne!(start_cell, middle_cell);
    assert_ne!(middle_cell, final_cell);
    assert_eq!(final_cell.landblock(), home);

    let start = Position::new(start_cell, Frame::new(start_origin, Quat::IDENTITY));
    {
        let mut scene = app.world_scene_mut().unwrap();
        scene.character.as_mut().unwrap().teleport(start);
        scene.follow_character_now();
    }

    let (mut peer, net) = ReplayPeer::new();
    app.attach_replay_network(net)
        .expect("socket-free replay endpoint");
    peer.send(
        &mut app,
        &LoginCreatePlayer {
            player_id: ObjectId(PLAYER),
        },
    );
    let mut player =
        create_message_with_setup(PLAYER, start_cell, start_origin, ALUVIAN_MALE_SETUP.0);
    player.0.physicsdesc.state = VISIBLE;
    player.0.physicsdesc.timestamps.instance = 1;
    peer.send(&mut app, &player);
    peer.send(
        &mut app,
        &ItemCreateObject(ObjectCreatePayload {
            id: ObjectId(PLAYER_CHILD),
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::PARENT,
                setup_id: Some(HOLDER_SETUP),
                parent: Some((ObjectId(PLAYER), 1)),
                timestamps: PhysicsTimestamps {
                    instance: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            wdesc: PublicWeenieDesc::default(),
        }),
    );
    assert!(app.frame(), "the encoded player and held child arrive");
    assert_eq!(app.objects().player(), Some(ObjectId(PLAYER)));
    assert_eq!(
        app.objects()
            .presence(ObjectId(PLAYER_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(PLAYER), 1))
    );
    assert_eq!(app.world_scene().unwrap().resident_blocks(), 9);

    let first = Position::new(middle_cell, Frame::new(middle_origin, Quat::IDENTITY));
    let second = Position::new(final_cell, Frame::new(final_origin, Quat::IDENTITY));
    peer.send(&mut app, &position_message(ObjectId(PLAYER), first, 1, 1));
    peer.send(&mut app, &position_message(ObjectId(PLAYER), second, 1, 2));

    let mut remote = create_message_with_setup(
        RAPID_REMOTE,
        final_cell,
        Vec3::new(final_origin.x + 2.0, final_origin.y, final_origin.z),
        HOLDER_SETUP,
    );
    remote.0.physicsdesc.state = HIDDEN;
    remote.0.physicsdesc.timestamps.instance = 1;
    peer.send(&mut app, &remote);
    let child = ItemCreateObject(ObjectCreatePayload {
        id: ObjectId(RAPID_CHILD),
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::PARENT,
            setup_id: Some(HOLDER_SETUP),
            parent: Some((ObjectId(RAPID_REMOTE), 1)),
            timestamps: PhysicsTimestamps {
                instance: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    });
    peer.send(&mut app, &child);
    peer.send(
        &mut app,
        &ItemSetState {
            id: ObjectId(RAPID_REMOTE),
            state: VISIBLE,
            timestamps: PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        },
    );

    let released = app.world_scene().unwrap().draw.stats.blocks_released;
    assert!(
        app.frame(),
        "the two teleports and destination arrivals share one received batch"
    );
    assert_eq!(
        app.probe().player_teleports_applied(),
        2,
        "both encoded position messages were admitted"
    );
    assert_eq!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .position()
            .cell,
        final_cell,
        "the second accepted teleport owns the final local position"
    );
    assert_eq!(
        app.world_scene().unwrap().draw.stats.blocks_released - released,
        9,
        "the first release empties the array; the second correctly releases no not-yet-loaded slots"
    );
    assert_eq!(
        app.objects().physics_state(ObjectId(RAPID_REMOTE)),
        Some(VISIBLE)
    );
    assert_eq!(
        app.objects()
            .presence(ObjectId(RAPID_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(RAPID_REMOTE), 1)),
        "the final destination's ordinary parent-first create pair authors the held relation"
    );
    let remote_handle = app
        .objects()
        .physics
        .handle(ObjectId(RAPID_REMOTE))
        .expect("the unloaded-cell arrival still owns one physical instance");
    assert_eq!(
        cell_of(app.objects(), RAPID_REMOTE),
        Some(final_cell),
        "the destination arrival can place during its own event boundary"
    );

    assert!(
        app.frame(),
        "the rebuilt final destination runs the ordinary re-entry consumer"
    );
    let character = app.world_state().unwrap().character.as_ref().unwrap();
    assert_eq!(character.position().cell, final_cell);
    assert_eq!(
        character
            .world
            .get(character.handle)
            .and_then(|body| body.cell),
        Some(final_cell),
        "the same local body re-enters only the last destination"
    );
    assert_eq!(
        app.objects().physics.handle(ObjectId(RAPID_REMOTE)),
        Some(remote_handle),
        "the destination arrival keeps one instance and one physical handle"
    );
    assert_eq!(cell_of(app.objects(), RAPID_REMOTE), Some(final_cell));
    assert!(!doomed(app.objects(), RAPID_REMOTE));
    assert!(!doomed(app.objects(), RAPID_CHILD));
    assert_eq!(
        app.objects()
            .presence(ObjectId(RAPID_CHILD))
            .and_then(|p| p.parent),
        Some((ObjectId(RAPID_REMOTE), 1))
    );
    assert!(
        character
            .world
            .get(remote_handle)
            .is_some_and(|body| body.transient_state.is_active()),
        "the final set-state unhide remains active after re-entry"
    );

    app.probe_mut().objects_mut().world.tables.visible.clear();
    app.probe_mut()
        .objects_mut()
        .use_time::<dereth_client_net::client_session::testing::MockTransport>(
            ServerTime(DESTRUCTION_TIME + 10.0),
            None,
        );
    for id in [PLAYER, PLAYER_CHILD, RAPID_REMOTE, RAPID_CHILD] {
        assert!(
            in_table(app.objects(), id),
            "{id:#X} survives the old release deadline"
        );
    }

    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// Objects in a departed block's outdoor cells are released
// ---------------------------------------------------------------------------------------------

mod outdoor_objects {
    //! Objects standing in outdoor cells are released when their block leaves the streaming window.
    //! Scrolling a block off the window releases its land-cell objects, then its visible interior
    //! cells, then the block itself. The land-cell step (a block in load state 9) walks every cell of
    //! the block's grid; each cell release removes its shadows and hands its objects to maintenance,
    //! which skips static objects (state bit 0) and parented objects and takes the rest out of
    //! visibility with a 25 s destruction deadline. Outdoor and interior releases share that filter,
    //! and static and interior controls tell the land-cell walk apart from a replaced interior half.
    //! Retail also drops each shadow's draw parts at once; our outdoor pass has no per-cell draw list,
    //! so the scene object here survives until the queued deletion (checked as stored frames, not
    //! pixels). Fixture: the retail dats on a software device, Holtburg; no datagram leaves.
    use super::{
        block_at, doomed, embodied, feed, store, DESTRUCTION_TIME, DOOR_OPEN, PORTAL, PORTAL_2,
        SETUP,
    };
    use dereth_scene::world_scene::SceneWrites;
    use std::sync::Arc;

    use dereth_client_runtime::character::CharacterInput;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{
        CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
    };
    use dereth_render::device::Gpu;
    use {
        dereth_client_runtime::landblock::block_xy,
        dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_scene::world_scene::WorldScene,
    };

    /// STATIC_PS is state bit 0, the shared release filter's static-object exclusion.
    const STATIC_PS: u32 = 0x0000_0001;

    /// An outdoor cell index. `CellId::is_outdoor` is `0x01..=0x40`.
    const OUTDOOR: u16 = 0x0001;
    /// A second one, so "the land half walked more than the first cell" is measurable.
    const OUTDOOR_2: u16 = 0x0021;
    /// An interior cell index, for the control that separates the land-cell and interior halves of
    /// the block release.
    const INTERIOR: u16 = 0x0100;

    /// Outdoor static-state control: the shared release filter must retain it.
    const STATIC_OUTDOOR: u32 = 0x7168_0003;
    /// The interior control: it must leave as well, or the land-cell half has replaced the interior
    /// half instead of running beside it.
    const INDOORS: u32 = 0x7168_0004;

    // =================================================================================================
    // Harness: `world::landblock_interior_release::interior_cell_objects`'s, with the block-release counters checked
    // =================================================================================================

    fn create_body(id: u32, state: u32, cell: CellId) -> Vec<u8> {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
        let bitfield = flags::POSITION | flags::SETUP;
        dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: ObjectId(id),
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield,
                    state,
                    setup_id: Some(SETUP),
                    parent: None,
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: cell.0,
                        frame: dereth_protocol::types::Frame::default(),
                    }),
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode")
    }

    fn populate(block: LandblockId) -> ObjectStream {
        let mut s = ObjectStream::new();
        feed(&mut s, create_body(PORTAL, DOOR_OPEN, block.cell(OUTDOOR)));
        feed(
            &mut s,
            create_body(PORTAL_2, DOOR_OPEN, block.cell(OUTDOOR_2)),
        );
        feed(
            &mut s,
            create_body(STATIC_OUTDOOR, DOOR_OPEN | STATIC_PS, block.cell(OUTDOOR)),
        );
        feed(
            &mut s,
            create_body(INDOORS, DOOR_OPEN, block.cell(INTERIOR)),
        );
        s
    }

    fn cell_of(s: &ObjectStream, id: u32) -> Option<CellId> {
        s.world
            .physics(ObjectId(id))
            .expect("the create reached the game table")
            .cell
    }

    fn go_to(
        scene: &mut WorldScene,
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        stream: &mut ObjectStream,
        block: LandblockId,
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
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(0.0),
            0.0,
        );
        scene.stream(store, gpu).expect("the streamed blocks build");
        scene
            .sync_objects(store, gpu, stream)
            .expect("the objects synchronise");
    }

    // =================================================================================================
    // The ring-shift release
    // =================================================================================================

    /// Behaviour: world.landblock-release.objects-in-a-departed-blocks-outdoor-cells-are-released
    ///
    /// An object on the terrain of a block that scrolls out of the window loses its cell, leaves
    /// visibility and is deleted after the destruction deadline; a static object stays.
    ///
    /// Drives production streaming and object synchronization after moving the local body across
    /// blocks: `WorldScene::update`, `queue`/`release_block_interiors` and `sync_objects` reach
    /// `ObjectStream::release_block_obj_cells_with_physics`. This is direct scene/event driving, not
    /// encrypted packet replay or pixel rendering. Cell membership, visible-table admission and the
    /// scene representation are asserted before the release, so absence cannot pass vacuously.
    #[test]
    fn a_portal_on_the_terrain_of_a_departed_block_is_released_rather_than_left_drawn() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);

        let home = LandblockId(DEFAULT_LANDBLOCK);
        let outside = home.cell(OUTDOOR);
        let mut stream = populate(home);
        let mut scene = embodied(&store, &mut gpu);

        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        go_to(&mut scene, &store, &mut gpu, &mut stream, block_at(hx, hy));

        // --- the premise, at home -------------------------------------------------------------
        assert_eq!(
            cell_of(&stream, PORTAL),
            Some(outside),
            "the portal starts in an outdoor cell"
        );
        assert_eq!(
            cell_of(&stream, PORTAL_2),
            Some(home.cell(OUTDOOR_2)),
            "and so does the second"
        );
        assert_eq!(
            cell_of(&stream, STATIC_OUTDOOR),
            Some(outside),
            "and the static control"
        );
        assert_eq!(
            cell_of(&stream, INDOORS),
            Some(home.cell(INTERIOR)),
            "the interior control"
        );
        assert_eq!(
            stream.stats.objects_left_visibility, 0,
            "nothing has left yet"
        );
        stream.world.update_visible_object_list();
        assert!(
            stream.world.tables.visible.contains(&ObjectId(PORTAL)),
            "the premise: the portal IS in the visible table to begin with"
        );
        assert!(
            scene.server_object_frame(ObjectId(PORTAL)).is_some(),
            "and the scene represents it before release"
        );

        // Move five blocks in both axes, then assert that streaming actually released blocks.
        go_to(
            &mut scene,
            &store,
            &mut gpu,
            &mut stream,
            block_at(hx + 5, hy + 5),
        );
        assert!(
            scene.draw.stats.blocks_released > 0,
            "the premise: the scroll released blocks"
        );
        assert!(
            stream.stats.blocks_flushed > 0,
            "and the object half was reached at all"
        );

        // 1. Check two different outdoor cells in the released block.
        assert_eq!(
            cell_of(&stream, PORTAL),
            None,
            "the object on the departed block's terrain loses its cell"
        );
        assert_eq!(
            cell_of(&stream, PORTAL_2),
            None,
            "and an object in another land cell of the same block also loses its cell"
        );
        assert!(
            doomed(&stream, PORTAL),
            "the released object has a destruction deadline"
        );
        assert!(doomed(&stream, PORTAL_2));
        assert_eq!(
            stream
                .world
                .tables
                .lost_cells
                .get(outside)
                .map(|c| c.objects.clone()),
            Some(vec![ObjectId(PORTAL)]),
            "the object is parked under its former cell for possible re-entry"
        );
        stream.world.update_visible_object_list();
        assert!(
            !stream.world.tables.visible.contains(&ObjectId(PORTAL)),
            "and the 1 Hz sweep now leaves it out"
        );

        // 2. Observe the retained scene representation before the queued deletion.
        //
        // Leaving visibility schedules destruction at now + 25.0. Once time passes that deadline,
        // object deletion propagates to `WorldScene::objects`. Before then the scene keeps the
        // representation, which `server_object_frame` can still find.
        //
        // Retail's cell release removes each shadow's parts from that cell's draw list at once.
        // `ObjectPhase::Outdoors` submits from `WorldScene::objects` with no per-cell list, so this
        // is a known difference, not retail pixels remaining for 25 s. This station checks stored
        // frames and eventual deletion, not draw submissions.
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(1.0),
            1.0 / 30.0,
        );
        assert!(
            scene.server_object_frame(ObjectId(PORTAL)).is_some(),
            "the scene representation remains immediately after release"
        );

        // --- 3. the shared filter still bites, and the interior half still runs ------------------
        assert_eq!(
            cell_of(&stream, STATIC_OUTDOOR),
            Some(outside),
            "the shared static-state filter retains the outdoor control's cell"
        );
        assert!(
            !doomed(&stream, STATIC_OUTDOOR),
            "and a refused object is not scheduled either"
        );
        assert_eq!(
            cell_of(&stream, INDOORS),
            None,
            "and the environment-cell visible-release half still runs: the block release is both \
         halves, not one swapped for the other"
        );

        // --- 4. the denominator -----------------------------------------------------------------
        assert_eq!(
            stream.stats.objects_left_visibility, 3,
            "exactly three of the four: PORTAL, PORTAL_2 and INDOORS. STATIC_OUTDOOR stays."
        );

        // 5. Advance to 25 s plus 5 s slack; require both non-static scene objects removed.
        let dead = DESTRUCTION_TIME + 5.0;
        stream.use_time::<dereth_client_net::client_session::testing::MockTransport>(
            dereth_primitives::ServerTime(dead),
            None,
        );
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("the removal drains");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(dead),
            1.0 / 30.0,
        );
        assert!(
            scene.server_object_frame(ObjectId(PORTAL)).is_none(),
            "{dead:.0} s after block departure, the portal still has a scene representation"
        );
        assert!(
            scene.server_object_frame(ObjectId(PORTAL_2)).is_none(),
            "and neither is the second"
        );
        assert!(
            scene
                .server_object_frame(ObjectId(STATIC_OUTDOOR))
                .is_some(),
            "the static control retains its scene representation"
        );
    }
}
