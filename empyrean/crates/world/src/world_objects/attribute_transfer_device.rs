// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/AttributeTransferDevice.cs
//! Port of `Source/ACE.Server/WorldObjects/AttributeTransferDevice.cs`.
//!
//! The use (asked through the ConfirmationManager, then applied on the answer)
//! moves up to 10 innate points from one attribute to another. `TransferFromAttribute` and
//! `TransferToAttribute` are in `props/attribute_transfer_device.rs`.

use empyrean_entity::enums::{
    PropertyAttribute, Skill, WeenieError, WeenieErrorWithString, WieldRequirement,
};
use empyrean_entity::ObjectGuid;

use crate::entity::confirmation::Confirmation;
use crate::network::game_messages::game_message;
use crate::network::game_messages::messages::game_message_private_update_attribute::game_message_private_update_attribute;
use crate::world_objects::entity::creature_attribute::CreatureAttribute;
use crate::world_objects::managers::confirmation_manager;
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::player_networking::{send_weenie_error, send_weenie_error_with_string};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// Non-property fields declared in `AttributeTransferDevice.cs`.
#[derive(Debug, Default)]
pub struct AttributeTransferDeviceFields {}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!("System.NullReferenceException: object {g:?} is not in World.objects")
    })
}

/// `player.Attributes[attribute]`.
fn attribute(w: &World, player: ObjectGuid, attribute: PropertyAttribute) -> CreatureAttribute {
    *obj(w, player)
        .attributes()
        .get(&attribute)
        .unwrap_or_else(|| {
            panic!(
                "KeyNotFoundException: Attributes[{}]",
                attribute.to_dotnet_string()
            )
        })
}

// ACE: AttributeTransferDevice.ActOnUse
/// The use without a confirmation: `ActOnUse(activator, false)`.
pub fn attribute_transfer_device_act_on_use(
    w: &mut World,
    this: ObjectGuid,
    activator: ObjectGuid,
) {
    act_on_use(w, this, activator, false);
}

// ACE: AttributeTransferDevice.ActOnUse
/// Unconfirmed, asks the player (`Confirmation_AlterAttribute`); confirmed, moves up to 10 innate
/// points from `TransferFromAttribute` to `TransferToAttribute` and consumes the device.
pub fn act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid, confirmed: bool) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    let (transfer_from_attribute, transfer_to_attribute) = {
        let o = obj(w, this);
        (o.transfer_from_attribute(), o.transfer_to_attribute())
    };
    if transfer_from_attribute == PropertyAttribute::Undef
        || transfer_to_attribute == PropertyAttribute::Undef
    {
        send_weenie_error(w, player, WeenieError::YouHaveFailedToAlterAttributes);
        return;
    }

    let device =
        player_inventory::find_object(w, player, this, SearchLocations::MyInventory).result;
    if device.is_none() {
        send_weenie_error(w, player, WeenieError::YouHaveFailedToAlterAttributes);
        return;
    }

    let from_attr = attribute(w, player, transfer_from_attribute);
    let to_attr = attribute(w, player, transfer_to_attribute);

    if !verify_requirements(w, this, player, from_attr, to_attr) {
        return;
    }

    if !confirmed {
        let text = format!(
            "This action will transfer 10 points from your {} to your {}.",
            from_attr.attribute.to_dotnet_string(),
            to_attr.attribute.to_dotnet_string()
        );
        if !confirmation_manager::enqueue_send(
            w,
            player,
            Confirmation::alter_attribute(player, this),
            &text,
        ) {
            send_weenie_error(w, player, WeenieError::ConfirmationInProgress);
        }
        return;
    }

    let (from_amount, to_amount) = {
        let o = obj(w, player);
        (
            10u32.min(from_attr.starting_value(o) - 10),
            (100 - to_attr.starting_value(o)).min(10),
        )
    };

    let amount = from_amount.min(to_amount);

    let o = w.objects.get_mut(player).expect("the player");
    let v = from_attr.starting_value(o);
    from_attr.set_starting_value(o, v - amount);
    let v = to_attr.starting_value(o);
    to_attr.set_starting_value(o, v + amount);

    let update_from = game_message_private_update_attribute(o, from_attr);
    let update_to = game_message_private_update_attribute(o, to_attr);

    //// begin things not seen in pcaps?
    //var msgFrom = new GameMessageSystemChat($"Your base {TransferFromAttribute} is now {fromAttr.Base}!", ChatMessageType.Broadcast);
    //var msgTo = new GameMessageSystemChat($"Your base {TransferToAttribute} is now {toAttr.Base}!", ChatMessageType.Broadcast);

    //var sound = new GameMessageSound(player.Guid, Sound.RaiseTrait);
    //// end things not seen in pcaps?
    //// the above provides better feedback to player but it wasn't seen in pcaps...

    //player.Session.Network.EnqueueSend(updateFrom, updateTo, msgFrom, msgTo, sound);
    let session = shims::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    game_message::enqueue_send_many(w, session, [update_from, update_to]);

    // this should be a UseDone(WeenieError.YouHaveSucceededTransferringAttributes) but Player.TryUseItem has built in UseDone(WeenieError.None) which would conflict.
    send_weenie_error(
        w,
        player,
        WeenieError::YouHaveSucceededTransferringAttributes,
    );

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, player, true);

    player_inventory::try_consume_from_inventory_with_networking(w, player, this, 1);
}

// ACE: AttributeTransferDevice.VerifyRequirements
/// No wielded item with an attribute requirement, the source above 10 and the target below 100.
// Not ACE's (a fix, V342): the "already as high as it can be" message names
// the attribute the device raises, the one at its maximum. ACE named the attribute it lowers.
pub fn verify_requirements(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    transfer_from_attribute: CreatureAttribute,
    transfer_to_attribute: CreatureAttribute,
) -> bool {
    // Check for equipped items that have requirements in skill or attributes
    if check_wielded_items(w, player) {
        // Items are wielded which might be affected by a transfer operation
        //player.Session.Network.EnqueueSend(new GameEventWeenieErrorWithString(player.Session, WeenieErrorWithString.CannotLowerSkillWhileWieldingItem, skill.Skill.ToSentence()));
        send_weenie_error(
            w,
            player,
            WeenieError::CannotTransferAttributesWhileWieldingItem,
        );
        return false;
    }

    let from_name = obj(w, this).transfer_from_attribute().to_dotnet_string();
    if transfer_from_attribute.starting_value(obj(w, player)) <= 10 {
        //player.Session.Network.EnqueueSend(new GameMessageSystemChat($"Your innate {TransferFromAttribute} must be above 10 to use the {Name}.", ChatMessageType.Broadcast));
        let text = format!("Your innate level of {from_name} is already as low as it can be. You may not reduce it any further.");
        send_weenie_error_with_string(
            w,
            player,
            WeenieErrorWithString::AttributeTransferFromTooLow,
            &text,
        );
        return false;
    }

    if transfer_to_attribute.starting_value(obj(w, player)) >= 100 {
        //player.Session.Network.EnqueueSend(new GameMessageSystemChat($"Your innate {TransferToAttribute} must be below 100 to use the {Name}.", ChatMessageType.Broadcast));
        let to_name = obj(w, this).transfer_to_attribute().to_dotnet_string();
        let text = format!("Your innate level of {to_name} is already as high as it can be. You may not increase it any further.");
        send_weenie_error_with_string(
            w,
            player,
            WeenieErrorWithString::AttributeTransferToTooHigh,
            &text,
        );
        return false;
    }

    true
}

// ACE: AttributeTransferDevice.CheckWieldedItems
/// Checks wielded items and their requirements to see if they'd be violated by an impending
/// attribute transfer operation.
fn check_wielded_items(w: &World, player: ObjectGuid) -> bool {
    for equipped_item in
        crate::world_objects::creature_equipment::equipped_objects_values(w, player)
    {
        let e = obj(w, equipped_item);
        if check_wield_requirement(
            e.wield_requirements(),
            e.wield_skill_type(),
            e.wield_difficulty(),
        ) || check_wield_requirement(
            e.wield_requirements2(),
            e.wield_skill_type2(),
            e.wield_difficulty2(),
        ) || check_wield_requirement(
            e.wield_requirements3(),
            e.wield_skill_type3(),
            e.wield_difficulty3(),
        ) || check_wield_requirement(
            e.wield_requirements4(),
            e.wield_skill_type4(),
            e.wield_difficulty4(),
        )
        //||
        //CheckActivationRequirements(player, equippedItem))
        {
            return true;
        }
    }
    false
}

// ACE: AttributeTransferDevice.CheckWieldRequirement
fn check_wield_requirement(
    item_wield_req: WieldRequirement,
    _wield_skill_type: Option<i32>,
    _wield_skill_difficulty: Option<i32>,
) -> bool {
    //if (itemWieldReq == WieldRequirement.RawSkill || itemWieldReq == WieldRequirement.Skill
    //    || itemWieldReq == WieldRequirement.RawAttrib || itemWieldReq == WieldRequirement.Attrib
    //    || itemWieldReq == WieldRequirement.RawSecondaryAttrib || itemWieldReq == WieldRequirement.SecondaryAttrib
    //    || itemWieldReq == WieldRequirement.Training)
    //if ((itemWieldReq >= WieldRequirement.Skill && itemWieldReq <= WieldRequirement.RawSecondaryAttrib) || itemWieldReq == WieldRequirement.Training)
    item_wield_req == WieldRequirement::RawAttrib || item_wield_req == WieldRequirement::Attrib
}

// ACE: AttributeTransferDevice.CheckActivationRequirements
/// Whether an equipped item has an activation requirement. ACE never calls it (its one use, in
/// `CheckWieldedItems`, is commented out).
#[allow(dead_code)] // unused in ACE: its one caller is commented out
fn check_activation_requirements(
    w: &mut World,
    player: ObjectGuid,
    equipped_item: ObjectGuid,
) -> bool {
    use empyrean_common::dotnet::CsCast;

    let (
        item_difficulty,
        item_skill_limit,
        item_specialized_only,
        use_requires_skill,
        use_requires_skill_spec,
    ) = {
        let e = obj(w, equipped_item);
        (
            e.item_difficulty(),
            e.item_skill_limit(),
            e.item_specialized_only(),
            e.use_requires_skill(),
            e.use_requires_skill_spec(),
        )
    };
    if item_difficulty.is_some_and(|d| d > 0) {
        return true;
    }

    let convert = |w: &mut World, skill: Skill| {
        crate::world_objects::world_object::convert_to_mo_a_skill(w, player, skill)
    };

    if convert(w, item_skill_limit.unwrap_or(Skill::None)) != Skill::None {
        return true;
    }

    if convert(w, item_specialized_only.unwrap_or(Skill::None)) != Skill::None {
        return true;
    }

    if convert(w, Skill(use_requires_skill.unwrap_or(0).cs_cast())) != Skill::None {
        return true;
    }

    if convert(w, Skill(use_requires_skill_spec.unwrap_or(0).cs_cast())) != Skill::None {
        return true;
    }

    false
}

// ---- constructors and SetEphemeralValues ----

/// `new AttributeTransferDevice(weenie, guid)` / `new AttributeTransferDevice(biota)`: the `WorldObject` constructor, then
/// AttributeTransferDevice's `SetEphemeralValues`.
// ACE: AttributeTransferDevice.AttributeTransferDevice
pub fn attribute_transfer_device_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    attribute_transfer_device_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: AttributeTransferDevice.SetEphemeralValues
fn attribute_transfer_device_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
