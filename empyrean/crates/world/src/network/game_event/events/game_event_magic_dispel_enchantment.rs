// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicDispelEnchantment.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicDispelEnchantment.cs`.

use dereth_protocol::qualities as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventMagicDispelEnchantment.GameEventMagicDispelEnchantment
#[must_use]
pub fn game_event_magic_dispel_enchantment(
    session: &mut SessionData,
    spell_id: u16,
    layer: u16,
) -> GameMessage {
    // `spellId` then `layer`, two ushorts: the layered spell id `spellId | layer << 16`.
    game_event_from_proto(
        GameEventType::MagicDispelEnchantment,
        GameMessageGroup::UIQueue,
        session,
        &proto::MagicDispelEnchantment {
            layered_spell_id: u32::from(spell_id) | (u32::from(layer) << 16),
        },
    )
}
