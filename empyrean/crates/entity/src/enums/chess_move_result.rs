// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChessMoveResult.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChessMoveResult.cs`; do not edit by hand

/// Identifies the chess move attempt result. Negative/0 values are failures.
///
/// ACE enum `ChessMoveResult`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChessMoveResult(pub i32);

#[allow(non_upper_case_globals)]
impl ChessMoveResult {
    pub const NoMoveResult: Self = Self(0);
    pub const OKMoveToEmptySquare: Self = Self(1);
    pub const OKMoveToOccupiedSquare: Self = Self(2);
    pub const OKMoveCheck: Self = Self(1024);
    pub const OKMoveCheckmate: Self = Self(2048);
    pub const OKMovePromotion: Self = Self(4096);
    pub const BadMoveInvalidCommand: Self = Self(-1);
    pub const BadMoveNotPlaying: Self = Self(-2);
    pub const BadMoveNotYourTurn: Self = Self(-3);
    pub const BadMoveDirection: Self = Self(-100);
    pub const BadMoveDistance: Self = Self(-101);
    pub const BadMoveNoPiece: Self = Self(-102);
    pub const BadMoveNotYours: Self = Self(-103);
    pub const BadMoveDestination: Self = Self(-104);
    pub const BadMoveWouldClobber: Self = Self(-105);
    pub const BadMoveSelfCheck: Self = Self(-106);
    pub const BadMoveWouldCollide: Self = Self(-107);
    pub const BadMoveCantCastleOutOfCheck: Self = Self(-108);
    pub const BadMoveCantCastleThroughCheck: Self = Self(-109);
    pub const BadMoveCantCastleAfterMoving: Self = Self(-110);
    pub const BadMoveInvalidBoardState: Self = Self(-111);
}

impl ChessMoveResult {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NoMoveResult, Self::OKMoveToEmptySquare, Self::OKMoveToOccupiedSquare, Self::OKMoveCheck, Self::OKMoveCheckmate, Self::OKMovePromotion, Self::BadMoveInvalidBoardState, Self::BadMoveCantCastleAfterMoving, Self::BadMoveCantCastleThroughCheck, Self::BadMoveCantCastleOutOfCheck, Self::BadMoveWouldCollide, Self::BadMoveSelfCheck, Self::BadMoveWouldClobber, Self::BadMoveDestination, Self::BadMoveNotYours, Self::BadMoveNoPiece, Self::BadMoveDistance, Self::BadMoveDirection, Self::BadMoveNotYourTurn, Self::BadMoveNotPlaying, Self::BadMoveInvalidCommand];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NoMoveResult", "OKMoveToEmptySquare", "OKMoveToOccupiedSquare", "OKMoveCheck", "OKMoveCheckmate", "OKMovePromotion", "BadMoveInvalidBoardState", "BadMoveCantCastleAfterMoving", "BadMoveCantCastleThroughCheck", "BadMoveCantCastleOutOfCheck", "BadMoveWouldCollide", "BadMoveSelfCheck", "BadMoveWouldClobber", "BadMoveDestination", "BadMoveNotYours", "BadMoveNoPiece", "BadMoveDistance", "BadMoveDirection", "BadMoveNotYourTurn", "BadMoveNotPlaying", "BadMoveInvalidCommand"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[7, 9, 8, 13, 17, 16, 6, 20, 15, 19, 18, 14, 11, 12, 10, 0, 3, 4, 5, 1, 2];
}

super::support::ace_enum!(ChessMoveResult, i32, plain);
super::support::ace_enum_from!(ChessMoveResult, i32 => i64);
