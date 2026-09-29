// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChannelList.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChannelList.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{AccessLevel, Channel};
use empyrean_net::SessionId;

use crate::network::game_event::events::game_event_channel_list::game_event_channel_list;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::World;

// ACE: GameActionChannelList.Handle
/// Who is on a legacy channel the (staff or advocate) player is allowed.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CommunicationChannelListRequest>()?;
    let chat_channel_id = Channel(m.channel.cs_cast());

    let player = session_player(w, session);
    let o = w.objects.get(player).expect("ACE: session.Player is null");
    if w.sessions.get(session).expect("the session").access_level == AccessLevel::Player
        && !o.is_advocate()
    {
        return Ok(());
    }

    if o.channels_allowed()
        .is_some_and(|allowed| allowed.contains(chat_channel_id))
    {
        let msg = game_event_channel_list(w, session, chat_channel_id);
        enqueue_send(w, session, msg);
    }
    Ok(())
}
