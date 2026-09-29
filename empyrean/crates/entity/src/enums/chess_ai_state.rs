// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessAiState.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessAiState.cs`; do not edit by hand

/// ACE enum `ChessAiState`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessAiState(pub i32);

#[allow(non_upper_case_globals)]
impl ChessAiState {
    pub const None: Self = Self(0);
    pub const WaitingToStart: Self = Self(1);
    pub const WaitingForWorker: Self = Self(2);
    pub const InProgress: Self = Self(3);
    pub const WaitingForFinish: Self = Self(4);
}

impl ChessAiState {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::WaitingToStart, Self::WaitingForWorker, Self::InProgress, Self::WaitingForFinish];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "WaitingToStart", "WaitingForWorker", "InProgress", "WaitingForFinish"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 0, 4, 2, 1];
}

super::support::ace_enum!(ChessAiState, i32, plain);
super::support::ace_enum_from!(ChessAiState, i32 => i64);
