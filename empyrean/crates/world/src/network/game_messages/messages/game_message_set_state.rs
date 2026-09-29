// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageSetState.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageSetState.cs`.

use dereth_protocol::objects as proto;
use dereth_protocol::types::PhysicsEventStamp;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::PhysicsState;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ushort_sequence;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessageSetState.GameMessageSetState
#[must_use]
pub fn game_message_set_state(
    world_object: &mut impl HasSequences,
    state: PhysicsState,
) -> GameMessage {
    let id = world_object.world_object().guid.into();
    let instance = ushort_sequence(
        &world_object
            .sequences()
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    let event = ushort_sequence(
        &world_object
            .sequences()
            .get_next_sequence(SequenceType::ObjectState),
    );
    GameMessage::from_proto(
        GameMessageOpcode::SetState,
        GameMessageGroup::SmartboxQueue,
        &proto::ItemSetState {
            state: state.0.cs_cast(),
            id,
            timestamps: PhysicsEventStamp { instance, event },
        },
    )
}
