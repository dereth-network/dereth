// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementTypes.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MovementTypes.cs`; do not edit by hand

/// These are used with various movement related messages. 0 & 6-9 are used with F74C Animation
///
/// ACE enum `MovementTypes`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MovementTypes(pub i32);

#[allow(non_upper_case_globals)]
impl MovementTypes {
    pub const General: Self = Self(0);
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

impl MovementTypes {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::General, Self::RawCommand, Self::InterpretedCommand, Self::StopRawCommand, Self::StopInterpretedCommand, Self::StopCompletely, Self::MoveToObject, Self::MoveToPosition, Self::TurnToObject, Self::TurnToHeading];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["General", "RawCommand", "InterpretedCommand", "StopRawCommand", "StopInterpretedCommand", "StopCompletely", "MoveToObject", "MoveToPosition", "TurnToObject", "TurnToHeading"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 2, 6, 7, 1, 5, 4, 3, 9, 8];
}

super::support::ace_enum!(MovementTypes, i32, plain);
super::support::ace_enum_from!(MovementTypes, i32 => i64);
