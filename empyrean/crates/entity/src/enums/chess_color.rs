// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessColor.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessColor.cs`; do not edit by hand

/// ACE enum `ChessColor`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessColor(pub i32);

#[allow(non_upper_case_globals)]
impl ChessColor {
    pub const None: Self = Self(-1);
    pub const White: Self = Self(0);
    pub const Black: Self = Self(1);
}

impl ChessColor {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::White, Self::Black, Self::None];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["White", "Black", "None"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(ChessColor, i32, plain);
super::support::ace_enum_from!(ChessColor, i32 => i64);
