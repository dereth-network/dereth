// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePlayerTeleport.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePlayerTeleport.cs`.

use dereth_protocol::objects as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ushort_sequence;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePlayerTeleport.GameMessagePlayerTeleport
#[must_use]
pub fn game_message_player_teleport(player: &mut impl HasSequences) -> GameMessage {
    let teleport_sequence = ushort_sequence(
        &player
            .sequences()
            .get_next_sequence(SequenceType::ObjectTeleport),
    );
    // ACE ends the message with `Writer.Align()`; the shared message writes the same two bytes,
    // as the retail server did (V254).
    GameMessage::from_proto(
        GameMessageOpcode::PlayerTeleport,
        GameMessageGroup::SmartboxQueue,
        &proto::EffectsPlayerTeleport { teleport_sequence },
    )
}
