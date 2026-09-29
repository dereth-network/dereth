// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MovementType.cs`; do not edit by hand

/// These are used with various movement related messages. 0 & 6-9 are used with F74C movement
///
/// ACE enum `MovementType`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MovementType(pub u8);

#[allow(non_upper_case_globals)]
impl MovementType {
    pub const Invalid: Self = Self(0);
    pub const RawCommand: Self = Self(1);
    pub const InterpretedCommand: Self = Self(2);
    pub const StopRawCommand: Self = Self(3);
    pub const StopInterpretedCommand: Self = Self(4);
    pub const StopCompletely: Self = Self(5);
    pub const MoveToObject: Self = Self(6);
    pub const MoveToPosition: Self = Self(7);
    pub const TurnToObject: Self = Self(8);
    pub const TurnToHeading: Self = Self(9);
}

impl MovementType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::RawCommand, Self::InterpretedCommand, Self::StopRawCommand, Self::StopInterpretedCommand, Self::StopCompletely, Self::MoveToObject, Self::MoveToPosition, Self::TurnToObject, Self::TurnToHeading];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "RawCommand", "InterpretedCommand", "StopRawCommand", "StopInterpretedCommand", "StopCompletely", "MoveToObject", "MoveToPosition", "TurnToObject", "TurnToHeading"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 0, 6, 7, 1, 5, 4, 3, 9, 8];
}

super::support::ace_enum!(MovementType, u8, plain);
super::support::ace_enum_from!(MovementType, u8 => u16, u32, u64, i32, i64);
