// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipQuit.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipQuit.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::social as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventFellowshipQuit.GameEventFellowshipQuit
#[must_use]
pub fn game_event_fellowship_quit(session: &mut SessionData, player_id: u32) -> GameMessage {
    game_event_from_proto(
        GameEventType::FellowshipQuit,
        GameMessageGroup::UIQueue,
        session,
        &proto::FellowshipQuitNotice {
            member: ObjectId(player_id),
        },
    )
}
