// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventChannelBroadcast.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventChannelBroadcast.cs`.

use dereth_protocol::comms as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::Channel;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventChannelBroadcast.GameEventChannelBroadcast
#[must_use]
pub fn game_event_channel_broadcast(
    session: &mut SessionData,
    chat_channel: Channel,
    sender_name: &str,
    message_text: &str,
) -> GameMessage {
    let body = proto::CommunicationChannelBroadcastRecv {
        channel: chat_channel.0.cs_cast(),
        sender_name: ace_str(sender_name), // This should be "" when sending back to initiator which makes client go "You Say..." instead of "PlayerName says..."
        message: ace_str(message_text),
    };
    game_event_from_proto_strings(
        GameEventType::ChannelBroadcast,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[sender_name, message_text],
    )
}
