// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Attributes.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Attributes.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{ChatMessageType, PlayScript, PropertyAttribute, Sound};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::messages::game_message_private_update_attribute::game_message_private_update_attribute;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::entity::creature_attribute::CreatureAttribute;
use crate::world_objects::player_skills::{
    exceeds_available_experience, handle_run_rate_update, name, property_manager_get_bool, send,
    session, spend_xp,
};
use crate::world_objects::player_vitals::update_vital_message;
use crate::world_objects::world_object::{play_particle_effect, WorldObject};
use crate::World;

/// Non-property fields declared in `Player_Attributes.cs`.
#[derive(Debug, Default)]
pub struct PlayerAttributesFields {}

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

/// `new GameMessagePrivateUpdateAttribute(this, creatureAttribute)`.
fn update_attribute_message(
    w: &mut World,
    this: ObjectGuid,
    creature_attribute: CreatureAttribute,
) -> GameMessage {
    game_message_private_update_attribute(obj_mut(w, this), creature_attribute)
}

// ACE: Player.HandleActionRaiseAttribute
/// GameAction 0x45 RaiseAttribute: spends `amount` XP on a primary attribute.
pub fn handle_action_raise_attribute(
    w: &mut World,
    this: ObjectGuid,
    attribute: PropertyAttribute,
    amount: u32,
) -> bool {
    let Some(creature_attribute) = obj(w, this).attributes().get(&attribute).copied() else {
        log::warn!(
            "{}.HandleActionRaiseAttribute({}, {amount}) - invalid attribute",
            name(w, this),
            attribute.to_dotnet_string()
        );
        return false;
    };

    if exceeds_available_experience(w, this, i64::from(amount)) {
        log::warn!(
            "{}.HandleActionRaiseAttribute({}, {amount}) - amount > AvaiableExperience",
            name(w, this),
            attribute.to_dotnet_string()
        );
        return false;
    }

    let prev_rank = creature_attribute.ranks(obj(w, this));

    if !spend_attribute_xp(w, this, creature_attribute, amount, true) {
        let s = session(w, this);
        crate::network::chat_packet::send_server_message(
            w,
            Some(s),
            &format!(
                "Your attempt to raise {} has failed.",
                attribute.to_dotnet_string()
            ),
            ChatMessageType::Broadcast,
        );
        return false;
    }

    let update = update_attribute_message(w, this, creature_attribute);
    send(w, this, [update]);

    if prev_rank != creature_attribute.ranks(obj(w, this)) {
        // checks if max rank is achieved and plays fireworks w/ special text
        let mut suffix = "";
        if creature_attribute.is_max_rank(w, obj(w, this)) {
            // fireworks
            play_particle_effect(w, this, PlayScript::WeddingBliss, this, 1.0f32);
            suffix = " and has reached its upper limit";
        }

        let sound = game_message_sound(this, Sound::RaiseTrait, 1.0f32);
        let base = creature_attribute.base(obj(w, this));
        let msg = game_message_system_chat(
            &format!(
                "Your base {} is now {base}{suffix}!",
                attribute.to_dotnet_string()
            ),
            ChatMessageType::Advancement,
        );

        send(w, this, [sound, msg]);

        if attribute == PropertyAttribute::Endurance {
            // this packet appears to trigger client to update both health and stamina
            let health = obj(w, this).health();
            let update_health = update_vital_message(w, this, health);

            send(w, this, [update_health]);
        } else if attribute == PropertyAttribute::Self_ {
            let mana = obj(w, this).mana();
            let update_mana = update_vital_message(w, this, mana);

            send(w, this, [update_mana]);
        }

        // retail was missing the 'raise attribute' runrate hook here
        if (attribute == PropertyAttribute::Strength || attribute == PropertyAttribute::Quickness)
            && property_manager_get_bool(w, "runrate_add_hooks")
        {
            handle_run_rate_update(w, this);
        }
    }

    true
}

// ACE: Player.SpendAttributeXp
/// Spends `amount` on the attribute if it is below max rank and the amount fits; recomputes
/// ranks.
pub(crate) fn spend_attribute_xp(
    w: &mut World,
    this: ObjectGuid,
    creature_attribute: CreatureAttribute,
    amount: u32,
    send_network_update: bool,
) -> bool {
    // ensure attribute is not already max rank
    if creature_attribute.is_max_rank(w, obj(w, this)) {
        log::warn!(
            "{}.SpendAttributeXp({}, {amount}) - player tried to raise attribute beyond max rank",
            name(w, this),
            creature_attribute.attribute.to_dotnet_string()
        );
        return false;
    }

    // the client should already handle this naturally,
    // but ensure player can't spend xp beyond the max rank
    let amount_to_end = creature_attribute.experience_left(w, obj(w, this));

    if amount > amount_to_end {
        log::warn!(
            "{}.SpendAttributeXp({}, {amount}) - player tried to raise attribute beyond {amount_to_end} experience",
            name(w, this),
            creature_attribute.attribute.to_dotnet_string()
        );
        return false; // returning error here, instead of setting amount to amountToEnd
    }

    // everything looks good at this point,
    // spend xp on attribute
    if !spend_xp(w, this, i64::from(amount), send_network_update) {
        log::warn!(
            "{}.SpendAttributeXp({}, {amount}) - SpendXP failed",
            name(w, this),
            creature_attribute.attribute.to_dotnet_string()
        );
        return false;
    }

    let o = obj_mut(w, this);
    let spent = creature_attribute.experience_spent(o).wrapping_add(amount);
    creature_attribute.set_experience_spent(o, spent);

    // calculate new rank
    let rank: u16 = calc_attribute_rank(w, spent).cs_cast();
    creature_attribute.set_ranks(obj_mut(w, this), u32::from(rank));

    true
}

// ACE: Player.SpendAllAvailableAttributeXp
/// Spends as much available XP on the attribute as it can take.
pub fn spend_all_available_attribute_xp(
    w: &mut World,
    this: ObjectGuid,
    creature_attribute: CreatureAttribute,
    send_network_update: bool,
) {
    let mut amount_remaining = creature_attribute.experience_left(w, obj(w, this));

    if exceeds_available_experience(w, this, i64::from(amount_remaining)) {
        let available = obj(w, this).available_experience().unwrap_or(0);
        amount_remaining = available.cs_cast();
    }

    spend_attribute_xp(
        w,
        this,
        creature_attribute,
        amount_remaining,
        send_network_update,
    );
}

// ACE: Player.CalcAttributeRank
/// The highest attribute rank `xp_amount` buys. ACE's loop answers -1 below the first entry, which
/// is 0 in the dat, so no amount reaches it; the shared rules' search is the same function over
/// the dat's (ascending) table.
#[must_use]
pub fn calc_attribute_rank(w: &World, xp_amount: u32) -> i32 {
    let xp_table = w.dats.portal_dat().xp_table();
    let rank = dereth_rules::advancement::attribute_level_from_experience(xp_table, xp_amount);
    i32::try_from(rank).expect("a List index fits an int")
}
