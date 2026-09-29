// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HeritageGroup.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HeritageGroup.cs`; do not edit by hand

/// ACE enum `HeritageGroup`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HeritageGroup(pub i32);

#[allow(non_upper_case_globals)]
impl HeritageGroup {
    pub const Invalid: Self = Self(0);
    pub const Aluvian: Self = Self(1);
    pub const Gharundim: Self = Self(2);
    pub const Sho: Self = Self(3);
    pub const Viamontian: Self = Self(4);
    pub const Shadowbound: Self = Self(5);
    pub const Gearknight: Self = Self(6);
    pub const Tumerok: Self = Self(7);
    pub const Lugian: Self = Self(8);
    pub const Empyrean: Self = Self(9);
    pub const Penumbraen: Self = Self(10);
    pub const Undead: Self = Self(11);
    pub const Olthoi: Self = Self(12);
    pub const OlthoiAcid: Self = Self(13);
}

impl HeritageGroup {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Aluvian, Self::Gharundim, Self::Sho, Self::Viamontian, Self::Shadowbound, Self::Gearknight, Self::Tumerok, Self::Lugian, Self::Empyrean, Self::Penumbraen, Self::Undead, Self::Olthoi, Self::OlthoiAcid];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Aluvian", "Gharundim", "Sho", "Viamontian", "Shadowbound", "Gearknight", "Tumerok", "Lugian", "Empyrean", "Penumbraen", "Undead", "Olthoi", "OlthoiAcid"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 9, 6, 2, 0, 8, 12, 13, 10, 5, 3, 7, 11, 4];
}

super::support::ace_enum!(HeritageGroup, i32, plain);
super::support::ace_enum_from!(HeritageGroup, i32 => i64);
