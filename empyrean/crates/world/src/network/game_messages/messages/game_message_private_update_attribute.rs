// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateAttribute.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateAttribute.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use dereth_protocol::types::qualities::Attribute;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;
use crate::world_objects::entity::creature_attribute::CreatureAttribute;

// ACE: GameMessagePrivateUpdateAttribute.GameMessagePrivateUpdateAttribute
/// `creature_attribute` is an attribute of `world_object` (ACE's `CreatureAttribute` holds its
/// creature; the handle reads the record from `world_object`'s biota).
#[must_use]
pub fn game_message_private_update_attribute(
    world_object: &mut impl HasSequences,
    creature_attribute: CreatureAttribute,
) -> GameMessage {
    let creature = world_object.world_object();
    let value = Attribute {
        level_from_cp: creature_attribute.ranks(creature),
        init_level: creature_attribute.starting_value(creature),
        cp_spent: creature_attribute.experience_spent(creature),
    };
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdateAttribute, creature_attribute.attribute),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdateAttribute,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateAttribute(PrivateUpdate {
            sequence,
            property_id: creature_attribute.attribute.into(),
            value,
        }),
    )
}
