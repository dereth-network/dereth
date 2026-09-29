// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionLoginComplete.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionLoginComplete.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::{player_location, player_networking};
use crate::World;

// ACE: GameActionLoginComplete.Handle
/// The client left portal space (at login, or after a teleport): `OnTeleportComplete`; the first
/// time after entering the world, the property updates and overrides follow.
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_location::on_teleport_complete(w, player);

    if !w
        .objects
        .get(player)
        .expect("session.Player is null")
        .first_enter_world_done()
    {
        w.objects
            .get_mut(player)
            .expect("session.Player is null")
            .set_first_enter_world_done(true);
        player_networking::send_property_updates_and_overrides(w, player);
    }
    Ok(())
}
