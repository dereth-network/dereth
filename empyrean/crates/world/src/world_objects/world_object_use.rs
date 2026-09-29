// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`.
//!
//! The activation path every double-click ends in (`OnActivate`: `CheckUseRequirements`, the use
//! cooldown, the `ActivationResponse` hooks and the activation target), the base `ActOnUse`, and
//! the use-radius test `Player_Use` and the move-to chains measure with.
//!
//! The typed properties at the top of the ACE file (`UseTimestamp`, `ResetTimestamp`,
//! `ResetInterval`, `DefaultLocked`, `DefaultOpen`) are generated in `props/world_object_use.rs`.

use empyrean_entity::enums::{
    ActivationResponse, ChatMessageType, CreatureType, HeritageGroup, PropertyAttribute2nd, Skill,
    SkillAdvancementClass, WeenieError, WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;

use crate::entity::activation_result::ActivationResult;
use crate::entity::spell::Spell;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::managers::{
    emote_manager, enchantment_manager, enchantment_manager_with_caching,
};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    player_networking, world_object, world_object_generators, world_object_magic,
    world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `WorldObject_Use.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectUseFields {}

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

fn is_player(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_player)
}

fn is_creature(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_creature)
}

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, msg);
}

/// `new GameEventWeenieErrorWithString(player.Session, error, str)`.
fn weenie_error_with_string(
    w: &mut World,
    player: ObjectGuid,
    error: WeenieErrorWithString,
    str: &str,
) -> GameMessage {
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    let data = w
        .sessions
        .get_mut(session)
        .expect("the session's game half");
    game_event_weenie_error_with_string(data, error, str)
}

/// `new GameEventCommunicationTransientString(player.Session, msg)`.
fn transient_string(w: &mut World, player: ObjectGuid, msg: &str) -> GameMessage {
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    let data = w
        .sessions
        .get_mut(session)
        .expect("the session's game half");
    game_event_communication_transient_string(data, msg)
}

/// `player.GetCreatureSkill(skill)`: the skill's `Current`, its advancement class, and the skill.
fn creature_skill(
    w: &mut World,
    player: ObjectGuid,
    skill: Skill,
) -> (u32, SkillAdvancementClass, Skill) {
    let s = obj_mut(w, player)
        .get_creature_skill(skill, true)
        .expect("GetCreatureSkill adds a missing skill");
    let advancement_class = s.advancement_class(obj(w, player));
    let current = s.current(w, player);
    (current, advancement_class, s.skill)
}

// ================================================================================ WorldObject_Use.cs

/// Used to determine how close you need to be to use an item. `use_radius` defaults to the
/// target's `UseRadius ?? 0.6`.
// ACE: WorldObject.IsWithinUseRadiusOf
#[must_use]
pub fn is_within_use_radius_of(
    w: &World,
    this: ObjectGuid,
    wo: ObjectGuid,
    use_radius: Option<f32>,
) -> bool {
    let use_radius = match use_radius {
        Some(r) => r,
        None => obj(w, wo).use_radius().unwrap_or(0.6),
    };

    let cyl_dist = get_cylinder_distance(w, this, wo);

    cyl_dist <= use_radius
}

/// Handles the 'GameAction 0x35 - UseWithTarget' network message on a per-object type basis.
// ACE: WorldObject.HandleActionUseOnTarget
pub fn world_object_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    crate::managers::recipe_manager::use_object_on_target(w, player, this, target, false);
}

/// When players double click an object, and the packet comes in as GameAction 0x36 - UseItem:
/// from the game perspective, technically this starts as an 'Activate', which can have a list of
/// possible ActivationResponses - Use (by far the most common), Animate, Talk, Emote, CastSpell,
/// Generate.
// ACE: WorldObject.OnActivate
pub fn world_object_on_activate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    //Console.WriteLine($"{Name}.OnActivate({activator.Name})");

    // PropertyInt.Active indicates if this object can be activated, default is true
    if !obj(w, this).active() {
        return;
    }

    // verify use requirements
    let result = dispatch::check_use_requirements::check_use_requirements(w, this, activator);

    let player = is_player(w, activator).then_some(activator);
    if !result.success {
        if let (Some(message), Some(player)) = (result.message, player) {
            send(w, player, message);
        }

        return;
    }

    if let Some(player) = player {
        enchantment_manager_with_caching::start_cooldown(w, player, this);
    }

    let activation_response = obj(w, this).activation_response();

    // perform motion animation - rarely used (only 4 instances in PY16 db)
    if activation_response.contains(ActivationResponse::Animate) {
        dispatch::on_animate::on_animate(w, this, activator);
    }

    // perform activation emote
    if activation_response.contains(ActivationResponse::Emote) {
        dispatch::on_emote::on_emote(w, this, activator);
    }

    // cast a spell on the player (spell traps)
    if activation_response.contains(ActivationResponse::CastSpell) {
        dispatch::on_cast_spell::on_cast_spell(w, this, activator);
    }

    // call to generator to spawn new object
    if activation_response.contains(ActivationResponse::Generate) {
        dispatch::on_generate::on_generate(w, this, activator);
    }

    // default use action
    if activation_response.contains(ActivationResponse::Use) {
        if is_creature(w, activator) {
            //target.EmoteManager.OnActivation(creature); // found a few things with Activation on them but not ActivationResponse.Emote...
            emote_manager::on_use(w, this, activator);
        }

        dispatch::act_on_use::act_on_use(w, this, activator);
    }

    // send chat text - rarely used (only 8 instances in PY16 db)
    if activation_response.contains(ActivationResponse::Talk) {
        dispatch::on_talk::on_talk(w, this, activator);
    }

    let Some(o) = w.objects.get(this) else { return };
    let activation_target = o.activation_target();
    if !o.is_creature() && activation_target > 0 {
        let target = o.current_landblock.and_then(|lb| {
            crate::entity::landblock::get_object(w, lb, ObjectGuid::new(activation_target), true)
        });
        if let Some(target) = target {
            dispatch::on_activate::on_activate(w, target, activator);
        } else {
            log::warn!(
                "{}.OnActivate({}): couldn't find activation target {activation_target:08X}",
                name(w, this),
                name(w, activator)
            );
        }
    }
}

/// Empty base - individual WorldObject types should override: logs an error, and tells a player.
// ACE: WorldObject.ActOnUse
pub fn world_object_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    let o = obj(w, this);
    let msg = format!(
        "{}.ActOnUse({}) - undefined for wcid {} type {}",
        name(w, this),
        name(w, activator),
        o.biota.weenie_class_id,
        o.biota.weenie_type.to_dotnet_string()
    );
    log::error!("{msg}");

    if is_player(w, activator) {
        let m = game_message_system_chat(&msg, ChatMessageType::Broadcast);
        send(w, activator, m);
    }
}

// ACE: WorldObject.OnAnimate
pub fn world_object_on_animate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    let _ = activator;
    let activation_animation = obj(w, this).activation_animation();
    let motion = Motion::from_world_object(w, this, activation_animation, 1.0);
    world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);
}

// ACE: WorldObject.OnTalk
// Not ACE's (a fix, V341): an object with no `ActivationTalk` says nothing,
// and the activation goes on (to its activation target). ACE built the chat line from the null
// text and threw, ending the activation; the world database has such an object (wcid 70090, a
// pressure plate with Generate and Talk and no text).
pub fn world_object_on_talk(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    // todo: verify the format of this message
    if is_player(w, activator) {
        let Some(activation_talk) = obj(w, this).activation_talk() else {
            return;
        };
        let m = game_message_system_chat(&activation_talk, ChatMessageType::Broadcast);
        send(w, activator, m);
    }
}

// ACE: WorldObject.OnEmote
pub fn world_object_on_emote(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if is_creature(w, activator) {
        emote_manager::on_activation(w, this, activator);
    }
}

// ACE: WorldObject.OnCastSpell
pub fn world_object_on_cast_spell(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if let Some(spell_did) = obj(w, this).spell_did() {
        let spell = Spell::new(w, spell_did, true);
        world_object_magic::try_cast_spell(
            w,
            this,
            &spell,
            Some(activator),
            Some(this),
            None,
            false,
            false,
            true,
        );
    }
}

// ACE: WorldObject.OnGenerate
pub fn world_object_on_generate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    let _ = activator;
    if obj(w, this).is_generator() {
        world_object_generators::generator_generate(w, this);
    }
}

/// Verifies the use requirements for activating an item.
///
/// # Panics
/// When an attribute or vital limit names a stat the player does not hold (ACE:
/// `KeyNotFoundException`), or the player has no session.
// ACE: WorldObject.CheckUseRequirements
#[allow(
    clippy::too_many_lines,
    clippy::if_same_then_else,
    clippy::collapsible_if
)] // ACE's branch structure, kept
pub fn world_object_check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> crate::entity::activation_result::ActivationResult {
    //Console.WriteLine($"{Name}.CheckUseRequirements({activator.Name})");

    if !w.objects.contains(activator) {
        log::error!(
            "0x{}:{}.CheckUseRequirements() (wcid: {}): activator is null",
            this,
            name(w, this),
            obj(w, this).biota.weenie_class_id
        );
        return ActivationResult::new(false);
    }

    if !is_player(w, activator) {
        return ActivationResult::new(true);
    }
    let player = activator;

    // verify arcane lore requirement
    if let Some(item_difficulty) = obj(w, this).item_difficulty() {
        let (current, _, skill) = creature_skill(w, player, Skill::ArcaneLore);
        if i64::from(current) < i64::from(item_difficulty) {
            let m = weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                &skill.to_sentence(),
            );
            return ActivationResult::with_message(m);
        }
    }

    // verify skill - does this have to be trained, or only in conjunction with UseRequiresSkillLevel?
    // only seems to be used for summoning so far...
    if let (Some(item_skill_limit), Some(item_skill_level_limit)) = (
        obj(w, this).item_skill_limit(),
        obj(w, this).item_skill_level_limit(),
    ) {
        let skill = world_object::convert_to_mo_a_skill(w, activator, item_skill_limit);
        let (current, _, player_skill) = creature_skill(w, player, skill);

        //if (playerSkill.AdvancementClass < SkillAdvancementClass.Trained)
        //{
        //    //return new ActivationResult(new GameEventWeenieErrorWithString(player.Session, WeenieErrorWithString.Your_SkillMustBeTrained, playerSkill.Skill.ToSentence()));
        //    player.Session.Network.EnqueueSend(new GameEventCommunicationTransientString(player.Session, $"You must have {playerSkill.Skill.ToSentence()} trained to use that item's magic"));
        //    return new ActivationResult(false);
        //}

        // verify skill level
        if i64::from(current) < i64::from(item_skill_level_limit) {
            let m = weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                &player_skill.to_sentence(),
            );
            return ActivationResult::with_message(m);
        }
    }

    if let Some(use_requires_skill) = obj(w, this).use_requires_skill() {
        let skill = world_object::convert_to_mo_a_skill(w, activator, Skill(use_requires_skill));
        let (current, advancement_class, player_skill) = creature_skill(w, player, skill);

        if advancement_class.0 < SkillAdvancementClass::Trained.0 {
            //return new ActivationResult(new GameEventWeenieErrorWithString(player.Session, WeenieErrorWithString.Your_SkillMustBeTrained, playerSkill.Skill.ToSentence()));
            let m = transient_string(
                w,
                player,
                &format!(
                    "You must have {} trained to use that item's magic",
                    player_skill.to_sentence()
                ),
            );
            send(w, player, m);
            return ActivationResult::new(false);
        }

        // verify skill level
        if let Some(use_requires_skill_level) = obj(w, this).use_requires_skill_level() {
            if i64::from(current) < i64::from(use_requires_skill_level) {
                let m = weenie_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                    &player_skill.to_sentence(),
                );
                return ActivationResult::with_message(m);
            }
        }
    }

    // verify skill specialized
    // is this always in conjunction with UseRequiresSkill?
    // again, only seems to be for summoning so far...
    if let Some(use_requires_skill_spec) = obj(w, this).use_requires_skill_spec() {
        let skill =
            world_object::convert_to_mo_a_skill(w, activator, Skill(use_requires_skill_spec));
        let (current, advancement_class, player_skill) = creature_skill(w, player, skill);

        if advancement_class.0 < SkillAdvancementClass::Specialized.0 {
            let m = weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::YouMustSpecialize_ToUseItemMagic,
                &player_skill.to_sentence(),
            );
            return ActivationResult::with_message(m);
        }

        // verify skill level
        if let Some(use_requires_skill_level) = obj(w, this).use_requires_skill_level() {
            if i64::from(current) < i64::from(use_requires_skill_level) {
                let m = weenie_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                    &player_skill.to_sentence(),
                );
                return ActivationResult::with_message(m);
            }
        }
    }

    // verify skill specialized
    // only found on a few items, doesn't show up on ID panel so is effectively a hidden requirement unless noted in ShortDesc/LongDesc string text
    if let Some(item_specialized_only) = obj(w, this).item_specialized_only() {
        let skill = world_object::convert_to_mo_a_skill(w, activator, item_specialized_only);
        let (current, advancement_class, player_skill) = creature_skill(w, player, skill);

        if advancement_class.0 < SkillAdvancementClass::Specialized.0 {
            let m = weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::YouMustSpecialize_ToUseItemMagic,
                &player_skill.to_sentence(),
            );
            return ActivationResult::with_message(m);
        }

        // verify skill level (if this was included)
        if let Some(item_skill_level_limit) = obj(w, this).item_skill_level_limit() {
            if i64::from(current) < i64::from(item_skill_level_limit) {
                let m = weenie_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                    &player_skill.to_sentence(),
                );
                return ActivationResult::with_message(m);
            }
        }
    }

    // verify player level
    if let Some(use_requires_level) = obj(w, this).use_requires_level() {
        let player_level = obj(w, player).level().unwrap_or(1);
        if player_level < use_requires_level {
            //return new ActivationResult(new GameEventWeenieErrorWithString(player.Session, WeenieErrorWithString.YouMustBe_ToUseItemMagic, $"level {UseRequiresLevel.Value}")); // not retail
            let m = transient_string(w, player, "You are not high enough level to use that!");
            return ActivationResult::with_message(m);
        }
    }

    // verify attribute / vital limits
    if let Some(item_attribute_limit) = obj(w, this).item_attribute_limit() {
        let player_attr = obj(w, player)
            .attributes()
            .get(&item_attribute_limit)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "KeyNotFoundException: Creature.Attributes[{}]",
                    item_attribute_limit.to_dotnet_string()
                )
            });
        let current = player_attr.current(&mut StatCtx::in_world(w, player));

        // `uint < int?`: false when the limit is null
        if obj(w, this)
            .item_attribute_level_limit()
            .is_some_and(|limit| i64::from(current) < i64::from(limit))
        {
            let m = weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                &player_attr.attribute.to_dotnet_string(),
            );
            return ActivationResult::with_message(m);
        }
    }

    if let Some(item_attribute2nd_limit) = obj(w, this).item_attribute2nd_limit() {
        let player_vital = obj(w, player)
            .vitals()
            .get(&item_attribute2nd_limit)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "KeyNotFoundException: Creature.Vitals[{}]",
                    item_attribute2nd_limit.to_dotnet_string()
                )
            });
        let max_value = player_vital.max_value(&mut StatCtx::in_world(w, player));

        if obj(w, this)
            .item_attribute2nd_level_limit()
            .is_some_and(|limit| i64::from(max_value) < i64::from(limit))
        {
            let vital: PropertyAttribute2nd = player_vital.vital;
            let m = weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::Your_IsTooLowToUseItemMagic,
                &vital.to_sentence(),
            );
            return ActivationResult::with_message(m);
        }
    }

    // verify heritage group
    let heritage_group = obj(w, this).heritage_group();
    if heritage_group != HeritageGroup::Invalid {
        if !is_creature(w, this) {
            // Creatures are not restricted to their own hertigage group
            let player_heritage_group = obj(w, player).heritage_group();

            if player_heritage_group != heritage_group {
                let m = weenie_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::YouMustBe_ToUseItemMagic,
                    &heritage_group.to_sentence(),
                );
                return ActivationResult::with_message(m);
            }
        }
    }

    // Check for a cooldown
    let cooldown_id = obj(w, this).cooldown_id();
    if !enchantment_manager::check_cooldown(w, player, cooldown_id) {
        // TODO: werror/string not found, find exact message

        /*var cooldown = player.GetCooldown(this);
        var timer = cooldown.GetFriendlyString();
        player.Session.Network.EnqueueSend(new GameMessageSystemChat($"{Name} can be activated again in {timer}", ChatMessageType.Broadcast));*/

        let m = transient_string(w, player, "You have used this item too recently");
        send(w, player, m);
        return ActivationResult::new(false);
    }

    if is_olthoi_player(w, player) {
        //player.Session.Network.EnqueueSend(new GameEventCommunicationTransientString(player.Session, "Olthoi can't interact with that!"));
        //player.SendWeenieError(WeenieError.OlthoiCannotInteractWithThat);
        //return new ActivationResult(false);

        let o = obj(w, this);
        if o.is_creature() {
            if o.creature_type() == Some(CreatureType::Olthoi) {
                return ActivationResult::new(true);
            }

            if o.is_vendor() {
                player_networking::send_weenie_error(
                    w,
                    player,
                    WeenieError::OlthoiVendorLooksInHorror,
                );
            } else if o.npc_looks_like_object().unwrap_or(false) {
                player_networking::send_weenie_error(
                    w,
                    player,
                    WeenieError::OlthoiCannotInteractWithThat,
                );
            } else {
                let n = name(w, this);
                player_networking::send_weenie_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::_CowersFromYou,
                    &n,
                );
            }

            return ActivationResult::new(false);
        } else if o.is_lifestone() {
            player_networking::send_weenie_error(w, player, WeenieError::OlthoiCannotUseLifestones);
            return ActivationResult::new(false);
        } else if o.is_container() && !o.is_corpse() {
            player_networking::send_weenie_error(
                w,
                player,
                WeenieError::OlthoiCannotInteractWithThat,
            );
            return ActivationResult::new(false);
        } else if o.is_attribute_transfer_device()
            || o.is_augmentation_device()
            || o.is_bindstone()
            || o.is_book()
            || o.is_game()
            || o.is_gem()
            || o.is_generic_object()
            || o.is_key()
            || o.is_skill_alteration_device()
        {
            player_networking::send_weenie_error(
                w,
                player,
                WeenieError::OlthoiCannotInteractWithThat,
            );
            return ActivationResult::new(false);
        }
    }

    ActivationResult::new(true)
}

// ================================================================================ pointers

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`), as `SetEphemeralValues` set it.
fn is_olthoi_player(w: &World, player: ObjectGuid) -> bool {
    w.objects
        .get(player)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// The distance between the two bodies' cylinders.
///
/// # Panics
/// Without physics bodies (ACE: `NullReferenceException`).
// ACE: WorldObject.GetCylinderDistance
#[must_use]
#[allow(clippy::cast_possible_truncation)] // ACE's `(float)`
pub fn get_cylinder_distance(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) -> f32 {
    let size = |g: empyrean_entity::ObjectGuid| {
        let h = crate::physics::phys_ext::physics_obj(w, g)
            .expect("ACE: PhysicsObj is null (NullReferenceException)");
        let o = w
            .physics
            .get(h)
            .expect("ACE: PhysicsObj is null (NullReferenceException)");
        (o.radius(), o.height(), o.position)
    };
    let (r1, h1, p1) = size(this);
    let (r2, h2, p2) = size(wo);
    crate::world_objects::monster_navigation::cylinder_distance(r1, h1, &p1, r2, h2, &p2) as f32
}
