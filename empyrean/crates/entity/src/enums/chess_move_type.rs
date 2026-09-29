// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessMoveType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessMoveType.cs`; do not edit by hand

/// ACE enum `ChessMoveType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessMoveType(pub i32);

#[allow(non_upper_case_globals)]
impl ChessMoveType {
    pub const Invalid: Self = Self(0);
    pub const Pass: Self = Self(1);
    pub const Resign: Self = Self(2);
    pub const Stalemate: Self = Self(3);
    pub const Grid: Self = Self(4);
    pub const FromTo: Self = Self(5);
    pub const SelectedPiece: Self = Self(6);
}

impl ChessMoveType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Pass, Self::Resign, Self::Stalemate, Self::Grid, Self::FromTo, Self::SelectedPiece];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Pass", "Resign", "Stalemate", "Grid", "FromTo", "SelectedPiece"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 4, 0, 1, 2, 6, 3];
}

super::support::ace_enum!(ChessMoveType, i32, plain);
super::support::ace_enum_from!(ChessMoveType, i32 => i64);
