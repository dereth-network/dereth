// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/CharacterPositionExtensions.cs
//! `CharacterPositionExtensions`: the starting position of a new character.

use crate::landblock_id::LandblockId;
use crate::position::Position;

/// ACE: CharacterPositionExtensions.StarterTown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum StarterTown {
    Holtburg = 0,
    Shoushi = 1,
    Yaraq = 2,
    Sanamar = 3,
}

/// The start position for character-creation area `start_area`; unknown areas get Holtburg's.
// ACE: CharacterPositionExtensions.StartingPosition
#[must_use]
pub fn starting_position(start_area: u32) -> Position {
    let landblock_id: u32 = match start_area {
        x if x == StarterTown::Shoushi as u32 => 2_130_903_469,
        x if x == StarterTown::Yaraq as u32 => 2_349_072_813,
        x if x == StarterTown::Sanamar as u32 => 1_912_799_661,
        // StarterTown.Holtburg and anything else
        _ => {
            debug_assert_eq!(StarterTown::Holtburg as u32, 0);
            2_248_343_981
        }
    };

    Position::from_components(
        landblock_id,
        12.3199,
        -28.482,
        0.004_999_999_5,
        0.0,
        0.0,
        -0.940_805_9,
        -0.338_945_9,
        false,
    )
}

/// A zero position (cell 0, identity rotation). `character_id` is unused, as in ACE.
// ACE: CharacterPositionExtensions.InvalidPosition
#[must_use]
pub fn invalid_position(_character_id: u32) -> Position {
    let mut invalid_position = Position::new();
    invalid_position.set_landblock_id(LandblockId::default());
    invalid_position
}
