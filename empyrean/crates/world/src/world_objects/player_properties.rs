// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Properties.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Properties.cs`.

use empyrean_entity::enums::{
    AccessLevel, DamageType, PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId,
    PropertyInt, PropertyInt64, PropertyString,
};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_public_update_data_id::game_message_public_update_data_id;
use crate::network::game_messages::messages::game_message_public_update_instance_id::game_message_public_update_instance_id;
use crate::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;
use crate::network::game_messages::messages::game_message_public_update_property_float::game_message_public_update_property_float;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_public_update_property_int64::game_message_public_update_property_int64;
use crate::network::game_messages::messages::game_message_public_update_property_string::game_message_public_update_property_string;
use crate::world_objects::entity::creature_attribute::{em, StatCtx};
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::enqueue_broadcast;
use crate::World;

/// Non-property fields declared in `Player_Properties.cs`.
#[derive(Debug, Default)]
pub struct PlayerPropertiesFields {
    /// Flag indicates if player is an Olthoi Player
    // ACE: Player.IsOlthoiPlayer
    pub is_olthoi_player: bool,
    /// Flag indicates if player is a Gear Knight and Core Plating server option (gearknight_core_plating) is enforced
    // ACE: Player.IsGearKnightPlayer
    pub is_gear_knight_player: bool,
}

/// Will return 1.0f if no vitae exists. The player is the stat context's creature (in the world,
/// or detached while it is being constructed).
// ACE: Player.Vitae
#[must_use]
pub fn player_vitae(c: &StatCtx<'_>) -> f32 {
    let vitae = em::get_vitae(c);

    // `if (vitae == null) return 1.0f; return vitae.StatModValue;`
    vitae.unwrap_or(1.0f32)
}

// ACE: Player.GetNameWithSuffix
/// The name chat shows: an Olthoi player's name ends in `^` with `NoOlthoiTalk`, else in `&`.
#[must_use]
pub fn get_name_with_suffix(w: &crate::World, this: empyrean_entity::ObjectGuid) -> String {
    let name = crate::dispatch::name::name(w, this).unwrap_or_default();
    let o = w.objects.get(this).expect("ACE: this is null");
    let is_olthoi_player = o
        .player
        .as_ref()
        .is_some_and(|p| p.player_properties.is_olthoi_player);
    if !is_olthoi_player {
        name
    } else if o.no_olthoi_talk() {
        name + "^"
    } else {
        name + "&"
    }
}

// ACE: Player.IsPlussed
/// `(Character != null && Character.IsPlussed) || (Session != null &&
/// ConfigManager.Config.Server.Accounts.OverrideCharacterPermissions && Session.AccessLevel >
/// AccessLevel.Advocate)`.
#[must_use]
pub fn is_plussed(w: &World, this: ObjectGuid) -> bool {
    let character_plussed = w
        .objects
        .get(this)
        .and_then(crate::world_objects::world_object_networking::shims::player_character)
        .is_some_and(|c| c.is_plussed);
    if character_plussed {
        return true;
    }
    let Some(session) = crate::managers::player_manager::player_session(w, this) else {
        return false;
    };
    w.auth
        .lock()
        .accounts_config()
        .override_character_permissions
        && w.sessions
            .get(session)
            .is_some_and(|s| s.access_level > AccessLevel::Advocate)
}

// ACE: Player.HasVitae
/// `EnchantmentManager.HasVitae`.
pub fn has_vitae(w: &mut World, this: ObjectGuid) -> bool {
    emc::has_vitae(w, this)
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyInt prop, int? value, bool broadcast = false)`.
pub fn update_property_int(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyInt,
    value: Option<i32>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v),
        None => o.remove_property(prop),
    }
    let msg = game_message_public_update_property_int(o, prop, value.unwrap_or(0));

    send_network(w, this, msg, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyBool prop, bool? value, bool broadcast = false)`.
pub fn update_property_bool(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyBool,
    value: Option<bool>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v),
        None => o.remove_property(prop),
    }
    let msg = game_message_public_update_property_bool(o, prop, value.unwrap_or(false));

    send_network(w, this, msg, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyFloat prop, double? value, bool broadcast = false)`.
pub fn update_property_float(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyFloat,
    value: Option<f64>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v),
        None => o.remove_property(prop),
    }
    let msg = game_message_public_update_property_float(o, prop, value.unwrap_or(0.0));

    send_network(w, this, msg, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyDataId prop, uint? value, bool broadcast = false)`.
pub fn update_property_data_id(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyDataId,
    value: Option<u32>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v),
        None => o.remove_property(prop),
    }
    let msg = game_message_public_update_data_id(o, prop, value.unwrap_or(0));

    send_network(w, this, msg, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(PropertyInstanceId prop, uint? value, bool broadcast = false)`: updates a
/// property on the server, and broadcasts to appropriate players (if `broadcast`, also to all
/// players who know about this player).
pub fn update_property_instance_id_self(
    w: &mut World,
    this: ObjectGuid,
    prop: PropertyInstanceId,
    value: Option<u32>,
    broadcast: bool,
) {
    update_property_instance_id(w, this, this, prop, value, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyInstanceId prop, uint? value, bool broadcast = false)`.
pub fn update_property_instance_id(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyInstanceId,
    value: Option<u32>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v),
        None => o.remove_property(prop),
    }
    let msg = game_message_public_update_instance_id(o, prop, ObjectGuid::new(value.unwrap_or(0)));

    send_network(w, this, msg, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyString prop, string value, bool broadcast = false)`.
pub fn update_property_string(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyString,
    value: Option<&str>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v.to_owned()),
        None => o.remove_property(prop),
    }
    // the client seems to cache these values somewhere,
    // and the object will not update until relogging or CO
    let msg = game_message_public_update_property_string(o, prop, value);

    send_network(w, this, msg, broadcast);
}

// ACE: Player.UpdateProperty
/// `UpdateProperty(WorldObject obj, PropertyInt64 prop, long? value, bool broadcast = false)`.
pub fn update_property_int64(
    w: &mut World,
    this: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyInt64,
    value: Option<i64>,
    broadcast: bool,
) {
    let o = obj_mut(w, obj);
    match value {
        Some(v) => o.set_property(prop, v),
        None => o.remove_property(prop),
    }
    let msg = game_message_public_update_property_int64(o, prop, value.unwrap_or(0));

    send_network(w, this, msg, broadcast);
}

// ACE: Player.SendNetwork
pub fn send_network(w: &mut World, this: ObjectGuid, msg: GameMessage, broadcast: bool) {
    if broadcast {
        enqueue_broadcast(w, this, true, &[msg]);
    } else {
        let session = crate::managers::player_manager::player_session(w, this)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        enqueue_send(w, session, msg);
    }
}

// ACE: Player.GetAugmentationResistance
/// Returns player's augmentation resistance for damage type.
#[must_use]
pub fn get_augmentation_resistance(o: &WorldObject, damage_type: DamageType) -> i32 {
    match damage_type {
        DamageType::Slash => o.augmentation_resistance_slash(),
        DamageType::Pierce => o.augmentation_resistance_pierce(),
        DamageType::Bludgeon => o.augmentation_resistance_blunt(),
        DamageType::Fire => o.augmentation_resistance_fire(),
        DamageType::Cold => o.augmentation_resistance_frost(),
        DamageType::Acid => o.augmentation_resistance_acid(),
        DamageType::Electric => o.augmentation_resistance_lightning(),
        _ => 0,
    }
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .expect("ACE: obj is null (NullReferenceException)")
}
