// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceInfoResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceInfoResponse.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::allegiance_profile::{self, AllegianceProfile};
use crate::sessions::SessionData;

// ACE: GameEventAllegianceInfoResponse.GameEventAllegianceInfoResponse
#[must_use]
pub fn game_event_allegiance_info_response(
    session: &mut SessionData,
    player_guid: u32,
    profile: &AllegianceProfile,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::AllegianceInfoResponse,
        GameMessageGroup::UIQueue,
        session,
    );
    msg.data.write_u32(player_guid);
    allegiance_profile::write(&mut msg.data, profile);
    msg
}
