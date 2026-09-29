// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyFloat.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyFloat.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use empyrean_entity::enums::PropertyFloat;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdatePropertyFloat.GameMessagePrivateUpdatePropertyFloat
#[must_use]
pub fn game_message_private_update_property_float(
    world_object: &mut impl HasSequences,
    property: PropertyFloat,
    value: f64,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyDouble, property),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdatePropertyFloat,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateFloat(PrivateUpdate {
            sequence,
            property_id: property.into(),
            value,
        }),
    )
}
