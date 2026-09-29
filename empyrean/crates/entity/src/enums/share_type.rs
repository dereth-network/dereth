// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ShareType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ShareType.cs`; do not edit by hand

/// ACE enum `ShareType` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ShareType(pub i32);

#[allow(non_upper_case_globals)]
impl ShareType {
    pub const None: Self = Self(0x0);
    pub const Fellowship: Self = Self(0x1);
    pub const Allegiance: Self = Self(0x2);
    pub const All: Self = Self(0x3);
}

impl ShareType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Fellowship, Self::Allegiance, Self::All];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Fellowship", "Allegiance", "All"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 1, 0];
}

super::support::ace_enum!(ShareType, i32, flags);
super::support::ace_enum_from!(ShareType, i32 => i64);
