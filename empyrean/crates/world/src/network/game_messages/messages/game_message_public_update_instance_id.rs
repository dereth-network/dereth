// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdateInstanceID.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdateInstanceID.cs`.

use dereth_protocol::qualities::{self as proto, PublicUpdate};
use empyrean_entity::enums::PropertyInstanceId;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePublicUpdateInstanceID.GameMessagePublicUpdateInstanceID
#[must_use]
pub fn game_message_public_update_instance_id(
    world_object: &mut impl HasSequences,
    property: PropertyInstanceId,
    value: ObjectGuid,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyInstanceID, property),
    );
    let object = world_object.world_object().guid.into();
    GameMessage::from_proto(
        GameMessageOpcode::PublicUpdateInstanceId,
        GameMessageGroup::UIQueue,
        &proto::QualitiesUpdateInstanceId(PublicUpdate {
            sequence,
            object,
            property_id: property.into(),
            value: value.into(),
        }),
    )
}
