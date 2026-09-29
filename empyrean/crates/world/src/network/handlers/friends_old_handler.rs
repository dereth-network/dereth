// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/FriendsOldHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/FriendsOldHandler.cs`.

use empyrean_entity::enums::ChatMessageType;
use empyrean_net::SessionId;

use crate::network::chat_packet;
use crate::network::managers::inbound_message_manager::{HandlerResult, Payload};
use crate::World;

// ACE: FriendsOldHandler.FriendsOld
/// The retail friends list opcode: the server answers that it does not use it.
// DIVERGE: ACE's reply says "in the emulator"; ours says "on this server" (brand).
pub fn friends_old(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    chat_packet::send_server_message(
        w,
        Some(session),
        "That command is not used on this server.",
        ChatMessageType::Broadcast,
    );
    Ok(())
}
