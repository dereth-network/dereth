// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GamePieceState.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GamePieceState.cs`; do not edit by hand

/// ACE enum `GamePieceState`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GamePieceState(pub i32);

#[allow(non_upper_case_globals)]
impl GamePieceState {
    pub const None: Self = Self(0);
    pub const MoveToSquare: Self = Self(1);
    pub const WaitingForMoveToSquare: Self = Self(2);
    pub const WaitingForMoveToSquareAnimComplete: Self = Self(3);
    pub const MoveToAttack: Self = Self(4);
    pub const WaitingForMoveToAttack: Self = Self(5);
    pub const Combat: Self = Self(6);
}

impl GamePieceState {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::MoveToSquare, Self::WaitingForMoveToSquare, Self::WaitingForMoveToSquareAnimComplete, Self::MoveToAttack, Self::WaitingForMoveToAttack, Self::Combat];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "MoveToSquare", "WaitingForMoveToSquare", "WaitingForMoveToSquareAnimComplete", "MoveToAttack", "WaitingForMoveToAttack", "Combat"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 4, 1, 0, 5, 2, 3];
}

super::support::ace_enum!(GamePieceState, i32, plain);
super::support::ace_enum_from!(GamePieceState, i32 => i64);
