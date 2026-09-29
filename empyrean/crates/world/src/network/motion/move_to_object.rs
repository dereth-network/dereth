// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/MoveToObject.cs
//! Port of `Source/ACE.Server/Network/Motion/MoveToObject.cs`.

use empyrean_entity::ObjectGuid;

use super::move_to_parameters::{self, MoveToParameters};
use super::movement_data::Motion;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::structure::origin::{self, Origin};

// ACE: MoveToObject
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveToObject {
    // ACE: MoveToObject.Target
    /// target guid to move to
    pub target: ObjectGuid,
    // ACE: MoveToObject.Origin
    /// the location of the target
    pub origin: Origin,
    // ACE: MoveToObject.MoveToParams
    /// set of movement parameters
    pub move_to_params: MoveToParameters,
    // ACE: MoveToObject.RunRate
    /// run speed of the moving object
    pub run_rate: f32,
}

impl MoveToObject {
    // ACE: MoveToObject.MoveToObject
    /// `new MoveToObject(Motion motion)`: a null `motion.Position` throws in ACE.
    #[must_use]
    pub fn new(motion: &Motion) -> Self {
        Self {
            target: motion.target_guid,
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

// ACE: MoveToObjectExtensions.Write
pub fn write(writer: &mut Vec<u8>, move_to: &MoveToObject) {
    writer.write_guid(move_to.target);
    origin::write(writer, &move_to.origin);
    move_to_parameters::write(writer, &move_to.move_to_params);
    writer.write_f32(move_to.run_rate);
}
