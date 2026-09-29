// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/TurnToParameters.cs
//! Port of `Source/ACE.Server/Network/Motion/TurnToParameters.cs`.

use empyrean_entity::enums::MovementParams;

use super::movement_data::Motion;
use crate::network::game_messages::game_message::BinaryWriter;

// ACE: TurnToParameters
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnToParameters {
    // ACE: TurnToParameters.MovementParams
    pub movement_params: MovementParams,
    // ACE: TurnToParameters.Speed
    /// speed of the turn
    pub speed: f32,
    // ACE: TurnToParameters.DesiredHeading
    /// the angle to turn to
    pub desired_heading: f32,
}

impl TurnToParameters {
    // ACE: TurnToParameters.TurnToParameters
    #[must_use]
    pub fn new(motion: &Motion) -> Self {
        Self {
            movement_params: motion.move_to_parameters.movement_parameters,
            speed: motion.move_to_parameters.speed,
            desired_heading: motion.desired_heading,
        }
    }
}

// ACE: TurnToParametersExtensions.Write
pub fn write(writer: &mut Vec<u8>, turn_to: &TurnToParameters) {
    writer.write_u32(turn_to.movement_params.0);
    writer.write_f32(turn_to.speed);
    writer.write_f32(turn_to.desired_heading);
}
