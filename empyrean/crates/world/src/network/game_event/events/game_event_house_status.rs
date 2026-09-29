// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseStatus.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseStatus.cs`.

use dereth_protocol::trade as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventHouseStatus.GameEventHouseStatus
/// `weenieError` defaults to `WeenieError.BadParam` in ACE.
#[must_use]
pub fn game_event_house_status(
    session: &mut SessionData,
    weenie_error: WeenieError,
) -> GameMessage {
    //var noticeType = 2u;    // type of message to display
    game_event_from_proto(
        GameEventType::HouseStatus,
        GameMessageGroup::UIQueue,
        session,
        &proto::HouseHouseStatus {
            notice_type: weenie_error.0.cs_cast(),
        },
    )
}
