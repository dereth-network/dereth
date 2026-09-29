// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessMoveFlag.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessMoveFlag.cs`; do not edit by hand

/// ACE enum `ChessMoveFlag` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessMoveFlag(pub i32);

#[allow(non_upper_case_globals)]
impl ChessMoveFlag {
    pub const None: Self = Self(0x0);
    pub const Normal: Self = Self(0x1);
    pub const Capture: Self = Self(0x2);
    pub const BigPawn: Self = Self(0x4);
    pub const EnPassantCapture: Self = Self(0x8);
    pub const Promotion: Self = Self(0x10);
    pub const KingSideCastle: Self = Self(0x20);
    pub const QueenSideCastle: Self = Self(0x40);
}

impl ChessMoveFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Normal, Self::Capture, Self::BigPawn, Self::EnPassantCapture, Self::Promotion, Self::KingSideCastle, Self::QueenSideCastle];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Normal", "Capture", "BigPawn", "EnPassantCapture", "Promotion", "KingSideCastle", "QueenSideCastle"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 4, 6, 0, 1, 5, 7];
}

super::support::ace_enum!(ChessMoveFlag, i32, flags);
super::support::ace_enum_from!(ChessMoveFlag, i32 => i64);
