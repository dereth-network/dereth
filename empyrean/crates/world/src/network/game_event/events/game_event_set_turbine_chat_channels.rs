// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventSetTurbineChatChannels.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventSetTurbineChatChannels.cs`.

use dereth_protocol::comms as proto;
use empyrean_net::GameMessageGroup;

use crate::entity::turbine_chat_channel;
use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventSetTurbineChatChannels.GameEventSetTurbineChatChannels
/// `allegiance` and `society` default to 0 in ACE.
/// Every room but the allegiance's and the player's own society's, as the message lists them.
pub const ALL_ROOMS: [u32; 8] = [
    turbine_chat_channel::GENERAL,
    turbine_chat_channel::TRADE,
    turbine_chat_channel::LFG,
    turbine_chat_channel::ROLEPLAY,
    turbine_chat_channel::OLTHOI,
    turbine_chat_channel::SOCIETY_CELESTIAL_HAND,
    turbine_chat_channel::SOCIETY_ELDRYTCH_WEB,
    turbine_chat_channel::SOCIETY_RADIANT_BLOOD,
];

#[must_use]
pub fn game_event_set_turbine_chat_channels(
    session: &mut SessionData,
    allegiance: u32,
    society: u32,
    open: &[u32],
) -> GameMessage {
    let room = |channel: u32| if open.contains(&channel) { channel } else { 0 };
    let body = proto::ChatRoomMembership {
        allegiance_room: allegiance,
        general_room: room(turbine_chat_channel::GENERAL),
        trade_room: room(turbine_chat_channel::TRADE),
        lfg_room: room(turbine_chat_channel::LFG),
        roleplay_room: room(turbine_chat_channel::ROLEPLAY),
        olthoi_room: room(turbine_chat_channel::OLTHOI),
        society_room: society,
        society_celhan_room: room(turbine_chat_channel::SOCIETY_CELESTIAL_HAND),
        society_eldweb_room: room(turbine_chat_channel::SOCIETY_ELDRYTCH_WEB),
        society_radblo_room: room(turbine_chat_channel::SOCIETY_RADIANT_BLOOD),
    };
    game_event_from_proto(
        GameEventType::SetTurbineChatChannels,
        GameMessageGroup::UIQueue,
        session,
        &body,
    )
}
