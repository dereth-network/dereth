// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseTransaction.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseTransaction.cs`.

use dereth_protocol::trade as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventHouseTransaction.GameEventHouseTransaction
#[must_use]
pub fn game_event_house_transaction(session: &mut SessionData) -> GameMessage {
    let notice_type: u32 = 2; // type of message to display
    game_event_from_proto(
        GameEventType::HouseTransaction,
        GameMessageGroup::UIQueue,
        session,
        &proto::HouseHouseTransaction { notice_type },
    )
}
