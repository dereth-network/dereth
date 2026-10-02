// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Combat.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Combat.cs`.
//!
//! Combat with a Player as the attacker (melee and missile): the attack and defence skills, the
//! power/accuracy modifiers and stamina costs, `DamageTarget`, the Player's `TakeDamage`
//! (physical and damage-over-time), evasion, Recklessness, natural resistances, PK status and
//! timers, and the combat mode change handler.
//!
//! `CombatType` is declared in this file in ACE; it lives in `creature_combat.rs` (the damage
//! pipeline is built on it) and is re-exported here.
//!
//! The generated virtual-dispatch targets keep their signatures, including the generator's `()`
//! for `CombatType`/`Spell` arguments (a known limitation of the generator); each calls
//! the typed port.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AttackConditions, AttackType, ChatMessageType, CombatMode, CoverageMask, DamageType, EquipMask,
    HeritageGroup, MotionCommand, MotionStance, MovementType, ParentLocation, Placement,
    PlayScript, PlayerKillerStatus, PowerAccuracy, PropertyAttribute, PropertyInstanceId,
    PropertyInt, ResistanceType, Skill, SkillAdvancementClass, Sound, WeenieError,
    WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::base_damage::BaseDamage;
use crate::entity::base_damage_mod::BaseDamageMod;
use crate::entity::body_part::{self, BodyPart};
use crate::entity::damage_event::DamageEvent;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::entity::spell::Spell;
use crate::entity::stamina_table;
use crate::entity::timers;
use crate::entity::{damage_history, landblock};
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_attacker_notification::game_event_attacker_notification;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_defender_notification::game_event_defender_notification;
use crate::network::game_event::events::game_event_evasion_attacker_notification::game_event_evasion_attacker_notification;
use crate::network::game_event::events::game_event_evasion_defender_notification::game_event_evasion_defender_notification;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_instance_id::game_message_private_update_instance_id;
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::creature_combat::{self, shim};
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_weapon::{self as weapon_mod, SkillOf};
use crate::world_objects::{
    creature_death, creature_equipment, creature_melee, creature_rating, creature_vitals,
    monster_combat, player, player_death, player_inventory, player_melee, player_missile,
    player_networking, world_object_networking,
};
use crate::{dispatch, World};

pub use crate::world_objects::creature_combat::CombatType;

/// Non-property fields declared in `Player_Combat.cs`.
#[derive(Debug, Default)]
pub struct PlayerCombatFields {
    // ACE: Player.AttackSequence
    pub attack_sequence: i32,
    // ACE: Player.Attacking
    pub attacking: bool,
    // ACE: Player.AttackCancelled
    pub attack_cancelled: bool,
    // ACE: Player.NextRefillTime
    pub next_refill_time: DotNetDateTime,
    // ACE: Player.LastCombatMode
    pub last_combat_mode: CombatMode,
    /// SHIM storage for `Player.NextUseTime` (`Player_Use.cs`, an auto-property) as the combat code
    /// sees it; read and written only through [`next_use_time`] / [`set_next_use_time`].
    pub shim_next_use_time: DotNetDateTime,
    /// Not ACE (V417): when the latest combat-mode change waiting for the previous one's
    /// animation runs, in `Timers.PortalYearTicks`, so a later request never runs before it.
    pub deferred_combat_mode_end: f64,
}

// ============================================================================== helpers

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

/// This player's `Player_Combat` fields.
///
/// # Panics
/// When `this` is not a live player (ACE: `InvalidCastException`).
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerCombatFields {
    &object(w, this)
        .player
        .as_ref()
        .expect("InvalidCastException: not a Player")
        .player_combat
}

/// This player's `Player_Combat` fields, mutably.
///
/// # Panics
/// When `this` is not a live player (ACE: `InvalidCastException`).
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerCombatFields {
    &mut object_mut(w, this)
        .player
        .as_mut()
        .expect("InvalidCastException: not a Player")
        .player_combat
}

/// `Player.Session`.
///
/// # Panics
/// Without a session (ACE: `NullReferenceException`).
pub(crate) fn session(w: &World, this: ObjectGuid) -> SessionId {
    world_object_networking::shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `Session.Network.EnqueueSend(msg)`.
pub(crate) fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let s = session(w, this);
    enqueue_send(w, s, msg);
}

/// `EnqueueBroadcast(msgs)` (to self and every player that knows this object).
fn enqueue_broadcast(w: &mut World, this: ObjectGuid, msgs: &[GameMessage]) {
    let _ = world_object_networking::enqueue_broadcast(w, this, true, msgs);
}

/// `x is Creature`.
fn is_creature(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_creature)
}

/// `x is Player`.
fn is_player(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_player)
}

/// `Creature.IsDead` (`Health.Current <= 0`).
fn is_dead(w: &World, this: ObjectGuid) -> bool {
    monster_combat::is_dead(object(w, this))
}

/// `Creature.IsAlive` (`Health.Current > 0`).
fn is_alive(w: &World, this: ObjectGuid) -> bool {
    !is_dead(w, this)
}

/// `Health.MaxValue` (the enchanted, cached value).
fn max_health(w: &mut World, this: ObjectGuid) -> u32 {
    let h = object(w, this).health();
    h.max_value(&mut StatCtx::in_world(w, this))
}

/// `(PlayScript)Enum.Parse(typeof(PlayScript), "Splatter" + attacker.GetSplatterHeight() +
/// attacker.GetSplatterDir(target))`.
fn splatter_script(w: &World, attacker: ObjectGuid, target: ObjectGuid) -> PlayScript {
    let name = format!(
        "Splatter{}{}",
        creature_combat::get_splatter_height(w, attacker),
        creature_combat::get_splatter_dir(w, attacker, target)
    );
    PlayScript::from_name(&name).expect("System.ArgumentException: Enum.Parse(PlayScript)")
}

/// `Time.GetUnixTime()`: the tick's clock.
fn unix_time(w: &World) -> f64 {
    w.now.unix_time
}

/// `<attribute>.Base`.
fn attribute_base(w: &World, this: ObjectGuid, attribute: PropertyAttribute) -> u32 {
    creature_combat::attribute_base(w, this, attribute)
}

/// SHIM: `Player.NextUseTime` (`Player_Use.cs`), stored in [`PlayerCombatFields`].
#[must_use]
pub fn next_use_time(w: &World, this: ObjectGuid) -> DotNetDateTime {
    fields(w, this).shim_next_use_time
}

/// SHIM: `Player.NextUseTime = value`.
pub fn set_next_use_time(w: &mut World, this: ObjectGuid, value: DotNetDateTime) {
    fields_mut(w, this).shim_next_use_time = value;
}

/// `Player.IsLoggingOut` (`Player.cs`).
fn is_logging_out(w: &World, this: ObjectGuid) -> bool {
    object(w, this)
        .player
        .as_ref()
        .is_some_and(|p| p.player.is_logging_out)
}

/// `Player.GetCurrentMagicSkill()` (`Player_Magic.cs`).
fn get_current_magic_skill(w: &World, this: ObjectGuid) -> Skill {
    crate::world_objects::player_magic::get_current_magic_skill(w, this)
}

/// `Proficiency.OnSuccessUse(player, skill, difficulty)` (`Entity/Proficiency.cs`).
fn proficiency_on_success_use(w: &mut World, player: ObjectGuid, skill: SkillOf, difficulty: u32) {
    crate::entity::proficiency::on_success_use(w, player, skill.skill, difficulty);
}

/// `SquelchManager.Squelches.Contains(source, type)`.
fn squelches_contains(
    w: &World,
    this: ObjectGuid,
    source: ObjectGuid,
    type_: ChatMessageType,
) -> bool {
    crate::world_objects::managers::squelch_manager::squelches_contains(
        w,
        this,
        Some(source),
        type_,
    )
}

/// `target.EmoteManager.OnDamage(this)` (`EmoteManager.cs`).
fn emote_manager_on_damage(w: &mut World, target: ObjectGuid, attacker: ObjectGuid) {
    crate::world_objects::managers::emote_manager::on_damage(w, target, Some(attacker));
}

/// `target.EmoteManager.OnReceiveCritical(this)` (`EmoteManager.cs`).
fn emote_manager_on_receive_critical(w: &mut World, target: ObjectGuid, attacker: ObjectGuid) {
    crate::world_objects::managers::emote_manager::on_receive_critical(w, target, Some(attacker));
}

/// `Player.OnAttackMonster(monster)` (`Player_Monster.cs`).
fn on_attack_monster(w: &mut World, this: ObjectGuid, monster: ObjectGuid) {
    crate::world_objects::player_monster::on_attack_monster(w, this, Some(monster));
}

/// `Cloak.HasDamageProc(cloak) && Cloak.RollProc(cloak, percent)` (`Entity/Cloak.cs`).
fn cloak_damage_proc(w: &mut World, cloak: ObjectGuid, percent: f32) -> bool {
    crate::entity::cloak::has_damage_proc(w.objects.get(cloak))
        && crate::entity::cloak::roll_proc(w, cloak, percent)
}

/// `Cloak.GetReducedAmount(source, uint amount)` (`Entity/Cloak.cs`).
fn cloak_get_reduced_amount(w: &World, source: Option<ObjectGuid>, amount: u32) -> u32 {
    crate::entity::cloak::get_reduced_amount_uint(w, source, amount)
}

/// `Cloak.ShowMessage(this, source, amount, reducedAmount)` with `uint` amounts: C# picks the
/// `float` overload (`Entity/Cloak.cs`).
#[allow(clippy::cast_precision_loss)]
fn cloak_show_message(
    w: &mut World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    amount: u32,
    reduced: u32,
) {
    crate::entity::cloak::show_message_float(w, this, source, amount as f32, reduced as f32);
}

/// `Cloak.HasProcSpell(cloak)` (`Entity/Cloak.cs`).
fn cloak_has_proc_spell(w: &World, cloak: ObjectGuid) -> bool {
    crate::entity::cloak::has_proc_spell(w.objects.get(cloak))
}

/// `MagicState.IsCasting` (`Player_Magic.cs`).
fn magic_state_is_casting(w: &World, this: ObjectGuid) -> bool {
    world_object_networking::shims::player_magic_state_is_casting(w, this)
}

/// `FailCast()` (`Player_Magic.cs`; `tryFizzle` defaults to true).
fn fail_cast(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player_magic::fail_cast(w, this, true);
}

/// `House.HasPermission`, `LinkedHouses`, `HouseOwner`, `OpenStatus` (`House.cs`, not ported):
/// a found house reads as unowned, so it restricts nothing.
fn house_root_owned_and_closed(w: &World, house: ObjectGuid) -> Option<ObjectGuid> {
    let root_house = crate::world_objects::house::fields(w, house)
        .linked_houses
        .first()
        .copied()
        .unwrap_or(house);
    let r = w
        .objects
        .get(root_house)
        .expect("System.NullReferenceException: rootHouse");
    (r.house_owner().is_some() && !r.open_to_everyone()).then_some(root_house)
}

/// `house.HasPermission(player)` (`House.cs`, not ported).
fn house_has_permission(w: &World, house: ObjectGuid, player: ObjectGuid) -> bool {
    crate::world_objects::house::has_permission(w, house, player, false)
}

// ============================================================================== members

/// Returns the current attack skill for the player.
// ACE: Player.GetCurrentAttackSkill
pub fn get_current_attack_skill(w: &mut World, this: ObjectGuid) -> Skill {
    if creature_combat::combat_mode(w, this) == CombatMode::Magic {
        get_current_magic_skill(w, this)
    } else {
        get_current_weapon_skill(w, this)
    }
}

/// Returns the current weapon skill for the player.
// ACE: Player.GetCurrentWeaponSkill
pub fn get_current_weapon_skill(w: &mut World, this: ObjectGuid) -> Skill {
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);

    let Some(weapon) = weapon else {
        // DIVERGE: before the weapon-skill consolidation (`EraFeatures::
        // consolidated_weapon_skills`) an empty-handed player fights with Unarmed Combat
        // (ClassicACE's `GetCurrentWeaponSkill` outside its end-of-retail ruleset).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Combat.cs
        if !w.era.features.consolidated_weapon_skills {
            return Skill::UnarmedCombat;
        }
        return get_highest_melee_skill(w, this);
    };

    let weapon_skill = object(w, weapon).weapon_skill();
    let mut skill =
        crate::world_objects::world_object::convert_to_mo_a_skill(w, this, weapon_skill);

    // DualWieldAlternate will be TRUE if *next* attack is offhand
    if creature_melee::is_dual_wield_attack(w, this)
        && !creature_melee::dual_wield_alternate(w, this)
    {
        let weapon_skill = SkillOf::get(w, this, skill);
        let dual_wield = SkillOf::get(w, this, Skill::DualWield);

        // offhand attacks use the lower skill level between dual wield and weapon skill
        if dual_wield.current(w) < weapon_skill.current(w) {
            skill = Skill::DualWield;
        }
    }
    //Console.WriteLine($"{Name}.GetCurrentWeaponSkill - {skill}");
    skill
}

/// Returns the highest melee skill for the player (light / heavy / finesse).
// ACE: Player.GetHighestMeleeSkill
pub fn get_highest_melee_skill(w: &mut World, this: ObjectGuid) -> Skill {
    let light = SkillOf::get(w, this, Skill::LightWeapons);
    let heavy = SkillOf::get(w, this, Skill::HeavyWeapons);
    let finesse = SkillOf::get(w, this, Skill::FinesseWeapons);

    let mut max_melee = light;
    if heavy.current(w) > max_melee.current(w) {
        max_melee = heavy;
    }
    if finesse.current(w) > max_melee.current(w) {
        max_melee = finesse;
    }

    max_melee.skill.skill
}

/// Melee, or Missile with a weapon wielded in the missile slot.
// ACE: Player.GetCombatType
#[must_use]
pub fn get_combat_type(w: &World, this: ObjectGuid) -> CombatType {
    // this is an unsafe function, move away from this
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);

    match weapon {
        Some(g) if object(w, g).current_wielded_location() == Some(EquipMask::MissileWeapon) => {
            CombatType::Missile
        }
        _ => CombatType::Melee,
    }
}

/// The player hits (or misses) a creature with `damage_source`: the PK check, the damage event,
/// the target's damage or evasion, and the attacker's messages, sounds and procs. `None` for a
/// dead target or a PK refusal.
// ACE: Player.DamageTarget
pub fn damage_target(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    damage_source: Option<ObjectGuid>,
) -> Option<DamageEvent> {
    // DIVERGE: ACE's strike closure holds the `Creature` itself, so a target that died and was
    // destroyed between the swing and the strike still reads `Health.Current <= 0` and returns
    // null here. The store no longer has it, so a missing target reads as dead. A target
    // destroyed without dying (a despawn) would still be struck in ACE; here it is not.
    if w.objects.get(target).is_none() || is_dead(w, target) {
        return None;
    }

    let target_player = is_player(w, target);

    // check PK status
    if let Some(pk_error) = check_pk_status_vs_target(w, this, Some(target), None) {
        let target_name = shim::name(w, target);
        let s = session(w, this);
        let msg =
            game_event_weenie_error_with_string(session_data(w, s), pk_error[0], &target_name);
        enqueue_send(w, s, msg);
        if target_player {
            let name = shim::name(w, this);
            let ts = session(w, target);
            let msg = game_event_weenie_error_with_string(session_data(w, ts), pk_error[1], &name);
            enqueue_send(w, ts, msg);
        }
        return None;
    }

    let damage_event = DamageEvent::calculate_damage(w, this, target, damage_source, None, None);

    if damage_event.has_damage() {
        on_damage_target(
            w,
            this,
            target,
            damage_event.combat_type,
            damage_event.is_critical,
        );

        if target_player {
            take_damage_event(w, target, Some(this), &damage_event);
        } else {
            dispatch::take_damage::take_damage(
                w,
                target,
                this,
                damage_event.damage_type,
                damage_event.damage,
                damage_event.is_critical,
            );
        }
    } else {
        if damage_event.lifestone_protection {
            let msg = game_message_system_chat(
                &format!(
                    "The Lifestone's magic protects {} from the attack!",
                    shim::name(w, target)
                ),
                ChatMessageType::Magic,
            );
            send(w, this, msg);
        } else if !squelches_contains(w, this, target, ChatMessageType::CombatSelf) {
            let target_name = shim::name(w, target);
            let s = session(w, this);
            let msg = game_event_evasion_attacker_notification(session_data(w, s), &target_name);
            enqueue_send(w, s, msg);
        }

        if target_player {
            on_evade(w, target, this, damage_event.combat_type);
        }
    }

    if damage_event.has_damage() && is_alive(w, target) {
        // notify attacker
        let int_damage: u32 = math::round(f64::from(damage_event.damage)).cs_cast();

        if !squelches_contains(w, this, this, ChatMessageType::CombatSelf) {
            let max = max_health(w, target);
            let target_name = shim::name(w, target);
            let s = session(w, this);
            let msg = game_event_attacker_notification(
                session_data(w, s),
                &target_name,
                damage_event.damage_type,
                int_damage as f32 / max as f32,
                int_damage,
                damage_event.is_critical,
                damage_event.attack_conditions(),
            );
            enqueue_send(w, s, msg);
        }

        // splatter effects
        if !target_player {
            send(w, this, game_message_sound(target, Sound::HitFlesh1, 0.5));
            if damage_event.damage >= max_health(w, target) as f32 * 0.25f32 {
                let pain_sound =
                    Sound::from_name(&format!("Wound{}", ThreadSafeRandom::next(1, 3)))
                        .expect("System.ArgumentException: Enum.Parse(Sound)");
                send(w, this, game_message_sound(target, pain_sound, 1.0));
            }
            let splatter = splatter_script(w, this, target);
            send(w, this, game_message_script(target, splatter, 1.0));
        }

        // handle Dirty Fighting
        if SkillOf::get(w, this, Skill::DirtyFighting).advancement_class(w)
            >= SkillAdvancementClass::Trained
        {
            creature_combat::fight_dirty(w, this, target, damage_event.weapon);
        }

        emote_manager_on_damage(w, target, this);

        if damage_event.is_critical {
            emote_manager_on_receive_critical(w, target, this);
        }
    }

    if !target_player {
        on_attack_monster(w, this, target);
    }

    Some(damage_event)
}

/// Sets the creature that last attacked a player. Called when the player takes damage, evades or
/// resists a spell from a creature; a change is sent to the client (its 'last attacker' key).
// ACE: Player.SetCurrentAttacker
pub fn set_current_attacker(w: &mut World, this: ObjectGuid, current_attacker: ObjectGuid) {
    if current_attacker == this
        || object(w, this).get_property(PropertyInstanceId::CurrentAttacker)
            == Some(current_attacker.full())
    {
        return;
    }

    object_mut(w, this).set_property(PropertyInstanceId::CurrentAttacker, current_attacker.full());

    let msg = game_message_private_update_instance_id(
        object_mut(w, this),
        PropertyInstanceId::CurrentAttacker,
        current_attacker.full(),
    );
    send(w, this, msg);
}

/// Called when a player hits a target: the weapon skill's proficiency.
// ACE: Player.OnDamageTarget
pub fn on_damage_target(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    _attack_type: CombatType,
    _critical: bool,
) {
    let weapon_skill = get_current_weapon_skill(w, this);
    let attack_skill = SkillOf::get(w, this, weapon_skill);
    let difficulty = get_target_effective_defense_skill(w, this, target);

    proficiency_on_success_use(w, this, attack_skill, difficulty);
}

/// The weapon skill scaled by the accuracy bar and the weapon's offense modifier.
// ACE: Player.GetEffectiveAttackSkill
pub fn get_effective_attack_skill(w: &mut World, this: ObjectGuid) -> u32 {
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);
    let skill = get_current_weapon_skill(w, this);
    let attack_skill = SkillOf::get(w, this, skill).current(w);
    let offense_mod = weapon_mod::get_weapon_offense_modifier(w, Some(this));
    let accuracy_mod = get_accuracy_mod(w, this, weapon);

    //if (IsExhausted)
    //attackSkill = GetExhaustedSkill(attackSkill);

    //var baseStr = offenseMod != 1.0f ? $" (base: {GetCreatureSkill(GetCurrentWeaponSkill()).Current})" : "";
    //Console.WriteLine("Attack skill: " + attackSkill + baseStr);

    math::round(f64::from(attack_skill as f32 * accuracy_mod * offense_mod)).cs_cast()
}

/// The target creature's defense skill against this player's current combat type (0 for a
/// non-creature or an exhausted target).
// ACE: Player.GetTargetEffectiveDefenseSkill
pub fn get_target_effective_defense_skill(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
) -> u32 {
    if !is_creature(w, target) {
        return 0;
    }
    let creature = target;

    let attack_type = get_combat_type(w, this);
    let defense_skill = if attack_type == CombatType::Missile {
        Skill::MissileDefense
    } else {
        Skill::MeleeDefense
    };
    let defense_mod = if defense_skill == Skill::MeleeDefense {
        weapon_mod::get_weapon_melee_defense_modifier(w, Some(creature))
    } else {
        1.0f32
    };
    let current = SkillOf::get(w, creature, defense_skill).current(w);
    let mut effective_defense: u32 = math::round(f64::from(current as f32 * defense_mod)).cs_cast();

    if crate::world_objects::creature::is_exhausted(
        w.objects.get(creature).expect("ACE: this is null"),
    ) {
        effective_defense = 0;
    }

    //var baseStr = defenseMod != 1.0f ? $" (base: {creature.GetCreatureSkill(defenseSkill).Current})" : "";
    //Console.WriteLine("Defense skill: " + effectiveDefense + baseStr);

    effective_defense
}

/// Returns a modifier to the player's defense skill, based on current motion state.
// ACE: Player.GetDefenseStanceMod
#[must_use]
pub fn get_defense_stance_mod(w: &World, this: ObjectGuid) -> f32 {
    if crate::world_objects::player::is_jumping(w, this) {
        return 0.5;
    }

    if is_logging_out(w, this) {
        return 0.8;
    }

    if creature_combat::combat_mode(w, this) != CombatMode::NonCombat {
        return 1.0;
    }

    let d = &object(w, this)
        .wo
        .world_object_properties
        .current_movement_data;
    let forward_command = match &d.invalid {
        Some(invalid) if d.movement_type == MovementType::Invalid => invalid.state.forward_command,
        _ => MotionCommand::Invalid,
    };

    match forward_command {
        // TODO: verify multipliers
        MotionCommand::Crouch => 0.4,
        MotionCommand::Sitting => 0.3,
        MotionCommand::Sleeping => 0.2,
        _ => 1.0,
    }
}

/// Called when player successfully avoids an attack: the last attacker, then (in combat mode) a
/// stamina point unless Endurance saves it, the evasion message and the defense proficiency.
// ACE: Player.OnEvade
pub fn on_evade(w: &mut World, this: ObjectGuid, attacker: ObjectGuid, attack_type: CombatType) {
    let creature_attacker = is_creature(w, attacker).then_some(attacker);

    if let Some(creature_attacker) = creature_attacker {
        set_current_attacker(w, this, creature_attacker);
    }

    if object(w, this).under_lifestone_protection() {
        return;
    }

    // http://asheron.wikia.com/wiki/Attributes

    // Endurance will also make it less likely that you use a point of stamina to successfully evade a missile or melee attack.
    // A player is required to have Melee Defense for melee attacks or Missile Defense for missile attacks trained or specialized
    // in order for this specific ability to work. This benefit is tied to Endurance only, and it caps out at around a 75% chance
    // to avoid losing a point of stamina per successful evasion.

    let defense_skill_type = if attack_type == CombatType::Missile {
        Skill::MissileDefense
    } else {
        Skill::MeleeDefense
    };
    let defense_skill = SkillOf::get(w, this, defense_skill_type);

    if creature_combat::combat_mode(w, this) != CombatMode::NonCombat {
        if defense_skill.advancement_class(w) >= SkillAdvancementClass::Trained {
            let endurance_base: i32 =
                attribute_base(w, this, PropertyAttribute::Endurance).cs_cast();

            // TODO: find exact formula / where it caps out at 75%

            // more literal / linear formula
            //var noStaminaUseChance = (enduranceBase - 50) / 320.0f;

            // gdle curve-based formula, caps at 300 instead of 290
            let mut no_stamina_use_chance =
                (endurance_base as f32 * endurance_base as f32 * 0.000005f32)
                    + (endurance_base as f32 * 0.00124f32)
                    - 0.07f32;

            no_stamina_use_chance = math_clamp_f32(no_stamina_use_chance, 0.0, 0.75);

            //Console.WriteLine($"NoStaminaUseChance: {noStaminaUseChance}");

            if f64::from(no_stamina_use_chance) <= ThreadSafeRandom::next_float(0.0, 1.0) {
                let stamina = object(w, this).stamina();
                creature_vitals::update_vital_delta(w, this, stamina, -1);
            }
        } else {
            let stamina = object(w, this).stamina();
            creature_vitals::update_vital_delta(w, this, stamina, -1);
        }
    } else {
        // if the player is in non-combat mode, no stamina is consumed on evade
        // reference: https://youtu.be/uFoQVgmSggo?t=145
        // from the dm guide, page 147: "if you are not in Combat mode, you lose no Stamina when an attack is thrown at you"

        //UpdateVitalDelta(Stamina, -1);
    }

    if !squelches_contains(w, this, attacker, ChatMessageType::CombatEnemy) {
        let attacker_name = shim::name(w, attacker);
        let s = session(w, this);
        let msg = game_event_evasion_defender_notification(session_data(w, s), &attacker_name);
        enqueue_send(w, s, msg);
    }

    let Some(creature_attacker) = creature_attacker else {
        return;
    };

    let weapon_skill =
        dispatch::get_current_weapon_skill::get_current_weapon_skill(w, creature_attacker);
    let difficulty = SkillOf::get(w, creature_attacker, weapon_skill).current(w);
    // attackMod?
    proficiency_on_success_use(w, this, defense_skill, difficulty);
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

/// The base damage of an attack with `damage_source`: the player's own attack uses the hand or
/// foot armor for a punch or kick, else a bare 2 (Olthoi 130) with 0.75 variance.
// ACE: Player.GetBaseDamageMod
pub fn get_base_damage_mod(
    w: &mut World,
    this: ObjectGuid,
    damage_source: ObjectGuid,
) -> BaseDamageMod {
    if damage_source == this && w.era.formulas.older_melee_damage {
        // DIVERGE: before the weapon-skill consolidation (`EraFormulas::older_melee_damage`) a
        // bare punch or kick does 1, and hand or foot armour adds its own damage to that 1 (the
        // strategy guide's `BaseDmg = 1 + ArmorDmg + Skill / 20`, the skill's part added by the
        // damage event); ClassicACE's `GetBaseDamageMod` outside its end-of-retail ruleset.
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Combat.cs
        let attack_type = creature_combat::attack_type(w, this);
        let damage_source = if attack_type == AttackType::Punch {
            hand_armor(w, this)
        } else if attack_type == AttackType::Kick {
            foot_armor(w, this)
        } else {
            Some(damage_source)
        };
        return match damage_source {
            None => BaseDamageMod::new(BaseDamage::new(1, 0.75)),
            Some(damage_source) => {
                let mut m = crate::world_objects::world_object::get_damage_mod(
                    w,
                    damage_source,
                    this,
                    Some(damage_source),
                );
                m.base_damage.max_damage = m.base_damage.max_damage.wrapping_add(1);
                m
            }
        };
    }
    if damage_source == this {
        let attack_type = creature_combat::attack_type(w, this);
        let damage_source = if attack_type == AttackType::Punch {
            hand_armor(w, this)
        } else if attack_type == AttackType::Kick {
            foot_armor(w, this)
        } else {
            Some(damage_source)
        };

        // no weapon, no hand or foot armor
        return match damage_source
            .filter(|&g| object(w, g).get_property(PropertyInt::Damage).is_some())
        {
            None => {
                if object(w, this).heritage_group() == HeritageGroup::Olthoi {
                    BaseDamageMod::new(BaseDamage::new(130, 0.75))
                } else {
                    BaseDamageMod::new(BaseDamage::new(2, 0.75))
                }
            }
            Some(damage_source) => crate::world_objects::world_object::get_damage_mod(
                w,
                damage_source,
                this,
                Some(damage_source),
            ),
        };
    }
    crate::world_objects::world_object::get_damage_mod(w, damage_source, this, None)
}

/// Returns 0.5-1.5 for non-bow attacks, from the power bar.
// ACE: Player.GetPowerMod
#[must_use]
pub fn get_power_mod(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    if weapon.is_none_or(|g| !object(w, g).is_ranged()) {
        player_melee::power_level(w, this) + 0.5
    } else {
        1.0
    }
}

/// Returns 0.6-1.6 for bow attacks, from the accuracy bar.
// ACE: Player.GetAccuracyMod
#[must_use]
pub fn get_accuracy_mod(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    if weapon.is_some_and(|g| object(w, g).is_ranged()) {
        player_missile::accuracy_level(w, this) + 0.6
    } else {
        1.0
    }
}

/// The accuracy bar for a missile attack, else the power bar.
// ACE: Player.GetPowerAccuracyBar
#[must_use]
pub fn get_power_accuracy_bar(w: &World, this: ObjectGuid) -> f32 {
    if get_combat_type(w, this) == CombatType::Missile {
        player_missile::accuracy_level(w, this)
    } else {
        player_melee::power_level(w, this)
    }
}

/// The sound of a hit on this player (always `HitFlesh1`; ACE's armor lookup is commented out).
// ACE: Player.GetHitSound
#[must_use]
pub fn get_hit_sound(
    _w: &World,
    _this: ObjectGuid,
    _source: Option<ObjectGuid>,
    _body_part: BodyPart,
) -> Sound {
    /*var creature = source as Creature;
    var armors = creature.GetArmor(bodyPart);

    foreach (var armor in armors)
    {
        var material = armor.GetProperty(PropertyInt.MaterialType) ?? 0;
        //Console.WriteLine("Name: " + armor.Name + " | Material: " + material);
    }*/
    Sound::HitFlesh1
}

/// Simplified player take damage function, only called for DoTs currently.
// ACE: Player.TakeDamageOverTime
pub fn take_damage_over_time(
    w: &mut World,
    this: ObjectGuid,
    amount_f: f32,
    damage_type: DamageType,
) {
    if object(w, this).invincible() || is_dead(w, this) {
        return;
    }

    // check lifestone protection
    if object(w, this).under_lifestone_protection() {
        player_death::handle_lifestone_protection(w, this);
        return;
    }

    let amount: u32 = math::round(f64::from(amount_f)).cs_cast();
    let percent = amount as f32 / max_health(w, this) as f32;

    // update health
    let health = object(w, this).health();
    let _damage_taken: u32 =
        creature_vitals::update_vital_delta(w, this, health, amount.cast_signed().wrapping_neg())
            .wrapping_neg()
            .cast_unsigned();

    // update stamina
    //UpdateVitalDelta(Stamina, -1);

    //if (Fellowship != null)
    //Fellowship.OnVitalUpdate(this);

    // send damage text message
    //if (PropertyManager.GetBool("show_dot_messages").Item)
    //{
    let nether = if damage_type == DamageType::Nether {
        "nether "
    } else {
        ""
    };
    let chat_message_type = if damage_type == DamageType::Nether {
        ChatMessageType::Magic
    } else {
        ChatMessageType::Combat
    };
    let text = format!("You receive {amount} points of periodic {nether}damage.");
    player::send_message(w, this, &text, chat_message_type);
    //}

    // splatter effects
    //var splatter = new GameMessageScript(Guid, (PlayScript)Enum.Parse(typeof(PlayScript), "Splatter" + creature.GetSplatterHeight() + creature.GetSplatterDir(this)));  // not sent in retail, but great visual indicator?
    let splatter = game_message_script(
        this,
        if damage_type == DamageType::Nether {
            PlayScript::HealthDownVoid
        } else {
            PlayScript::DirtyFightingDamageOverTime
        },
        1.0,
    );
    enqueue_broadcast(w, this, &[splatter]);

    if is_dead(w, this) {
        // since damage over time is possibly combined from multiple sources,
        // sending a message to the last damager here could be tricky..

        // TODO: get last damager from dot stack instead?
        let last_damager = damage_history::of(w, this).last_damager();
        dispatch::on_death::on_death(w, this, last_damager, damage_type, false);
        creature_death::die(w, this);

        return;
    }

    if percent >= 0.1 {
        let msg = game_message_sound(this, Sound::Wound1, 1.0);
        enqueue_broadcast(w, this, &[msg]);
    }
}

/// `TakeDamage(WorldObject source, DamageEvent damageEvent)`.
// ACE: Player.TakeDamage
pub fn take_damage_event(
    w: &mut World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    damage_event: &DamageEvent,
) -> i32 {
    take_damage(
        w,
        this,
        source,
        damage_event.damage_type,
        damage_event.damage,
        damage_event.body_part,
        damage_event.is_critical,
        damage_event.attack_conditions(),
    )
}

/// Applies damage to a player from a physical damage source (`TakeDamage(WorldObject source,
/// DamageType, float _amount, BodyPart bodyPart, bool crit = false, AttackConditions
/// attackConditions = None)`); answers the damage taken.
// ACE: Player.TakeDamage
#[allow(clippy::too_many_arguments)]
pub fn take_damage(
    w: &mut World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    damage_type: DamageType,
    amount_f: f32,
    body_part: BodyPart,
    crit: bool,
    attack_conditions: AttackConditions,
) -> i32 {
    if object(w, this).invincible() || is_dead(w, this) {
        return 0;
    }

    if let Some(creature_attacker) = source.filter(|&s| is_creature(w, s)) {
        set_current_attacker(w, this, creature_attacker);
    }

    // check lifestone protection
    if object(w, this).under_lifestone_protection() {
        player_death::handle_lifestone_protection(w, this);
        return 0;
    }

    if amount_f < 0.0 {
        log::error!(
            "{}.TakeDamage({} ({}), {}, {amount_f}) - negative damage, this shouldn't happen",
            shim::name(w, this),
            source.map(|s| shim::name(w, s)).unwrap_or_default(),
            source.map(|s| s.to_string()).unwrap_or_default(),
            damage_type.to_dotnet_string()
        );
        return 0;
    }

    let mut amount: u32 = math::round(f64::from(amount_f)).cs_cast();
    let mut percent = amount as f32 / max_health(w, this) as f32;

    let equipped_cloak = creature_combat::equipped_cloak(w, this);

    if let Some(cloak) = equipped_cloak {
        if cloak_damage_proc(w, cloak, percent) {
            let reduced_amount = cloak_get_reduced_amount(w, source, amount);

            cloak_show_message(w, this, source, amount, reduced_amount);

            amount = reduced_amount;
            percent = amount as f32 / max_health(w, this) as f32;
        }
    }

    // update health
    let health = object(w, this).health();
    let damage_taken: u32 =
        creature_vitals::update_vital_delta(w, this, health, amount.cast_signed().wrapping_neg())
            .wrapping_neg()
            .cast_unsigned();
    // DIVERGE: ACE's `DamageHistory.Add(null, ...)` throws for a null source; none reaches here.
    if let Some(source) = source {
        damage_history::add(w, this, source, damage_type, damage_taken);
    }

    // update stamina
    if creature_combat::combat_mode(w, this) != CombatMode::NonCombat {
        // if the player is in non-combat mode, no stamina is consumed on evade
        // reference: https://youtu.be/uFoQVgmSggo?t=145
        // from the dm guide, page 147: "if you are not in Combat mode, you lose no Stamina when an attack is thrown at you"

        let stamina = object(w, this).stamina();
        creature_vitals::update_vital_delta(w, this, stamina, -1);
    }

    //if (Fellowship != null)
    //Fellowship.OnVitalUpdate(this);

    if is_dead(w, this) {
        let last_damager = source.map(|s| DamageHistoryInfo::new(w, s, 0.0));
        dispatch::on_death::on_death(w, this, last_damager, damage_type, crit);
        creature_death::die(w, this);
        return damage_taken.cast_signed();
    }

    let Some(&i_damage_location) = body_part::INDICES.get(&body_part) else {
        log::warn!(
            "{}.TakeDamage({}, {}, {amount}, {}, {crit}): avoided crash for bad damage location",
            shim::name(w, this),
            source.map(|s| shim::name(w, s)).unwrap_or_default(),
            damage_type.to_dotnet_string(),
            body_part.0
        );
        return 0;
    };
    let damage_location = empyrean_net::enums::DamageLocation(i_damage_location.cast_unsigned());

    // send network messages
    if let Some(creature) = source.filter(|&s| is_creature(w, s)) {
        if !squelches_contains(w, this, creature, ChatMessageType::CombatEnemy) {
            let creature_name = shim::name(w, creature);
            let s = session(w, this);
            let msg = game_event_defender_notification(
                session_data(w, s),
                &creature_name,
                damage_type,
                percent,
                amount,
                damage_location,
                crit,
                attack_conditions,
            );
            enqueue_send(w, s, msg);
        }

        let hit_sound = game_message_sound(this, get_hit_sound(w, this, source, body_part), 1.0);
        let splatter = game_message_script(this, splatter_script(w, creature, this), 1.0);
        enqueue_broadcast(w, this, &[hit_sound, splatter]);
    }

    if percent >= 0.1 {
        // Wound1 - Aahhh!    - elemental attacks above some threshold
        // Wound2 - Deep Ugh! - bludgeoning attacks above some threshold
        // Wound3 - Ooh!      - slashing / piercing / undef attacks above some threshold

        let mut wound_sound = Sound::Wound3;

        if damage_type == DamageType::Bludgeon {
            wound_sound = Sound::Wound2;
        } else if (damage_type & DamageType::Elemental) != DamageType::Undef {
            wound_sound = Sound::Wound1;
        }

        let msg = game_message_sound(this, wound_sound, 1.0);
        enqueue_broadcast(w, this, &[msg]);
    }

    if let Some(cloak) = equipped_cloak {
        if cloak_has_proc_spell(w, cloak) {
            crate::entity::cloak::try_proc_spell(w, this, source, Some(cloak), percent);
        }
    }

    // if player attacker, update PK timer
    if let Some(attacker) = source.filter(|&s| is_player(w, s)) {
        update_pk_timers(w, attacker, this);
    }

    damage_taken.cast_signed()
}

/// Flesh, Leather, Chain, Plate (for hit sounds): not implemented in ACE.
// ACE: Player.GetArmorType
#[must_use]
pub fn get_armor_type(_w: &World, _this: ObjectGuid, _body_part: BodyPart) -> Option<String> {
    // Flesh, Leather, Chain, Plate
    // for hit sounds
    None
}

/// Returns the total burden of items held in both hands (main hand and offhand).
// ACE: Player.GetHeldItemBurden
#[must_use]
pub fn get_held_item_burden(w: &World, this: ObjectGuid) -> i32 {
    let mainhand = creature_equipment::get_equipped_main_hand(w, this);
    let offhand = creature_equipment::get_equipped_off_hand(w, this);

    let mainhand_burden = mainhand
        .and_then(|g| object(w, g).encumbrance_val())
        .unwrap_or(0);
    let offhand_burden = offhand
        .and_then(|g| object(w, g).encumbrance_val())
        .unwrap_or(0);

    mainhand_burden.wrapping_add(offhand_burden)
}

/// The Endurance discount on attack stamina: `1 - (Endurance.Base - 50) / 480`, clamped to 0.5-1.
// ACE: Player.GetStaminaMod
#[must_use]
pub fn get_stamina_mod(w: &World, this: ObjectGuid) -> f32 {
    let endurance: i32 = attribute_base(w, this, PropertyAttribute::Endurance).cs_cast();

    // more literal / linear formula
    let mut stamina_mod = 1.0f32 - endurance.wrapping_sub(50) as f32 / 480.0f32;

    // gdle curve-based formula, caps at 300 instead of 290
    //var staminaMod = (endurance * endurance * -0.000003175f) - (endurance * 0.0008889f) + 1.052f;

    stamina_mod = math_clamp_f32(stamina_mod, 0.5, 1.0);

    // this is also specific to gdle,
    // additive luck which can send the base stamina way over 1.0
    /*var luck = ThreadSafeRandom.Next(0.0f, 1.0f);
    staminaMod += luck;*/

    stamina_mod
}

/// Calculates the amount of stamina required to perform this attack: the held burden's cost at
/// this bar, less the Endurance discount, at least 1.
// ACE: Player.GetAttackStamina
#[must_use]
pub fn get_attack_stamina(w: &World, this: ObjectGuid, power_accuracy: PowerAccuracy) -> i32 {
    // Stamina cost for melee and missile attacks is based on the total burden of what you are holding
    // in your hands (main hand and offhand), and your power/accuracy bar.

    // Attacking(Low power / accuracy bar)   1 point per 700 burden units
    //                                       1 point per 1200 burden units
    //                                       1.5 points per 1600 burden units
    // Attacking(Mid power / accuracy bar)   1 point per 700 burden units
    //                                       2 points per 1200 burden units
    //                                       3 points per 1600 burden units
    // Attacking(High power / accuracy bar)  2 point per 700 burden units
    //                                       4 points per 1200 burden units
    //                                       6 points per 1600 burden units

    // The higher a player's base Endurance, the less stamina one uses while attacking. This benefit is tied to Endurance only,
    // and caps out at 50% less stamina used per attack. Scaling is similar to other Endurance bonuses. Applies only to players.

    // When stamina drops to 0, your melee and missile defenses also drop to 0 and you will be incapable of attacking.
    // In addition, you will suffer a 50% penalty to your weapon skill. This applies to players and creatures.

    let burden = get_held_item_burden(w, this);

    let base_cost = stamina_table::get_stamina_cost(power_accuracy, burden);

    let stamina_mod = get_stamina_mod(w, this);

    let stamina_cost = math::max_f32(base_cost * stamina_mod, 1.0);

    //Console.WriteLine($"GetAttackStamina({powerAccuracy}) - burden: {burden}, baseCost: {baseCost}, staminaMod: {staminaMod}, staminaCost: {staminaCost}");

    math::round(f64::from(stamina_cost)).cs_cast()
}

/// Returns the damage rating modifier for an applicable Recklessness attack.
// ACE: Player.GetRecklessnessMod
pub fn get_recklessness_mod(w: &mut World, this: ObjectGuid) -> f32 {
    // ensure melee or missile combat mode
    let mode = creature_combat::combat_mode(w, this);
    if mode != CombatMode::Melee && mode != CombatMode::Missile {
        return 1.0;
    }

    let skill = SkillOf::get(w, this, Skill::Recklessness);

    // recklessness skill must be either trained or specialized to use
    if skill.advancement_class(w) < SkillAdvancementClass::Trained {
        return 1.0;
    }

    // recklessness is active when attack bar is between 20% and 80% (according to wiki)
    // client attack bar range seems to indicate this might have been updated, between 10% and 90%?
    let power_accuracy_bar = get_power_accuracy_bar(w, this);
    //if (powerAccuracyBar < 0.2f || powerAccuracyBar > 0.8f)
    if power_accuracy_bar < 0.1 || power_accuracy_bar > 0.9 {
        return 1.0;
    }

    // recklessness only applies to non-critical hits,
    // which is handled outside of this method.

    // damage rating is increased by 20 for specialized, and 10 for trained.
    // incoming non-critical damage from all sources is increased by the same.
    let mut damage_rating: i32 = if skill.advancement_class(w) == SkillAdvancementClass::Specialized
    {
        20
    } else {
        10
    };

    // if recklessness skill is lower than current attack skill (as determined by your equipped weapon)
    // then the damage rating is reduced proportionately. The damage rating caps at 10 for trained
    // and 20 for specialized, so there is no reason to raise the skill above your attack skill.
    let attack = get_current_attack_skill(w, this);
    let attack_skill = SkillOf::get(w, this, attack);

    if skill.current(w) < attack_skill.current(w) {
        let scale = skill.current(w) as f32 / attack_skill.current(w) as f32;
        damage_rating = math::round(f64::from(damage_rating as f32 * scale)).cs_cast();
    }

    // The damage rating adjustment for incoming damage is also adjusted proportinally if your Recklessness skill
    // is lower than your active attack skill

    // trained DR 1.10 = 10% additional damage
    // specialized DR 1.20 = 20% additional damage
    creature_rating::get_damage_rating_int(damage_rating)
}

/// Returns TRUE if this player is PK and died to another player (`IsPKDeath(DamageHistoryInfo)`
/// and `IsPKDeath(uint? killerGuid)`).
// ACE: Player.IsPKDeath
#[must_use]
pub fn is_pk_death(w: &World, this: ObjectGuid, killer_guid: Option<u32>) -> bool {
    object(w, this)
        .player_killer_status()
        .contains(PlayerKillerStatus::PK)
        && ObjectGuid::new(killer_guid.unwrap_or(0)).is_player()
        && killer_guid != Some(this.full())
}

/// Returns TRUE if this player is PKLite and died to another player.
// ACE: Player.IsPKLiteDeath
#[must_use]
pub fn is_pk_lite_death(w: &World, this: ObjectGuid, killer_guid: Option<u32>) -> bool {
    object(w, this)
        .player_killer_status()
        .contains(PlayerKillerStatus::PKLite)
        && ObjectGuid::new(killer_guid.unwrap_or(0)).is_player()
        && killer_guid != Some(this.full())
}

// ACE: Player.UseTimeEpsilon
pub const USE_TIME_EPSILON: f32 = 0.05;

/// Not ACE (V417): how long after an earlier waiting combat-mode change a later one runs at the
/// soonest (one microsecond: an order, not a delay anyone sees).
const DEFERRED_COMBAT_MODE_GAP: f64 = 1e-6;

/// The client asks to change combat mode (game action 0x0053): an invalid weapon setup is
/// refused back to peace; otherwise the change runs now, or once the previous one's animation
/// (`NextUseTime`) is over. ACE's defaults: `force_hand_combat = false`, `callback = null`.
// ACE: Player.HandleActionChangeCombatMode
#[allow(clippy::type_complexity)]
pub fn handle_action_change_combat_mode(
    w: &mut World,
    this: ObjectGuid,
    new_combat_mode: CombatMode,
    force_hand_combat: bool,
    callback: Option<Box<dyn FnOnce(&mut World) + Send>>,
) {
    //log.Info($"{Name}.HandleActionChangeCombatMode({newCombatMode})");

    // Make sure the player doesn't have an invalid weapon setup (e.g. sword + wand)
    if !player_inventory::check_weapon_collision(w, this, None, None, Some(new_combat_mode)) {
        let s = session(w, this);
        let msg = game_event_weenie_error(session_data(w, s), WeenieError::ActionCancelled); // "Action cancelled!"
        enqueue_send(w, s, msg);

        // Go back to non-Combat mode
        let (anim_time, _queue_time) =
            creature_combat::set_combat_mode_with(w, this, new_combat_mode, false, true);

        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(anim_time));
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
        });
        action_chain.enqueue_chain(w);

        let t = w.now.utc.add_seconds(f64::from(anim_time));
        set_next_use_time(w, this, t);
        return;
    }

    fields_mut(w, this).last_combat_mode = new_combat_mode;

    if w.now.utc >= next_use_time(w, this).add_seconds(f64::from(USE_TIME_EPSILON)) {
        handle_action_change_combat_mode_inner(
            w,
            this,
            new_combat_mode,
            force_hand_combat,
            callback,
        );
    } else {
        let mut action_chain = ActionChain::new();
        let delay =
            (next_use_time(w, this) - w.now.utc).total_seconds() + f64::from(USE_TIME_EPSILON);
        // Not ACE (V417): a change that waits runs after any change that began waiting before
        // it. Two requests waiting on the same animation (a weapon swap's unwield and wield) aim
        // at the same moment; ACE reads the live clock, so the one asked first comes out earlier
        // and the last request is the one that sticks. This server's clock is one reading per
        // tick, so the two ends tie up to rounding and could run in either order.
        let now = timers::portal_year_ticks(w);
        let fields = fields_mut(w, this);
        let end = (now + delay).max(fields.deferred_combat_mode_end + DEFERRED_COMBAT_MODE_GAP);
        fields.deferred_combat_mode_end = end;
        let delay = end - now;
        action_chain.add_delay_seconds(w, delay);
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            handle_action_change_combat_mode_inner(
                w,
                this,
                new_combat_mode,
                force_hand_combat,
                callback,
            );
        });
        action_chain.enqueue_chain(w);
    }

    if object(w, this).is_afk() {
        player_networking::handle_action_set_afk_mode(w, this, false);
    }
}

/// The combat mode change itself: cancels any attack, validates the weapons for the new mode
/// (reverting the client to peace when they do not fit), sets the ammo's placement, switches the
/// stance and schedules `callback` after the animation.
// ACE: Player.HandleActionChangeCombatMode_Inner
#[allow(clippy::type_complexity, clippy::collapsible_match)] // ACE's switch
pub fn handle_action_change_combat_mode_inner(
    w: &mut World,
    this: ObjectGuid,
    new_combat_mode: CombatMode,
    force_hand_combat: bool,
    callback: Option<Box<dyn FnOnce(&mut World) + Send>>,
) {
    //log.Info($"{Name}.HandleActionChangeCombatMode_Inner({newCombatMode})");

    let current_combat_stance = creature_combat::get_combat_stance(w, this);

    let missile_weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let caster = creature_equipment::get_equipped_wand(w, this);

    if creature_combat::combat_mode(w, this) == CombatMode::Magic && magic_state_is_casting(w, this)
    {
        fail_cast(w, this);
    }

    player_melee::handle_action_cancel_attack(w, this, WeenieError::None);

    let is_launcher_stance = |s: MotionStance| {
        s == MotionStance::BowCombat
            || s == MotionStance::CrossbowCombat
            || s == MotionStance::AtlatlCombat
    };

    match new_combat_mode {
        CombatMode::NonCombat => {
            if is_launcher_stance(current_combat_stance) {
                if let Some(equipped_ammo) = creature_equipment::get_equipped_ammo(w, this) {
                    creature_equipment::clear_child(w, this, equipped_ammo); // We must clear the placement/parent when going back to peace
                }
            }
        }
        CombatMode::Melee => {
            // todo expand checks
            if !force_hand_combat && (missile_weapon.is_some() || caster.is_some()) {
                // client has already independently brought the melee bar up by this point, revert and sync everything back up
                creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
                return;
            }
        }
        CombatMode::Missile => {
            if missile_weapon.is_none() {
                // client has already independently switched to missile mode by this point,
                // so instead of simply returning here, we need to deny the request by reverting to either the current server combat state, or switching to NonCombat to maintain client sync
                // this is especially important for missile, because the client is unable to break out of this bugged state for this mode specifically
                // see: ClientCombatSystem::PlayerInReadyPosition

                creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
                return;
            }

            if is_launcher_stance(current_combat_stance) {
                match creature_equipment::get_equipped_ammo(w, this) {
                    None => {
                        let (anim_time, _queue_time) = creature_combat::set_combat_mode_with(
                            w,
                            this,
                            new_combat_mode,
                            false,
                            false,
                        );

                        let mut action_chain = ActionChain::new();
                        action_chain.add_delay_seconds(w, f64::from(anim_time));
                        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
                            let s = session(w, this);
                            let msg = game_event_communication_transient_string(
                                session_data(w, s),
                                "You are out of ammunition!",
                            );
                            enqueue_send(w, s, msg);
                            creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
                        });
                        action_chain.enqueue_chain(w);

                        let t = w.now.utc.add_seconds(f64::from(anim_time));
                        set_next_use_time(w, this, t);
                        return;
                    }
                    Some(equipped_ammo) => {
                        // We must set the placement/parent when going into combat
                        let ammo = object_mut(w, equipped_ammo);
                        ammo.set_placement(Some(Placement::RightHandCombat));
                        ammo.set_parent_location(Some(ParentLocation::RightHand));
                    }
                }
            }
        }
        CombatMode::Magic => {
            // todo expand checks
            if caster.is_none() {
                // client has already independently brought the magic bar up by this point, revert and sync everything back up
                creature_combat::set_combat_mode(w, this, CombatMode::NonCombat);
                return;
            }
        }
        _ => {}
    }

    // animTime already includes queueTime
    let (anim_time, _queue_time) =
        creature_combat::set_combat_mode_with(w, this, new_combat_mode, force_hand_combat, false);
    //log.Info($"{Name}.HandleActionChangeCombatMode_Inner({newCombatMode}) - animTime: {animTime}, queueTime: {queueTime}");

    let t = w.now.utc.add_seconds(f64::from(anim_time));
    set_next_use_time(w, this, t);

    if magic_state_is_casting(w, this)
        && crate::world_objects::player_magic::fields(w, this)
            .record_cast
            .enabled
    {
        crate::entity::record_cast::on_set_combat_mode(w, this, new_combat_mode);
    }

    if let Some(callback) = callback {
        let mut callback_chain = ActionChain::new();
        callback_chain.add_delay_seconds(w, f64::from(anim_time));
        callback_chain.add_action(Actor::Object(this), callback);
        callback_chain.enqueue_chain(w);
    }
}

/// A player can damage an attackable creature that is not teleporting and not a combat pet.
// ACE: Player.CanDamage
#[must_use]
pub fn can_damage(w: &World, _this: ObjectGuid, target: ObjectGuid) -> bool {
    let t = object(w, target);
    t.attackable() && !t.wo.world_object.teleporting && !t.is_combat_pet()
}

// http://acpedia.org/wiki/Announcements_-_2002/04_-_Betrayal

// Some combination of strength and endurance (the two are roughly of equivalent importance) now allows one to have a level of "natural resistances" to the 7 damage types,
// and to partially resist drain health and harm attacks.

// This caps out at a 50% resistance (the equivalent to level 5 life prots) to these damage types.

// This resistance is not additive to life protections: higher level life protections will overwrite these natural resistances,
// although life vulns will take these natural resistances into account, if the player does not have a higher level life protection cast upon them.

// For example, a player will not get a free protective bonus from natural resistances if they have both Prot 7 and Vuln 7 cast upon them.
// The Prot and Vuln will cancel each other out, and since the Prot has overwritten the natural resistances, there will be no resistance bonus.

// The natural resistances, drain resistances, and regeneration rate info are now visible on the Character Information Panel, in what was once the Burden panel.

// The 5 categories for the endurance benefits are, in order from lowest benefit to highest: Poor, Mediocre, Hardy, Resilient, and Indomitable,
// with each range of benefits divided up equally amongst the 5 (e.g. Poor describes having anywhere from 1-10% resistance against drain health attacks, etc.).

// A few other important notes:

// - The abilities that Endurance or Endurance/Strength conveys are not increased by Strength or Endurance buffs.
//   It is the raw Strength and/or Endurance scores that determine the various bonuses.
// - For April, natural resistances will offer some protection versus hollow type damage, whether it is from a Hollow Minion or a Hollow weapon. This will be changed in May.
// - These abilities are player-only, creatures with high endurance will not benefit from any of these changes.
// - Come May, you can type @help endurance for a summary of the April changes to Endurance.

/// The player's natural resistance to a damage type from base Strength + Endurance (up to 50%);
/// nether always halves.
// ACE: Player.GetNaturalResistance
#[must_use]
pub fn get_natural_resistance(w: &World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    if damage_type == DamageType::Undef {
        return 1.0;
    }

    // http://acpedia.org/wiki/Announcements_-_11th_Anniversary_Preview#Void_Magic_and_You.21
    // Creatures under Asheron's protection take half damage from any nether type spell.
    if damage_type == DamageType::Nether {
        return 0.5;
    }

    // base strength and endurance give the player a natural resistance to damage,
    // which caps at 50% (equivalent to level 5 life prots)
    // these do not stack with life protection spells

    // - natural resistances are ignored by hollow damage

    let str_and_end = attribute_base(w, this, PropertyAttribute::Strength)
        .wrapping_add(attribute_base(w, this, PropertyAttribute::Endurance));

    if str_and_end <= 200 {
        return 1.0;
    }

    let mut natural_resistance = 1.0f32 - (str_and_end - 200) as f32 / 300.0f32 * 0.5f32;
    natural_resistance = math::max_f32(natural_resistance, 0.5);

    natural_resistance
}

/// The Character Information Panel's natural resistance category.
// ACE: Player.GetNaturalResistanceString
#[must_use]
pub fn get_natural_resistance_string(
    w: &World,
    this: ObjectGuid,
    _resistance_type: ResistanceType,
) -> &'static str {
    let str_and_end = attribute_base(w, this, PropertyAttribute::Strength)
        .wrapping_add(attribute_base(w, this, PropertyAttribute::Endurance));

    if str_and_end > 440 {
        "Indomitable"
    } else if str_and_end > 380 {
        "Resilient"
    } else if str_and_end > 320 {
        "Hardy"
    } else if str_and_end > 260 {
        "Mediocre"
    } else if str_and_end > 200 {
        "Poor"
    } else {
        "None"
    }
}

/// The Character Information Panel's regeneration category (Strength + 2 x Endurance).
// ACE: Player.GetRegenBonusString
#[must_use]
pub fn get_regen_bonus_string(w: &World, this: ObjectGuid) -> &'static str {
    let str_and_end = attribute_base(w, this, PropertyAttribute::Strength)
        .wrapping_add(2u32.wrapping_mul(attribute_base(w, this, PropertyAttribute::Endurance)));

    if str_and_end > 690 {
        "Indomitable"
    } else if str_and_end > 580 {
        "Resilient"
    } else if str_and_end > 470 {
        "Hardy"
    } else if str_and_end > 346 {
        "Mediocre"
    } else if str_and_end > 200 {
        "Poor"
    } else {
        "None"
    }
}

/// If a player has been involved in a PK battle this recently, logging off leaves their character
/// in a frozen state for 20 seconds.
// ACE: Player.PKLogoffTimer
#[must_use]
pub fn pk_logoff_timer() -> TimeSpan {
    TimeSpan::from_minutes(2.0)
}

// ACE: Player.UpdatePKTimer
pub fn update_pk_timer(w: &mut World, this: ObjectGuid) {
    //log.Info($"Updating PK timer for {Name}");

    let now = unix_time(w);
    object_mut(w, this).set_last_pk_attack_timestamp(now);
}

/// Called when a successful attack is landed in PVP: both PKs' timestamps are updated. An evaded
/// physical attack or a resisted spell does not call this.
// ACE: Player.UpdatePKTimers
pub fn update_pk_timers(w: &mut World, attacker: ObjectGuid, defender: ObjectGuid) {
    if attacker == defender {
        return;
    }

    if object(w, attacker).player_killer_status() == PlayerKillerStatus::Free
        || object(w, defender).player_killer_status() == PlayerKillerStatus::Free
    {
        return;
    }

    update_pk_timer(w, attacker);
    update_pk_timer(w, defender);
}

// ACE: Player.PKTimerActive
#[must_use]
pub fn pk_timer_active(w: &World, this: ObjectGuid) -> bool {
    is_pk_type(w, this)
        && unix_time(w) - object(w, this).last_pk_attack_timestamp()
            < property_manager::get_long(w, "pk_timer", 0, true).item as f64
}

// ACE: Player.PKLogoutActive
#[must_use]
pub fn pk_logout_active(w: &World, this: ObjectGuid) -> bool {
    is_pk_type(w, this)
        && unix_time(w) - object(w, this).last_pk_attack_timestamp()
            < pk_logoff_timer().total_seconds()
}

// ACE: Player.IsPKType
#[must_use]
pub fn is_pk_type(w: &World, this: ObjectGuid) -> bool {
    let s = object(w, this).player_killer_status();
    s == PlayerKillerStatus::PK || s == PlayerKillerStatus::PKLite
}

// ACE: Player.IsPK
#[must_use]
pub fn is_pk(w: &World, this: ObjectGuid) -> bool {
    object(w, this).player_killer_status() == PlayerKillerStatus::PK
}

// ACE: Player.IsPKL
#[must_use]
pub fn is_pkl(w: &World, this: ObjectGuid) -> bool {
    object(w, this).player_killer_status() == PlayerKillerStatus::PKLite
}

// ACE: Player.IsNPK
#[must_use]
pub fn is_npk(w: &World, this: ObjectGuid) -> bool {
    object(w, this).player_killer_status() == PlayerKillerStatus::NPK
}

/// `(CurrentLandblock?.IsDungeon ?? false) ? Location.Cell : Location.GetOutdoorCell()`.
fn house_restriction_cell(w: &mut World, g: ObjectGuid) -> u32 {
    let location = object(w, g)
        .location()
        .expect("System.NullReferenceException: Location");
    let is_dungeon = match object(w, g).current_landblock {
        Some(id) => w
            .landblock_manager
            .landblocks
            .get_mut(id)
            .is_some_and(|l| l.is_dungeon()),
        None => false,
    };
    if is_dungeon {
        location.cell()
    } else {
        crate::entity::position_extensions::get_outdoor_cell(&location)
    }
}

/// Whether PvP between this player and `player` is allowed across house boundaries: both must
/// have permission in every owned, closed house either stands in.
// ACE: Player.CheckHouseRestrictions
pub fn check_house_restrictions(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let this_cell = object(w, this)
        .location()
        .expect("System.NullReferenceException: Location")
        .cell();
    let player_cell = object(w, player)
        .location()
        .expect("System.NullReferenceException: Location")
        .cell();
    if this_cell == player_cell {
        return true;
    }

    // dealing with outdoor cell equivalents at this point, if applicable
    let cell = house_restriction_cell(w, this);
    let player_cell = house_restriction_cell(w, player);

    if cell == player_cell {
        return true;
    }

    let house_guid = empyrean_tables::house_cell::HOUSE_CELLS
        .get(&cell)
        .copied()
        .unwrap_or(0);
    let player_house_guid = empyrean_tables::house_cell::HOUSE_CELLS
        .get(&player_cell)
        .copied()
        .unwrap_or(0);

    // pass if both of these players aren't in a house cell
    if house_guid == 0 && player_house_guid == 0 {
        return true;
    }

    let mut houses: Vec<ObjectGuid> = Vec::new();
    check_house_restrictions_get_house(w, this, house_guid, &mut houses);
    check_house_restrictions_get_house(w, player, player_house_guid, &mut houses);

    for house in houses {
        if !house_has_permission(w, house, this) || !house_has_permission(w, house, player) {
            return false;
        }
    }
    true
}

/// Adds the root house of `house_guid` (in this player's landblock) to `houses` when it is
/// owned, closed and not already listed.
// ACE: Player.CheckHouseRestrictions_GetHouse
pub fn check_house_restrictions_get_house(
    w: &mut World,
    this: ObjectGuid,
    house_guid: u32,
    houses: &mut Vec<ObjectGuid>,
) {
    if house_guid == 0 {
        return;
    }

    let current_landblock = object(w, this)
        .current_landblock
        .expect("System.NullReferenceException: CurrentLandblock");
    let house = landblock::get_object(w, current_landblock, ObjectGuid::new(house_guid), true)
        .filter(|&g| object(w, g).is_house());
    if let Some(house) = house {
        // `var rootHouse = house.LinkedHouses.Count > 0 ? house.LinkedHouses[0] : house;` and the
        // owner / open-status test, both in [`house_root_owned_and_closed`].
        let Some(root_house) = house_root_owned_and_closed(w, house) else {
            return;
        };
        if houses.contains(&root_house) {
            return;
        }

        //Console.WriteLine($"{Name}.CheckHouseRestrictions_GetHouse({houseGuid:X8}): found root house {house.Name} ({house.HouseId})");
        houses.push(root_house);
    } else {
        log::error!(
            "{}.CheckHouseRestrictions_GetHouse({house_guid:08X}): couldn't find house from {:08X}",
            shim::name(w, this),
            current_landblock.raw()
        );
    }
}

/// Returns the damage type for the currently equipped weapon / ammo (`GetDamageType(bool
/// multiple = false, CombatType? combatType = null)`, the player override). With `multiple`, all
/// of the weapon's damage types.
// ACE: Player.GetDamageType
#[must_use]
pub fn get_damage_type(
    w: &World,
    this: ObjectGuid,
    multiple: bool,
    combat_type: Option<CombatType>,
) -> DamageType {
    // player override
    let combat_type = combat_type.unwrap_or_else(|| get_combat_type(w, this));

    let mut weapon = creature_equipment::get_equipped_weapon(w, this, false);
    let ammo = creature_equipment::get_equipped_ammo(w, this);

    if weapon.is_none() && combat_type == CombatType::Melee {
        // handle gauntlets/ boots
        let attack_type = creature_combat::attack_type(w, this);
        if attack_type == AttackType::Punch {
            weapon = hand_armor(w, this);
        } else if attack_type == AttackType::Kick {
            weapon = foot_armor(w, this);
        } else {
            log::warn!(
                "{}.GetDamageType(): no weapon, AttackType={}",
                shim::name(w, this),
                attack_type.to_dotnet_string()
            );
            return DamageType::Undef;
        }

        if weapon.is_some_and(|g| object(w, g).w_damage_type() == DamageType::Undef) {
            return DamageType::Bludgeon;
        }
    }

    let Some(weapon) = weapon else {
        return DamageType::Bludgeon;
    };

    let damage_source = match ammo {
        Some(ammo) if combat_type != CombatType::Melee && object(w, weapon).is_ammo_launcher() => {
            ammo
        }
        _ => weapon,
    };

    let damage_type = object(w, damage_source).w_damage_type();

    if damage_type == DamageType::Undef {
        log::warn!(
            "{}.GetDamageType(): {} ({}, {}): no DamageType",
            shim::name(w, this),
            shim::name(w, damage_source),
            damage_source,
            object(w, damage_source).biota.weenie_class_id
        );
        return DamageType::Bludgeon;
    }

    // return multiple damage types
    if multiple || !damage_type.is_multi_damage() {
        return damage_type;
    }

    // get single damage type
    if damage_type == (DamageType::Pierce | DamageType::Slash) {
        let attack_type = creature_combat::attack_type(w, this);
        if (attack_type & AttackType::Punches) != AttackType::Undef {
            return if player_melee::power_level(w, this) < weapon_mod::THRUST_THRESHOLD {
                DamageType::Pierce
            } else {
                DamageType::Slash
            };
        }

        return if (attack_type & AttackType::Thrusts) != AttackType::Undef {
            DamageType::Pierce
        } else {
            DamageType::Slash
        };
    }

    let power_level = if combat_type == CombatType::Melee {
        Some(player_melee::power_level(w, this))
    } else {
        None
    };

    empyrean_entity::enums::DamageType::select_damage_type(damage_type, power_level)
}

/// The first equipped object covering the hands.
// ACE: Player.HandArmor
#[must_use]
pub fn hand_armor(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .find(|&i| {
            (object(w, i)
                .clothing_priority()
                .unwrap_or(CoverageMask(0))
                .0
                & CoverageMask::Hands.0)
                > 0
        })
}

/// The first equipped object covering the feet.
// ACE: Player.FootArmor
#[must_use]
pub fn foot_armor(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .find(|&i| {
            (object(w, i)
                .clothing_priority()
                .unwrap_or(CoverageMask(0))
                .0
                & CoverageMask::Feet.0)
                > 0
        })
}

/// Determines if the player can damage (or cast `spell` on) a target by PlayerKillerStatus:
/// `None` if there is no error, else the pair of errors for the player and the target.
// ACE: Player.CheckPKStatusVsTarget
pub fn check_pk_status_vs_target(
    w: &mut World,
    this: ObjectGuid,
    target: Option<ObjectGuid>,
    spell: Option<&Spell>,
) -> Option<Vec<WeenieErrorWithString>> {
    let target = target.filter(|&t| t != this)?;

    let mut target_creature = is_creature(w, target).then_some(target);
    if target_creature.is_none() {
        if let Some(wielder_id) = object(w, target).wielder_id() {
            // handle casting item spells
            let current_landblock = object(w, this)
                .current_landblock
                .expect("System.NullReferenceException: CurrentLandblock");
            target_creature =
                landblock::get_object(w, current_landblock, ObjectGuid::new(wielder_id), true)
                    .filter(|&g| is_creature(w, g));
        }
    }
    let target_creature = target_creature?;

    let own_status = object(w, this).player_killer_status();
    let target_creature_status = object(w, target_creature).player_killer_status();

    if own_status == PlayerKillerStatus::Free || target_creature_status == PlayerKillerStatus::Free
    {
        return None;
    }

    if is_player(w, target) {
        let target_player = target;
        let target_player_status = object(w, target_player).player_killer_status();

        if spell.is_none_or(Spell::is_harmful) {
            // Ensure that a non-PK cannot cast harmful spells on another player
            if own_status == PlayerKillerStatus::NPK {
                return Some(vec![
                    WeenieErrorWithString::YouFailToAffect_YouAreNotPK,
                    WeenieErrorWithString::_FailsToAffectYou_TheyAreNotPK,
                ]);
            }

            if target_player_status == PlayerKillerStatus::NPK {
                return Some(vec![
                    WeenieErrorWithString::YouFailToAffect_TheyAreNotPK,
                    WeenieErrorWithString::_FailsToAffectYou_YouAreNotPK,
                ]);
            }

            // Ensure not attacking across housing boundary
            if !check_house_restrictions(w, this, target_player) {
                return Some(vec![
                    WeenieErrorWithString::YouFailToAffect_AcrossHouseBoundary,
                    WeenieErrorWithString::_FailsToAffectYouAcrossHouseBoundary,
                ]);
            }
        }

        // additional checks for different PKTypes
        if own_status != target_player_status {
            // require same pk status, unless beneficial spell being cast on NPK
            // https://asheron.fandom.com/wiki/Player_Killer
            // https://asheron.fandom.com/wiki/Player_Killer_Lite

            if spell.is_none_or(Spell::is_harmful)
                || target_player_status != PlayerKillerStatus::NPK
            {
                return Some(vec![
                    WeenieErrorWithString::YouFailToAffect_NotSamePKType,
                    WeenieErrorWithString::_FailsToAffectYou_NotSamePKType,
                ]);
            }
        }
    } else {
        // if monster has a non-default pk status, ensure pk types match up
        if target_creature_status != PlayerKillerStatus::NPK && own_status != target_creature_status
        {
            return Some(vec![
                WeenieErrorWithString::YouFailToAffect_NotSamePKType,
                WeenieErrorWithString::_FailsToAffectYou_NotSamePKType,
            ]);
        }
    }
    None
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Player.GetCurrentAttackSkill
pub fn player_get_current_attack_skill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::Skill {
    get_current_attack_skill(w, this)
}

// ACE: Player.GetCurrentWeaponSkill
pub fn player_get_current_weapon_skill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::Skill {
    get_current_weapon_skill(w, this)
}

// ACE: Player.GetCombatType
pub fn player_get_combat_type(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> crate::world_objects::creature_combat::CombatType {
    get_combat_type(w, this)
}

// ACE: Player.OnDamageTarget
pub fn player_on_damage_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    attack_type: crate::world_objects::creature_combat::CombatType,
    critical: bool,
) {
    on_damage_target(w, this, target, attack_type, critical);
}

// ACE: Player.GetEffectiveAttackSkill
pub fn player_get_effective_attack_skill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> u32 {
    get_effective_attack_skill(w, this)
}

// ACE: Player.OnEvade
pub fn player_on_evade(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    attacker: empyrean_entity::ObjectGuid,
    attack_type: crate::world_objects::creature_combat::CombatType,
) {
    on_evade(w, this, attacker, attack_type);
}

// ACE: Player.GetPowerMod
pub fn player_get_power_mod(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    weapon: empyrean_entity::ObjectGuid,
) -> f32 {
    get_power_mod(w, this, Some(weapon))
}

// ACE: Player.GetAccuracyMod
pub fn player_get_accuracy_mod(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    weapon: empyrean_entity::ObjectGuid,
) -> f32 {
    get_accuracy_mod(w, this, Some(weapon))
}

// ACE: Player.TakeDamageOverTime
pub fn player_take_damage_over_time(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    amount: f32,
    damage_type: empyrean_entity::enums::DamageType,
) {
    take_damage_over_time(w, this, amount, damage_type);
}

// ACE: Player.CanDamage
pub fn player_can_damage(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) -> bool {
    can_damage(w, this, target)
}

// ACE: Player.GetNaturalResistance
pub fn player_get_natural_resistance(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    damage_type: empyrean_entity::enums::DamageType,
) -> f32 {
    get_natural_resistance(w, this, damage_type)
}

// ACE: Player.GetDamageType
pub fn player_get_damage_type(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    multiple: bool,
    combat_type: Option<crate::world_objects::creature_combat::CombatType>,
) -> empyrean_entity::enums::DamageType {
    get_damage_type(w, this, multiple, combat_type)
}

// ACE: Player.CheckPKStatusVsTarget
/// The generated signature carries `()` for the `Spell`, so the call is ACE's `null` (a physical
/// attack): see [`check_pk_status_vs_target`].
pub fn player_check_pk_status_vs_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    spell: (),
) -> Option<Vec<empyrean_entity::enums::WeenieErrorWithString>> {
    let () = spell;
    check_pk_status_vs_target(w, this, Some(target), None)
}
