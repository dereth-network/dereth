// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventDeclineTrade.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventDeclineTrade.cs`.

use dereth_protocol::trade as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventDeclineTrade.GameEventDeclineTrade
#[must_use]
pub fn game_event_decline_trade(
    session: &mut SessionData,
    who_declined: ObjectGuid,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::DeclineTrade,
        GameMessageGroup::UIQueue,
        session,
        &proto::TradeDeclineTradeRecv {
            source: who_declined.into(),
        },
    )
}
