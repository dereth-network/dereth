// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RegenerationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RegenerationType.cs`; do not edit by hand

/// ACE enum `RegenerationType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RegenerationType(pub u32);

#[allow(non_upper_case_globals)]
impl RegenerationType {
    pub const Undef: Self = Self(0x0);
    pub const Destruction: Self = Self(0x1);
    pub const PickUp: Self = Self(0x2);
    pub const Death: Self = Self(0x4);
}

impl RegenerationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Destruction, Self::PickUp, Self::Death];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Destruction", "PickUp", "Death"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 1, 2, 0];
}

super::support::ace_enum!(RegenerationType, u32, flags);
super::support::ace_enum_from!(RegenerationType, u32 => u64, i64);
