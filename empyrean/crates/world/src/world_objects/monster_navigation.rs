// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Navigation.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Navigation.cs`: turning and moving towards the
//! target, the chase and give-up rules, and the return home.
//!
//! The body's MoveTo/TurnTo run on the shared motion stack; ACE's
//! `PhysicsObj` calls map to `phys_ext`.

use dereth_animation::motion::{flags as mvp_flags, MovementParameters};
use dereth_physics::{PhysHandle, V3};
use dereth_primitives::{Position as PPosition, Vec3};
use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::Vector3;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{MotionCommand, PositionType, Skill, WeenieError};
use empyrean_entity::{ObjectGuid, Position};

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::position_extensions::to_global;
use crate::entity::timers;
use crate::network::motion::move_to_parameters::{RetailMoveTo, RETAIL_WALK_RUN_THRESHOLD};
use crate::network::motion::movement_data::Motion;
use crate::physics::{motion, motion_table, phys_ext, weenie_object};
use crate::world_objects::creature_combat::CombatType;
use crate::world_objects::creature_navigation::{
    self, a_frame_get_heading, get_angle, get_rotate_delay_target, turn_to_target,
    update_position_sync_location,
};
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::monster::{self, State};
use crate::world_objects::monster_combat;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::{
    enqueue_broadcast_motion, send_update_position,
};
use crate::world_objects::{monster_magic, monster_missile, monster_tick};
use crate::World;

/// Return to home if target distance exceeds this range.
// ACE: Creature.MaxChaseRange
pub const MAX_CHASE_RANGE: f32 = 96.0;
// ACE: Creature.MaxChaseRangeSq
pub const MAX_CHASE_RANGE_SQ: f32 = MAX_CHASE_RANGE * MAX_CHASE_RANGE;

/// Determines if a monster is within melee range of target.
// ACE: Creature.MaxMeleeRange
pub const MAX_MELEE_RANGE: f32 = 0.75;

/// Non-property fields declared in `Monster_Navigation.cs`.
#[derive(Debug, Default)]
pub struct MonsterNavigationFields {
    /// The distance per second from running animation.
    // ACE: Creature.MoveSpeed
    pub move_speed: f32,
    /// The run skill via MovementSystem GetRunRate().
    // ACE: Creature.RunRate
    pub run_rate: f32,
    /// Flag indicates monster is turning towards target.
    // ACE: Creature.IsTurning
    pub is_turning: bool,
    /// Flag indicates monster is moving towards target.
    // ACE: Creature.IsMoving
    pub is_moving: bool,
    /// The last time a movement tick was processed.
    // ACE: Creature.LastMoveTime
    pub last_move_time: f64,
    // ACE: Creature.DebugMove
    pub debug_move: bool,
    // ACE: Creature.NextMoveTime
    pub next_move_time: f64,
    // ACE: Creature.NextCancelTime
    pub next_cancel_time: f64,
    // ACE: Creature.homeRadiusSq
    pub home_radius_sq: Option<f32>,
    /// Not ACE's (retail, V336): the distance a ranged or casting monster
    /// runs to before its current attack, drawn once per attack decision (see
    /// [`closes_to_range`]); `None` until drawn, and always with the option off.
    pub closing_distance: Option<f32>,
}

/// `Creature`'s `Monster_Navigation.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterNavigationFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_navigation
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterNavigationFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_navigation
}

/// `PhysicsObj` (ACE dereferences it without a check).
///
/// # Panics
/// When the object has no body (ACE: `NullReferenceException`).
#[must_use]
pub fn physics_obj(w: &World, this: ObjectGuid) -> PhysHandle {
    w.objects
        .get(this)
        .and_then(|o| o.phys)
        .expect("ACE: PhysicsObj is null (NullReferenceException)")
}

/// `PhysicsObj.IsSticky`: the position manager sticks to a target.
#[must_use]
pub fn is_sticky(w: &World, this: ObjectGuid) -> bool {
    let h = physics_obj(w, this);
    motion::with_driver(w, h, |d| d.movement.sticky.is_sticky()).unwrap_or(false)
}

/// `PhysicsObj.UpdateTime = PhysicsTimer.CurrentTime` (also `PhysicsObj.StartTimer()`).
pub(crate) fn physics_obj_start_timer(w: &mut World, h: PhysHandle) {
    let now = phys_ext::physics_timer_current_time(w);
    if let Some(o) = w.physics.get_mut(h) {
        o.update_time = now;
    }
}

/// `PhysicsObj.CachedVelocity = Vector3.Zero`.
fn physics_obj_clear_cached_velocity(w: &mut World, this: ObjectGuid) {
    let h = physics_obj(w, this);
    if let Some(o) = w.physics.get_mut(h) {
        o.cached_velocity = Vec3::ZERO;
    }
}

/// `Location` (ACE dereferences it without a check).
fn location(w: &World, this: ObjectGuid) -> Position {
    w.objects
        .get(this)
        .and_then(WorldObject::location)
        .expect("ACE: Location is null (NullReferenceException)")
}

/// Starts the process of monster turning towards target.
// ACE: Creature.StartTurn
pub fn start_turn(w: &mut World, this: ObjectGuid) {
    //if (Timers.RunningTime < NextMoveTime)
    //return;
    if !monster_combat::move_ready(w, this) {
        return;
    }

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    if fields(w, this).move_speed == 0.0 {
        get_movement_speed(w, this);
    }

    //Console.WriteLine($"[{Timers.RunningTime}] - {Name} ({Guid}) - starting turn");

    fields_mut(w, this).is_turning = true;

    // send network actions
    let target_dist = get_distance_to_target(w, this);
    let next = next_move(w, this, target_dist);
    let turn_to = next == NextMove::TurnTo;
    let attack_target = monster_combat::attack_target(w, this)
        .expect("ACE: AttackTarget is null (NullReferenceException)");
    match next {
        NextMove::TurnTo => {
            turn_to_target(w, this, attack_target, false);
        }
        NextMove::Chase => {
            let run_rate = fields(w, this).run_rate;
            creature_navigation::creature_move_to(w, this, attack_target, run_rate);
        }
        NextMove::CloseTo(distance_to_object) => {
            let run_rate = fields(w, this).run_rate;
            creature_navigation::creature_close_to(
                w,
                this,
                attack_target,
                run_rate,
                distance_to_object,
            );
        }
    }

    // need turning listener?
    let now = timers::running_time(w);
    let f = fields_mut(w, this);
    f.is_turning = false;
    f.is_moving = true;
    f.last_move_time = now;
    let cancel = ThreadSafeRandom::next(2, 4);
    fields_mut(w, this).next_cancel_time = now + f64::from(cancel);
    crate::world_objects::monster_melee::fields_mut(w, this).move_bit = false;

    let mvp = get_movement_parameters(w, this);
    let h = physics_obj(w, this);
    let target_h = physics_obj(w, attack_target);
    if turn_to {
        phys_ext::turn_to_object(w, h, Some(target_h), &mvp);
    } else {
        phys_ext::move_to_object(w, h, target_h, &mvp);
    }

    // prevent initial snap
    physics_obj_start_timer(w, h);
}

/// What [`start_turn`] sends: a TurnTo, the attack chase, or (with the `monster_ranged_closing`
/// option) the run to within a distance of the target.
#[derive(Debug, Clone, Copy, PartialEq)]
enum NextMove {
    TurnTo,
    Chase,
    CloseTo(f32),
}

/// The move [`start_turn`] makes: ACE's turn or chase ([`turn_to_instead_of_move_to`]), or, for
/// an attack that closes to its range ([`closes_to_range`]), a turn when the target is within
/// range and otherwise the run to the closing distance, drawn here once per attack decision.
fn next_move(w: &mut World, this: ObjectGuid, target_dist: f32) -> NextMove {
    if !closes_to_range(w, this) {
        return if turn_to_instead_of_move_to(w, this, target_dist) {
            NextMove::TurnTo
        } else {
            NextMove::Chase
        };
    }

    // Not ACE's (retail, V336): a missile or spell attack whose target is
    // beyond its range R runs to a distance drawn uniformly from 2/3 R to 16/15 R, once per
    // attack decision (a repeated run keeps it), then turns and attacks from there. A target
    // already within range, or within the drawn distance, is only turned to: retail's monsters
    // never backed away.
    if is_in_closing_range(w, this, target_dist) {
        return NextMove::TurnTo;
    }
    let distance_to_object = match fields(w, this).closing_distance {
        Some(d) => d,
        None => {
            let max_range = monster_combat::fields(w, this).max_range;
            #[allow(clippy::cast_possible_truncation)] // a distance in metres
            let d = (f64::from(max_range)
                * ThreadSafeRandom::next_float(CLOSING_FRACTION_MIN, CLOSING_FRACTION_MAX))
                as f32;
            fields_mut(w, this).closing_distance = Some(d);
            d
        }
    };
    if target_dist <= distance_to_object {
        NextMove::TurnTo
    } else {
        NextMove::CloseTo(distance_to_object)
    }
}

/// Not ACE's (retail, V336): the nearest fraction of the attack range a
/// ranged or casting monster runs to (retail's captured closing distances start sharply at 2/3).
pub const CLOSING_FRACTION_MIN: f32 = 2.0 / 3.0;
/// Not ACE's (retail, V336): the farthest fraction of the attack range a
/// ranged or casting monster runs to (retail's end sharply at 16/15, slightly beyond the range).
pub const CLOSING_FRACTION_MAX: f32 = 16.0 / 15.0;

/// Not ACE's (retail, V336): whether the `monster_ranged_closing` server
/// option is on (the default).
#[must_use]
pub fn ranged_closing_enabled(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "monster_ranged_closing", true, true).item
}

/// Not ACE's (retail, V336): whether the current attack closes to its range
/// before it is made: the `monster_ranged_closing` option is on, the attack is a missile or a
/// spell, and the monster can move. A self spell has no range and is never beyond it.
pub fn closes_to_range(w: &World, this: ObjectGuid) -> bool {
    matches!(
        monster_combat::fields(w, this).current_attack,
        Some(CombatType::Missile | CombatType::Magic)
    ) && !w.objects.get(this).expect("ACE: this").ai_immobile()
        && ranged_closing_enabled(w)
}

/// Not ACE's (retail, V336): whether `target_dist` is close enough for the
/// current attack: within its range, or within the distance drawn for this decision, which can
/// be up to 16/15 of the range (retail's monsters attacked from there).
#[must_use]
pub fn is_in_closing_range(w: &World, this: ObjectGuid, target_dist: f32) -> bool {
    target_dist <= monster_combat::fields(w, this).max_range
        || fields(w, this)
            .closing_distance
            .is_some_and(|d| target_dist <= d)
}

/// `IsRanged || (CurrentAttack == CombatType.Magic && targetDist <= GetSpellMaxRange()) ||
/// AiImmobile`, evaluated left to right.
fn turn_to_instead_of_move_to(w: &mut World, this: ObjectGuid, target_dist: f32) -> bool {
    monster_missile::is_ranged(w, this)
        || (monster_combat::fields(w, this).current_attack == Some(CombatType::Magic)
            && target_dist <= monster_magic::get_spell_max_range(w, this))
        || w.objects.get(this).expect("ACE: this").ai_immobile()
}

/// Called when the TurnTo process has completed.
// ACE: Creature.OnTurnComplete
pub fn on_turn_complete(w: &mut World, this: ObjectGuid) {
    let attack_target = monster_combat::attack_target(w, this)
        .expect("ACE: AttackTarget is null (NullReferenceException)");
    let dir = Vector3::normalize(
        to_global(&location(w, attack_target), false) - to_global(&location(w, this), false),
    );
    if let Some(l) = w
        .objects
        .get_mut(this)
        .and_then(|o| o.get_position_mut(PositionType::Location))
    {
        l.rotate(dir);
    }

    fields_mut(w, this).is_turning = false;

    if !monster_missile::is_ranged(w, this) {
        start_move(w, this);
    }
}

/// Starts the process of monster moving towards target.
// ACE: Creature.StartMove
pub fn start_move(w: &mut World, this: ObjectGuid) {
    let now = timers::running_time(w);
    let f = fields_mut(w, this);
    f.last_move_time = now;
    f.is_moving = true;
}

/// Called when the MoveTo process has completed.
// ACE: Creature.OnMoveComplete
pub fn creature_on_move_complete(w: &mut World, this: ObjectGuid, status: WeenieError) {
    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    if status != WeenieError::None {
        return;
    }

    if w.objects.get(this).expect("ACE: this").ai_immobile()
        && monster_combat::fields(w, this).current_attack == Some(CombatType::Melee)
    {
        let target_dist = get_distance_to_target(w, this);
        if target_dist > monster_combat::fields(w, this).max_range {
            monster_combat::reset_attack(w, this);
        }
    }

    if monster::monster_state(w, this) == State::Return {
        dispatch::sleep::sleep(w, this);
    }

    physics_obj_clear_cached_velocity(w, this);
    fields_mut(w, this).is_moving = false;
}

/// Estimates the time it will take the monster to turn and move towards target.
// ACE: Creature.EstimateTargetTime
#[must_use]
pub fn estimate_target_time(w: &World, this: ObjectGuid) -> f32 {
    estimate_turn_to(w, this) + estimate_move_to(w, this)
}

/// Estimates the time it will take the monster to turn towards target.
// ACE: Creature.EstimateTurnTo
#[must_use]
pub fn estimate_turn_to(w: &World, this: ObjectGuid) -> f32 {
    let attack_target = monster_combat::attack_target(w, this)
        .expect("ACE: AttackTarget is null (NullReferenceException)");
    get_rotate_delay_target(w, this, attack_target)
}

/// Estimates the time it will take the monster to move towards target.
// ACE: Creature.EstimateMoveTo
#[must_use]
pub fn estimate_move_to(w: &World, this: ObjectGuid) -> f32 {
    get_distance_to_target(w, this) / fields(w, this).move_speed
}

/// Returns TRUE if monster is within target melee range.
// ACE: Creature.IsMeleeRange
#[must_use]
pub fn is_melee_range(w: &World, this: ObjectGuid) -> bool {
    get_distance_to_target(w, this) <= MAX_MELEE_RANGE
}

/// Returns TRUE if monster in range for current attack type.
// ACE: Creature.IsAttackRange
#[must_use]
pub fn is_attack_range(w: &World, this: ObjectGuid) -> bool {
    // Not ACE's (retail, V336): a monster that closed to a distance drawn
    // beyond its range (up to 16/15 of it) attacks from there; with the option off no distance is
    // drawn and this is ACE's range check.
    is_in_closing_range(w, this, get_distance_to_target(w, this))
}

/// Gets the distance to target, with radius excluded: the cylinder distance between the two
/// bodies (`float.MaxValue` with no target).
// ACE: Creature.GetDistanceToTarget
#[must_use]
pub fn get_distance_to_target(w: &World, this: ObjectGuid) -> f32 {
    let Some(attack_target) = monster_combat::attack_target(w, this) else {
        return f32::MAX;
    };

    //var matchIndoors = Location.Indoors == AttackTarget.Location.Indoors;
    //var targetPos = matchIndoors ? AttackTarget.Location.ToGlobal() : AttackTarget.Location.Pos;
    //var sourcePos = matchIndoors ? Location.ToGlobal() : Location.Pos;

    //var dist = (targetPos - sourcePos).Length();
    //var radialDist = dist - (AttackTarget.PhysicsObj.GetRadius() + PhysicsObj.GetRadius());

    // always use spheres?
    let (r1, h1, p1) = body_size_and_position(w, physics_obj(w, this));
    let (r2, h2, p2) = body_size_and_position(w, physics_obj(w, attack_target));
    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    let cyl_dist = cylinder_distance(r1, h1, &p1, r2, h2, &p2) as f32;

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    //return radialDist;
    cyl_dist
}

/// `GetRadius()`, `GetHeight()` and `Position` of a body.
fn body_size_and_position(w: &World, h: PhysHandle) -> (f32, f32, PPosition) {
    let o = w
        .physics
        .get(h)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    (o.radius(), o.height(), o.position)
}

/// `Position.CylinderDistance(radius, height, pos, otherRadius, otherHeight, otherPos)`
/// (`Physics/Common/Position.cs`): the distance between the two bodies' cylinders, negative when
/// they overlap.
///
/// Not ACE's (retail, V310): the shared crate's cylinder distance. ACE answered
/// the reach between the sides whenever the reach and the vertical gap did not share a sign, so a
/// body directly below another standing on a ledge (overlapping horizontally, a gap apart
/// vertically) was at a negative distance, touching it; here it is the vertical gap away. Where
/// the bodies overlap vertically and their sides just touch (a reach of exactly 0), the distance
/// is minus the vertical overlap rather than 0. Everywhere else the two agree.
// ACE: Physics.Common.Position.CylinderDistance
#[must_use]
pub fn cylinder_distance(
    radius: f32,
    height: f32,
    pos: &PPosition,
    other_radius: f32,
    other_height: f32,
    other_pos: &PPosition,
) -> f64 {
    f64::from(dereth_physics::math::cylinder_distance(
        radius,
        height,
        pos,
        other_radius,
        other_height,
        other_pos,
    ))
}

/// `Position.CylinderDistanceSq(...)`: [`cylinder_distance`] squared, keeping its sign, so a
/// comparison against a squared range, or a closest-first ordering, answers as the unsquared
/// distance would.
///
/// Not ACE's (retail, V310): the signed square of the shared crate's cylinder
/// distance. ACE squared the reach, losing its sign, wherever the reach and the vertical gap did
/// not share one: a body directly below another on a ledge answered its reach squared rather than
/// the gap squared, and bodies overlapping horizontally with their ends level a positive value.
// ACE: Physics.Common.Position.CylinderDistanceSq
#[must_use]
pub fn cylinder_distance_sq(
    radius: f32,
    height: f32,
    pos: &PPosition,
    other_radius: f32,
    other_height: f32,
    other_pos: &PPosition,
) -> f64 {
    let dist = cylinder_distance(radius, height, pos, other_radius, other_height, other_pos);
    dist * dist.abs()
}

/// `PhysicsObj.get_distance_to_object(obj.PhysicsObj, use_cyls)` between two world objects.
// ACE: PhysicsObj.get_distance_to_object
#[must_use]
pub fn get_distance_to_object(w: &World, this: ObjectGuid, obj: ObjectGuid, use_cyls: bool) -> f64 {
    let (r1, h1, p1) = body_size_and_position(w, physics_obj(w, this));
    let (r2, h2, p2) = body_size_and_position(w, physics_obj(w, obj));
    if !use_cyls {
        return f64::from(dereth_physics::math::distance(&p1, &p2));
    }
    cylinder_distance(r1, h1, &p1, r2, h2, &p2)
}

/// `PhysicsObj.get_distance_sq_to_object(obj.PhysicsObj, use_cyls)` between two world objects.
// ACE: PhysicsObj.get_distance_sq_to_object
#[must_use]
pub fn get_distance_sq_to_object(
    w: &World,
    this: ObjectGuid,
    obj: ObjectGuid,
    use_cyls: bool,
) -> f64 {
    let (r1, h1, p1) = body_size_and_position(w, physics_obj(w, this));
    let (r2, h2, p2) = body_size_and_position(w, physics_obj(w, obj));
    if !use_cyls {
        return f64::from(dereth_physics::math::get_offset(&p1, &p2).mag2());
    }
    cylinder_distance_sq(r1, h1, &p1, r2, h2, &p2)
}

/// Returns the destination position the monster is attempting to move to to perform a melee
/// attack.
// ACE: Creature.GetDestination
#[must_use]
pub fn get_destination(w: &World, this: ObjectGuid) -> Vector3 {
    let attack_target = monster_combat::attack_target(w, this)
        .expect("ACE: AttackTarget is null (NullReferenceException)");
    let target_location = location(w, attack_target);
    let dir = Vector3::normalize(
        to_global(&location(w, this), false) - to_global(&target_location, false),
    );
    let (r_target, _, _) = body_size_and_position(w, physics_obj(w, attack_target));
    let (r_this, _, _) = body_size_and_position(w, physics_obj(w, this));
    target_location.pos() + dir * (r_target + r_this)
}

/// Primary movement handler, determines if target in range.
// ACE: Creature.Movement
pub fn movement(w: &mut World, this: ObjectGuid) {
    //if (!IsRanged)
    update_position(w, this, true);

    if monster::monster_state(w, this) == State::Awake
        && get_distance_to_target(w, this) >= MAX_CHASE_RANGE
    {
        cancel_move_to(w, this);
        dispatch::find_next_target::find_next_target(w, this);
        return;
    }

    let h = physics_obj(w, this);
    if phys_ext::move_to_fail_progress_count(w, h) > 0
        && timers::running_time(w) > fields(w, this).next_cancel_time
    {
        cancel_move_to(w, this);
    }
}

/// Steps the body, syncs the location from it and (with `netsend`) broadcasts it; a finished
/// return home puts the monster to sleep, and a finished MoveTo clears `IsMoving`.
// ACE: Creature.UpdatePosition
pub fn update_position(w: &mut World, this: ObjectGuid, netsend: bool) {
    //stopwatch.Restart();
    let h = physics_obj(w, this);
    phys_ext::update_object(w, h);
    //ServerPerformanceMonitor.AddToCumulativeEvent(...);
    update_position_sync_location(w, this);

    if netsend {
        send_update_position(w, this, false);
    }

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    if monster::monster_state(w, this) == State::Return
        && phys_ext::move_to_pending_actions(w, h) == 0
    {
        dispatch::sleep::sleep(w, this);
    }

    if monster::monster_state(w, this) == State::Awake
        && fields(w, this).is_moving
        && phys_ext::move_to_pending_actions(w, h) == 0
    {
        fields_mut(w, this).is_moving = false;
    }
}

/// Debug distance and angle to the target (both results are unused).
// ACE: Creature.DebugDistance
pub fn debug_distance(w: &World, this: ObjectGuid) {
    let Some(attack_target) = monster_combat::attack_target(w, this) else {
        return;
    };

    let _dist = get_distance_to_target(w, this);
    let _angle = get_angle(w, this, attack_target);
    //Console.WriteLine("Dist: " + dist);
    //Console.WriteLine("Angle: " + angle);
}

/// Caches the run rate and the move speed (the motion table's run speed, 2.5 if none, times the
/// run rate and the object scale).
// ACE: Creature.GetMovementSpeed
pub fn get_movement_speed(w: &mut World, this: ObjectGuid) {
    let (motion_table_id, scale) = {
        let o = w.objects.get(this).expect("ACE: this");
        (o.motion_table_id(), o.obj_scale().unwrap_or(1.0))
    };
    let mut move_speed = motion_table::get_run_speed(w, motion_table_id);
    if move_speed == 0.0 {
        move_speed = 2.5;
    }

    let run_rate = get_run_rate(w, this);

    let f = fields_mut(w, this);
    f.run_rate = run_rate;
    f.move_speed = move_speed * run_rate * scale;

    //Console.WriteLine(Name + " - Run: " + runSkill + " - RunRate: " + RunRate + " - Move: " + MoveSpeed + " - Scale: " + scale);
}

/// Returns the RunRate that is sent to the client as myRunRate.
// ACE: Creature.GetRunRate
pub fn get_run_rate(w: &mut World, this: ObjectGuid) -> f32 {
    let mut burden = 0.0f32;

    // assuming burden only applies to players...
    if w.objects.get(this).expect("ACE: this").is_player() {
        let strength_attribute = w.objects.get(this).expect("ACE: this").strength();
        let strength: i32 = strength_attribute
            .current(&mut StatCtx::in_world(w, this))
            .cs_cast();

        let num_augs = w
            .objects
            .get(this)
            .expect("ACE: this")
            .augmentation_increased_carrying_capacity();
        let capacity = weenie_object::encumbrance_system_encumbrance_capacity(strength, num_augs);
        let encumbrance = w
            .objects
            .get(this)
            .expect("ACE: this")
            .encumbrance_val()
            .unwrap_or(0);
        burden = dereth_rules::burden::load(capacity, encumbrance);

        // TODO: find this exact formula in client
        // technically this would be based on when the player releases / presses the movement key after stamina > 0
        let o = w.objects.get(this).expect("ACE: this");
        if o.stamina().current(o) == 0 {
            burden = 3.0;
        }
    }

    let run = w
        .objects
        .get_mut(this)
        .expect("ACE: this")
        .get_creature_skill(Skill::Run, true)
        .expect("GetCreatureSkill(add: true)");
    let run_skill = run.current(w, this);
    let run_rate = if w.objects.get(this).expect("ACE: this").is_player() {
        weenie_object::player_run_rate(burden, run_skill.cs_cast())
    } else {
        weenie_object::movement_system_get_run_rate(burden, run_skill.cs_cast(), 1.0)
    };

    run_rate.cs_cast()
}

/// Sets the corpse to the final position.
// ACE: Creature.SetFinalPosition
pub fn set_final_position(w: &mut World, this: ObjectGuid) {
    let Some(attack_target) = monster_combat::attack_target(w, this) else {
        return;
    };

    let target_location = location(w, attack_target);
    let player_dir = target_location.get_current_dir();
    let (r_target, _, _) = body_size_and_position(w, physics_obj(w, attack_target));
    let (r_this, _, _) = body_size_and_position(w, physics_obj(w, this));
    let pos = target_location.pos() + player_dir * (r_target + r_this);
    if let Some(l) = w
        .objects
        .get_mut(this)
        .and_then(|o| o.get_position_mut(PositionType::Location))
    {
        l.set_pos(pos);
    }
    send_update_position(w, this, false);
}

/// Returns TRUE if monster is facing towards the target: within 5 degrees, plus 1.5 degrees for
/// every metre closer than 10.
// ACE: Creature.IsFacing
#[must_use]
pub fn is_facing(w: &World, this: ObjectGuid, target: Option<ObjectGuid>) -> bool {
    let Some(target) =
        target.filter(|&t| w.objects.get(t).and_then(WorldObject::location).is_some())
    else {
        return false;
    };

    let angle = get_angle(w, this, target);
    let dist = 0.0f32.max(get_distance_to_target(w, this));

    is_facing_threshold(angle, dist)
}

/// The threshold arithmetic of [`is_facing`], over its angle and distance.
#[must_use]
pub fn is_facing_threshold(angle: f32, dist: f32) -> bool {
    // rotation accuracy?
    let mut threshold = 5.0f32;

    let min_dist = 10.0f32;

    if dist < min_dist {
        threshold += (min_dist - dist) * 1.5f32;
    }

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    angle < threshold
}

/// The monster's MoveTo/TurnTo parameters: a TurnTo takes ACE's `new MovementParameters()`
/// without walking; a MoveTo takes the attack chase (`0x1EFF0`, walk/run threshold 15), or, for
/// a ranged or casting monster closing to its range, the same chase without Sticky (`0x1EF70`) to
/// the drawn distance.
// ACE: Creature.GetMovementParameters
pub fn get_movement_parameters(w: &mut World, this: ObjectGuid) -> MovementParameters {
    let mut mvp = phys_ext::ace_movement_parameters();

    // set non-default params for monster movement
    mvp.flags &= !mvp_flags::CAN_WALK;

    let next = next_move(w, this, get_distance_to_target(w, this));

    if next == NextMove::Chase {
        // Not ACE's (the retail captures, V257): the body chases with the parameters the clients
        // are sent, retail's attack chase; ACE adds FailWalk, UseFinalHeading, Sticky and
        // MoveAway to its own defaults without CanWalk (0x1EFFE, threshold 1.0).
        mvp = RetailMoveTo::AttackChase.movement_parameters();
    } else if let NextMove::CloseTo(distance_to_object) = next {
        // Not ACE's (retail, V336): the run to within the drawn distance,
        // with no minimum distance and no fail distance, not stuck to the target on arrival.
        mvp = RetailMoveTo::AttackChaseUnstuck.movement_parameters();
        mvp.distance_to_object = distance_to_object;
        mvp.min_distance = 0.0;
        mvp.fail_distance = f32::MAX;
    }

    mvp
}

// ACE: Creature.DefaultHomeRadius
pub const DEFAULT_HOME_RADIUS: f32 = 192.0;
// ACE: Creature.DefaultHomeRadiusSq
pub const DEFAULT_HOME_RADIUS_SQ: f32 = DEFAULT_HOME_RADIUS * DEFAULT_HOME_RADIUS;

/// The squared home radius (`HomeRadius`, default 192), cached on first read.
// ACE: Creature.HomeRadiusSq
pub fn home_radius_sq(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(v) = fields(w, this).home_radius_sq {
        return v;
    }
    let home_radius = w
        .objects
        .get(this)
        .expect("ACE: this")
        .home_radius()
        .unwrap_or(f64::from(DEFAULT_HOME_RADIUS));
    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    let v = (home_radius * home_radius) as f32;
    fields_mut(w, this).home_radius_sq = Some(v);
    v
}

/// Sends the monster home when it has strayed beyond its home radius.
// ACE: Creature.CheckMissHome
pub fn check_miss_home(w: &mut World, this: ObjectGuid) {
    if monster::monster_state(w, this) == State::Return {
        return;
    }

    let home_position = w
        .objects
        .get(this)
        .and_then(|o| o.get_position(PositionType::Home))
        .expect("ACE: GetPosition(PositionType.Home) is null (NullReferenceException)");
    //var matchIndoors = Location.Indoors == homePosition.Indoors;

    //var globalPos = matchIndoors ? Location.ToGlobal() : Location.Pos;
    //var globalHomePos = matchIndoors ? homePosition.ToGlobal() : homePosition.Pos;
    let global_pos = to_global(&location(w, this), false);
    let global_home_pos = to_global(&home_position, false);

    let home_dist_sq = vector3_distance_squared(global_home_pos, global_pos);

    if home_dist_sq > home_radius_sq(w, this) {
        move_to_home(w, this);
    }
}

/// `Vector3.DistanceSquared(a, b)` in float.
fn vector3_distance_squared(a: Vector3, b: Vector3) -> f32 {
    (a - b).length_squared()
}

/// Drops the target and runs home (a MoveToPosition to the home position, 0.6 m away, facing its
/// heading); at home already, sleeps.
// ACE: Creature.MoveToHome
pub fn move_to_home(w: &mut World, this: ObjectGuid) {
    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    let prev_attack_target = monster_combat::attack_target(w, this);

    monster::set_monster_state_value(w, this, State::Return);
    monster_combat::set_attack_target(w, this, None);

    let home = w
        .objects
        .get(this)
        .and_then(|o| o.get_position(PositionType::Home))
        .expect("ACE: GetPosition(PositionType.Home) is null (NullReferenceException)");

    if location(w, this).equals(&home) {
        dispatch::sleep::sleep(w, this);
        return;
    }

    fields_mut(w, this).next_cancel_time = timers::running_time(w) + f64::from(5.0f32);

    let run_rate = fields(w, this).run_rate;
    // Not ACE's (the retail captures, V257): every retail MoveTo had a walk/run threshold of 15.0
    // and an NPC's move the defaults plus UseFinalHeading (0x1EE4F). ACE sends the way home with
    // 1.0 and moves the body with its chase parameters; the body now moves as the clients are
    // told to.
    creature_navigation::creature_move_to_position(
        w,
        this,
        &home,
        run_rate,
        false,
        Some(RETAIL_WALK_RUN_THRESHOLD),
        None,
    );

    let home_pos = phys_ext::to_physics_position(&home);

    let mut mvp = RetailMoveTo::Use.movement_parameters();
    mvp.distance_to_object = 0.6;
    mvp.desired_heading = a_frame_get_heading(home.rotation());

    let h = physics_obj(w, this);
    phys_ext::move_to_position(w, h, &home_pos, &mvp);
    fields_mut(w, this).is_moving = true;

    monster_awareness_on_home_sick(w, this, prev_attack_target);
}

fn monster_awareness_on_home_sick(
    w: &mut World,
    this: ObjectGuid,
    prev_attack_target: Option<ObjectGuid>,
) {
    crate::world_objects::monster_awareness::emote_manager_on_home_sick(
        w,
        this,
        prev_attack_target,
    );
}

/// Cancels the MoveTo, returns to Ready (a returning monster is placed home), resets the attack
/// and looks for the next target.
// ACE: Creature.CancelMoveTo
pub fn cancel_move_to(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.CancelMoveTo()");

    let h = physics_obj(w, this);
    phys_ext::cancel_move_to(w, h, WeenieError::ActionCancelled);
    phys_ext::set_move_to_fail_progress_count(w, h, 0);

    if monster::monster_state(w, this) == State::Return {
        force_home(w, this);
    }

    let stance = monster_tick::current_stance(w, this);
    enqueue_broadcast_motion(
        w,
        this,
        &Motion::new(stance, MotionCommand::Ready, 1.0),
        None,
        None,
    );

    let now = timers::running_time(w);
    let f = fields_mut(w, this);
    f.is_moving = false;
    f.next_move_time = now + f64::from(1.0f32);

    monster_combat::reset_attack(w, this);

    dispatch::find_next_target::find_next_target(w, this);
}

/// Teleports the monster to its home position and puts it to sleep half a second later.
// ACE: Creature.ForceHome
pub fn force_home(w: &mut World, this: ObjectGuid) {
    let home_pos = w
        .objects
        .get(this)
        .and_then(|o| o.get_position(PositionType::Home))
        .expect("ACE: GetPosition(PositionType.Home) is null (NullReferenceException)");

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    // `new SetPosition { Pos = new Physics.Common.Position(homePos), Flags = SetPositionFlags.Teleport }`
    let h = physics_obj(w, this);
    phys_ext::set_position(w, h, &phys_ext::to_physics_position(&home_pos));

    update_position_sync_location(w, this);

    send_update_position(w, this, false);

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(0.5f32));
    action_chain.add_action(Actor::Object(this), move |w| {
        dispatch::sleep::sleep(w, this)
    });
    action_chain.enqueue_chain(w);
}
