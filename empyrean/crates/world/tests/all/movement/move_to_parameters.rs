//! Divergence: V257
//! MoveTo broadcasts carry retail's 15.0 threshold and flag word per kind (attack chase, NPC/home
//! move, pet follow, melee charge, player chains) and the body moves as told.
//! Fixture: isolated world state.

// V257.

use dereth_protocol::movement::{MoveToArm, MovementParameters, MovementSetObjectMovement};
use empyrean_entity::enums::{PlayerKillerStatus, PositionType, WeenieType};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_world::dispatch::Class;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::motion::move_to_parameters::{
    RetailMoveTo, RETAIL_WALK_RUN_THRESHOLD,
};
use empyrean_world::world_objects::{
    creature_navigation as cn, monster, monster_combat, monster_navigation as nav, pet,
    player_move, player_move2,
};

use super::monster_ai::{H, MONSTER, PLAYER};

const UPDATE_MOTION: u32 = 0xF74C;
const TARGET: ObjectGuid = ObjectGuid::new(0x8000_0200);

const ATTACK_CHASE: u32 = 0x1_EFF0;
const PLAIN: u32 = 0x1_EE0F;
const USE: u32 = 0x1_EE4F;
const PORTAL: u32 = 0x1_EA4F;

/// Which MoveTo a decoded UpdateMotion carried.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Kind {
    Object,
    Position,
}

/// The MoveTos about `about` the capture holds: (object or position, flag word, threshold).
fn move_tos(about: ObjectGuid) -> Vec<(Kind, u32, f32)> {
    take_sent()
        .into_iter()
        .filter(|(_, _, b)| b[..4] == UPDATE_MOTION.to_le_bytes())
        .map(|(_, _, b)| {
            dereth_protocol::read_body_padded::<MovementSetObjectMovement>(&b[4..])
                .expect("UpdateMotion decodes")
        })
        .filter(|m| m.id.0 == about.full())
        .filter_map(
            |m| match m.decoded_movement().unwrap().body.decode_move_to().unwrap() {
                Some(MoveToArm::MoveToObject {
                    params:
                        MovementParameters::MoveTo {
                            bitfield,
                            walk_run_threshold,
                            ..
                        },
                    ..
                }) => Some((Kind::Object, bitfield, walk_run_threshold)),
                Some(MoveToArm::MoveToPosition {
                    params:
                        MovementParameters::MoveTo {
                            bitfield,
                            walk_run_threshold,
                            ..
                        },
                    ..
                }) => Some((Kind::Position, bitfield, walk_run_threshold)),
                _ => None,
            },
        )
        .collect()
}

/// The player at (100, 100) and a monster 20 m east (past ACE's 7.5 m CanCharge line).
fn player_and_monster() -> H {
    let mut h = H::new();
    empyrean_world::managers::landblock_manager::get_landblock(
        &mut h.w,
        super::monster_ai::lb_id(),
        false,
        false,
    );
    h.player(100.0, 100.0);
    h.monster(120.0, 100.0);
    h
}

/// A second object 20 m north of the player, of weenie type `weenie_type`.
fn target(h: &mut H, weenie_type: WeenieType) {
    h.creature(Class::Creature, TARGET, "Target", 100.0, 120.0);
    h.w.objects.get_mut(TARGET).unwrap().biota.weenie_type = weenie_type;
    h.place(TARGET);
}

#[test]
fn the_table_of_retail_flag_words() {
    assert_eq!(RetailMoveTo::AttackChase.flags(), ATTACK_CHASE);
    assert_eq!(RetailMoveTo::AttackChaseUnstuck.flags(), 0x1_EF70);
    assert_eq!(RetailMoveTo::Plain.flags(), PLAIN);
    assert_eq!(RetailMoveTo::Use.flags(), USE);
    assert_eq!(RetailMoveTo::Portal.flags(), PORTAL);
    assert_eq!(RETAIL_WALK_RUN_THRESHOLD, 15.0);
    for kind in [
        RetailMoveTo::AttackChase,
        RetailMoveTo::Plain,
        RetailMoveTo::Use,
        RetailMoveTo::Portal,
    ] {
        let p = kind.movement_parameters();
        assert_eq!(
            (p.flags, p.walk_run_threshold),
            (kind.flags(), 15.0),
            "{kind:?}"
        );
    }
}

/// A monster's chase (`Creature.MoveTo(target)`): the attack chase on the wire and in the body.
#[test]
fn a_monster_chase_is_the_attack_chase() {
    let mut h = player_and_monster();
    start_capture();
    cn::creature_move_to(&mut h.w, MONSTER, PLAYER, 1.0);
    assert_eq!(
        move_tos(MONSTER),
        [(Kind::Object, ATTACK_CHASE, 15.0)],
        "ACE: 0x1EFFF, 1.0"
    );
    let motion = h
        .o(MONSTER)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
        .unwrap();
    assert_eq!(
        motion.move_to_parameters.movement_parameters.0,
        ATTACK_CHASE
    );

    monster_combat::set_attack_target(&mut h.w, MONSTER, Some(PLAYER));
    let mvp = nav::get_movement_parameters(&mut h.w, MONSTER);
    assert_eq!(
        (mvp.flags, mvp.walk_run_threshold),
        (ATTACK_CHASE, 15.0),
        "ACE: 0x1EFFE, 1.0"
    );

    // a newcomer is sent the same chase (`BroadcastMoveTo`)
    start_capture();
    cn::creature_broadcast_move_to(&mut h.w, MONSTER, PLAYER);
    assert_eq!(move_tos(MONSTER), [(Kind::Object, ATTACK_CHASE, 15.0)]);
}

/// A monster going home (`MoveToHome`), and a newcomer seeing it: an NPC move, threshold 15.
#[test]
fn a_monster_going_home_is_an_npc_move() {
    let mut h = player_and_monster();
    let home =
        Position::from_components(0xA9B4_0001, 130.0, 100.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .set_position(PositionType::Home, Some(home));
    // a monster that can run (at a run rate of 0, ACE's clear of CanRun stands: 0x1EE4D)
    nav::fields_mut(&mut h.w, MONSTER).run_rate = 1.0;
    start_capture();
    nav::move_to_home(&mut h.w, MONSTER);
    assert_eq!(
        move_tos(MONSTER),
        [(Kind::Position, USE, 15.0)],
        "ACE: 0x1EE4F, 1.0"
    );

    start_capture();
    cn::creature_broadcast_move_to(&mut h.w, MONSTER, PLAYER);
    assert_eq!(move_tos(MONSTER), [(Kind::Position, USE, 15.0)]);
}

/// An NPC's emote move (`Creature.MoveTo(position, ..., speed)`): unchanged, already retail's.
#[test]
fn an_npc_move_uses_the_final_heading() {
    let mut h = player_and_monster();
    let to = Position::from_components(0xA9B4_0001, 125.0, 105.0, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    start_capture();
    cn::creature_move_to_position(&mut h.w, MONSTER, &to, 1.0, false, None, Some(1.2));
    assert_eq!(move_tos(MONSTER), [(Kind::Position, USE, 15.0)]);
}

/// A passive pet following its owner (`Pet.MoveTo`): the attack chase (ACE: 0x1EE1F, 0.0).
#[test]
fn a_pet_follows_with_the_attack_chase() {
    let mut h = player_and_monster();
    monster::fields_mut(&mut h.w, MONSTER).is_passive_pet = true;
    start_capture();
    pet::pet_move_to(&mut h.w, MONSTER, PLAYER, 1.0);
    assert_eq!(move_tos(MONSTER), [(Kind::Object, ATTACK_CHASE, 15.0)]);
}

/// The player's melee charge (`Player.MoveTo`): the attack chase on the wire and in the body.
#[test]
fn the_players_melee_charge_is_the_attack_chase() {
    let mut h = player_and_monster();
    start_capture();
    player_move::player_move_to(&mut h.w, PLAYER, MONSTER, 0.0);
    assert_eq!(
        move_tos(PLAYER),
        [(Kind::Object, ATTACK_CHASE, 15.0)],
        "ACE: 0x1EFFF, 1.0"
    );
    let mvp = player_move::get_charge_parameters();
    assert_eq!(
        (mvp.flags, mvp.walk_run_threshold),
        (ATTACK_CHASE, 15.0),
        "ACE: 0x1EFFE, 1.0"
    );
}

/// The player's move-to chains: a use-move (never CanCharge, even 20 m away), an item pickup,
/// a melee approach and a portal, on the legacy chain.
#[test]
fn the_players_move_to_chains_send_the_kind_of_move() {
    for (weenie_type, kind, expected) in [
        (WeenieType::Creature, RetailMoveTo::Use, (Kind::Object, USE)),
        (
            WeenieType::Generic,
            RetailMoveTo::Plain,
            (Kind::Object, PLAIN),
        ),
        (
            WeenieType::Creature,
            RetailMoveTo::AttackChase,
            (Kind::Object, ATTACK_CHASE),
        ),
        (
            WeenieType::Portal,
            RetailMoveTo::Use,
            (Kind::Position, PORTAL),
        ),
    ] {
        let mut h = player_and_monster();
        target(&mut h, weenie_type);
        start_capture();
        player_move::create_move_to_chain_as(
            &mut h.w,
            PLAYER,
            TARGET,
            Box::new(|_, _| {}),
            None,
            true,
            kind,
        );
        assert_eq!(
            move_tos(PLAYER),
            [(expected.0, expected.1, 15.0)],
            "{weenie_type:?} {kind:?} (ACE: 0x1EE0F or 0x1EE1F)"
        );
    }
    // `CreateMoveToChain` itself is a use-move
    let mut h = player_and_monster();
    target(&mut h, WeenieType::Chest);
    start_capture();
    player_move::create_move_to_chain(&mut h.w, PLAYER, TARGET, Box::new(|_, _| {}), None, true);
    assert_eq!(move_tos(PLAYER), [(Kind::Object, USE, 15.0)]);
}

/// The same on the full-physics chain (a PK player): the wire and the body's parameters.
#[test]
fn the_full_physics_chain_moves_the_body_as_the_clients_are_told() {
    for (weenie_type, kind, expected) in [
        (WeenieType::Chest, RetailMoveTo::Use, USE),
        (WeenieType::Generic, RetailMoveTo::Plain, PLAIN),
        (WeenieType::Portal, RetailMoveTo::Use, PORTAL),
    ] {
        let mut h = player_and_monster();
        h.w.objects
            .get_mut(PLAYER)
            .unwrap()
            .set_player_killer_status_prop(PlayerKillerStatus::PK);
        target(&mut h, weenie_type);
        start_capture();
        player_move::create_move_to_chain_as(
            &mut h.w,
            PLAYER,
            TARGET,
            Box::new(|_, _| {}),
            None,
            true,
            kind,
        );
        assert_eq!(
            move_tos(PLAYER),
            [(Kind::Object, expected, 15.0)],
            "{weenie_type:?}"
        );
    }
    let mut h = player_and_monster();
    target(&mut h, WeenieType::Chest);
    let mvp = player_move2::get_move_to_params(&h.w, PLAYER, TARGET, Some(2.0), RetailMoveTo::Use);
    assert_eq!(
        (mvp.flags, mvp.walk_run_threshold, mvp.distance_to_object),
        (USE, 15.0, 2.0),
        "ACE: 0x1EE1F less CanRun, 1.0"
    );
}
