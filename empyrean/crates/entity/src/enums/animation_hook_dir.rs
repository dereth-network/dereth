// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AnimationHookDir.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AnimationHookDir.cs`; do not edit by hand

/// ACE enum `AnimationHookDir`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AnimationHookDir(pub i32);

#[allow(non_upper_case_globals)]
impl AnimationHookDir {
    pub const Unknown: Self = Self(-2);
    pub const Backward: Self = Self(-1);
    pub const Both: Self = Self(0);
    pub const Forward: Self = Self(1);
}

impl AnimationHookDir {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Both, Self::Forward, Self::Unknown, Self::Backward];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Both", "Forward", "Unknown", "Backward"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 0, 1, 2];
}

super::support::ace_enum!(AnimationHookDir, i32, plain);
super::support::ace_enum_from!(AnimationHookDir, i32 => i64);
