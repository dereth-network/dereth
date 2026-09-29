// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipAssignNewLeader.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipAssignNewLeader.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_fellowship;
use crate::World;

// ACE: GameActionFellowshipAssignNewLeader.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::FellowshipAssignNewLeader>()?;
    let new_leader_id = m.target.0;

    let player = session_player(w, session);
    player_fellowship::fellowship_new_leader(w, player, new_leader_id);
    Ok(())
}
