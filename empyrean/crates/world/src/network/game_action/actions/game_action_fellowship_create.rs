// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipCreate.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipCreate.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_fellowship;
use crate::World;

// ACE: GameActionFellowshipCreate.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::FellowshipCreate>()?;
    let fellowship_name = m.name;
    let share_xp = CsCast::<u32>::cs_cast(m.share_xp) > 0;

    let player = session_player(w, session);
    player_fellowship::fellowship_create(w, player, &fellowship_name, share_xp);
    Ok(())
}
