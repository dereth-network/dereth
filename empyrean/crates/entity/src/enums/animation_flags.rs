// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AnimationFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AnimationFlags.cs`; do not edit by hand

/// ACE enum `AnimationFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AnimationFlags(pub i32);

#[allow(non_upper_case_globals)]
impl AnimationFlags {
    pub const PosFrames: Self = Self(0x1);
}

impl AnimationFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::PosFrames];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["PosFrames"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0];
}

super::support::ace_enum!(AnimationFlags, i32, flags);
super::support::ace_enum_from!(AnimationFlags, i32 => i64);
