// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventPortalStormImminent.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventPortalStormImminent.cs`.

use dereth_protocol::trade as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventPortalStormImminent.GameEventPortalStormImminent
/// `extent` defaults to 0.6f in ACE.
#[must_use]
pub fn game_event_portal_storm_imminent(session: &mut SessionData, extent: f32) -> GameMessage {
    game_event_from_proto(
        GameEventType::MiscPortalStormImminent,
        GameMessageGroup::UIQueue,
        session,
        &proto::MiscPortalStormImminent { extent },
    )
}
