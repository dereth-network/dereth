// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateVital.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateVital.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use dereth_protocol::types::qualities::{Attribute, SecondaryAttribute};
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;
use crate::world_objects::entity::creature_vital::CreatureVital;

// ACE: GameMessagePrivateUpdateVital.GameMessagePrivateUpdateVital
#[must_use]
pub fn game_message_private_update_vital(
    world_object: &mut impl HasSequences,
    creature_vital: CreatureVital,
) -> GameMessage {
    let creature = world_object.world_object();
    let value = SecondaryAttribute {
        attribute: Attribute {
            level_from_cp: creature_vital.ranks(creature),
            init_level: creature_vital.starting_value(creature),
            cp_spent: creature_vital.experience_spent(creature),
        },
        current_level: creature_vital.current(creature),
    };
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdateAttribute2ndLevel, creature_vital.vital),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdateVital,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateAttribute2nd(PrivateUpdate {
            sequence,
            property_id: creature_vital.vital.into(),
            value,
        }),
    )
}
