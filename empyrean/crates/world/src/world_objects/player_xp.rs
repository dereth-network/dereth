// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Xp.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Xp.cs`.
//!
//! Members are free functions over `(w, this)`; `this` must be a Player with a session (ACE
//! dereferences `Player.Session` without a check, and so do these). The level curve is the portal
//! dat's `XpTable` (`CharacterLevelXPList` is `level_xp`, `CharacterLevelSkillCreditList` is
//! `level_credits`). `SpendXP` and `RefundXP` are here, as in ACE; `player_skills.rs` re-exports
//! them for its raise paths.

use empyrean_common::dotnet::math::round as math_round;
use empyrean_common::dotnet::{format as dotnet_format, CsCast};
use empyrean_common::extensions::float_extensions::epsilon_equals;
use empyrean_entity::enums::{
    ChatMessageType, PlayScript, PropertyInt, PropertyInt64, ShareType, SkillAdvancementClass,
    XpType,
};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::{self, Actor};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_private_update_property_int64::game_message_private_update_property_int64;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::managers::{
    enchantment_manager as em, enchantment_manager_with_caching as emc,
};
use crate::world_objects::player_skills::send;
use crate::world_objects::world_object::{play_particle_effect, WorldObject};
use crate::World;

/// Non-property fields declared in `Player_Xp.cs`.
#[derive(Debug, Default)]
pub struct PlayerXpFields {}

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

/// `DatManager.PortalDat.XpTable.CharacterLevelXPList`.
fn level_xp_list(w: &World) -> &[u64] {
    &w.dats.portal_dat().xp_table().level_xp
}

/// `DatManager.PortalDat.XpTable.CharacterLevelSkillCreditList`.
fn level_credit_list(w: &World) -> &[u32] {
    &w.dats.portal_dat().xp_table().level_credits
}

/// `list[index]`: .NET's `ArgumentOutOfRangeException` for an index outside the list.
fn at<T: Copy>(list: &[T], index: i64) -> T {
    usize::try_from(index)
        .ok()
        .and_then(|i| list.get(i).copied())
        .unwrap_or_else(|| {
            panic!(
                "ArgumentOutOfRangeException: index {index} of {}",
                list.len()
            )
        })
}

// ACE: Player.EarnXP
/// A player earns XP through natural progression, ie. kills and quests completed. The XP
/// modifiers apply (quest XP is multiplicative with general XP modification), then `GrantXP`.
pub fn earn_xp(
    w: &mut World,
    this: ObjectGuid,
    amount: i64,
    xp_type: XpType,
    share_type: ShareType,
) {
    //Console.WriteLine($"{Name}.EarnXP({amount}, {sharable}, {fixedAmount})");

    // apply xp modifiers.  Quest XP is multiplicative with general XP modification
    let quest_modifier =
        crate::managers::property_manager::get_double(w, "quest_xp_modifier", 0.0, true).item;
    let mut modifier =
        crate::managers::property_manager::get_double(w, "xp_modifier", 0.0, true).item;
    if xp_type == XpType::Quest {
        modifier *= quest_modifier;
    }

    // should this be passed upstream to fellowship / allegiance?
    let enchantment = get_xp_and_luminance_modifier(w, this, xp_type);

    // `amount * enchantment` is long * float (a float), then times the double modifier
    let amount_f: f32 = amount.cs_cast();
    let m_amount: i64 = math_round(f64::from(amount_f * enchantment) * modifier).cs_cast();

    if m_amount < 0 {
        let name = crate::world_objects::creature_death::name_of(w, this);
        log::warn!("{name}.EarnXP({amount}, {})", share_type.to_dotnet_string());
        log::warn!("modifier: {modifier}, enchantment: {enchantment}, m_amount: {m_amount}");
        return;
    }

    grant_xp(w, this, m_amount, xp_type, share_type);
}

// ACE: Player.GrantXP
/// Directly grants XP to the player, without the XP modifier: split with the fellowship when
/// shareable, otherwise added on the player's own queue, passed up the allegiance, and granted to
/// equipped leveling items for kill and quest XP.
pub fn grant_xp(
    w: &mut World,
    this: ObjectGuid,
    amount: i64,
    xp_type: XpType,
    share_type: ShareType,
) {
    if crate::entity::damage_history_info::player_is_olthoi_player(w, this) {
        if emc::has_vitae(w, this) {
            update_xp_vitae(w, this, amount);
        }

        return;
    }

    if fellowship_share_xp(w, this) && share_type.contains(ShareType::Fellowship) {
        // this will divy up the XP, and re-call this function
        // with ShareType.Fellowship removed
        fellowship_split_xp(w, this, amount.cs_cast(), xp_type, share_type);
        return;
    }

    // Make sure UpdateXpAndLevel is done on this players thread
    i_actor::enqueue(
        w,
        Actor::Object(this),
        Action::delegate(move |w| update_xp_and_level(w, this, amount, xp_type)),
    );

    // for passing XP up the allegiance chain,
    // this function is only called at the very beginning, to start the process.
    if share_type.contains(ShareType::Allegiance) {
        update_xp_allegiance(w, this, amount);
    }

    // only certain types of XP are granted to items
    if xp_type == XpType::Kill || xp_type == XpType::Quest {
        grant_item_xp(w, this, amount);
    }
}

// ACE: Player.UpdateXpAndLevel
/// Adds XP to a player's total XP, handles triggers (vitae, level up). Private in ACE; public
/// here so the player's queued action can be run directly.
pub fn update_xp_and_level(w: &mut World, this: ObjectGuid, amount: i64, xp_type: XpType) {
    // until we are max level we must make sure that we send
    let max_level = get_max_level(w);
    let max_level_xp = *level_xp_list(w)
        .last()
        .expect("InvalidOperationException: Sequence contains no elements");

    let level = obj(w, this).level();
    if level.is_none_or(|l| i64::from(l) != i64::from(max_level)) {
        let mut add_amount = amount;

        // ACE-BUG: `(long)maxLevelXp - TotalExperience ?? 0` parses as `((long)maxLevelXp -
        // TotalExperience) ?? 0`, so a player with no TotalExperience gets 0 XP left to the end
        // (and no XP) instead of the whole curve.
        let max_level_xp: i64 = max_level_xp.cs_cast();
        let amount_left_to_end = obj(w, this)
            .total_experience()
            .map_or(0, |t| max_level_xp.wrapping_sub(t));
        if amount > amount_left_to_end {
            add_amount = amount_left_to_end;
        }

        {
            let o = obj_mut(w, this);
            let available = o.available_experience().map(|v| v.wrapping_add(add_amount));
            o.set_available_experience(available);
            let total = o.total_experience().map(|v| v.wrapping_add(add_amount));
            o.set_total_experience(total);
        }

        let o = obj_mut(w, this);
        let total = o.total_experience().unwrap_or(0);
        let xp_total_update =
            game_message_private_update_property_int64(o, PropertyInt64::TotalExperience, total);
        let available = o.available_experience().unwrap_or(0);
        let xp_avail_update = game_message_private_update_property_int64(
            o,
            PropertyInt64::AvailableExperience,
            available,
        );
        send(w, this, [xp_total_update, xp_avail_update]);

        check_for_levelup(w, this);
    }

    if xp_type == XpType::Quest {
        let msg = format!("You've earned {} experience.", dotnet_format(amount, "N0"));
        send(
            w,
            this,
            [game_message_system_chat(&msg, ChatMessageType::Broadcast)],
        );
    }

    if emc::has_vitae(w, this) && xp_type != XpType::Allegiance {
        update_xp_vitae(w, this, amount);
    }
}

// ACE: Player.UpdateXpAllegiance
/// Optionally passes XP up the Allegiance tree.
fn update_xp_allegiance(w: &mut World, this: ObjectGuid, amount: i64) {
    if !player_has_allegiance(w, this) {
        return;
    }

    allegiance_manager_pass_xp(w, this, amount.cs_cast(), true);
}

// ACE: Player.UpdateXpVitae
/// Handles updating the vitae penalty through earned XP: each full pool of XP gives back 1%.
pub fn update_xp_vitae(w: &mut World, this: ObjectGuid, amount: i64) {
    let Some(vitae) = em::get_vitae(w, this).cloned() else {
        let name = crate::world_objects::creature_death::name_of(w, this);
        log::error!(
            "{name}.UpdateXpVitae({amount}) vitae null, likely due to cross-thread operation or corrupt EnchantmentManager cache. Please report this."
        );
        log::error!("{}", std::backtrace::Backtrace::force_capture());
        return;
    };

    let mut vitae_penalty = vitae.stat_mod_value;
    let start_penalty = vitae_penalty;

    let death_level = obj(w, this)
        .death_level()
        .expect("InvalidOperationException: DeathLevel.Value");
    let mut max_pool: i32 = vitae_cp_pool_threshold(vitae_penalty, death_level).cs_cast();
    let mut cur_pool = obj(w, this)
        .vitae_cp_pool()
        .map(|p| i64::from(p).wrapping_add(amount));
    while cur_pool.is_some_and(|c| c >= i64::from(max_pool)) {
        cur_pool = cur_pool.map(|c| c - i64::from(max_pool));
        vitae_penalty = emc::reduce_vitae(w, this);
        if vitae_penalty == 1.0 {
            break;
        }
        max_pool = vitae_cp_pool_threshold(vitae_penalty, death_level).cs_cast();
    }
    let cur_pool: i32 = cur_pool
        .expect("InvalidOperationException: Nullable object must have a value.")
        .cs_cast();
    obj_mut(w, this).set_vitae_cp_pool(Some(cur_pool));

    let o = obj_mut(w, this);
    let msg = game_message_private_update_property_int(o, PropertyInt::VitaeCpPool, cur_pool);
    send(w, this, [msg]);

    if vitae_penalty != start_penalty {
        send(
            w,
            this,
            [game_message_system_chat(
                "Your experience has reduced your Vitae penalty!",
                ChatMessageType::Magic,
            )],
        );
        em::send_update_vitae(w, this);
    }

    if epsilon_equals(vitae_penalty, 1.0) || vitae_penalty > 1.0 {
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(2.0f32));
        action_chain.add_action(Actor::Object(this), move |w| {
            if let Some(vitae) = em::get_vitae(w, this) {
                let cur_penalty = vitae.stat_mod_value;
                if epsilon_equals(cur_penalty, 1.0) || cur_penalty > 1.0 {
                    em::remove_vitae(w, this);
                }
            }
        });
        action_chain.enqueue_chain(w);
    }
}

// ACE: Player.GetMaxLevel
/// Returns the maximum possible character level.
#[must_use]
pub fn get_max_level(w: &World) -> u32 {
    let count = u32::try_from(level_xp_list(w).len()).unwrap_or(u32::MAX);
    count.wrapping_sub(1)
}

// ACE: Player.IsMaxLevel
/// Returns TRUE if player >= MaxLevel.
#[must_use]
pub fn is_max_level(w: &World, this: ObjectGuid) -> bool {
    obj(w, this)
        .level()
        .is_some_and(|l| i64::from(l) >= i64::from(get_max_level(w)))
}

// ACE: Player.GetRemainingXP
/// Returns the remaining XP required to reach a level (null outside `1..=MaxLevel`).
///
/// # Panics
/// When the player has no TotalExperience (ACE: `InvalidOperationException`).
#[must_use]
pub fn get_remaining_xp_to_level(w: &World, this: ObjectGuid, level: u32) -> Option<i64> {
    let max_level = get_max_level(w);
    if level < 1 || level > max_level {
        return None;
    }

    let level_total_xp: i64 = at(level_xp_list(w), i64::from(level)).cs_cast();

    let total = obj(w, this)
        .total_experience()
        .expect("InvalidOperationException: TotalExperience.Value");
    Some(level_total_xp.wrapping_sub(total))
}

// ACE: Player.GetRemainingXP
/// Returns the remaining XP required to the next level.
///
/// # Panics
/// When the player has no Level or TotalExperience below max level (ACE:
/// `InvalidOperationException`).
#[must_use]
pub fn get_remaining_xp(w: &World, this: ObjectGuid) -> u64 {
    let max_level = get_max_level(w);
    let level = obj(w, this).level();
    if level.is_some_and(|l| i64::from(l) >= i64::from(max_level)) {
        return 0;
    }

    let level = level.expect("InvalidOperationException: Level.Value");
    let next_level_total_xp = at(level_xp_list(w), i64::from(level) + 1);
    let total: u64 = obj(w, this)
        .total_experience()
        .expect("InvalidOperationException: TotalExperience.Value")
        .cs_cast();
    next_level_total_xp.wrapping_sub(total)
}

// ACE: Player.GetTotalXP
/// Returns the total XP required to reach a level (0 outside `0..=MaxLevel`).
#[must_use]
pub fn get_total_xp(w: &World, level: i32) -> u64 {
    let max_level = get_max_level(w);
    if level < 0 || i64::from(level) > i64::from(max_level) {
        return 0;
    }

    at(level_xp_list(w), i64::from(level))
}

// ACE: Player.MaxLevelXP
/// Returns the total amount of XP required for a player reach max level.
#[must_use]
pub fn max_level_xp(w: &World) -> i64 {
    let xp_table = level_xp_list(w);

    let count = i64::try_from(xp_table.len()).unwrap_or(i64::MAX);
    at(xp_table, count - 1).cs_cast()
}

// ACE: Player.GetXPBetweenLevels
/// Returns the XP required to go from level A to level B (A clamped to `[1, MaxLevel - 1]`, B to
/// `[1, MaxLevel]`).
///
/// # Panics
/// When `MaxLevel - 1 < 1` (`Math.Clamp` throws `ArgumentException` for min > max), and on a
/// negative result (`ulong` subtraction is unchecked in ACE: it wraps; here too).
#[must_use]
pub fn get_xp_between_levels(w: &World, level_a: i32, level_b: i32) -> u64 {
    // special case for max level
    let max_level: i32 = get_max_level(w).cs_cast();

    let level_a = clamp(level_a, 1, max_level - 1);
    let level_b = clamp(level_b, 1, max_level);

    let level_a_total_xp = at(level_xp_list(w), i64::from(level_a));
    let level_b_total_xp = at(level_xp_list(w), i64::from(level_b));

    level_b_total_xp.wrapping_sub(level_a_total_xp)
}

/// `Math.Clamp(int, int, int)`: throws when `min > max`.
fn clamp(value: i32, min: i32, max: i32) -> i32 {
    assert!(
        min <= max,
        "ArgumentException: '{min}' cannot be greater than {max}."
    );
    value.clamp(min, max)
}

// ACE: Player.GetXPToNextLevel
#[must_use]
pub fn get_xp_to_next_level(w: &World, level: i32) -> u64 {
    get_xp_between_levels(w, level, level.wrapping_add(1))
}

// ACE: Player.CheckForLevelup
/// Determines if the player has advanced a level: raises the level while the total XP reaches the
/// next threshold, adding the table's skill credits, then tells the client.
pub fn check_for_levelup(w: &mut World, this: ObjectGuid) {
    let max_level = i64::from(get_max_level(w));

    let starting_level = obj(w, this).level();
    if starting_level.is_some_and(|l| i64::from(l) >= max_level) {
        return;
    }

    let mut credit_earned = false;

    // increases until the correct level is found
    // ACE-BUG: with a null Level, `Level++` stays null while the index reads `(Level ?? 0) + 1`, so
    // a player without a Level and with TotalExperience past level 1 loops forever.
    loop {
        let (total, level) = {
            let o = obj(w, this);
            (o.total_experience().unwrap_or(0), o.level())
        };
        let total: u64 = total.cs_cast();
        if total < at(level_xp_list(w), i64::from(level.unwrap_or(0)) + 1) {
            break;
        }
        // DIVERGE: V221 (fix) ACE's loop never ends with a null Level (above); the world thread
        // hung. Nothing it would do before hanging is observable, so it stops here instead.
        if level.is_none() {
            break;
        }

        let level = level.map(|l| l.wrapping_add(1));
        obj_mut(w, this).set_level(level);

        // increase the skill credits if the chart allows this level to grant a credit
        let credits = at(level_credit_list(w), i64::from(level.unwrap_or(0)));
        if credits > 0 {
            let credits: i32 = credits.cs_cast();
            let o = obj_mut(w, this);
            let available = o.available_skill_credits().map(|c| c.wrapping_add(credits));
            o.set_available_skill_credits(available);
            let total = o.total_skill_credits().map(|c| c.wrapping_add(credits));
            o.set_total_skill_credits(total);
            credit_earned = true;
        }

        // break if we reach max
        if level.is_some_and(|l| i64::from(l) == max_level) {
            play_particle_effect(w, this, PlayScript::WeddingBliss, this, 1.0);
            break;
        }
    }

    let level = obj(w, this).level();
    if let (Some(level_now), Some(start)) = (level, starting_level) {
        if level_now <= start {
            return;
        }
    } else {
        return;
    }
    let level_now = level.unwrap_or(0);

    let mut message = if i64::from(level_now) == max_level {
        format!("You have reached the maximum level of {level_now}!")
    } else {
        format!("You are now level {level_now}!")
    };

    let (available_skill_credits, available_experience) = {
        let o = obj(w, this);
        (o.available_skill_credits(), o.available_experience())
    };
    let available_experience = available_experience
        .map(|x| dotnet_format(x, "#,###0"))
        .unwrap_or_default();
    if available_skill_credits.is_some_and(|c| c > 0) {
        message += &format!(
            "\nYou have {available_experience} experience points and {} skill credits available to raise skills and attributes.",
            available_skill_credits.unwrap_or(0)
        );
    } else {
        message += &format!("\nYou have {available_experience} experience points available to raise skills and attributes.");
    }

    let o = obj_mut(w, this);
    let level_up = game_message_private_update_property_int(o, PropertyInt::Level, level_now);
    let credits = o.available_skill_credits().unwrap_or(0);
    let current_credits =
        game_message_private_update_property_int(o, PropertyInt::AvailableSkillCredits, credits);

    if i64::from(level_now) != max_level && !credit_earned {
        let mut next_level_with_credits = 0;

        let mut i = i64::from(level_now) + 1;
        while i <= max_level {
            if at(level_credit_list(w), i) > 0 {
                next_level_with_credits = i;
                break;
            }
            i += 1;
        }
        message +=
            &format!("\nYou will earn another skill credit at level {next_level_with_credits}.");
    }

    if crate::world_objects::player_fellowship::fellowship(w, this).is_some() {
        fellowship_on_fellow_level_up(w, this);
    }

    if player_has_allegiance_node(w, this) {
        allegiance_node_on_level_up(w, this);
    }

    send(w, this, [level_up]);

    dispatch::set_max_vitals::set_max_vitals(w, this);

    // play level up effect
    play_particle_effect(w, this, PlayScript::LevelUp, this, 1.0);

    send(
        w,
        this,
        [
            game_message_system_chat(&message, ChatMessageType::Advancement),
            current_credits,
        ],
    );
}

// ACE: Player.SpendXP
/// Spends the amount of XP specified, deducting it from available experience.
pub fn spend_xp(w: &mut World, this: ObjectGuid, amount: i64, send_network_update: bool) -> bool {
    if crate::world_objects::player_skills::exceeds_available_experience(w, this, amount) {
        return false;
    }

    let o = obj_mut(w, this);
    let v = o.available_experience().map(|x| x.wrapping_sub(amount));
    o.set_available_experience(v);

    if send_network_update {
        let o = obj_mut(w, this);
        let value = o.available_experience().unwrap_or(0);
        let msg = game_message_private_update_property_int64(
            o,
            PropertyInt64::AvailableExperience,
            value,
        );
        send(w, this, [msg]);
    }

    true
}

// ACE: Player.SpendAllXp
/// Tries to spend all of the players Xp into Attributes, Vitals and Skills.
pub fn spend_all_xp(w: &mut World, this: ObjectGuid, send_network_update: bool) {
    use crate::world_objects::{player_attributes, player_skills, player_vitals};

    let attributes = {
        let o = obj(w, this);
        [
            o.strength(),
            o.endurance(),
            o.coordination(),
            o.quickness(),
            o.focus(),
            o.self_(),
        ]
    };
    for attribute in attributes {
        player_attributes::spend_all_available_attribute_xp(
            w,
            this,
            attribute,
            send_network_update,
        );
    }

    let vitals = {
        let o = obj(w, this);
        [o.health(), o.stamina(), o.mana()]
    };
    for vital in vitals {
        player_vitals::spend_all_available_vital_xp(w, this, vital, send_network_update);
    }

    let skills: Vec<_> = obj(w, this).skills().values().copied().collect();
    for skill in skills {
        if skill.advancement_class(obj(w, this)).0 >= SkillAdvancementClass::Trained.0 {
            player_skills::spend_all_available_skill_xp(w, this, skill, send_network_update);
        }
    }
}

// ACE: Player.RefundXP
/// Gives available XP of the amount specified, without increasing total XP.
pub fn refund_xp(w: &mut World, this: ObjectGuid, amount: i64) {
    let o = obj_mut(w, this);
    let v = o.available_experience().map(|x| x.wrapping_add(amount));
    o.set_available_experience(v);

    let o = obj_mut(w, this);
    let value = o.available_experience().unwrap_or(0);
    let xp_update =
        game_message_private_update_property_int64(o, PropertyInt64::AvailableExperience, value);
    send(w, this, [xp_update]);
}

// ACE: Player.HandleMissingXp
/// After 5 s, tells the player about the XP a `VerifyXp` property records and applies it.
pub fn handle_missing_xp(w: &mut World, this: ObjectGuid) {
    let verify_xp = obj(w, this)
        .get_property(PropertyInt64::VerifyXp)
        .unwrap_or(0);
    if verify_xp == 0 {
        return;
    }

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(5.0f32));
    action_chain.add_action(Actor::Object(this), move |w| {
        let xp_type = if verify_xp > 0 { "unassigned experience" } else { "experience points" };

        // `Math.Abs(long.MinValue)` throws OverflowException
        let abs = verify_xp.checked_abs().expect("OverflowException: Negating the minimum value of a twos complement number is invalid.");
        let msg = format!(
            "This character was missing some {xp_type} --\nYou have gained an additional {} {xp_type}!",
            dotnet_format(abs, "N0")
        );

        send(w, this, [game_message_system_chat(&msg, ChatMessageType::Broadcast)]);

        if verify_xp < 0 {
            // add to character's total XP
            let o = obj_mut(w, this);
            let total = o.total_experience().map(|t| t.wrapping_sub(verify_xp));
            o.set_total_experience(total);

            check_for_levelup(w, this);
        }

        obj_mut(w, this).remove_property(PropertyInt64::VerifyXp);
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.VitaeCPPoolThreshold
/// Returns the total amount of XP required to go from vitae to vitae + 0.01. `vitae` is the
/// current player life force (0.95 = 5% penalty), `level` the player's DeathLevel.
#[must_use]
pub fn vitae_cp_pool_threshold(vitae: f32, level: i32) -> f64 {
    (empyrean_common::math::pow(f64::from(level), 2.5) * 2.5 + 20.0)
        * empyrean_common::math::pow(f64::from(vitae), 5.0)
        + 0.5
}

// ACE: Player.GrantLevelProportionalXp
/// Raise the available XP by a percentage of the current level XP or a maximum.
///
/// # Panics
/// When the player has no Level (ACE: `InvalidOperationException`).
pub fn grant_level_proportional_xp(
    w: &mut World,
    this: ObjectGuid,
    percent: f64,
    min: i64,
    max: i64,
) {
    let level = obj(w, this)
        .level()
        .expect("InvalidOperationException: Level.Value");
    let next_level_xp = get_xp_between_levels(w, level, level.wrapping_add(1));

    let next: f64 = next_level_xp.cs_cast();
    let mut scaled_xp: i64 = math_round(next * percent).cs_cast();

    if max > 0 {
        scaled_xp = scaled_xp.min(max);
    }

    if min > 0 {
        scaled_xp = scaled_xp.max(min);
    }

    // apply xp modifiers?
    earn_xp(w, this, scaled_xp, XpType::Quest, ShareType::Allegiance);
}

// ACE: Player.GrantItemXP
/// The player earns XP for items that can be leveled up by killing creatures and completing
/// quests, while those items are equipped.
pub fn grant_item_xp(w: &mut World, this: ObjectGuid, amount: i64) {
    let items: Vec<ObjectGuid> =
        crate::world_objects::creature_death::creature_equipped_objects(w, this)
            .into_iter()
            .filter(|i| world_object_has_item_level(w, *i))
            .collect();
    for item in items {
        grant_item_xp_to(w, this, item, amount);
    }
}

// ACE: Player.GrantItemXP
/// The item overload: adds the XP to one item and handles its level-up.
///
/// # Panics
/// When the item has no ItemLevel (ACE: `InvalidOperationException`).
pub fn grant_item_xp_to(w: &mut World, this: ObjectGuid, item: ObjectGuid, amount: i64) {
    let prev_item_level =
        world_object_item_level(w, item).expect("InvalidOperationException: ItemLevel.Value");
    let add_item_xp = world_object_add_item_xp(w, item, amount);

    if add_item_xp > 0 {
        let o = obj_mut(w, item);
        let total = o
            .item_total_xp()
            .expect("InvalidOperationException: ItemTotalXp.Value");
        let msg = game_message_private_update_property_int64(o, PropertyInt64::ItemTotalXp, total);
        send(w, this, [msg]);
    }

    // handle item leveling up
    let new_item_level =
        world_object_item_level(w, item).expect("InvalidOperationException: ItemLevel.Value");
    if new_item_level > prev_item_level {
        player_on_item_level_up(w, this, item, prev_item_level);

        let mut action_chain = ActionChain::new();
        action_chain.add_action(Actor::Object(this), move |w| {
            let item_name = crate::world_objects::creature_death::name_of(w, item);
            let msg = format!("Your {item_name} has increased in power to level {new_item_level}!");
            send(
                w,
                this,
                [game_message_system_chat(&msg, ChatMessageType::Broadcast)],
            );

            // `EnqueueBroadcast(new GameMessageScript(Guid, PlayScript.AetheriaLevelUp));`
            world_object_enqueue_broadcast_script(w, this, PlayScript::AetheriaLevelUp);
        });
        action_chain.enqueue_chain(w);
    }
}

// ACE: Player.GetXPAndLuminanceModifier
/// Returns the multiplier to XP and Luminance from Trinkets and Augmentations.
pub fn get_xp_and_luminance_modifier(w: &mut World, this: ObjectGuid, xp_type: XpType) -> f32 {
    let enchantment_bonus = emc::get_xp_bonus(w, this);

    let mut aug_bonus = 0.0f32;
    let augmentation_bonus_xp = obj(w, this).augmentation_bonus_xp();
    if xp_type == XpType::Kill && augmentation_bonus_xp > 0 {
        let aug: f32 = augmentation_bonus_xp.cs_cast();
        aug_bonus = aug * 0.05;
    }

    //Console.WriteLine($"XPAndLuminanceModifier: {modifier}");
    1.0 + enchantment_bonus + aug_bonus
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files, named after them.
// ---------------------------------------------------------------------------------------------

/// `Fellowship != null && Fellowship.ShareXP` (`Player_Fellowship.cs`, `Entity/Fellowship.cs`).
fn fellowship_share_xp(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_fellowship::fellowship(w, this).is_some_and(|f| f.get(w).share_xp)
}

/// `Fellowship.SplitXp((ulong)amount, xpType, shareType, this)`.
fn fellowship_split_xp(
    w: &mut World,
    this: ObjectGuid,
    amount: u64,
    xp_type: XpType,
    share_type: ShareType,
) {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, this)
        .expect("ACE: Fellowship is null (NullReferenceException)");
    crate::entity::fellowship::split_xp(w, &fellowship, amount, xp_type, share_type, this);
}

/// `Fellowship.OnFellowLevelUp(this)`.
fn fellowship_on_fellow_level_up(w: &mut World, this: ObjectGuid) {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, this)
        .expect("ACE: Fellowship is null (NullReferenceException)");
    crate::entity::fellowship::on_fellow_level_up(w, &fellowship, this);
}

/// `HasAllegiance` (`Player_Allegiance.cs`).
fn player_has_allegiance(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_allegiance::has_allegiance(w, this)
}

/// `AllegianceNode != null` (`Player_Allegiance.cs`).
fn player_has_allegiance_node(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_allegiance::i_player_allegiance_node(
        w,
        crate::entity::i_player::IPlayer::Online(this),
    )
    .is_some()
}

/// `AllegianceNode.OnLevelUp()`.
fn allegiance_node_on_level_up(w: &mut World, this: ObjectGuid) {
    if let Some(node) = crate::world_objects::player_allegiance::i_player_allegiance_node(
        w,
        crate::entity::i_player::IPlayer::Online(this),
    ) {
        crate::entity::allegiance_node::on_level_up(w, node);
    }
}

/// `AllegianceManager.PassXP(AllegianceNode, amount, true)`.
fn allegiance_manager_pass_xp(w: &mut World, this: ObjectGuid, amount: u64, direct: bool) {
    let node = crate::world_objects::player_allegiance::i_player_allegiance_node(
        w,
        crate::entity::i_player::IPlayer::Online(this),
    )
    .expect("ACE: AllegianceNode is null (NullReferenceException)");
    crate::managers::allegiance_manager::pass_xp(w, node, amount, direct);
}

/// `item.HasItemLevel` (`WorldObject_Set.cs`).
fn world_object_has_item_level(w: &World, item: ObjectGuid) -> bool {
    w.objects
        .get(item)
        .expect("ACE: item is null (NullReferenceException)")
        .has_item_level()
}

/// `item.ItemLevel` (`WorldObject_Set.cs`, computed from ItemTotalXp).
fn world_object_item_level(w: &World, item: ObjectGuid) -> Option<i32> {
    w.objects
        .get(item)
        .expect("ACE: item is null (NullReferenceException)")
        .item_level()
}

/// `item.AddItemXP(amount)` (`WorldObject_Set.cs`).
fn world_object_add_item_xp(w: &mut World, item: ObjectGuid, amount: i64) -> i64 {
    w.objects
        .get_mut(item)
        .expect("ACE: item is null (NullReferenceException)")
        .add_item_xp(amount)
}

/// `OnItemLevelUp(item, prevItemLevel)` (`Player_Aetheria.cs`).
fn player_on_item_level_up(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    prev_item_level: i32,
) {
    crate::world_objects::player_spells::on_item_level_up(w, this, item, prev_item_level);
}

/// `EnqueueBroadcast(new GameMessageScript(Guid, script))` (`WorldObject_Networking.cs`).
fn world_object_enqueue_broadcast_script(w: &mut World, this: ObjectGuid, script: PlayScript) {
    let msg = crate::network::game_messages::messages::game_message_script::game_message_script(
        this, script, 1.0,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}
