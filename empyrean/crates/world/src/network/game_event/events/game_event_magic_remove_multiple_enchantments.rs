// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicRemoveMultipleEnchantments.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicRemoveMultipleEnchantments.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::layered_spell::{self, LayeredSpell};
use crate::sessions::SessionData;

// ACE: GameEventMagicRemoveMultipleEnchantments.GameEventMagicRemoveMultipleEnchantments
#[must_use]
pub fn game_event_magic_remove_multiple_enchantments(
    session: &mut SessionData,
    spells: &[LayeredSpell],
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::MagicRemoveMultipleEnchantments,
        GameMessageGroup::UIQueue,
        session,
        56,
    );
    layered_spell::write_list(&mut msg.data, spells);
    msg
}
