// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDEndDDD.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDEndDDD.cs`.

use dereth_protocol::admin as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageDDDEndDDD.GameMessageDDDEndDDD
#[must_use]
pub fn game_message_ddd_end_ddd() -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::DDD_EndDDD,
        GameMessageGroup::DatabaseQueue,
        &proto::DddEndDdd,
    )
}
