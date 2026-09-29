// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipChangeOpenness.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipChangeOpenness.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_fellowship;
use crate::World;

// ACE: GameActionFellowshipChangeOpenness.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::FellowshipChangeFellowOpenness>()?;
    let is_open = m.open != 0;

    let player = session_player(w, session);
    player_fellowship::handle_action_fellowship_change_openness(w, player, is_open);
    Ok(())
}
