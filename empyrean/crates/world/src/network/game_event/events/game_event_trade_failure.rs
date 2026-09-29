// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventTradeFailure.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventTradeFailure.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::trade as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventTradeFailure.GameEventTradeFailure
#[must_use]
pub fn game_event_trade_failure(
    session: &mut SessionData,
    object_guid: u32,
    reason: WeenieError,
) -> GameMessage {
    let body = proto::TradeTradeFailure {
        item: ObjectId(object_guid),
        reason: reason.0.cs_cast(),
    };
    game_event_from_proto(
        GameEventType::TradeFailure,
        GameMessageGroup::UIQueue,
        session,
        &body,
    )
}
