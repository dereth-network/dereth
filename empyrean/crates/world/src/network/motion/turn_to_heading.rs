// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Motion/TurnToHeading.cs
//! Port of `Source/ACE.Server/Network/Motion/TurnToHeading.cs`.

use super::movement_data::Motion;
use super::turn_to_parameters::{self, TurnToParameters};

// ACE: TurnToHeading
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnToHeading {
    // ACE: TurnToHeading.TurnToParameters
    /// set of turning parameters
    pub turn_to_parameters: TurnToParameters,
}

impl TurnToHeading {
    // ACE: TurnToHeading.TurnToHeading
    #[must_use]
    pub fn new(motion: &Motion) -> Self {
        Self {
            turn_to_parameters: TurnToParameters::new(motion),
        }
    }
}

// ACE: TurnToHeadingExtensions.Write
pub fn write(writer: &mut Vec<u8>, turn_to: &TurnToHeading) {
    turn_to_parameters::write(writer, &turn_to.turn_to_parameters);
}
