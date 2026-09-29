// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventCloseTrade.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventCloseTrade.cs`.

use dereth_protocol::trade as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::EndTradeReason;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventCloseTrade.GameEventCloseTrade
#[must_use]
pub fn game_event_close_trade(
    session: &mut SessionData,
    end_trade_reason: EndTradeReason,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::CloseTrade,
        GameMessageGroup::UIQueue,
        session,
        &proto::TradeCloseTrade {
            reason: end_trade_reason.0.cs_cast(),
        },
    )
}
