// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateInstanceID.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateInstanceID.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use empyrean_entity::enums::PropertyInstanceId;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdateInstanceID.GameMessagePrivateUpdateInstanceID
#[must_use]
pub fn game_message_private_update_instance_id(
    world_object: &mut impl HasSequences,
    property: PropertyInstanceId,
    value: u32,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyInstanceID, property),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdatePropertyInstanceID,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateInstanceId(PrivateUpdate {
            sequence,
            property_id: property.into(),
            value: ObjectId(value),
        }),
    )
}
