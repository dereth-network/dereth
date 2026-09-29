// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipDismiss.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipDismiss.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_fellowship;
use crate::World;

// ACE: GameActionFellowshipDismiss.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::FellowshipDismiss>()?;
    let player_id_to_dismiss = m.target.0;

    let player = session_player(w, session);
    if player_fellowship::fellowship(w, player).is_some() {
        player_fellowship::fellowship_dismiss_player(w, player, player_id_to_dismiss);
    }
    Ok(())
}
