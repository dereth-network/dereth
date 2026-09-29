// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicUpdateEnchantment.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicUpdateEnchantment.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::enchantment::{self, Enchantment};
use crate::sessions::SessionData;

// ACE: GameEventMagicUpdateEnchantment.GameEventMagicUpdateEnchantment
#[must_use]
pub fn game_event_magic_update_enchantment(
    session: &mut SessionData,
    enchantment: &Enchantment,
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::MagicUpdateEnchantment,
        GameMessageGroup::UIQueue,
        session,
        68,
    );
    enchantment::write(&mut msg.data, enchantment);
    msg
}
