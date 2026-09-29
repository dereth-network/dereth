// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/GamePiece.cs
//! Port of `Source/ACE.Server/WorldObjects/GamePiece.cs`: a chess piece's weenie (a Creature). Its
//! monster tick walks it to its square (or to a captured piece, fights it, then walks on) and tells
//! the match when it is ready.

use dereth_animation::motion::flags as mvp_flags;
use empyrean_entity::enums::{
    GamePieceState, MotionCommand, MotionStance, MovementParams, PlayScript, WeenieError,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::chess::chess_match::{self, ChessMatchRef};
use crate::entity::landblock;
use crate::managers::player_manager;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_update_motion::game_message_update_motion;
use crate::network::motion::movement_data::{Motion, MovementData};
use crate::physics::phys_ext;
use crate::world_objects::creature_navigation::{a_frame_get_heading, MONSTER_TICK_INTERVAL};
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, monster, monster_awareness, monster_combat, monster_melee, monster_navigation,
    monster_tick, world_object, world_object_networking,
};
use crate::World;

/// Non-property fields declared in `GamePiece.cs`.
#[derive(Debug, Default)]
pub struct GamePieceFields {
    // ACE: GamePiece.ChessMatch
    pub chess_match: Option<ChessMatchRef>,

    // ACE: GamePiece.GamePieceState
    pub game_piece_state: GamePieceState,
    // ACE: GamePiece.Position
    /// The square being walked to.
    pub position: Option<Position>,
    // ACE: GamePiece.TargetPiece
    /// The piece being captured (`CurrentLandblock.GetObject(victim) as GamePiece`).
    pub target_piece: Option<ObjectGuid>,

    // ACE: GamePiece.LastMoveTo
    pub last_move_to: Option<Motion>,
}

fn fields_of(o: &WorldObject) -> &GamePieceFields {
    match &o.kind {
        KindData::GamePiece(d) => &d.game_piece,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a GamePiece",
            o.guid.full()
        ),
    }
}

fn fields_of_mut(o: &mut WorldObject) -> &mut GamePieceFields {
    let guid = o.guid;
    match &mut o.kind {
        KindData::GamePiece(d) => &mut d.game_piece,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a GamePiece",
            guid.full()
        ),
    }
}

/// This piece's fields.
///
/// # Panics
/// For a missing object or one that is not a GamePiece.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &GamePieceFields {
    fields_of(
        w.objects
            .get(this)
            .expect("ACE: GamePiece is null (NullReferenceException)"),
    )
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut GamePieceFields {
    fields_of_mut(
        w.objects
            .get_mut(this)
            .expect("ACE: GamePiece is null (NullReferenceException)"),
    )
}

/// `(wo as GamePiece).ChessMatch = value`: a weenie that is not a GamePiece is ACE's
/// `NullReferenceException`.
pub fn set_chess_match_checked(w: &mut World, this: ObjectGuid, value: Option<ChessMatchRef>) {
    let o = w
        .objects
        .get_mut(this)
        .filter(|o| o.is_game_piece())
        .expect("ACE: (wo as GamePiece) is null (NullReferenceException)");
    let old = std::mem::replace(&mut fields_of_mut(o).chess_match, value);
    w.chess_matches.replace_reference(old, value);
}

// ACE: GamePiece.Kill
/// Plays the death motion, 5 s later the destroy effect, 1 s after that removes the piece.
pub fn kill(w: &mut World, this: ObjectGuid) {
    let mut kill_chain = ActionChain::new();
    kill_chain.add_action(Actor::Object(this), move |w| {
        world_object_networking::enqueue_broadcast_motion(
            w,
            this,
            &Motion::new(MotionStance::NonCombat, MotionCommand::Dead, 1.0),
            None,
            None,
        );
    });
    kill_chain.add_delay_seconds(w, 5.0);
    kill_chain.add_action(Actor::Object(this), move |w| {
        world_object::apply_visual_effects(w, this, PlayScript::Destroy, 1.0);
    });
    kill_chain.add_delay_seconds(w, 1.0);
    kill_chain.add_action(Actor::Object(this), move |w| {
        world_object::destroy(w, this, true, false)
    });
    kill_chain.enqueue_chain(w);
}

// ACE: GamePiece.MoveEnqueue
pub fn move_enqueue(w: &mut World, this: ObjectGuid, dest: Position) {
    let f = fields_mut(w, this);
    f.game_piece_state = GamePieceState::MoveToSquare;
    f.position = Some(dest);
}

// ACE: GamePiece.AttackEnqueue
pub fn attack_enqueue(w: &mut World, this: ObjectGuid, dest: Position, victim: ObjectGuid) {
    {
        let f = fields_mut(w, this);
        f.game_piece_state = GamePieceState::MoveToAttack;
        f.position = Some(dest);
    }
    let lb = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: GamePiece.CurrentLandblock is null (NullReferenceException)");
    let target = landblock::get_object(w, lb, victim, true)
        .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_game_piece));
    fields_mut(w, this).target_piece = target;
}

/// `PhysicsObj.GetRadius()` of a world object's body.
fn radius(w: &World, g: Option<ObjectGuid>) -> f32 {
    let h = g
        .and_then(|g| phys_ext::physics_obj(w, g))
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    w.physics
        .get(h)
        .expect("ACE: PhysicsObj is null (NullReferenceException)")
        .radius()
}

// ACE: GamePiece.Tick
/// The piece's monster tick (every `monsterTickInterval`): start the walk or the walk-to-attack,
/// step it, report ready once the walk is done, or swing at the target.
pub fn tick(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    monster_tick::fields_mut(w, this).next_monster_tick_time =
        current_unix_time + MONSTER_TICK_INTERVAL;

    let state = fields(w, this).game_piece_state;
    match state {
        GamePieceState::MoveToSquare => {
            fields_mut(w, this).game_piece_state = GamePieceState::WaitingForMoveToSquare;
            let position = fields(w, this)
                .position
                .expect("ACE: GamePiece.Position is null (NullReferenceException)");
            move_weenie(w, this, &position, 0.3, true);
        }

        // visual awareness range of piece is only 1, make sure we are close enough to attack
        GamePieceState::MoveToAttack => {
            fields_mut(w, this).game_piece_state = GamePieceState::WaitingForMoveToAttack;
            let (position, target) = {
                let f = fields(w, this);
                (
                    f.position
                        .expect("ACE: GamePiece.Position is null (NullReferenceException)"),
                    f.target_piece,
                )
            };
            let distance = radius(w, Some(this)) + radius(w, target);
            move_weenie(w, this, &position, distance, false);
        }

        GamePieceState::WaitingForMoveToSquare | GamePieceState::WaitingForMoveToAttack => {
            monster_navigation::update_position(w, this, true);
        }

        GamePieceState::WaitingForMoveToSquareAnimComplete => {
            /*if (PhysicsObj.IsAnimating)
            {
                UpdatePosition();
                break;
            }*/

            let m = fields(w, this)
                .chess_match
                .expect("ACE: GamePiece.ChessMatch is null (NullReferenceException)");
            chess_match::piece_ready(w, &m, this);
            if let Some(o) = w.objects.get_mut(this) {
                fields_of_mut(o).game_piece_state = GamePieceState::None;
            }
        }

        GamePieceState::Combat => {
            let has_table = w
                .objects
                .get(this)
                .is_some_and(|o| creature_combat::fields(o).combat_table.is_some());
            if !has_table {
                monster_combat::get_combat_table(w, this);
            }

            monster_melee::melee_attack(w, this);
        }
        _ => {}
    }
}

// ACE: GamePiece.OnMoveComplete
pub fn game_piece_on_move_complete(w: &mut World, this: ObjectGuid, status: WeenieError) {
    //Console.WriteLine($"{Name}.OnMoveComplete({status})");

    monster_navigation::creature_on_move_complete(w, this, status);

    if status != WeenieError::None {
        return;
    }

    let state = fields(w, this).game_piece_state;
    match state {
        // we are done, tell the match so the turn can finish
        GamePieceState::WaitingForMoveToSquare => {
            fields_mut(w, this).game_piece_state =
                GamePieceState::WaitingForMoveToSquareAnimComplete;
        }

        // there is another piece on this square, attack it!
        GamePieceState::WaitingForMoveToAttack => {
            let target = fields(w, this).target_piece;
            monster_combat::set_attack_target(w, this, target);
            fields_mut(w, this).game_piece_state = GamePieceState::Combat;
        }
        _ => {}
    }
}

// ACE: GamePiece.OnDealtDamage
/// weenie piece is dead, time to move into the square completely
pub fn on_dealt_damage(w: &mut World, this: ObjectGuid) {
    let target = fields(w, this).target_piece;
    let dead = target
        .and_then(|t| w.objects.get(t))
        .map(monster_combat::is_dead);
    // `TargetPiece.IsDead`: a null target throws in ACE
    if dead.expect("ACE: GamePiece.TargetPiece is null (NullReferenceException)") {
        fields_mut(w, this).game_piece_state = GamePieceState::MoveToSquare;
    }
}

// ACE: GamePiece.MoveWeenie
/// Starts the server-side MoveToPosition to `to` (stopping `distance_to_object` short, facing its
/// heading with `final_heading`) and broadcasts the MoveToPosition motion.
pub fn move_weenie(
    w: &mut World,
    this: ObjectGuid,
    to: &Position,
    distance_to_object: f32,
    final_heading: bool,
) {
    if monster_navigation::fields(w, this).move_speed == 0.0 {
        monster_navigation::get_movement_speed(w, this);
    }

    let mut move_to_position = Motion::to_position(w, this, to);
    move_to_position.move_to_parameters.distance_to_object = distance_to_object;
    move_to_position.move_to_parameters.movement_parameters &= !MovementParams::UseSpheres;

    if final_heading {
        move_to_position.move_to_parameters.movement_parameters |= MovementParams::UseFinalHeading;
    }

    let phys_pos = phys_ext::to_physics_position(to);
    let desired_heading = a_frame_get_heading(to.rotation());
    move_to_position.move_to_parameters.desired_heading = desired_heading;

    crate::world_objects::player_move::creature_set_walk_run_threshold(
        w,
        this,
        &mut move_to_position,
        to,
    );

    let mut mvp = monster_navigation::get_movement_parameters(w, this);
    mvp.flags |= mvp_flags::CAN_WALK;
    mvp.flags |= mvp_flags::STOP_COMPLETELY;
    mvp.flags &= !mvp_flags::USE_SPHERES;
    mvp.distance_to_object = distance_to_object;
    if final_heading {
        mvp.flags |= mvp_flags::USE_FINAL_HEADING;
    } else {
        mvp.flags &= !mvp_flags::USE_FINAL_HEADING;
    }
    mvp.desired_heading = desired_heading;

    let h = monster_navigation::physics_obj(w, this);
    phys_ext::move_to_position(w, h, &phys_pos, &mvp);
    monster_navigation::fields_mut(w, this).is_moving = true;
    monster::set_monster_state_value(w, this, monster::State::Awake);
    monster_awareness::fields_mut(w, this).is_awake = true;

    fields_mut(w, this).last_move_to = Some(move_to_position.clone());

    world_object_networking::enqueue_broadcast_motion(w, this, &move_to_position, None, None);
}

// ACE: GamePiece.BroadcastMoveTo
/// Sends the piece's last MoveTo motion to a player who has just started seeing it.
pub fn game_piece_broadcast_move_to(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    let last = fields(w, this)
        .last_move_to
        .clone()
        .expect("ACE: GamePiece.LastMoveTo is null (NullReferenceException)");
    let movement_data = MovementData::from_motion(this, &last);
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let msg = game_message_update_motion(o, &movement_data);
    let session = player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    enqueue_send(w, session, msg);
}

// ---- constructors and SetEphemeralValues ----

/// `new GamePiece(weenie, guid)` / `new GamePiece(biota)`: the `Creature` constructor, then
/// GamePiece's `SetEphemeralValues`.
// ACE: GamePiece.GamePiece
pub fn game_piece_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::creature::creature_ctor(o, env, src);
    game_piece_set_ephemeral_values(o, env);
}

// ACE: GamePiece.SetEphemeralValues
fn game_piece_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.set_time_to_rot(Some(-1.0));
}
