//! ACE: Source/ACE.Server/WorldObjects/Player_Move.cs::MoveTo
//! MoveToState/AutonomousPosition move the player; speed-hack position rejected; second client
//! sees UpdateMotion/UpdatePosition; far teleport through portal space; portal use; lifestone
//! recall.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::actions::pack_action_raw;
use dereth_protocol::comms::CommunicationTextboxString;
use dereth_protocol::login::{
    CharacterLoginCompleteNotification, LoginCharacterSet, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use dereth_protocol::movement::{
    AutonomousPosition, MoveTimestamps, MoveToStatePack, MovementAutonomousPosition, MovementBody,
    MovementMoveToState, MovementPositionEvent, MovementSetObjectMovement, RawMotionState,
};
use dereth_protocol::objects::EffectsPlayerTeleport;
use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};
use dereth_protocol::Opcode;
use empyrean_common::dotnet::DotNetDict;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, ClientId, ClientStatus, TestServer};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::player_manager;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{player_location, player_move, portal};

const HOME: u16 = 0xA9B4;
/// A landblock far from `HOME` (the teleport, portal and recall destination).
const FAR: u16 = 0x2B2C;
const BLOCKS: [u16; 11] = [
    0xA8B3, 0xA8B4, 0xA8B5, 0xA9B3, 0xA9B4, 0xA9B5, 0xAAB3, 0xAAB4, 0xAAB5, 0xACB4, FAR,
];

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;
const PORTAL: u32 = 0x7000_0001;

/// `MotionCommand.RunForward` and `HoldKey.Run`.
const RUN_FORWARD: u32 = 0x4400_0007;
const HOLD_RUN: u32 = 2;

const UPDATE_MOTION: u32 = 0xF74C;
const UPDATE_POSITION: u32 = 0xF748;
const PLAYER_TELEPORT: u32 = 0xF751;
const SYSTEM_CHAT: u32 = 0xF7E0;
const GAME_EVENT: u32 = 0xF7B0;
const EV_WEENIE_ERROR: u32 = 0x028A;
/// `WeenieError.ITeleported`.
const I_TELEPORTED: u32 = 60;

/// An outdoor position: the landblock, the cell under `(x, y)` and an identity rotation.
fn outdoor(lb: u16, x: f32, y: f32, z: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    Position::from_components(
        u32::from(lb) << 16 | cell,
        x,
        y,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

fn wire(p: &Position) -> PositionWire {
    PositionWire {
        objcell_id: p.cell(),
        frame: Frame {
            origin: Vec3 {
                x: p.position_x,
                y: p.position_y,
                z: p.position_z,
            },
            orientation: Quat {
                w: p.rotation_w,
                x: p.rotation_x,
                y: p.rotation_y,
                z: p.rotation_z,
            },
        },
    }
}

/// Fake dats with the retail-shaped secondary attribute table (max mana = Self, and so on).
fn dats() -> std::sync::Arc<empyrean_dat::DatManager> {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    let table = SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .build()
        .expect("fake dats")
}

/// A server on flat land (20 m high) over `BLOCKS`, with the synthetic test setup.
fn server() -> TestServer {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &BLOCKS, 10);
    ts
}

/// An account with one character whose biota stands at `at`: the synthetic body, all attributes
/// 100, every vital 100 of 100 (with `with_stat_tables`' zero formulas), collisions reported.
fn seed(ts: &TestServer, account: &str, guid: u32, name: &str, at: Position) {
    let account_id = ts
        .auth()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    biota.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    biota.set_property(PropertyBool::ReportCollisions, true);
    biota.set_property(PropertyBool::IgnoreCollisions, false);
    let attributes = biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    for a in &PropertyAttribute::ALL[1..] {
        attributes.insert(
            *a,
            PropertiesAttribute {
                init_level: 100,
                ..PropertiesAttribute::default()
            },
        );
    }
    let vitals = biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        vitals.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                current_level: 100,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    let position = PropertiesPosition {
        obj_cell_id: at.cell(),
        position_x: at.position_x,
        position_y: at.position_y,
        position_z: at.position_z,
        rotation_w: at.rotation_w,
        rotation_x: at.rotation_x,
        rotation_y: at.rotation_y,
        rotation_z: at.rotation_z,
    };
    biota
        .properties_position
        .get_or_insert_with(DotNetDict::new)
        .insert(PositionType::Location, position);
    let character = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
}

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

/// A server with the accounts `alpha` (at `HOME` 100, 100) and `bravo` (4 m east), one
/// character each, `PlayerManager` loaded.
fn seeded() -> TestServer {
    let mut ts = server();
    ts.world.content = Arc::new(MemContent::new().weenie(
        Weenie::new(1, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ),
    ));
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    seed(
        &ts,
        "alpha",
        ALPHA,
        "Alpha",
        outdoor(HOME, 100.0, 100.0, 20.0),
    );
    seed(
        &ts,
        "bravo",
        BRAVO,
        "Bravo",
        outdoor(HOME, 104.0, 100.0, 20.0),
    );
    player_manager::initialize(&mut ts.world);
    ts
}

fn enter(ts: &mut TestServer, account: &str, guid: u32, at: Position) -> (ClientId, SessionId) {
    let id = ts.connect(account, "pw");
    assert_eq!(
        ts.client(id).status(),
        ClientStatus::Connected,
        "{account} logs in"
    );
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list arrives"
    );
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(guid),
            account: account.to_owned(),
        },
    );
    let g = ObjectGuid::new(guid);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "{account} enters the world"
    );
    let session = ts.world.net.find_by_account(account).expect("a session");
    let l = location(ts, guid);
    assert_eq!(
        (l.cell(), l.position_x, l.position_y),
        (at.cell(), at.position_x, at.position_y),
        "at its saved location"
    );

    // the client's LoginComplete: out of the login pink bubble
    ts.send_game_action(id, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    assert!(
        !phys_ext::get_physics_state(&ts.world, g, empyrean_entity::enums::PhysicsState::Hidden),
        "LoginComplete: out of the pink bubble"
    );
    let _ = TestServer::take_not_ported();
    (id, session)
}

/// The opcodes a client received from index `mark` on; a game event is `0xF7B0_0000 | event`.
fn opcodes_since(ts: &TestServer, id: ClientId, mark: usize) -> Vec<u32> {
    ts.received_raw(id)[mark..]
        .iter()
        .map(|m| {
            if m.opcode == GAME_EVENT {
                0xF7B0_0000 | u32::from_le_bytes(m.body[8..12].try_into().expect("event type"))
            } else {
                m.opcode
            }
        })
        .collect()
}

/// The movement opcodes only (motion, position, teleport).
fn movement_since(ts: &TestServer, id: ClientId, mark: usize) -> Vec<u32> {
    opcodes_since(ts, id, mark)
        .into_iter()
        .filter(|op| [UPDATE_MOTION, UPDATE_POSITION, PLAYER_TELEPORT].contains(op))
        .collect()
}

fn location(ts: &TestServer, guid: u32) -> Position {
    ts.world
        .objects
        .get(ObjectGuid::new(guid))
        .and_then(WorldObject::location)
        .expect("a location")
}

fn run_forward(at: &Position) -> MovementMoveToState {
    MovementMoveToState(MoveToStatePack {
        raw_motion_state: RawMotionState {
            current_holdkey: Some(HOLD_RUN),
            forward_command: Some(RUN_FORWARD),
            forward_holdkey: Some(HOLD_RUN),
            ..RawMotionState::default()
        },
        position: wire(at),
        timestamps: MoveTimestamps::default(),
        contact: true,
        longjump_mode: false,
    })
}

fn autonomous_position(at: &Position, contact: bool) -> MovementAutonomousPosition {
    MovementAutonomousPosition(AutonomousPosition {
        position: wire(at),
        timestamps: MoveTimestamps::default(),
        contact: u8::from(contact),
    })
}

/// The movement a `0xF74C` carries: its interpreted forward command (the wire's 16-bit command
/// index, the command's low word).
fn forward_command(m: &MovementSetObjectMovement) -> Option<u32> {
    let body: MovementBody = m
        .decoded_movement()
        .expect("the movement buffer decodes")
        .body;
    body.interpreted
        .expect("an interpreted state")
        .forward_command
        .map(u32::from)
}

// ---- MoveToState and AutonomousPosition ---------------------------------------------------------

/// A MoveToState "run forward": the player's movement is broadcast (to itself too) as an
/// UpdateMotion carrying RunForward, and the position it carries is requested without a
/// broadcast; the next physics tick moves the player there and sends the UpdatePosition. Then an
/// AutonomousPosition 4 m ahead is accepted: the location and the body move, the contact records
/// the ground position, and the UpdatePosition carries the new origin.
#[test]
fn a_run_forward_move_to_state_and_autonomous_positions_move_the_player() {
    let mut ts = seeded();
    let start = outdoor(HOME, 100.0, 100.0, 20.0);
    let (a, _) = enter(&mut ts, "alpha", ALPHA, start);
    let g = ObjectGuid::new(ALPHA);

    let mark = ts.received_raw(a).len();
    ts.send_game_action(a, &run_forward(&start));
    ts.advance(0.2);
    assert_eq!(
        movement_since(&ts, a, mark),
        [UPDATE_MOTION, UPDATE_POSITION],
        "UpdateMotion from the handler, UpdatePosition from the tick"
    );
    let motion = ts.received::<MovementSetObjectMovement>(a);
    let last = motion.last().expect("an UpdateMotion");
    assert_eq!(last.id, ObjectId(ALPHA));
    assert_eq!(forward_command(last), Some(RUN_FORWARD & 0xFFFF));
    let f = player_move::fields(&ts.world, g);
    assert!(f.last_move_to_state.is_some(), "LastMoveToState");
    assert!(
        ts.world
            .objects
            .get(g)
            .unwrap()
            .wo
            .world_object
            .requested_location
            .is_none(),
        "consumed by UpdateObjectPhysics"
    );

    let ahead = outdoor(HOME, 100.0, 104.0, 20.0);
    let mark = ts.received_raw(a).len();
    ts.send_game_action(a, &autonomous_position(&ahead, true));
    ts.advance(0.2);
    assert_eq!(movement_since(&ts, a, mark), [UPDATE_POSITION]);
    let pos = ts
        .received::<MovementPositionEvent>(a)
        .last()
        .copied()
        .expect("an UpdatePosition");
    assert_eq!(
        (pos.id, pos.position.origin.origin.y),
        (ObjectId(ALPHA), 104.0)
    );
    assert_eq!(location(&ts, ALPHA).position_y, 104.0);
    let h = phys_ext::physics_obj(&ts.world, g).expect("a body");
    assert_eq!(
        phys_ext::position(&ts.world, h).unwrap().frame.origin.y,
        104.0,
        "the body is at the requested position"
    );
    let f = empyrean_world::world_objects::player::fields(&ts.world, g);
    assert!(f.last_contact);
    assert_eq!(
        f.last_ground_pos.map(|p| p.position_y),
        Some(104.0),
        "a grounded AutonomousPosition is the last ground position"
    );
}

/// A speed hack: an AutonomousPosition three landblocks away (over 50 m and more than one block)
/// is refused by `UpdatePlayerPosition`: the location stays, and nothing is sent.
#[test]
fn a_speed_hack_position_is_rejected() {
    let mut ts = seeded();
    let start = outdoor(HOME, 100.0, 100.0, 20.0);
    let (a, _) = enter(&mut ts, "alpha", ALPHA, start);

    let mark = ts.received_raw(a).len();
    ts.send_game_action(
        a,
        &autonomous_position(&outdoor(0xACB4, 100.0, 100.0, 20.0), true),
    );
    ts.advance(0.3);
    assert!(
        movement_since(&ts, a, mark).is_empty(),
        "no UpdatePosition for a refused move"
    );
    let l = location(&ts, ALPHA);
    assert_eq!(
        (l.landblock(), l.position_x, l.position_y),
        (u32::from(HOME), 100.0, 100.0)
    );
    assert_eq!(
        ts.world
            .objects
            .get(ObjectGuid::new(ALPHA))
            .unwrap()
            .current_landblock
            .map(|b| b.landblock()),
        Some(HOME)
    );
}

/// A second client sees the update motion and update position.
#[test]
fn a_second_client_sees_the_update_motion_and_update_position() {
    let mut ts = seeded();
    let start = outdoor(HOME, 100.0, 100.0, 20.0);
    let (a, _) = enter(&mut ts, "alpha", ALPHA, start);
    let (b, _) = enter(&mut ts, "bravo", BRAVO, outdoor(HOME, 104.0, 100.0, 20.0));

    let mark = ts.received_raw(b).len();
    ts.send_game_action(a, &run_forward(&start));
    ts.advance(0.2);
    ts.send_game_action(
        a,
        &autonomous_position(&outdoor(HOME, 100.0, 104.0, 20.0), true),
    );
    ts.advance(0.2);

    let about_alpha: Vec<u32> = ts.received_raw(b)[mark..]
        .iter()
        .filter(|m| [UPDATE_MOTION, UPDATE_POSITION].contains(&m.opcode))
        .filter(|m| m.body[0..4] == ALPHA.to_le_bytes())
        .map(|m| m.opcode)
        .collect();
    assert_eq!(
        about_alpha,
        [UPDATE_MOTION, UPDATE_POSITION, UPDATE_POSITION]
    );
    let motion = ts.received::<MovementSetObjectMovement>(b);
    assert_eq!(
        forward_command(
            motion
                .iter()
                .find(|m| m.id == ObjectId(ALPHA))
                .expect("alpha's motion")
        ),
        Some(RUN_FORWARD & 0xFFFF)
    );
}

// ---- teleport, portal and recall ---------------------------------------------------------------

/// `Player.Teleport` to a far landblock: PlayerTeleport, the "fake" UpdatePosition at the
/// destination, then (after the physics state change) the forced position update, which moves the
/// player's body and its landblock (loading it) and sends the position privately. The player is
/// in portal space (teleporting, hidden, ignoring collisions) until `OnTeleportComplete` (ACE's
/// `GameActionLoginComplete`, sent by the client) materializes it.
#[test]
fn a_teleport_to_a_far_landblock_enters_portal_space_and_arrives() {
    let mut ts = seeded();
    let (a, _) = enter(&mut ts, "alpha", ALPHA, outdoor(HOME, 100.0, 100.0, 20.0));
    let g = ObjectGuid::new(ALPHA);

    let mark = ts.received_raw(a).len();
    let dest = outdoor(FAR, 50.0, 60.0, 20.0);
    player_location::teleport(&mut ts.world, g, &dest, false);
    let hits = TestServer::take_not_ported();
    ts.advance(0.1);

    assert_eq!(
        movement_since(&ts, a, mark),
        [PLAYER_TELEPORT, UPDATE_POSITION, UPDATE_POSITION]
    );
    assert_eq!(ts.received::<EffectsPlayerTeleport>(a).len(), 1);
    let positions: Vec<(u32, f32)> = ts
        .received::<MovementPositionEvent>(a)
        .iter()
        .rev()
        .take(2)
        .map(|p| {
            (
                p.position.origin.objcell_id >> 16,
                p.position.origin.origin.x,
            )
        })
        .collect();
    // The fake update is 0.005 m above the destination (ObjScale 1); both are at the destination.
    assert_eq!(positions, [(u32::from(FAR), 50.0), (u32::from(FAR), 50.0)]);
    assert!(
        !hits.contains_key("ACE: WorldObject.EnqueueBroadcastPhysicsState"),
        "SetState is ported: {hits:?}"
    );
    // The portal-space SetState: the player's own client sees it hidden (Hidden | IgnoreCollisions).
    assert!(
        set_states(&ts, a, mark, ALPHA)
            .last()
            .is_some_and(|s| s & HIDDEN != 0),
        "a hidden SetState reaches the client"
    );

    let o = ts.world.objects.get(g).unwrap();
    assert!(o.wo.world_object.teleporting);
    assert_eq!(
        o.current_landblock.map(|b| b.landblock()),
        Some(FAR),
        "relocated to the destination landblock"
    );
    let l = o.location().unwrap();
    assert!(
        (l.position_z - 20.005).abs() < 1e-4,
        "PositionZ += 0.005 * ObjScale: {}",
        l.position_z
    );
    let h = phys_ext::physics_obj(&ts.world, g).expect("a body");
    assert_eq!(
        phys_ext::position(&ts.world, h).unwrap().cell.landblock().0,
        FAR,
        "the body is placed at the destination"
    );
    assert!(
        phys_ext::get_physics_state(&ts.world, g, empyrean_entity::enums::PhysicsState::Hidden),
        "pink bubble"
    );
    assert_eq!(o.ignore_collisions(), Some(true));
    assert_eq!(o.report_collisions(), Some(false));
    assert!(o.last_teleport_start_timestamp().is_some());

    // While teleporting, an AutonomousPosition from the old landblock is ignored.
    let mark = ts.received_raw(a).len();
    ts.send_game_action(
        a,
        &autonomous_position(&outdoor(HOME, 100.0, 100.0, 20.0), true),
    );
    ts.advance(0.2);
    assert!(movement_since(&ts, a, mark).is_empty());
    assert_eq!(location(&ts, ALPHA).landblock(), u32::from(FAR));

    let mark = ts.received_raw(a).len();
    ts.send_game_action(a, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    // `OnTeleportComplete` materializes the player: the client gets a SetState without Hidden.
    let states = set_states(&ts, a, mark, ALPHA);
    assert!(
        states
            .last()
            .is_some_and(|s| s & HIDDEN == 0 && s & IGNORE_COLLISIONS == 0),
        "a materializing SetState: {states:X?}"
    );
    let o = ts.world.objects.get(g).unwrap();
    assert!(!o.wo.world_object.teleporting);
    assert!(!phys_ext::get_physics_state(
        &ts.world,
        g,
        empyrean_entity::enums::PhysicsState::Hidden
    ));
    assert_eq!(
        (o.ignore_collisions(), o.report_collisions()),
        (Some(false), Some(true))
    );
}

/// A portal: its use requirements pass, `ActOnUse` teleports through `ThreadSafeTeleport` (the
/// next world tick), then its follow-up records the portal as the last one and says
/// `ITeleported`. Straight after, the portal refuses a second use as too recent (the error once;
/// then silently, within 3.5 s).
#[test]
fn a_portal_use_teleports_to_its_destination() {
    let mut ts = seeded();
    let (a, _) = enter(&mut ts, "alpha", ALPHA, outdoor(HOME, 100.0, 100.0, 20.0));
    let g = ObjectGuid::new(ALPHA);

    let p = ObjectGuid::new(PORTAL);
    let mut o = WorldObject {
        guid: p,
        kind: KindData::Portal(Box::default()),
        ..Default::default()
    };
    o.biota.id = PORTAL;
    o.biota.weenie_class_id = 1234;
    o.biota.weenie_type = WeenieType::Portal;
    o.set_property(PropertyString::Name, "Test Portal".to_owned());
    o.set_position(
        PositionType::Destination,
        Some(outdoor(FAR, 50.0, 60.0, 20.0)),
    );
    o.set_location(Some(outdoor(HOME, 102.0, 100.0, 20.0)));
    ts.world.objects.insert(o).expect("fresh");

    // Player.EnterWorld set LastTeleportStartTimestamp to the login time, so OnTeleportComplete
    // did not stamp LastPortalTeleportTimestamp: a portal may be used right after login.
    assert!(ts
        .world
        .objects
        .get(g)
        .unwrap()
        .last_portal_teleport_timestamp()
        .is_none());
    let r = portal::check_use_requirements(&mut ts.world, p, g);
    assert!(r.success && r.message.is_none());

    let mark = ts.received_raw(a).len();
    empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, p, g);
    assert!(
        !ts.world.objects.get(g).unwrap().wo.world_object.teleporting,
        "ThreadSafeTeleport waits for the world queue"
    );
    ts.advance(0.2);
    assert_eq!(
        opcodes_since(&ts, a, mark)
            .into_iter()
            .filter(|&op| op != SYSTEM_CHAT)
            .collect::<Vec<_>>(),
        // `Teleport` sends the portal-space SetState between the fake and the forced UpdatePosition.
        [
            PLAYER_TELEPORT,
            UPDATE_POSITION,
            SET_STATE,
            UPDATE_POSITION,
            0xF7B0_0000 | EV_WEENIE_ERROR
        ]
    );
    let err = ts
        .received_raw(a)
        .iter()
        .rev()
        .find(|m| m.opcode == GAME_EVENT)
        .expect("the weenie error");
    assert_eq!(
        u32::from_le_bytes(err.body[12..16].try_into().unwrap()),
        I_TELEPORTED
    );
    let o = ts.world.objects.get(g).unwrap();
    assert_eq!(
        o.last_portal_did(),
        Some(1234),
        "LastPortalDID: the portal can be recalled to"
    );
    assert_eq!(o.current_landblock.map(|b| b.landblock()), Some(FAR));
    assert!(o.last_portal_teleport_timestamp().is_some(), "fromPortal");

    // too soon: the error once, then nothing
    ts.send_game_action(a, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    let r = portal::check_use_requirements(&mut ts.world, p, g);
    assert!(
        !r.success && r.message.is_some(),
        "YouHaveBeenTeleportedTooRecently"
    );
    let r = portal::check_use_requirements(&mut ts.world, p, g);
    assert!(
        !r.success && r.message.is_none(),
        "no second message within 3.5 s"
    );
}

/// The lifestone recall (`/ls`): half the mana, the recall chat to those in range, the recall
/// motion; the player is busy for the animation (no motion table in the fake dats: 0 s), then
/// teleports to its sanctuary.
#[test]
fn a_lifestone_recall_teleports_to_the_sanctuary() {
    let mut ts = seeded();
    let (a, _) = enter(&mut ts, "alpha", ALPHA, outdoor(HOME, 100.0, 100.0, 20.0));
    let g = ObjectGuid::new(ALPHA);

    // No sanctuary: the message, nothing else.
    let mark = ts.received_raw(a).len();
    ts.client_mut(a).send(
        NetQueue::Weenie,
        &pack_action_raw(0x100, Opcode(0x0063), &[]),
    );
    ts.advance(0.1);
    let chat: Vec<String> = ts
        .received::<CommunicationTextboxString>(a)
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert_eq!(
        chat.last().map(String::as_str),
        Some("Your spirit has not been attuned to a sanctuary location.")
    );
    assert!(movement_since(&ts, a, mark).is_empty());

    ts.world.objects.get_mut(g).unwrap().set_position(
        PositionType::Sanctuary,
        Some(outdoor(FAR, 50.0, 60.0, 20.0)),
    );
    let mark = ts.received_raw(a).len();
    ts.client_mut(a).send(
        NetQueue::Weenie,
        &pack_action_raw(0x101, Opcode(0x0063), &[]),
    );
    ts.advance(0.3);

    let chat: Vec<String> = ts
        .received::<CommunicationTextboxString>(a)
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert_eq!(
        chat.last().map(String::as_str),
        Some("Alpha is recalling to the lifestone.")
    );
    let ops = opcodes_since(&ts, a, mark);
    let chat_at = ops
        .iter()
        .position(|&op| op == SYSTEM_CHAT)
        .expect("the recall chat");
    assert_eq!(
        movement_since(&ts, a, mark),
        [
            UPDATE_MOTION,
            PLAYER_TELEPORT,
            UPDATE_POSITION,
            UPDATE_POSITION
        ]
    );
    assert!(
        ops.iter().position(|&op| op == UPDATE_MOTION).unwrap() > chat_at,
        "chat, then the recall motion"
    );

    let o = ts.world.objects.get(g).unwrap();
    let mana = o.mana();
    assert_eq!(mana.current(o), 50, "UpdateVital(Mana, Mana.Current / 2)");
    assert!(!o.wo.world_object.is_busy);
    assert_eq!(o.current_landblock.map(|b| b.landblock()), Some(FAR));
}

const SET_STATE: u32 = 0xF74B;
const HIDDEN: u32 = 0x4000;
const IGNORE_COLLISIONS: u32 = 0x10;

/// The `PhysicsState` of every SetState (0xF74B) for `guid` the client received since `from`.
fn set_states(ts: &TestServer, id: ClientId, from: usize, guid: u32) -> Vec<u32> {
    ts.received_raw(id)
        .iter()
        .skip(from)
        .filter(|m| {
            m.opcode == 0xF74B && u32::from_le_bytes(m.body[0..4].try_into().unwrap()) == guid
        })
        .map(|m| u32::from_le_bytes(m.body[4..8].try_into().unwrap()))
        .collect()
}
