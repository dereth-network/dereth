// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Combat.cs, Source/ACE.Server/WorldObjects/Monster_Melee.cs, Source/ACE.Server/WorldObjects/Monster_Combat.cs, Source/ACE.Server/WorldObjects/Creature_Rating.cs, Source/ACE.Server/WorldObjects/Creature_Properties.cs
//! The damage pipeline's remaining adapters: the plain `Name` read for log lines, the
//! `GameEventMagicUpdateEnchantment` send, and the `virt_*` stand-ins for the virtual dispatch where
//! the generated one cannot carry a null weapon or a `CombatType`. Every other
//! member of these files is in its own port.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_entity::enums::{DamageType, PropertyString};
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;

use super::{object, CombatType};
use crate::network::game_event::events::game_event_magic_update_enchantment::game_event_magic_update_enchantment;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::World;

// ============================================================================== WorldObject / Creature

/// `Name` (the plain property; for log lines and chat text).
#[must_use]
pub fn name(w: &World, g: ObjectGuid) -> String {
    w.objects
        .get(g)
        .and_then(|o| o.get_property(PropertyString::Name))
        .unwrap_or_default()
}

// ============================================================================== Player (Player_*.cs)

fn session(w: &World, player: ObjectGuid) -> empyrean_net::SessionId {
    crate::world_objects::player_skills::session(w, player)
}

/// `playerTarget.Session.Network.EnqueueSend(new GameEventMagicUpdateEnchantment(session, new
/// Enchantment(playerTarget, addResult.Enchantment)))`.
///
/// # Panics
/// Without an enchantment (ACE: `NullReferenceException`), or a session.
pub fn send_update_enchantment(
    w: &mut World,
    player: ObjectGuid,
    entry: Option<&PropertiesEnchantmentRegistry>,
) {
    let entry = entry.expect("System.NullReferenceException: addResult.Enchantment");
    let enchantment =
        crate::network::structure::enchantment::enchantment_from_registry(w, player, entry);
    let s = session(w, player);
    let msg = game_event_magic_update_enchantment(session_data(w, s), &enchantment);
    enqueue_send(w, s, msg);
}

// ============================================================================== virtual dispatch stand-ins

/// `GetPowerMod(weapon)` (virtual; the generated dispatch cannot pass a null weapon).
#[must_use]
pub fn virt_get_power_mod(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    if object(w, this).is_player() {
        crate::world_objects::player_combat::get_power_mod(w, this, weapon)
    } else {
        1.0
    }
}

/// `GetAccuracyMod(weapon)` (virtual; the generated dispatch cannot pass a null weapon).
#[must_use]
pub fn virt_get_accuracy_mod(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    if object(w, this).is_player() {
        crate::world_objects::player_combat::get_accuracy_mod(w, this, weapon)
    } else {
        1.0
    }
}

/// `GetDamageType(multiple, combatType)` (virtual; the generated dispatch carries `()` for the
/// `CombatType`).
#[must_use]
pub fn virt_get_damage_type(
    w: &World,
    this: ObjectGuid,
    multiple: bool,
    combat_type: Option<CombatType>,
) -> DamageType {
    if object(w, this).is_player() {
        crate::world_objects::player_combat::get_damage_type(w, this, multiple, combat_type)
    } else {
        super::get_damage_type(w, this, multiple, combat_type)
    }
}

/// `GetHeritageBonus(weapon)` (virtual; the Player override is ported in `player_skills.rs`).
#[must_use]
pub fn virt_get_heritage_bonus(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> bool {
    if object(w, this).is_player() {
        match weapon {
            Some(weapon) => {
                crate::world_objects::player_skills::player_get_heritage_bonus(w, this, weapon)
            }
            None => false,
        }
    } else {
        super::get_heritage_bonus(w, this, weapon)
    }
}
