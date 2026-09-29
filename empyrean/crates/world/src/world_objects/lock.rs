// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Lock.cs
//! Port of `Source/ACE.Server/WorldObjects/Lock.cs`.
//!
//! `UnlockResults`, the `Lock` interface (implemented by `Door` and `Chest`: [`unlock_key`] and
//! [`unlock_lockpick`] route to their `Unlock` overloads), `UnlockerHelper` (a key or lockpick
//! used on a lock: the skill check, the messages, the uses left) and `LockHelper` (the lock's
//! side: pickability, difficulty, lock codes, and the unlock itself).

use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    ChatMessageType, PropertyBool, PropertyFloat, PropertyInt, Skill, SkillAdvancementClass, Sound,
    WeenieError, WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::managers::{enchantment_manager, enchantment_manager_with_caching};
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::world_objects::{
    chest, door, player_inventory, player_use, skill_check, world_object, world_object_networking,
};
use crate::{dispatch, World};

// ACE: UnlockResults
/// The outcome of a key or lockpick used on a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum UnlockResults {
    UnlockSuccess = 0,
    PickLockFailed = 1,
    IncorrectKey = 2,
    AlreadyUnlocked = 3,
    CannotBePicked = 4,
    Open = 5,
}

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

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `wo is Lock`: a `Door`, or a `Chest` (`Storage` included).
#[must_use]
pub fn is_lock(o: &WorldObject) -> bool {
    o.is_door() || o.is_chest()
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`: equal after simple upper-casing of
/// each char (a char whose upper case is several chars compares as itself, as .NET's simple case
/// mapping does).
fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    fn upper(c: char) -> char {
        let mut u = c.to_uppercase();
        match (u.next(), u.next()) {
            (Some(one), None) => one,
            _ => c,
        }
    }
    a.chars().count() == b.chars().count()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| x == y || upper(x) == upper(y))
}

// ================================================================================ Lock (interface)

/// `@lock.Unlock(unlockerGuid, key, keyCode)`: `Door.Unlock` or `Chest.Unlock` (the key overload).
///
/// # Panics
/// When `target` is not a `Lock`.
pub fn lock_unlock_key(
    w: &mut World,
    target: ObjectGuid,
    unlocker_guid: u32,
    key: Option<ObjectGuid>,
    key_code: Option<&str>,
) -> UnlockResults {
    let o = obj(w, target);
    if o.is_door() {
        door::unlock_key(w, target, unlocker_guid, key, key_code)
    } else if o.is_chest() {
        chest::unlock_key(w, target, unlocker_guid, key, key_code)
    } else {
        panic!(
            "System.InvalidCastException: 0x{:08X} is not a Lock",
            target.full()
        )
    }
}

/// `@lock.Unlock(unlockerGuid, playerLockpickSkillLvl, ref difficulty)`: `Door.Unlock` or
/// `Chest.Unlock` (the lockpick overload).
///
/// # Panics
/// When `target` is not a `Lock`.
pub fn lock_unlock_lockpick(
    w: &mut World,
    target: ObjectGuid,
    unlocker_guid: u32,
    player_lockpick_skill_lvl: u32,
    difficulty: &mut i32,
) -> UnlockResults {
    let o = obj(w, target);
    if o.is_door() {
        door::unlock_lockpick(
            w,
            target,
            unlocker_guid,
            player_lockpick_skill_lvl,
            difficulty,
        )
    } else if o.is_chest() {
        chest::unlock_lockpick(
            w,
            target,
            unlocker_guid,
            player_lockpick_skill_lvl,
            difficulty,
        )
    } else {
        panic!(
            "System.InvalidCastException: 0x{:08X} is not a Lock",
            target.full()
        )
    }
}

// ================================================================================ UnlockerHelper

/// Tells the player how the key or lockpick did and how many uses are left, spends one use (the
/// last one consumes the item), and sends `UseDone`.
// ACE: UnlockerHelper.ConsumeUnlocker
pub fn consume_unlocker(
    w: &mut World,
    player: ObjectGuid,
    unlocker: ObjectGuid,
    target: ObjectGuid,
    success: bool,
) {
    // is Sonic Screwdriver supposed to be consumed on use?
    // it doesn't have a Structure, and it doesn't have PropertyBool.UnlimitedUse

    let u = obj(w, unlocker);
    let unlimited_uses =
        u.structure().is_none() || u.get_property(PropertyBool::UnlimitedUse).unwrap_or(false);
    let is_lockpick = u.biota.weenie_type == WeenieType::Lockpick;

    let mut msg;
    if is_lockpick {
        if success {
            msg = "You have successfully picked the lock!  It is now unlocked.\n ".to_owned();
        } else {
            msg = "You have failed to pick the lock.  It is still locked.  ".to_owned();
        }
    } else if success {
        msg = format!("The {} has been unlocked.\n", name(w, target));
    } else {
        msg = format!("The {} is still locked.\n", name(w, target));
    }

    if !unlimited_uses {
        msg += &format!("Your {} ", if is_lockpick { "lockpicks" } else { "key" });

        let structure_unit_value = CtorEnv::with_world(w, |env| {
            world_object::structure_unit_value(env, obj(w, unlocker))
        });

        let u = obj_mut(w, unlocker);
        // `Structure > 0` is false for null; `Structure--` on a ushort?
        match u.structure() {
            Some(s) if s > 0 => u.set_structure(Some(s - 1)),
            _ => u.set_structure(Some(0)),
        }

        let structure = u.structure();
        if structure == Some(0) {
            msg += if is_lockpick { "are" } else { "is" };
            msg += " used up.";
        } else {
            let structure = structure.expect("set above");
            msg += &format!(
                "{} {structure} use{} left.",
                if is_lockpick { "have" } else { "has" },
                if structure > 1 { "s" } else { "" }
            );
        }

        // `Value -= StructureUnitValue` (a null Value stays null)
        let value = u.value().map(|v| v.wrapping_sub(structure_unit_value));
        u.set_value(value);

        if u.value().is_some_and(|v| v < 0) {
            // fix negative value
            u.set_value(Some(0));
        }
    }

    let m = game_message_system_chat(&msg, ChatMessageType::Broadcast);
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, m);
    if !unlimited_uses {
        let structure = obj(w, unlocker).structure();
        if structure == Some(0) {
            if !player_inventory::try_consume_from_inventory_with_networking(w, player, unlocker, 1)
            {
                let u = obj(w, unlocker);
                log::warn!(
                    "UnlockerHelper.ConsumeUnlocker: TryConsumeFromInventoryWithNetworking failed for {} (0x{}:{}), used on {} (0x{}:{}) and used by {} (0x{})",
                    name(w, unlocker),
                    unlocker,
                    u.biota.weenie_class_id,
                    name(w, target),
                    target,
                    obj(w, target).biota.weenie_class_id,
                    name(w, player),
                    player
                );
            }
        } else {
            let structure = i32::from(structure.expect("set above"));
            let m = game_message_public_update_property_int(
                obj_mut(w, unlocker),
                PropertyInt::Structure,
                structure,
            );
            enqueue_send(w, session, m);
        }
    }
    player_use::send_use_done_event(w, player, WeenieError::None);
}

/// The lockpick skill with the lockpick's bonuses: `Round(skill * multiplier + additive)`, at
/// least 0. A multiplier above 1 is read as a percentage (`1 + mult * 0.01`).
// ACE: UnlockerHelper.GetEffectiveLockpickSkill
pub fn get_effective_lockpick_skill(
    w: &mut World,
    player: ObjectGuid,
    unlocker: ObjectGuid,
) -> u32 {
    let skill = obj_mut(w, player)
        .get_creature_skill(Skill::Lockpick, true)
        .expect("GetCreatureSkill adds a missing skill");
    let lockpick_skill = skill.current(w, player);

    let u = obj(w, unlocker);
    let additive_bonus = u.get_property(PropertyInt::LockpickMod).unwrap_or(0);
    let multiplicative_bonus = u
        .get_property(PropertyFloat::LockpickMod)
        .unwrap_or(f64::from(1.0f32));

    effective_lockpick_skill(lockpick_skill, additive_bonus, multiplicative_bonus)
}

/// The arithmetic of [`get_effective_lockpick_skill`]: `lockpick_skill` is the skill's `Current`,
/// `additive_bonus` `PropertyInt.LockpickMod ?? 0`, `multiplicative_bonus`
/// `PropertyFloat.LockpickMod ?? 1.0f` (a `double`).
#[must_use]
pub fn effective_lockpick_skill(
    lockpick_skill: u32,
    additive_bonus: i32,
    multiplicative_bonus: f64,
) -> u32 {
    let mut multiplicative_bonus = multiplicative_bonus;

    // is this really 10x bonus, or +10% bonus?
    if multiplicative_bonus > f64::from(1.0f32) {
        multiplicative_bonus = f64::from(1.0f32) + multiplicative_bonus * f64::from(0.01f32);
    }

    let effective_skill: i32 =
        math::round(f64::from(lockpick_skill) * multiplicative_bonus + f64::from(additive_bonus))
            .cs_cast();

    let effective_skill = effective_skill.max(0);

    //Console.WriteLine($"Base skill: {lockpickSkill}");
    //Console.WriteLine($"Effective skill: {effectiveSkill}");

    effective_skill.cs_cast()
}

/// A key or lockpick used on `target` (on the player's next action): refuses an untrained
/// lockpicker, keys on doors that take none and non-locks; otherwise unlocks through the lock and
/// answers each result with ACE's sound, message and `UseDone`.
// ACE: UnlockerHelper.UseUnlocker
pub fn use_unlocker(w: &mut World, player: ObjectGuid, unlocker: ObjectGuid, target: ObjectGuid) {
    let mut chain = ActionChain::new();

    chain.add_action(Actor::Object(player), move |w: &mut World| {
        use_unlocker_action(w, player, unlocker, target)
    });

    chain.enqueue_chain(w);
}

/// The body of [`use_unlocker`]'s action.
///
/// # Panics
/// When the player has no Lockpick record (ACE: `KeyNotFoundException` on `Skills[Skill.Lockpick]`).
fn use_unlocker_action(
    w: &mut World,
    player: ObjectGuid,
    unlocker: ObjectGuid,
    target: ObjectGuid,
) {
    let unlocker_is_lockpick = obj(w, unlocker).biota.weenie_type == WeenieType::Lockpick;
    if unlocker_is_lockpick {
        let p = obj(w, player);
        let skill = p
            .skills()
            .get(&Skill::Lockpick)
            .copied()
            .expect("KeyNotFoundException: Skills[Skill.Lockpick]");
        let advancement_class = skill.advancement_class(p);
        if advancement_class != SkillAdvancementClass::Trained
            && advancement_class != SkillAdvancementClass::Specialized
        {
            player_use::send_use_done_event(w, player, WeenieError::YouArentTrainedInLockpicking);
            return;
        }
    }
    if !is_lock(obj(w, target)) {
        player_use::send_use_done_event(w, player, WeenieError::YouCannotLockOrUnlockThat);
        return;
    }

    let mut result = UnlockResults::IncorrectKey;
    let mut difficulty = 0;
    if unlocker_is_lockpick {
        let effective_lockpick_skill = get_effective_lockpick_skill(w, player, unlocker);
        result = lock_unlock_lockpick(
            w,
            target,
            player.full(),
            effective_lockpick_skill,
            &mut difficulty,
        );
    } else if obj(w, unlocker).is_key() {
        let t = obj(w, target);
        if t.is_door() && t.lock_code().as_deref() == Some("") {
            // the door isn't to be opened with keys
            player_use::send_use_done_event(w, player, WeenieError::YouCannotLockOrUnlockThat);
            return;
        }
        result = lock_unlock_key(w, target, player.full(), Some(unlocker), None);
    }

    match result {
        UnlockResults::UnlockSuccess => {
            if unlocker_is_lockpick {
                // the source guid for this sound must be the player, else the sound will not play
                // which differs from PicklockFail and LockSuccess being in the target sound table
                let sound = game_message_sound(player, Sound::Lockpicking, 1.0);
                world_object_networking::enqueue_broadcast(w, player, true, &[sound]);

                proficiency_on_success_use(w, player, Skill::Lockpick, difficulty);
            }

            consume_unlocker(w, player, unlocker, target, true);
        }
        UnlockResults::Open => {
            player_use::send_use_done_event(w, player, WeenieError::YouCannotLockWhatIsOpen)
        }
        UnlockResults::AlreadyUnlocked => {
            player_use::send_use_done_event(w, player, WeenieError::LockAlreadyUnlocked)
        }
        UnlockResults::PickLockFailed => {
            let sound = game_message_sound(target, Sound::PicklockFail, 1.0);
            world_object_networking::enqueue_broadcast(w, target, true, &[sound]);
            consume_unlocker(w, player, unlocker, target, false);
        }
        UnlockResults::CannotBePicked => {
            player_use::send_use_done_event(w, player, WeenieError::YouCannotLockOrUnlockThat)
        }
        UnlockResults::IncorrectKey => {
            player_use::send_use_done_event(w, player, WeenieError::KeyDoesntFitThisLock)
        }
    }
}

// ================================================================================ LockHelper

/// Returns TRUE if wo is a lockable item that can be picked
// ACE: LockHelper.IsPickable
#[must_use]
pub fn is_pickable(o: &WorldObject) -> bool {
    if !is_lock(o) {
        return false;
    }

    let resist_lockpick = o.resist_lockpick();

    // TODO: find out if ResistLockpick >= 9999 is a special 'unpickable' value in acclient,
    // similar to ResistMagic >= 9999 being equivalent to Unenchantable?

    !resist_lockpick.is_none_or(|r| r >= 9999)
}

/// The lock's difficulty: its `ResistLockpick` plus the Strengthen/Weaken Lock enchantments, at
/// least 0; an unpickable lock's own value (no enchantments); null for a non-lock.
// ACE: LockHelper.GetResistLockpick
pub fn get_resist_lockpick(w: &mut World, wo: ObjectGuid) -> Option<i32> {
    let o = obj(w, wo);
    if !is_lock(o) {
        return None;
    }

    // if base ResistLockpick without enchantments is unpickable,
    // do not apply enchantments
    let is_pickable = is_pickable(o);

    if !is_pickable {
        return o.resist_lockpick();
    }

    let resist_lockpick = o
        .resist_lockpick()
        .expect("pickable: ResistLockpick has a value");
    let enchantment_mod = enchantment_manager_with_caching::get_resist_lockpick(w, wo);

    let difficulty = resist_lockpick.wrapping_add(enchantment_mod);

    // minimum 0 difficulty
    Some(difficulty.max(0))
}

/// [`get_resist_lockpick`] for callers that hold `&World` (appraisal): the same value, read through
/// the uncached base `EnchantmentManager` (the caching manager only memoises it).
#[must_use]
pub fn get_resist_lockpick_view(w: &World, wo: ObjectGuid) -> Option<i32> {
    let o = obj(w, wo);
    if !is_lock(o) {
        return None;
    }
    if !is_pickable(o) {
        return o.resist_lockpick();
    }
    let resist_lockpick = o
        .resist_lockpick()
        .expect("pickable: ResistLockpick has a value");
    Some(
        resist_lockpick
            .wrapping_add(enchantment_manager::get_resist_lockpick(w, wo))
            .max(0),
    )
}

/// A Door's or Chest's `LockCode`; null for anything else.
// ACE: LockHelper.GetLockCode
#[must_use]
pub fn get_lock_code(me: &WorldObject) -> Option<String> {
    let mut my_lock_code = None;
    if me.is_door() || me.is_chest() {
        my_lock_code = me.lock_code();
    }
    my_lock_code
}

/// Unlocks `target` with `key` (or `key_code`, which defaults to the key's `KeyCode`): the lock
/// code matches (ignoring case), or it is the Sonic Screwdriver's code, or the key opens any lock.
// ACE: LockHelper.Unlock
pub fn unlock_key(
    w: &mut World,
    target: ObjectGuid,
    key: Option<ObjectGuid>,
    key_code: Option<&str>,
) -> UnlockResults {
    let key_code: Option<String> = match key_code {
        Some(k) => Some(k.to_owned()),
        None => key.and_then(|k| obj(w, k).key_code()),
    };

    let Some(my_lock_code) = get_lock_code(obj(w, target)) else {
        return UnlockResults::IncorrectKey;
    };

    if obj(w, target).is_open() {
        return UnlockResults::Open;
    }

    // there is only 1 instance of an 'opens all' key in PY16 data, 'keysonicscrewdriver'
    // which uses keyCode '_bohemund's_magic_key_'

    // when LSD added the rare skeleton key (keyrarevolatileuniversal),
    // they used PropertyBool.OpensAnyLock, which appears to have been used for something else in retail on Writables:

    // https://github.com/ACEmulator/ACE-World-16PY/blob/master/Database/3-Core/9%20WeenieDefaults/SQL/Key/Key/09181%20Sonic%20Screwdriver.sql
    // https://github.com/ACEmulator/ACE-World-16PY/search?q=OpensAnyLock

    let code_fits = key_code.as_deref().is_some_and(|k| {
        equals_ordinal_ignore_case(k, &my_lock_code) || k == "_bohemund's_magic_key_"
    });
    let opens_any_lock = key.is_some_and(|k| obj(w, k).opens_any_lock());
    if code_fits || opens_any_lock {
        if !obj(w, target).is_locked() {
            return UnlockResults::AlreadyUnlocked;
        }

        let t = obj_mut(w, target);
        t.set_is_locked(false);
        let is_locked = t.is_locked();
        let update_property =
            game_message_public_update_property_bool(t, PropertyBool::Locked, is_locked);
        let sound = game_message_sound(target, Sound::LockSuccess, 1.0);
        world_object_networking::enqueue_broadcast(w, target, true, &[update_property, sound]);
        return UnlockResults::UnlockSuccess;
    }
    UnlockResults::IncorrectKey
}

/// Picks `target`'s lock with the effective skill `player_lockpick_skill_lvl`: one
/// `ThreadSafeRandom.Next(0, 1)` draw against `SkillCheck.GetSkillChance(skill, difficulty)`.
/// `difficulty` receives the lock's difficulty (`ref int`).
///
/// # Panics
/// When `target` is not in the world.
// ACE: LockHelper.Unlock
pub fn unlock_lockpick(
    w: &mut World,
    target: ObjectGuid,
    player_lockpick_skill_lvl: u32,
    difficulty: &mut i32,
) -> UnlockResults {
    let is_pickable = is_pickable(obj(w, target));

    if !is_pickable {
        return UnlockResults::CannotBePicked;
    }

    let my_resist_lockpick = get_resist_lockpick(w, target);

    *difficulty = my_resist_lockpick.expect("pickable: GetResistLockpick has a value");

    if obj(w, target).is_open() {
        return UnlockResults::Open;
    }

    if !obj(w, target).is_locked() {
        return UnlockResults::AlreadyUnlocked;
    }

    let pick_chance = skill_check::get_skill_chance(
        player_lockpick_skill_lvl.cs_cast(),
        *difficulty,
        skill_check::DEFAULT_FACTOR,
    );

    //#if DEBUG Debug.WriteLine($"{pickChance.FormatChance()} chance of UnlockSuccess");

    let dice = ThreadSafeRandom::next_float(0.0, 1.0);
    if dice >= pick_chance {
        return UnlockResults::PickLockFailed;
    }

    let t = obj_mut(w, target);
    t.set_is_locked(false);
    let is_locked = t.is_locked();
    let update_property =
        game_message_public_update_property_bool(t, PropertyBool::Locked, is_locked);
    world_object_networking::enqueue_broadcast(w, target, true, &[update_property]);
    //target.CurrentLandblock?.EnqueueBroadcastSound(target, Sound.Lockpicking);
    UnlockResults::UnlockSuccess
}

// ================================================================================ pointers

/// `Proficiency.OnSuccessUse(player, player.GetCreatureSkill(skill), difficulty)` (the `int`
/// overload).
fn proficiency_on_success_use(w: &mut World, player: ObjectGuid, skill: Skill, difficulty: i32) {
    let skill = crate::entity::proficiency::get_creature_skill(w, player, skill);
    crate::entity::proficiency::on_success_use_int(w, player, skill, difficulty);
}
