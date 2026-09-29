// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicDispelMultipleEnchantments.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicDispelMultipleEnchantments.cs`.

use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::layered_spell::{self, LayeredSpell};
use crate::sessions::SessionData;

// ACE: GameEventMagicDispelMultipleEnchantments.GameEventMagicDispelMultipleEnchantments
/// `GameEventMagicDispelMultipleEnchantments(Session, List<LayeredSpell>)`.
#[must_use]
pub fn game_event_magic_dispel_multiple_enchantments(
    session: &mut SessionData,
    spells: &[LayeredSpell],
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::MagicDispelMultipleEnchantments,
        GameMessageGroup::UIQueue,
        session,
        204,
    );
    layered_spell::write_list(&mut msg.data, spells);
    msg
}

/// `GameEventMagicDispelMultipleEnchantments(Session, List<PropertiesEnchantmentRegistry>)`.
#[must_use]
pub fn game_event_magic_dispel_multiple_enchantments_registry(
    session: &mut SessionData,
    enchantments: &[PropertiesEnchantmentRegistry],
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::MagicDispelMultipleEnchantments,
        GameMessageGroup::UIQueue,
        session,
        204,
    );
    layered_spell::write_registry_list(&mut msg.data, enchantments);
    msg
}
