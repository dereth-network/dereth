// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessState.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessState.cs`; do not edit by hand

/// ACE enum `ChessState`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessState(pub i32);

#[allow(non_upper_case_globals)]
impl ChessState {
    pub const WaitingForPlayers: Self = Self(0);
    pub const InProgress: Self = Self(1);
    pub const Finished: Self = Self(2);
}

impl ChessState {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::WaitingForPlayers, Self::InProgress, Self::Finished];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["WaitingForPlayers", "InProgress", "Finished"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(ChessState, i32, plain);
super::support::ace_enum_from!(ChessState, i32 => i64);
