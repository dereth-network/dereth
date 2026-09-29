// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicUpdateMultipleEnchantments.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicUpdateMultipleEnchantments.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::enchantment::{self, Enchantment};
use crate::sessions::SessionData;

// ACE: GameEventMagicUpdateMultipleEnchantments.GameEventMagicUpdateMultipleEnchantments
#[must_use]
pub fn game_event_magic_update_multiple_enchantments(
    session: &mut SessionData,
    enchantments: &[Enchantment],
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::MagicUpdateMultipleEnchantments,
        GameMessageGroup::UIQueue,
        session,
    );
    enchantment::write_list(&mut msg.data, enchantments);
    msg
}
