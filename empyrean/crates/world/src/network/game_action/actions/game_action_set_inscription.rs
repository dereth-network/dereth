// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetInscription.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetInscription.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionSetInscription.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode_padded::<proto::trade::WritingSetInscription>()?;
    let (object_guid, inscription_text) = (m.object_id.0, m.text);

    let player = session_player(w, session);
    player_inventory::handle_action_set_inscription(w, player, object_guid, &inscription_text);
    Ok(())
}
