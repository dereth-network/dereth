// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseQueryLord.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseQueryLord.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{HandlerResult, Payload};
use crate::World;

// ACE: GameActionHouseQueryLord.Handle
/// ACE reads the slumlord id and does nothing with it.
pub fn handle(_w: &mut World, message: &mut Payload<'_>, _session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::HouseQueryLord>()?;
    let _lord = m.target.0; // slumlord ID to request info for
    Ok(())
}
