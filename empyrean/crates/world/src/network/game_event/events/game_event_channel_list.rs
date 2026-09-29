// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventChannelList.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventChannelList.cs`.

use dereth_protocol::comms as proto;
use empyrean_entity::enums::Channel;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::managers::player_manager;
use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventChannelList.GameEventChannelList
/// The names of the online players whose `ChannelsActive` has `chat_channel`, in
/// `PlayerManager.GetAllOnline()` order, after their count.
#[must_use]
pub fn game_event_channel_list(
    w: &mut World,
    session: SessionId,
    chat_channel: Channel,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::ChannelList,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );

    let mut player_names = Vec::new();
    for client in player_manager::get_all_online(w) {
        let Some(o) = w.objects.get(client) else {
            continue;
        };
        if !o
            .channels_active()
            .unwrap_or(Channel(0))
            .contains(chat_channel)
        {
            continue;
        }
        player_names.push(crate::dispatch::name::name(w, client).unwrap_or_default());
    }

    // `numClientsConnected`, then each name.
    let body = proto::CommunicationChannelListRecv {
        names: player_names.iter().map(|n| ace_str(n.as_str())).collect(),
    };
    let strings: Vec<&str> = player_names.iter().map(String::as_str).collect();
    msg.write_proto_strings(&body, &strings);
    msg
}
