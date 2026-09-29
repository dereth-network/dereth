// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/ChatPacket.cs
//! Port of `Source/ACE.Server/Network/ChatPacket.cs`.
//!
//! ACE keeps this beside the transport (`Network/`), but it builds a `GameMessageSystemChat`, a
//! world message, so it lives in empyrean-world (empyrean-net cannot reach the world).

use empyrean_entity::enums::ChatMessageType;
use empyrean_net::SessionId;

use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::World;

// ACE: ChatPacket.SendServerMessage
/// A system chat line to `session`; with no session, nothing (ACE's "TODO: broadcast").
pub fn send_server_message(
    w: &mut World,
    session: Option<SessionId>,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    let Some(session) = session else {
        // TODO: broadcast
        return;
    };
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, chat_message_type),
    );
}
