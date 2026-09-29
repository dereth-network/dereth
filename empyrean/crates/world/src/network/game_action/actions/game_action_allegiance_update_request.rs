// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceUpdateRequest.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceUpdateRequest.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::entity::i_player::IPlayer;
use crate::network::managers::inbound_message_manager::{to_boolean, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionAllegianceUpdateRequest.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::AllegianceUpdateRequest>()?;
    let ui_panel = to_boolean(m.on_off);

    let player = w.sessions.get(session).and_then(|s| s.player);

    // Not ACE's (retail, V281; the retail captures): ACE ignores the flag; retail kept it
    // as the player's allegiance-update subscription, which gates the tick's AllegianceUpdateDone.
    if let Some(player) = player {
        crate::world_objects::player_tick::set_allegiance_updates(w, player, ui_panel);
    }

    let allegiance =
        player.and_then(|p| player_allegiance::i_player_allegiance(w, IPlayer::Online(p)));
    let allegiance_node =
        player.and_then(|p| player_allegiance::i_player_allegiance_node(w, IPlayer::Online(p)));

    // Not ACE's (retail, V289; the retail captures): the view the answer shows is the
    // baseline of the player's later changes.
    let (allegiance_update, view) =
        crate::network::game_event::events::game_event_allegiance_update::game_event_allegiance_update_with_view(w, session, allegiance, allegiance_node);

    // Not ACE's (retail, V255): ACE followed every update with AllegianceUpdateDone;
    // only the first one of a login is paired (the one the client asks for at login), and the
    // rest ride the Age update.
    crate::network::game_messages::game_message::enqueue_send(w, session, allegiance_update);
    if let Some(player) = player {
        crate::world_objects::player_tick::note_allegiance_update_sent(w, player, view);
        crate::world_objects::player_tick::send_login_allegiance_update_done(w, player);
    }
    Ok(())
}
