// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventMoveResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventMoveResponse.cs`.

use dereth_protocol::trade as proto;
use empyrean_entity::enums::ChessMoveResult;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventMoveResponse.GameEventMoveResponse
#[must_use]
pub fn game_event_move_response(
    session: &mut SessionData,
    board_guid: ObjectGuid,
    result: ChessMoveResult,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::MoveResponse,
        GameMessageGroup::UIQueue,
        session,
        &proto::GameMoveResponse {
            game_id: board_guid.full(),
            result: result.0,
        },
    )
}
