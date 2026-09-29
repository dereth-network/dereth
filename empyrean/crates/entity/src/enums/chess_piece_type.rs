// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessPieceType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessPieceType.cs`; do not edit by hand

/// ACE enum `ChessPieceType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessPieceType(pub i32);

#[allow(non_upper_case_globals)]
impl ChessPieceType {
    pub const Empty: Self = Self(0);
    pub const Pawn: Self = Self(1);
    pub const Rook: Self = Self(2);
    pub const Knight: Self = Self(3);
    pub const Bishop: Self = Self(4);
    pub const Queen: Self = Self(5);
    pub const King: Self = Self(6);
    pub const Count: Self = Self(7);
}

impl ChessPieceType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Empty, Self::Pawn, Self::Rook, Self::Knight, Self::Bishop, Self::Queen, Self::King, Self::Count];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Empty", "Pawn", "Rook", "Knight", "Bishop", "Queen", "King", "Count"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 7, 0, 6, 3, 1, 5, 2];
}

super::support::ace_enum!(ChessPieceType, i32, plain);
super::support::ace_enum_from!(ChessPieceType, i32 => i64);
