// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePickupEvent.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePickupEvent.cs`.

use dereth_protocol::objects as proto;
use dereth_protocol::types::PhysicsEventStamp;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ushort_sequence;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePickupEvent.GameMessagePickupEvent
#[must_use]
pub fn game_message_pickup_event(target_item: &mut impl HasSequences) -> GameMessage {
    let id = target_item.world_object().guid.into();
    let instance = ushort_sequence(
        &target_item
            .sequences()
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    let event = ushort_sequence(
        &target_item
            .sequences()
            .get_next_sequence(SequenceType::ObjectPosition),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PickupEvent,
        GameMessageGroup::SmartboxQueue,
        &proto::InventoryPickupEvent {
            id,
            timestamps: PhysicsEventStamp { instance, event },
        },
    )
}
