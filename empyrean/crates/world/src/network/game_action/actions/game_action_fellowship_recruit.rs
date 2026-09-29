// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipRecruit.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipRecruit.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::managers::player_manager;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_fellowship;
use crate::World;

// ACE: GameActionFellowshipRecruit.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::FellowshipRecruit>()?;
    let new_member_guid = m.target.0;
    let new_player = player_manager::get_online_player(w, new_member_guid);

    let player = session_player(w, session);
    player_fellowship::fellowship_recruit(w, player, new_player);
    Ok(())
}
