// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageDeleteObject.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDeleteObject.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ushort_sequence;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessageDeleteObject.GameMessageDeleteObject
#[must_use]
pub fn game_message_delete_object(world_object: &mut impl HasSequences) -> GameMessage {
    let guid = world_object.world_object().guid;
    let instance_sequence = ushort_sequence(
        &world_object
            .sequences()
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    // ACE ends the message with `Writer.Align()`; the shared message writes the same two bytes,
    // as the retail server did (V254).
    GameMessage::from_proto(
        GameMessageOpcode::ObjectDelete,
        GameMessageGroup::SmartboxQueue,
        &dereth_protocol::objects::ItemDeleteObject {
            id: guid.into(),
            instance_sequence,
        },
    )
}
