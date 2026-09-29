// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/DelayedActionType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/DelayedActionType.cs`; do not edit by hand

/// ACE enum `DelayedActionType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DelayedActionType(pub i32);

#[allow(non_upper_case_globals)]
impl DelayedActionType {
    pub const Start: Self = Self(0);
    pub const Move: Self = Self(1);
    pub const MovePass: Self = Self(2);
    pub const Stalemate: Self = Self(3);
    pub const Quit: Self = Self(4);
}

impl DelayedActionType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Start, Self::Move, Self::MovePass, Self::Stalemate, Self::Quit];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Start", "Move", "MovePass", "Stalemate", "Quit"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 4, 3, 0];
}

super::support::ace_enum!(DelayedActionType, i32, plain);
super::support::ace_enum_from!(DelayedActionType, i32 => i64);
