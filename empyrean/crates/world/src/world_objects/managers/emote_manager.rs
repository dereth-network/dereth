// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs
//! Port of `Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs`.
//!
//! The manager's state is [`EmoteManager`], stored as `WorldObjectFields.emote_manager` and built
//! by `WorldObject.SetEphemeralValues`. Every member that reaches other objects, the network or
//! the action queues is a free function taking `this`, the object that owns the manager (ACE's
//! `_worldObject`); [`world_object`](fn@world_object) is ACE's `WorldObject` property (`_proxy ?? _worldObject`).
//!
//! References are guids: an emote's target that has left `World.objects`
//! reads as ACE's `null`. An emote set is shared with its queued continuations as an
//! `Arc<PropertiesEmote>` copy of the biota's record (ACE shares the record itself).
//!
//! Callees in other ACE files (QuestManager, Fellowship, contracts, events, luminance,
//! titles, Monster_Magic's cast motions, doors, `DeleteObject`, ...) are called by ACE's names
//! through the `shims` module at the end of this file, each a pointer to its port.

use std::sync::Arc;

use empyrean_common::dotnet::{CsCast, Quaternion, TimeSpan};
use empyrean_common::extensions::time_span_extensions;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::ext::motion_command_helper;
use empyrean_entity::enums::{
    Channel, CharacterTitle, ChatMessageType, CreatureType, EmoteCategory, EmoteType, ItemType,
    MotionCommand, MotionStance, PlayScript, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyFloat, PropertyInt, PropertyInt64, PropertyString, ShareType, Skill,
    SkillAdvancementClass, Sound, SpellFlags, VendorType, XpType,
};
use empyrean_entity::models::properties_emote::PropertiesEmote;
use empyrean_entity::models::properties_emote_action::PropertiesEmoteAction;
use empyrean_entity::{LandblockId, ObjectGuid, Position, Vector3};
use empyrean_tables::enums::{TreasureItemCategory, TreasureItemType};

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::entity::landblock;
use crate::entity::position_extensions;
use crate::entity::spell::Spell;
use crate::managers::player_manager::{self, player_session};
use crate::network::game_event::events::game_event_popup_string::game_event_popup_string;
use crate::network::game_event::events::game_event_tell::game_event_tell;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_hear_ranged_speech::game_message_hear_ranged_speech;
use crate::network::game_messages::messages::game_message_hear_speech::game_message_hear_speech;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;
use crate::network::game_messages::messages::game_message_public_update_property_float::game_message_public_update_property_float;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_public_update_property_int64::game_message_public_update_property_int64;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::world_object::{self, WorldObject, LOCAL_BROADCAST_RANGE};
use crate::world_objects::{
    container, creature_death, player_inventory, player_skills, player_xp, world_object_networking,
};
use crate::World;

/// `EmoteManager`'s fields (`WorldObject.EmoteManager`, one per object).
// ACE: EmoteManager
#[derive(Debug, Default, Clone)]
pub struct EmoteManager {
    // ACE's `_worldObject` is the object that holds this manager: `this` in every function here.
    /// Set while the object is used through a proxy (a Hooker on a Hook).
    // ACE: EmoteManager._proxy
    proxy: Option<ObjectGuid>,
    /// Returns TRUE if this WorldObject is currently busy processing other emotes
    // ACE: EmoteManager.IsBusy
    pub is_busy: bool,
    // ACE: EmoteManager.Nested
    pub nested: i32,
    // ACE: EmoteManager.Debug
    pub debug: bool,
}

impl EmoteManager {
    // ACE: EmoteManager.EmoteManager
    #[must_use]
    pub fn new() -> Self {
        EmoteManager {
            proxy: None,
            is_busy: false,
            nested: 0,
            debug: false,
        }
    }
}

/// The maximum animation range of the client. Motions broadcast outside of this range will be
/// automatically queued by client.
// ACE: EmoteManager.ClientMaxAnimRange
pub const CLIENT_MAX_ANIM_RANGE: f32 = 96.0; // verify: same indoors?

/// The client automatically queues animations that are broadcast outside of 96.0f range.
/// Normally we exclude these emotes from being broadcast outside this range, but for certain
/// emotes (like monsters going to sleep) we want to always broadcast / enqueue.
// ACE: EmoteManager.MotionQueue
pub const MOTION_QUEUE: &[MotionCommand] = &[MotionCommand::Sleeping];

// ================================================================================ helpers

/// An object that must exist: panics like the C# `NullReferenceException` when it does not.
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

/// `this.EmoteManager`.
fn em(w: &World, this: ObjectGuid) -> &EmoteManager {
    &obj(w, this).wo.world_object.emote_manager
}

fn em_mut(w: &mut World, this: ObjectGuid) -> &mut EmoteManager {
    &mut obj_mut(w, this).wo.world_object.emote_manager
}

/// `Name` (a null name reads as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `o as Player` / `o as Creature`: `None` for ACE's null, and for an object that is not one.
fn as_player(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player))
}

fn as_creature(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature))
}

/// `player.Session.Network.EnqueueSend(msg)`: nothing without a session.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    if let Some(s) = player_session(w, player) {
        enqueue_send(w, s, msg);
    }
}

fn system_chat(
    w: &mut World,
    player: ObjectGuid,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    send(
        w,
        player,
        game_message_system_chat(message, chat_message_type),
    );
}

fn weenie_class_id(w: &World, g: ObjectGuid) -> u32 {
    obj(w, g).biota.weenie_class_id
}

/// `string.IsNullOrWhiteSpace`.
fn is_null_or_white_space(s: Option<&str>) -> bool {
    s.is_none_or(|s| s.chars().all(char::is_whitespace))
}

/// `StringComparison.OrdinalIgnoreCase` equality of two characters (simple case mapping).
fn eq_ignore_case_char(a: char, b: char) -> bool {
    a == b || a.to_uppercase().eq(b.to_uppercase())
}

/// `string.Equals(other, StringComparison.OrdinalIgnoreCase)`.
fn equals_ignore_case(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| eq_ignore_case_char(x, y))
}

/// `string.Replace(oldValue, newValue, StringComparison.OrdinalIgnoreCase)` (`old` is never empty
/// in ACE's calls).
fn replace_ignore_case(s: &str, old: &str, new: &str) -> String {
    let old: Vec<char> = old.chars().collect();
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let matches = !old.is_empty()
            && i + old.len() <= chars.len()
            && old
                .iter()
                .enumerate()
                .all(|(k, &c)| eq_ignore_case_char(chars[i + k], c));
        if matches {
            result.push_str(new);
            i += old.len();
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// `(T)nullable` on a null value: .NET throws `InvalidOperationException`.
fn value<T>(v: Option<T>, what: &str) -> T {
    v.unwrap_or_else(|| {
        panic!("System.InvalidOperationException: Nullable object must have a value ({what})")
    })
}

/// `emote.Stat` cast to an enum keyed by `ushort` (`(PropertyInt)emote.Stat` and the others).
fn stat_u16(emote: &PropertiesEmoteAction) -> u16 {
    value(emote.stat, "emote.Stat").cs_cast()
}

fn stat_skill(emote: &PropertiesEmoteAction) -> Skill {
    Skill(value(emote.stat, "emote.Stat"))
}

/// `TestSuccess` or `TestFailure`.
fn test(success: bool) -> EmoteCategory {
    if success {
        EmoteCategory::TestSuccess
    } else {
        EmoteCategory::TestFailure
    }
}

/// `QuestSuccess` or `QuestFailure`.
fn quest_result(success: bool) -> EmoteCategory {
    if success {
        EmoteCategory::QuestSuccess
    } else {
        EmoteCategory::QuestFailure
    }
}

/// `value >= (emote.Min ?? int.MinValue) && value <= (emote.Max ?? int.MaxValue)` for a `uint`
/// stat (C# lifts both sides to `long`).
fn in_int_range(v: u32, emote: &PropertiesEmoteAction) -> bool {
    i64::from(v) >= i64::from(emote.min.unwrap_or(i32::MIN))
        && i64::from(v) <= i64::from(emote.max.unwrap_or(i32::MAX))
}

// ================================================================================ EmoteManager.cs

/// `WorldObject => _proxy ?? _worldObject`.
// ACE: EmoteManager.WorldObject
#[must_use]
pub fn world_object(w: &World, this: ObjectGuid) -> ObjectGuid {
    em(w, this).proxy.unwrap_or(this)
}

/// Executes an emote. `emote_set` is the parent set of this emote, `target_object` a target
/// object, usually player. Returns the delay before the next emote.
// ACE: EmoteManager.ExecuteEmote
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn execute_emote(
    w: &mut World,
    this: ObjectGuid,
    emote_set: &Arc<PropertiesEmote>,
    emote: &PropertiesEmoteAction,
    target_object: Option<ObjectGuid>,
) -> f32 {
    let wo = world_object(w, this);
    let player = as_player(w, target_object);
    let creature = as_creature(w, Some(wo));
    let target_creature = as_creature(w, target_object);

    let mut delay = 0.0f32;
    let emote_type = EmoteType(emote.r#type.cs_cast());

    //if (Debug)
    //Console.WriteLine($"{WorldObject.Name}.ExecuteEmote({emoteType})");

    let text = emote.message.as_deref();
    let quest = emote_set.quest.as_deref();
    let message_ref = emote.message.as_deref();

    match emote_type {
        EmoteType::Act => {
            // short for 'acting' text
            let message = replace(w, this, text, Some(wo), target_object, quest);
            world_object_networking::enqueue_broadcast_range(
                w,
                wo,
                &game_message_system_chat(&message, ChatMessageType::Broadcast),
                30.0,
                None,
            );
        }

        EmoteType::Activate => {
            let activation_target = obj(w, wo).activation_target();
            let generator_id = obj(w, wo).generator_id();
            if activation_target > 0 {
                // ActOnUse delay?
                let lb = obj(w, wo).current_landblock;
                let activation_target = lb.and_then(|lb| {
                    landblock::get_object(w, lb, ObjectGuid::new(activation_target), true)
                });
                if let Some(activation_target) = activation_target {
                    dispatch::on_activate::on_activate(w, activation_target, player.unwrap_or(wo));
                }
            } else if generator_id.is_some_and(|g| g > 0) {
                // Fallback to linked generator
                let lb = obj(w, wo).current_landblock;
                let linked_generator = lb.and_then(|lb| {
                    landblock::get_object(w, lb, ObjectGuid::new(generator_id.unwrap_or(0)), true)
                });
                if let Some(linked_generator) = linked_generator {
                    dispatch::on_activate::on_activate(w, linked_generator, wo);
                }
            }
        }

        EmoteType::AddCharacterTitle => {
            // emoteAction.Stat == null for all EmoteType.AddCharacterTitle entries in current db?
            if let Some(player) = player.filter(|_| emote.amount != Some(0)) {
                // A null Amount passes the `!= 0` test and then throws on the cast, as in ACE.
                shims::player_add_title(
                    w,
                    player,
                    CharacterTitle(value(emote.amount, "emote.Amount").cast_unsigned()),
                );
            }
        }

        EmoteType::AddContract => {
            // Contracts werent in emote table for 16py, guessing that Stat was used to hold id for contract.
            if let Some(player) = player {
                if let Some(stat) = emote.stat.filter(|&s| s > 0) {
                    shims::contract_manager_add(w, player, stat);
                }
            }
        }

        EmoteType::AdminSpam => {
            let text = replace(w, this, message_ref, Some(wo), target_object, quest);

            player_manager::broadcast_to_channel_from_emote(w, Channel::Admin, &text);
        }

        EmoteType::AwardLevelProportionalSkillXP => {
            let min = emote.min_64.or(emote.min.map(i64::from)).unwrap_or(0);
            let max = emote.max_64.or(emote.max.map(i64::from)).unwrap_or(0);

            if let Some(player) = player {
                player_skills::grant_level_proportional_skill_xp(
                    w,
                    player,
                    stat_skill(emote),
                    emote.percent.unwrap_or(0.0),
                    min,
                    max,
                );
            }
        }

        EmoteType::AwardLevelProportionalXP => {
            let min = emote.min_64.or(emote.min.map(i64::from)).unwrap_or(0);
            let max = emote.max_64.or(emote.max.map(i64::from)).unwrap_or(0);

            if let Some(player) = player {
                player_xp::grant_level_proportional_xp(
                    w,
                    player,
                    emote.percent.unwrap_or(0.0),
                    min,
                    max,
                );
            }
        }

        EmoteType::AwardLuminance => {
            if let Some(player) = player {
                shims::player_earn_luminance(
                    w,
                    player,
                    emote.amount_64.or(emote.hero_xp_64).unwrap_or(0),
                    XpType::Quest,
                    ShareType::None,
                );
            }
        }

        EmoteType::AwardNoShareXP => {
            if let Some(player) = player {
                player_xp::earn_xp(
                    w,
                    player,
                    emote.amount_64.or(emote.amount.map(i64::from)).unwrap_or(0),
                    XpType::Quest,
                    ShareType::None,
                );
            }
        }

        EmoteType::AwardSkillPoints => {
            if let Some(player) = player {
                player_skills::award_skill_points(
                    w,
                    player,
                    stat_skill(emote),
                    value(emote.amount, "emote.Amount").cast_unsigned(),
                );
            }
        }

        EmoteType::AwardSkillXP => {
            if let Some(player) = player {
                if delay < 1.0 {
                    delay += 1.0; // because of how AwardSkillXP grants and then raises the skill, ensure delay is at least 1 to allow for processing correctly
                }
                player_skills::award_skill_xp(
                    w,
                    player,
                    stat_skill(emote),
                    value(emote.amount, "emote.Amount").cast_unsigned(),
                    true,
                );
            }
        }

        EmoteType::AwardTrainingCredits => {
            if let Some(player) = player {
                player_skills::add_skill_credits(w, player, emote.amount.unwrap_or(0));
            }
        }

        EmoteType::AwardXP => {
            if let Some(player) = player {
                let amt = emote.amount_64.or(emote.amount.map(i64::from)).unwrap_or(0);
                if amt > 0 {
                    player_xp::earn_xp(w, player, amt, XpType::Quest, ShareType::All);
                } else if amt < 0 {
                    player_xp::spend_xp(w, player, amt.wrapping_neg(), true);
                }
            }
        }

        EmoteType::BLog => {
            let text = replace(w, this, message_ref, Some(wo), target_object, quest);

            log::info!(
                "0x{}:{}({}).EmoteManager.BLog - {}",
                wo,
                name(w, wo),
                weenie_class_id(w, wo),
                text
            );
        }

        EmoteType::CastSpell => {
            // `if (WorldObject != null)`: always here.
            let spell = Spell::from_int(w, value(emote.spell_id, "emote.SpellId"), true);
            if spell.not_found() {
                log::error!(
                    "{} ({}) EmoteManager.CastSpell - unknown spell {:?}",
                    name(w, wo),
                    wo,
                    emote.spell_id
                );
            } else {
                // ACE-BUG: `creature` is `WorldObject as Creature`; a CastSpell emote on an object that is not a
                // Creature throws NullReferenceException here, which aborts the emote set and leaves IsBusy set.
                let creature = creature.unwrap_or_else(|| panic!("System.NullReferenceException: EmoteManager.CastSpell on a non-Creature 0x{:08X}", wo.full()));

                shims::creature_check_for_human_pre_cast(w, creature, &spell);

                let spell_target = get_spell_target(w, this, &spell, target_object);

                let pre_cast_time = shims::creature_pre_cast_motion(w, creature, spell_target);

                delay = pre_cast_time + shims::creature_get_post_cast_time(w, creature, &spell);

                let mut cast_chain = ActionChain::new();
                cast_chain.add_delay_seconds(w, f64::from(pre_cast_time));
                cast_chain.add_action(Actor::Object(creature), move |w| {
                    crate::world_objects::world_object_magic::try_cast_spell_with_redirects(
                        w,
                        creature,
                        &spell,
                        spell_target,
                        Some(creature),
                        None,
                        false,
                        false,
                        true,
                    );
                    shims::creature_post_cast_motion(w, creature);
                });
                cast_chain.enqueue_chain(w);
            }
        }

        EmoteType::CastSpellInstant => {
            // `if (WorldObject != null)`: always here.
            let spell = Spell::from_int(w, value(emote.spell_id, "emote.SpellId"), true);

            if !spell.not_found() {
                let spell_target = get_spell_target(w, this, &spell, target_object);

                crate::world_objects::world_object_magic::try_cast_spell_with_redirects(
                    w,
                    wo,
                    &spell,
                    spell_target,
                    Some(wo),
                    None,
                    false,
                    false,
                    true,
                );
            }
        }

        EmoteType::CloseMe => {
            // animation delay?
            if obj(w, wo).is_container() {
                dispatch::close::close(w, wo, ObjectGuid::INVALID);
            } else if obj(w, wo).is_door() {
                shims::door_close(w, wo);
            }
        }

        EmoteType::CreateTreasure => {
            if let Some(player) = player {
                create_treasure(w, wo, player, emote);
            }
        }

        // decrements a PropertyInt stat by some amount
        EmoteType::DecrementIntStat | EmoteType::IncrementIntStat => {
            // DecrementIntStat: only used by 1 emote in 16PY - check for lower bounds?
            if let (Some(target_object), Some(_)) = (target_object, emote.stat) {
                let int_property = PropertyInt(stat_u16(emote));
                let mut current = obj(w, target_object)
                    .get_property(int_property)
                    .unwrap_or(0);
                if emote_type == EmoteType::DecrementIntStat {
                    current = current.wrapping_sub(emote.amount.unwrap_or(1));
                } else {
                    current = current.wrapping_add(emote.amount.unwrap_or(1));
                }
                obj_mut(w, target_object).set_property(int_property, current);

                if let Some(player) = player {
                    let m = game_message_private_update_property_int(
                        obj_mut(w, player),
                        int_property,
                        current,
                    );
                    send(w, player, m);
                }
            }
        }

        EmoteType::DecrementMyQuest | EmoteType::DecrementQuest => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                shims::quest_manager_decrement(
                    w,
                    quest_target,
                    message_ref,
                    emote.amount.unwrap_or(1),
                );
            }
        }

        EmoteType::DeleteSelf => {
            if let Some(player) = player {
                let found = player_inventory::find_object(
                    w,
                    player,
                    obj(w, wo).guid,
                    player_inventory::SearchLocations::Everywhere,
                );

                shims::world_object_delete_object(w, wo, found.root_owner);
            } else {
                shims::world_object_delete_object(w, wo, None);
            }
        }

        EmoteType::DirectBroadcast => {
            let text = replace(w, this, message_ref, Some(wo), target_object, quest);

            if let Some(player) = player {
                system_chat(w, player, &text, ChatMessageType::Broadcast);
            }
        }

        EmoteType::Enlightenment => {
            if let Some(player) = player {
                shims::enlightenment_handle_enlightenment(w, wo, player);
            }
        }

        EmoteType::EraseMyQuest | EmoteType::EraseQuest => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                shims::quest_manager_erase(w, quest_target, message_ref);
            }
        }

        EmoteType::FellowBroadcast => {
            if let Some(player) = player {
                if shims::player_has_fellowship(w, player) {
                    let text = replace(w, this, message_ref, Some(wo), Some(player), quest);

                    shims::fellowship_broadcast_to_fellow(w, player, &text);
                }
            }
        }

        EmoteType::Generate => {
            if obj(w, wo).is_generator() {
                crate::world_objects::world_object_generators::generator_generate(w, wo);
            }
        }

        EmoteType::Give => {
            let stack_size = emote.stack_size.unwrap_or(1);

            if let (Some(player), Some(_)) = (player, emote.weenie_class_id) {
                let mut motion_chain = ActionChain::new();

                if !obj(w, wo).dont_turn_or_move_when_giving() {
                    if let (Some(creature), Some(target_creature)) = (creature, target_creature) {
                        delay = dispatch::rotate::rotate(w, creature, target_creature);
                        motion_chain.add_delay_seconds(w, f64::from(delay));
                    }
                }
                let weenie_class_id = emote.weenie_class_id.unwrap_or(0);
                let palette = emote.palette.unwrap_or(0);
                let shade = emote.shade.unwrap_or(0.0);
                motion_chain.add_action(Actor::Object(wo), move |w| {
                    if !w.objects.contains(this) || !w.objects.contains(player) {
                        return;
                    }
                    let wo = world_object(w, this);
                    player_inventory::give_from_emote(
                        w,
                        player,
                        Some(wo),
                        weenie_class_id,
                        if stack_size > 0 { stack_size } else { 1 },
                        palette,
                        shade,
                    );
                });
                motion_chain.enqueue_chain(w);
            }
        }

        // redirects to the GotoSet category for this action
        EmoteType::Goto => {
            // TODO: revisit if nested chains need to back-propagate timers
            let goto_set = get_emote_set(
                w,
                this,
                EmoteCategory::GotoSet,
                message_ref,
                None,
                None,
                true,
            );
            execute_emote_set_arc(w, this, goto_set.map(Arc::new), target_object, true);
        }

        EmoteType::IncrementMyQuest | EmoteType::IncrementQuest => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                shims::quest_manager_increment(
                    w,
                    quest_target,
                    message_ref,
                    emote.amount.unwrap_or(1),
                );
            }
        }

        EmoteType::InflictVitaePenalty => {
            if let Some(player) = player {
                crate::world_objects::player_death::inflict_vitae_penalty(
                    w,
                    player,
                    emote.amount.unwrap_or(5),
                );
            }
        }

        EmoteType::InqAttributeStat | EmoteType::InqRawAttributeStat => {
            if let Some(target_creature) = target_creature {
                // `targetCreature.Attributes[(PropertyAttribute)emote.Stat]`: the indexer throws
                // KeyNotFoundException for an attribute the creature does not have, so `attr` is never
                // null and the TestNoQuality branch is never taken.
                let attribute = PropertyAttribute(stat_u16(emote));
                let attr = *obj(w, target_creature)
                    .attributes()
                    .get(&attribute)
                    .unwrap_or_else(|| panic!("System.Collections.Generic.KeyNotFoundException: Attributes[{attribute:?}]"));

                let attr_value = if emote_type == EmoteType::InqAttributeStat {
                    attr.current(&mut StatCtx::in_world(w, target_creature))
                } else {
                    attr.base(obj(w, target_creature))
                };
                let success = in_int_range(attr_value, emote);

                execute_emote_set_category(
                    w,
                    this,
                    test(success),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqBoolStat => {
            if let Some(target) = target_object {
                let stat = obj(w, target).get_property(PropertyBool(stat_u16(emote)));

                if stat.is_none() && has_valid_test_no_quality(w, this, message_ref) {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestNoQuality,
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    let success = stat.unwrap_or(false);

                    execute_emote_set_category(
                        w,
                        this,
                        test(success),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqContractsFull => {
            let full = player.is_some_and(|p| shims::contract_manager_is_full(w, p));
            execute_emote_set_category(w, this, test(full), message_ref, target_object, true);
        }

        EmoteType::InqEvent => {
            let started = shims::event_manager_is_event_started(w, message_ref, wo, target_object);
            execute_emote_set_category(
                w,
                this,
                if started {
                    EmoteCategory::EventSuccess
                } else {
                    EmoteCategory::EventFailure
                },
                message_ref,
                target_object,
                true,
            );
        }

        EmoteType::InqFellowNum => {
            // unused in PY16 - ensure # of fellows between min-max?
            let mut result = if has_valid_test_no_fellow(w, this, message_ref) {
                EmoteCategory::TestNoFellow
            } else {
                EmoteCategory::NumFellowsFailure
            };

            if let Some(player) = player.filter(|&p| shims::player_has_fellowship(w, p)) {
                let fellows = shims::fellowship_get_fellowship_members_count(w, player);

                result = if fellows < emote.min.unwrap_or(i32::MIN)
                    || fellows > emote.max.unwrap_or(i32::MAX)
                {
                    EmoteCategory::NumFellowsFailure
                } else {
                    EmoteCategory::NumFellowsSuccess
                };
            }
            execute_emote_set_category(w, this, result, message_ref, target_object, true);
        }

        EmoteType::InqFellowQuest => {
            if let Some(player) = player {
                if shims::player_has_fellowship(w, player) {
                    let has_quest =
                        shims::fellowship_quest_manager_has_quest(w, player, message_ref);
                    let can_solve =
                        shims::fellowship_quest_manager_can_solve(w, player, message_ref);

                    // verify: QuestSuccess = player has quest, and their last completed time + quest minDelta <= currentTime
                    let success = has_quest && !can_solve;

                    execute_emote_set_category(
                        w,
                        this,
                        quest_result(success),
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::QuestNoFellow,
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqFloatStat => {
            if let Some(target) = target_object {
                let stat = obj(w, target).get_property(PropertyFloat(stat_u16(emote)));

                if stat.is_none() && has_valid_test_no_quality(w, this, message_ref) {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestNoQuality,
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    let stat = stat.unwrap_or(0.0);
                    let success = stat >= emote.min_dbl.unwrap_or(f64::MIN)
                        && stat <= emote.max_dbl.unwrap_or(f64::MAX);
                    execute_emote_set_category(
                        w,
                        this,
                        test(success),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqInt64Stat => {
            if let Some(target) = target_object {
                let stat = obj(w, target).get_property(PropertyInt64(stat_u16(emote)));

                if stat.is_none() && has_valid_test_no_quality(w, this, message_ref) {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestNoQuality,
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    let stat = stat.unwrap_or(0);
                    let success = stat >= emote.min_64.unwrap_or(i64::MIN)
                        && stat <= emote.max_64.unwrap_or(i64::MAX);
                    execute_emote_set_category(
                        w,
                        this,
                        test(success),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqIntStat => {
            if let Some(target) = target_object {
                let stat = obj(w, target).get_property(PropertyInt(stat_u16(emote)));

                if stat.is_none() && has_valid_test_no_quality(w, this, message_ref) {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestNoQuality,
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    let stat = stat.unwrap_or(0);
                    let success = stat >= emote.min.unwrap_or(i32::MIN)
                        && stat <= emote.max.unwrap_or(i32::MAX);
                    execute_emote_set_category(
                        w,
                        this,
                        test(success),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqNumCharacterTitles => {
            if let Some(player) = player {
                let num_titles = obj(w, player).num_character_titles();
                let success = num_titles.is_some_and(|n| {
                    n >= emote.min.unwrap_or(i32::MIN) && n <= emote.max.unwrap_or(i32::MAX)
                });
                let category = if success {
                    EmoteCategory::NumCharacterTitlesSuccess
                } else {
                    EmoteCategory::NumCharacterTitlesFailure
                };
                execute_emote_set_category(w, this, category, message_ref, target_object, true);
            }
        }

        EmoteType::InqOwnsItems => {
            if let Some(player) = player {
                let num_required = emote.stack_size.unwrap_or(1);

                let mut items = container::get_inventory_items_of_wcid(
                    w,
                    player,
                    emote.weenie_class_id.unwrap_or(0),
                );
                items.extend(player_inventory::get_equipped_objects_of_wcid(
                    w,
                    player,
                    emote.weenie_class_id.unwrap_or(0),
                ));
                // `items.Sum(i => i.StackSize ?? 1)`: LINQ's int Sum is checked.
                let num_items = items.iter().fold(0i32, |sum, &i| {
                    sum.checked_add(obj(w, i).stack_size().unwrap_or(1)).expect(
                        "System.OverflowException: Arithmetic operation resulted in an overflow.",
                    )
                });

                let success = num_items >= num_required;

                execute_emote_set_category(
                    w,
                    this,
                    test(success),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqPackSpace => {
            if let Some(player) = player {
                let num_required = emote.amount.unwrap_or(1);

                // Since emote was not in 16py and we have just the two fields to go on, I will assume you could "mask" the value to pick between free Item Capacity space or free Container Capacity space
                let success = if num_required > 10000 {
                    let free_space = container::get_free_container_slots(w, player);

                    free_space >= num_required - 10000
                } else {
                    let free_space = container::get_free_inventory_slots(w, player, false); // assuming this was only for main pack. makes things easier at this point.

                    free_space >= num_required
                };

                execute_emote_set_category(
                    w,
                    this,
                    test(success),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqMyQuest | EmoteType::InqQuest => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                let has_quest = shims::quest_manager_has_quest(w, quest_target, message_ref);
                let can_solve = shims::quest_manager_can_solve(w, quest_target, message_ref);

                //  verify: QuestSuccess = player has quest, but their quest timer is currently still on cooldown
                let success = has_quest && !can_solve;

                execute_emote_set_category(
                    w,
                    this,
                    quest_result(success),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqMyQuestBitsOff | EmoteType::InqQuestBitsOff => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                let has_no_quest_bits = shims::quest_manager_has_no_quest_bits(
                    w,
                    quest_target,
                    message_ref,
                    emote.amount.unwrap_or(0),
                );

                execute_emote_set_category(
                    w,
                    this,
                    quest_result(has_no_quest_bits),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqMyQuestBitsOn | EmoteType::InqQuestBitsOn => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                let has_quest_bits = shims::quest_manager_has_quest_bits(
                    w,
                    quest_target,
                    message_ref,
                    emote.amount.unwrap_or(0),
                );

                execute_emote_set_category(
                    w,
                    this,
                    quest_result(has_quest_bits),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqMyQuestSolves | EmoteType::InqQuestSolves => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                let quest_solves = shims::quest_manager_has_quest_solves(
                    w,
                    quest_target,
                    message_ref,
                    emote.min,
                    emote.max,
                );

                execute_emote_set_category(
                    w,
                    this,
                    quest_result(quest_solves),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqRawSecondaryAttributeStat | EmoteType::InqSecondaryAttributeStat => {
            if let Some(target_creature) = target_creature {
                // `targetCreature.Vitals[(PropertyAttribute2nd)emote.Stat]`: the indexer throws
                // KeyNotFoundException for a vital the creature does not have, so `vital` is never null.
                let key = PropertyAttribute2nd(stat_u16(emote));
                let vital = *obj(w, target_creature)
                    .vitals()
                    .get(&key)
                    .unwrap_or_else(|| {
                        panic!("System.Collections.Generic.KeyNotFoundException: Vitals[{key:?}]")
                    });

                let vital_value = if emote_type == EmoteType::InqRawSecondaryAttributeStat {
                    vital.base(w, obj(w, target_creature))
                } else {
                    vital.current(obj(w, target_creature))
                };
                let success = in_int_range(vital_value, emote);

                execute_emote_set_category(
                    w,
                    this,
                    test(success),
                    message_ref,
                    target_object,
                    true,
                );
            }
        }

        EmoteType::InqRawSkillStat
        | EmoteType::InqSkillStat
        | EmoteType::InqSkillSpecialized
        | EmoteType::InqSkillTrained => {
            if let Some(target_creature) = target_creature {
                // `GetCreatureSkill(skill)` adds a missing skill (add = true), so it is never null.
                let skill = obj_mut(w, target_creature).get_creature_skill(stat_skill(emote), true);

                if skill.is_none() && has_valid_test_no_quality(w, this, message_ref) {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestNoQuality,
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    let success = skill.is_some_and(|skill| match emote_type {
                        EmoteType::InqRawSkillStat => {
                            in_int_range(skill.base(w, obj(w, target_creature)), emote)
                        }
                        EmoteType::InqSkillStat => {
                            in_int_range(skill.current(w, target_creature), emote)
                        }
                        EmoteType::InqSkillSpecialized => {
                            skill.advancement_class(obj(w, target_creature))
                                == SkillAdvancementClass::Specialized
                        }
                        _ => {
                            skill.advancement_class(obj(w, target_creature))
                                >= SkillAdvancementClass::Trained
                        }
                    });

                    execute_emote_set_category(
                        w,
                        this,
                        test(success),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqStringStat => {
            if let Some(target) = target_object {
                let string_stat = obj(w, target).get_property(PropertyString(stat_u16(emote)));

                if string_stat.is_none() && has_valid_test_no_quality(w, this, message_ref) {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestNoQuality,
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    let success = string_stat
                        .is_some_and(|s| emote.test_string.as_deref() == Some(s.as_str()));
                    execute_emote_set_category(
                        w,
                        this,
                        test(success),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::InqYesNo => {
            if let Some(player) = player {
                let replaced = replace(
                    w,
                    this,
                    emote.test_string.as_deref(),
                    Some(wo),
                    target_object,
                    quest,
                );
                if !shims::confirmation_manager_enqueue_send_yes_no(
                    w,
                    player,
                    wo,
                    message_ref,
                    &replaced,
                ) {
                    // Not ACE's (a fix, V341): the TestFailure set runs nested, as
                    // the other test emotes' branches do, so a player whose confirmation could not be
                    // sent (one is already open) gets the failure emotes. ACE ran it as a new set,
                    // which the busy object refused, so nothing followed.
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::TestFailure,
                        message_ref,
                        Some(player),
                        true,
                    );
                }
            }
        }

        EmoteType::Invalid => {}

        EmoteType::KillSelf => {
            if let Some(creature) = creature {
                creature_death::smite(w, creature, creature, false);
            }
        }

        EmoteType::LocalBroadcast => {
            let message = replace(w, this, message_ref, Some(wo), target_object, quest);
            world_object_networking::enqueue_broadcast(
                w,
                wo,
                true,
                &[game_message_system_chat(
                    &message,
                    ChatMessageType::Broadcast,
                )],
            );
        }

        EmoteType::LocalSignal => {
            // `if (WorldObject != null)`: always here.
            if let Some(lb) = obj(w, wo).current_landblock {
                landblock::emit_signal(w, lb, wo, message_ref.unwrap_or(""));
            }
        }

        EmoteType::LockFellow => {
            if let Some(player) = player.filter(|&p| shims::player_has_fellowship(w, p)) {
                shims::player_handle_action_fellowship_change_lock(w, player, true, quest);
            }
        }

        // plays an animation on the target object (usually the player)
        EmoteType::ForceMotion => {
            let motion_command =
                motion_command_helper::get_motion(value(emote.motion, "emote.Motion"));
            let target = target_object.unwrap_or_else(|| {
                panic!("System.NullReferenceException: EmoteManager.ForceMotion with no target")
            });
            let motion = Motion::from_world_object(w, target, motion_command, emote.extent);
            world_object_networking::enqueue_broadcast_motion(w, target, &motion, None, None);
        }

        // plays an animation on the source object
        EmoteType::Motion => {
            if let Some(d) = execute_motion_emote(w, this, wo, emote_set, emote) {
                delay = d;
            }
        }

        // move to position relative to home
        EmoteType::Move => {
            if let Some(creature) = creature {
                move_relative_to_home(w, this, wo, creature, emote);
            }
        }

        EmoteType::MoveHome => {
            // TODO: call MoveToManager on server, handle delay for this?
            if let Some(creature) = creature {
                if let Some(home) = obj(w, creature).home() {
                    let location = obj(w, creature)
                        .location()
                        .expect("System.NullReferenceException: creature.Location");
                    // are we already at home origin?
                    if location.pos() == home.pos() {
                        // just turnto if required?
                        if em(w, this).debug {
                            log::debug!(" - already at home origin, checking rotation");
                        }

                        if location.rotation() != home.rotation() {
                            delay = shims::creature_turn_to(w, creature, &home);
                        }
                    } else {
                        // how to get delay with this, callback required?
                        let run_rate = shims::creature_get_run_rate(w, creature);
                        crate::world_objects::creature_navigation::creature_move_to_position(
                            w,
                            creature,
                            &home,
                            run_rate,
                            true,
                            None,
                            Some(emote.extent),
                        );
                    }
                }
            }
        }

        EmoteType::MoveToPos => {
            if let Some(creature) = creature {
                let current_pos = obj(w, creature)
                    .location()
                    .expect("System.NullReferenceException: creature.Location");

                let mut new_pos = Position::new();
                new_pos.set_landblock_id(LandblockId::new(
                    emote
                        .obj_cell_id
                        .unwrap_or(current_pos.landblock_id().raw()),
                ));

                new_pos.set_pos(Vector3::new(
                    emote.origin_x.unwrap_or(current_pos.position_x),
                    emote.origin_y.unwrap_or(current_pos.position_y),
                    emote.origin_z.unwrap_or(current_pos.position_z),
                ));

                if emote.angles_x.is_none()
                    || emote.angles_y.is_none()
                    || emote.angles_z.is_none()
                    || emote.angles_w.is_none()
                {
                    new_pos.set_rotation(Quaternion::new(
                        current_pos.rotation_x,
                        current_pos.rotation_y,
                        current_pos.rotation_z,
                        current_pos.rotation_w,
                    ));
                } else {
                    new_pos.set_rotation(Quaternion::new(
                        emote.angles_x.unwrap_or(0.0),
                        emote.angles_y.unwrap_or(0.0),
                        emote.angles_z.unwrap_or(0.0),
                        emote.angles_w.unwrap_or(1.0),
                    ));
                }

                //if (emote.ObjCellId != null)
                //newPos.LandblockId = new LandblockId(emote.ObjCellId.Value);

                new_pos
                    .set_landblock_id(LandblockId::new(position_extensions::get_cell(w, &new_pos)));

                // TODO: handle delay for this?
                let run_rate = shims::creature_get_run_rate(w, creature);
                crate::world_objects::creature_navigation::creature_move_to_position(
                    w,
                    creature,
                    &new_pos,
                    run_rate,
                    true,
                    None,
                    Some(emote.extent),
                );
            }
        }

        EmoteType::OpenMe => {
            if obj(w, wo).is_container() {
                // ACE-BUG: `Container.Open(null)` dereferences the null player (`player.LastOpenedContainerId`),
                // so an OpenMe emote on a container throws NullReferenceException.
                dispatch::open::open(w, wo, ObjectGuid::INVALID);
            } else if obj(w, wo).is_door() {
                shims::door_open(w, wo);
            }
        }

        EmoteType::PetCastSpellOnOwner => {
            if let Some(creature) = creature.filter(|&c| obj(w, c).is_pet()) {
                if let Some(owner) = shims::pet_p_pet_owner(w, creature) {
                    let spell = Spell::from_int(w, value(emote.spell_id, "emote.SpellId"), true);
                    crate::world_objects::world_object_magic::try_cast_spell(
                        w,
                        creature,
                        &spell,
                        Some(owner),
                        None,
                        None,
                        false,
                        false,
                        true,
                    );
                }
            }
        }

        EmoteType::PhysScript => {
            let p_script: PlayScript = value(emote.p_script, "emote.PScript");
            world_object::play_particle_effect(w, wo, p_script, wo, emote.extent);
        }

        EmoteType::PopUp => {
            if let Some(player) = player {
                if let Some(s) = player_session(w, player) {
                    let m = game_event_popup_string(
                        crate::network::game_event::game_event_message::session_data(w, s),
                        message_ref.unwrap_or(""),
                    );
                    enqueue_send(w, s, m);
                }
            }
        }

        EmoteType::RemoveContract => {
            if let Some(player) = player {
                if let Some(stat) = emote.stat.filter(|&s| s > 0) {
                    crate::world_objects::player_contracts::handle_action_abandon_contract(
                        w,
                        player,
                        stat.cast_unsigned(),
                    );
                }
            }
        }

        EmoteType::RemoveVitaePenalty => {
            if let Some(player) = player {
                crate::world_objects::managers::enchantment_manager::remove_vitae(w, player);
            }
        }

        EmoteType::ResetHomePosition => {
            if let Some(location) = obj(w, wo).location() {
                obj_mut(w, wo).set_home(Some(Position::from_position(&location)));
            }
        }

        EmoteType::Say => {
            if em(w, this).debug {
                log::debug!(" - {:?}", emote.message);
            }

            let message = replace(w, this, message_ref, Some(wo), target_object, quest);

            let wo_name = name(w, wo);
            let name = if obj(w, wo).creature_type() == Some(CreatureType::Olthoi) {
                wo_name + "&"
            } else {
                wo_name
            };

            let m = if emote.extent > 0.0 {
                game_message_hear_ranged_speech(
                    &message,
                    &name,
                    wo.full(),
                    emote.extent,
                    ChatMessageType::Emote,
                )
            } else {
                game_message_hear_speech(&message, &name, wo.full(), ChatMessageType::Emote)
            };
            world_object_networking::enqueue_broadcast_range(
                w,
                wo,
                &m,
                LOCAL_BROADCAST_RANGE,
                None,
            );
        }

        EmoteType::SetAltRacialSkills
        | EmoteType::SetEyePalette
        | EmoteType::SetEyeTexture
        | EmoteType::SetHeadObject
        | EmoteType::SetHeadPalette
        | EmoteType::SetMouthPalette
        | EmoteType::SetMouthTexture
        | EmoteType::SetNosePalette
        | EmoteType::SetNoseTexture => {
            // SetEyePalette:   //if (creature != null) creature.EyesPaletteDID = (uint)emote.Display;
            // SetEyeTexture:   //if (creature != null) creature.EyesTextureDID = (uint)emote.Display;
            // SetHeadObject:   //if (creature != null) creature.HeadObjectDID = (uint)emote.Display;
            // SetMouthTexture: //if (creature != null) creature.MouthTextureDID = (uint)emote.Display;
            // SetNoseTexture:  //if (creature != null) creature.NoseTextureDID = (uint)emote.Display;
        }

        EmoteType::SetBoolStat => {
            if let Some(player) = player {
                let prop = PropertyBool(stat_u16(emote));
                let v = emote.amount != Some(0);
                crate::world_objects::player_properties::update_property_bool(
                    w,
                    player,
                    player,
                    prop,
                    Some(v),
                    false,
                );
                let m = game_message_public_update_property_bool(obj_mut(w, player), prop, v);
                world_object_networking::enqueue_broadcast(w, player, false, &[m]);
            }
        }

        EmoteType::SetFloatStat => {
            if let Some(player) = player {
                let prop = PropertyFloat(stat_u16(emote));
                crate::world_objects::player_properties::update_property_float(
                    w,
                    player,
                    player,
                    prop,
                    emote.percent,
                    false,
                );
                // `Convert.ToDouble(null)` is 0.
                let m = game_message_public_update_property_float(
                    obj_mut(w, player),
                    prop,
                    emote.percent.unwrap_or(0.0),
                );
                world_object_networking::enqueue_broadcast(w, player, false, &[m]);
            }
        }

        EmoteType::SetInt64Stat => {
            if let Some(player) = player {
                let prop = PropertyInt64(stat_u16(emote));
                crate::world_objects::player_properties::update_property_int64(
                    w,
                    player,
                    player,
                    prop,
                    emote.amount_64,
                    false,
                );
                // `Convert.ToInt64(null)` is 0.
                let m = game_message_public_update_property_int64(
                    obj_mut(w, player),
                    prop,
                    emote.amount_64.unwrap_or(0),
                );
                world_object_networking::enqueue_broadcast(w, player, false, &[m]);
            }
        }

        EmoteType::SetIntStat => {
            if let Some(player) = player {
                let prop = PropertyInt(stat_u16(emote));
                crate::world_objects::player_properties::update_property_int(
                    w,
                    player,
                    player,
                    prop,
                    emote.amount,
                    false,
                );
                // `Convert.ToInt32(null)` is 0.
                let m = game_message_public_update_property_int(
                    obj_mut(w, player),
                    prop,
                    emote.amount.unwrap_or(0),
                );
                world_object_networking::enqueue_broadcast(w, player, false, &[m]);
            }
        }

        EmoteType::SetMyQuestBitsOff | EmoteType::SetQuestBitsOff => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let (Some(quest_target), Some(message), Some(amount)) =
                (quest_target, message_ref, emote.amount)
            {
                shims::quest_manager_set_quest_bits(w, quest_target, message, amount, false);
            }
        }

        EmoteType::SetMyQuestBitsOn | EmoteType::SetQuestBitsOn => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let (Some(quest_target), Some(message), Some(amount)) =
                (quest_target, message_ref, emote.amount)
            {
                shims::quest_manager_set_quest_bits(w, quest_target, message, amount, true);
            }
        }

        EmoteType::SetMyQuestCompletions | EmoteType::SetQuestCompletions => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let (Some(quest_target), Some(amount)) = (quest_target, emote.amount) {
                shims::quest_manager_set_quest_completions(w, quest_target, message_ref, amount);
            }
        }

        EmoteType::SetSanctuaryPosition => {
            if let Some(player) = player {
                let pos = Position::from_components(
                    value(emote.obj_cell_id, "emote.ObjCellId"),
                    value(emote.origin_x, "emote.OriginX"),
                    value(emote.origin_y, "emote.OriginY"),
                    value(emote.origin_z, "emote.OriginZ"),
                    value(emote.angles_x, "emote.AnglesX"),
                    value(emote.angles_y, "emote.AnglesY"),
                    value(emote.angles_z, "emote.AnglesZ"),
                    value(emote.angles_w, "emote.AnglesW"),
                    false,
                );
                obj_mut(w, player).set_position(PositionType::Sanctuary, Some(pos));
            }
        }

        EmoteType::Sound => {
            let sound: Sound = value(emote.sound, "emote.Sound");
            let m = game_message_sound(obj(w, wo).guid, sound, 1.0);
            world_object_networking::enqueue_broadcast(w, wo, true, &[m]);
        }

        EmoteType::SpendLuminance => {
            if let Some(player) = player {
                shims::player_spend_luminance(
                    w,
                    player,
                    emote.amount_64.or(emote.hero_xp_64).unwrap_or(0),
                );
            }
        }

        EmoteType::StampFellowQuest => {
            if let Some(player) = player {
                if shims::player_has_fellowship(w, player) {
                    shims::fellowship_quest_manager_stamp(w, player, message_ref);
                }
            }
        }

        EmoteType::StampMyQuest | EmoteType::StampQuest => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                let quest_name = message_ref.unwrap_or_else(|| {
                    panic!("System.NullReferenceException: emote.Message.EndsWith")
                });

                if quest_name.ends_with("@#kt") {
                    log::warn!(
                        "0x{}:{} ({}).EmoteManager.ExecuteEmote: EmoteType.StampQuest({}) is a depreciated kill task method.",
                        wo,
                        name(w, wo),
                        weenie_class_id(w, wo),
                        quest_name
                    );
                }

                shims::quest_manager_stamp(w, quest_target, message_ref);
            }
        }

        EmoteType::StartBarber => {
            if let Some(player) = player {
                shims::player_start_barber(w, player);
            }
        }

        EmoteType::StartEvent => {
            shims::event_manager_start_event(w, message_ref, wo, target_object);
        }

        EmoteType::StopEvent => {
            shims::event_manager_stop_event(w, message_ref, wo, target_object);
        }

        EmoteType::TakeItems => {
            if let Some(player) = player {
                take_items(w, wo, player, emote);
            }
        }

        EmoteType::TeachSpell => {
            if let Some(player) = player {
                shims::player_learn_spell_with_networking(
                    w,
                    player,
                    value(emote.spell_id, "emote.SpellId").cast_unsigned(),
                    false,
                );
            }
        }

        EmoteType::TeleportSelf => {
            //if (WorldObject is Player)
            //(WorldObject as Player).Teleport(emote.Position);
        }

        EmoteType::TeleportTarget => {
            if let Some(player) = player {
                teleport_target(w, wo, player, emote);
            }
        }

        EmoteType::Tell => {
            if let Some(player) = player {
                let message = replace(w, this, message_ref, Some(wo), Some(player), quest);
                if let Some(s) = player_session(w, player) {
                    let m = game_event_tell(w, wo, &message, player, s, ChatMessageType::Tell);
                    enqueue_send(w, s, m);
                }
            }
        }

        EmoteType::TellFellow => {
            if let Some(player) = player {
                if shims::player_has_fellowship(w, player) {
                    let text = replace(w, this, message_ref, Some(wo), Some(player), quest);

                    shims::fellowship_tell_fellow(w, player, wo, &text);
                }
            }
        }

        EmoteType::TextDirect => {
            if let Some(player) = player {
                let message = replace(w, this, message_ref, Some(wo), Some(player), quest);
                system_chat(w, player, &message, ChatMessageType::Broadcast);
            }
        }

        EmoteType::Turn => {
            if let Some(creature) = creature {
                // turn to heading
                let rotation = Quaternion::new(
                    emote.angles_x.unwrap_or(0.0),
                    emote.angles_y.unwrap_or(0.0),
                    emote.angles_z.unwrap_or(0.0),
                    emote.angles_w.unwrap_or(1.0),
                );
                let location = obj(w, creature)
                    .location()
                    .expect("System.NullReferenceException: new Position(creature.Location)");
                let mut new_pos = Position::from_position(&location);
                new_pos.set_rotation(rotation);

                let rotate_time = shims::creature_turn_to(w, creature, &new_pos);
                delay = rotate_time;
            }
        }

        EmoteType::TurnToTarget => {
            if let (Some(creature), Some(target_creature)) = (creature, target_creature) {
                delay = dispatch::rotate::rotate(w, creature, target_creature);
            }
        }

        EmoteType::UntrainSkill => {
            if let Some(player) = player {
                player_skills::reset_skill(w, player, stat_skill(emote), true);
            }
        }

        EmoteType::UpdateFellowQuest => {
            if let Some(player) = player {
                if shims::player_has_fellowship(w, player) {
                    let quest_name = message_ref;

                    let has_quest =
                        shims::fellowship_quest_manager_has_quest(w, player, quest_name);

                    if has_quest {
                        // update existing quest
                        let can_solve =
                            shims::fellowship_quest_manager_can_solve(w, player, quest_name);
                        if can_solve {
                            shims::fellowship_quest_manager_stamp(w, player, quest_name);
                        }
                        execute_emote_set_category(
                            w,
                            this,
                            quest_result(can_solve),
                            message_ref,
                            target_object,
                            true,
                        );
                    } else {
                        // add new quest
                        shims::fellowship_quest_manager_update(w, player, quest_name);
                        let has_quest =
                            shims::fellowship_quest_manager_has_quest(w, player, quest_name);
                        execute_emote_set_category(
                            w,
                            this,
                            quest_result(has_quest),
                            message_ref,
                            target_object,
                            true,
                        );
                    }
                } else {
                    execute_emote_set_category(
                        w,
                        this,
                        EmoteCategory::QuestNoFellow,
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::UpdateMyQuest | EmoteType::UpdateQuest => {
            let quest_target = get_quest_target(emote_type, target_creature, creature);

            if let Some(quest_target) = quest_target {
                let quest_name = message_ref;

                let has_quest = shims::quest_manager_has_quest(w, quest_target, quest_name);

                if has_quest {
                    // update existing quest
                    let can_solve = shims::quest_manager_can_solve(w, quest_target, quest_name);
                    if can_solve {
                        shims::quest_manager_stamp(w, quest_target, quest_name);
                    }
                    execute_emote_set_category(
                        w,
                        this,
                        quest_result(can_solve),
                        message_ref,
                        target_object,
                        true,
                    );
                } else {
                    // add new quest
                    shims::quest_manager_update(w, quest_target, quest_name);
                    let has_quest = shims::quest_manager_has_quest(w, quest_target, quest_name);
                    execute_emote_set_category(
                        w,
                        this,
                        quest_result(has_quest),
                        message_ref,
                        target_object,
                        true,
                    );
                }
            }
        }

        EmoteType::WorldBroadcast => {
            let message = replace(w, this, text, Some(wo), target_object, quest);

            player_manager::broadcast_to_all(
                w,
                &game_message_system_chat(&message, ChatMessageType::WorldBroadcast),
            );

            player_manager::log_broadcast_chat(w, Channel::AllBroadcast, Some(wo), &message);
        }

        _ => {
            log::debug!(
                "EmoteManager.Execute - Encountered Unhandled EmoteType {:?} for {} ({})",
                emote_type,
                name(w, wo),
                weenie_class_id(w, wo)
            );
        }
    }

    delay
}

/// `EmoteType.CreateTreasure`, the body of its `if (player != null)`.
fn create_treasure(
    w: &mut World,
    wo: ObjectGuid,
    player: ObjectGuid,
    emote: &PropertiesEmoteAction,
) {
    let treasure_tier = emote.wealth_rating.unwrap_or(1);

    let treasure_type = emote
        .treasure_type
        .map_or(TreasureItemCategory::Undef, TreasureItemCategory);

    let treasure_class = emote
        .treasure_class
        .map_or(TreasureItemType::Undef, TreasureItemType);

    // Create a dummy treasure profile for passing emote values
    let profile = empyrean_content::models::world::TreasureDeath {
        tier: treasure_tier,
        //TreasureType = (uint)treasureType,
        loot_quality_mod: 0.0,
        item_chance: 100,
        item_min_amount: 1,
        item_max_amount: 1,
        //ItemTreasureTypeSelectionChances = (int)treasureClass,
        magic_item_chance: 100,
        magic_item_min_amount: 1,
        magic_item_max_amount: 1,
        //MagicItemTreasureTypeSelectionChances = (int)treasureClass,
        mundane_item_chance: 100,
        mundane_item_min_amount: 1,
        mundane_item_max_amount: 1,
        //MundaneItemTypeSelectionChances = (int)treasureClass,
        unknown_chances: 21,
        ..Default::default()
    };

    let treasure =
        crate::factories::loot_generation_factory::create_random_loot_objects_of_category(
            w,
            &profile,
            treasure_type,
            treasure_class,
        );
    if let Some(treasure) = treasure {
        let guid = treasure.guid;
        if w.objects.insert(treasure).is_err() {
            panic!("a new loot object 0x{:08X} is already live", guid.full());
        }
        // an item that could not be given is dropped (ACE leaves it to the GC)
        if !player_inventory::try_create_for_give(w, player, wo, guid) {
            w.objects.remove(guid);
        }
    }
}

/// `EmoteType.Move`, the body of its `if (creature != null)`: move to position relative to home.
fn move_relative_to_home(
    w: &mut World,
    this: ObjectGuid,
    wo: ObjectGuid,
    creature: ObjectGuid,
    emote: &PropertiesEmoteAction,
) {
    // If the landblock is dormant, there are no players in range
    if landblock_is_dormant(w, wo) {
        return;
    }

    // are there players within emote range?
    if !world_object_networking::players_in_range(w, wo, CLIENT_MAX_ANIM_RANGE) {
        return;
    }

    let home = obj(w, creature)
        .home()
        .unwrap_or_else(|| panic!("System.NullReferenceException: new Position(creature.Home)"));
    let mut new_pos = Position::from_position(&home);
    new_pos.set_pos(
        new_pos.pos()
            + Vector3::new(
                emote.origin_x.unwrap_or(0.0),
                emote.origin_y.unwrap_or(0.0),
                emote.origin_z.unwrap_or(0.0),
            ),
    ); // uses relative position

    // ensure valid quaternion - all 0s for example can lock up physics engine
    if let (Some(x), Some(y), Some(z), Some(qw)) = (
        emote.angles_x,
        emote.angles_y,
        emote.angles_z,
        emote.angles_w,
    ) {
        if x != 0.0 || y != 0.0 || z != 0.0 || qw != 0.0 {
            // also relative, or absolute?
            new_pos.set_rotation(new_pos.rotation() * Quaternion::new(x, y, z, qw));
        }
    }

    if em(w, this).debug {
        log::debug!("{}", new_pos.to_loc_string());
    }

    // get new cell
    new_pos.set_landblock_id(LandblockId::new(position_extensions::get_cell(w, &new_pos)));

    // TODO: handle delay for this?
    let run_rate = shims::creature_get_run_rate(w, creature);
    crate::world_objects::creature_navigation::creature_move_to_position(
        w,
        creature,
        &new_pos,
        run_rate,
        true,
        None,
        Some(emote.extent),
    );
}

/// `EmoteType.TeleportTarget`, the body of its `if (player != null)`.
fn teleport_target(
    w: &mut World,
    wo: ObjectGuid,
    player: ObjectGuid,
    emote: &PropertiesEmoteAction,
) {
    let (Some(cell), Some(x), Some(y), Some(z), Some(ax), Some(ay), Some(az), Some(aw)) = (
        emote.obj_cell_id,
        emote.origin_x,
        emote.origin_y,
        emote.origin_z,
        emote.angles_x,
        emote.angles_y,
        emote.angles_z,
        emote.angles_w,
    ) else {
        return;
    };

    if cell > 0 {
        let mut destination = Position::from_components(cell, x, y, z, ax, ay, az, aw, false);

        world_object::adjust_dungeon(w, &mut destination);
        crate::managers::world_manager::thread_safe_teleport(w, player, destination, None, false);
    } else {
        // position is relative to WorldObject's current location
        let location = obj(w, wo).location().unwrap_or_else(|| {
            panic!("System.NullReferenceException: new Position(WorldObject.Location)")
        });
        let mut relative_destination = Position::from_position(&location);
        relative_destination.set_pos(relative_destination.pos() + Vector3::new(x, y, z));
        relative_destination.set_rotation(Quaternion::new(ax, ay, az, aw));
        relative_destination.set_landblock_id(LandblockId::new(position_extensions::get_cell(
            w,
            &relative_destination,
        )));

        world_object::adjust_dungeon(w, &mut relative_destination);
        crate::managers::world_manager::thread_safe_teleport(
            w,
            player,
            relative_destination,
            None,
            false,
        );
    }
}

/// `EmoteType.TakeItems`, the body of its `if (player != null)`.
fn take_items(w: &mut World, wo: ObjectGuid, player: ObjectGuid, emote: &PropertiesEmoteAction) {
    let weenie_item_to_take = emote.weenie_class_id.unwrap_or(0);
    let amount_to_take = emote.stack_size.unwrap_or(1);

    if weenie_item_to_take == 0 {
        log::warn!(
            "EmoteManager.Execute: 0x{} {} ({}) EmoteType.TakeItems has invalid emote.WeenieClassId: {}",
            wo,
            name(w, wo),
            weenie_class_id(w, wo),
            weenie_item_to_take
        );
        return;
    }

    if amount_to_take < -1 || amount_to_take == 0 {
        log::warn!(
            "EmoteManager.Execute: 0x{} {} ({}) EmoteType.TakeItems has invalid emote.StackSize: {}",
            wo,
            name(w, wo),
            weenie_class_id(w, wo),
            amount_to_take
        );
        return;
    }

    let amount = if amount_to_take == -1 {
        i32::MAX
    } else {
        amount_to_take
    };
    let taken = (container::get_num_inventory_items_of_wcid(w, player, weenie_item_to_take) > 0
        && player_inventory::try_consume_from_inventory_with_networking_wcid(
            w,
            player,
            weenie_item_to_take,
            amount,
        ))
        || (player_inventory::get_num_equipped_objects_of_wcid(w, player, weenie_item_to_take) > 0
            && player_inventory::try_consume_from_equipped_objects_with_networking_wcid(
                w,
                player,
                weenie_item_to_take,
                amount,
            ));
    if taken {
        let item_taken = w.content.get_cached_weenie(weenie_item_to_take);
        if let Some(item_taken) = item_taken {
            let amount = if amount_to_take == -1 {
                "all".to_owned()
            } else {
                amount_to_take.to_string()
            };

            let msg = format!(
                "You hand over {amount} of your {}.",
                item_taken.get_plural_name()
            );

            system_chat(w, player, &msg, ChatMessageType::Broadcast);
        }
    }
}

/// `WorldObject.CurrentLandblock?.IsDormant ?? false`.
fn landblock_is_dormant(w: &World, wo: ObjectGuid) -> bool {
    obj(w, wo)
        .current_landblock
        .and_then(|lb| w.landblock_manager.landblocks.get(lb))
        .is_some_and(|l| l.is_dormant)
}

/// `EmoteType.Motion`: plays an animation on the source object. `None` is a `break` that leaves
/// `delay` as it was.
fn execute_motion_emote(
    w: &mut World,
    this: ObjectGuid,
    wo: ObjectGuid,
    emote_set: &Arc<PropertiesEmote>,
    emote: &PropertiesEmoteAction,
) -> Option<f32> {
    let debug_motion = false;

    if em(w, this).debug {
        log::debug!(".{:?}", emote.motion);
    }

    // If the landblock is dormant, there are no players in range
    if landblock_is_dormant(w, wo) {
        return None;
    }

    // are there players within emote range?
    if !world_object_networking::players_in_range(w, wo, CLIENT_MAX_ANIM_RANGE) {
        return None;
    }

    if let Some(h) = obj(w, wo).phys {
        if crate::physics::motion::is_moving_to(w, h) {
            return None;
        }
    }

    let current = obj(w, wo)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()?;

    let mut delay = None;

    // TODO: REFACTOR ME
    if let Some(style) = emote_set
        .style
        .filter(|_| emote_set.category != EmoteCategory::Vendor)
    {
        let starting_motion =
            Motion::new(style, value(emote_set.substyle, "emoteSet.Substyle"), 1.0);
        let emote_motion = value(emote.motion, "emote.Motion");
        let motion = Motion::new(style, emote_motion, emote.extent);

        if current.stance != starting_motion.stance {
            if current.stance == MotionStance::Invalid {
                if debug_motion {
                    log::debug!(
                        "{} running starting motion {:?}, {:?}",
                        name(w, wo),
                        style,
                        emote_set.substyle
                    );
                }

                delay = Some(world_object::execute_motion(
                    w,
                    wo,
                    starting_motion,
                    true,
                    None,
                    false,
                ));
            }
        } else if current.motion_state.forward_command
            == starting_motion.motion_state.forward_command
            && starting_motion.stance == MotionStance::NonCombat
        {
            // enforce non-combat here?
            if debug_motion {
                log::debug!(
                    "{} running motion {:?}, {:?}",
                    name(w, wo),
                    style,
                    emote_motion
                );
            }

            let mut max_range = Some(CLIENT_MAX_ANIM_RANGE);
            if MOTION_QUEUE.contains(&emote_motion) {
                max_range = None;
            }

            let anim_length = motion_table_get_animation_length(
                w,
                wo,
                current.stance,
                emote_motion,
                Some(MotionCommand::Ready),
            );

            delay = Some(world_object::execute_motion(
                w, wo, motion, true, max_range, false,
            ));

            let mut motion_chain = ActionChain::new();
            motion_chain.add_delay_seconds(w, f64::from(anim_length));
            motion_chain.add_action(Actor::Object(wo), move |w| {
                if !w.objects.contains(this) {
                    return;
                }
                let wo = world_object(w, this);
                // FIXME: better cycle handling
                let cmd = obj(w, wo)
                    .wo
                    .world_object_properties
                    .current_motion_state
                    .as_ref()
                    .map(|m| m.motion_state.forward_command)
                    .unwrap_or_else(|| {
                        panic!("System.NullReferenceException: WorldObject.CurrentMotionState")
                    });
                if cmd != MotionCommand::Dead
                    && cmd != MotionCommand::Sleeping
                    && cmd != MotionCommand::Sitting
                    && !cmd.to_dotnet_string().ends_with("State")
                {
                    if debug_motion {
                        log::debug!("{} running starting motion again", name(w, wo));
                    }

                    world_object::execute_motion(w, wo, starting_motion, true, None, false);
                }
            });
            motion_chain.enqueue_chain(w);

            if debug_motion {
                log::debug!(
                    "{} appending time to existing chain: {anim_length}",
                    name(w, wo)
                );
            }
        }
    } else {
        // vendor / other motions
        let starting_motion = Motion::new(MotionStance::NonCombat, MotionCommand::Ready, 1.0);
        let emote_motion = value(emote.motion, "emote.Motion");
        let anim_length = motion_table_get_animation_length(
            w,
            wo,
            current.stance,
            emote_motion,
            Some(MotionCommand::Ready),
        );

        let motion = Motion::new(MotionStance::NonCombat, emote_motion, emote.extent);

        if debug_motion {
            log::debug!(
                "{} running motion (block 2) NonCombat, {:?}",
                name(w, wo),
                emote.motion
            );
        }

        delay = Some(world_object::execute_motion(
            w, wo, motion, true, None, false,
        ));

        let mut motion_chain = ActionChain::new();
        motion_chain.add_delay_seconds(w, f64::from(anim_length));
        motion_chain.add_action(Actor::Object(wo), move |w| {
            if !w.objects.contains(this) {
                return;
            }
            let wo = world_object(w, this);
            world_object::execute_motion(w, wo, starting_motion, false, None, false);
        });

        motion_chain.enqueue_chain(w);
    }

    delay
}

/// `DatManager.PortalDat.ReadFromDat<MotionTable>(WorldObject.MotionTableId).GetAnimationLength(stance,
/// motion, currentMotion)`.
fn motion_table_get_animation_length(
    w: &World,
    wo: ObjectGuid,
    stance: MotionStance,
    motion: MotionCommand,
    current_motion: Option<MotionCommand>,
) -> f32 {
    let id = obj(w, wo).motion_table_id();
    let mt = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
    crate::physics::motion_table::get_animation_length_in(
        w,
        mt.as_deref(),
        stance,
        motion,
        current_motion,
    )
}

/// Selects an emote set based on category, and optional: quest, vendor, rng. ACE's defaults:
/// `questName = null`, `vendorType = null`, `wcid = null`, `useRNG = true`. The set is a copy of
/// the biota's record.
// ACE: EmoteManager.GetEmoteSet
pub fn get_emote_set(
    w: &mut World,
    this: ObjectGuid,
    category: EmoteCategory,
    quest_name: Option<&str>,
    vendor_type: Option<VendorType>,
    wcid: Option<u32>,
    use_rng: bool,
) -> Option<PropertiesEmote> {
    //if (Debug) Console.WriteLine($"{WorldObject.Name}.EmoteManager.GetEmoteSet({category}, {questName}, {vendorType}, {wcid}, {useRNG})");

    // always pull emoteSet from _worldObject
    let owner = this;
    let all = obj(w, owner).biota.properties_emote.clone()?;
    let mut emote_set: Vec<&PropertiesEmote> =
        all.iter().filter(|e| e.category == category).collect();

    // optional criteria
    if (category == EmoteCategory::HearChat || category == EmoteCategory::ReceiveTalkDirect)
        && quest_name.is_some()
    {
        let q = quest_name.unwrap_or("");
        emote_set.retain(|e| {
            e.quest
                .as_deref()
                .is_some_and(|eq| equals_ignore_case(eq, q))
                || e.quest.is_none()
        });
    } else if let Some(q) = quest_name {
        emote_set.retain(|e| {
            e.quest
                .as_deref()
                .is_some_and(|eq| equals_ignore_case(eq, q))
        });
    }
    if let Some(vendor_type) = vendor_type {
        emote_set.retain(|e| e.vendor_type == Some(vendor_type));
    }
    if let Some(wcid) = wcid {
        emote_set.retain(|e| e.weenie_class_id == Some(wcid));
    }

    if category == EmoteCategory::HeartBeat {
        let (current_stance, current_motion) =
            world_object::get_current_motion_state(w, world_object(w, this));

        emote_set.retain(|e| e.style.is_none_or(|s| s == current_stance));
        emote_set.retain(|e| e.substyle.is_none_or(|s| s == current_motion));
    }

    if category == EmoteCategory::WoundedTaunt && obj(w, owner).is_creature() {
        let health = obj(w, owner).health();
        let percent = health.percent(&mut StatCtx::in_world(w, owner));
        // a null MinHealth / MaxHealth compares false
        emote_set.retain(|e| {
            e.min_health.is_some_and(|min| percent >= min)
                && e.max_health.is_some_and(|max| percent <= max)
        });
    }

    if use_rng {
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);
        emote_set.retain(|e| f64::from(e.probability) > rng);
        // `OrderBy` is a stable sort
        emote_set.sort_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        //emoteSet = emoteSet.Where(e => e.Probability >= rng);
    }

    emote_set.first().map(|e| (*e).clone())
}

/// Convenience wrapper between GetEmoteSet and ExecuteEmoteSet (`ExecuteEmoteSet(EmoteCategory
/// category, string quest = null, WorldObject targetObject = null, bool nested = false)`).
// ACE: EmoteManager.ExecuteEmoteSet
pub fn execute_emote_set_category(
    w: &mut World,
    this: ObjectGuid,
    category: EmoteCategory,
    quest: Option<&str>,
    target_object: Option<ObjectGuid>,
    nested: bool,
) {
    //if (Debug) Console.WriteLine($"{WorldObject.Name}.EmoteManager.ExecuteEmoteSet({category}, {quest}, {targetObject}, {nested})");

    let Some(emote_set) = get_emote_set(w, this, category, quest, None, None, true) else {
        return;
    };

    // TODO: revisit if nested chains need to propagate timers
    execute_emote_set_arc(w, this, Some(Arc::new(emote_set)), target_object, nested);
}

/// Executes a set of emotes to run with delays (`ExecuteEmoteSet(PropertiesEmote emoteSet,
/// WorldObject targetObject = null, bool nested = false)`). `emote_set` may be null, as in ACE.
// ACE: EmoteManager.ExecuteEmoteSet
pub fn execute_emote_set(
    w: &mut World,
    this: ObjectGuid,
    emote_set: Option<&PropertiesEmote>,
    target_object: Option<ObjectGuid>,
    nested: bool,
) -> bool {
    execute_emote_set_arc(
        w,
        this,
        emote_set.map(|e| Arc::new(e.clone())),
        target_object,
        nested,
    )
}

fn execute_emote_set_arc(
    w: &mut World,
    this: ObjectGuid,
    emote_set: Option<Arc<PropertiesEmote>>,
    target_object: Option<ObjectGuid>,
    nested: bool,
) -> bool {
    //if (Debug) Console.WriteLine($"{WorldObject.Name}.EmoteManager.ExecuteEmoteSet({emoteSet}, {targetObject}, {nested})");

    // detect busy state
    // TODO: maybe eventually we should consider having categories that can be queued?
    // there are some categories that shouldn't be queued, like heartbeats...
    if em(w, this).is_busy && !nested {
        return false;
    }

    // start action chain
    let e = em_mut(w, this);
    e.nested = e.nested.wrapping_add(1);
    enqueue(w, this, emote_set, target_object, 0, 0.0);

    true
}

/// `Nested--; if (Nested == 0) IsBusy = false;`
fn end_nested(w: &mut World, this: ObjectGuid) {
    let e = em_mut(w, this);
    e.nested = e.nested.wrapping_sub(1);

    if e.nested == 0 {
        e.is_busy = false;
    }
}

/// ACE's defaults: `emoteIdx = 0`, `delay = 0.0f`.
// ACE: EmoteManager.Enqueue
pub fn enqueue(
    w: &mut World,
    this: ObjectGuid,
    emote_set: Option<Arc<PropertiesEmote>>,
    target_object: Option<ObjectGuid>,
    emote_idx: usize,
    delay: f32,
) {
    //if (Debug) Console.WriteLine($"{WorldObject.Name}.EmoteManager.Enqueue({emoteSet}, {targetObject}, {emoteIdx}, {delay})");

    let Some(emote_set) = emote_set else {
        let e = em_mut(w, this);
        e.nested = e.nested.wrapping_sub(1);
        return;
    };

    em_mut(w, this).is_busy = true;

    // ACE-BUG: `ElementAt(emoteIdx)` throws ArgumentOutOfRangeException for an emote set with no actions, after
    // IsBusy was set and Nested incremented, so the object stays busy (refusing Give and every non-nested emote)
    // until an OnPortal or OnDeath clears IsBusy.
    let emote = emote_set
        .properties_emote_action
        .get(emote_idx)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "System.ArgumentOutOfRangeException: PropertiesEmoteAction.ElementAt({emote_idx})"
            )
        });

    if em(w, this).nested > 75
        && emote_set.quest.as_deref().is_some_and(|q| !q.is_empty())
        && emote_set.quest == emote.message
        && emote_is_branching_type(Some(&emote))
    {
        let mut emote_stack = format!(
            "{:?}: {}\n",
            emote_set.category,
            emote_set.quest.as_deref().unwrap_or("")
        );
        // ACE-BUG: the loop prints the current `emote` once per action instead of each action `e`, so the
        // infinite-loop log repeats one line (log text only).
        for _e in &emote_set.properties_emote_action {
            let m = emote
                .message
                .as_deref()
                .filter(|m| !m.is_empty())
                .map_or(String::new(), |m| format!(": {m}"));
            emote_stack += &format!("       - {:?}{m}\n", EmoteType(emote.r#type.cs_cast()));
        }

        let wo = world_object(w, this);
        log::error!(
            "[EMOTE] {}.EmoteManager.Enqueue(): Nested > 75, possible Infinite loop detected and aborted on 0x{}:{}\n-> {emote_stack}",
            name(w, wo),
            wo,
            weenie_class_id(w, wo)
        );

        end_nested(w, this);

        return;
    }

    if delay + emote.delay > 0.0 {
        let wo = world_object(w, this);
        let mut action_chain = ActionChain::new();

        if em(w, this).debug {
            let d = emote.delay;
            action_chain.add_action(Actor::Object(wo), move |_| log::debug!("{d} - "));
        }

        // delay = post-delay from actual time of previous emote
        // emote.Delay = pre-delay for current emote
        action_chain.add_delay_seconds(w, f64::from(delay + emote.delay));

        action_chain.add_action(Actor::Object(wo), move |w| {
            if w.objects.contains(this) {
                do_enqueue(w, this, &emote_set, target_object, emote_idx, &emote);
            }
        });
        action_chain.enqueue_chain(w);
    } else {
        do_enqueue(w, this, &emote_set, target_object, emote_idx, &emote);
    }
}

/// This should only be called by Enqueue.
// ACE: EmoteManager.DoEnqueue
fn do_enqueue(
    w: &mut World,
    this: ObjectGuid,
    emote_set: &Arc<PropertiesEmote>,
    target_object: Option<ObjectGuid>,
    emote_idx: usize,
    emote: &PropertiesEmoteAction,
) {
    if em(w, this).debug {
        log::debug!("{:?}", EmoteType(emote.r#type.cs_cast()));
    }

    // A target that has left the world reads as null from here on.
    let target_object = target_object.filter(|&t| w.objects.contains(t));

    let next_delay = execute_emote(w, this, emote_set, emote, target_object);

    if em(w, this).debug {
        log::debug!(" - {next_delay}");
    }

    if emote_idx + 1 < emote_set.properties_emote_action.len() {
        enqueue(
            w,
            this,
            Some(Arc::clone(emote_set)),
            target_object,
            emote_idx + 1,
            next_delay,
        );
    } else if next_delay > 0.0 {
        let wo = world_object(w, this);
        let mut delay_chain = ActionChain::new();
        delay_chain.add_delay_seconds(w, f64::from(next_delay));
        delay_chain.add_action(Actor::Object(wo), move |w| {
            if w.objects.contains(this) {
                end_nested(w, this);
            }
        });
        delay_chain.enqueue_chain(w);
    } else {
        end_nested(w, this);
    }
}

// ACE: EmoteManager.EmoteIsBranchingType
fn emote_is_branching_type(emote: Option<&PropertiesEmoteAction>) -> bool {
    let Some(emote) = emote else { return false };

    let emote_type = EmoteType(emote.r#type.cs_cast());

    matches!(
        emote_type,
        EmoteType::UpdateQuest
            | EmoteType::InqQuest
            | EmoteType::InqQuestSolves
            | EmoteType::InqBoolStat
            | EmoteType::InqIntStat
            | EmoteType::InqFloatStat
            | EmoteType::InqStringStat
            | EmoteType::InqAttributeStat
            | EmoteType::InqRawAttributeStat
            | EmoteType::InqSecondaryAttributeStat
            | EmoteType::InqRawSecondaryAttributeStat
            | EmoteType::InqSkillStat
            | EmoteType::InqRawSkillStat
            | EmoteType::InqSkillTrained
            | EmoteType::InqSkillSpecialized
            | EmoteType::InqEvent
            | EmoteType::InqFellowQuest
            | EmoteType::InqFellowNum
            | EmoteType::UpdateFellowQuest
            | EmoteType::Goto
            | EmoteType::InqNumCharacterTitles
            | EmoteType::InqYesNo
            | EmoteType::InqOwnsItems
            | EmoteType::UpdateMyQuest
            | EmoteType::InqMyQuest
            | EmoteType::InqMyQuestSolves
            | EmoteType::InqPackSpace
            | EmoteType::InqQuestBitsOn
            | EmoteType::InqQuestBitsOff
            | EmoteType::InqMyQuestBitsOn
            | EmoteType::InqMyQuestBitsOff
            | EmoteType::InqInt64Stat
            | EmoteType::InqContractsFull
    )
}

// ACE: EmoteManager.HasValidTestNoQuality
pub fn has_valid_test_no_quality(w: &mut World, this: ObjectGuid, test_name: Option<&str>) -> bool {
    get_emote_set(
        w,
        this,
        EmoteCategory::TestNoQuality,
        test_name,
        None,
        None,
        true,
    )
    .is_some()
}

// ACE: EmoteManager.HasValidTestNoFellow
pub fn has_valid_test_no_fellow(w: &mut World, this: ObjectGuid, test_name: Option<&str>) -> bool {
    get_emote_set(
        w,
        this,
        EmoteCategory::TestNoFellow,
        test_name,
        None,
        None,
        true,
    )
    .is_some()
}

// ACE: EmoteManager.DoVendorEmote
pub fn do_vendor_emote(
    w: &mut World,
    this: ObjectGuid,
    vendor_type: VendorType,
    target: Option<ObjectGuid>,
) {
    let vendor_set = get_emote_set(
        w,
        this,
        EmoteCategory::Vendor,
        None,
        Some(vendor_type),
        None,
        true,
    );
    let heartbeat_set = get_emote_set(
        w,
        this,
        EmoteCategory::Vendor,
        None,
        Some(VendorType::Heartbeat),
        None,
        true,
    );

    execute_emote_set_arc(w, this, vendor_set.map(Arc::new), target, false);
    execute_emote_set_arc(w, this, heartbeat_set.map(Arc::new), target, true);
}

/// The emote sets of `emote_category`, from `WorldObject` (the proxy, when set).
///
/// # Panics
/// When the object has no emote table (ACE: `ArgumentNullException` from `Where`).
// ACE: EmoteManager.Emotes
#[must_use]
pub fn emotes(w: &World, this: ObjectGuid, emote_category: EmoteCategory) -> Vec<PropertiesEmote> {
    let wo = world_object(w, this);
    let all = obj(w, wo)
        .biota
        .properties_emote
        .as_ref()
        .unwrap_or_else(|| {
            panic!("System.ArgumentNullException: WorldObject.Biota.PropertiesEmote")
        });
    all.iter()
        .filter(|x| x.category == emote_category)
        .cloned()
        .collect()
}

/// Replaces the emote text tokens (`%n`, `%s`, levels, templates, heritage, the quest timers, ...).
/// `source` is the emoting object in all of ACE's calls; `None` stands for ACE's `null`.
// ACE: EmoteManager.Replace
#[must_use]
pub fn replace(
    w: &World,
    this: ObjectGuid,
    message: Option<&str>,
    source: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
    quest: Option<&str>,
) -> String {
    let Some(message) = message else {
        // ACE-BUG: the warning dereferences source and target, so a null message with no target (for example a
        // heartbeat Say with no text) throws NullReferenceException instead of returning "".
        let source = source.unwrap_or_else(|| {
            panic!("System.NullReferenceException: source.Name in EmoteManager.Replace")
        });
        let target = target.unwrap_or_else(|| {
            panic!("System.NullReferenceException: target.Name in EmoteManager.Replace")
        });
        let wo = world_object(w, this);
        log::warn!(
            "[EMOTE] {}.EmoteManager.Replace(message, {}:0x{}:{}, {}:0x{}:{}, {}): message was null!",
            name(w, wo),
            name(w, source),
            source,
            weenie_class_id(w, source),
            name(w, target),
            target,
            weenie_class_id(w, target),
            quest.unwrap_or("")
        );
        return String::new();
    };

    let mut result = message.to_owned();

    let source_name = source.map_or_else(String::new, |s| name(w, s));
    let target_name = target.map_or_else(String::new, |t| name(w, t));

    result = result.replace("%n", &source_name);
    result = result.replace("%mn", &source_name);
    result = result.replace("%s", &target_name);
    result = result.replace("%tn", &target_name);

    let source_level =
        source.map_or_else(String::new, |s| obj(w, s).level().unwrap_or(0).to_string());
    let target_level =
        target.map_or_else(String::new, |t| obj(w, t).level().unwrap_or(0).to_string());
    result = result.replace("%ml", &source_level);
    result = result.replace("%tl", &target_level);

    //var sourceTemplate = source != null ? source.GetProperty(PropertyString.Title) : "";
    //var targetTemplate = source != null ? target.GetProperty(PropertyString.Title) : "";
    let source_template = source
        .and_then(|s| obj(w, s).get_property(PropertyString::Template))
        .unwrap_or_default();
    let target_template = target
        .and_then(|t| obj(w, t).get_property(PropertyString::Template))
        .unwrap_or_default();
    result = result.replace("%mt", &source_template);
    result = result.replace("%tt", &target_template);

    let source_heritage = source
        .and_then(|s| obj(w, s).heritage_group_name())
        .unwrap_or_default();
    let target_heritage = target
        .and_then(|t| obj(w, t).heritage_group_name())
        .unwrap_or_default();
    result = result.replace("%mh", &source_heritage);
    result = result.replace("%th", &target_heritage);

    //result = result.Replace("%mf", $"{source.GetProperty(PropertyString.Fellowship)}");
    //result = result.Replace("%tf", $"{target.GetProperty(PropertyString.Fellowship)}");

    //result = result.Replace("%l", $"{???}"); // level?
    //result = result.Replace("%pk", $"{???}"); // pk status?
    //result = result.Replace("%a", $"{???}"); // allegiance?
    //result = result.Replace("%p", $"{???}"); // patron?

    // Find quest in standard or LSD custom usage for %tqt and %CDtime
    let embedded_quest_name = if result.contains('@') {
        message.split('@').next()
    } else {
        None
    };
    let quest_name = if is_null_or_white_space(embedded_quest_name) {
        quest
    } else {
        embedded_quest_name
    };
    let quest_name_text = quest_name.unwrap_or("");

    // LSD custom tqt usage
    result = replace_ignore_case(
        &result,
        &format!("{quest_name_text}@%tqt"),
        "You may complete this quest again in %tqt.",
    );

    // LSD custom CDtime variable
    if result.contains("%CDtime") {
        result = replace_ignore_case(&result, &format!("{quest_name_text}@"), "");
    }

    let has_quest = !is_null_or_white_space(quest);

    if let Some(target_player) = as_player(w, target) {
        let tqt = if has_quest {
            time_span_extensions::get_friendly_string(shims::quest_manager_get_next_solve_time(
                w,
                target_player,
                quest_name,
            ))
        } else {
            String::new()
        };
        result = result.replace("%tqt", &tqt);

        let cd_time = if has_quest {
            time_span_extensions::get_friendly_string(shims::quest_manager_get_next_solve_time(
                w,
                target_player,
                quest_name,
            ))
        } else {
            String::new()
        };
        result = result.replace("%CDtime", &cd_time);

        let tf = if shims::player_has_fellowship(w, target_player) {
            shims::fellowship_fellowship_name(w, target_player)
        } else {
            String::new()
        };
        result = result.replace("%tf", &tf);

        let fqt = if has_quest && shims::player_has_fellowship(w, target_player) {
            time_span_extensions::get_friendly_string(
                shims::fellowship_quest_manager_get_next_solve_time(w, target_player, quest_name),
            )
        } else {
            String::new()
        };
        result = result.replace("%fqt", &fqt);

        let tqm = if has_quest {
            shims::quest_manager_get_max_solves(w, target_player, quest_name).to_string()
        } else {
            String::new()
        };
        result = result.replace("%tqm", &tqm);

        let tqc = if has_quest {
            shims::quest_manager_get_current_solves(w, target_player, quest_name).to_string()
        } else {
            String::new()
        };
        result = result.replace("%tqc", &tqc);
    }

    if let Some(source_creature) = as_creature(w, source) {
        let mqt = if has_quest {
            time_span_extensions::get_friendly_string(shims::quest_manager_get_next_solve_time(
                w,
                source_creature,
                quest_name,
            ))
        } else {
            String::new()
        };
        result = result.replace("%mqt", &mqt);

        let mxqt = if has_quest {
            time_span_extensions::get_friendly_long_string(
                shims::quest_manager_get_next_solve_time(w, source_creature, quest_name),
            )
        } else {
            String::new()
        };
        result = result.replace("%mxqt", &mxqt);

        //result = result.Replace("%CDtime", !string.IsNullOrWhiteSpace(quest) ? sourceCreature.QuestManager.GetNextSolveTime(questName).GetFriendlyString() : "");

        let mqc = if has_quest {
            shims::quest_manager_get_current_solves(w, source_creature, quest_name).to_string()
        } else {
            String::new()
        };
        result = result.replace("%mqc", &mqc);
    }

    result
}

/// Returns the creature target for quest emotes.
// ACE: EmoteManager.GetQuestTarget
#[must_use]
pub fn get_quest_target(
    emote: EmoteType,
    target: Option<ObjectGuid>,
    self_: Option<ObjectGuid>,
) -> Option<ObjectGuid> {
    match emote {
        // MyQuest always targets self
        EmoteType::DecrementMyQuest
        | EmoteType::EraseMyQuest
        | EmoteType::IncrementMyQuest
        | EmoteType::InqMyQuest
        | EmoteType::InqMyQuestBitsOff
        | EmoteType::InqMyQuestBitsOn
        | EmoteType::InqMyQuestSolves
        | EmoteType::SetMyQuestBitsOff
        | EmoteType::SetMyQuestBitsOn
        | EmoteType::SetMyQuestCompletions
        | EmoteType::StampMyQuest
        | EmoteType::UpdateMyQuest => self_,

        _ => target.or(self_),
    }
}

// ACE: EmoteManager.GetSpellTarget
fn get_spell_target(
    w: &World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
) -> Option<ObjectGuid> {
    let target_self = (spell.flags() & SpellFlags::SelfTargeted) == SpellFlags::SelfTargeted;
    let untargeted = spell.non_component_target_type() == ItemType::None;

    let mut spell_target = target;
    if untargeted {
        spell_target = None;
    } else if target_self {
        spell_target = Some(world_object(w, this));
    }

    spell_target
}

// ACE: EmoteManager.HeartBeat
pub fn heart_beat(w: &mut World, this: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    let wo = world_object(w, this);
    // player didn't do idle emotes in retail?
    if obj(w, wo).is_player() {
        return;
    }

    if obj(w, wo).is_creature() && shims::creature_is_awake(w, wo) {
        return;
    }

    execute_emote_set_category(w, this, EmoteCategory::HeartBeat, None, None, false);
}

/// `this` is the object used. Every hook does nothing for an object that has left the world.
// ACE: EmoteManager.OnUse
pub fn on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Use, None, Some(activator), false);
}

// ACE: EmoteManager.OnPortal
pub fn on_portal(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    em_mut(w, this).is_busy = false;

    execute_emote_set_category(w, this, EmoteCategory::Portal, None, Some(activator), false);
}

// ACE: EmoteManager.OnActivation
pub fn on_activation(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(
        w,
        this,
        EmoteCategory::Activation,
        None,
        Some(activator),
        false,
    );
}

// ACE: EmoteManager.OnGeneration
pub fn on_generation(w: &mut World, this: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Generation, None, None, false);
}

// ACE: EmoteManager.OnWield
pub fn on_wield(w: &mut World, this: ObjectGuid, wielder: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Wield, None, Some(wielder), false);
}

// ACE: EmoteManager.OnUnwield
pub fn on_unwield(w: &mut World, this: ObjectGuid, wielder: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::UnWield, None, Some(wielder), false);
}

// ACE: EmoteManager.OnPickup
pub fn on_pickup(w: &mut World, this: ObjectGuid, initiator: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::PickUp, None, Some(initiator), false);
}

// ACE: EmoteManager.OnDrop
pub fn on_drop(w: &mut World, this: ObjectGuid, dropper: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Drop, None, Some(dropper), false);
}

/// Called when an idle mob becomes alerted by a player and initially wakes up. `target` is
/// `AttackTarget as Creature` (`None` for null).
// ACE: EmoteManager.OnWakeUp
pub fn on_wake_up(w: &mut World, this: ObjectGuid, target: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Scream, None, target, false);
}

/// Called when a monster switches targets.
// ACE: EmoteManager.OnNewEnemy
pub fn on_new_enemy(w: &mut World, this: ObjectGuid, new_enemy: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::NewEnemy, None, new_enemy, false);
}

/// Called when a monster completes an attack.
// ACE: EmoteManager.OnAttack
pub fn on_attack(w: &mut World, this: ObjectGuid, target: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Taunt, None, target, false);
}

// ACE: EmoteManager.OnDamage
pub fn on_damage(w: &mut World, this: ObjectGuid, attacker: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::WoundedTaunt, None, attacker, false);
}

// ACE: EmoteManager.OnReceiveCritical
pub fn on_receive_critical(w: &mut World, this: ObjectGuid, attacker: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(
        w,
        this,
        EmoteCategory::ReceiveCritical,
        None,
        attacker,
        false,
    );
}

// ACE: EmoteManager.OnResistSpell
pub fn on_resist_spell(w: &mut World, this: ObjectGuid, attacker: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::ResistSpell, None, attacker, false);
}

// ACE: EmoteManager.OnDeath
pub fn on_death(w: &mut World, this: ObjectGuid, last_damager_info: Option<&DamageHistoryInfo>) {
    if !w.objects.contains(this) {
        return;
    }
    if get_emote_set(w, this, EmoteCategory::Death, None, None, None, true).is_none() {
        return;
    }

    em_mut(w, this).is_busy = false;

    let last_damager = last_damager_info.and_then(|l| l.try_get_pet_owner_or_attacker(w));

    execute_emote_set_category(w, this, EmoteCategory::Death, None, last_damager, false);
}

/// Called when a monster kills a player.
// ACE: EmoteManager.OnKill
pub fn on_kill(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::KillTaunt, None, Some(player), false);
}

/// Called when player interacts with item that has a Quest string.
// ACE: EmoteManager.OnQuest
pub fn on_quest(w: &mut World, this: ObjectGuid, initiator: ObjectGuid) {
    if !w.objects.contains(this) {
        return;
    }
    let wo = world_object(w, this);
    let quest_name = obj(w, wo).quest();
    let quest_name = quest_name.as_deref();

    let has_quest = shims::quest_manager_has_quest(w, initiator, quest_name);

    if has_quest {
        // update existing quest
        let can_solve = shims::quest_manager_can_solve(w, initiator, quest_name);
        if can_solve {
            shims::quest_manager_stamp(w, initiator, quest_name);
        }
        execute_emote_set_category(
            w,
            this,
            quest_result(can_solve),
            quest_name,
            Some(initiator),
            false,
        );
    } else {
        // add new quest
        shims::quest_manager_update(w, initiator, quest_name);
        let has_quest = shims::quest_manager_has_quest(w, initiator, quest_name);
        execute_emote_set_category(
            w,
            this,
            quest_result(has_quest),
            quest_name,
            Some(initiator),
            false,
        );
    }
}

/// Called when this NPC receives a direct text message from a player.
// ACE: EmoteManager.OnTalkDirect
pub fn on_talk_direct(w: &mut World, this: ObjectGuid, player: ObjectGuid, message: &str) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(
        w,
        this,
        EmoteCategory::ReceiveTalkDirect,
        Some(message),
        Some(player),
        false,
    );
}

/// Called when this NPC receives a local signal from a player.
// ACE: EmoteManager.OnLocalSignal
pub fn on_local_signal(w: &mut World, this: ObjectGuid, emitter: ObjectGuid, message: &str) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(
        w,
        this,
        EmoteCategory::ReceiveLocalSignal,
        Some(message),
        Some(emitter),
        false,
    );
}

/// Called when monster exceeds the maximum distance from home position.
// ACE: EmoteManager.OnHomeSick
pub fn on_home_sick(w: &mut World, this: ObjectGuid, attack_target: Option<ObjectGuid>) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(w, this, EmoteCategory::Homesick, None, attack_target, false);
}

/// Called when this NPC hears local chat from a player.
// ACE: EmoteManager.OnHearChat
pub fn on_hear_chat(w: &mut World, this: ObjectGuid, player: ObjectGuid, message: &str) {
    if !w.objects.contains(this) {
        return;
    }
    execute_emote_set_category(
        w,
        this,
        EmoteCategory::HearChat,
        Some(message),
        Some(player),
        false,
    );
}

//public bool HasAntennas => WorldObject.Biota.BiotaPropertiesEmote.Count(x => x.Category == (int)EmoteCategory.ReceiveLocalSignal) > 0;

/// Call this function when WorldObject is being used via a proxy object, e.g.: Hooker on a Hook.
// ACE: EmoteManager.SetProxy
pub fn set_proxy(w: &mut World, this: ObjectGuid, world_object: ObjectGuid) {
    em_mut(w, this).proxy = Some(world_object);
}

/// Called when this object is removed from the proxy object (Hooker is picked up from Hook).
// ACE: EmoteManager.ClearProxy
pub fn clear_proxy(w: &mut World, this: ObjectGuid) {
    em_mut(w, this).proxy = None;
}

/// `target.EmoteManager.IsBusy`; an object that has left the world is not busy.
#[must_use]
pub fn is_busy(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .is_some_and(|o| o.wo.world_object.emote_manager.is_busy)
}

// ================================================================================ shims

/// Callees in other ACE files, by ACE's names: each points at its port, or is a `not_ported!`
/// pointer answering the type's default.
pub mod shims {

    use super::{CharacterTitle, ObjectGuid, Position, ShareType, Spell, TimeSpan, World, XpType};

    // ---- QuestManager: `creature.QuestManager.X(...)`, over `quest_manager.rs`. ACE passes
    // `emote.Message`, which may be null: `QuestManager.GetQuestName` then throws.

    use crate::managers::quest_manager::{self, QuestOwner};

    /// `questFormat.IndexOf('@')` on a null string: the `NullReferenceException`.
    fn quest_format(quest_name: Option<&str>) -> &str {
        quest_name.expect("NullReferenceException: QuestManager.GetQuestName(null)")
    }

    pub fn quest_manager_decrement(
        w: &mut World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
        amount: i32,
    ) {
        quest_manager::decrement(
            w,
            &mut QuestOwner::Creature(creature),
            quest_format(quest_name),
            amount,
        );
    }

    pub fn quest_manager_erase(w: &mut World, creature: ObjectGuid, quest_name: Option<&str>) {
        quest_manager::erase(
            w,
            &mut QuestOwner::Creature(creature),
            quest_format(quest_name),
        );
    }

    pub fn quest_manager_increment(
        w: &mut World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
        amount: i32,
    ) {
        quest_manager::increment(
            w,
            &mut QuestOwner::Creature(creature),
            quest_format(quest_name),
            amount,
        );
    }

    pub fn quest_manager_has_quest(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
    ) -> bool {
        quest_manager::has_quest(w, &QuestOwner::Creature(creature), quest_format(quest_name))
    }

    pub fn quest_manager_can_solve(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
    ) -> bool {
        quest_manager::can_solve(w, &QuestOwner::Creature(creature), quest_format(quest_name))
    }

    pub fn quest_manager_has_no_quest_bits(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
        bits: i32,
    ) -> bool {
        quest_manager::has_no_quest_bits(
            w,
            &QuestOwner::Creature(creature),
            quest_format(quest_name),
            bits,
        )
    }

    pub fn quest_manager_has_quest_bits(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
        bits: i32,
    ) -> bool {
        quest_manager::has_quest_bits(
            w,
            &QuestOwner::Creature(creature),
            quest_format(quest_name),
            bits,
        )
    }

    pub fn quest_manager_has_quest_solves(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
        min: Option<i32>,
        max: Option<i32>,
    ) -> bool {
        quest_manager::has_quest_solves(
            w,
            &QuestOwner::Creature(creature),
            quest_format(quest_name),
            min,
            max,
        )
    }

    pub fn quest_manager_set_quest_bits(
        w: &mut World,
        creature: ObjectGuid,
        quest_name: &str,
        bits: i32,
        on: bool,
    ) {
        quest_manager::set_quest_bits(w, &mut QuestOwner::Creature(creature), quest_name, bits, on);
    }

    pub fn quest_manager_set_quest_completions(
        w: &mut World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
        amount: i32,
    ) {
        quest_manager::set_quest_completions(
            w,
            &mut QuestOwner::Creature(creature),
            quest_format(quest_name),
            amount,
        );
    }

    pub fn quest_manager_stamp(w: &mut World, creature: ObjectGuid, quest_name: Option<&str>) {
        quest_manager::stamp(
            w,
            &mut QuestOwner::Creature(creature),
            quest_format(quest_name),
        );
    }

    pub fn quest_manager_update(w: &mut World, creature: ObjectGuid, quest_name: Option<&str>) {
        quest_manager::update(
            w,
            &mut QuestOwner::Creature(creature),
            quest_format(quest_name),
        );
    }

    pub fn quest_manager_get_next_solve_time(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
    ) -> TimeSpan {
        quest_manager::get_next_solve_time(
            w,
            &QuestOwner::Creature(creature),
            quest_format(quest_name),
        )
    }

    /// `QuestManager.GetMaxSolves` reads only the world's quest table.
    pub fn quest_manager_get_max_solves(
        w: &World,
        _creature: ObjectGuid,
        quest_name: Option<&str>,
    ) -> i32 {
        quest_manager::get_max_solves(w, quest_format(quest_name))
    }

    pub fn quest_manager_get_current_solves(
        w: &World,
        creature: ObjectGuid,
        quest_name: Option<&str>,
    ) -> i32 {
        quest_manager::get_current_solves(
            w,
            &QuestOwner::Creature(creature),
            quest_format(quest_name),
        )
    }

    // ---- Fellowship (`Entity/Fellowship.cs`, `Player_Fellowship.cs`): `player.Fellowship` and its
    // members; its QuestManager is lent through `FellowshipRef::with_quest_manager` / `read_quest_manager`.

    /// `player.Fellowship`, which every caller has checked for null first.
    fn fellowship_of(w: &World, player: ObjectGuid) -> crate::entity::fellowship::FellowshipRef {
        crate::world_objects::player_fellowship::fellowship(w, player)
            .expect("ACE: player.Fellowship is null (NullReferenceException)")
    }

    /// `player.Fellowship != null`.
    pub fn player_has_fellowship(w: &World, player: ObjectGuid) -> bool {
        crate::world_objects::player_fellowship::fellowship(w, player).is_some()
    }

    /// `player.Fellowship.BroadcastToFellow(message)`.
    pub fn fellowship_broadcast_to_fellow(w: &mut World, player: ObjectGuid, message: &str) {
        let f = fellowship_of(w, player);
        crate::entity::fellowship::broadcast_to_fellow(w, &f, message);
    }

    /// `player.Fellowship.TellFellow(sender, message)`.
    pub fn fellowship_tell_fellow(
        w: &mut World,
        player: ObjectGuid,
        sender: ObjectGuid,
        message: &str,
    ) {
        let f = fellowship_of(w, player);
        crate::entity::fellowship::tell_fellow(w, &f, sender, message);
    }

    /// `player.Fellowship.GetFellowshipMembers().Count`.
    pub fn fellowship_get_fellowship_members_count(w: &mut World, player: ObjectGuid) -> i32 {
        let f = fellowship_of(w, player);
        i32::try_from(crate::entity::fellowship::get_fellowship_members(w, &f).len())
            .expect("a Dictionary.Count is an int")
    }

    /// `player.Fellowship.FellowshipName`.
    pub fn fellowship_fellowship_name(w: &World, player: ObjectGuid) -> String {
        fellowship_of(w, player).get(w).fellowship_name.clone()
    }

    pub fn fellowship_quest_manager_has_quest(
        w: &World,
        player: ObjectGuid,
        quest_name: Option<&str>,
    ) -> bool {
        fellowship_of(w, player).read_quest_manager(w, |qm| {
            quest_manager::has_quest(w, &QuestOwner::Fellowship(qm), quest_format(quest_name))
        })
    }

    pub fn fellowship_quest_manager_can_solve(
        w: &World,
        player: ObjectGuid,
        quest_name: Option<&str>,
    ) -> bool {
        fellowship_of(w, player).read_quest_manager(w, |qm| {
            quest_manager::can_solve(w, &QuestOwner::Fellowship(qm), quest_format(quest_name))
        })
    }

    pub fn fellowship_quest_manager_stamp(
        w: &mut World,
        player: ObjectGuid,
        quest_name: Option<&str>,
    ) {
        fellowship_of(w, player).with_quest_manager(w, |w, qm| {
            quest_manager::stamp(w, &mut QuestOwner::Fellowship(qm), quest_format(quest_name))
        });
    }

    pub fn fellowship_quest_manager_update(
        w: &mut World,
        player: ObjectGuid,
        quest_name: Option<&str>,
    ) {
        fellowship_of(w, player).with_quest_manager(w, |w, qm| {
            quest_manager::update(w, &mut QuestOwner::Fellowship(qm), quest_format(quest_name))
        });
    }

    pub fn fellowship_quest_manager_get_next_solve_time(
        w: &World,
        player: ObjectGuid,
        quest_name: Option<&str>,
    ) -> TimeSpan {
        fellowship_of(w, player).read_quest_manager(w, |qm| {
            quest_manager::get_next_solve_time(
                w,
                &QuestOwner::Fellowship(qm),
                quest_format(quest_name),
            )
        })
    }

    /// `player.HandleActionFellowshipChangeLock(lockState, lockName)` (`Player_Fellowship.cs`).
    pub fn player_handle_action_fellowship_change_lock(
        w: &mut World,
        player: ObjectGuid,
        lock_state: bool,
        lock_name: Option<&str>,
    ) {
        crate::world_objects::player_fellowship::handle_action_fellowship_change_lock(
            w, player, lock_state, lock_name,
        );
    }

    // ---- Player partials not yet ported

    /// `player.AddTitle((CharacterTitle)emote.Amount)` (`Player_Character.cs`).
    pub fn player_add_title(w: &mut World, player: ObjectGuid, title: CharacterTitle) {
        crate::world_objects::player_character::add_title_enum(w, player, title, false);
    }

    /// `player.ContractManager.Add(emote.Stat.Value)` (its result is discarded).
    pub fn contract_manager_add(w: &mut World, player: ObjectGuid, contract_id: i32) {
        crate::world_objects::managers::contract_manager::add_int(w, player, contract_id);
    }

    /// `player.ContractManager.IsFull`.
    pub fn contract_manager_is_full(w: &World, player: ObjectGuid) -> bool {
        crate::world_objects::managers::contract_manager::is_full(w, player)
    }

    /// `player.EarnLuminance(amount, xpType, shareType)` (`Player_Luminance.cs`).
    pub fn player_earn_luminance(
        w: &mut World,
        player: ObjectGuid,
        amount: i64,
        xp_type: XpType,
        share_type: ShareType,
    ) {
        crate::world_objects::player_luminance::earn_luminance(
            w, player, amount, xp_type, share_type,
        );
    }

    /// `player.SpendLuminance(amount)` (`Player_Luminance.cs`; the result is discarded).
    pub fn player_spend_luminance(w: &mut World, player: ObjectGuid, amount: i64) {
        let _ = crate::world_objects::player_luminance::spend_luminance(w, player, amount);
    }

    /// `player.StartBarber()` (`Player_Character.cs`).
    pub fn player_start_barber(w: &mut World, player: ObjectGuid) {
        crate::world_objects::player_character::start_barber(w, player);
    }

    /// `player.LearnSpellWithNetworking(spellId, uiOutput)` (`Player_Spells.cs`).
    pub fn player_learn_spell_with_networking(
        w: &mut World,
        player: ObjectGuid,
        spell_id: u32,
        ui_output: bool,
    ) {
        crate::world_objects::player_spells::learn_spell_with_networking(
            w, player, spell_id, ui_output,
        );
    }

    // ---- Creature / Monster partials

    /// `creature.IsAwake` (`Monster_Awareness.cs`): creatures are never awake until it
    /// `creature.IsAwake` (`Monster_Awareness.cs`).
    pub fn creature_is_awake(w: &World, creature: ObjectGuid) -> bool {
        crate::world_objects::monster_awareness::is_awake(w, creature)
    }

    /// `creature.GetRunRate()` (`Monster_Navigation.cs`).
    pub fn creature_get_run_rate(w: &mut World, creature: ObjectGuid) -> f32 {
        crate::world_objects::monster_navigation::get_run_rate(w, creature)
    }

    /// `creature.TurnTo(position)` (`Creature_Navigation.cs`).
    pub fn creature_turn_to(w: &mut World, creature: ObjectGuid, position: &Position) -> f32 {
        crate::world_objects::creature_navigation::turn_to_position(w, creature, position)
    }

    /// `creature.CheckForHumanPreCast(spell)` (`Monster_Magic.cs`).
    pub fn creature_check_for_human_pre_cast(w: &mut World, creature: ObjectGuid, spell: &Spell) {
        crate::world_objects::monster_magic::check_for_human_pre_cast(w, creature, spell);
    }

    /// `creature.PreCastMotion(target)` (`Monster_Magic.cs`; `fallback` false). A null target is
    /// only passed through.
    pub fn creature_pre_cast_motion(
        w: &mut World,
        creature: ObjectGuid,
        target: Option<ObjectGuid>,
    ) -> f32 {
        crate::world_objects::monster_magic::pre_cast_motion(
            w,
            creature,
            target.unwrap_or(ObjectGuid::INVALID),
            false,
        )
    }

    /// `creature.GetPostCastTime(spell)` (`Monster_Magic.cs`; `fallback` false).
    pub fn creature_get_post_cast_time(w: &World, creature: ObjectGuid, spell: &Spell) -> f32 {
        crate::world_objects::monster_magic::get_post_cast_time(w, creature, spell, false)
    }

    /// `creature.PostCastMotion()` (`Monster_Magic.cs`).
    pub fn creature_post_cast_motion(w: &mut World, creature: ObjectGuid) {
        crate::world_objects::monster_magic::post_cast_motion(w, creature);
    }

    /// `pet.P_PetOwner` (`Pet.cs`).
    pub fn pet_p_pet_owner(w: &World, pet: ObjectGuid) -> Option<ObjectGuid> {
        crate::world_objects::pet::p_pet_owner(w, pet)
    }

    // ---- other callees

    /// `WorldObject.DeleteObject(rootOwner)` (`WorldObject_Decay.cs`).
    pub fn world_object_delete_object(
        w: &mut World,
        wo: ObjectGuid,
        root_owner: Option<ObjectGuid>,
    ) {
        crate::world_objects::world_object_decay::delete_object(w, wo, root_owner);
    }

    /// `Enlightenment.HandleEnlightenment(npc, player)` (`Entity/Enlightenment.cs`).
    pub fn enlightenment_handle_enlightenment(w: &mut World, npc: ObjectGuid, player: ObjectGuid) {
        crate::entity::enlightenment::handle_enlightenment(w, npc, player);
    }

    /// `player.ConfirmationManager.EnqueueSend(new Confirmation_YesNo(WorldObject.Guid, player.Guid,
    /// quest), text)` (`ConfirmationManager.cs`, `Entity/Confirmation.cs`).
    pub fn confirmation_manager_enqueue_send_yes_no(
        w: &mut World,
        player: ObjectGuid,
        source: ObjectGuid,
        quest: Option<&str>,
        text: &str,
    ) -> bool {
        let confirmation = crate::entity::confirmation::Confirmation::yes_no(source, player, quest);
        crate::world_objects::managers::confirmation_manager::enqueue_send(
            w,
            player,
            confirmation,
            text,
        )
    }

    /// `door.Close()` (`Door.cs`): the default closer `new ObjectGuid()`.
    pub fn door_close(w: &mut World, door: ObjectGuid) {
        crate::world_objects::door::close(w, door, ObjectGuid::default());
    }

    /// `door.Open()` (`Door.cs`): the default opener `new ObjectGuid()`.
    pub fn door_open(w: &mut World, door: ObjectGuid) {
        crate::world_objects::door::open(w, door, ObjectGuid::default());
    }

    /// `EventManager.IsEventStarted(eventName, source, target)` (`Managers/EventManager.cs`); a
    /// null name is `GetEventName`'s `NullReferenceException`.
    pub fn event_manager_is_event_started(
        w: &mut World,
        event_name: Option<&str>,
        source: ObjectGuid,
        target: Option<ObjectGuid>,
    ) -> bool {
        let event_name = event_name.expect("System.NullReferenceException: emote.Message");
        crate::managers::event_manager::is_event_started(w, event_name, Some(source), target)
    }

    /// `EventManager.StartEvent(eventName, source, target)`.
    pub fn event_manager_start_event(
        w: &mut World,
        event_name: Option<&str>,
        source: ObjectGuid,
        target: Option<ObjectGuid>,
    ) {
        let event_name = event_name.expect("System.NullReferenceException: emote.Message");
        crate::managers::event_manager::start_event(w, event_name, Some(source), target);
    }

    /// `EventManager.StopEvent(eventName, source, target)`.
    pub fn event_manager_stop_event(
        w: &mut World,
        event_name: Option<&str>,
        source: ObjectGuid,
        target: Option<ObjectGuid>,
    ) {
        let event_name = event_name.expect("System.NullReferenceException: emote.Message");
        crate::managers::event_manager::stop_event(w, event_name, Some(source), target);
    }
}
