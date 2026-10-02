// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Move.cs, Source/ACE.Server/WorldObjects/Player.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Move.cs`.
//!
//! The legacy move-to chains (`CreateMoveToChain`, `MoveToChain`), the melee charge (`MoveTo`,
//! `OnMoveComplete`) and falling damage. This file also hosts `Player.HandleActionJump`, the
//! `CurrentMoveToState`/`LastMoveToState` fields of `WorldObject`, and ACE's
//! `MovementSystem.JumpStaminaCost`, which the jump calls (its `EncumbranceSystem` capacity is
//! `physics::weenie_object`'s, its load the shared rules' `burden`). (`IsJumping` and the
//! `Player.cs` movement fields are in `player.rs`.)

use dereth_animation::motion::{HoldKey, MovementParameters};
use dereth_primitives::Vec3;
use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_entity::enums::{
    CombatMode, DamageType, MovementType, Sound, WeenieError, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::strings;
use crate::network::game_messages::messages::{
    game_message_sound, game_message_update_motion, game_message_vector_update,
};
use crate::network::motion::move_to_parameters::RetailMoveTo;
use crate::network::motion::move_to_state::MoveToState;
use crate::network::motion::movement_data::{Motion, MovementData};
use crate::network::motion::movement_invalid::MovementInvalid;
use crate::network::structure::jump_pack::JumpPack;
use crate::physics::phys_ext;
use crate::world_objects::world_object_networking::{self, shims};
use crate::world_objects::{
    creature_vitals, monster_navigation, player, player_move2, player_networking, player_tick,
};
use crate::World;

/// A move-to chain's completion callback (`Action<bool>`).
pub type MoveToCallback = Box<dyn FnOnce(&mut World, bool) + Send>;

/// Non-property fields declared in `Player_Move.cs`, and the hosted movement
/// fields of `Player.cs` and `WorldObject` (see the module docs).
#[derive(Debug)]
pub struct PlayerMoveFields {
    // ACE: Player.moveToChainCounter
    pub move_to_chain_counter: i32,
    // ACE: Player.moveToChainStartTime
    pub move_to_chain_start_time: DotNetDateTime,
    // ACE: Player.lastCompletedMove
    pub last_completed_move: i32,
    // ACE: Player.StartJump
    pub start_jump: Option<Position>,

    // ---- WorldObject (hosted) ----
    // ACE: WorldObject.CurrentMoveToState
    pub current_move_to_state: MoveToState,
    // ACE: WorldObject.LastMoveToState
    pub last_move_to_state: Option<MoveToState>,
}

impl Default for PlayerMoveFields {
    fn default() -> Self {
        PlayerMoveFields {
            move_to_chain_counter: 0,
            move_to_chain_start_time: DotNetDateTime::MIN_VALUE,
            last_completed_move: 0,
            start_jump: None,
            current_move_to_state: MoveToState::default(),
            last_move_to_state: None,
        }
    }
}

/// The player's `Player_Move` fields.
///
/// # Panics
/// When `this` is not a live player.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerMoveFields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("ACE: this is a Player")
        .player_move
}

/// The player's `Player_Move` fields, mutably.
///
/// # Panics
/// When `this` is not a live player.
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerMoveFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: this is a Player")
        .player_move
}

// ACE: Player.defaultMoveToTimeout
/// This is just a starting point number. It may be far off from retail.
#[must_use]
pub fn default_move_to_timeout() -> TimeSpan {
    TimeSpan::from_seconds(15.0)
}

// ACE: Player.IsPlayerMovingTo
#[must_use]
pub fn is_player_moving_to(w: &World, this: ObjectGuid) -> bool {
    let f = fields(w, this);
    f.move_to_chain_counter > f.last_completed_move
}

// ACE: Player.GetNextMoveToChainNumber
fn get_next_move_to_chain_number(w: &mut World, this: ObjectGuid) -> i32 {
    let f = fields_mut(w, this);
    f.move_to_chain_counter = f.move_to_chain_counter.wrapping_add(1);
    f.move_to_chain_counter
}

// ACE: Player.StopExistingMoveToChains
pub fn stop_existing_move_to_chains(w: &mut World, this: ObjectGuid) {
    let f = fields_mut(w, this);
    f.move_to_chain_counter = f.move_to_chain_counter.wrapping_add(1);

    f.last_completed_move = f.move_to_chain_counter;
}

// ACE: Player.CreateMoveToChain
/// Walks (or turns) to `target`, then calls `callback` with whether it arrived within the use
/// radius. `use_radius` defaults to the target's, `rotate` to true. The move is a use-move
/// ([`RetailMoveTo::Use`], a portal's [`RetailMoveTo::Portal`]); [`create_move_to_chain_as`]
/// names another kind.
pub fn create_move_to_chain(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    callback: MoveToCallback,
    use_radius: Option<f32>,
    rotate: bool,
) {
    create_move_to_chain_as(
        w,
        this,
        target,
        callback,
        use_radius,
        rotate,
        RetailMoveTo::Use,
    );
}

/// [`create_move_to_chain`] with the kind of move the MoveTo's flag word follows (V257): `kind`,
/// or [`RetailMoveTo::Portal`] for a portal.
pub fn create_move_to_chain_as(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    callback: MoveToCallback,
    use_radius: Option<f32>,
    rotate: bool,
    kind: RetailMoveTo,
) {
    if player_tick::fast_tick(w, this) {
        player_move2::create_move_to_chain2(w, this, target, callback, use_radius, rotate, kind);
        return;
    }

    let this_move_to_chain_number = get_next_move_to_chain_number(w, this);

    let Some(target_obj) = w.objects.get(target) else {
        panic!("ACE: target is null (NullReferenceException)");
    };
    if target_obj.location().is_none() {
        stop_existing_move_to_chains(w, this);
        log::error!("{this:?}.CreateMoveToChain({target:?}): target.Location is null");

        callback(w, false);
        return;
    }

    // fix bug in magic combat mode after walking to target,
    // crouch animation steps out of range
    let mut use_radius = use_radius.unwrap_or_else(|| target_obj.use_radius().unwrap_or(0.6));
    let target_is_portal = target_obj.biota.weenie_type == WeenieType::Portal;

    if creature_combat_mode(w, this) == CombatMode::Magic {
        use_radius = (use_radius - 0.2).max(0.0);
    }

    // already within use distance?
    let current_landblock = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: CurrentLandblock is null");
    let (within_use_radius, _target_valid) = crate::entity::landblock::within_use_radius(
        w,
        current_landblock,
        this,
        target,
        Some(use_radius),
    );
    if within_use_radius {
        if rotate {
            // send TurnTo motion
            let rotate_time = dispatch::rotate::rotate(w, this, target);
            let mut action_chain = ActionChain::new();
            action_chain.add_delay_seconds(w, f64::from(rotate_time));
            action_chain.add_action(Actor::Object(this), move |w: &mut World| {
                fields_mut(w, this).last_completed_move = this_move_to_chain_number;
                callback(w, true);
            });
            action_chain.enqueue_chain(w);
        } else {
            fields_mut(w, this).last_completed_move = this_move_to_chain_number;
            callback(w, true);
        }
        return;
    }

    if target_is_portal {
        let location = w
            .objects
            .get(target)
            .and_then(crate::world_objects::world_object::WorldObject::location)
            .expect("checked");
        creature_move_to_position(w, this, &location, RetailMoveTo::Portal);
    } else {
        creature_move_to_object(w, this, target, Some(use_radius), kind);
    }

    fields_mut(w, this).move_to_chain_start_time = w.now.utc;

    move_to_chain(
        w,
        this,
        target,
        this_move_to_chain_number,
        callback,
        Some(use_radius),
    );
}

// ACE: Player.MoveToChain
/// One step of the legacy chain: stops when superseded, portaled or logged out, timed out, or the
/// target is gone; otherwise checks again every 0.1 s until within the use radius.
pub fn move_to_chain(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    this_move_to_chain_number: i32,
    callback: MoveToCallback,
    use_radius: Option<f32>,
) {
    if this_move_to_chain_number != fields(w, this).move_to_chain_counter {
        let f = fields_mut(w, this);
        if this_move_to_chain_number > f.last_completed_move {
            f.last_completed_move = this_move_to_chain_number;
        }

        callback(w, false);
        return;
    }

    // Break loop if CurrentLandblock == null (we portaled or logged out)
    let Some(current_landblock) = w.objects.get(this).and_then(|o| o.current_landblock) else {
        stop_existing_move_to_chains(w, this); // This increments our moveToChainCounter and thus, should stop any additional actions in this chain
        callback(w, false);
        return;
    };

    // Have we timed out?
    if fields(w, this).move_to_chain_start_time + default_move_to_timeout() <= w.now.utc {
        stop_existing_move_to_chains(w, this); // This increments our moveToChainCounter and thus, should stop any additional actions in this chain
        callback(w, false);
        return;
    }

    // Are we within use radius?
    let (success, target_valid) =
        crate::entity::landblock::within_use_radius(w, current_landblock, this, target, use_radius);

    // If one of the items isn't on a landblock
    if !target_valid {
        stop_existing_move_to_chains(w, this); // This increments our moveToChainCounter and thus, should stop any additional actions in this chain
        callback(w, false);
        return;
    }

    if success {
        let f = fields_mut(w, this);
        if this_move_to_chain_number > f.last_completed_move {
            f.last_completed_move = this_move_to_chain_number;
        }

        callback(w, true);
    } else {
        // target not reached yet
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, 0.1);
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            move_to_chain(
                w,
                this,
                target,
                this_move_to_chain_number,
                callback,
                use_radius,
            );
        });
        action_chain.enqueue_chain(w);
    }
}

// ACE: Player.MoveTo
/// The melee charge: broadcasts a sticky MoveToObject and runs the body toward the target,
/// ticking every `MoveToRate` seconds while moving. `run_rate` 0 means the player's run rate.
pub fn player_move_to(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    run_rate: f32,
) {
    #[allow(clippy::float_cmp)] // C#'s == 0.0f
    let run_rate = if run_rate == 0.0 {
        creature_get_run_rate(w, this)
    } else {
        run_rate
    };

    //Console.WriteLine($"{Name}.MoveTo({target.Name})");

    let mut motion = Motion::to_object(w, this, target, MovementType::MoveToObject);
    // Not ACE's (retail captures, V257): retail's charge is the attack chase (0x1EFF0,
    // threshold 15.0); ACE adds the chase bits to the defaults (0x1EFFF) with 1.0.
    RetailMoveTo::AttackChase.apply(&mut motion.move_to_parameters);
    motion.move_to_parameters.speed = 1.5; // charge modifier
    motion.move_to_parameters.fail_distance = 15.0;
    motion.run_rate = run_rate;

    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .wo
        .world_object_properties
        .current_motion_state = Some(motion.clone());

    world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

    let mvp = get_charge_parameters();
    let h =
        phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
    if let Some(target_h) = phys_ext::physics_obj(w, target) {
        phys_ext::move_to_object(w, h, target_h, &mvp);
    }

    monster_navigation::fields_mut(w, this).is_moving = true;

    move_to_tick(w, this);
}

// ACE: Player.MoveToRate
pub const MOVE_TO_RATE: f32 = 0.1;

// ACE: Player.MoveTo_Tick
pub fn move_to_tick(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.MoveTo_Tick()");

    if let Some(h) = phys_ext::physics_obj(w, this) {
        phys_ext::update_object(w, h);
    }

    if monster_navigation::fields(w, this).is_moving {
        enqueue_next_move_tick(w, this);
    }
}

// ACE: Player.Enqueue_NextMoveTick
pub fn enqueue_next_move_tick(w: &mut World, this: ObjectGuid) {
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(MOVE_TO_RATE));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        move_to_tick(w, this)
    });
    action_chain.enqueue_chain(w);
}

// ACE: Player.OnMoveComplete
/// A charge (or a `MoveTo2` chain) ended: a successful charge attacks, a failed one reports the
/// error and cancels the attack.
pub fn player_on_move_complete(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    status: empyrean_entity::enums::WeenieError,
) {
    //Console.WriteLine($"{Name}.OnMoveComplete({status})");

    monster_navigation::fields_mut(w, this).is_moving = false;

    if player_move2::fields(w, this).is_player_moving_to2 {
        player_move2::on_move_complete_move_to2(w, this, status);

        if shims::player_magic_state_is_casting(w, this) {
            crate::world_objects::player_magic::on_move_complete_magic(w, this, status);
        }

        return;
    }

    if status == WeenieError::None {
        let attack_sequence = crate::world_objects::player_combat::fields(w, this).attack_sequence;
        match crate::world_objects::player_melee::fields(w, this).melee_target {
            Some(melee_target) => crate::world_objects::player_melee::attack(
                w,
                this,
                melee_target,
                attack_sequence,
                false,
            ),
            // `Attack(null, AttackSequence)`: the sequence matches, then `MeleeTarget == null`
            None => crate::world_objects::player_melee::on_attack_done(w, this, WeenieError::None),
        }
    } else {
        player_networking::send_weenie_error(w, this, status);
        crate::world_objects::player_melee::handle_action_cancel_attack(w, this, WeenieError::None);
    }
}

// ACE: Player.GetChargeParameters
/// Non-default parameters for the player melee charge.
#[must_use]
pub fn get_charge_parameters() -> MovementParameters {
    // set non-default params for player melee charge
    // Not ACE's (retail captures, V257): the body charges with the parameters the clients are
    // sent, retail's attack chase (0x1EFF0, threshold 15.0); ACE takes its own defaults without
    // CanWalk plus the chase bits (0x1EFFE, 1.0).
    let mut mvp = RetailMoveTo::AttackChase.movement_parameters();
    mvp.hold_key_to_apply = HoldKey::Run;
    mvp.fail_distance = 15.0;
    mvp.speed = 1.5;

    mvp
}

// ACE: Player.HandleFallingDamage
/// Landing: the vertical speed past the jump velocity plus 4.5 m/s of leeway becomes falling
/// damage. ACE's `EnvCollisionProfile` parameter is unused.
pub fn handle_falling_damage(w: &mut World, this: ObjectGuid) {
    // starting with phat logic

    // jumping skill sort of used as a damping factor here
    let jump_velocity = 11.254_34_f32; // TODO: figure out how to scale this better

    //var currVelocity = FastTick ? PhysicsObj.Velocity : PhysicsObj.CachedVelocity;
    let h =
        phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
    let curr_velocity = phys_ext::velocity(w, h);

    let overspeed = jump_velocity + curr_velocity.z + 4.5; // a little leeway

    let ratio = -overspeed / jump_velocity;

    if ratio > 0.0 {
        let damage = ratio * 87.293_81;

        // bludgeon damage
        // impact damage
        if damage > 0.0 {
            take_damage_falling(w, this, damage);
        }
    }
}

// ACE: Player.TakeDamage_Falling
pub fn take_damage_falling(w: &mut World, this: ObjectGuid, amount: f32) {
    let o = w.objects.get(this).expect("ACE: this is null");
    let is_dead = o.health().current(o) == 0;
    if is_dead || o.invincible() {
        return;
    }

    // handle lifestone protection?
    if o.under_lifestone_protection() {
        crate::world_objects::player_death::handle_lifestone_protection(w, this);
        return;
    }

    // scale by bludgeon protection
    let resistance = creature_get_resistance_mod(w, this, DamageType::Bludgeon);
    let damage: u32 =
        empyrean_common::dotnet::math::round(f64::from(amount * resistance)).cs_cast();

    // update health
    let health = w.objects.get(this).expect("ACE: this is null").health();
    let neg_damage: i32 = damage.cs_cast();
    let damage_taken: u32 =
        creature_vitals::update_vital_delta(w, this, health, neg_damage.wrapping_neg())
            .wrapping_neg()
            .cs_cast();
    crate::entity::damage_history::add(w, this, this, DamageType::Bludgeon, damage_taken);

    let health = w.objects.get(this).expect("ACE: this is null").health();
    let max_health = health.max_value(
        &mut crate::world_objects::entity::creature_attribute::StatCtx::in_world(w, this),
    );
    let msg = strings::get_fall_message(damage_taken, max_health);

    send_message(
        w,
        this,
        &msg,
        empyrean_entity::enums::ChatMessageType::Combat,
    );

    let o = w.objects.get(this).expect("ACE: this is null");
    if o.health().current(o) == 0 {
        let last_damager = Some(crate::entity::damage_history_info::DamageHistoryInfo::new(
            w, this, 0.0,
        ));
        crate::dispatch::on_death::on_death(w, this, last_damager, DamageType::Bludgeon, false);
        crate::world_objects::creature_death::die(w, this);
    } else {
        let sound = game_message_sound::game_message_sound(this, Sound::Wound3, 1.0);
        world_object_networking::enqueue_broadcast(w, this, true, &[sound]);
    }
}

// ---- Player.cs: jumping (hosted) --------------------------------------------------------------

// ACE: Player.HandleActionJump
/// The client jumped: the stamina cost, then the jump in the physics engine, an UpdateMotion
/// (the charged-jump fix) and the VectorUpdate broadcast.
pub fn handle_action_jump(w: &mut World, this: ObjectGuid, jump: &JumpPack) {
    let o = w.objects.get(this).expect("ACE: this is null");
    fields_mut(w, this).start_jump = o.location().map(|l| Position::from_position(&l));
    //Console.WriteLine($"JumpPack: Velocity: {jump.Velocity}, Extent: {jump.Extent}");

    let o = w.objects.get(this).expect("ACE: this is null");
    let strength_attr = o.strength();
    let strength = strength_attr
        .current(&mut crate::world_objects::entity::creature_attribute::StatCtx::in_world(w, this));
    let o = w.objects.get(this).expect("ACE: this is null");
    #[allow(clippy::cast_possible_wrap)] // `(int)strength`
    let capacity = crate::physics::weenie_object::encumbrance_system_encumbrance_capacity(
        strength as i32,
        o.augmentation_increased_carrying_capacity(),
    );
    let burden = dereth_rules::burden::load(capacity, o.encumbrance_val().unwrap_or(0));

    // calculate stamina cost for this jump
    let extent = jump.extent.clamp(0.0, 1.0);
    let stamina_cost = jump_stamina_cost(extent, burden, pk_timer_active(w, this));

    //Console.WriteLine($"Strength: {strength}, Capacity: {capacity}, Encumbrance: {EncumbranceVal ?? 0}, Burden: {burden}, StaminaCost: {staminaCost}");

    // ensure player has enough stamina to jump: ACE comments this out

    player::fields_mut(w, this).last_jump_time = w.now.utc;

    let stamina = w.objects.get(this).expect("ACE: this is null").stamina();
    creature_vitals::update_vital_delta(w, this, stamina, stamina_cost.wrapping_neg());

    //Console.WriteLine($"Jump velocity: {jump.Velocity}");

    // TODO: have server verify / scale magnitude
    let h =
        phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
    let fast_tick = player_tick::fast_tick(w, this);
    if fast_tick {
        phys_ext::restart_clock_if_idle(w, h);
    } else {
        let now = phys_ext::physics_timer_current_time(w);
        if let Some(o) = w.physics.get_mut(h) {
            o.update_time = now;
        }
    }

    // perform jump in physics engine
    if let Some(o) = w.physics.get_mut(h) {
        o.transient_state.set_contact(false);
        o.transient_state.set_water_contact(false);
    }
    phys_ext::calc_acceleration(w, h);
    phys_ext::set_on_walkable(w, h, false);
    let now = phys_ext::physics_timer_current_time(w);
    if let Some(o) = w.physics.get_mut(h) {
        o.set_local_velocity(
            Vec3::new(jump.velocity.x, jump.velocity.y, jump.velocity.z),
            now,
        );
    }
    crate::physics::motion::handle_enter_world(w, h); // RemoveLinkAnimations: matches MotionInterp.LeaveGround more closely
    phys_ext::clear_pending_motions(w, h); // hack; IsAnimating = false

    if fast_tick
        && creature_combat_mode(w, this) == CombatMode::Magic
        && shims::player_magic_state_is_casting(w, this)
    {
        // clear possible CastMotion out of InterpretedMotionState.ForwardCommand
        crate::physics::motion::minterp_stop_completely(w, h);
        crate::world_objects::player_magic::fail_cast(w, this, true);
    }

    // this shouldn't be needed, but without sending this update motion / simulated movement event beforehand,
    // running forward and then performing a charged jump does an uncharged shallow arc jump instead
    // this hack fixes that...
    let mut movement_data = MovementData::new(this);
    movement_data.is_autonomous = true;
    movement_data.movement_type = MovementType::Invalid;
    movement_data.invalid = Some(MovementInvalid::new(&movement_data));
    let numbering = w.dats.portal_dat().command_numbering();
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let msg = game_message_update_motion::game_message_update_motion(o, &movement_data, numbering);
    world_object_networking::enqueue_broadcast(w, this, true, &[msg]);

    // broadcast jump
    let msg = game_message_vector_update::game_message_vector_update(w, this);
    world_object_networking::enqueue_broadcast(w, this, true, &[msg]);

    if shims::player_magic_state_is_casting(w, this)
        && crate::world_objects::player_magic::fields(w, this)
            .record_cast
            .enabled
    {
        crate::entity::record_cast::on_jump(w, this, jump);
    }
}

// ---- the static helpers of ACE's physics port the jump uses (hosted) -------------------------

// ACE: MovementSystem.JumpStaminaCost
#[must_use]
pub fn jump_stamina_cost(power: f32, burden: f32, pk: bool) -> i32 {
    if pk {
        ((power + 1.0) * 100.0).cs_cast()
    } else {
        f64::from((burden + 0.5) * power * 8.0 + 2.0)
            .ceil()
            .cs_cast()
    }
}

// ---- pointers to members ported in other files --------------------------------------------------

/// `Player.PKTimerActive` (`Player_Combat.cs`): a PK type attacked within `pk_timer` seconds.
pub(crate) fn pk_timer_active(w: &World, this: ObjectGuid) -> bool {
    let o = w.objects.get(this).expect("ACE: this is null");
    let status = o.player_killer_status();
    let is_pk_type = status == empyrean_entity::enums::PlayerKillerStatus::PK
        || status == empyrean_entity::enums::PlayerKillerStatus::PKLite;
    #[allow(clippy::cast_precision_loss)] // C#'s long compared as double
    let pk_timer = crate::managers::property_manager::get_long(w, "pk_timer", 20, true).item as f64;
    is_pk_type && w.now.unix_time - o.last_pk_attack_timestamp() < pk_timer
}

/// `Creature.CombatMode` (`Creature_Combat.cs`, not a property).
pub(crate) fn creature_combat_mode(w: &World, this: ObjectGuid) -> CombatMode {
    let o = w.objects.get(this).expect("ACE: this is null");
    crate::world_objects::creature_combat::fields(o).combat_mode
}

/// `Creature.GetRunRate()` (`Monster_Navigation.cs`).
fn creature_get_run_rate(w: &mut World, this: ObjectGuid) -> f32 {
    crate::world_objects::monster_navigation::get_run_rate(w, this)
}

/// `Creature.MoveToObject(target, useRadius)` (`Creature.cs`).
pub(crate) fn creature_move_to_object(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    use_radius: Option<f32>,
    kind: RetailMoveTo,
) {
    crate::world_objects::creature::move_to_object(w, this, target, use_radius, kind);
}

/// `Creature.MoveToPosition(position)` (`Creature.cs`).
fn creature_move_to_position(
    w: &mut World,
    this: ObjectGuid,
    position: &Position,
    kind: RetailMoveTo,
) {
    crate::world_objects::creature::move_to_position(w, this, position, kind);
}

/// `Creature.SetWalkRunThreshold(motion, targetLocation)` (`Creature.cs`).
pub(crate) fn creature_set_walk_run_threshold(
    w: &mut World,
    this: ObjectGuid,
    motion: &mut Motion,
    target_location: &Position,
) {
    crate::world_objects::creature::set_walk_run_threshold(w, this, motion, target_location);
}

/// `Creature.GetResistanceMod(DamageType, null, null)` (`Creature_Properties.cs`).
fn creature_get_resistance_mod(w: &mut World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    crate::world_objects::creature_properties::get_resistance_mod_damage(
        w,
        this,
        damage_type,
        None,
        None,
        1.0,
    )
}

/// `Player.SendMessage(string, ChatMessageType)` (`Player_Chat.cs`): the chat line to the player's
/// session. The squelch check is `SquelchManager`'s (not ported: nothing is squelched).
fn send_message(
    w: &mut World,
    this: ObjectGuid,
    msg: &str,
    chat_message_type: empyrean_entity::enums::ChatMessageType,
) {
    let m =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            msg,
            chat_message_type,
        );
    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    crate::network::game_messages::game_message::enqueue_send(w, session, m);
}
