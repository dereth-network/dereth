// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyString.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdatePropertyString.cs`.

use dereth_protocol::qualities::{self as proto, AlignedString, PrivateUpdate};
use empyrean_entity::enums::PropertyString;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{ace_str, byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdatePropertyString.GameMessagePrivateUpdatePropertyString
#[must_use]
pub fn game_message_private_update_property_string(
    world_object: &mut impl HasSequences,
    property: PropertyString,
    value: Option<&str>,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyString, property),
    );
    // The property, `Writer.Align()`, then `WriteString16L`: dereth-protocol's aligned string.
    let body = proto::QualitiesPrivateUpdateString(PrivateUpdate {
        sequence,
        property_id: property.into(),
        value: AlignedString(ace_str(value)),
    });
    let mut msg = GameMessage::new(
        GameMessageOpcode::PrivateUpdatePropertyString,
        GameMessageGroup::UIQueue,
    );
    msg.write_proto_strings(&body, &[value.unwrap_or("")]);
    msg
}
