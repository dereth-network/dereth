// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAddToTrade.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAddToTrade.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::trade as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::TradeSide;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventAddToTrade.GameEventAddToTrade
#[must_use]
pub fn game_event_add_to_trade(
    session: &mut SessionData,
    object_guid: u32,
    trade_side: TradeSide,
) -> GameMessage {
    let body = proto::TradeAddToTradeRecv {
        item: ObjectId(object_guid),
        side: trade_side.0.cs_cast(),
        container_properties: 0, // location / slot
    };
    game_event_from_proto(
        GameEventType::AddToTrade,
        GameMessageGroup::UIQueue,
        session,
        &body,
    )
}
