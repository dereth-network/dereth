// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Healer.cs
//! Port of `Source/ACE.Server/WorldObjects/Healer.cs`.
//!
//! A healing kit used on a player (`UseWithTarget`): the refusals, the heal motion, then (unless
//! the healer moved 5 m or more, PKs only) the skill check and the heal, each drawing
//! `ThreadSafeRandom` in ACE's order: the skill check, the heal amount, the critical.
//!
//! The arithmetic of `DoSkillCheck` and `GetHealAmount` is split into [`skill_check_values`] and
//! [`heal_amount`] (the same casts and draws) so the ACE vectors can drive it.

use dereth_primitives::Position as PPosition;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    ChatMessageType, CombatMode, MotionCommand, PlayerKillerStatus, PropertyAttribute2nd,
    PropertyInt, Skill, SkillAdvancementClass, WeenieError, WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::damage_history;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::physics::{motion_table, phys_ext};
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::entity::creature_vital::CreatureVital;
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::world_objects::{
    creature_combat, creature_vitals, food, monster_combat, player, player_inventory,
    player_location, player_networking, player_use, skill_check, world_object,
    world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Healer.cs`.
#[derive(Debug, Default)]
pub struct HealerFields {}

// ACE: Healer.Healing_MaxMove
pub const HEALING_MAX_MOVE: f32 = 5.0;

// ================================================================================ helpers

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

fn session_send(w: &mut World, player: ObjectGuid, msgs: Vec<GameMessage>) {
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    enqueue_send_many(w, session, msgs);
}

fn is_teleporting(w: &World, g: ObjectGuid) -> bool {
    obj(w, g).wo.world_object.teleporting
}

fn suicide_in_progress(w: &World, g: ObjectGuid) -> bool {
    obj(w, g)
        .player
        .as_ref()
        .is_some_and(|p| p.player_death.suicide_in_progress)
}

/// `target.GetCreatureVital(BoosterEnum)`; ACE dereferences the null an unexpected vital gives.
fn booster_vital(w: &World, this: ObjectGuid, target: ObjectGuid) -> CreatureVital {
    let booster_enum = obj(w, this).booster_enum();
    obj(w, target)
        .get_creature_vital(booster_enum)
        .expect("System.NullReferenceException: GetCreatureVital(BoosterEnum)")
}

/// The physics body's position (`PhysicsObj.Position`).
fn physics_position(w: &World, this: ObjectGuid) -> PPosition {
    let h = obj(w, this)
        .phys
        .expect("System.NullReferenceException: PhysicsObj");
    phys_ext::position(w, h).expect("System.NullReferenceException: PhysicsObj")
}

/// `Physics.Common.Position.Distance(pos)`: the length of the global offset.
fn physics_distance(a: &PPosition, b: &PPosition) -> f32 {
    let v = dereth_physics::math::get_offset(a, b);
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

/// `GetCreatureSkill(skill)`: `Current` and the advancement class.
fn creature_skill(
    w: &mut World,
    creature: ObjectGuid,
    skill: Skill,
) -> (u32, SkillAdvancementClass) {
    let s = obj_mut(w, creature)
        .get_creature_skill(skill, true)
        .expect("GetCreatureSkill adds a missing skill");
    let advancement_class = s.advancement_class(obj(w, creature));
    (s.current(w, creature), advancement_class)
}

// ================================================================================ Healer.cs

/// `UsesLeft`: `Structure` (TODO: change structure / maxstructure to int, cast to ushort at
/// network level).
// ACE: Healer.UsesLeft
#[must_use]
pub fn uses_left(o: &WorldObject) -> Option<u16> {
    o.structure()
}

// ACE: Healer.UsesLeft
pub fn set_uses_left(o: &mut WorldObject, value: Option<u16>) {
    o.set_structure(value);
}

/// The refusals (untrained, busy, not a player, in the air, another PK type, the vital full),
/// then the heal motion. (The MoveTo is handled in the base `Player_Use`.)
// ACE: Healer.HandleActionUseOnTarget
pub fn healer_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    let healer = player;

    let (_, advancement_class) = creature_skill(w, healer, Skill::Healing);
    if advancement_class.0 < SkillAdvancementClass::Trained.0 {
        player_use::send_use_done_event(w, healer, WeenieError::YouArentTrainedInHealing);
        return;
    }

    if food::player_is_busy_teleporting_or_suiciding(w, healer) {
        player_use::send_use_done_event(w, healer, WeenieError::YoureTooBusy);
        return;
    }

    if !w.objects.get(target).is_some_and(WorldObject::is_player) || is_teleporting(w, target) {
        player_use::send_use_done_event(w, healer, WeenieError::YouCantHealThat);
        return;
    }
    let target_player = target;

    if player::is_jumping(w, healer) {
        player_use::send_use_done_event(w, healer, WeenieError::YouCantDoThatWhileInTheAir);
        return;
    }

    // ensure same PKType, although PK and PKLite players can heal NPKs:
    // https://asheron.fandom.com/wiki/Player_Killer
    // https://asheron.fandom.com/wiki/Player_Killer_Lite

    let target_status = obj(w, target_player).player_killer_status();
    if target_status != obj(w, healer).player_killer_status()
        && target_status != PlayerKillerStatus::NPK
    {
        let target_name = name(w, target_player);
        player_networking::send_weenie_error_with_string(
            w,
            healer,
            WeenieErrorWithString::YouFailToAffect_NotSamePKType,
            &target_name,
        );
        player_use::send_use_done_event(w, healer, WeenieError::None);
        return;
    }

    // ensure target player vital < MaxValue
    let vital = booster_vital(w, this, target_player);

    let current = vital.current(obj(w, target_player));
    let max_value = vital.max_value(&mut StatCtx::in_world(w, target_player));
    if current == max_value {
        let target_name = name(w, target);
        let session =
            player_session(w, healer).expect("System.NullReferenceException: Player.Session");
        let data = w
            .sessions
            .get_mut(session)
            .expect("the session's game half");
        let message = match vital.vital {
            PropertyAttribute2nd::MaxHealth => Some(game_event_weenie_error_with_string(
                data,
                WeenieErrorWithString::_IsAtFullHealth,
                &target_name,
            )),
            PropertyAttribute2nd::MaxStamina => Some(game_event_communication_transient_string(
                data,
                &format!("{target_name} is already at full stamina!"),
            )),
            PropertyAttribute2nd::MaxMana => Some(game_event_communication_transient_string(
                data,
                &format!("{target_name} is already at full mana!"),
            )),
            _ => None,
        };
        if let Some(message) = message {
            enqueue_send(w, session, message);
        }
        player_use::send_use_done_event(w, healer, WeenieError::None);
        return;
    }

    /*if (!healer.Equals(targetPlayer))
    {
        // perform moveto
        healer.CreateMoveToChain(target, (success) => DoHealMotion(healer, targetPlayer, success));
    }
    else
        DoHealMotion(healer, targetPlayer, true);*/

    // MoveTo is now handled in base Player_Use
    do_heal_motion(w, this, healer, target_player, true);
}

/// The heal motion (self or other); at its end, unless the healer moved `Healing_MaxMove` or
/// more (NPKs are exempt), `DoHealing` with the vital missing when it started; then `UseDone`.
// ACE: Healer.DoHealMotion
pub fn do_heal_motion(
    w: &mut World,
    this: ObjectGuid,
    healer: ObjectGuid,
    target: ObjectGuid,
    success: bool,
) {
    if !success
        || monster_combat::is_dead(obj(w, target))
        || is_teleporting(w, target)
        || suicide_in_progress(w, target)
    {
        player_use::send_use_done_event(w, healer, WeenieError::None);
        return;
    }

    obj_mut(w, healer).wo.world_object.is_busy = true;

    let motion_command = if healer == target {
        MotionCommand::SkillHealSelf
    } else {
        MotionCommand::SkillHealOther
    };

    // `var motion = new Motion(healer, motionCommand);` (only the commented-out broadcast uses it)
    let current_stance = obj(w, healer)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .expect("System.NullReferenceException: CurrentMotionState")
        .stance;
    let motion_table_id = obj(w, healer).motion_table_id();
    let anim_length =
        motion_table::get_animation_length(w, motion_table_id, current_stance, motion_command, 1.0);

    let start_pos = physics_position(w, healer);

    let vital = booster_vital(w, this, target);

    let missing_vital = vital.missing(&mut StatCtx::in_world(w, target));

    let mut action_chain = ActionChain::new();
    //actionChain.AddAction(healer, () => healer.EnqueueBroadcastMotion(motion));
    action_chain.add_action(Actor::Object(healer), move |w: &mut World| {
        player_location::send_motion_as_commands(w, healer, motion_command, current_stance)
    });
    action_chain.add_delay_seconds(w, f64::from(anim_length));
    action_chain.add_action(Actor::Object(healer), move |w: &mut World| {
        // check healing move distance cap
        let end_pos = physics_position(w, healer);
        let dist = physics_distance(&start_pos, &end_pos);

        //Console.WriteLine($"Dist: {dist}");

        // only PKs affected by these caps?
        if dist < HEALING_MAX_MOVE
            || obj(w, healer).player_killer_status() == PlayerKillerStatus::NPK
        {
            do_healing(w, this, healer, target, missing_vital);
        } else {
            let m = game_message_system_chat(
                "Your movement disrupted healing!",
                ChatMessageType::Broadcast,
            );
            session_send(w, healer, vec![m]);
        }

        obj_mut(w, healer).wo.world_object.is_busy = false;

        player_use::send_use_done_event(w, healer, WeenieError::None);
    });

    world_object_networking::enqueue_motion(
        w,
        healer,
        &mut action_chain,
        MotionCommand::Ready,
        1.0,
        true,
        None,
        false,
        false,
    );

    action_chain.enqueue_chain(w);

    let next_use_time = w.now.utc + TimeSpan::from_seconds(f64::from(anim_length));
    player_use::fields_mut(w, healer).next_use_time = next_use_time;
}

/// One use of the kit, the skill check, then the heal (and its stamina cost) with the messages
/// to both players; the last use consumes the kit.
///
/// # Panics
/// When the kit has no `Structure` (ACE: `InvalidOperationException` on `UsesLeft.Value`).
// ACE: Healer.DoHealing
// ACE-BUG: `UsesLeft.Value` is read for the Structure update message even for an UnlimitedUse kit, so a kit with no Structure throws InvalidOperationException before healing; latent: every one of ACE's 25 healer weenies has a Structure.
pub fn do_healing(
    w: &mut World,
    this: ObjectGuid,
    healer: ObjectGuid,
    target: ObjectGuid,
    missing_vital: u32,
) {
    if monster_combat::is_dead(obj(w, target)) || is_teleporting(w, target) {
        return;
    }

    let mut remaining_msg = String::new();

    if !obj(w, this).unlimited_use() {
        let structure_unit_value = CtorEnv::with_world(w, |env| {
            world_object::structure_unit_value(env, obj(w, this))
        });
        let kit_name = name(w, this);
        let o = obj_mut(w, this);
        // `UsesLeft--` on a ushort? (null stays null)
        let uses_left = uses_left(o).map(|u| u.wrapping_sub(1));
        set_uses_left(o, uses_left);
        let s = if uses_left == Some(1) { "" } else { "s" };
        remaining_msg = if uses_left.is_some_and(|u| u > 0) {
            format!(
                " Your {kit_name} has {} use{s} left.",
                uses_left.expect("checked")
            )
        } else {
            format!(" Your {kit_name} is used up.")
        };

        let value = o.value().map(|v| v.wrapping_sub(structure_unit_value));
        o.set_value(value);

        if o.value().is_some_and(|v| v < 0) {
            // fix negative value
            o.set_value(Some(0));
        }
    }

    let uses_left_value = uses_left(obj(w, this))
        .expect("InvalidOperationException: Nullable object must have a value.");
    let stack_size = game_message_public_update_property_int(
        obj_mut(w, this),
        PropertyInt::Structure,
        i32::from(uses_left_value),
    );
    let target_name = if healer == target {
        "yourself".to_owned()
    } else {
        name(w, target)
    };

    let vital = booster_vital(w, this, target);

    // skill check
    let mut difficulty = 0;
    let skill_check = do_skill_check(w, this, healer, target, missing_vital, &mut difficulty);
    let unlimited_use = obj(w, this).unlimited_use();
    if !skill_check {
        let fail_msg = game_message_system_chat(
            &format!("You fail to heal {target_name}.{remaining_msg}"),
            ChatMessageType::Broadcast,
        );
        session_send(w, healer, vec![fail_msg, stack_size]);
        if healer != target {
            let m = game_message_system_chat(
                &format!("{} fails to heal you.", name(w, healer)),
                ChatMessageType::Broadcast,
            );
            session_send(w, target, vec![m]);
        }
        if uses_left(obj(w, this)).is_some_and(|u| u == 0) && !unlimited_use {
            player_inventory::try_consume_from_inventory_with_networking(w, healer, this, 1);
        }
        return;
    }

    // heal up
    let (heal_amount, critical, stamina_cost) =
        get_heal_amount(w, this, healer, target, missing_vital);

    let stamina = obj(w, healer).stamina();
    creature_vitals::update_vital_delta(w, healer, stamina, (-i64::from(stamina_cost)).cs_cast());
    // Amount displayed to player can exceed actual amount healed due to heal boost ratings, but we only want to record the actual amount healed
    let actual_heal_amount: u32 =
        creature_vitals::update_vital_delta_uint(w, target, vital, heal_amount).cs_cast();
    if vital.vital == PropertyAttribute2nd::MaxHealth {
        damage_history::on_heal(w, target, actual_heal_amount);
    }

    //if (target.Fellowship != null)
    //target.Fellowship.OnVitalUpdate(target);

    proficiency_on_success_use(w, healer, Skill::Healing, difficulty);

    let booster_enum = obj(w, this).booster_enum().to_dotnet_string();
    let crit = if critical { "expertly " } else { "" };
    let message = game_message_system_chat(
        &format!(
            "You {crit}heal {target_name} for {heal_amount} {booster_enum} points.{remaining_msg}"
        ),
        ChatMessageType::Broadcast,
    );

    session_send(w, healer, vec![message, stack_size]);

    if healer != target {
        let m = game_message_system_chat(
            &format!(
                "{} heals you for {heal_amount} {booster_enum} points.",
                name(w, healer)
            ),
            ChatMessageType::Broadcast,
        );
        session_send(w, target, vec![m]);
    }

    if uses_left(obj(w, this)).is_some_and(|u| u == 0) && !unlimited_use {
        player_inventory::try_consume_from_inventory_with_networking(w, healer, this, 1);
    }
}

/// Determines if healer successfully heals target for attempt: (healing skill + healing kit
/// boost) * trainedMod vs. damage * 2 * combatMod. `difficulty` receives the difficulty.
// ACE: Healer.DoSkillCheck
pub fn do_skill_check(
    w: &mut World,
    this: ObjectGuid,
    healer: ObjectGuid,
    target: ObjectGuid,
    missing_vital: u32,
    difficulty: &mut i32,
) -> bool {
    let _ = target;
    let (current, advancement_class) = creature_skill(w, healer, Skill::Healing);
    let non_combat = creature_combat::combat_mode(w, healer) == CombatMode::NonCombat;
    let boost_value = obj(w, this).boost_value();

    skill_check_roll(
        current,
        advancement_class,
        boost_value,
        non_combat,
        missing_vital,
        difficulty,
    )
}

/// `DoSkillCheck`'s values: the effective skill and the difficulty.
#[must_use]
pub fn skill_check_values(
    current: u32,
    advancement_class: SkillAdvancementClass,
    boost_value: i32,
    non_combat: bool,
    missing_vital: u32,
) -> (i32, i32) {
    let trained_mod: f32 = if advancement_class == SkillAdvancementClass::Specialized {
        1.5
    } else {
        1.1
    };

    let combat_mod: f32 = if non_combat { 1.0 } else { 1.1 };

    // `(healingSkill.Current + BoostValue)` is a long (uint + int); times the float
    #[allow(clippy::cast_precision_loss)]
    let skill_sum = (i64::from(current) + i64::from(boost_value)) as f32;
    let effective_skill: i32 = math::round(f64::from(skill_sum * trained_mod)).cs_cast();
    // `missingVital * 2` is a uint product; times the float
    #[allow(clippy::cast_precision_loss)]
    let missing = missing_vital.wrapping_mul(2) as f32;
    let difficulty: i32 = math::round(f64::from(missing * combat_mod)).cs_cast();

    (effective_skill, difficulty)
}

/// `DoSkillCheck`'s roll: `GetSkillChance(effectiveSkill, difficulty) > Next(0, 1)` (one draw).
pub fn skill_check_roll(
    current: u32,
    advancement_class: SkillAdvancementClass,
    boost_value: i32,
    non_combat: bool,
    missing_vital: u32,
    difficulty: &mut i32,
) -> bool {
    let (effective_skill, d) = skill_check_values(
        current,
        advancement_class,
        boost_value,
        non_combat,
        missing_vital,
    );
    *difficulty = d;

    let skill_check =
        skill_check::get_skill_chance(effective_skill, d, skill_check::DEFAULT_FACTOR);
    skill_check > ThreadSafeRandom::next_float(0.0, 1.0)
}

/// Returns the healing amount for this attempt, whether it was a critical heal, and the stamina
/// it costs.
///
/// # Panics
/// When the kit has no `HealkitMod` (ACE: `InvalidOperationException`).
// ACE: Healer.GetHealAmount
pub fn get_heal_amount(
    w: &mut World,
    this: ObjectGuid,
    healer: ObjectGuid,
    target: ObjectGuid,
    missing_vital: u32,
) -> (u32, bool, u32) {
    // factors: healing skill, healing kit bonus, stamina, critical chance
    let (healing_skill, _) = creature_skill(w, healer, Skill::Healing);
    let healkit_mod = obj(w, this)
        .healkit_mod()
        .expect("InvalidOperationException: HealkitMod");
    let stamina_current = {
        let o = obj(w, healer);
        o.stamina().current(o)
    };

    // verify healing boost comes from target instead of healer?
    // sounds like target in LumAugHealingRating...
    let rating_mod_of_target = |w: &mut World| creature_get_healing_rating_mod(w, target);

    heal_amount(
        healing_skill,
        healkit_mod,
        missing_vital,
        stamina_current,
        || rating_mod_of_target(w),
    )
}

/// `GetHealAmount`'s arithmetic: two draws (`Next(healMin, healMax)`, then the critical
/// `Next(0, 1) < 0.1f`); `rating_mod` is read after them, as ACE reads the target's rating last.
pub fn heal_amount(
    healing_skill: u32,
    healkit_mod: f64,
    missing_vital: u32,
    stamina_current: u32,
    rating_mod: impl FnOnce() -> f32,
) -> (u32, bool, u32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // (float)HealkitMod.Value; uint * float
    let heal_base = healing_skill as f32 * (healkit_mod as f32);

    // todo: determine applicable range from pcaps
    let heal_min = heal_base * 0.2; // ??
    let heal_max = heal_base * 0.5;
    let mut heal_amount = ThreadSafeRandom::next_float(heal_min, heal_max);

    // chance for critical healing
    let critical_heal = ThreadSafeRandom::next_float(0.0, 1.0) < f64::from(0.1f32);
    if critical_heal {
        heal_amount *= 2.0;
    }

    // cap to missing vital
    if heal_amount > f64::from(missing_vital) {
        heal_amount = f64::from(missing_vital);
    }

    // stamina check? On the Q&A board a dev posted that stamina directly effects the amount of damage you can heal
    // low stam = less vital healed. I don't have exact numbers for it. Working through forum archive.

    // stamina cost: 1 stamina per 5 vital healed
    let mut stamina_cost: u32 = math::round(heal_amount / f64::from(5.0f32)).cs_cast();
    if stamina_cost > stamina_current {
        stamina_cost = stamina_current;
        heal_amount = f64::from(stamina_cost.wrapping_mul(5));
    }

    heal_amount *= f64::from(rating_mod());

    (
        math::round(heal_amount).cs_cast(),
        critical_heal,
        stamina_cost,
    )
}

// ================================================================================ pointers

/// `Creature.GetHealingRatingMod()` (`Creature_Rating.cs`).
pub(crate) fn creature_get_healing_rating_mod(w: &mut World, creature: ObjectGuid) -> f32 {
    crate::world_objects::creature_rating::get_healing_rating_mod(w, creature)
}

/// `Proficiency.OnSuccessUse(player, player.GetCreatureSkill(skill), difficulty)` (the `int`
/// overload).
fn proficiency_on_success_use(w: &mut World, player: ObjectGuid, skill: Skill, difficulty: i32) {
    let skill = crate::entity::proficiency::get_creature_skill(w, player, skill);
    crate::entity::proficiency::on_success_use_int(w, player, skill, difficulty);
}

// ---- constructors and SetEphemeralValues ----

/// `new Healer(weenie, guid)` / `new Healer(biota)`: the `WorldObject` constructor, then
/// Healer's `SetEphemeralValues`.
// ACE: Healer.Healer
pub fn healer_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    healer_set_ephemeral_values(o, env);
}

// ACE: Healer.SetEphemeralValues
fn healer_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Healer;
}
