// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventGameOver.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventGameOver.cs`.

use dereth_protocol::trade as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventGameOver.GameEventGameOver
#[must_use]
pub fn game_event_game_over(
    session: &mut SessionData,
    board_guid: ObjectGuid,
    team_winner: i32,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::GameOver,
        GameMessageGroup::UIQueue,
        session,
        &proto::GameGameOver {
            game_id: board_guid.full(),
            team_winner,
        },
    )
}
