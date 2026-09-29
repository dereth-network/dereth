// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Missile.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Missile.cs`: the monster missile attack
//! (aim, launch, reload) and the switch to melee.
//!
//! The projectile maths and launch are `Creature_Missile.cs`'s (`creature_missile.rs`).

use empyrean_entity::enums::{
    CombatMode, CombatStyle, MotionCommand, MotionStance, ParentLocation, Placement,
};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::base_damage_mod::BaseDamageMod;
use crate::entity::timers;
use crate::network::game_messages::messages::game_message_parent_event::game_message_parent_event;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::physics::motion_table;
use crate::world_objects::world_object;
use crate::world_objects::world_object_networking::{
    enqueue_broadcast, enqueue_motion, enqueue_motion_force,
};
use crate::world_objects::{creature_combat, creature_missile, monster_combat, monster_melee};
use crate::world_objects::{
    creature_equipment, monster_inventory, monster_navigation, monster_tick,
};
use crate::World;

/// The delay between missile attacks (todo: find actual value).
// ACE: Creature.MissileDelay
pub const MISSILE_DELAY: f32 = 1.0;

/// Non-property fields declared in `Monster_Missile.cs`.
#[derive(Debug, Default)]
pub struct MonsterMissileFields {
    // ACE: Creature.MonsterProjectile_OnCollideEnvironment_Counter
    pub monster_projectile_on_collide_environment_counter: i32,
    // ACE: Creature.SwitchWeaponsPending
    pub switch_weapons_pending: bool,
}

/// `Creature`'s `Monster_Missile.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterMissileFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_missile
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterMissileFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_missile
}

/// Returns TRUE if monster has physical ranged attacks.
// ACE: Creature.IsRanged
#[must_use]
pub fn is_ranged(w: &World, this: ObjectGuid) -> bool {
    creature_equipment::get_equipped_missile_weapon(w, this).is_some()
}

/// Starts a monster missile attack.
// ACE: Creature.RangeAttack
pub fn range_attack(w: &mut World, this: ObjectGuid) {
    let target = monster_combat::attack_target_creature(w, this);

    let Some(_target) =
        target.filter(|&t| !monster_combat::is_dead(w.objects.get(t).expect("resolved")))
    else {
        dispatch::find_next_target::find_next_target(w, this);
        return;
    };

    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let ammo = creature_equipment::get_equipped_ammo(w, this);

    let Some(weapon) = weapon else { return };
    if w.objects.get(weapon).expect("equipped").is_ammo_launcher() && ammo.is_none() {
        return;
    }

    // simulate accuracy bar / allow client rotate to fully complete
    let mut action_chain = ActionChain::new();
    //IsTurning = true;
    //actionChain.AddDelaySeconds(0.5f);

    // do missile attack
    action_chain.add_action(Actor::Object(this), move |w| launch_missile(w, this));
    action_chain.enqueue_chain(w);
}

/// Launches a missile attack from monster to target: the aim motion, the projectile at its end,
/// the reload, and the next attack after the whole sequence plus `MissileDelay`.
// ACE: Creature.LaunchMissile
pub fn launch_missile(w: &mut World, this: ObjectGuid) {
    //IsTurning = false;

    // DIVERGE: the action holds a creature destroyed since it was queued (an `@create`d monster
    // that rotted, `WorldObject.Decay`, mid-attack; V207). ACE's closure keeps the dead
    // object and runs on (its EquippedObjects still listed, its PhysicsObj gone); here it is gone
    // from the store, and the launch is dropped.
    if w.objects.get(this).is_none() {
        return;
    }

    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let (Some(weapon), Some(attack_target)) = (weapon, monster_combat::attack_target(w, this))
    else {
        return;
    };

    // Not ACE's (a fix, V325): a target whose log-off finished is no target: no
    // shot, and the monster looks for another, as for a dead one. ACE aimed at the destroyed body's
    // frame in landblock 0.
    if creature_missile::target_left_world(w, attack_target) {
        dispatch::find_next_target::find_next_target(w, this);
        return;
    }

    let ammo = if w.objects.get(weapon).expect("equipped").is_ammo_launcher() {
        creature_equipment::get_equipped_ammo(w, this)
    } else {
        Some(weapon)
    };
    let Some(ammo) = ammo else { return };

    let launcher = creature_equipment::get_equipped_missile_launcher(w, this);

    /*if (!IsDirectVisible(AttackTarget)) ... */
    if fields(w, this).switch_weapons_pending {
        monster_combat::fields_mut(w, this).next_attack_time =
            timers::running_time(w) + f64::from(1.0f32);
        return;
    }

    // should this be called each launch?
    let height = monster_combat::choose_attack_height(w, this);
    monster_combat::fields_mut(w, this).attack_height = Some(height);

    let _dist = monster_navigation::get_distance_to_target(w, this);
    //Console.WriteLine("RangeAttack: " + dist);

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    let projectile_speed = creature_missile::get_projectile_speed(w, this);

    // get z-angle for aim motion
    let aim_velocity = creature_missile::get_aim_velocity(w, this, attack_target, projectile_speed);

    let aim_level = creature_missile::get_aim_level(aim_velocity);

    // calculate projectile spawn pos and velocity
    let ammo_wcid = w.objects.get(ammo).expect("equipped").biota.weenie_class_id;
    let local_origin = creature_missile::get_projectile_spawn_origin(w, this, ammo_wcid, aim_level);

    let (velocity, origin, orientation) = creature_missile::calculate_projectile_velocity(
        w,
        this,
        local_origin,
        attack_target,
        projectile_speed,
    );

    //Console.WriteLine($"Velocity: {velocity}");

    // launch animation
    let mut action_chain = ActionChain::new();
    let launch_time = enqueue_motion(
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
    //Console.WriteLine("LaunchTime: " + launchTime);

    // launch projectile
    action_chain.add_action(Actor::Object(this), move |w| {
        if w.objects.get(this).is_none_or(monster_combat::is_dead) {
            return;
        }

        // handle self-procs
        monster_melee::try_proc_equipped_items(w, this, this, true, Some(weapon));

        let sound = creature_missile::get_launch_missile_sound(w, weapon);
        enqueue_broadcast(w, this, true, &[game_message_sound(this, sound, 1.0)]);

        // TODO: monster stamina usage

        // Not ACE's (a fix, V325): nothing is fired at a target whose log-off
        // finished during the windup.
        if let Some(attack_target) = monster_combat::attack_target(w, this)
            .filter(|&t| !creature_missile::target_left_world(w, t))
        {
            let _projectile = creature_missile::launch_projectile(
                w,
                this,
                launcher,
                ammo,
                attack_target,
                origin,
                orientation,
                velocity,
            );
            dispatch::update_ammo_after_launch::update_ammo_after_launch(w, this, ammo);
        }
    });

    // will ammo be depleted?
    /*if (ammo.StackSize == null || ammo.StackSize <= 1) ... */

    // reload animation
    let anim_speed = creature_combat::get_anim_speed(w, this);
    let reload_time = enqueue_motion(
        w,
        this,
        &mut action_chain,
        MotionCommand::Reload,
        anim_speed,
        true,
        None,
        false,
        false,
    );
    //Console.WriteLine("ReloadTime: " + reloadTime);

    // reset for next projectile
    enqueue_motion(
        w,
        this,
        &mut action_chain,
        MotionCommand::Ready,
        1.0,
        true,
        None,
        false,
        false,
    );

    let link_anim = if reload_time > 0.0 {
        MotionCommand::Reload
    } else {
        aim_level
    };

    let motion_table_id = w.objects.get(this).expect("ACE: this").motion_table_id();
    let stance = monster_tick::current_stance(w, this);
    let link_time = motion_table::get_animation_length_between(
        w,
        motion_table_id,
        stance,
        link_anim,
        MotionCommand::Ready,
        1.0,
    );

    if w.objects.get(weapon).expect("equipped").is_thrown_weapon() {
        if reload_time > 0.0 {
            action_chain.enqueue_chain(w);
            action_chain = ActionChain::new();
        }

        action_chain.add_delay_seconds(w, f64::from(link_time));
    }

    //log.Info($"{Name}.Reload time: launchTime({launchTime}) + reloadTime({reloadTime}) + linkTime({linkTime})");

    action_chain.add_action(Actor::Object(this), move |w| {
        let Some((creature, item)) = w.objects.get2_mut(this, ammo) else {
            return;
        };
        let msg = game_message_parent_event(
            creature,
            item,
            Some(ParentLocation::RightHand),
            Some(Placement::RightHandCombat),
        );
        enqueue_broadcast(w, this, true, &[msg]);
    });

    action_chain.enqueue_chain(w);

    let now = timers::running_time(w);
    monster_combat::fields_mut(w, this).prev_attack_time = now;

    let time_offset = launch_time + reload_time + link_time;

    let next = now + f64::from(time_offset) + f64::from(MISSILE_DELAY);
    monster_combat::fields_mut(w, this).next_attack_time = next;
    monster_navigation::fields_mut(w, this).next_move_time = next;
}

/// Returns missile base damage from a monster attack.
// ACE: Creature.GetMissileDamage
pub fn get_missile_damage(w: &mut World, this: ObjectGuid) -> BaseDamageMod {
    // FIXME: use actual projectile, instead of currently equipped ammo
    let ammo = creature_equipment::get_missile_ammo(w, this)
        .expect("ACE: GetMissileAmmo() is null (NullReferenceException)");

    crate::world_objects::world_object::get_damage_mod(w, ammo, this, None)
}

/// A projectile of this monster hit the environment: after three, it tries melee.
// ACE: Creature.MonsterProjectile_OnCollideEnvironment
pub fn monster_projectile_on_collide_environment(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.MonsterProjectile_OnCollideEnvironment()");
    fields_mut(w, this).monster_projectile_on_collide_environment_counter += 1;

    // chance of switching to melee, or static counter in retail?
    /*var rng = ThreadSafeRandom.Next(1, 3);
    if (rng == 3)
        SwitchToMeleeAttack();*/

    if fields(w, this).monster_projectile_on_collide_environment_counter >= 3 {
        try_switch_to_melee_attack(w, this);
    }
}

/// Switches to melee once the current move/attack is over (never for a stubborn missile monster
/// or an invisible one).
// ACE: Creature.TrySwitchToMeleeAttack
pub fn try_switch_to_melee_attack(w: &mut World, this: ObjectGuid) {
    // 24139 - Invisible Assailant never switches to melee?
    let o = w.objects.get(this).expect("ACE: this");
    if o.ai_allowed_combat_style() == CombatStyle::StubbornMissile || o.visibility() {
        return;
    }

    fields_mut(w, this).switch_weapons_pending = true;

    let now = timers::running_time(w);
    let next_move_time = monster_navigation::fields(w, this).next_move_time;
    if next_move_time > now {
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, next_move_time - now);
        action_chain.add_action(Actor::Object(this), move |w| {
            switch_to_melee_attack(w, this)
        });
        action_chain.enqueue_chain(w);
    } else {
        switch_to_melee_attack(w, this);
    }
}

/// Puts the missile weapon away (destroying it and its ammo so they cannot be re-selected),
/// wields the inventory's melee weapons and returns to melee stance.
// ACE: Creature.SwitchToMeleeAttack
pub fn switch_to_melee_attack(w: &mut World, this: ObjectGuid) {
    if w.objects.get(this).is_none_or(monster_combat::is_dead) {
        return;
    }

    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let ammo = creature_equipment::get_equipped_ammo(w, this);

    if weapon.is_none() && ammo.is_none() {
        return;
    }

    let mut action_chain = ActionChain::new();

    let stance = monster_tick::current_stance(w, this);
    enqueue_motion_force(
        w,
        this,
        &mut action_chain,
        MotionStance::NonCombat,
        MotionCommand::Ready,
        Some(MotionCommand(stance.0)),
        1.0,
        1.0,
    );

    enqueue_motion_force(
        w,
        this,
        &mut action_chain,
        MotionStance::HandCombat,
        MotionCommand::Ready,
        Some(MotionCommand::NonCombat),
        1.0,
        1.0,
    );

    action_chain.add_action(Actor::Object(this), move |w| {
        if w.objects.get(this).is_none_or(monster_combat::is_dead) {
            return;
        }

        // actually destroys the missile weapon + ammo here,
        // to ensure they can't be re-selected from inventory
        if let Some(weapon) = weapon {
            creature_equipment::try_unwield_object_with_broadcasting(w, this, weapon, false);
            world_object::destroy(w, weapon, true, false);
        }

        if let Some(ammo) = ammo {
            creature_equipment::try_unwield_object_with_broadcasting(w, this, ammo, false);
            world_object::destroy(w, ammo, true, false);
        }

        monster_inventory::equip_inventory_items(w, this, true);

        let mut inner_chain = ActionChain::new();

        let stance = monster_tick::current_stance(w, this);
        enqueue_motion_force(
            w,
            this,
            &mut inner_chain,
            MotionStance::NonCombat,
            MotionCommand::Ready,
            Some(MotionCommand(stance.0)),
            1.0,
            1.0,
        );

        inner_chain.add_action(Actor::Object(this), move |w| {
            if w.objects.get(this).is_none_or(monster_combat::is_dead) {
                return;
            }

            //DoAttackStance();

            // inlined DoAttackStance() / slightly modified -- do not rely on SetCombatMode() for stance swapping time in 1 action,
            // as it doesn't support that anymore

            let new_stance_time = creature_combat::set_combat_mode(w, this, CombatMode::Melee);

            let next = timers::running_time(w) + f64::from(new_stance_time);
            monster_navigation::fields_mut(w, this).next_move_time = next;
            monster_combat::fields_mut(w, this).next_attack_time = next;

            let ai_use_magic_delay = w
                .objects
                .get(this)
                .expect("ACE: this")
                .ai_use_magic_delay()
                .unwrap_or(f64::from(3.0f32));
            monster_combat::fields_mut(w, this).prev_attack_time = next - ai_use_magic_delay;

            let h = monster_navigation::physics_obj(w, this);
            monster_navigation::physics_obj_start_timer(w, h);

            // end inline

            monster_combat::reset_attack(w, this);

            fields_mut(w, this).switch_weapons_pending = false;

            // this is an unfortunate hack to fix the following scenario:

            // since this function can be called at any point in time now,
            // including when LaunchMissile -> EnqueueMotion is in the middle of an action queue,
            // CurrentMotionState.Stance can get reset to the previous combat stance if that happens

            let new_stance = monster_tick::current_stance(w, this);

            let mut swap_chain = ActionChain::new();
            swap_chain.add_delay_seconds(w, f64::from(2.0f32));
            swap_chain.add_action(Actor::Object(this), move |w| {
                if let Some(m) = w
                    .objects
                    .get_mut(this)
                    .and_then(|o| o.wo.world_object_properties.current_motion_state.as_mut())
                {
                    m.stance = new_stance;
                }
            });
            swap_chain.enqueue_chain(w);
        });
        inner_chain.enqueue_chain(w);
    });
    action_chain.enqueue_chain(w);
}
