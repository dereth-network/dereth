// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChannelIndex.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChannelIndex.cs`.

use empyrean_net::SessionId;

use crate::network::game_event::events::game_event_channel_index::game_event_channel_index;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::World;

// ACE: GameActionChannelIndex.Handle
/// The legacy channel names, for an admin, arch or PSR player.
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    // Probably need some IsAdvocate and IsSentinel type thing going on here as well. leaving for now
    let player = session_player(w, session);
    let o = w.objects.get(player).expect("ACE: session.Player is null");
    if !o.is_admin_prop() && !o.is_arch() && !o.is_psr() {
        return Ok(());
    }

    let msg = game_event_channel_index(w, session);
    enqueue_send(w, session, msg);
    Ok(())
}
