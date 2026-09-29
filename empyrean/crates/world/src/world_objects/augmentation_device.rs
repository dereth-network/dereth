// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/AugmentationDevice.cs
//! Port of `Source/ACE.Server/WorldObjects/AugmentationDevice.cs`.
//!
//! The augmentation gems. The use checks the requirements, asks through the
//! ConfirmationManager, and on the answer applies the augmentation (`DoAugmentation`).
//! `AugmentationCost` and `AugmentationStat` are in `props/augmentation_device.rs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::ext::aug_type_helper;
use empyrean_entity::enums::{
    AugmentationType, ChatMessageType, PropertyInt, PropertyInt64, SkillAdvancementClass,
    WeenieError, WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;

use crate::entity::confirmation::Confirmation;
use crate::network::game_messages::game_message;
use crate::network::game_messages::messages::{
    game_message_private_update_attribute::game_message_private_update_attribute,
    game_message_private_update_property_int::game_message_private_update_property_int,
    game_message_private_update_property_int64::game_message_private_update_property_int64,
    game_message_private_update_skill::game_message_private_update_skill,
    game_message_script::game_message_script, game_message_system_chat::game_message_system_chat,
};
use crate::world_objects::managers::confirmation_manager;
use crate::world_objects::player_inventory;
use crate::world_objects::player_networking::{send_weenie_error, send_weenie_error_with_string};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::{self, shims};
use crate::World;

/// Non-property fields declared in `AugmentationDevice.cs`.
#[derive(Debug, Default)]
pub struct AugmentationDeviceFields {}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!("System.NullReferenceException: object {g:?} is not in World.objects")
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!("System.NullReferenceException: object {g:?} is not in World.objects")
    })
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

fn session(w: &World, player: ObjectGuid) -> empyrean_net::SessionId {
    shims::player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `(AugmentationType)(AugmentationStat ?? 0)`.
fn augmentation_type(w: &World, this: ObjectGuid) -> AugmentationType {
    AugmentationType(obj(w, this).augmentation_stat().unwrap_or(0).cs_cast())
}

// ACE: AugmentationDevice.AttributeAugmentationSafetyCapEnabled
/// Indicates that the server should enforce logic to prevent players from augmenting a given
/// attribute's innate value over 100.
#[must_use]
pub fn attribute_augmentation_safety_cap_enabled(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "attribute_augmentation_safety_cap", false, true)
        .item
}

// ACE: AugmentationDevice.ActOnUse
/// The use without a confirmation: `ActOnUse(activator, false)`.
pub fn augmentation_device_act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    act_on_use(w, this, activator, false);
}

// ACE: AugmentationDevice.ActOnUse
/// Unconfirmed, asks the player (`Confirmation_Augmentation`); confirmed, applies the augmentation.
pub fn act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid, confirmed: bool) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    if !verify_requirements(w, this, player) {
        return;
    }

    if !confirmed {
        let cost = obj(w, this)
            .augmentation_cost()
            .map_or_else(String::new, |c| empyrean_common::dotnet::format(c, "N0"));
        let text = format!("This action will augment your character with {} and will cost {cost} available experience.", name(w, this));
        if !confirmation_manager::enqueue_send(
            w,
            player,
            Confirmation::augmentation(player, this),
            &text,
        ) {
            send_weenie_error(w, player, WeenieError::ConfirmationInProgress);
        }

        return;
    }
    do_augmentation(w, this, player);
}

// ACE: AugmentationDevice.DoAugmentation
/// Raises the player's augmentation property and applies its effect, spends the experience,
/// consumes the gem, and tells the player and everyone around.
pub fn do_augmentation(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    //Console.WriteLine($"{Name}.DoAugmentation({player.Name})");

    // set augmentation props for player
    let r#type = augmentation_type(w, this);
    let aug_prop = aug_props(r#type);
    let cur_val = obj(w, player).get_property(aug_prop).unwrap_or(0);
    let new_val = cur_val.wrapping_add(1);
    obj_mut(w, player).set_property(aug_prop, new_val);

    if aug_type_helper::is_attribute(r#type) {
        let o = obj_mut(w, player);
        let family = o.augmentation_innate_family();
        o.set_augmentation_innate_family(family.wrapping_add(1));

        let attr = aug_type_helper::get_attribute(r#type);
        let cap = attribute_augmentation_safety_cap_enabled(w);
        let o = obj_mut(w, player);
        let player_attr = *o.attributes().get(&attr).unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: Attributes[{}]",
                attr.to_dotnet_string()
            )
        });
        let starting_value = player_attr.starting_value(o);
        // `100 - StartingValue` is uint arithmetic; VerifyRequirements keeps StartingValue below 96
        // when the cap is on.
        let add = if cap {
            5u32.min(100u32.wrapping_sub(starting_value))
        } else {
            5
        };
        player_attr.set_starting_value(o, starting_value.wrapping_add(add));
        let msg = game_message_private_update_attribute(o, player_attr);
        let s = session(w, player);
        game_message::enqueue_send(w, s, msg);
    } else if aug_type_helper::is_resist(r#type) {
        let o = obj_mut(w, player);
        let family = o.augmentation_resistance_family();
        o.set_augmentation_resistance_family(family.wrapping_add(1));
    } else if aug_type_helper::is_skill(r#type) {
        let skill = aug_type_helper::get_skill(r#type);
        let player_skill = obj_mut(w, player)
            .get_creature_skill(skill, true)
            .expect("GetCreatureSkill(add: true) always answers");
        let o = obj_mut(w, player);
        player_skill.set_advancement_class(o, SkillAdvancementClass::Specialized);
        player_skill.set_init_level(o, 10);
        // adjust rank?
        // handle overages?
        // if trained skill is maxed, there will be a ~103m xp overage...
        let experience_spent = player_skill.experience_spent(o);
        let spec_rank = crate::world_objects::player_skills::calc_skill_rank(
            w,
            SkillAdvancementClass::Specialized,
            experience_spent,
        );
        let o = obj_mut(w, player);
        player_skill.set_ranks(o, spec_rank.cs_cast());
        let msg = game_message_private_update_skill(o, player_skill);
        let s = session(w, player);
        game_message::enqueue_send(w, s, msg);
    } else if r#type == AugmentationType::PackSlot {
        // still seems to require the client to relog
        let o = obj_mut(w, player);
        let capacity = o.container_capacity().map(|c| c.wrapping_add(1));
        o.set_container_capacity(capacity);
        let value = i32::from(
            o.container_capacity()
                .expect("InvalidOperationException: Nullable object must have a value."),
        );
        let msg =
            game_message_private_update_property_int(o, PropertyInt::ContainersCapacity, value);
        let s = session(w, player);
        game_message::enqueue_send(w, s, msg);
    } else if r#type == AugmentationType::BurdenLimit {
        let capacity = player_inventory::get_encumbrance_capacity(w, obj(w, player));
        let msg = game_message_private_update_property_int(
            obj_mut(w, player),
            PropertyInt::EncumbranceCapacity,
            capacity,
        );
        let s = session(w, player);
        game_message::enqueue_send(w, s, msg);
    }

    // consume xp
    let cost = obj(w, this).augmentation_cost();
    let o = obj_mut(w, player);
    let available = o.available_experience();
    o.set_available_experience(available.zip(cost).map(|(a, c)| a.wrapping_sub(c)));

    // `Name` is read below, after the gem is consumed; the consumed gem leaves the store, so it is
    // read here (ACE's reference still reads the same name).
    let device_name = name(w, this);

    // consume augmentation gem
    player_inventory::try_consume_from_inventory_with_networking(w, player, this, 1);

    // send network messages
    let o = obj_mut(w, player);
    let update_prop = game_message_private_update_property_int(o, aug_prop, new_val);
    let available = o.available_experience().unwrap_or(0);
    let update_xp = game_message_private_update_property_int64(
        o,
        PropertyInt64::AvailableExperience,
        available,
    );

    let s = session(w, player);
    game_message::enqueue_send_many(w, s, [update_prop, update_xp]);
    send_weenie_error_with_string(
        w,
        player,
        WeenieErrorWithString::YouSuccededAcquiringAugmentation,
        &device_name,
    );

    // also broadcast to nearby players
    let script = game_message_script(player, aug_type_helper::get_effect(r#type), 1.0);
    world_object_networking::enqueue_broadcast(w, player, true, &[script]);
    let text = format!(
        "{} has acquired the {device_name} augmentation!",
        name(w, player)
    );
    world_object_networking::enqueue_broadcast(
        w,
        player,
        true,
        &[game_message_system_chat(&text, ChatMessageType::Broadcast)],
    );

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, player, true);
}

// ACE: AugmentationDevice.VerifyRequirements
/// The cost is set and affordable, the family and per-type caps are not reached, an attribute is
/// below its innate maximum and a skill is trained.
pub fn verify_requirements(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let available_xp = obj(w, player).available_experience().unwrap_or(0);
    let augmentation_cost = obj(w, this).augmentation_cost();
    let aug_cost = augmentation_cost.unwrap_or(0);

    if augmentation_cost.is_none() {
        let text = format!("{} is missing AugmentationCost", name(w, this));
        world_object_networking::enqueue_broadcast(
            w,
            player,
            true,
            &[game_message_system_chat(&text, ChatMessageType::System)],
        );
        return false;
    }

    if available_xp < aug_cost {
        send_weenie_error(w, player, WeenieError::AugmentationNotEnoughExperience);
        return false;
    }

    let r#type = augmentation_type(w, this);

    // per-type checks
    if aug_type_helper::is_attribute(r#type) {
        // innate attributes shared cap
        if obj(w, player).augmentation_innate_family() >= max_augs(r#type) {
            send_weenie_error(w, player, WeenieError::AugmentationTypeUsedTooManyTimes);
            return false;
        }

        let attr = aug_type_helper::get_attribute(r#type);
        let o = obj(w, player);
        let player_attribute = *o.attributes().get(&attr).unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: Attributes[{}]",
                attr.to_dotnet_string()
            )
        });

        // check InitLevel
        let max_innate_value = if attribute_augmentation_safety_cap_enabled(w) {
            96
        } else {
            100
        };
        if player_attribute.starting_value(o) >= max_innate_value {
            let text = format!(
                "You are not able to purchase this augmentation because your {} is already at the maximum innate level!",
                player_attribute.attribute.to_dotnet_string()
            );
            send_weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::AugmentationSkillNotTrained,
                &text,
            );
            return false;
        }
    } else if aug_type_helper::is_skill(r#type) {
        let player_skill = obj_mut(w, player)
            .get_creature_skill(aug_type_helper::get_skill(r#type), true)
            .expect("GetCreatureSkill(add: true) always answers");

        if player_skill.advancement_class(obj(w, player)) != SkillAdvancementClass::Trained {
            let text = format!(
                "You are not able to purchase this augmentation because you are not trained in {}!",
                player_skill.skill.to_sentence()
            );
            send_weenie_error_with_string(
                w,
                player,
                WeenieErrorWithString::AugmentationSkillNotTrained,
                &text,
            );
            return false;
        }
    } else if aug_type_helper::is_resist(r#type) {
        // resistance shared cap
        if obj(w, player).augmentation_resistance_family() >= max_augs(r#type) {
            send_weenie_error(w, player, WeenieError::AugmentationTypeUsedTooManyTimes);
            return false;
        }
    }

    // common checks
    let aug_prop = obj(w, player).get_property(aug_props(r#type)).unwrap_or(0);

    if aug_prop >= max_augs(r#type) {
        send_weenie_error(w, player, WeenieError::AugmentationUsedTooManyTimes);
        return false;
    }

    true
}

// ACE: AugmentationDevice.MaxAugs
/// The most times each augmentation can be bought (for the attribute and resistance families,
/// also the family's shared cap).
pub const MAX_AUGS: &[(AugmentationType, i32)] = &[
    (AugmentationType::Strength, 10), // attributes in shared group
    (AugmentationType::Endurance, 10),
    (AugmentationType::Coordination, 10),
    (AugmentationType::Quickness, 10),
    (AugmentationType::Focus, 10),
    (AugmentationType::Self_, 10),
    (AugmentationType::Salvage, 1),
    (AugmentationType::ItemTinkering, 1),
    (AugmentationType::ArmorTinkering, 1),
    (AugmentationType::MagicItemTinkering, 1),
    (AugmentationType::WeaponTinkering, 1),
    (AugmentationType::PackSlot, 1),
    (AugmentationType::BurdenLimit, 5),
    (AugmentationType::DeathItemLoss, 3),
    (AugmentationType::DeathSpellLoss, 1),
    (AugmentationType::CritProtect, 1),
    (AugmentationType::BonusXP, 1),
    (AugmentationType::BonusSalvage, 4),
    (AugmentationType::ImbueChance, 1),
    (AugmentationType::RegenBonus, 2),
    (AugmentationType::SpellDuration, 5),
    (AugmentationType::ResistSlash, 2), // resistances in shared group
    (AugmentationType::ResistPierce, 2),
    (AugmentationType::ResistBludgeon, 2),
    (AugmentationType::ResistAcid, 2),
    (AugmentationType::ResistFire, 2),
    (AugmentationType::ResistCold, 2),
    (AugmentationType::ResistElectric, 2),
    (AugmentationType::FociCreature, 1),
    (AugmentationType::FociItem, 1),
    (AugmentationType::FociLife, 1),
    (AugmentationType::FociWar, 1),
    (AugmentationType::CritChance, 1),
    (AugmentationType::CritDamage, 1),
    (AugmentationType::Melee, 1),
    (AugmentationType::Missile, 1),
    (AugmentationType::Magic, 1),
    (AugmentationType::Damage, 1),
    (AugmentationType::DamageResist, 1),
    (AugmentationType::AllStats, 1),
    (AugmentationType::FociVoid, 1),
];

// ACE: AugmentationDevice.AugProps
/// The player property each augmentation raises.
pub const AUG_PROPS: &[(AugmentationType, PropertyInt)] = &[
    (
        AugmentationType::Strength,
        PropertyInt::AugmentationInnateStrength,
    ),
    (
        AugmentationType::Endurance,
        PropertyInt::AugmentationInnateEndurance,
    ),
    (
        AugmentationType::Coordination,
        PropertyInt::AugmentationInnateCoordination,
    ),
    (
        AugmentationType::Quickness,
        PropertyInt::AugmentationInnateQuickness,
    ),
    (
        AugmentationType::Focus,
        PropertyInt::AugmentationInnateFocus,
    ),
    (AugmentationType::Self_, PropertyInt::AugmentationInnateSelf),
    (
        AugmentationType::Salvage,
        PropertyInt::AugmentationSpecializeSalvaging,
    ),
    (
        AugmentationType::ItemTinkering,
        PropertyInt::AugmentationSpecializeItemTinkering,
    ),
    (
        AugmentationType::ArmorTinkering,
        PropertyInt::AugmentationSpecializeArmorTinkering,
    ),
    (
        AugmentationType::MagicItemTinkering,
        PropertyInt::AugmentationSpecializeMagicItemTinkering,
    ),
    (
        AugmentationType::WeaponTinkering,
        PropertyInt::AugmentationSpecializeWeaponTinkering,
    ),
    (
        AugmentationType::PackSlot,
        PropertyInt::AugmentationExtraPackSlot,
    ),
    (
        AugmentationType::BurdenLimit,
        PropertyInt::AugmentationIncreasedCarryingCapacity,
    ),
    (
        AugmentationType::DeathItemLoss,
        PropertyInt::AugmentationLessDeathItemLoss,
    ),
    (
        AugmentationType::DeathSpellLoss,
        PropertyInt::AugmentationSpellsRemainPastDeath,
    ),
    (
        AugmentationType::CritProtect,
        PropertyInt::AugmentationCriticalDefense,
    ),
    (AugmentationType::BonusXP, PropertyInt::AugmentationBonusXp),
    (
        AugmentationType::BonusSalvage,
        PropertyInt::AugmentationBonusSalvage,
    ),
    (
        AugmentationType::ImbueChance,
        PropertyInt::AugmentationBonusImbueChance,
    ),
    (
        AugmentationType::RegenBonus,
        PropertyInt::AugmentationFasterRegen,
    ),
    (
        AugmentationType::SpellDuration,
        PropertyInt::AugmentationIncreasedSpellDuration,
    ),
    (
        AugmentationType::ResistSlash,
        PropertyInt::AugmentationResistanceSlash,
    ),
    (
        AugmentationType::ResistPierce,
        PropertyInt::AugmentationResistancePierce,
    ),
    (
        AugmentationType::ResistBludgeon,
        PropertyInt::AugmentationResistanceBlunt,
    ),
    (
        AugmentationType::ResistAcid,
        PropertyInt::AugmentationResistanceAcid,
    ),
    (
        AugmentationType::ResistFire,
        PropertyInt::AugmentationResistanceFire,
    ),
    (
        AugmentationType::ResistCold,
        PropertyInt::AugmentationResistanceFrost,
    ),
    (
        AugmentationType::ResistElectric,
        PropertyInt::AugmentationResistanceLightning,
    ),
    (
        AugmentationType::FociCreature,
        PropertyInt::AugmentationInfusedCreatureMagic,
    ),
    (
        AugmentationType::FociItem,
        PropertyInt::AugmentationInfusedItemMagic,
    ),
    (
        AugmentationType::FociLife,
        PropertyInt::AugmentationInfusedLifeMagic,
    ),
    (
        AugmentationType::FociWar,
        PropertyInt::AugmentationInfusedWarMagic,
    ),
    (
        AugmentationType::CritChance,
        PropertyInt::AugmentationCriticalExpertise,
    ),
    (
        AugmentationType::CritDamage,
        PropertyInt::AugmentationCriticalPower,
    ),
    (
        AugmentationType::Melee,
        PropertyInt::AugmentationSkilledMelee,
    ),
    (
        AugmentationType::Missile,
        PropertyInt::AugmentationSkilledMissile,
    ),
    (
        AugmentationType::Magic,
        PropertyInt::AugmentationSkilledMagic,
    ),
    (
        AugmentationType::Damage,
        PropertyInt::AugmentationDamageBonus,
    ),
    (
        AugmentationType::DamageResist,
        PropertyInt::AugmentationDamageReduction,
    ),
    (
        AugmentationType::AllStats,
        PropertyInt::AugmentationJackOfAllTrades,
    ),
    (
        AugmentationType::FociVoid,
        PropertyInt::AugmentationInfusedVoidMagic,
    ),
];

/// `MaxAugs[type]`.
///
/// # Panics
/// For a type without an entry (ACE: `KeyNotFoundException`).
#[must_use]
pub fn max_augs(r#type: AugmentationType) -> i32 {
    MAX_AUGS
        .iter()
        .find(|(t, _)| *t == r#type)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: MaxAugs[{}]",
                r#type.to_dotnet_string()
            )
        })
}

/// `AugProps[type]`.
///
/// # Panics
/// For a type without an entry (ACE: `KeyNotFoundException`).
#[must_use]
pub fn aug_props(r#type: AugmentationType) -> PropertyInt {
    AUG_PROPS
        .iter()
        .find(|(t, _)| *t == r#type)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: AugProps[{}]",
                r#type.to_dotnet_string()
            )
        })
}

// ---- constructors and SetEphemeralValues ----

/// `new AugmentationDevice(weenie, guid)` / `new AugmentationDevice(biota)`: the `WorldObject` constructor, then
/// AugmentationDevice's `SetEphemeralValues`.
// ACE: AugmentationDevice.AugmentationDevice
pub fn augmentation_device_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    augmentation_device_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: AugmentationDevice.SetEphemeralValues
fn augmentation_device_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
