// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdatePropertyString.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdatePropertyString.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::qualities::{self as proto, AlignedString, PublicUpdate};
use empyrean_entity::enums::PropertyString;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{ace_str, byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePublicUpdatePropertyString.GameMessagePublicUpdatePropertyString
/// **Retail's order (V234):** the object guid, then the property, as the client
/// reads it and as every other public update is written. ACE wrote the property first, so the
/// client filed the string under object = the property number and dropped it (ACE's caller blamed
/// client caching: renames and description changes showed only after a relog).
#[must_use]
pub fn game_message_public_update_property_string(
    world_object: &mut impl HasSequences,
    property: PropertyString,
    value: Option<&str>,
) -> GameMessage {
    // 33 is the avg seen in retail pcaps, 104 is the max seen in retail pcaps
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyString, property),
    );
    let body = proto::QualitiesUpdateString(PublicUpdate {
        sequence,
        object: ObjectId(world_object.world_object().guid.full()),
        property_id: property.into(),
        value: AlignedString(ace_str(value)),
    });
    let mut msg = GameMessage::new(
        GameMessageOpcode::PublicUpdatePropertyString,
        GameMessageGroup::UIQueue,
    );
    msg.write_proto_strings(&body, &[value.unwrap_or("")]);
    msg
}
