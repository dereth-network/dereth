// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Melee.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Melee.cs`: the monster melee attack (combat
//! maneuver, swing motion, strikes at the attack frames), and the armor helpers the damage
//! calculation uses.
//!
//! The strike's damage is `DamageEvent.CalculateDamage`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dereth_animation::hooks::AttackCone;
use dereth_assets::{AnimHook, HookData, MotionTable};
use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::math;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AttackHeight, AttackType, CombatBodyPart, DamageType, MotionCommand, MotionFlags, MotionStance,
    PowerAccuracy, PropertyFloat, Skill, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::base_damage::BaseDamage;
use crate::entity::base_damage_mod::BaseDamageMod;
use crate::entity::body_part::{get_coverage_mask, BodyPart};
use crate::entity::damage_event::DamageEvent;
use crate::entity::timers;
use crate::network::motion::movement_data::Motion;
use crate::physics::{motion_table, phys_ext};
use crate::world_objects::creature_combat::{self, CombatType};
use crate::world_objects::managers::enchantment_manager_with_caching as em;
use crate::world_objects::monster_combat;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::{
    enqueue_broadcast_motion, send_update_position,
};
use crate::world_objects::{
    creature_equipment, creature_melee, monster_awareness, monster_navigation, monster_tick,
    skill_formula, world_object_weapon,
};
use crate::World;

/// Non-property fields declared in `Monster_Melee.cs`.
#[derive(Debug, Default)]
pub struct MonsterMeleeFields {
    // ACE: Creature.moveBit
    pub move_bit: bool,
}

/// `Creature`'s `Monster_Melee.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterMeleeFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_melee
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterMeleeFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_melee
}

/// Returns TRUE if creature can perform a melee attack.
// ACE: Creature.MeleeReady
#[must_use]
pub fn melee_ready(w: &World, this: ObjectGuid) -> bool {
    monster_navigation::is_melee_range(w, this)
        && timers::running_time(w) >= monster_combat::fields(w, this).next_attack_time
}

/// One attack frame: its time (0..1 of the animation) and its attack hook (`null` for the
/// defaults).
pub type AttackFrame = (f32, Option<AnimHook>);

/// Performs a melee attack for the monster; answers the length in seconds of the attack
/// animation. Each strike runs at its attack frame: the damage (`DamageEvent.CalculateDamage`),
/// or the target's evade.
// ACE: Creature.MeleeAttack
pub fn melee_attack(w: &mut World, this: ObjectGuid) -> f32 {
    let target = monster_combat::attack_target_creature(w, this);
    let attack_target = monster_combat::attack_target(w, this);
    let target_player =
        attack_target.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player));
    let target_pet =
        attack_target.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_combat_pet));
    let combat_pet = w.objects.get(this).expect("ACE: this").is_combat_pet();

    let Some(target) =
        target.filter(|&t| !monster_combat::is_dead(w.objects.get(t).expect("resolved")))
    else {
        dispatch::find_next_target::find_next_target(w, this);
        return 0.0;
    };
    let attack_target = attack_target.expect("the creature target");

    if monster_tick::current_stance(w, this) == MotionStance::NonCombat {
        monster_combat::do_attack_stance(w, this);
    }

    let weapon = creature_equipment::get_equipped_weapon(w, this, false);

    // select combat maneuver
    let Some(motion_command) = get_combat_maneuver(w, this) else {
        return 0.0;
    };

    let (anim_length, attack_frames) = do_swing_motion(w, this, attack_target, motion_command);

    let h = monster_navigation::physics_obj(w, this);
    if !w.objects.get(this).expect("ACE: this").ai_immobile() {
        let target_id =
            phys_ext::id(w, monster_navigation::physics_obj(w, attack_target)).unwrap_or(0);
        phys_ext::stick_to_object(w, h, target_id);
    }

    let num_strikes = attack_frames.len();

    let mut action_chain = ActionChain::new();

    // handle self-procs
    try_proc_equipped_items(w, this, this, true, weapon);

    let mut prev_time = 0.0f32;
    let target_proc = Arc::new(AtomicBool::new(false));

    let hooks = strike_hooks(&attack_frames);
    for (frame, &hook) in attack_frames.iter().take(num_strikes).zip(&hooks) {
        action_chain.add_delay_seconds(w, f64::from(frame.0 * anim_length - prev_time));
        prev_time = frame.0 * anim_length;

        let target_proc = Arc::clone(&target_proc);
        action_chain.add_action(Actor::Object(this), move |w| {
            let Some(me) = w.objects.get(this) else {
                return;
            };
            let target_dead = w.objects.get(target).is_none_or(monster_combat::is_dead);
            if monster_combat::attack_target(w, this).is_none()
                || monster_combat::is_dead(me)
                || target_dead
            {
                return;
            }

            if me.biota.weenie_type == WeenieType::GamePiece {
                let t = w.objects.get(target).expect("alive above");
                #[allow(clippy::cast_precision_loss)] // `uint` to `float`, as C# converts it
                let amount = t.health().current(t) as f32;
                dispatch::take_damage::take_damage(
                    w,
                    target,
                    this,
                    DamageType::Slash,
                    amount,
                    false,
                );
                game_piece_on_dealt_damage(w, this);
                return;
            }

            let damage_event =
                DamageEvent::calculate_damage(w, this, target, weapon, Some(motion_command), hook);

            //var damage = CalculateDamage(ref damageType, maneuver, bodyPart, ref critical, ref shieldMod);

            if damage_event.has_damage() {
                if let Some(target_player) = target_player {
                    // this is a player taking damage
                    player_take_damage(w, target_player, this, &damage_event);

                    if damage_event.shield_mod != 1.0 {
                        proficiency_on_success_use_shield(w, target_player);
                    }

                    // handle Dirty Fighting
                    if dirty_fighting_trained(w, this) {
                        creature_combat::fight_dirty(w, this, target_player, damage_event.weapon);
                    }
                } else if combat_pet
                    || target_pet.is_some()
                    || w.objects
                        .get(this)
                        .and_then(WorldObject::faction1_bits)
                        .is_some()
                    || w.objects
                        .get(target)
                        .and_then(WorldObject::faction1_bits)
                        .is_some()
                    || creature_combat::potential_foe(w, this, target)
                {
                    // combat pet inflicting or receiving damage
                    //Console.WriteLine($"{target.Name} taking {Math.Round(damage)} {damageType} damage from {Name}");
                    dispatch::take_damage::take_damage(
                        w,
                        target,
                        this,
                        damage_event.damage_type,
                        damage_event.damage,
                        false,
                    );

                    monster_combat::emit_splatter(w, this, target, damage_event.damage);

                    // handle Dirty Fighting
                    if dirty_fighting_trained(w, this) {
                        creature_combat::fight_dirty(w, this, target, damage_event.weapon);
                    }
                }

                // handle target procs
                if !target_proc.load(Ordering::Relaxed) {
                    try_proc_equipped_items(w, this, target, false, weapon);
                    target_proc.store(true, Ordering::Relaxed);
                }
            } else {
                dispatch::on_evade::on_evade(w, target, this, CombatType::Melee);
            }

            if combat_pet {
                combat_pet_pet_on_attack_monster(w, this, target);
            } else if target_player.is_none() {
                creature_combat::monster_on_attack_monster(w, this, target);
            }
        });
    }
    action_chain.enqueue_chain(w);

    let now = timers::running_time(w);
    monster_combat::fields_mut(w, this).prev_attack_time = now;
    monster_navigation::fields_mut(w, this).next_move_time =
        now + f64::from(anim_length) + f64::from(0.5f32);

    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    let powerup_time = w
        .objects
        .get(this)
        .expect("ACE: this")
        .powerup_time()
        .unwrap_or(f64::from(1.0f32)) as f32;
    let melee_delay = ThreadSafeRandom::next_float(0.0, powerup_time);

    monster_combat::fields_mut(w, this).next_attack_time =
        now + f64::from(anim_length) + melee_delay;

    anim_length
}

/// Selects a random combat maneuver for a monster's next attack: an attack height at random (from
/// Medium when the stance has three heights, else from Low), the attack type for the weapon (or a
/// punch or a kick), one of its maneuvers at random, reduced to one the motion table has.
// ACE: Creature.GetCombatManeuver
pub fn get_combat_maneuver(w: &mut World, this: ObjectGuid) -> Option<MotionCommand> {
    // similar to Player.GetSwingAnimation(), more logging

    let (name, wcid, motion_table_id, combat_table_did) = {
        let o = w.objects.get(this).expect("ACE: this");
        (
            monster_awareness::name(w, this),
            o.biota.weenie_class_id,
            o.motion_table_id(),
            o.combat_table_did().unwrap_or(0),
        )
    };

    let Some(combat_table) = creature_combat::fields(w.objects.get(this).expect("ACE: this"))
        .combat_table
        .clone()
    else {
        log::error!("{name} ({this}).GetCombatManeuver() - WCID: {wcid} | CombatTable is null");
        return None;
    };

    //ShowCombatTable();

    let Some(motion_table) = w
        .dats
        .portal_dat()
        .read_from_dat::<MotionTable>(motion_table_id)
    else {
        log::error!("{name} ({this}).GetCombatManeuver() - WCID: {wcid} | motionTable is null");
        return None;
    };

    let stance = monster_tick::current_stance(w, this);
    let stance_maneuvers: Vec<_> = combat_table
        .maneuvers
        .iter()
        .filter(|m| m.style == stance.0)
        .collect();
    if stance_maneuvers.is_empty() {
        log::error!(
            "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find stance {} in CMT {combat_table_did:08X}",
            stance.to_dotnet_string()
        );
        return None;
    }

    let stance_key = stance.0 << 16 | (MotionCommand::Ready.0 & 0xFF_FFFF);
    let Some(motions) = motion_table.links.get(&stance_key) else {
        log::error!(
            "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find stance {} in MotionTable {motion_table_id:08X}",
            stance.to_dotnet_string()
        );
        return None;
    };
    let motions_contains = |m: MotionCommand| motions.iter().any(|d| d.key == m.0);

    // choose a random attack height?
    // apparently this might have been based on monster Z vs. player Z?

    // 28659 - Uber Penguin (CMT 30000040) doesn't have High attack height
    // do a more thorough investigation for this...
    let mut heights: Vec<u32> = Vec::new();
    for m in &stance_maneuvers {
        if !heights.contains(&m.attack_height) {
            heights.push(m.attack_height);
        }
    }
    let start_height = if heights.len() == 3 { 1 } else { 2 };

    let attack_height = AttackHeight(ThreadSafeRandom::next(start_height, 3));
    monster_combat::fields_mut(w, this).attack_height = Some(attack_height);

    let attack_types: Vec<_> = stance_maneuvers
        .iter()
        .filter(|m| m.attack_height == attack_height.0.cast_unsigned())
        .collect();
    if attack_types.is_empty() {
        log::error!(
            "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find attack height {} for stance {} in CMT {combat_table_did:08X}",
            attack_height.to_dotnet_string(),
            stance.to_dotnet_string()
        );
        return None;
    }
    let maneuvers_of = |t: AttackType| -> Vec<MotionCommand> {
        attack_types
            .iter()
            .filter(|m| m.attack_type == t.0.cast_unsigned())
            .map(|m| MotionCommand(m.motion))
            .collect()
    };

    if creature_melee::is_dual_wield_attack(w, this) {
        let f = &mut w
            .objects
            .get_mut(this)
            .and_then(|o| o.creature.as_mut())
            .expect("ACE: this")
            .creature_melee;
        f.dual_wield_alternate = !f.dual_wield_alternate;
    }

    let offhand = creature_melee::is_dual_wield_attack(w, this)
        && !creature_melee::dual_wield_alternate(w, this);

    let weapon = creature_equipment::get_equipped_melee_weapon(w, this, false);

    // monsters supposedly always used 0.5 PowerLevel according to anon docs,
    // which translates into a 1.0 PowerMod

    let attack_type = if let Some(weapon) = weapon {
        world_object_weapon::get_attack_type(w, weapon, stance, 0.5, offhand)
    } else if attack_height != AttackHeight::Low {
        AttackType::Punch
    } else {
        AttackType::Kick
    };
    set_attack_type(w, this, attack_type);

    let mut maneuvers = maneuvers_of(attack_type);
    if maneuvers.is_empty() {
        if attack_type == AttackType::Punch && attack_height == AttackHeight::Low
            || attack_type == AttackType::Kick
        {
            // 27864 - Mosswart Muckstalker w/ a katar, low punch not found in CMT, but contains kick
            // might need additional research
            let swapped = if attack_type == AttackType::Punch {
                AttackType::Kick
            } else {
                AttackType::Punch
            };
            set_attack_type(w, this, swapped);

            maneuvers = maneuvers_of(swapped);
            if maneuvers.is_empty() {
                log::error!(
                    "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find attack type Kick or Punch for attack height {} and stance {} in CMT {combat_table_did:08X}",
                    attack_height.to_dotnet_string(),
                    stance.to_dotnet_string()
                );
                return None;
            }
        } else if attack_type_is_multi_strike(attack_type) {
            let reduced = attack_type_reduce_multi_strike(attack_type);

            maneuvers = maneuvers_of(reduced);
            if maneuvers.is_empty() {
                log::error!(
                    "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find attack type {} for attack height {} and stance {} in CMT {combat_table_did:08X}",
                    reduced.to_dotnet_string(),
                    attack_height.to_dotnet_string(),
                    stance.to_dotnet_string()
                );
                return None;
            }
            //else
            //log.Info(...);
        } else {
            log::error!(
                "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find attack type {} for attack height {} and stance {} in CMT {combat_table_did:08X}",
                attack_type.to_dotnet_string(),
                attack_height.to_dotnet_string(),
                stance.to_dotnet_string()
            );
            return None;
        }
    }

    let mut motion_command = maneuvers[0];

    if maneuvers.len() > 1 {
        // only used for special attacks?

        // note that with rolling for AttackHeight first,
        // for a CMT with high, med, med-special, and low
        // the chance of rolling the special attack is reduced from 1/4 to 1/6 -- investigate

        let count = i32::try_from(maneuvers.len()).expect("a few maneuvers");
        let rng = ThreadSafeRandom::next(0, count - 1);
        motion_command = maneuvers[usize::try_from(rng).expect("ACE: ArgumentOutOfRangeException")];
    }

    // ensure this motionCommand exists in monster's motion table
    if !motions_contains(motion_command) {
        // for some reason, the combat maneuvers table can return stance motions that don't exist in the motion table
        // ie. skeletons (combat maneuvers table 0x30000000, motion table 0x09000025)
        // for sword combat, they have double and triple strikes (dagger / two-handed only?)
        if motion_command_is_multi_strike(motion_command) {
            let single_strike = motion_command_reduce_multi_strike(motion_command);

            if motions_contains(single_strike) {
                //log.Info(...);
                return Some(single_strike);
            }
        } else if motion_command_is_subsequent(motion_command) {
            let first_command = motion_command_reduce_subsequent(motion_command);

            if motions_contains(first_command) {
                //log.Info(...);
                return Some(first_command);
            }
        }
        log::error!(
            "{name} ({this}).GetCombatManeuver() - WCID: {wcid} | couldn't find {} in MotionTable {motion_table_id:08X}",
            motion_command.to_dotnet_string()
        );
        return None;
    }

    //Console.WriteLine(motionCommand);

    Some(motion_command)
}

/// Shows debug info for this monster's combat maneuvers table.
// ACE: Creature.ShowCombatTable
pub fn show_combat_table(w: &World, this: ObjectGuid) {
    let Some(table) = creature_combat::fields(w.objects.get(this).expect("ACE: this"))
        .combat_table
        .clone()
    else {
        panic!("ACE: CombatTable is null (NullReferenceException)");
    };
    log::info!("CombatManeuverTable ID: {:08X}", table.id.0);
    // `CombatTable.ShowCombatTable()` (DatLoader): one line per maneuver
    for m in &table.maneuvers {
        log::info!(
            "{:08X} - {:?} - {:08X} - {:?}",
            m.style,
            m.attack_height,
            m.motion,
            m.attack_type
        );
    }
}

thread_local! {
    /// `missingAttackFrames`: the (motion table, stance, motion) combos already warned about.
    static MISSING_ATTACK_FRAMES: std::cell::RefCell<HashSet<(u32, u32, u32)>> = std::cell::RefCell::new(HashSet::new());
}

/// `defaultAttackFrames`: one strike a third of the way in, with no hook.
// ACE: Creature.defaultAttackFrames
#[must_use]
pub fn default_attack_frames() -> Vec<AttackFrame> {
    vec![(1.0f32 / 3.0f32, None)]
}

/// Perform the melee attack swing animation; answers the animation length and the attack frames.
// ACE: Creature.DoSwingMotion
pub fn do_swing_motion(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    motion_command: MotionCommand,
) -> (f32, Vec<AttackFrame>) {
    if !fields(w, this).move_bit {
        send_update_position(w, this, true);
        fields_mut(w, this).move_bit = true;
    }

    //Console.WriteLine($"{maneuver.Style} - {maneuver.Motion} - {maneuver.AttackHeight}");

    let base_speed = creature_combat::get_anim_speed(w, this);
    let anim_speed_mod = if creature_melee::is_dual_wield_attack(w, this) {
        1.2f32
    } else {
        1.0f32
    }; // dual wield swing animation 20% faster
    let anim_speed = base_speed * anim_speed_mod;

    let motion_table_id = w.objects.get(this).expect("ACE: this").motion_table_id();
    let stance = monster_tick::current_stance(w, this);
    let anim_length =
        motion_table::get_animation_length(w, motion_table_id, stance, motion_command, anim_speed);

    let mut attack_frames: Vec<AttackFrame> =
        motion_table::get_attack_frames(w, motion_table_id, stance, motion_command)
            .into_iter()
            .map(|(t, h)| (t, Some(h)))
            .collect();

    if attack_frames.is_empty() {
        let key = (motion_table_id, stance.0, motion_command.0);
        let first = MISSING_ATTACK_FRAMES.with(|m| m.borrow_mut().insert(key));
        if first {
            // only show warning message once for each combo
            log::warn!(
                "{} ({this}) - no attack frames for MotionTable {motion_table_id:08X}, {}, {}, using defaults",
                monster_awareness::name(w, this),
                stance.to_dotnet_string(),
                motion_command.to_dotnet_string()
            );
        }
        attack_frames = default_attack_frames();
    }

    let mut motion = Motion::from_world_object(w, this, motion_command, anim_speed);
    motion.motion_state.turn_speed = 2.25;

    if !w.objects.get(this).expect("ACE: this").ai_immobile() {
        motion.motion_flags |= MotionFlags::StickToObject;
    }

    motion.target_guid = target;
    w.objects
        .get_mut(this)
        .expect("ACE: this")
        .wo
        .world_object_properties
        .current_motion_state = Some(motion.clone());

    enqueue_broadcast_motion(w, this, &motion, None, None);

    (anim_length, attack_frames)
}

/// Returns base damage range for next monster attack: the missile's for a missile attack with
/// ammo, the melee weapon's, else the attacking body part's.
// ACE: Creature.GetBaseDamage
pub fn get_base_damage(
    w: &mut World,
    this: ObjectGuid,
    attack_part: &PropertiesBodyPart,
) -> BaseDamageMod {
    if monster_combat::fields(w, this).current_attack == Some(CombatType::Missile)
        && creature_equipment::get_missile_ammo(w, this).is_some()
    {
        return crate::world_objects::monster_missile::get_missile_damage(w, this);
    }

    // use weapon damage for every attack?
    let weapon = creature_equipment::get_equipped_melee_weapon(w, this, false);
    if let Some(weapon) = weapon {
        //Console.WriteLine($"{Name} using weapon damage: {weaponDamage}");
        return crate::world_objects::world_object::get_damage_mod(w, weapon, this, None);
    }

    let max_damage = attack_part.d_val;
    let variance = attack_part.d_var;

    let base_damage = BaseDamage::new(max_damage, variance);
    BaseDamageMod::new(base_damage)
}

/// Returns the creature armor for a body part: the target's equipped clothing covering it, in
/// equipped order.
// ACE: Creature.GetArmorLayers
#[must_use]
pub fn get_armor_layers(w: &World, target: ObjectGuid, body_part: BodyPart) -> Vec<ObjectGuid> {
    //Console.WriteLine("BodyPart: " + bodyPart);
    //Console.WriteLine("===");

    let coverage_mask = get_coverage_mask(body_part);

    creature_equipment::equipped_objects_values(w, target)
        .into_iter()
        .filter(|&e| {
            let o = w.objects.get(e).expect("an equipped object");
            o.is_clothing() && (o.clothing_priority().unwrap_or_default().0 & coverage_mask.0) != 0
        })
        .collect()
}

/// Returns the percent of damage absorbed by layered armor + clothing (`GetArmorMod(Creature
/// defender, DamageType, List<WorldObject> armors, WorldObject weapon, float armorRendingMod =
/// 1.0f)`).
// ACE: Creature.GetArmorMod
pub fn get_armor_mod_layers(
    w: &mut World,
    this: ObjectGuid,
    defender: ObjectGuid,
    damage_type: DamageType,
    armors: &[ObjectGuid],
    weapon: Option<ObjectGuid>,
    armor_rending_mod: f32,
) -> f32 {
    let (me_armor, me_resist) = {
        let o = w.objects.get(this).expect("ACE: this");
        (o.ignore_magic_armor(), o.ignore_magic_resist())
    };
    let weapon_obj = weapon.and_then(|g| w.objects.get(g));
    let ignore_magic_armor = weapon_obj.is_some_and(WorldObject::ignore_magic_armor) || me_armor;
    let ignore_magic_resist = weapon_obj.is_some_and(WorldObject::ignore_magic_resist) || me_resist;

    let mut effective_al = 0.0f32;

    for &armor in armors {
        effective_al += get_armor_mod(w, this, armor, damage_type, ignore_magic_armor);
    }

    // life spells
    // additive: armor/imperil
    let mut body_armor_mod = em::get_body_armor_mod(w, defender);
    if ignore_magic_resist {
        body_armor_mod = ignore_magic_resist_scaled(w, this, body_armor_mod);
    }

    // handle armor rending mod here?
    //if (bodyArmorMod > 0)
    //bodyArmorMod *= armorRendingMod;

    //Console.WriteLine("==");
    //Console.WriteLine("Armor Self: " + bodyArmorMod);
    #[allow(clippy::cast_precision_loss)] // `float += int` in C#
    {
        effective_al += body_armor_mod as f32;
    }

    // Armor Rending reduces physical armor too?
    if effective_al > 0.0 {
        effective_al *= armor_rending_mod;
    }

    //Console.WriteLine("Total AL: " + effectiveAL);
    //Console.WriteLine("Armor mod: " + armorMod);

    skill_formula::calc_armor_mod(effective_al)
}

/// The part of `enchantments` a PvP attacker's IgnoreMagicArmor removes (0 for a non-player).
// ACE: Creature.IgnoreMagicArmorScaled
#[must_use]
pub fn ignore_magic_armor_scaled(w: &World, this: ObjectGuid, enchantments: f32) -> f32 {
    if !w.objects.get(this).expect("ACE: this").is_player() {
        return 0.0;
    }

    let scalar = crate::managers::property_manager::get_double(
        w,
        "ignore_magic_armor_pvp_scalar",
        0.0,
        true,
    )
    .item;

    if scalar == 1.0 {
        0.0
    } else {
        (f64::from(enchantments) * (1.0 - scalar)).cs_cast()
    }
}

/// The part of `enchantments` a PvP attacker's IgnoreMagicResist removes (0 for a non-player).
// ACE: Creature.IgnoreMagicResistScaled
#[must_use]
pub fn ignore_magic_resist_scaled(w: &World, this: ObjectGuid, enchantments: i32) -> i32 {
    if !w.objects.get(this).expect("ACE: this").is_player() {
        return 0;
    }

    let scalar = crate::managers::property_manager::get_double(
        w,
        "ignore_magic_resist_pvp_scalar",
        0.0,
        true,
    )
    .item;

    if scalar == 1.0 {
        0
    } else {
        math::round(f64::from(enchantments) * (1.0 - scalar)).cs_cast()
    }
}

/// Returns the effective AL for 1 piece of armor/clothing.
// ACE: Creature.GetArmorMod
pub fn get_armor_mod(
    w: &mut World,
    this: ObjectGuid,
    armor: ObjectGuid,
    damage_type: DamageType,
    ignore_magic_armor: bool,
) -> f32 {
    // get base armor/resistance level
    let base_armor = w
        .objects
        .get(armor)
        .expect("ACE: armor")
        .armor_level()
        .unwrap_or(0);
    //var armorType = armor.GetProperty(PropertyInt.ArmorType) ?? 0;
    let resistance = get_resistance(w.objects.get(armor).expect("ACE: armor"), damage_type);

    /*Console.WriteLine(armor.Name); ...*/

    // armor level additives
    let mut armor_mod = em::get_armor_mod(w, armor);

    if ignore_magic_armor {
        #[allow(clippy::cast_precision_loss)] // `int` to `float`, as C# converts it
        let scaled = ignore_magic_armor_scaled(w, this, armor_mod as f32);
        armor_mod = math::round(f64::from(scaled)).cs_cast();
    }

    // Console.WriteLine("Impen: " + armorMod);
    let effective_al = base_armor.wrapping_add(armor_mod);

    // resistance additives
    let mut armor_bane = em::get_armor_mod_vs_type(w, armor, damage_type);

    if ignore_magic_armor {
        armor_bane = ignore_magic_armor_scaled(w, this, armor_bane);
    }

    // Console.WriteLine("Bane: " + armorBane);
    let mut effective_rl: f32 = (resistance + f64::from(armor_bane)).cs_cast();

    // resistance clamp
    effective_rl = effective_rl.clamp(-2.0, 2.0);

    // TODO: could brittlemail / lures send a piece of armor or clothing's AL into the negatives?
    //if (effectiveAL < 0 && effectiveRL != 0)
    //effectiveRL = 1.0f / effectiveRL;

    #[allow(clippy::cast_precision_loss)] // `int * float` in C#
    let al = effective_al as f32;
    al * effective_rl
}

/// Returns the natural resistance to DamageType for a piece of armor.
// ACE: Creature.GetResistance
#[must_use]
pub fn get_resistance(armor: &WorldObject, damage_type: DamageType) -> f64 {
    let p = match damage_type {
        DamageType::Slash => PropertyFloat::ArmorModVsSlash,
        DamageType::Pierce => PropertyFloat::ArmorModVsPierce,
        DamageType::Bludgeon => PropertyFloat::ArmorModVsBludgeon,
        DamageType::Fire => PropertyFloat::ArmorModVsFire,
        DamageType::Cold => PropertyFloat::ArmorModVsCold,
        DamageType::Acid => PropertyFloat::ArmorModVsAcid,
        DamageType::Electric => PropertyFloat::ArmorModVsElectric,
        // Not ACE's (retail, V273): armour with no nether multiplier counts as 1.0,
        // the value the retail captures showed in every appraisal of such armour (ACE used 0 here
        // and 1.0 in the appraisal). Whether retail stored 1.0 or defaulted to it is not provable
        // from the wire. The other types are present on armour in practice and keep ACE's 0.
        DamageType::Nether => {
            return armor
                .get_property(PropertyFloat::ArmorModVsNether)
                .unwrap_or(1.0)
        }
        _ => return 0.0,
    };
    armor.get_property(p).unwrap_or(0.0)
}

/// Displays all of the natural resistances for a piece of armor.
// ACE: Creature.ShowResistance
pub fn show_resistance(armor: &WorldObject) {
    let show = |label: &str, p: PropertyFloat| {
        log::info!(
            "{label}{}",
            armor
                .get_property(p)
                .map(|v| v.to_string())
                .unwrap_or_default()
        );
    };
    log::info!("Resistance:");
    show("Slashing: ", PropertyFloat::ArmorModVsSlash);
    show("Piercing: ", PropertyFloat::ArmorModVsPierce);
    show("Bludgeoning: ", PropertyFloat::ArmorModVsBludgeon);
    show("Fire: ", PropertyFloat::ArmorModVsFire);
    show("Ice: ", PropertyFloat::ArmorModVsCold);
    show("Acid: ", PropertyFloat::ArmorModVsAcid);
    show("Lightning: ", PropertyFloat::ArmorModVsElectric);
    show("Nether: ", PropertyFloat::ArmorModVsNether);
}

/// Returns the power range for the current melee attack.
// ACE: Creature.GetPowerRange
#[must_use]
pub fn creature_get_power_range(_w: &World, _this: ObjectGuid) -> PowerAccuracy {
    PowerAccuracy::Low // always low for monsters?
}

/// Returns the monster body part performing the next attack: the attack hook's part, the Breath
/// for a special attack, else any damaging part but the Breath; one of them at random.
// ACE: Creature.GetAttackPart
pub fn get_attack_part(
    w: &World,
    this: ObjectGuid,
    motion_command: MotionCommand,
    attack_hook: Option<AttackCone>,
) -> Option<(CombatBodyPart, PropertiesBodyPart)> {
    let o = w.objects.get(this).expect("ACE: this");
    let body_parts = o
        .biota
        .properties_body_part
        .as_ref()
        .expect("ACE: Biota.PropertiesBodyPart is null (NullReferenceException)");
    let mut parts: Option<Vec<(CombatBodyPart, PropertiesBodyPart)>> = None;

    // todo: speed up key lookup?
    if let Some(attack_hook) = attack_hook {
        let part_index = attack_hook.part_index;
        parts = Some(
            body_parts
                .iter()
                .filter(|(k, _)| k.0.cast_unsigned() == part_index)
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
        );
    } else if motion_command.0 >= MotionCommand::SpecialAttack1.0
        && motion_command.0 <= MotionCommand::SpecialAttack3.0
    {
        //parts = Biota.BiotaPropertiesBodyPart.Where(b => b.DVal != 0 && b.BH == 0).ToList();
        parts = Some(
            body_parts
                .iter()
                .filter(|(k, _)| **k == CombatBodyPart::Breath)
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
        );
        // always use Breath?
    }

    // added parts.Count check for monsters wielding weapons -- should we be getting a body part here?
    let parts = match parts {
        Some(p) if !p.is_empty() => p,
        //parts = Biota.BiotaPropertiesBodyPart.Where(b => b.DVal != 0 && b.BH != 0).ToList();
        _ => body_parts
            .iter()
            .filter(|(k, v)| v.d_val != 0 && **k != CombatBodyPart::Breath)
            .map(|(k, v)| (*k, v.clone()))
            .collect(),
    };

    if parts.is_empty() {
        log::warn!(
            "{} ({this}.GetAttackPart({}) failed",
            monster_awareness::name(w, this),
            motion_command.to_dotnet_string()
        );
        log::warn!(
            "CombatTable: {:08X}, MotionTable: {:08X}, CurrentStance: {}, AttackHeight: {}, AttackType: {}",
            o.combat_table_did().unwrap_or(0),
            o.motion_table_id(),
            monster_tick::current_stance(w, this).to_dotnet_string(),
            monster_combat::fields(w, this).attack_height.map(|h| h.to_dotnet_string()).unwrap_or_default(),
            creature_combat::attack_type(w, this).to_dotnet_string()
        );
        return None;
    }

    let count = i32::try_from(parts.len()).expect("a few parts");
    let part = parts[usize::try_from(ThreadSafeRandom::next(0, count - 1))
        .expect("ACE: ArgumentOutOfRangeException")]
    .clone();

    Some(part)
}

/// `AttackHook` of an attack frame (ACE casts the frame's hook to `AttackHook`): its attack cone.
/// Not ACE's (a fix, V253): the attack cone each strike of a swing hits with,
/// its own frame's, so each strike's own body part picks its damage. ACE passed
/// `attackFrames[0].attackHook` to every strike.
#[must_use]
pub fn strike_hooks(attack_frames: &[AttackFrame]) -> Vec<Option<AttackCone>> {
    attack_frames
        .iter()
        .map(|frame| frame.1.as_ref().map(attack_cone))
        .collect()
}

fn attack_cone(hook: &AnimHook) -> AttackCone {
    let HookData::Attack {
        part_index,
        left,
        right,
        radius,
        height,
    } = hook.data
    else {
        panic!("ACE: an attack frame's hook is an AttackHook (InvalidCastException)");
    };
    AttackCone {
        part_index,
        left,
        right,
        radius,
        height,
    }
}

/// `AttackType = value` (`Creature_Combat.cs`).
fn set_attack_type(w: &mut World, this: ObjectGuid, attack_type: AttackType) {
    creature_combat::fields_mut(w.objects.get_mut(this).expect("ACE: this")).attack_type =
        attack_type;
}

// ---------------------------------------------------------------------------------------------
// ACE.Entity enum extensions the maneuver choice uses (`ACE.Entity/Enum/AttackType.cs`,
// `MotionCommand.cs`): not ported in empyrean-entity yet, so kept here.
// ---------------------------------------------------------------------------------------------

// ACE: AttackTypeExtensions.IsMultiStrike
#[must_use]
pub fn attack_type_is_multi_strike(attack_type: AttackType) -> bool {
    (attack_type.0 & AttackType::MultiStrike.0) != 0
}

// ACE: AttackTypeExtensions.ReduceMultiStrike
#[must_use]
pub fn attack_type_reduce_multi_strike(attack_type: AttackType) -> AttackType {
    if !attack_type_is_multi_strike(attack_type) {
        return AttackType::Undef;
    }

    match attack_type {
        AttackType::DoubleThrust | AttackType::TripleThrust => AttackType::Thrust,
        AttackType::DoubleSlash | AttackType::TripleSlash => AttackType::Slash,
        AttackType::OffhandDoubleThrust | AttackType::OffhandTripleThrust => {
            AttackType::OffhandThrust
        }
        AttackType::OffhandDoubleSlash | AttackType::OffhandTripleSlash => AttackType::OffhandSlash,
        _ => AttackType::Undef,
    }
}

// ACE: MotionCommandHelper.IsMultiStrike
#[must_use]
pub fn motion_command_is_multi_strike(motion_command: MotionCommand) -> bool {
    let m = motion_command.0;
    let range = |a: MotionCommand, b: MotionCommand| m >= a.0 && m <= b.0;
    range(
        MotionCommand::DoubleSlashLow,
        MotionCommand::TripleThrustHigh,
    ) || range(
        MotionCommand::OffhandDoubleSlashLow,
        MotionCommand::OffhandTripleThrustHigh,
    )
}

// ACE: MotionCommandHelper.ReduceMultiStrike
#[must_use]
pub fn motion_command_reduce_multi_strike(motion_command: MotionCommand) -> MotionCommand {
    if !motion_command_is_multi_strike(motion_command) {
        return MotionCommand::Invalid;
    }

    match motion_command {
        MotionCommand::DoubleSlashLow | MotionCommand::TripleSlashLow => MotionCommand::SlashLow,
        MotionCommand::DoubleSlashMed | MotionCommand::TripleSlashMed => MotionCommand::SlashMed,
        MotionCommand::DoubleSlashHigh | MotionCommand::TripleSlashHigh => MotionCommand::SlashHigh,
        MotionCommand::DoubleThrustLow | MotionCommand::TripleThrustLow => MotionCommand::ThrustLow,
        MotionCommand::DoubleThrustMed | MotionCommand::TripleThrustMed => MotionCommand::ThrustMed,
        MotionCommand::DoubleThrustHigh | MotionCommand::TripleThrustHigh => {
            MotionCommand::ThrustHigh
        }
        MotionCommand::OffhandDoubleSlashLow | MotionCommand::OffhandTripleSlashLow => {
            MotionCommand::SlashLow
        }
        MotionCommand::OffhandDoubleSlashMed | MotionCommand::OffhandTripleSlashMed => {
            MotionCommand::SlashMed
        }
        MotionCommand::OffhandDoubleSlashHigh | MotionCommand::OffhandTripleSlashHigh => {
            MotionCommand::SlashHigh
        }
        MotionCommand::OffhandDoubleThrustLow | MotionCommand::OffhandTripleThrustLow => {
            MotionCommand::ThrustLow
        }
        MotionCommand::OffhandDoubleThrustMed | MotionCommand::OffhandTripleThrustMed => {
            MotionCommand::ThrustMed
        }
        MotionCommand::OffhandDoubleThrustHigh | MotionCommand::OffhandTripleThrustHigh => {
            MotionCommand::ThrustHigh
        }
        _ => MotionCommand::Invalid,
    }
}

// ACE: MotionCommandHelper.IsSubsequent
#[must_use]
pub fn motion_command_is_subsequent(motion_command: MotionCommand) -> bool {
    let m = motion_command.0;
    let range = |a: MotionCommand, b: MotionCommand| m >= a.0 && m <= b.0;
    range(MotionCommand::AttackHigh2, MotionCommand::AttackLow3)
        || range(MotionCommand::AttackHigh4, MotionCommand::AttackLow6)
}

// ACE: MotionCommandHelper.ReduceSubsequent
#[must_use]
pub fn motion_command_reduce_subsequent(motion_command: MotionCommand) -> MotionCommand {
    if !motion_command_is_subsequent(motion_command) {
        return MotionCommand::Invalid;
    }

    match motion_command {
        MotionCommand::AttackLow2
        | MotionCommand::AttackLow3
        | MotionCommand::AttackLow4
        | MotionCommand::AttackLow5
        | MotionCommand::AttackLow6 => MotionCommand::AttackLow1,
        MotionCommand::AttackMed2
        | MotionCommand::AttackMed3
        | MotionCommand::AttackMed4
        | MotionCommand::AttackMed5
        | MotionCommand::AttackMed6 => MotionCommand::AttackMed1,
        MotionCommand::AttackHigh2
        | MotionCommand::AttackHigh3
        | MotionCommand::AttackHigh4
        | MotionCommand::AttackHigh5
        | MotionCommand::AttackHigh6 => MotionCommand::AttackHigh1,
        _ => MotionCommand::Invalid,
    }
}

// ---------------------------------------------------------------------------------------------
// Pointers to members of other ACE files (each `not_ported!` with ACE's no-data answer)
// ---------------------------------------------------------------------------------------------

/// `Player.TakeDamage(WorldObject source, DamageEvent damageEvent)` (`Player_Combat.cs`).
fn player_take_damage(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    damage_event: &DamageEvent,
) {
    crate::world_objects::player_combat::take_damage_event(w, player, Some(source), damage_event);
}

/// `var shieldSkill = targetPlayer.GetCreatureSkill(Skill.Shield);`
/// `Proficiency.OnSuccessUse(targetPlayer, shieldSkill, shieldSkill.Current)`.
fn proficiency_on_success_use_shield(w: &mut World, player: ObjectGuid) {
    let shield_skill = crate::entity::proficiency::get_creature_skill(
        w,
        player,
        empyrean_entity::enums::Skill::Shield,
    );
    let current = shield_skill.current(w, player);
    crate::entity::proficiency::on_success_use(w, player, shield_skill, current);
}

/// `GetCreatureSkill(Skill.DirtyFighting).AdvancementClass >= SkillAdvancementClass.Trained`.
fn dirty_fighting_trained(w: &mut World, this: ObjectGuid) -> bool {
    let o = w.objects.get_mut(this).expect("ACE: this");
    let skill = o
        .get_creature_skill(Skill::DirtyFighting, true)
        .expect("GetCreatureSkill(add: true)");
    skill.advancement_class(o).0 >= SkillAdvancementClass::Trained.0
}

/// `TryProcEquippedItems(attacker, target, selfTarget, weapon)` (`WorldObject_Combat.cs`), called
/// on the attacker itself (every caller's `this` is `attacker`).
pub(crate) fn try_proc_equipped_items(
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

/// `(this as GamePiece).OnDealtDamage()` (`GamePiece.cs`).
fn game_piece_on_dealt_damage(w: &mut World, this: ObjectGuid) {
    crate::world_objects::game_piece::on_dealt_damage(w, this);
}

/// `combatPet.PetOnAttackMonster(target)`.
fn combat_pet_pet_on_attack_monster(w: &mut World, this: ObjectGuid, target: ObjectGuid) {
    crate::world_objects::pet_monster::pet_on_attack_monster(w, this, target);
}
