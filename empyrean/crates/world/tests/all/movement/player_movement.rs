//! ACE: Source/ACE.Server/WorldObjects/Player_Move.cs::MoveTo
//! Player move/move2/tick/location members on hermetic world; portal destination text; run
//! factor; PK jump mid-cast stops cast; real-content: movement replies match recorded sessions'
//! opcode order.
//! Fixture: A player in an isolated world with synthetic terrain and dats; real-content checks use retail dats and recorded movement messages.

use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_primitives::{DataId, Vec3};
use dereth_protocol::actions::pack_action;
use dereth_protocol::movement::{
    AutonomousPosition, JumpPack, MoveTimestamps, MoveToStatePack, MovementAutonomousPosition,
    MovementJump, MovementMoveToState, RawMotionState,
};
use dereth_protocol::types::space::{Frame, PositionWire, Quat as WQuat, Vec3 as WVec3};
use dereth_protocol::Message;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{DotNetDict, Vector3};
use empyrean_common::not_ported;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    EnvironChangeType, MotionStance, PlayerKillerStatus, PortalBitmask, PositionType,
    PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyInt,
    PropertyString, WeenieError, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{ClientMessage, SessionId, SessionState};
use empyrean_testkit::land;
use empyrean_world::entity::strings;
use empyrean_world::managers::landblock_manager;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::managers::inbound_message_manager::{
    handle_client_message, run_inbound_message_queue,
};
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::network::sequence::sequence_type::SequenceType;
use empyrean_world::physics::phys_ext;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    creature_vitals, player, player_location, player_move, player_move2, player_tick, portal,
    world_object_tick as tick,
};
use empyrean_world::World;

pub(crate) const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const P: u32 = 0x5000_0001;
pub(crate) const HOME: u16 = 0xA9B4;
const BLOCKS: [u16; 6] = [0xA9B4, 0xAAB4, 0xA9B5, 0xA8B4, 0xA9B3, 0xACB4];

const UPDATE_MOTION: u32 = 0xF74C;
const UPDATE_POSITION: u32 = 0xF748;
const VECTOR_UPDATE: u32 = 0xF74E;
const SYSTEM_CHAT: u32 = 0xF7E0;
const SOUND: u32 = 0xF750;

pub(crate) fn g() -> ObjectGuid {
    ObjectGuid::new(P)
}

pub(crate) fn outdoor(lb: u16, x: f32, y: f32, z: f32) -> Position {
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

pub(crate) fn wire(p: &Position) -> PositionWire {
    PositionWire {
        objcell_id: p.cell(),
        frame: Frame {
            origin: WVec3 {
                x: p.position_x,
                y: p.position_y,
                z: p.position_z,
            },
            orientation: WQuat {
                w: p.rotation_w,
                x: p.rotation_x,
                y: p.rotation_y,
                z: p.rotation_z,
            },
        },
    }
}

fn dats() -> Arc<empyrean_dat::DatManager> {
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
    // Jump = (Strength + Coordination) / 2, as retail's
    let mut skills = empyrean_dat::fake::sample::skill_table();
    skills.skills.entry(22).or_insert_with(|| SkillBase {
        description: String::new(),
        name: "Jump".to_owned(),
        icon: 0,
        trained_cost: 0,
        specialized_cost: 4,
        category: 1,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z: 2,
            attr1: 1,
            attr2: 4,
        },
        upper_bound: 0.0,
        lower_bound: 0.0,
        learn_mod: 0.0,
    });
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .with_skill_table(skills)
        .build()
        .expect("fake dats")
}

/// A world on flat land with one player at `HOME` (100, 100, 20) joined to its landblock and to a
/// `WorldConnected` session, out of the login pink bubble.
pub(crate) fn world() -> World {
    world_on(
        Some(&BLOCKS),
        outdoor(HOME, 100.0, 100.0, 20.0),
        dats(),
        land::TEST_SETUP,
    )
}

/// [`world`] over flat `blocks` (or, with `None`, the dats' own land), with the player at `at`,
/// its body made from `setup`.
fn world_on(
    blocks: Option<&[u16]>,
    at: Position,
    dats: Arc<empyrean_dat::DatManager>,
    setup: u32,
) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats);
    if let Some(blocks) = blocks {
        land::use_flat_land_with_test_setup(&mut w, blocks, 10);
    }
    w.sessions.insert(
        S,
        SessionData {
            state: SessionState::WorldConnected,
            player: Some(g()),
            ..SessionData::default()
        },
    );

    let mut o = WorldObject {
        guid: g(),
        container: Some(Box::default()),
        creature: Some(Box::default()),
        player: Some(Box::default()),
        kind: KindData::Player,
        ..Default::default()
    };
    o.biota.id = P;
    o.biota.weenie_type = WeenieType::Creature;
    o.set_property(PropertyDataId::Setup, setup);
    o.set_property(PropertyString::Name, "Alpha".to_owned());
    o.set_property(PropertyBool::ReportCollisions, true);
    o.set_property(PropertyBool::IgnoreCollisions, false);
    let attributes = o
        .biota
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
    let vitals = o
        .biota
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
                current_level: 150,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    o.set_location(Some(at));
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::from_stance(MotionStance::NonCombat));
    creature_vitals::set_ephemeral_stat_values(&w, &mut o);
    tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    // a player biota as the shard stores it: an (empty) enchantment registry
    o.biota.properties_enchantment_registry = Some(Default::default());
    // Player.EnterWorld: `LastTeleportStartTimestamp = lastLoginTimestamp`
    o.set_last_teleport_start_timestamp(Some(w.now.unix_time));
    w.objects.insert(o).expect("fresh");
    assert!(landblock_manager::add_object(&mut w, g(), false));
    player_location::on_teleport_complete(&mut w, g());
    not_ported::take_local();
    w
}

/// Hands one game action to the inbound manager and runs its queue, as the world loop would.
pub(crate) fn action<M: Message>(w: &mut World, m: &M) {
    handle_client_message(
        w,
        ClientMessage::new(pack_action(0x10, m).expect("encode")).expect("opcode"),
        S,
    );
    run_inbound_message_queue(w);
}

pub(crate) fn sent_opcodes() -> Vec<u32> {
    take_sent()
        .into_iter()
        .map(|(_, _, b)| u32::from_le_bytes(b[0..4].try_into().unwrap()))
        .collect()
}

pub(crate) fn obj(w: &World) -> &WorldObject {
    w.objects.get(g()).expect("the player")
}

pub(crate) fn body(w: &World) -> dereth_physics::PhysHandle {
    phys_ext::physics_obj(w, g()).expect("a body")
}

// ---- the static helpers --------------------------------------------------------------------

/// `EncumbranceSystem.EncumbranceCapacity`: 150 per point of strength, plus 30 per augmentation
/// (capped at 150) per point; `GetBurden` (the shared rules' `burden::load`): 3.0 without
/// capacity, 0 for negative encumbrance;
/// `MovementSystem.JumpStaminaCost`: a PK pays `(int)((power + 1) * 100)`, anyone else
/// `ceil((burden + 0.5) * power * 8 + 2)`.
#[test]
fn encumbrance_and_jump_stamina_follow_ace() {
    use dereth_rules::burden;
    use empyrean_world::physics::weenie_object::encumbrance_system_encumbrance_capacity as encumbrance_capacity;
    assert_eq!(encumbrance_capacity(100, 0), 15_000);
    assert_eq!(encumbrance_capacity(100, 3), 24_000);
    assert_eq!(
        encumbrance_capacity(100, 10),
        30_000,
        "the bonus caps at 150"
    );
    assert_eq!(
        encumbrance_capacity(100, -1),
        15_000,
        "a negative bonus is dropped"
    );
    assert_eq!(encumbrance_capacity(0, 3), 0);
    assert_eq!(burden::load(0, 100), 3.0);
    assert_eq!(burden::load(15_000, 7_500), 0.5);
    assert_eq!(burden::load(15_000, -5), 0.0);
    assert_eq!(player_move::jump_stamina_cost(1.0, 0.5, false), 10);
    assert_eq!(player_move::jump_stamina_cost(0.5, 0.0, false), 4);
    assert_eq!(
        player_move::jump_stamina_cost(0.3, 0.0, false),
        4,
        "ceil(3.2)"
    );
    assert_eq!(player_move::jump_stamina_cost(0.5, 0.0, true), 150);
}

/// `GetChargeParameters`: run hold key, fail distance 15, speed 1.5, as ACE; the flags and the
/// threshold are retail's attack chase (V257: 0x1EFF0 and 15.0; ACE: its defaults without
/// CanWalk plus the charge flags, 0x1EFFE, and 1.0). `GetTurnToParams` toggles StopCompletely.
/// `GetMoveToParams` takes the use radius and the kind of move's flag word (V257); ACE cleared
/// CanRun at half its 1.0 threshold or farther.
#[test]
fn move_to_parameters_follow_the_recorded_movement_kind() {
    use dereth_animation::motion::{flags, HoldKey};
    use empyrean_world::network::motion::move_to_parameters::RetailMoveTo;
    let mvp = player_move::get_charge_parameters();
    assert_eq!(mvp.flags, 0x1_EFF0);
    assert_eq!(mvp.flags & flags::CAN_WALK, 0);
    assert_eq!(
        (
            mvp.hold_key_to_apply,
            mvp.fail_distance,
            mvp.speed,
            mvp.walk_run_threshold
        ),
        (HoldKey::Run, 15.0, 1.5, 15.0)
    );
    assert_eq!(
        player_move2::get_turn_to_params(true).flags & flags::STOP_COMPLETELY,
        flags::STOP_COMPLETELY
    );
    assert_eq!(
        player_move2::get_turn_to_params(false).flags & flags::STOP_COMPLETELY,
        0
    );

    let mut w = world();
    let t = ObjectGuid::new(0x7000_0001);
    let mut o = WorldObject {
        guid: t,
        ..Default::default()
    };
    o.set_location(Some(outdoor(HOME, 100.4, 100.0, 20.0)));
    w.objects.insert(o).unwrap();
    let near = player_move2::get_move_to_params(&w, g(), t, None, RetailMoveTo::Use);
    assert_eq!(
        (near.distance_to_object, near.flags, near.walk_run_threshold),
        (0.6, 0x1_EE4F, 15.0),
        "0.4 m"
    );
    w.objects
        .get_mut(t)
        .unwrap()
        .set_location(Some(outdoor(HOME, 110.0, 100.0, 20.0)));
    let far = player_move2::get_move_to_params(&w, g(), t, Some(2.0), RetailMoveTo::Plain);
    assert_eq!(
        (far.distance_to_object, far.flags, far.walk_run_threshold),
        (2.0, 0x1_EE0F, 15.0),
        "10 m: CanRun kept"
    );
}

// ---- ValidateMovement and UpdatePlayerPosition ------------------------------------------------

/// `ValidateMovement`: no landblock refuses; outside a teleport, a landblock change between two
/// interior cells is refused unless both are the bugged cells; outdoors it is allowed.
#[test]
fn validate_movement_follows_ace() {
    let mut w = world();
    assert!(
        player_tick::validate_movement(&mut w, g(), &outdoor(0xAAB4, 5.0, 5.0, 20.0)),
        "outdoors into the next block"
    );

    let inside =
        Position::from_components(0xA9B4_0105, 10.0, 10.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    w.objects.get_mut(g()).unwrap().set_location(Some(inside));
    let next_inside =
        Position::from_components(0xAAB4_0105, 10.0, 10.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    assert!(
        !player_tick::validate_movement(&mut w, g(), &next_inside),
        "interior to interior across landblocks"
    );
    let same_block =
        Position::from_components(0xA9B4_0106, 10.0, 10.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    assert!(player_tick::validate_movement(&mut w, g(), &same_block));
    w.objects.get_mut(g()).unwrap().wo.world_object.teleporting = true;
    assert!(
        player_tick::validate_movement(&mut w, g(), &next_inside),
        "a teleport may"
    );
    w.objects.get_mut(g()).unwrap().wo.world_object.teleporting = false;

    let bugged_a =
        Position::from_components(0xD699_0112, 10.0, 10.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    w.objects.get_mut(g()).unwrap().set_location(Some(bugged_a));
    assert!(
        !player_tick::validate_movement(&mut w, g(), &next_inside),
        "only both bugged cells pass"
    );
    let bugged_b =
        Position::from_components(0xD599_012C, 10.0, 10.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    w.objects.get_mut(g()).unwrap().set_location(Some(bugged_a));
    assert!(
        player_tick::validate_movement(&mut w, g(), &bugged_b),
        "the two bugged cells"
    );

    w.objects.get_mut(g()).unwrap().current_landblock = None;
    assert!(
        !player_tick::validate_movement(&mut w, g(), &bugged_b),
        "no CurrentLandblock"
    );
}

/// The UpdatePosition cadence: a broadcast request, or the first update in over a second, goes
/// through `SendUpdatePosition` (which stamps `LastUpdatePosition`); a MoveToState request within
/// the second is sent to the player alone. A request at the same place (within epsilon) only
/// turns the body, and still sends.
#[test]
fn update_player_position_follows_aces_cadence() {
    let mut w = world();
    start_capture();
    let first = outdoor(HOME, 101.0, 100.0, 20.0);
    assert!(
        !player_tick::update_player_position(&mut w, g(), first, false),
        "same landblock"
    );
    assert_eq!(sent_opcodes(), [UPDATE_POSITION]);
    let stamped = obj(&w).wo.world_object_networking.last_update_position;
    assert_eq!(
        stamped, w.now.utc,
        "SendUpdatePosition stamps LastUpdatePosition"
    );
    assert_eq!(
        phys_ext::position(&w, body(&w)).unwrap().frame.origin.x,
        101.0
    );

    // within the second, without a broadcast: private
    w.objects
        .get_mut(g())
        .unwrap()
        .wo
        .world_object
        .requested_location_broadcast = false;
    w.now.utc = w.now.utc.add_seconds(0.5);
    let second = outdoor(HOME, 102.0, 100.0, 20.0);
    player_tick::update_player_position(&mut w, g(), second, false);
    assert_eq!(sent_opcodes(), [UPDATE_POSITION]);
    assert_eq!(
        obj(&w).wo.world_object_networking.last_update_position,
        stamped,
        "the private send does not stamp"
    );

    // the same place, turned: the body's rotation follows
    let mut turned = second;
    turned.rotation_z = std::f32::consts::FRAC_1_SQRT_2;
    turned.rotation_w = std::f32::consts::FRAC_1_SQRT_2;
    player_tick::update_player_position(&mut w, g(), turned, false);
    assert_eq!(
        phys_ext::position(&w, body(&w)).unwrap().frame.rotation.z,
        std::f32::consts::FRAC_1_SQRT_2
    );
    assert_eq!(
        obj(&w).location().unwrap().rotation_z,
        std::f32::consts::FRAC_1_SQRT_2
    );
}

/// The speed check: over 50 m and more than one landblock is refused (here 199 m, two blocks).
#[test]
fn a_move_over_50_m_across_two_landblocks_is_refused() {
    let mut w = world();
    w.objects
        .get_mut(g())
        .unwrap()
        .set_location(Some(outdoor(HOME, 190.0, 100.0, 20.0)));
    start_capture();
    assert!(!player_tick::update_player_position(
        &mut w,
        g(),
        outdoor(0xABB4, 5.0, 100.0, 20.0),
        false
    ));
    assert_eq!(obj(&w).location().unwrap().landblock(), u32::from(HOME));
    assert!(sent_opcodes().is_empty());
}

/// The z-position hack: in the same landblock, over 10 m above the last ground position, with no
/// jump in the last second and Jump skill under 1000, a player off the ground with velocity is
/// snapped back to the last ground position, with a force-position sequence and an
/// UpdatePosition, and the move is refused.
#[test]
fn a_z_hack_is_snapped_back_to_the_last_ground_position() {
    let mut w = world();
    let ground = outdoor(HOME, 100.0, 100.0, 20.0);
    player::fields_mut(&mut w, g()).last_ground_pos = Some(ground);
    let h = body(&w);
    let o = w.physics.get_mut(h).unwrap();
    o.set_on_walkable(false);
    o.velocity_vector = Vec3::new(0.0, 0.0, 5.0);
    let before = w
        .objects
        .get_mut(g())
        .unwrap()
        .sequences
        .get_current_sequence(SequenceType::ObjectForcePosition);

    start_capture();
    let up = outdoor(HOME, 100.0, 101.0, 35.0);
    assert!(!player_tick::update_player_position(&mut w, g(), up, false));
    let l = obj(&w).location().unwrap();
    assert_eq!(
        (l.position_y, l.position_z),
        (100.0, 20.0),
        "Location = LastGroundPos"
    );
    assert_ne!(
        w.objects
            .get_mut(g())
            .unwrap()
            .sequences
            .get_current_sequence(SequenceType::ObjectForcePosition),
        before
    );
    assert_eq!(sent_opcodes(), [UPDATE_POSITION]);

    // after a recent jump the same move is allowed
    player::fields_mut(&mut w, g()).last_jump_time = w.now.utc;
    assert!(!player_tick::update_player_position(&mut w, g(), up, false));
    assert_eq!(obj(&w).location().unwrap().position_z, 35.0);
}

/// `SyncLocationWithPhysics` copies the body's position; its landblock test shifts the wrong way
/// (ACE-BUG), so it answers true for a player that has not changed landblock.
#[test]
fn sync_location_with_physics_copies_the_body_and_carries_aces_shift_bug() {
    let mut w = world();
    assert!(
        player_tick::sync_location_with_physics(&mut w, g()),
        "ACE-BUG: `blockcell << 16`"
    );
    let p = phys_ext::position(&w, body(&w)).unwrap();
    assert_eq!(obj(&w).location().unwrap().cell(), p.cell.0);
}

// ---- the handlers ------------------------------------------------------------------------------

/// MoveToState: the current and last state, a request without a broadcast, and the UpdateMotion
/// (sent to the player too). An AFK player pressing a movement key at run speed leaves AFK.
#[test]
fn move_to_state_requests_the_position_and_leaves_afk() {
    let mut w = world();
    w.objects.get_mut(g()).unwrap().set_is_afk(true);
    let at = outdoor(HOME, 100.0, 100.0, 20.0);
    let m = MovementMoveToState(MoveToStatePack {
        raw_motion_state: RawMotionState {
            current_holdkey: Some(2),
            forward_command: Some(0x4400_0007),
            forward_holdkey: Some(2),
            ..RawMotionState::default()
        },
        position: wire(&at),
        timestamps: MoveTimestamps::default(),
        contact: true,
        longjump_mode: false,
    });
    start_capture();
    action(&mut w, &m);
    let o = obj(&w);
    assert_eq!(
        o.wo.world_object.requested_location.map(|p| p.cell()),
        Some(at.cell())
    );
    assert!(
        !o.wo.world_object.requested_location_broadcast,
        "SetRequestedLocation(position, false)"
    );
    let f = player_move::fields(&w, g());
    assert_eq!(
        f.current_move_to_state.raw_motion_state.forward_command.0,
        0x4400_0007
    );
    assert!(f.last_move_to_state.is_some());
    assert!(!o.is_afk(), "HandleActionSetAFKMode(false)");
    let ops = sent_opcodes();
    assert!(ops.contains(&UPDATE_MOTION), "{ops:x?}");
}

/// AutonomousPosition: the contact flag and, on the ground, the last ground position; the
/// request carries a broadcast. While teleporting nothing is requested.
#[test]
fn autonomous_position_records_contact_and_requests_with_a_broadcast() {
    let mut w = world();
    let at = outdoor(HOME, 103.0, 100.0, 20.0);
    action(
        &mut w,
        &MovementAutonomousPosition(AutonomousPosition {
            position: wire(&at),
            timestamps: MoveTimestamps::default(),
            contact: 1,
        }),
    );
    assert!(player::fields(&w, g()).last_contact);
    assert_eq!(
        player::fields(&w, g())
            .last_ground_pos
            .map(|p| p.position_x),
        Some(103.0)
    );
    assert!(obj(&w).wo.world_object.requested_location_broadcast);

    let o = w.objects.get_mut(g()).unwrap();
    o.wo.world_object.requested_location = None;
    o.wo.world_object.teleporting = true;
    let air = outdoor(HOME, 104.0, 100.0, 25.0);
    action(
        &mut w,
        &MovementAutonomousPosition(AutonomousPosition {
            position: wire(&air),
            timestamps: MoveTimestamps::default(),
            contact: 0,
        }),
    );
    assert!(!player::fields(&w, g()).last_contact);
    assert_eq!(
        player::fields(&w, g())
            .last_ground_pos
            .map(|p| p.position_x),
        Some(103.0),
        "unchanged in the air"
    );
    assert!(
        obj(&w).wo.world_object.requested_location.is_none(),
        "not while teleporting"
    );
}

/// Jump (the client's 56-byte pack, jump payload): the stamina cost for the extent and burden, the jump
/// time and start position, the body launched off the ground at the pack's velocity, then an
/// UpdateMotion and a VectorUpdate.
#[test]
fn jump_costs_stamina_and_launches_the_body() {
    let mut w = world();
    let stamina_before = {
        let o = obj(&w);
        o.stamina().current(o)
    };
    let at = outdoor(HOME, 100.0, 100.0, 20.0);
    let m = MovementJump(JumpPack {
        extent: 1.0,
        velocity: WVec3 {
            x: 0.0,
            y: 2.0,
            z: 8.0,
        },
        position: wire(&at),
        timestamps: MoveTimestamps::default(),
    });
    start_capture();
    action(&mut w, &m);
    assert_eq!(
        sent_opcodes()
            .into_iter()
            .filter(|o| *o == UPDATE_MOTION || *o == VECTOR_UPDATE)
            .collect::<Vec<_>>(),
        [UPDATE_MOTION, VECTOR_UPDATE]
    );
    // burden 0 (no encumbrance), extent 1: ceil(0.5 * 1 * 8 + 2) = 6
    let stamina_after = {
        let o = obj(&w);
        o.stamina().current(o)
    };
    assert_eq!(stamina_before - stamina_after, 6);
    let f = player_move::fields(&w, g());
    assert_eq!(player::fields(&w, g()).last_jump_time, w.now.utc);
    assert_eq!(f.start_jump.map(|p| p.cell()), Some(at.cell()));
    let o = w.physics.get(body(&w)).unwrap();
    assert!(!o.transient_state.on_walkable() && !o.transient_state.in_contact());
    assert_eq!(
        o.velocity_vector.z, 8.0,
        "set_local_velocity (identity rotation)"
    );
}

// ---- falling damage, recalls, teleport --------------------------------------------------------

/// Falling damage: at -30 m/s the ratio is `-(11.25434 - 30 + 4.5) / 11.25434`, times 87.29381:
/// 110.4958 in single precision, rounded to 110, then the fall message and the wound sound.
#[test]
fn falling_damage_follows_aces_formula() {
    let mut w = world();
    let h = body(&w);
    w.physics.get_mut(h).unwrap().velocity_vector = Vec3::new(0.0, 0.0, -30.0);
    let health = obj(&w).health();
    let before = health.current(obj(&w));
    let max = health.max_value(
        &mut empyrean_world::world_objects::entity::creature_attribute::StatCtx::in_world(
            &mut w,
            g(),
        ),
    );
    start_capture();
    player_move::handle_falling_damage(&mut w, g());
    let after = {
        let o = obj(&w);
        o.health().current(o)
    };
    assert_eq!(before - after, 110);
    let sent = take_sent();
    let chat = sent
        .iter()
        .find(|(_, _, b)| u32::from_le_bytes(b[0..4].try_into().unwrap()) == SYSTEM_CHAT)
        .expect("the fall message");
    let len = usize::from(u16::from_le_bytes(chat.2[4..6].try_into().unwrap()));
    assert_eq!(
        String::from_utf8_lossy(&chat.2[6..6 + len]),
        strings::get_fall_message(110, max)
    );
    assert!(
        sent.iter()
            .any(|(_, _, b)| u32::from_le_bytes(b[0..4].try_into().unwrap()) == SOUND),
        "Sound.Wound3"
    );

    // a gentle landing does nothing
    w.physics.get_mut(h).unwrap().velocity_vector = Vec3::new(0.0, 0.0, -10.0);
    player_move::handle_falling_damage(&mut w, g());
    assert_eq!(
        {
            let o = obj(&w);
            o.health().current(o)
        },
        after
    );
}

/// The recall refusals, in ACE's order: a PK arena recall by a non-PK, a busy player, a moved
/// recall (the check after the animation).
#[test]
fn recalls_refuse_in_aces_order() {
    let mut w = world();
    start_capture();
    player_location::handle_action_tele_to_pk_arena(&mut w, g());
    assert_eq!(weenie_errors(), [we(WeenieError::OnlyPKsMayUseCommand)]);
    w.objects.get_mut(g()).unwrap().wo.world_object.is_busy = true;
    w.objects.get_mut(g()).unwrap().set_position(
        PositionType::Sanctuary,
        Some(outdoor(HOME, 10.0, 10.0, 20.0)),
    );
    player_location::handle_action_tele_to_lifestone(&mut w, g());
    assert_eq!(weenie_errors(), [we(WeenieError::YoureTooBusy)]);
    w.objects.get_mut(g()).unwrap().wo.world_object.is_busy = false;
    w.objects
        .get_mut(g())
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player
        .account = Some(empyrean_store::models::auth::Account {
        account_id: 9,
        ..Default::default()
    });
    let mut mine = empyrean_common::dotnet::DotNetDict::new();
    mine.insert(
        g().full(),
        empyrean_world::entity::i_player::IPlayer::Online(g()),
    );
    w.player_manager.player_accounts.insert(9, mine);
    player_location::handle_action_tele_to_house(&mut w, g());
    assert_eq!(
        weenie_errors(),
        [we(WeenieError::YouMustOwnHouseToUseCommand)]
    );
    player_location::handle_action_recall_allegiance_hometown(&mut w, g());
    assert_eq!(weenie_errors(), [we(WeenieError::YouAreNotInAllegiance)]);
}

fn we(e: WeenieError) -> u32 {
    e.0.cast_unsigned()
}

/// The weenie errors sent since the last capture (`GameEventWeenieError`, event 0x028A).
fn weenie_errors() -> Vec<u32> {
    take_sent()
        .into_iter()
        .filter(|(_, _, b)| {
            u32::from_le_bytes(b[0..4].try_into().unwrap()) == 0xF7B0
                && u32::from_le_bytes(b[12..16].try_into().unwrap()) == 0x028A
        })
        .map(|(_, _, b)| u32::from_le_bytes(b[16..20].try_into().unwrap()))
        .collect()
}

/// A fog colour other than clear delays the teleport: the fog is cleared and the teleport retried
/// a second later through `ThreadSafeTeleport`; nothing is sent now.
#[test]
fn a_fogged_teleport_clears_the_fog_first() {
    let mut w = world();
    w.objects
        .get_mut(g())
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_networking
        .current_fog_color = Some(EnvironChangeType::BlackFog);
    start_capture();
    player_location::teleport(&mut w, g(), &outdoor(0xACB4, 50.0, 50.0, 20.0), false);
    assert!(!obj(&w).wo.world_object.teleporting);
    assert!(sent_opcodes().is_empty());
}

/// Thread safe teleport teleports from the world queue then its follow up.
#[test]
fn thread_safe_teleport_teleports_from_the_world_queue_then_its_follow_up() {
    use empyrean_world::entity::actions::action_queue;
    use empyrean_world::entity::actions::i_action::Action;
    use empyrean_world::entity::actions::i_actor::Actor;
    use empyrean_world::managers::world_manager;

    let mut w = world();
    let followed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&followed);
    let follow_up = Action::delegate(move |_w: &mut World| {
        flag.store(true, std::sync::atomic::Ordering::Relaxed)
    });
    world_manager::thread_safe_teleport(
        &mut w,
        g(),
        outdoor(0xACB4, 50.0, 50.0, 20.0),
        Some(follow_up),
        true,
    );
    assert!(!obj(&w).wo.world_object.teleporting, "queued, not run");

    action_queue::run_actions(&mut w, Actor::World);
    assert!(
        obj(&w).wo.world_object.teleporting,
        "Player.Teleport ran: portal space"
    );
    assert_eq!(not_ported::take_local().get("ACE: Player.Teleport"), None);
    assert!(
        !followed.load(std::sync::atomic::Ordering::Relaxed),
        "the follow-up waits for the next pass"
    );

    action_queue::run_actions(&mut w, Actor::World);
    assert!(
        followed.load(std::sync::atomic::Ordering::Relaxed),
        "then the follow-up"
    );
}

/// `HandleNoLogLandblock`: a player saved in a no-log landblock is moved to its lifestone; a
/// sentinel, a player elsewhere or one without a lifestone is not.
#[test]
fn a_no_log_landblock_sends_the_player_to_its_lifestone() {
    let pos = |cell: u32, x: f32| PropertiesPosition {
        obj_cell_id: cell,
        position_x: x,
        rotation_w: 1.0,
        ..PropertiesPosition::default()
    };
    let biota = |weenie_type: WeenieType, location: u32, sanctuary: bool| {
        let mut b = empyrean_entity::Biota {
            weenie_type,
            ..Default::default()
        };
        let p = b.properties_position.get_or_insert_with(DotNetDict::new);
        p.insert(PositionType::Location, pos(location, 1.0));
        if sanctuary {
            p.insert(PositionType::Sanctuary, pos(0xA9B4_0021, 42.0));
        }
        b
    };
    let mut b = biota(WeenieType::Creature, 0x0007_0100, true);
    assert!(player_location::handle_no_log_landblock(&mut b));
    let l = b
        .properties_position
        .as_ref()
        .unwrap()
        .get(&PositionType::Location)
        .unwrap();
    assert_eq!((l.obj_cell_id, l.position_x), (0xA9B4_0021, 42.0));

    let mut b = biota(WeenieType::Sentinel, 0x0007_0100, true);
    assert!(!player_location::handle_no_log_landblock(&mut b));
    let mut b = biota(WeenieType::Creature, 0xA9B4_0001, true);
    assert!(!player_location::handle_no_log_landblock(&mut b));
    let mut b = biota(WeenieType::Creature, 0x0007_0100, false);
    assert!(!player_location::handle_no_log_landblock(&mut b));
    assert_eq!(
        b.properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .unwrap()
            .obj_cell_id,
        0x0007_0100
    );
}

// ---- move-to chains ----------------------------------------------------------------------------

/// The legacy chain: a superseded chain, a timed-out chain and a target within the use radius
/// (no rotation) each call back at once, keeping `lastCompletedMove` in step.
#[test]
fn legacy_move_to_chains_call_back_as_ace_does() {
    use std::sync::atomic::{AtomicU8, Ordering};
    static RESULT: AtomicU8 = AtomicU8::new(9);
    let cb = || -> player_move::MoveToCallback {
        Box::new(|_w, ok| RESULT.store(u8::from(ok), Ordering::SeqCst))
    };

    let mut w = world();
    let t = ObjectGuid::new(0x7000_0002);
    let mut o = WorldObject {
        guid: t,
        ..Default::default()
    };
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_location(Some(outdoor(HOME, 100.3, 100.0, 20.0)));
    tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    w.objects.insert(o).unwrap();
    assert!(
        landblock_manager::add_object(&mut w, t, false),
        "the target is on the landblock (WithinUseRadius finds it)"
    );

    // superseded: the chain number is not the counter
    player_move::move_to_chain(&mut w, g(), t, 5, cb(), None);
    assert_eq!(RESULT.load(Ordering::SeqCst), 0);
    assert_eq!(player_move::fields(&w, g()).last_completed_move, 5);

    // timed out: started 16 s ago
    let started = w.now.utc.add_seconds(-16.0);
    let f = player_move::fields_mut(&mut w, g());
    f.move_to_chain_counter = 7;
    f.move_to_chain_start_time = started;
    RESULT.store(9, Ordering::SeqCst);
    player_move::move_to_chain(&mut w, g(), t, 7, cb(), None);
    assert_eq!(RESULT.load(Ordering::SeqCst), 0);
    assert!(
        !player_move::is_player_moving_to(&w, g()),
        "StopExistingMoveToChains"
    );

    let far = outdoor(HOME, 105.0, 100.0, 20.0);
    w.objects.get_mut(t).unwrap().set_location(Some(far));
    let h = w.objects.get(t).unwrap().phys.unwrap();
    empyrean_world::physics::phys_ext::set_position(
        &mut w,
        h,
        &empyrean_world::physics::phys_ext::to_physics_position(&far),
    );
    RESULT.store(9, Ordering::SeqCst);
    assert!(
        !empyrean_world::world_objects::world_object_use::is_within_use_radius_of(
            &w,
            g(),
            t,
            Some(1.0)
        )
    );
    player_move::create_move_to_chain(&mut w, g(), t, cb(), Some(1.0), false);
    assert_eq!(RESULT.load(Ordering::SeqCst), 9);
    assert!(player_move::is_player_moving_to(&w, g()));
    let f = player_move::fields(&w, g());
    assert_eq!(f.move_to_chain_start_time, w.now.utc);
    player_move::stop_existing_move_to_chains(&mut w, g());
    assert!(!player_move::is_player_moving_to(&w, g()));

    // within reach and no rotate: called back at once with success
    let near = outdoor(HOME, 100.3, 100.0, 20.0);
    w.objects.get_mut(t).unwrap().set_location(Some(near));
    empyrean_world::physics::phys_ext::set_position(
        &mut w,
        h,
        &empyrean_world::physics::phys_ext::to_physics_position(&near),
    );
    player_move::create_move_to_chain(&mut w, g(), t, cb(), Some(1.0), false);
    assert_eq!(RESULT.load(Ordering::SeqCst), 1);
    assert!(!player_move::is_player_moving_to(&w, g()));
}

// ---- Portal ------------------------------------------------------------------------------------

pub(crate) fn spawn_portal(w: &mut World, restrictions: Option<i32>) -> ObjectGuid {
    let p = ObjectGuid::new(0x7000_0010);
    let mut o = WorldObject {
        guid: p,
        kind: KindData::Portal(Box::default()),
        ..Default::default()
    };
    o.biota.weenie_type = WeenieType::Portal;
    o.biota.weenie_class_id = 4321;
    o.set_property(PropertyString::Name, "Portal to Somewhere".to_owned());
    o.set_position(
        PositionType::Destination,
        Some(outdoor(0xACB4, 50.0, 50.0, 20.0)),
    );
    if let Some(r) = restrictions {
        o.set_property(PropertyInt::PortalBitmask, r);
    }
    w.objects.insert(o).unwrap();
    p
}

/// `Portal.CheckUseRequirements` refusals, in ACE's order, each with its weenie error; a
/// successful check has neither.
#[test]
fn portal_use_requirements_refuse_in_aces_order() {
    let mut w = world();
    let p = spawn_portal(&mut w, None);
    let r = portal::check_use_requirements(&mut w, p, g());
    assert!(r.success && r.message.is_none());
    // 1 s after a portal: still too soon (3.5 s)
    let now = w.now.unix_time;
    w.objects
        .get_mut(g())
        .unwrap()
        .set_last_portal_teleport_timestamp(Some(now - 1.0));
    let r = portal::check_use_requirements(&mut w, p, g());
    assert!(
        !r.success && r.message.is_some(),
        "YouHaveBeenTeleportedTooRecently"
    );
    w.objects
        .get_mut(g())
        .unwrap()
        .set_last_portal_teleport_timestamp(Some(now - 10.0));

    let refusal = |w: &mut World, set: &dyn Fn(&mut World)| {
        set(w);
        let r = portal::check_use_requirements(w, p, g());
        assert!(!r.success);
        r.message
            .map(|m| u32::from_le_bytes(m.data[16..20].try_into().unwrap()))
    };
    // not a player: silently
    let r = portal::check_use_requirements(&mut w, p, p);
    assert!(!r.success && r.message.is_none());
    // level
    assert_eq!(
        refusal(&mut w, &|w| {
            w.objects
                .get_mut(g())
                .unwrap()
                .set_property(PropertyInt::Level, 5);
            w.objects
                .get_mut(ObjectGuid::new(0x7000_0010))
                .unwrap()
                .set_property(PropertyInt::MinLevel, 10);
        }),
        Some(we(WeenieError::YouAreNotPowerfulEnoughToUsePortal))
    );
    w.objects
        .get_mut(p)
        .unwrap()
        .remove_property(PropertyInt::MinLevel);
    assert_eq!(
        refusal(&mut w, &|w| w
            .objects
            .get_mut(ObjectGuid::new(0x7000_0010))
            .unwrap()
            .set_property(PropertyInt::MaxLevel, 3)),
        Some(we(WeenieError::YouAreTooPowerfulToUsePortal))
    );
    w.objects
        .get_mut(p)
        .unwrap()
        .remove_property(PropertyInt::MaxLevel);
    // restrictions
    assert_eq!(
        refusal(&mut w, &|w| w
            .objects
            .get_mut(ObjectGuid::new(0x7000_0010))
            .unwrap()
            .set_property(PropertyInt::PortalBitmask, 0)),
        Some(we(WeenieError::PlayersMayNotUsePortal)),
        "Undef"
    );
    assert_eq!(
        refusal(&mut w, &|w| w
            .objects
            .get_mut(ObjectGuid::new(0x7000_0010))
            .unwrap()
            .set_property(
                PropertyInt::PortalBitmask,
                PortalBitmask::NoNPK.0
            )),
        Some(we(WeenieError::NonPKsMayNotUsePortal))
    );
    // a PK is not refused by NoNPK, but is by NoPk
    w.objects.get_mut(g()).unwrap().set_property(
        PropertyInt::PlayerKillerStatus,
        PlayerKillerStatus::PK.0.cast_signed(),
    );
    assert_eq!(
        refusal(&mut w, &|w| w
            .objects
            .get_mut(ObjectGuid::new(0x7000_0010))
            .unwrap()
            .set_property(PropertyInt::PortalBitmask, PortalBitmask::NoPk.0)),
        Some(we(WeenieError::PKsMayNotUsePortal))
    );
    // ignoring restrictions passes them all
    w.objects
        .get_mut(g())
        .unwrap()
        .set_property(PropertyBool::IgnorePortalRestrictions, true);
    let r = portal::check_use_requirements(&mut w, p, g());
    assert!(r.success);
    // teleporting: silently
    w.objects.get_mut(g()).unwrap().wo.world_object.teleporting = true;
    let r = portal::check_use_requirements(&mut w, p, g());
    assert!(!r.success && r.message.is_none());
}

/// `UpdatePortalDestination`: the destination, and the appraisal text "<name> (<coords>)." unless
/// the portal hides its destination.
#[test]
fn portal_destination_text() {
    let mut w = world();
    let p = spawn_portal(&mut w, None);
    let o = w.objects.get_mut(p).unwrap();
    let dest = outdoor(0xACB4, 50.0, 50.0, 20.0);
    portal::update_portal_destination(o, Some(dest));
    let coords =
        empyrean_world::entity::position_extensions::get_map_coord_str(&dest).expect("outdoors");
    assert_eq!(
        o.appraisal_portal_destination(),
        Some(format!("Portal to Somewhere ({coords})."))
    );
    o.set_appraisal_portal_destination(None);
    o.set_portal_show_destination(Some(false));
    portal::update_portal_destination(o, None);
    assert!(o.destination().is_none() && o.appraisal_portal_destination().is_none());
    assert!(!portal::is_gateway(o));
}

/// `Player.GetRotateDelay` is the creature's divided by `RunFactor` 1.5; `FastTick` is `IsPKType`.
#[test]
fn run_factor_and_fast_tick() {
    let mut w = world();
    assert_eq!(player_location::RUN_FACTOR, 1.5);
    assert!(!player_tick::fast_tick(&w, g()));
    w.objects.get_mut(g()).unwrap().set_property(
        PropertyInt::PlayerKillerStatus,
        PlayerKillerStatus::PKLite.0.cast_signed(),
    );
    assert!(player_tick::fast_tick(&w, g()));
    let _ = Vector3::ZERO;
}

// ---- the capture comparison (real-content: reads the recorded sessions) -----------------------

#[cfg(feature = "real-content")]
mod capture {
    //! The movement portion of a recorded ACE session replayed against our server in-process: the
    //! client's MoveToState / AutonomousPosition / Jump game actions, at their recorded times,
    //! from the first recorded position. Our replies about the player (UpdateMotion,
    //! UpdatePosition, VectorUpdate) are compared, as an ordered opcode list, with the recording's
    //! over the same span. Only opcodes and structure are compared; no payload is kept.
    //!
    //! Recordings are parsed once per process by the shared client-net corpus reader.

    use super::*;

    /// A heritage body (the human male setup).
    const HUMAN_SETUP: u32 = 0x0200_0001;

    fn real_dats() -> Arc<empyrean_dat::DatManager> {
        use std::sync::OnceLock;
        static DATS: OnceLock<Arc<empyrean_dat::DatManager>> = OnceLock::new();
        Arc::clone(DATS.get_or_init(|| {
            let dir = dereth_dat::testing::dat_dir();
            let real = empyrean_dat::RealDats::open(&dir).unwrap_or_else(|e| {
                panic!(
                    "the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                    dir.display()
                )
            });
            empyrean_dat::DatManager::initialize(Arc::new(real))
                .expect("the retail dats initialize")
        }))
    }

    const MOVE_TO_STATE: u32 = 0xF61C;
    const AUTONOMOUS_POSITION: u32 = 0xF753;
    const JUMP: u32 = 0xF61B;

    struct Blob {
        t: f64,
        c2s: bool,
        opcode: u32,
        payload: Vec<u8>,
    }

    fn blobs(session: &str) -> Vec<Blob> {
        use dereth_client_net::client_session::testing::{Corpus, Direction};
        Corpus::shared(session)
            .blobs
            .iter()
            .map(|blob| Blob {
                t: std::time::Duration::from_micros(blob.t_rel_micros).as_secs_f64(),
                c2s: blob.dir == Direction::ClientToServer,
                opcode: blob.opcode,
                payload: blob.payload.clone(),
            })
            .collect()
    }

    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// The reply opcodes about `guid`, from the recording or from our capture.
    /// An UpdateMotion is counted only when autonomous (the echo of the client's own movement:
    /// MoveToState and Jump); a server-initiated motion (a stance change, an emote, a recall, a
    /// logout) answers an action this replay does not send. Byte 14 of a `0xF74C` blob is the
    /// movement buffer's `autonomous` flag.
    fn about(guid: u32, opcode: u32, payload: &[u8]) -> Option<u32> {
        if u32_at(payload, 4) != guid {
            return None;
        }
        match opcode {
            UPDATE_MOTION => (payload[14] != 0).then_some(opcode),
            UPDATE_POSITION | VECTOR_UPDATE => Some(opcode),
            _ => None,
        }
    }

    /// The destination of a recorded teleport: the next UpdatePosition about the player (its
    /// PositionPack's cell and origin; the rotation is not needed).
    fn teleport_destination(blobs: &[Blob], from: usize, guid: u32) -> Option<Position> {
        blobs[from..]
            .iter()
            .find(|b| !b.c2s && b.opcode == UPDATE_POSITION && u32_at(&b.payload, 4) == guid)
            .map(|b| {
                let p = &b.payload;
                let f = |at: usize| f32::from_le_bytes(p[at..at + 4].try_into().unwrap());
                Position::from_components(
                    u32_at(p, 12),
                    f(16),
                    f(20),
                    f(24),
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    false,
                )
            })
    }

    fn replay(session: &str) -> (Vec<u32>, Vec<u32>, usize) {
        let blobs = blobs(session);
        let player = blobs
            .iter()
            .find(|b| b.c2s && b.opcode == 0xF657)
            .map(|b| u32_at(&b.payload, 4))
            .expect("an enter world");
        let logoff = blobs
            .iter()
            .find(|b| b.c2s && b.opcode == 0xF653)
            .map_or(f64::INFINITY, |b| b.t);
        let is_movement = |b: &Blob| {
            b.c2s
                && b.opcode == 0xF7B1
                && [MOVE_TO_STATE, AUTONOMOUS_POSITION, JUMP].contains(&u32_at(&b.payload, 8))
        };
        let is_login_complete =
            |b: &Blob| b.c2s && b.opcode == 0xF7B1 && u32_at(&b.payload, 8) == 0x00A1;
        // A teleport the server started for an action this replay does not send (a portal, a
        // recall) is replayed as `Player.Teleport` to the recorded destination.
        let is_teleport = |b: &Blob| !b.c2s && b.opcode == 0xF751;
        let events: Vec<(usize, &Blob)> = blobs
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.t < logoff && (is_movement(b) || is_login_complete(b) || is_teleport(b))
            })
            .collect();
        let actions: Vec<&Blob> = events
            .iter()
            .map(|(_, b)| *b)
            .filter(|b| is_movement(b))
            .collect();
        let t0 = actions.first().expect("movement").t;
        let recorded: Vec<u32> = blobs
            .iter()
            .filter(|b| !b.c2s && b.t >= t0 && b.t < logoff)
            .filter_map(|b| about(player, b.opcode, &b.payload))
            .collect();

        // The first recorded position: the first AutonomousPosition's.
        let first_pos = actions
            .iter()
            .find(|b| u32_at(&b.payload, 8) == AUTONOMOUS_POSITION)
            .map(|b| {
                let p = &b.payload[12..];
                let f = |at: usize| f32::from_le_bytes(p[at..at + 4].try_into().unwrap());
                Position::from_components(
                    u32_at(p, 0),
                    f(4),
                    f(8),
                    f(12),
                    f(20),
                    f(24),
                    f(28),
                    f(16),
                    false,
                )
            })
            .expect("an AutonomousPosition");

        // our world on the retail dats (real land and interiors), a human body there
        let mut w = world_on(None, first_pos, real_dats(), HUMAN_SETUP);
        // as `PlayerEnterWorld` leaves it: in portal space since now
        let now_unix = w.now.unix_time;
        let o = w.objects.get_mut(g()).unwrap();
        o.wo.world_object.teleporting = true;
        o.set_last_teleport_start_timestamp(Some(now_unix));
        not_ported::take_local();

        start_capture();
        let mut ours = Vec::new();
        let mut now = t0;
        let drain = |ours: &mut Vec<u32>| {
            for (_, _, bytes) in take_sent() {
                if let Some(op) = about(P, u32_at(&bytes, 0), &bytes) {
                    ours.push(op);
                }
            }
        };
        // one UpdateGameWorld period: the landblocks' actions and physics (UpdateObjectPhysics).
        // The first heartbeat calls `NotifyLandblocks`
        // (ported here), keeps the player's landblock awake, so it stands in every 5 s.
        let mut heartbeat = 0.0;
        let mut tick = |w: &mut World| {
            w.now.utc = w.now.utc.add_seconds(1.0 / 60.0);
            w.now.unix_time += 1.0 / 60.0;
            w.now.portal_year_ticks += 1.0 / 60.0;
            heartbeat += 1.0 / 60.0;
            if heartbeat >= 5.0 {
                heartbeat = 0.0;
                player_location::notify_landblocks(w, g());
            }
            let t = w.now.portal_year_ticks;
            landblock_manager::tick(w, t);
        };
        for &(index, b) in &events {
            // the world's physics ticks between two recorded actions
            while now + 1.0 / 60.0 < b.t {
                now += 1.0 / 60.0;
                tick(&mut w);
            }
            if is_login_complete(b) {
                player_location::on_teleport_complete(&mut w, g());
            } else if is_teleport(b) {
                if b.t >= t0 {
                    if let Some(dest) = teleport_destination(&blobs, index, player) {
                        player_location::teleport(&mut w, g(), &dest, true);
                    }
                }
            } else {
                handle_client_message(
                    &mut w,
                    ClientMessage::new(b.payload.clone()).expect("opcode"),
                    S,
                );
                run_inbound_message_queue(&mut w);
            }
            drain(&mut ours);
        }
        while now < logoff.min(t0 + 3600.0) && now < actions.last().unwrap().t + 2.0 {
            now += 1.0 / 60.0;
            tick(&mut w);
        }
        drain(&mut ours);
        (recorded, ours, actions.len())
    }

    fn count(v: &[u32], op: u32) -> usize {
        v.iter().filter(|&&o| o == op).count()
    }

    /// Every recorded session: our autonomous UpdateMotions (the echoes of MoveToState and Jump)
    /// and VectorUpdates match the recording's in number. In these sessions the whole ordered
    /// list (with the UpdatePositions) matches, opcode for opcode.
    const EXACT: [&str; 8] = [
        "first-login-walk-jump",
        "short-second-connection",
        "short-play-with-training",
        "melee-attack-run",
        "fellowship-two-monarch",
        "fellowship-three-vassal",
        "house-purchase-refused",
        "pre-relog-play",
    ];
    const ALL: [&str; 16] = [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "long-solo-play",
        "short-play-with-training",
        "combat-mode-while-moving",
        "melee-attack-run",
        "fellowship-one-vassal",
        "fellowship-two-monarch",
        "fellowship-three-vassal",
        "house-purchase-refused",
        "house-purchase-and-trade",
        "requested-death-vitae-salvage",
        "long-movement-run",
        "pre-relog-play",
        "post-relog-attribute-training",
    ];

    #[test]
    fn first_login_walk_jump_movement_replies_match_the_recording() {
        let (recorded, ours, n) = replay("first-login-walk-jump");
        assert!(n > 0, "the recording exercises movement actions");
        assert_eq!(count(&ours, VECTOR_UPDATE), count(&recorded, VECTOR_UPDATE));
        assert_eq!(ours, recorded);
    }

    /// The other sessions' UpdatePositions differ only where ACE moved the player for an action
    /// this replay does not send (a UseItem's move-to chain, an admin teleport, the mansion
    /// recall): the receipt lists them.
    #[test]
    fn every_recorded_sessions_movement_echoes_match() {
        for session in ALL {
            let (recorded, ours, _) = replay(session);
            assert_eq!(
                count(&ours, UPDATE_MOTION),
                count(&recorded, UPDATE_MOTION),
                "{session}: autonomous UpdateMotions"
            );
            assert_eq!(
                count(&ours, VECTOR_UPDATE),
                count(&recorded, VECTOR_UPDATE),
                "{session}: VectorUpdates"
            );
            if EXACT.contains(&session) {
                assert_eq!(ours, recorded, "{session}");
            }
        }
    }
}

/// A PK player jumping in magic combat mode mid-cast has the cast motion cleared out of its
/// interpreted forward command (the motion interpreter's stop) before the cast fails.
#[test]
fn a_pk_jump_mid_cast_stops_the_cast_motion() {
    const READY: u32 = 0x4100_0003;
    const CAST_LIKE: u32 = 0x4400_0007; // any forward command other than Ready
    let mut w = world();
    let h = body(&w);
    {
        let o = w.objects.get_mut(g()).unwrap();
        o.set_player_killer_status_prop(PlayerKillerStatus::PK);
        empyrean_world::world_objects::creature_combat::fields_mut(o).combat_mode =
            empyrean_entity::enums::CombatMode::Magic;
    }
    empyrean_world::world_objects::player_magic::fields_mut(&mut w, g())
        .magic_state
        .is_casting = true;
    phys_ext::get_minterp(&mut w, h, |mi| {
        mi.interpreted_state.forward_command.0 = CAST_LIKE
    })
    .expect("an interpreter");

    let at = outdoor(HOME, 100.0, 100.0, 20.0);
    let m = MovementJump(JumpPack {
        extent: 0.5,
        velocity: WVec3 {
            x: 0.0,
            y: 0.0,
            z: 5.0,
        },
        position: wire(&at),
        timestamps: MoveTimestamps::default(),
    });
    not_ported::take_local();
    action(&mut w, &m);
    assert!(!not_ported::take_local().contains_key("ACE: MotionInterp.StopCompletely"));
    assert_eq!(
        phys_ext::interpreted_state(&w, h)
            .expect("interp")
            .forward_command
            .0,
        READY
    );
    assert!(
        !empyrean_world::world_objects::player_magic::fields(&w, g())
            .magic_state
            .is_casting,
        "the cast failed"
    );
}

mod navigation {
    use crate::support::navigation_world::*;

    /// `Creature.GetAngle(target)`: `Location.ToGlobal()` has `skipIndoors = false`, so two indoor
    /// cells in neighbouring landblocks at the same local spot are 192 m apart east-west: a creature
    /// facing north (identity rotation) has its target at 90 degrees. (With `skipIndoors` the two
    /// positions would coincide and the angle would be 0.)
    #[test]
    fn get_angle_uses_aces_global_positions_between_indoor_landblocks() {
        let mut w = bare_world();
        let (a, b) = (ObjectGuid::new(0x7000_0001), ObjectGuid::new(0x7000_0002));
        w.objects.insert(at(a, 0x0100_0100, 10.0, 10.0)).unwrap();
        w.objects.insert(at(b, 0x0200_0100, 10.0, 10.0)).unwrap();
        let angle = creature_navigation::get_angle(&w, a, b);
        assert!((angle - 90.0).abs() < 1e-3, "{angle}");
    }

    /// `Creature.GetRotateDelay(angle)`: pi / TurnSpeed / 180 * angle; the synthetic motion table turns
    /// at pi/2 rad/s, so 90 degrees take 1 s, through the dispatch.
    #[test]
    fn a_creatures_rotate_delay_is_aces() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        let delay = dispatch::get_rotate_delay::get_rotate_delay(&h.w, MONSTER, 90.0);
        assert!((delay - 1.0).abs() < 1e-5, "{delay}");
        assert_eq!(
            dispatch::get_rotate_delay::get_rotate_delay(&h.w, MONSTER, 0.0),
            0.0
        );
    }
}

mod distance_queries {
    use crate::support::social_world::*;

    /// `Creature.GetDistance` (Creature_Navigation.cs): `Location.DistanceTo(target.Location)`.
    #[test]
    fn creature_get_distance_is_the_location_distance() {
        let mut h = H::small();
        h.player(A, "Alpha", 3);
        h.player(B, "Bravo", 3);
        let mut loc = h.w.objects.get(A).unwrap().location().unwrap();
        loc.position_x += 3.0;
        loc.position_y += 4.0;
        h.w.objects.get_mut(B).unwrap().set_location(Some(loc));
        assert_eq!(
            empyrean_world::world_objects::creature_navigation::get_distance(&h.w, A, B),
            5.0
        );
    }
}
