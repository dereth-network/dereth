// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventRegisterTrade.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventRegisterTrade.cs`.

use dereth_protocol::trade as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventRegisterTrade.GameEventRegisterTrade
#[must_use]
pub fn game_event_register_trade(
    session: &mut SessionData,
    initiator: ObjectGuid,
    partner: ObjectGuid,
) -> GameMessage {
    // ACE writes a `long` 0 as the last field; the double 0.0 is the same eight bytes.
    let body = proto::TradeRegisterTrade {
        initiator: initiator.into(),
        partner: partner.into(),
        stamp: 0.0,
    };
    game_event_from_proto(
        GameEventType::RegisterTrade,
        GameMessageGroup::UIQueue,
        session,
        &body,
    )
}
