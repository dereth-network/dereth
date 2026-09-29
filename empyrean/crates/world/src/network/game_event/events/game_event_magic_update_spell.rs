// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMagicUpdateSpell.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMagicUpdateSpell.cs`.

use dereth_protocol::qualities as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventMagicUpdateSpell.GameEventMagicUpdateSpell
/// `layer` defaults to 0 in ACE.
#[must_use]
pub fn game_event_magic_update_spell(
    session: &mut SessionData,
    spell_id: u16,
    layer: u16,
) -> GameMessage {
    // `spellId` then `layer`, two ushorts: the layered spell id `spellId | layer << 16`.
    game_event_from_proto(
        GameEventType::MagicUpdateSpell,
        GameMessageGroup::UIQueue,
        session,
        &proto::MagicUpdateSpell {
            layered_spell_id: u32::from(spell_id) | (u32::from(layer) << 16),
        },
    )
}
