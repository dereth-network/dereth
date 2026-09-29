// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Advocate.cs
//! Port of `Source/ACE.Server/Entity/Advocate.cs`.
//!
//! ACE takes the player as a `WorldObject` and tests `as Player`; here it is the player's guid, and
//! "not a player" is an object without player state (or none at all).

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{Channel, ChatMessageType, PropertyBool, PropertyInt, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::entity::items_to_receive::ItemsToReceive;
use crate::factories::world_object_factory;
use crate::managers::player_manager;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_bool::game_message_private_update_property_bool;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player_inventory::{self, DequipObjectAction};
use crate::world_objects::world_object_networking::shims;
use crate::world_objects::{container, creature_equipment};
use crate::World;

/// `Advocate.AdvocateChannels`: `Help | Abuse | Advocate1 | Advocate2 | Advocate3`.
pub const ADVOCATE_CHANNELS: Channel = Channel(
    Channel::Help.0
        | Channel::Abuse.0
        | Channel::Advocate1.0
        | Channel::Advocate2.0
        | Channel::Advocate3.0,
);

/// `playerToBeBestowed as Player`: the guid when it names an object with player state.
fn as_player(w: &World, wo: ObjectGuid) -> Option<ObjectGuid> {
    w.objects.get(wo).filter(|o| o.player.is_some()).map(|_| wo)
}

/// `player.Session.Network.EnqueueSend(msg)`.
///
/// # Panics
/// A player without a session (ACE's `NullReferenceException`).
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session =
        player_manager::player_session(w, player).expect("NullReferenceException: player.Session");
    enqueue_send(w, session, msg);
}

fn player_bool(
    w: &mut World,
    player: ObjectGuid,
    property: PropertyBool,
    value: bool,
) -> GameMessage {
    let o = w
        .objects
        .get_mut(player)
        .expect("NullReferenceException: player");
    game_message_private_update_property_bool(o, property, value)
}

fn player_int(w: &mut World, player: ObjectGuid, property: PropertyInt, value: i32) -> GameMessage {
    let o = w
        .objects
        .get_mut(player)
        .expect("NullReferenceException: player");
    game_message_private_update_property_int(o, property, value)
}

fn advocate_level(w: &World, player: ObjectGuid) -> Option<i32> {
    w.objects.get(player).and_then(|o| o.advocate_level())
}

/// `WorldObjectFactory.CreateNewWorldObject(weenieClassName)`: the weenie by class name from the
/// world database, a new dynamic guid, and the object (in `World.objects`, where every Rust world
/// object lives before it enters the world). `None` when there is no such weenie.
fn create_new_world_object(w: &mut World, weenie_class_name: &str) -> Option<ObjectGuid> {
    let o = world_object_factory::create_new_world_object_by_name_detached(w, weenie_class_name)?;
    let guid = o.guid;
    assert!(w.objects.insert(o).is_ok(), "fresh dynamic guid");
    Some(guid)
}

/// `player.EquippedObjects.Values.Where(a => a.WeenieClassName.Equals(name, OrdinalIgnoreCase))`.
fn equipped_of_weenie_class(
    w: &World,
    player: ObjectGuid,
    weenie_class_name: &str,
) -> Vec<ObjectGuid> {
    creature_equipment::equipped_objects_values(w, player)
        .into_iter()
        .filter(|&g| {
            let wcid = w.objects.get(g).map_or(0, |o| o.biota.weenie_class_id);
            player_manager::equals_ordinal_ignore_case(
                &shims::weenie_class_name(w, wcid),
                weenie_class_name,
            )
        })
        .collect()
}

// ACE: Advocate.Bestow
pub fn bestow(
    w: &mut World,
    player_to_be_bestowed: ObjectGuid,
    advocate_level_to_bestow: i32,
) -> bool {
    if !(1..=7).contains(&advocate_level_to_bestow) {
        return false;
    }

    let Some(player) = as_player(w, player_to_be_bestowed) else {
        return false;
    };

    if !w
        .objects
        .get(player)
        .is_some_and(|o| o.advocate_quest_player())
    {
        if let Some(o) = w.objects.get_mut(player) {
            o.set_advocate_quest_player(true);
        }

        if !has_advocate_tome(w, player) {
            let use_create_item = create_new_world_object(w, "bookadvocatefane");

            if let Some(use_create_item) = use_create_item {
                player_inventory::try_create_in_inventory_with_networking(
                    w,
                    player,
                    use_create_item,
                );
            }
        }
    }

    if let Some(o) = w.objects.get_mut(player) {
        o.set_is_advocate(true);
    }

    let is_advocate = w.objects.get(player).is_some_and(|o| o.is_advocate());
    let msg = player_bool(w, player, PropertyBool::IsAdvocate, is_advocate);
    send(w, player, msg);

    if let Some(level) = advocate_level(w, player) {
        let mut objects_to_be_consumed = Vec::new();

        objects_to_be_consumed.extend(container::get_inventory_items_of_weenie_class(
            w,
            player,
            &format!("shieldadvocate{level}"),
        ));

        for wo in objects_to_be_consumed {
            player_inventory::try_consume_from_inventory_with_networking(w, player, wo, i32::MAX);
        }

        let mut equipped_aegis = Vec::new();
        equipped_aegis.extend(equipped_of_weenie_class(
            w,
            player,
            &format!("shieldadvocate{level}"),
        ));

        for wo in equipped_aegis {
            player_inventory::try_dequip_object_with_networking(
                w,
                player,
                wo,
                DequipObjectAction::ConsumeItem,
            );
        }
    }

    if let Some(o) = w.objects.get_mut(player) {
        o.set_advocate_level(Some(advocate_level_to_bestow));
    }
    let level = advocate_level(w, player);
    let msg = player_int(w, player, PropertyInt::AdvocateLevel, level.unwrap_or(1));
    send(w, player, msg);

    if level.is_some_and(|l| l > 4) {
        if let Some(o) = w.objects.get_mut(player) {
            o.set_is_psr(true);
        }
        let is_psr = w.objects.get(player).is_some_and(|o| o.is_psr());
        let msg = player_bool(w, player, PropertyBool::IsPsr, is_psr);
        send(w, player, msg);
    }

    if !has_advocate_instructions(w, player) {
        let use_create_item = create_new_world_object(w, "bookadvocateinstructions");

        if let Some(use_create_item) = use_create_item {
            player_inventory::try_create_in_inventory_with_networking(w, player, use_create_item);
        }
    }

    let level = advocate_level(w, player)
        .expect("InvalidOperationException: Nullable object must have a value.");
    let use_create_aegis = create_new_world_object(w, &format!("shieldadvocate{level}"));

    if let Some(use_create_aegis) = use_create_aegis {
        player_inventory::try_create_in_inventory_with_networking(w, player, use_create_aegis);
    }

    if let Some(o) = w.objects.get_mut(player) {
        o.set_channels_allowed(Some(ADVOCATE_CHANNELS | Channel::TownChans));
        o.set_channels_active(Some(ADVOCATE_CHANNELS | Channel::TownChans));
    }

    let level = advocate_level(w, player)
        .map(|l| l.to_string())
        .unwrap_or_default();
    send(
        w,
        player,
        game_message_system_chat(
            &format!("You have been bestowed as an Advocate, level {level}!"),
            ChatMessageType::Broadcast,
        ),
    );

    true
}

// ACE: Advocate.Remove
pub fn remove(w: &mut World, player_to_be_removed: ObjectGuid) -> bool {
    let Some(player) = as_player(w, player_to_be_removed) else {
        return false;
    };

    if !w
        .objects
        .get(player)
        .is_some_and(|o| o.advocate_quest_player())
    {
        return false;
    }

    let mut objects_to_be_consumed = Vec::new();

    objects_to_be_consumed.extend(container::get_inventory_items_of_weenie_class(
        w,
        player,
        "bookadvocatefane",
    ));
    objects_to_be_consumed.extend(container::get_inventory_items_of_weenie_class(
        w,
        player,
        "bookadvocateinstructions",
    ));

    for i in 1..8 {
        objects_to_be_consumed.extend(container::get_inventory_items_of_weenie_class(
            w,
            player,
            &format!("shieldadvocate{i}"),
        ));
    }

    for wo in objects_to_be_consumed {
        player_inventory::try_consume_from_inventory_with_networking(w, player, wo, i32::MAX);
    }

    let mut equipped_aegis = Vec::new();
    for i in 1..8 {
        equipped_aegis.extend(equipped_of_weenie_class(
            w,
            player,
            &format!("shieldadvocate{i}"),
        ));
    }

    for wo in equipped_aegis {
        player_inventory::try_dequip_object_with_networking(
            w,
            player,
            wo,
            DequipObjectAction::ConsumeItem,
        );
    }

    if let Some(o) = w.objects.get_mut(player) {
        o.set_channels_active(None);
        o.set_channels_allowed(None);

        o.set_advocate_level(None);
        o.set_is_advocate(false);
        o.set_is_psr(false);
    }

    let msg = player_int(w, player, PropertyInt::AdvocateLevel, 0);
    send(w, player, msg);
    let msg = player_bool(w, player, PropertyBool::IsAdvocate, false);
    send(w, player, msg);
    let msg = player_bool(w, player, PropertyBool::IsPsr, false);
    send(w, player, msg);

    if let Some(o) = w.objects.get_mut(player) {
        o.set_advocate_quest_player(false);
    }

    send(
        w,
        player,
        game_message_system_chat(
            "You have been removed from the Advocate ranks!",
            ChatMessageType::Broadcast,
        ),
    );

    true
}

// ACE: Advocate.IsAdvocateFane
/// # Panics
/// An object that is gone (ACE's `NullReferenceException`).
#[must_use]
pub fn is_advocate_fane(w: &World, wo: ObjectGuid) -> bool {
    w.objects
        .get(wo)
        .expect("NullReferenceException: wo")
        .biota
        .weenie_type
        == WeenieType::AdvocateFane
}

// ACE: Advocate.IsAdvocate
#[must_use]
pub fn is_advocate(w: &World, wo: ObjectGuid) -> bool {
    if let Some(player) = as_player(w, wo) {
        return w
            .objects
            .get(player)
            .is_some_and(|o| o.advocate_quest_player());
    }

    false
}

// ACE: Advocate.HasAdvocateTome
#[must_use]
pub fn has_advocate_tome(w: &World, player: ObjectGuid) -> bool {
    container::get_num_inventory_items_of_weenie_class(w, player, "bookadvocatefane") > 0
}

// ACE: Advocate.HasAdvocateInstructions
#[must_use]
pub fn has_advocate_instructions(w: &World, player: ObjectGuid) -> bool {
    container::get_num_inventory_items_of_weenie_class(w, player, "bookadvocateinstructions") > 0
}

// ACE: Advocate.HasItem
#[must_use]
pub fn has_item(w: &World, player: ObjectGuid, weenie_class_id: u32) -> bool {
    container::get_num_inventory_items_of_wcid(w, player, weenie_class_id) > 0
}

/// `Advocate.AdvocateBooks` (a static field; `Dictionary` insertion order).
#[must_use]
pub fn advocate_books() -> DotNetDict<&'static str, u32> {
    let mut d = DotNetDict::new();
    d.insert("bookadvocatefane", 3653);
    d.insert("bookadvocateinstructions", 3941);
    d
}

/// `Advocate.AdvocateItems` (a static field; `Dictionary` insertion order).
#[must_use]
pub fn advocate_items() -> DotNetDict<&'static str, u32> {
    let mut d = DotNetDict::new();
    d.insert("shieldadvocate1", 2628);
    d.insert("shieldadvocate2", 2629);
    d.insert("shieldadvocate3", 2630);
    d.insert("shieldadvocate4", 2631);
    d.insert("shieldadvocate5", 2632);
    d.insert("shieldadvocate6", 2633);
    d.insert("shieldadvocate7", 3594);
    d
}

/// `AdvocateItems[key]`.
///
/// # Panics
/// A key outside `shieldadvocate1..7` (ACE's `KeyNotFoundException`).
fn advocate_item(key: &str) -> u32 {
    *advocate_items().get(&key).unwrap_or_else(|| {
        panic!("KeyNotFoundException: The given key '{key}' was not present in the dictionary.")
    })
}

// ACE: Advocate.CanAcceptAdvocateItems
/// # Panics
/// A current or new advocate level outside 1..=7 (ACE's `KeyNotFoundException`).
#[must_use]
pub fn can_accept_advocate_items(w: &World, player: ObjectGuid, advocate_level: i32) -> bool {
    let mut items_to_receive = ItemsToReceive::new(w, player);

    for (_, &value) in advocate_books().iter() {
        if has_item(w, player, value) {
            continue;
        }

        items_to_receive.add(w, value, 1);
    }

    if let Some(level) = self::advocate_level(w, player) {
        let current_shield_wcid = advocate_item(&format!("shieldadvocate{level}"));

        if has_item(w, player, current_shield_wcid) {
            let current_shield =
                container::get_inventory_items_of_wcid(w, player, current_shield_wcid);

            for aegis in current_shield {
                let wcid = w.objects.get(aegis).map_or(0, |o| o.biota.weenie_class_id);
                items_to_receive.remove(w, wcid, 1);
            }
        }
    }

    items_to_receive.add(
        w,
        advocate_item(&format!("shieldadvocate{advocate_level}")),
        1,
    );

    if items_to_receive.player_exceeds_limits() {
        return false;
    }

    true
}
