// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementStateFlag.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MovementStateFlag.cs`; do not edit by hand

/// The current movement state for an object This is sent as part of the InterpretedMotionState network structure
///
/// ACE enum `MovementStateFlag` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MovementStateFlag(pub u32);

#[allow(non_upper_case_globals)]
impl MovementStateFlag {
    pub const Invalid: Self = Self(0x0);
    pub const CurrentStyle: Self = Self(0x1);
    pub const ForwardCommand: Self = Self(0x2);
    pub const ForwardSpeed: Self = Self(0x4);
    pub const SideStepCommand: Self = Self(0x8);
    pub const SideStepSpeed: Self = Self(0x10);
    pub const TurnCommand: Self = Self(0x20);
    pub const TurnSpeed: Self = Self(0x40);
}

impl MovementStateFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::CurrentStyle, Self::ForwardCommand, Self::ForwardSpeed, Self::SideStepCommand, Self::SideStepSpeed, Self::TurnCommand, Self::TurnSpeed];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "CurrentStyle", "ForwardCommand", "ForwardSpeed", "SideStepCommand", "SideStepSpeed", "TurnCommand", "TurnSpeed"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 0, 4, 5, 6, 7];
}

super::support::ace_enum!(MovementStateFlag, u32, flags);
super::support::ace_enum_from!(MovementStateFlag, u32 => u64, i64);
