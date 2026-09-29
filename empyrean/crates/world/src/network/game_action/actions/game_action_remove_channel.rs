// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveChannel.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveChannel.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{AccessLevel, Channel, WeenieErrorWithString};
use empyrean_net::SessionId;

use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::World;

// ACE: GameActionRemoveChannel.Handle
/// A staff member (or advocate) leaves one of its active legacy channels.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CommunicationRemoveFromChannel>()?;
    let chat_channel_id = Channel(m.channel.cs_cast());

    let player = session_player(w, session);
    let o = w.objects.get(player).expect("ACE: session.Player is null");
    if w.sessions.get(session).expect("the session").access_level == AccessLevel::Player
        && !o.is_advocate()
    {
        return Ok(());
    }

    if let Some(active) = o
        .channels_active()
        .filter(|active| active.contains(chat_channel_id))
    {
        w.objects
            .get_mut(player)
            .expect("ACE: session.Player is null")
            .set_channels_active(Some(active & !chat_channel_id));
        let msg = game_event_weenie_error_with_string(
            session_data(w, session),
            WeenieErrorWithString::YouHaveLeftThe_Channel,
            &chat_channel_id.to_string(),
        );
        enqueue_send(w, session, msg);
    }
    Ok(())
}
