// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PKLevel.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PKLevel.cs`; do not edit by hand

/// ACE enum `PKLevel`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PKLevel(pub u32);

#[allow(non_upper_case_globals)]
impl PKLevel {
    pub const NPK: Self = Self(0);
    pub const PK: Self = Self(1);
    pub const PKLite: Self = Self(2);
    pub const Free: Self = Self(3);
}

impl PKLevel {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NPK, Self::PK, Self::PKLite, Self::Free];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NPK", "PK", "PKLite", "Free"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 0, 1, 2];
}

super::support::ace_enum!(PKLevel, u32, plain);
super::support::ace_enum_from!(PKLevel, u32 => u64, i64);
