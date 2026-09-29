// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageSetStackSize.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageSetStackSize.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::PropertyInt;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::byte_sequence;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessageSetStackSize.GameMessageSetStackSize
#[must_use]
pub fn game_message_set_stack_size(world_object: &mut impl HasSequences) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyInt, PropertyInt::StackSize),
    );
    let wo = world_object.world_object();
    let stack_size: u32 = wo.stack_size().unwrap_or(0).cs_cast();
    let value: u32 = wo.value().unwrap_or(0).cs_cast();
    GameMessage::from_proto(
        GameMessageOpcode::SetStackSize,
        GameMessageGroup::UIQueue,
        &dereth_protocol::items::ItemUpdateStackSize {
            sequence,
            item: wo.guid.into(),
            amount: stack_size,
            new_value: value,
        },
    )
}
