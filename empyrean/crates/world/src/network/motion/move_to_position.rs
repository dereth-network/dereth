// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MoveToPosition.cs
//! Port of `Source/ACE.Server/Network/Motion/MoveToPosition.cs`.

use super::move_to_parameters::{self, MoveToParameters};
use super::movement_data::Motion;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::structure::origin::{self, Origin};

// ACE: MoveToPosition
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveToPosition {
    // ACE: MoveToPosition.Origin
    /// the location of the target
    pub origin: Origin,
    // ACE: MoveToPosition.MoveToParams
    /// set of movement parameters
    pub move_to_params: MoveToParameters,
    // ACE: MoveToPosition.RunRate
    /// run speed of the moving object
    pub run_rate: f32,
}

impl MoveToPosition {
    // ACE: MoveToPosition.MoveToPosition
    /// `new MoveToPosition(Motion motion)`: a null `motion.Position` throws in ACE.
    #[must_use]
    pub fn new(motion: &Motion) -> Self {
        Self {
            origin: Origin::from_position(
                motion
                    .position
                    .as_ref()
                    .expect("ACE: motion.Position is null"),
            ),
            move_to_params: motion.move_to_parameters,
            run_rate: motion.run_rate,
        }
    }
}

// ACE: MoveToPositionExtensions.Write
pub fn write(writer: &mut Vec<u8>, move_to: &MoveToPosition) {
    origin::write(writer, &move_to.origin);
    move_to_parameters::write(writer, &move_to.move_to_params);
    writer.write_f32(move_to.run_rate);
}
