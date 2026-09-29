// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTrainSkill.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTrainSkill.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_skills;
use crate::World;

// ACE: GameActionTrainSkill.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::admin::TrainSkillAdvancementClass>()?;
    let skill = empyrean_entity::enums::Skill(m.skill_id.cs_cast());
    let credits_spent: i32 = m.credits_spent.cs_cast();

    let player = session_player(w, session);
    player_skills::handle_action_train_skill(w, player, skill, credits_spent);
    Ok(())
}
