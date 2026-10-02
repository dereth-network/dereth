//! `Magic_TestSpellFormula` (`0x004B`), the early clients' spell research test: ACE has no handler
//! (V432). See [`crate::world_objects::spell_research`].

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::spell_research;
use crate::World;

pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::combat::MagicTestSpellFormula>()?;
    let player = session_player(w, session);
    spell_research::handle_test_spell_formula(w, player, &m.components, m.target.0);
    Ok(())
}
