// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MotionDataFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MotionDataFlags.cs`; do not edit by hand

/// ACE enum `MotionDataFlags` (`[Flags]`), underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MotionDataFlags(pub u8);

#[allow(non_upper_case_globals)]
impl MotionDataFlags {
    pub const HasVelocity: Self = Self(0x1);
    pub const HasOmega: Self = Self(0x2);
}

impl MotionDataFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::HasVelocity, Self::HasOmega];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["HasVelocity", "HasOmega"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 0];
}

super::support::ace_enum!(MotionDataFlags, u8, flags);
super::support::ace_enum_from!(MotionDataFlags, u8 => u16, u32, u64, i32, i64);
