// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Navigation.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Navigation.cs`.

use dereth_animation::motion::{HoldKey, MovementParameters};
use dereth_physics::PhysHandle;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    MotionCommand, MovementParams, MovementType, PositionType, WeenieError,
};
use empyrean_entity::{LandblockId, ObjectGuid, Position};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::managers::landblock_manager;
use crate::network::motion::move_to_parameters::{
    MoveToParameters, RetailMoveTo, RETAIL_WALK_RUN_THRESHOLD,
};
use crate::network::motion::movement_data::Motion;
use crate::physics::phys_ext;
use empyrean_common::dotnet::Vector3;

use crate::dispatch;
use crate::entity::position_extensions::to_global;
use crate::world_objects::monster_combat;
use crate::world_objects::monster_navigation::estimate_turn_to;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::{
    enqueue_broadcast_motion, send_update_position,
};
use crate::World;

/// Non-property fields declared in `Creature_Navigation.cs`.
#[derive(Debug, Default)]
pub struct CreatureNavigationFields {}

// ---- virtual-dispatch targets ----

/// Starts rotating a creature from its current direction so that it eventually is facing the
/// target position. Returns the amount of time in seconds for the rotation to complete.
// ACE: Creature.Rotate
///
/// Not ACE's (a fix, V318): when the two objects stand at exactly the same
/// global point (two characters on the spot where new characters enter the world) there is no
/// direction to face, and the creature keeps its rotation. ACE normalized the zero vector (all
/// NaN) and set an all-NaN Location rotation, which reached clients and the save until the next
/// move replaced it.
pub fn creature_rotate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) -> f32 {
    if w.objects
        .get(target)
        .and_then(WorldObject::location)
        .is_none()
    {
        return 0.0;
    }

    // send network message to start turning creature
    turn_to_object(w, this, target, true);

    let angle = get_angle(w, this, target);
    //Console.WriteLine("Angle: " + angle);

    // estimate time to rotate to target
    let rotate_delay = dispatch::get_rotate_delay::get_rotate_delay(w, this, angle);
    //Console.WriteLine("RotateTime: " + rotateTime);

    // update server object rotation on completion
    // TODO: proper incremental rotation
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(rotate_delay));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        let Some(target_loc) = w.objects.get(target).and_then(WorldObject::location) else {
            return;
        };

        //var matchIndoors = Location.Indoors == target.Location.Indoors;

        //var globalLoc = matchIndoors ? Location.ToGlobal() : Location.Pos;
        //var targetLoc = matchIndoors ? target.Location.ToGlobal() : target.Location.Pos;
        let global_loc = to_global(&location(w, this), false);
        let target_loc = to_global(&target_loc, false);

        // the same point: nothing to face
        if global_loc == target_loc {
            return;
        }

        let target_dir = get_direction(global_loc, target_loc);

        w.objects
            .get_mut(this)
            .and_then(|o| o.get_position_mut(PositionType::Location))
            .expect("ACE: Location is null (NullReferenceException)")
            .rotate(target_dir);
    });
    action_chain.enqueue_chain(w);

    rotate_delay
}

/// Sends a TurnToObject command to the client (`stopCompletely` defaults to true).
// ACE: Creature.TurnToObject
pub fn turn_to_object(w: &mut World, this: ObjectGuid, target: ObjectGuid, stop_completely: bool) {
    let mut turn_to_motion = Motion::to_object(w, this, target, MovementType::TurnToObject);

    if !stop_completely {
        turn_to_motion.move_to_parameters.movement_parameters &= !MovementParams::StopCompletely;
    }

    enqueue_broadcast_motion(w, this, &turn_to_motion, None, None);
}

// ACE: Creature.GetRotateDelay
/// Returns the amount of time for this creature to rotate by the # of degrees from the input
/// angle, using the omega speed from its MotionTable.
///
/// # Panics
/// When `this` is gone (ACE: `NullReferenceException`).
#[must_use]
pub fn creature_get_rotate_delay(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    angle: f32,
) -> f32 {
    use empyrean_common::dotnet::cast::CsCast;
    let turn_speed = crate::physics::motion_table::get_turn_speed(
        w,
        w.objects.get(this).expect("ACE: this").motion_table_id(),
    );
    if turn_speed == 0.0 {
        return 0.0;
    }

    let rotate_time =
        std::f64::consts::PI / f64::from(turn_speed) / f64::from(180.0f32) * f64::from(angle);
    rotate_time.cs_cast()
}

/// Used by the monster AI system to start turning / running towards a target: the MoveToObject
/// motion becomes the current motion and is broadcast (`MoveTo(WorldObject target, float
/// runRate = 1.0f)`).
// ACE: Creature.MoveTo
pub fn creature_move_to(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    run_rate: f32,
) {
    // `if (DebugMove) Console.WriteLine(...)`: Monster_Navigation's per-object debug flag, never set

    let motion = get_move_to_motion(w, this, target, run_rate);

    set_current_motion_state(w, this, motion.clone());

    enqueue_broadcast_motion(w, this, &motion, None, None);
}

/// A MoveToObject motion towards `target`: the attack chase (`0x1EFF0`, walk/run threshold 15);
/// `run_rate` when positive, else no running.
// ACE: Creature.GetMoveToMotion
#[must_use]
pub fn get_move_to_motion(
    w: &crate::World,
    this: ObjectGuid,
    target: ObjectGuid,
    run_rate: f32,
) -> Motion {
    let mut motion = Motion::to_object(w, this, target, MovementType::MoveToObject);
    // Not ACE's (the retail captures, V257): retail's attack chase clears CanWalk, CanRun,
    // CanSideStep and CanWalkBackwards and keeps the threshold at 15.0; ACE adds CanCharge,
    // FailWalk, UseFinalHeading, Sticky and MoveAway to the defaults (0x1EFFF) with 1.0.
    RetailMoveTo::AttackChase.apply(&mut motion.move_to_parameters);

    if run_rate > 0.0 {
        motion.run_rate = run_rate;
    } else {
        motion.move_to_parameters.movement_parameters &= !MovementParams::CanRun;
    }

    motion
}

/// Not ACE's (retail, V336): a ranged or casting monster's run to within
/// `distance_to_object` of its target before it attacks: the MoveToObject motion becomes the
/// current motion and is broadcast, as [`creature_move_to`] does for the attack chase.
pub fn creature_close_to(
    w: &mut crate::World,
    this: ObjectGuid,
    target: ObjectGuid,
    run_rate: f32,
    distance_to_object: f32,
) {
    let motion = get_close_to_motion(w, this, target, run_rate, distance_to_object);

    set_current_motion_state(w, this, motion.clone());

    enqueue_broadcast_motion(w, this, &motion, None, None);
}

/// Not ACE's (retail, V336): the MoveToObject motion of a ranged or casting
/// monster closing to its attack range: the attack chase without Sticky (`0x1EF70`, walk/run
/// threshold 15), stopping `distance_to_object` from the target, with no minimum distance and no
/// fail distance.
#[must_use]
pub fn get_close_to_motion(
    w: &crate::World,
    this: ObjectGuid,
    target: ObjectGuid,
    run_rate: f32,
    distance_to_object: f32,
) -> Motion {
    let mut motion = get_move_to_motion(w, this, target, run_rate);
    RetailMoveTo::AttackChaseUnstuck.apply(&mut motion.move_to_parameters);
    motion.move_to_parameters.distance_to_object = distance_to_object;
    motion.move_to_parameters.min_distance = 0.0;
    motion.move_to_parameters.fail_distance = f32::MAX;
    motion
}

/// Sends a network message for moving a creature to a new position and, with `set_loc`, starts
/// the MoveTo on the server's physics body and its tick (`MoveTo(Position position, float runRate
/// = 1.0f, bool setLoc = true, float? walkRunThreshold = null, float? speed = null)`).
///
/// # Panics
/// With `set_loc` and no physics body (ACE: `NullReferenceException` on `PhysicsObj`).
// ACE: Creature.MoveTo
pub fn creature_move_to_position(
    w: &mut crate::World,
    this: ObjectGuid,
    position: &Position,
    run_rate: f32,
    set_loc: bool,
    walk_run_threshold: Option<f32>,
    speed: Option<f32>,
) {
    // build and send MoveToPosition message to client
    let motion = get_move_to_position(w, this, position, run_rate, walk_run_threshold, speed);
    enqueue_broadcast_motion(w, this, &motion, None, None);

    if !set_loc {
        return;
    }

    // start executing MoveTo iterator on server
    let h = physics_obj(w, this);
    // `if (!PhysicsObj.IsMovingOrAnimating) PhysicsObj.UpdateTime = PhysicsTimer.CurrentTime;`
    phys_ext::restart_clock_if_idle(w, h);

    let mvp = movement_parameters_from(&motion.move_to_parameters);
    phys_ext::move_to_position(w, h, &phys_ext::to_physics_position(position), &mvp);

    add_move_to_tick(w, this);
}

/// Every `monsterTickInterval` while the body is moving to: an `update_object`, the location
/// synced from physics and an update position; after 5 failed progress checks the MoveTo is
/// cancelled and the creature is broadcast back to Ready.
// ACE: Creature.AddMoveToTick
fn add_move_to_tick(w: &mut crate::World, this: ObjectGuid) {
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, MONSTER_TICK_INTERVAL);
    action_chain.add_action(Actor::Object(this), move |w| {
        let Some(o) = w.objects.get(this) else { return };
        let is_dead = o.health().current(o) == 0;
        let body = o
            .phys
            .filter(|&h| crate::physics::motion::has_movement_manager(w, h));
        let Some(h) = body.filter(|&h| !is_dead && phys_ext::is_moving_to(w, h)) else {
            return;
        };

        phys_ext::update_object(w, h);
        update_position_sync_location(w, this);
        send_update_position(w, this, false);

        if phys_ext::move_to_fail_progress_count(w, h) < 5 {
            add_move_to_tick(w, this);
        } else {
            phys_ext::cancel_move_to(w, h, WeenieError::ActionCancelled);
            phys_ext::set_move_to_fail_progress_count(w, h, 0);

            let stance = w
                .objects
                .get(this)
                .and_then(|o| o.wo.world_object_properties.current_motion_state.as_ref())
                .expect("ACE: CurrentMotionState is null (NullReferenceException)")
                .stance;
            enqueue_broadcast_motion(
                w,
                this,
                &Motion::new(stance, MotionCommand::Ready, 1.0),
                None,
                None,
            );
        }

        //Console.WriteLine($"{Name}.Position: {Location}");
    });
    action_chain.enqueue_chain(w);
}

/// A MoveToPosition motion: the optional walk/run threshold and speed, the final heading of
/// `position` (always used), distance 0.6; `run_rate` when positive, else no running.
// ACE: Creature.GetMoveToPosition
#[must_use]
pub fn get_move_to_position(
    w: &crate::World,
    this: ObjectGuid,
    position: &Position,
    run_rate: f32,
    walk_run_threshold: Option<f32>,
    speed: Option<f32>,
) -> Motion {
    // TODO: change parameters to accept an optional MoveToParameters

    let mut motion = Motion::to_position(w, this, position);
    motion.movement_type = MovementType::MoveToPosition;
    //motion.Flag |= MovementParams.CanCharge | MovementParams.FailWalk | MovementParams.UseFinalHeading | MovementParams.MoveAway;
    if let Some(walk_run_threshold) = walk_run_threshold {
        motion.move_to_parameters.walk_run_threshold = walk_run_threshold;
    }
    if let Some(speed) = speed {
        motion.move_to_parameters.speed = speed;
    }

    // always use final heading?
    motion.move_to_parameters.desired_heading = a_frame_get_heading(position.rotation());
    motion.move_to_parameters.movement_parameters |= MovementParams::UseFinalHeading;
    motion.move_to_parameters.distance_to_object = 0.6f32;

    if run_rate > 0.0 {
        motion.run_rate = run_rate;
    } else {
        motion.move_to_parameters.movement_parameters &= !MovementParams::CanRun;
    }

    motion
}

/// `new AFrame(position.Pos, position.Rotation).get_heading()`: the heading in degrees (0 north,
/// clockwise) from the rotation matrix's second row, in ACE's float and double steps.
// ACE: AFrame.get_heading
#[must_use]
pub fn a_frame_get_heading(orientation: empyrean_common::dotnet::Quaternion) -> f32 {
    // `Matrix4x4.CreateFromQuaternion(Orientation)`: M21 and M22
    let (x, y, z, w) = (orientation.x, orientation.y, orientation.z, orientation.w);
    let xx = x * x;
    let zz = z * z;
    let xy = x * y;
    let wz = z * w;
    let m21 = 2.0f32 * (xy - wz);
    let m22 = 1.0f32 - 2.0f32 * (zz + xx);

    let heading: f32 = empyrean_common::math::atan2(f64::from(m22), f64::from(m21)).cs_cast();
    // `ToDegrees`: `(float)(180.0f / Math.PI * rads)`
    let degrees: f32 = (180.0f64 / std::f64::consts::PI * f64::from(heading)).cs_cast();
    (450.0f32 - degrees) % 360.0f32
}

/// `new MovementParameters(MoveToParameters mvp)`: the flags, distances, heading, speed and
/// walk/run threshold of the network parameters; the context, hold key and action stamp at their
/// field defaults (0, `HoldKey.Invalid`, 0).
// ACE: MovementParameters.MovementParameters
fn movement_parameters_from(mvp: &MoveToParameters) -> MovementParameters {
    MovementParameters {
        flags: mvp.movement_parameters.0,
        distance_to_object: mvp.distance_to_object,
        min_distance: mvp.min_distance,
        desired_heading: mvp.desired_heading,
        speed: mvp.speed,
        fail_distance: mvp.fail_distance,
        walk_run_threshold: mvp.walk_run_threshold,
        context_id: 0,
        hold_key_to_apply: HoldKey::Invalid,
        action_stamp: 0,
    }
}

// ---------------------------------------------------------------------------------------------
// Not this file's: members of `Monster_Tick.cs` and `Monster_Navigation.cs` (not ported yet) that
// the MoveTo tick calls, ported here in full.
// ---------------------------------------------------------------------------------------------

/// `Creature.monsterTickInterval` (`Monster_Tick.cs`, `protected const double`).
// ACE: Creature.monsterTickInterval
pub const MONSTER_TICK_INTERVAL: f64 = 0.2;

/// Synchronizes the WorldObject Location with the Physics Location (a landblock change relocates
/// the object for physics).
// ACE: Creature.UpdatePosition_SyncLocation
pub fn update_position_sync_location(w: &mut crate::World, this: ObjectGuid) {
    // was the position successfully moved to?
    // use the physics position as the source-of-truth?
    let h = physics_obj(w, this);
    let new_pos = phys_ext::position(w, h).expect("ACE: PhysicsObj.Position");

    let location = w
        .objects
        .get(this)
        .and_then(WorldObject::location)
        .expect("ACE: Location is null");
    if location.landblock_id().raw() != new_pos.cell.0 {
        let prev_block = location.landblock_id().raw() >> 16;
        let new_block = new_pos.cell.0 >> 16;

        w.objects
            .get_mut(this)
            .and_then(|o| o.get_position_mut(PositionType::Location))
            .expect("ACE: Location is null")
            .set_landblock_id(LandblockId::new(new_pos.cell.0));

        if prev_block != new_block {
            landblock_manager::relocate_object_for_physics(w, this, true);
            //Console.WriteLine($"Relocating {Name} from {prevBlockCell:X8} to {newBlockCell:X8}");
        }
    }

    // skip ObjCellID check when updating from physics
    // TODO: update to newer version of ACE.Entity.Position
    let location = w
        .objects
        .get_mut(this)
        .and_then(|o| o.get_position_mut(PositionType::Location))
        .expect("ACE: Location is null");
    location.position_x = new_pos.frame.origin.x;
    location.position_y = new_pos.frame.origin.y;
    location.position_z = new_pos.frame.origin.z;

    let r = new_pos.frame.rotation;
    location.set_rotation(empyrean_common::dotnet::Quaternion::new(r.x, r.y, r.z, r.w));

    // `if (DebugMove) DebugDistance();`: never set
}

/// `PhysicsObj` (ACE dereferences it without a check).
fn physics_obj(w: &crate::World, this: ObjectGuid) -> PhysHandle {
    w.objects
        .get(this)
        .and_then(|o| o.phys)
        .expect("ACE: PhysicsObj is null (NullReferenceException)")
}

/// `CurrentMotionState = motion`.
fn set_current_motion_state(w: &mut crate::World, this: ObjectGuid, motion: Motion) {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .wo
        .world_object_properties
        .current_motion_state = Some(motion);
}

// ACE: Creature.BroadcastMoveTo
/// Sends a player who has just started seeing this creature its MoveTo: to the attack target, or
/// back home.
pub fn creature_broadcast_move_to(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    let run_rate = crate::world_objects::monster_navigation::fields(w, this).run_rate;

    let motion =
        if let Some(attack_target) = crate::world_objects::monster_combat::attack_target(w, this) {
            // move to object
            // Not ACE's (retail, V336): a monster closing to its attack range is
            // seen running there, not chasing to melee.
            if let Some(distance_to_object) =
                crate::world_objects::monster_navigation::fields(w, this).closing_distance
            {
                get_close_to_motion(w, this, attack_target, run_rate, distance_to_object)
            } else {
                get_move_to_motion(w, this, attack_target, run_rate)
            }
        } else {
            // move to position
            let home = w
                .objects
                .get(this)
                .and_then(|o| o.get_position(empyrean_entity::enums::PositionType::Home))
                .expect("ACE: GetPosition(PositionType.Home) is null (NullReferenceException)");

            // Not ACE's (the retail captures, V257): every retail MoveTo had a walk/run threshold of
            // 15.0; ACE sends the way home with 1.0.
            get_move_to_position(
                w,
                this,
                &home,
                run_rate,
                Some(RETAIL_WALK_RUN_THRESHOLD),
                None,
            )
        };

    let movement_data =
        crate::network::motion::movement_data::MovementData::from_motion(this, &motion);
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let msg = crate::network::game_messages::messages::game_message_update_motion::game_message_update_motion(o, &movement_data);
    let session = crate::managers::player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ---- the members the monster AI calls ----

/// Returns the 2D angle between current direction and position from an input target.
// ACE: Creature.GetAngle
#[must_use]
pub fn get_angle(w: &World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    let loc = location(w, this);
    let target_loc = location(w, target);
    let current_dir = loc.get_current_dir();

    let mut target_dir = if loc.indoors() == target_loc.indoors() {
        get_direction(to_global(&loc, false), to_global(&target_loc, false))
    } else {
        get_direction(loc.pos(), target_loc.pos())
    };

    target_dir.z = 0.0;
    target_dir = Vector3::normalize(target_dir);

    // get the 2D angle between these vectors
    get_angle_between(current_dir, target_dir)
}

/// Returns the 2D angle between 2 vectors.
// ACE: Creature.GetAngle
#[must_use]
pub fn get_angle_between(a: Vector3, b: Vector3) -> f32 {
    // `Vector3Extensions.Dot2D`
    let cos_theta = a.x * b.x + a.y * b.y;
    let rads = empyrean_common::math::acos(f64::from(cos_theta));
    if rads.is_nan() {
        return 0.0;
    }

    let angle = rads * (f64::from(180.0f32) / std::f64::consts::PI);
    angle.cs_cast()
}

/// Returns a normalized 2D vector from self to target (despite the name, ACE normalizes the 3D
/// difference: its 2D copies are unused).
// ACE: Creature.GetDirection
#[must_use]
pub fn get_direction(self_: Vector3, target: Vector3) -> Vector3 {
    let _target_2d = Vector3::new(self_.x, self_.y, 0.0);
    let _self_2d = Vector3::new(target.x, target.y, 0.0);

    Vector3::normalize(target - self_)
}

/// The time for this creature to turn from its current direction to face `target`
/// (`GetRotateDelay(WorldObject target)`: the virtual `GetRotateDelay(float angle)`).
// ACE: Creature.GetRotateDelay
#[must_use]
pub fn get_rotate_delay_target(w: &World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    let angle = get_angle(w, this, target);
    dispatch::get_rotate_delay::get_rotate_delay(w, this, angle)
}

/// This is called by the monster AI system for ranged attacks. It is mostly a duplicate of
/// Rotate(), and should be refactored eventually... It sets CurrentMotionState and AttackTarget
/// here (`TurnTo(WorldObject target, bool debug = false)`).
// ACE: Creature.TurnTo
pub fn turn_to_target(w: &mut World, this: ObjectGuid, target: ObjectGuid, debug: bool) -> f32 {
    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    if w.objects.get(this).expect("ACE: this").is_player() {
        return 0.0;
    }

    let turn_to_motion = Motion::to_object(w, this, target, MovementType::TurnToObject);
    enqueue_broadcast_motion(w, this, &turn_to_motion, None, None);

    w.objects
        .get_mut(this)
        .expect("ACE: this")
        .wo
        .world_object_properties
        .current_motion_state = Some(turn_to_motion);

    monster_combat::set_attack_target(w, this, Some(target));
    let rotate_delay = estimate_turn_to(w, this);
    if debug {
        log::info!("TurnTime = {rotate_delay}");
    }
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(rotate_delay));
    action_chain.add_action(Actor::Object(this), move |_w| {
        // fix me: in progress turn
        //var targetDir = GetDirection(Location.ToGlobal(), target.Location.ToGlobal());
        //Location.Rotate(targetDir);
        if debug {
            log::info!("Finished turning - {rotate_delay}s");
        }
    });
    action_chain.enqueue_chain(w);
    rotate_delay
}

/// `Location` (ACE dereferences it without a check).
fn location(w: &World, this: ObjectGuid) -> Position {
    w.objects
        .get(this)
        .and_then(WorldObject::location)
        .expect("ACE: Location is null (NullReferenceException)")
}

// ---- the rest of Creature_Navigation.cs ----

/// Returns the 3D distance between this creature and target.
// ACE: Creature.GetDistance
#[must_use]
pub fn get_distance(w: &World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    let target_loc = w.objects.get(target).and_then(WorldObject::location);
    location(w, this).distance_to(target_loc.as_ref())
}

/// `PhysicsObj.Position` (ACE dereferences it without a check).
fn physics_position(w: &World, this: ObjectGuid) -> dereth_primitives::Position {
    phys_ext::position(w, physics_obj(w, this)).expect("ACE: PhysicsObj.Position")
}

/// The physics body's facing, `Vector3.Normalize(Vector3.Transform(Vector3.UnitY, Orientation))`.
// ACE: Creature.GetCurrentDir_Physics
#[must_use]
pub fn get_current_dir_physics(w: &World, this: ObjectGuid) -> Vector3 {
    let r = physics_position(w, this).frame.rotation;
    let orientation = empyrean_common::dotnet::Quaternion::new(r.x, r.y, r.z, r.w);
    Vector3::normalize(Vector3::transform(Vector3::UNIT_Y, orientation))
}

/// [`get_angle`] from the physics body's facing.
// ACE: Creature.GetAngle_Physics
#[must_use]
pub fn get_angle_physics(w: &World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    let current_dir = get_current_dir_physics(w, this);

    let loc = location(w, this);
    let target_loc = location(w, target);
    let mut target_dir = if loc.indoors() == target_loc.indoors() {
        get_direction(to_global(&loc, false), to_global(&target_loc, false))
    } else {
        get_direction(loc.pos(), target_loc.pos())
    };

    target_dir.z = 0.0;
    target_dir = Vector3::normalize(target_dir);

    // get the 2D angle between these vectors
    get_angle_between(current_dir, target_dir)
}

/// `PhysicsObj.Position.heading_diff(target.PhysicsObj.Position)`: the heading toward the target
/// minus this body's heading.
// ACE: Creature.GetAngle_Physics2
#[must_use]
pub fn get_angle_physics2(w: &World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    use dereth_physics::math::V3;

    let pos = physics_position(w, this);
    let target_pos = physics_position(w, target);

    // Position.heading(position)
    let mut dir = dereth_physics::math::get_offset(&pos, &target_pos);
    dir.z = 0.0;
    let heading = if dir.normalize_check_small() {
        0.0
    } else {
        let rads: f32 = empyrean_common::math::atan2(f64::from(dir.y), f64::from(dir.x)).cs_cast();
        // ToDegrees: (float)(180.0f / Math.PI * rads)
        let degrees: f32 = (f64::from(180.0f32) / std::f64::consts::PI * f64::from(rads)).cs_cast();
        (450.0f32 - degrees) % 360.0f32
    };

    let r = pos.frame.rotation;
    heading - a_frame_get_heading(empyrean_common::dotnet::Quaternion::new(r.x, r.y, r.z, r.w))
}

/// For monsters only -- blips to a new position within the same landblock.
// ACE: Creature.FakeTeleport
pub fn fake_teleport(w: &mut World, this: ObjectGuid, new_position_in: &Position) {
    let mut new_position = Position::from_position(new_position_in);

    let obj_scale = w
        .objects
        .get(this)
        .expect("ACE: this is null")
        .obj_scale()
        .unwrap_or(1.0);
    new_position.position_z += 0.005 * obj_scale;

    let loc = location(w, this);
    if loc.landblock() != new_position.landblock() {
        let name = crate::dispatch::name::name(w, this).unwrap_or_default();
        log::error!("{name} tried to teleport from {loc} to a different landblock {new_position}");
        return;
    }

    // force out of hotspots
    let h = physics_obj(w, this);
    phys_ext::report_collision_end(w, h, true);

    //HandlePreTeleportVisibility(newPosition);

    // do the physics teleport: SetPosition(SendPositionEvent | Slide | Placement | Teleport)
    phys_ext::set_position(w, h, &phys_ext::to_physics_position(&new_position));

    // update ace location
    crate::world_objects::world_object::sync_location(w, this);

    // broadcast blip to new position
    send_update_position(w, this, true);
}

/// Returns the 2D angle between current direction and rotation from an input position.
// ACE: Creature.GetAngle
#[must_use]
pub fn get_angle_position(w: &World, this: ObjectGuid, position: &Position) -> f32 {
    let current_dir = location(w, this).get_current_dir();
    let target_dir = position.get_current_dir();

    // get the 2D angle between these vectors
    get_angle_between(current_dir, target_dir)
}

/// Returns the amount of time for this creature to rotate towards the rotation from the input
/// position, based on the omega speed from its MotionTable. Used by the emote system, which has
/// the target rotation stored in positions.
// ACE: Creature.GetRotateDelay
#[must_use]
pub fn get_rotate_delay_position(w: &World, this: ObjectGuid, position: &Position) -> f32 {
    let angle = get_angle_position(w, this, position);
    dispatch::get_rotate_delay::get_rotate_delay(w, this, angle)
}

/// Starts rotating a creature from its current direction so that it eventually is facing the
/// rotation from the input position. Used by the emote system, which has the target rotation
/// stored in positions. Returns the amount of time in seconds for the rotation to complete.
// ACE: Creature.TurnTo
pub fn turn_to_position(w: &mut World, this: ObjectGuid, position: &Position) -> f32 {
    let heading = a_frame_get_heading(position.rotation());

    // send network message to start turning creature
    let turn_to_motion = Motion::to_heading(w, this, position, heading);
    enqueue_broadcast_motion(w, this, &turn_to_motion, None, None);

    let angle = get_angle_position(w, this, position);
    //Console.WriteLine("Angle: " + angle);

    // estimate time to rotate to target
    let rotate_delay = dispatch::get_rotate_delay::get_rotate_delay(w, this, angle);
    //Console.WriteLine("RotateTime: " + rotateTime);

    // update server object rotation on completion
    // TODO: proper incremental rotation
    let position = Position::from_position(position);
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(rotate_delay));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        let target_dir = position.get_current_dir();
        let rotation = {
            let loc = w
                .objects
                .get_mut(this)
                .and_then(|o| o.get_position_mut(PositionType::Location))
                .expect("ACE: Location is null (NullReferenceException)");
            loc.rotate(target_dir);
            loc.rotation()
        };
        // `PhysicsObj.Position.Frame.Orientation = Location.Rotation`
        let h = physics_obj(w, this);
        if let Some(o) = w.physics.get_mut(h) {
            let mut frame = o.position.frame;
            frame.rotation =
                dereth_primitives::Quat::new(rotation.w, rotation.x, rotation.y, rotation.z);
            o.set_frame(frame);
        }
    });
    action_chain.enqueue_chain(w);

    rotate_delay
}
