// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventOpponentStalemate.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventOpponentStalemate.cs`.

use dereth_protocol::trade as proto;
use empyrean_entity::enums::ChessColor;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventOpponentStalemate.GameEventOpponentStalemate
#[must_use]
pub fn game_event_opponent_stalemate(
    session: &mut SessionData,
    board_guid: ObjectGuid,
    color: ChessColor,
    stalemate: bool,
) -> GameMessage {
    // `on`: 1 = offering stalemate, 0 = retracting stalemate
    game_event_from_proto(
        GameEventType::OpponentStalemate,
        GameMessageGroup::UIQueue,
        session,
        &proto::GameOpponentStalemateState {
            game_id: board_guid.full(),
            team: color.0,
            on: i32::from(stalemate),
        },
    )
}
