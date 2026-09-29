// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionBookData.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionBookData.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_book;
use crate::World;

// ACE: GameActionBookData.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::WritingBookData>()?;
    let book_guid = m.book_id.0;

    let player = session_player(w, session);
    player_book::read_book(w, player, book_guid);
    Ok(())
}
