//! Not ACE (V536): `0x02E2`, a skill's advancement class alone, which the retail servers answered
//! a skill trained with.

use dereth_protocol::qualities::{self as proto, PublicUpdate};
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{Skill, SkillAdvancementClass};
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

/// The object's `skill` now at `class`, in the skill's own sequence.
#[must_use]
pub fn game_message_public_update_skill_ac(
    world_object: &mut impl HasSequences,
    skill: Skill,
    class: SkillAdvancementClass,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdateSkill, skill),
    );
    let object = world_object.world_object().guid.into();
    GameMessage::from_proto(
        GameMessageOpcode::PublicUpdateSkillAC,
        GameMessageGroup::UIQueue,
        &proto::QualitiesUpdateSkillAc(PublicUpdate {
            sequence,
            object,
            property_id: skill.0.cs_cast(),
            value: class.0,
        }),
    )
}
