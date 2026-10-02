// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Luminance.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Luminance.cs`: earning, granting and spending
//! luminance.

use empyrean_common::dotnet::math::round as math_round;
use empyrean_common::dotnet::{format as dotnet_format, CsCast};
use empyrean_entity::enums::{ChatMessageType, PropertyInt64, ShareType, XpType};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::messages::game_message_private_update_property_int64::game_message_private_update_property_int64;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player_skills::send;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Player_Luminance.cs`.
#[derive(Debug, Default)]
pub struct PlayerLuminanceFields {}

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

// ACE: Player.EarnLuminance
/// Applies luminance modifiers before adding luminance (`shareType` defaults to `All`).
pub fn earn_luminance(
    w: &mut World,
    this: ObjectGuid,
    amount: i64,
    xp_type: XpType,
    share_type: ShareType,
) {
    if crate::entity::damage_history_info::player_is_olthoi_player(w, this) {
        return;
    }

    // following the same model as Player_Xp
    let quest_modifier =
        crate::managers::property_manager::get_double(w, "quest_lum_modifier", 0.0, true).item;
    let mut modifier =
        crate::managers::property_manager::get_double(w, "luminance_modifier", 0.0, true).item;
    if xp_type == XpType::Quest {
        modifier *= quest_modifier;
    }

    // should this be passed upstream to fellowship?
    let enchantment =
        crate::world_objects::player_xp::get_xp_and_luminance_modifier(w, this, xp_type);

    // `amount * enchantment` is long * float (a float), then times the double modifier
    let amount_f: f32 = amount.cs_cast();
    let m_amount: i64 = math_round(f64::from(amount_f * enchantment) * modifier).cs_cast();

    grant_luminance(w, this, m_amount, xp_type, share_type);
}

// ACE: Player.GrantLuminance
/// Directly grants luminance to the player, without any additional luminance modifiers
/// (`shareType` defaults to `All`).
pub fn grant_luminance(
    w: &mut World,
    this: ObjectGuid,
    amount: i64,
    xp_type: XpType,
    share_type: ShareType,
) {
    if crate::entity::damage_history_info::player_is_olthoi_player(w, this) {
        return;
    }

    // DIVERGE: a world without luminance (`EraFeatures::luminance`) awards none, by kill, quest
    // or fellowship share (V425).
    if !w.era.features.luminance {
        return;
    }

    let fellowship = crate::world_objects::player_fellowship::fellowship(w, this);
    match fellowship {
        Some(fellowship)
            if fellowship.get(w).share_xp && share_type.contains(ShareType::Fellowship) =>
        {
            // this will divy up the luminance, and re-call this function
            // with ShareType.Fellowship removed
            crate::entity::fellowship::split_luminance(
                w,
                &fellowship,
                amount.cs_cast(),
                xp_type,
                share_type,
                this,
            );
        }
        _ => add_luminance(w, this, amount, xp_type),
    }
}

// ACE: Player.AddLuminance
fn add_luminance(w: &mut World, this: ObjectGuid, amount: i64, xp_type: XpType) {
    let available = obj(w, this).available_luminance().unwrap_or(0);
    let maximum = obj(w, this).maximum_luminance().unwrap_or(0);

    if available == maximum {
        return;
    }

    // this is similar to Player_Xp.UpdateXpAndLevel()

    let remaining = maximum.wrapping_sub(available);

    let add_amount = amount.min(remaining);

    obj_mut(w, this).set_available_luminance(Some(available.wrapping_add(add_amount)));

    if xp_type == XpType::Quest {
        let msg = format!("You've earned {} Luminance.", dotnet_format(amount, "N0"));
        send(
            w,
            this,
            [game_message_system_chat(&msg, ChatMessageType::Broadcast)],
        );
    }

    update_luminance(w, this);
}

// ACE: Player.SpendLuminance
/// Spends the amount of luminance specified, deducting it from available luminance.
pub fn spend_luminance(w: &mut World, this: ObjectGuid, amount: i64) -> bool {
    // DIVERGE: a world without luminance (`EraFeatures::luminance`) spends none, so every
    // luminance augmentation's purchase fails as an unaffordable one does (V425).
    if !w.era.features.luminance {
        return false;
    }

    let available = obj(w, this).available_luminance().unwrap_or(0);

    if amount > available {
        return false;
    }

    obj_mut(w, this).set_available_luminance(Some(available.wrapping_sub(amount)));

    update_luminance(w, this);

    true
}

// ACE: Player.UpdateLuminance
/// Sends network message to update luminance.
fn update_luminance(w: &mut World, this: ObjectGuid) {
    let o = obj_mut(w, this);
    let value = o.available_luminance().unwrap_or(0);
    let msg =
        game_message_private_update_property_int64(o, PropertyInt64::AvailableLuminance, value);
    send(w, this, [msg]);
}
