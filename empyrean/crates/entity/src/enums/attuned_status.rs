// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttunedStatus.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AttunedStatus.cs`; do not edit by hand

/// ACE enum `AttunedStatus`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AttunedStatus(pub i32);

#[allow(non_upper_case_globals)]
impl AttunedStatus {
    pub const Normal: Self = Self(0);
    pub const Attuned: Self = Self(1);
    pub const Sticky: Self = Self(2);
}

impl AttunedStatus {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Normal, Self::Attuned, Self::Sticky];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Normal", "Attuned", "Sticky"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 0, 2];
}

super::support::ace_enum!(AttunedStatus, i32, plain);
super::support::ace_enum_from!(AttunedStatus, i32 => i64);
