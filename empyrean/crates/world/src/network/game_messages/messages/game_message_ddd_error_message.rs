// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDErrorMessage.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDErrorMessage.cs`.

use dereth_protocol::admin as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageDDDErrorMessage.GameMessageDDDErrorMessage
#[must_use]
pub fn game_message_ddd_error_message(
    resource_type: u32,
    data_id: u32,
    error_type: u32,
) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::DDD_ErrorMessage,
        GameMessageGroup::DatabaseQueue,
        &proto::DddError {
            resource_type,
            resource_id: data_id,
            error: error_type,
        },
    )
}
