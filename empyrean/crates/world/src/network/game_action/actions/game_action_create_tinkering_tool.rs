// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionCreateTinkeringTool.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionCreateTinkeringTool.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_crafting;
use crate::World;

// ACE: GameActionCreateTinkeringTool.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryCreateTinkeringTool>()?;
    let tool_guid = m.tool.0;
    let items: Vec<u32> = m.items.iter().map(|i| i.0).collect();

    let player = session_player(w, session);
    player_crafting::handle_salvaging(w, player, tool_guid, items);
    Ok(())
}
