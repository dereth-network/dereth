// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Missile.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Missile.cs`.
//!
//! The player missile attack: the targeted attack request, the aim and launch through
//! `Creature_Missile` (projectile, ammo use, stamina), the reload, the accuracy bar refill and
//! the repeat attacks.

use empyrean_entity::enums::{
    AttackHeight, CharacterOption, CombatMode, MotionCommand, MotionStance, PowerAccuracy,
    WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::landblock;
use crate::network::game_event::events::game_event_attack_done::game_event_attack_done;
use crate::network::game_event::events::game_event_combat_commence_attack::game_event_combat_commence_attack;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_pickup_event::game_message_pickup_event;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::physics::motion_table;
use crate::world_objects::creature_combat::{self, shim};
use crate::world_objects::player_inventory::DequipObjectAction;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_equipment, creature_missile, creature_vitals, monster_combat, monster_navigation,
    player_combat, player_death, player_inventory, player_melee, player_networking,
    world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Player_Missile.cs`.
#[derive(Debug, Default)]
pub struct PlayerMissileFields {
    /// `_accuracyLevel`: the accuracy bar level, 0-1 (read through [`accuracy_level`]).
    pub accuracy_level: f32,
    // ACE: Player.MissileTarget
    pub missile_target: Option<ObjectGuid>,
}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn object_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// This player's `Player_Missile` fields.
///
/// # Panics
/// When `this` is not a live player (ACE: `InvalidCastException`).
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerMissileFields {
    &object(w, this)
        .player
        .as_ref()
        .expect("InvalidCastException: not a Player")
        .player_missile
}

/// This player's `Player_Missile` fields, mutably.
///
/// # Panics
/// When `this` is not a live player (ACE: `InvalidCastException`).
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerMissileFields {
    &mut object_mut(w, this)
        .player
        .as_mut()
        .expect("InvalidCastException: not a Player")
        .player_missile
}

/// `creature.IsAlive` for a guid captured by a closure (a creature gone from the store is dead).
fn creature_is_alive(w: &World, g: ObjectGuid) -> bool {
    w.objects
        .get(g)
        .is_some_and(|o| o.is_creature() && !monster_combat::is_dead(o))
}

/// `EnqueueBroadcast(msgs)`.
fn enqueue_broadcast(w: &mut World, this: ObjectGuid, msgs: &[GameMessage]) {
    let _ = world_object_networking::enqueue_broadcast(w, this, true, msgs);
}

/// `Session.Network.EnqueueSend(msg)` for a session-bound game event.
fn send_attack_done(w: &mut World, this: ObjectGuid) {
    let s = player_combat::session(w, this);
    let msg = game_event_attack_done(session_data(w, s), WeenieError::None);
    enqueue_send(w, s, msg);
}

/// `TryProcEquippedItems(attacker, target, selfTarget, weapon)` (`WorldObject_Combat.cs`), called
/// on the player itself (`this` is `attacker`).
fn try_proc_equipped_items(
    w: &mut World,
    attacker: ObjectGuid,
    target: ObjectGuid,
    self_target: bool,
    weapon: Option<ObjectGuid>,
) {
    crate::world_objects::world_object_combat::try_proc_equipped_items(
        w,
        attacker,
        attacker,
        target,
        self_target,
        weapon,
    );
}

/// The accuracy bar level, 0-1 (0 while exhausted).
// ACE: Player.AccuracyLevel
#[must_use]
pub fn accuracy_level(w: &World, this: ObjectGuid) -> f32 {
    if crate::world_objects::creature::is_exhausted(w.objects.get(this).expect("ACE: this is null"))
    {
        0.0
    } else {
        fields(w, this).accuracy_level
    }
}

// ACE: Player.AccuracyLevel
pub fn set_accuracy_level(w: &mut World, this: ObjectGuid, value: f32) {
    fields_mut(w, this).accuracy_level = value;
}

/// The accuracy bar's third: Low below 0.33, Medium below 0.66, else High.
// ACE: Player.GetAccuracyRange
#[must_use]
pub fn get_accuracy_range(w: &World, this: ObjectGuid) -> PowerAccuracy {
    let accuracy_level = accuracy_level(w, this);
    if accuracy_level < 0.33 {
        PowerAccuracy::Low
    } else if accuracy_level < 0.66 {
        PowerAccuracy::Medium
    } else {
        PowerAccuracy::High
    }
}

/// Called by network packet handler 0xA (GameActionTargetedMissileAttack): `attack_height` is
/// 1-3, `accuracy_level` the 0-1 accuracy bar.
// ACE: Player.HandleActionTargetedMissileAttack
pub fn handle_action_targeted_missile_attack(
    w: &mut World,
    this: ObjectGuid,
    target_guid: u32,
    attack_height: u32,
    accuracy_level: f32,
) {
    //log.Info($"-");

    if creature_combat::combat_mode(w, this) != CombatMode::Missile {
        log::warn!(
            "{}.HandleActionTargetedMissileAttack({target_guid:08X}, {attack_height}, {accuracy_level}) - CombatMode mismatch {}, LastCombatMode: {}",
            shim::name(w, this),
            creature_combat::combat_mode(w, this).to_dotnet_string(),
            player_combat::fields(w, this).last_combat_mode.to_dotnet_string()
        );

        if player_combat::fields(w, this).last_combat_mode == CombatMode::Missile {
            creature_combat::set_combat_mode_field(w, this, CombatMode::Missile);
        } else {
            player_melee::on_attack_done(w, this, WeenieError::None);
            return;
        }
    }

    if player_melee::is_busy(w, this)
        || player_melee::teleporting(w, this)
        || player_melee::suicide_in_progress(w, this)
    {
        player_networking::send_weenie_error(w, this, WeenieError::YoureTooBusy);
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    if player_melee::is_jumping(w, this) {
        player_networking::send_weenie_error(w, this, WeenieError::YouCantDoThatWhileInTheAir);
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    if player_melee::pk_logout(w, this) {
        player_networking::send_weenie_error(
            w,
            this,
            WeenieError::YouHaveBeenInPKBattleTooRecently,
        );
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let ammo = creature_equipment::get_equipped_ammo(w, this);

    // sanity check
    let accuracy_level = math_clamp_f32(accuracy_level, 0.0, 1.0);

    if weapon.is_none_or(|g| object(w, g).is_ammo_launcher() && ammo.is_none()) {
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    player_melee::set_attack_height(w, this, Some(AttackHeight(attack_height.cast_signed())));
    player_melee::fields_mut(w, this)
        .attack_queue
        .add(accuracy_level);

    if fields(w, this).missile_target.is_none() {
        set_accuracy_level(w, this, accuracy_level); // verify
    }

    // get world object of target guid
    let target = object(w, this)
        .current_landblock
        .and_then(|lb| landblock::get_object(w, lb, ObjectGuid::new(target_guid), true))
        .filter(|&g| object(w, g).is_creature());
    let Some(target) = target.filter(|&g| !object(w, g).wo.world_object.teleporting) else {
        //log.Warn($"{Name}.HandleActionTargetedMissileAttack({targetGuid:X8}, {AttackHeight}, {accuracyLevel}) - couldn't find creature target guid");
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    };

    if player_combat::fields(w, this).attacking
        || fields(w, this)
            .missile_target
            .is_some_and(|t| creature_is_alive(w, t))
    {
        return;
    }

    if !dispatch::can_damage::can_damage(w, this, target) {
        let msg = format!("You cannot attack {}", shim::name(w, target));
        player_networking::send_transient_error(w, this, &msg);
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    //log.Info($"{Name}.HandleActionTargetedMissileAttack({targetGuid:X8}, {attackHeight}, {accuracyLevel})");

    player_melee::set_attack_target(w, this, Some(target));
    fields_mut(w, this).missile_target = Some(target);

    let attack_sequence = {
        let f = player_combat::fields_mut(w, this);
        f.attack_sequence = f.attack_sequence.wrapping_add(1);
        f.attack_sequence
    };

    // record stance here and pass it along
    // accounts for odd client behavior with swapping bows during repeat attacks
    let stance = player_melee::current_stance(w, this);

    // turn if required
    let rotate_time = dispatch::rotate::rotate(w, this, target);
    let mut action_chain = ActionChain::new();

    let mut delay_time = rotate_time;
    let next_refill_time = player_combat::fields(w, this).next_refill_time;
    if next_refill_time > w.now.utc.add_seconds(f64::from(delay_time)) {
        #[allow(clippy::cast_possible_truncation)] // C#'s `(float)`
        {
            delay_time = (next_refill_time - w.now.utc).total_seconds() as f32;
        }
    }

    action_chain.add_delay_seconds(w, f64::from(delay_time));

    // do missile attack
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        launch_missile(w, this, target, attack_sequence, stance, false);
    });
    action_chain.enqueue_chain(w);
}

/// `Math.Clamp(float, float, float)` (min <= max): a NaN passes through.
fn math_clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Launches a missile attack from player to target: the aim motion, then (on the chain) the
/// launch sound, the stamina cost, the projectile and the ammo use; then either out of ammo, or
/// the reload and, with repeat attacks on, the next launch once the accuracy bar has refilled.
/// ACE's default: `subsequent = false`.
// ACE: Player.LaunchMissile
#[allow(clippy::too_many_lines)]
pub fn launch_missile(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    attack_sequence: i32,
    stance: MotionStance,
    subsequent: bool,
) {
    if player_combat::fields(w, this).attack_sequence != attack_sequence {
        return;
    }

    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let Some(weapon) =
        weapon.filter(|_| creature_combat::combat_mode(w, this) != CombatMode::NonCombat)
    else {
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    };

    let ammo = if object(w, weapon).is_ammo_launcher() {
        creature_equipment::get_equipped_ammo(w, this)
    } else {
        Some(weapon)
    };
    let Some(ammo) = ammo else {
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    };

    let launcher = creature_equipment::get_equipped_missile_launcher(w, this);

    let creature = target;
    if monster_combat::is_dead(object(w, this))
        || player_melee::is_busy(w, this)
        || fields(w, this).missile_target.is_none()
        || !creature_is_alive(w, creature)
        || player_melee::suicide_in_progress(w, this)
        // Not ACE's (fix, V325): a target whose log-off finished is not shot
        // at; ACE aimed at its destroyed body's frame in landblock 0.
        || creature_missile::target_left_world(w, target)
    {
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    if !target_in_range(w, this, target) {
        // this must also be sent to actually display the transient message
        player_networking::send_weenie_error(w, this, WeenieError::MissileOutOfRange);

        // this prevents the accuracy bar from refilling when 'repeat attacks' is enabled
        player_melee::on_attack_done(w, this, WeenieError::None);

        return;
    }

    let mut action_chain = ActionChain::new();

    if subsequent && !monster_navigation::is_facing(w, this, Some(target)) {
        let rotate_time = dispatch::rotate::rotate(w, this, target);
        action_chain.add_delay_seconds(w, f64::from(rotate_time));
    }

    // launch animation
    // point of no return beyond this point -- cannot be cancelled
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        player_combat::fields_mut(w, this).attacking = true;
    });

    if subsequent {
        // client shows hourglass, until attack done is received
        // retail only did this for subsequent attacks w/ repeat attacks on
        let s = player_combat::session(w, this);
        let msg = game_event_combat_commence_attack(session_data(w, s));
        enqueue_send(w, s, msg);
    }

    let projectile_speed = creature_missile::get_projectile_speed(w, this);

    // get z-angle for aim motion
    let aim_velocity = creature_missile::get_aim_velocity(w, this, target, projectile_speed);

    let aim_level = creature_missile::get_aim_level(aim_velocity);

    // calculate projectile spawn pos and velocity
    let ammo_wcid = object(w, ammo).biota.weenie_class_id;
    let local_origin = creature_missile::get_projectile_spawn_origin(w, this, ammo_wcid, aim_level);

    let (velocity, origin, orientation) = creature_missile::calculate_projectile_velocity(
        w,
        this,
        local_origin,
        target,
        projectile_speed,
    );

    //Console.WriteLine($"Velocity: {velocity}");

    if velocity.equals(empyrean_common::dotnet::numerics::Vector3::ZERO) {
        // pre-check succeeded, but actual velocity calculation failed
        player_networking::send_weenie_error(w, this, WeenieError::MissileOutOfRange);

        // this prevents the accuracy bar from refilling when 'repeat attacks' is enabled
        player_combat::fields_mut(w, this).attacking = false;
        player_melee::on_attack_done(w, this, WeenieError::None);
        return;
    }

    let _launch_time = world_object_networking::enqueue_motion_persist(
        w,
        this,
        &mut action_chain,
        aim_level,
        1.0,
        true,
        None,
        false,
        false,
    );

    // launch projectile
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        // handle self-procs
        try_proc_equipped_items(w, this, this, true, Some(weapon));

        let sound = creature_missile::get_launch_missile_sound(w, weapon);
        enqueue_broadcast(w, this, &[game_message_sound(this, sound, 1.0)]);

        // stamina usage
        // TODO: ensure enough stamina for attack
        // TODO: verify formulas - double/triple cost for bow/xbow?
        let accuracy_range = get_accuracy_range(w, this);
        let stamina_cost = player_combat::get_attack_stamina(w, this, accuracy_range);
        let stamina = object(w, this).stamina();
        creature_vitals::update_vital_delta(w, this, stamina, stamina_cost.wrapping_neg());

        // Not ACE's (fix, V325): a target whose log-off finished during the
        // windup is not shot at, and the ammo is kept.
        if creature_missile::target_left_world(w, target) {
            return;
        }

        let _projectile = creature_missile::launch_projectile(
            w,
            this,
            launcher,
            ammo,
            target,
            origin,
            orientation,
            velocity,
        );
        dispatch::update_ammo_after_launch::update_ammo_after_launch(w, this, ammo);
    });

    // ammo remaining?
    let a = object(w, ammo);
    if !a.unlimited_use() && a.stack_size().is_none_or(|s| s <= 1) {
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            // Not ACE's (V325): no shot was fired at a target that left the world, so the last
            // ammo is still held; the attack just ends.
            if creature_missile::target_left_world(w, target) {
                player_combat::fields_mut(w, this).attacking = false;
                player_melee::on_attack_done(w, this, WeenieError::None);
                return;
            }

            let s = player_combat::session(w, this);
            let msg = game_event_communication_transient_string(
                session_data(w, s),
                "You are out of ammunition!",
            );
            enqueue_send(w, s, msg);
            creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
            player_combat::fields_mut(w, this).attacking = false;
            player_melee::on_attack_done(w, this, WeenieError::None);
        });

        action_chain.enqueue_chain(w);
        return;
    }

    // reload animation
    let anim_speed = creature_combat::get_anim_speed(w, this);
    let _reload_time = world_object_networking::enqueue_motion_persist_stance(
        w,
        this,
        &mut action_chain,
        stance,
        MotionCommand::Reload,
        anim_speed,
    );

    // reset for next projectile
    world_object_networking::enqueue_motion_persist_stance(
        w,
        this,
        &mut action_chain,
        stance,
        MotionCommand::Ready,
        1.0,
    );
    let link_time = motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        stance,
        MotionCommand::Reload,
        MotionCommand::Ready,
        1.0,
    );
    //var cycleTime = MotionTable.GetCycleLength(MotionTableId, CurrentMotionState.Stance, MotionCommand.Ready);

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if creature_combat::combat_mode(w, this) == CombatMode::Missile {
            creature_missile::broadcast_ammo_parent_event(w, this, ammo);
        }
    });

    action_chain.add_delay_seconds(w, f64::from(link_time));

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        player_combat::fields_mut(w, this).attacking = false;

        if creature_is_alive(w, creature)
            && player_melee::get_character_option(w, this, CharacterOption::AutoRepeatAttacks)
            && !player_melee::is_busy(w, this)
            && !player_combat::fields(w, this).attack_cancelled
        {
            // client starts refilling accuracy bar
            send_attack_done(w, this);

            let p = player_melee::fields_mut(w, this).attack_queue.fetch();
            set_accuracy_level(w, this, p);

            // can be cancelled, but cannot be pre-empted with another attack
            let mut next_attack = ActionChain::new();
            let next_refill_time = accuracy_level(w, this);

            let t = w.now.utc.add_seconds(f64::from(next_refill_time));
            player_combat::fields_mut(w, this).next_refill_time = t;
            next_attack.add_delay_seconds(w, f64::from(next_refill_time));

            // perform next attack
            next_attack.add_action(Actor::Object(this), move |w: &mut World| {
                launch_missile(w, this, target, attack_sequence, stance, true);
            });
            next_attack.enqueue_chain(w);
        } else {
            player_melee::on_attack_done(w, this, WeenieError::None);
        }
    });

    action_chain.enqueue_chain(w);

    if object(w, this).under_lifestone_protection() {
        player_death::lifestone_protection_dispel(w, this);
    }
}

// TODO: the damage pipeline currently uses the creature ammo instead of the projectile
// for calculating damage. when the last arrow is launched, the player ammo will be null
// give projectiles an owner, and have the damage pipeline take the actual damage source object
// (ie. the arrow-in-flight, or a melee weapon)

/// A divisor of the target's height for the aim point, from the attack height (High 1, Medium 2,
/// Low 3).
///
/// # Panics
/// With no attack height (ACE: `InvalidOperationException` on `AttackHeight.Value`).
// ACE: Player.GetAimHeight
#[must_use]
pub fn get_aim_height(w: &World, this: ObjectGuid, _target: ObjectGuid) -> f32 {
    match player_melee::attack_height(w, this).expect(
        "System.InvalidOperationException: Nullable object must have a value (AttackHeight)",
    ) {
        AttackHeight::High => 1.0,
        AttackHeight::Medium => 2.0,
        //case AttackHeight.Low: return target.Height;
        AttackHeight::Low => 3.0,
        _ => 2.0,
    }
}

/// Hides the launched ammo, then uses one: the last of a stack is dequipped as consumed, else the
/// stack shrinks by one (unlimited ammo is not used up).
// ACE: Player.UpdateAmmoAfterLaunch
pub fn update_ammo_after_launch(w: &mut World, this: ObjectGuid, ammo: ObjectGuid) {
    //if (ammo.UnlimitedUse)
    //    return;

    // hide previously held ammo
    let msg = game_message_pickup_event(object_mut(w, ammo));
    enqueue_broadcast(w, this, &[msg]);

    if object(w, ammo).unlimited_use() {
        return;
    }

    if object(w, ammo).stack_size().is_none_or(|s| s <= 1) {
        let _ = player_inventory::try_dequip_object_with_networking(
            w,
            this,
            ammo,
            DequipObjectAction::ConsumeItem,
        );
    } else {
        let _ = player_inventory::try_consume_from_inventory_with_networking(w, this, ammo, 1);
    }
}

/// Whether the target is within the missile weapon's range (3D distance).
// ACE: Player.TargetInRange
#[must_use]
pub fn target_in_range(w: &World, this: ObjectGuid, target: ObjectGuid) -> bool {
    // 2d or 3d distance?
    let loc = object(w, this)
        .location()
        .expect("System.NullReferenceException: Location");
    let target_loc = object(w, target)
        .location()
        .expect("System.NullReferenceException: target.Location");
    let dist = loc.distance_to(&target_loc);

    let max_range = creature_missile::get_max_missile_range(w, this);

    dist <= max_range
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Player.GetAimHeight
pub fn player_get_aim_height(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) -> f32 {
    get_aim_height(w, this, target)
}

// ACE: Player.UpdateAmmoAfterLaunch
pub fn player_update_ammo_after_launch(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    ammo: empyrean_entity::ObjectGuid,
) {
    update_ammo_after_launch(w, this, ammo);
}
