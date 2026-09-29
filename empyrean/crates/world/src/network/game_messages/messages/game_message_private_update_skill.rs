// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateSkill.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateSkill.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use dereth_protocol::types::qualities::Skill;
use empyrean_common::dotnet::CsCast;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;
use crate::world_objects::entity::creature_skill::CreatureSkill;

// ACE: GameMessagePrivateUpdateSkill.GameMessagePrivateUpdateSkill
#[must_use]
pub fn game_message_private_update_skill(
    world_object: &mut impl HasSequences,
    creature_skill: CreatureSkill,
) -> GameMessage {
    let creature = world_object.world_object();
    let properties_skill = creature_skill.properties_skill(creature);
    let (resistance_at_last_check, last_used_time) = (
        properties_skill.resistance_at_last_check,
        properties_skill.last_used_time,
    );
    let (ranks, advancement_class, experience_spent, init_level) = (
        creature_skill.ranks(creature),
        creature_skill.advancement_class(creature),
        creature_skill.experience_spent(creature),
        creature_skill.init_level(creature),
    );
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdateSkill, creature_skill.skill),
    );
    let adjust_pp: u16 = 1; // If this is not 0, it appears to trigger the initLevel to be treated as extra XP applied to the skill
    let value = Skill {
        level_from_pp: ranks,
        format_version: adjust_pp,
        sac: advancement_class.0,
        pp: experience_spent,
        init_level, // starting point for advancement of the skill (eg. bonus points)
        resistance_of_last_check: resistance_at_last_check.cast_signed(),
        last_used_time,
    };
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdateSkill,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateSkill(PrivateUpdate {
            sequence,
            property_id: creature_skill.skill.0.cs_cast(),
            value,
        }),
    )
}
