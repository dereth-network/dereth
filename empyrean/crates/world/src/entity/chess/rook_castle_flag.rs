// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/RookCastleFlag.cs
//! Port of `Source/ACE.Server/Entity/Chess/RookCastleFlag.cs`.

use empyrean_entity::enums::ChessMoveFlag;

// ACE: RookCastleFlag
/// A rook's starting corner and the castle it enables. ACE's `Vector2` is kept as whole numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RookCastleFlag {
    pub vector: (i32, i32),
    pub flag: ChessMoveFlag,
}

impl RookCastleFlag {
    // ACE: RookCastleFlag.RookCastleFlag
    #[must_use]
    pub const fn new(vector: (i32, i32), flag: ChessMoveFlag) -> Self {
        Self { vector, flag }
    }
}
