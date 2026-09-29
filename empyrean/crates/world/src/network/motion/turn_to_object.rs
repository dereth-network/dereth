// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/TurnToObject.cs
//! Port of `Source/ACE.Server/Network/Motion/TurnToObject.cs`.

use empyrean_entity::ObjectGuid;

use super::movement_data::Motion;
use super::turn_to_parameters::{self, TurnToParameters};
use crate::network::game_messages::game_message::BinaryWriter;

// ACE: TurnToObject
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnToObject {
    // ACE: TurnToObject.Target
    pub target: ObjectGuid,
    // ACE: TurnToObject.DesiredHeading
    /// heading of the object to turn to. this is used instead of the DesiredHeading in the
    /// TurnToParameters
    pub desired_heading: f32,
    // ACE: TurnToObject.TurnToParameters
    /// set of turning parameters
    pub turn_to_parameters: TurnToParameters,
}

impl TurnToObject {
    // ACE: TurnToObject.TurnToObject
    #[must_use]
    pub fn new(motion: &Motion) -> Self {
        Self {
            target: motion.target_guid,
            desired_heading: motion.desired_heading,
            turn_to_parameters: TurnToParameters::new(motion),
        }
    }
}

// ACE: TurnToObjectExtensions.Write
pub fn write(writer: &mut Vec<u8>, turn_to: &TurnToObject) {
    writer.write_guid(turn_to.target);
    writer.write_f32(turn_to.desired_heading);
    turn_to_parameters::write(writer, &turn_to.turn_to_parameters);
}
