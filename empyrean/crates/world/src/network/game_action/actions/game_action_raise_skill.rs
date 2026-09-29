// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseSkill.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseSkill.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_skills;
use crate::World;

// ACE: GameActionRaiseSkill.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::admin::TrainSkill>()?;
    let skill = empyrean_entity::enums::Skill(m.skill_id.cs_cast());
    let xp_spent = m.xp_spent;

    let player = session_player(w, session);
    player_skills::handle_action_raise_skill(w, player, skill, xp_spent);
    Ok(())
}
