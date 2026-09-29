// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementCommand.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MovementCommand.cs`; do not edit by hand

/// A subset of MotionCommand & 0xFFFF
///
/// ACE enum `MovementCommand`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MovementCommand(pub u16);

#[allow(non_upper_case_globals)]
impl MovementCommand {
    pub const Invalid: Self = Self(0);
    pub const HoldRun: Self = Self(1);
    pub const HoldSideStep: Self = Self(2);
    pub const Ready: Self = Self(3);
    pub const WalkForward: Self = Self(5);
    pub const WalkBackwards: Self = Self(6);
    pub const RunForward: Self = Self(7);
    pub const TurnRight: Self = Self(13);
    pub const TurnLeft: Self = Self(14);
    pub const SideStepRight: Self = Self(15);
    pub const SideStepLeft: Self = Self(16);
}

impl MovementCommand {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::HoldRun, Self::HoldSideStep, Self::Ready, Self::WalkForward, Self::WalkBackwards, Self::RunForward, Self::TurnRight, Self::TurnLeft, Self::SideStepRight, Self::SideStepLeft];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "HoldRun", "HoldSideStep", "Ready", "WalkForward", "WalkBackwards", "RunForward", "TurnRight", "TurnLeft", "SideStepRight", "SideStepLeft"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0, 3, 6, 10, 9, 8, 7, 5, 4];
}

super::support::ace_enum!(MovementCommand, u16, plain);
super::support::ace_enum_from!(MovementCommand, u16 => u32, u64, i32, i64);
