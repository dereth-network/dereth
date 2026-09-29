// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicPurgeBadEnchantments.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicPurgeBadEnchantments.cs`.

use dereth_protocol::qualities as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventMagicPurgeBadEnchantments.GameEventMagicPurgeBadEnchantments
#[must_use]
pub fn game_event_magic_purge_bad_enchantments(session: &mut SessionData) -> GameMessage {
    game_event_from_proto(
        GameEventType::MagicPurgeBadEnchantments,
        GameMessageGroup::UIQueue,
        session,
        &proto::MagicPurgeBadEnchantments,
    )
}
