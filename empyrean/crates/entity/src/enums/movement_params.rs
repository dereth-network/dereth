// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementParams.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MovementParams.cs`; do not edit by hand

/// ACE enum `MovementParams` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MovementParams(pub u32);

#[allow(non_upper_case_globals)]
impl MovementParams {
    pub const CanWalk: Self = Self(0x1);
    pub const CanRun: Self = Self(0x2);
    pub const CanSideStep: Self = Self(0x4);
    pub const CanWalkBackwards: Self = Self(0x8);
    pub const CanCharge: Self = Self(0x10);
    pub const FailWalk: Self = Self(0x20);
    pub const UseFinalHeading: Self = Self(0x40);
    pub const Sticky: Self = Self(0x80);
    pub const MoveAway: Self = Self(0x100);
    pub const MoveTowards: Self = Self(0x200);
    pub const UseSpheres: Self = Self(0x400);
    pub const SetHoldKey: Self = Self(0x800);
    pub const Autonomous: Self = Self(0x1000);
    pub const ModifyRawState: Self = Self(0x2000);
    pub const ModifyInterpretedState: Self = Self(0x4000);
    pub const CancelMoveTo: Self = Self(0x8000);
    pub const StopCompletely: Self = Self(0x10000);
    pub const DisableJumpDuringLink: Self = Self(0x20000);
}

impl MovementParams {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::CanWalk, Self::CanRun, Self::CanSideStep, Self::CanWalkBackwards, Self::CanCharge, Self::FailWalk, Self::UseFinalHeading, Self::Sticky, Self::MoveAway, Self::MoveTowards, Self::UseSpheres, Self::SetHoldKey, Self::Autonomous, Self::ModifyRawState, Self::ModifyInterpretedState, Self::CancelMoveTo, Self::StopCompletely, Self::DisableJumpDuringLink];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["CanWalk", "CanRun", "CanSideStep", "CanWalkBackwards", "CanCharge", "FailWalk", "UseFinalHeading", "Sticky", "MoveAway", "MoveTowards", "UseSpheres", "SetHoldKey", "Autonomous", "ModifyRawState", "ModifyInterpretedState", "CancelMoveTo", "StopCompletely", "DisableJumpDuringLink"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[12, 4, 1, 2, 0, 3, 15, 17, 5, 14, 13, 8, 9, 11, 7, 16, 6, 10];
}

super::support::ace_enum!(MovementParams, u32, flags);
super::support::ace_enum_from!(MovementParams, u32 => u64, i64);
