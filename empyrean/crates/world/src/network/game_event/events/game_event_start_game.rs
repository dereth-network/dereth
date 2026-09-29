// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventStartGame.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventStartGame.cs`.

use dereth_protocol::trade as proto;
use empyrean_entity::enums::ChessColor;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventStartGame.GameEventStartGame
#[must_use]
pub fn game_event_start_game(
    session: &mut SessionData,
    board_guid: ObjectGuid,
    color: ChessColor,
) -> GameMessage {
    // `team`: which team that should go first
    game_event_from_proto(
        GameEventType::StartGame,
        GameMessageGroup::UIQueue,
        session,
        &proto::GameStartGame {
            game_id: board_guid.full(),
            team: color.0,
        },
    )
}
