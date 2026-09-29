// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventOpponentTurn.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventOpponentTurn.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::chess_move_data::{self, ChessMoveData};
use crate::sessions::SessionData;

// ACE: GameEventOpponentTurn.GameEventOpponentTurn
#[must_use]
pub fn game_event_opponent_turn(
    session: &mut SessionData,
    board_guid: ObjectGuid,
    move_data: &ChessMoveData,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::OpponentTurn,
        GameMessageGroup::UIQueue,
        session,
    );
    msg.data.write_u32(board_guid.full());
    msg.data.write_i32(move_data.color.0);
    chess_move_data::write(&mut msg.data, move_data);
    msg
}
