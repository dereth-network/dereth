// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PortalBitmask.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PortalBitmask.cs`; do not edit by hand

/// ACE enum `PortalBitmask` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PortalBitmask(pub i32);

#[allow(non_upper_case_globals)]
impl PortalBitmask {
    pub const Undef: Self = Self(0x0);
    pub const Unrestricted: Self = Self(0x1);
    pub const NoPk: Self = Self(0x2);
    pub const NoPKLite: Self = Self(0x4);
    pub const NoNPK: Self = Self(0x8);
    pub const NoSummon: Self = Self(0x10);
    pub const NoRecall: Self = Self(0x20);
    pub const OnlyOlthoiPCs: Self = Self(0x40);
    pub const NoOlthoiPCs: Self = Self(0x80);
    pub const NoVitae: Self = Self(0x100);
    pub const NoNewAccounts: Self = Self(0x200);
}

impl PortalBitmask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Unrestricted, Self::NoPk, Self::NoPKLite, Self::NoNPK, Self::NoSummon, Self::NoRecall, Self::OnlyOlthoiPCs, Self::NoOlthoiPCs, Self::NoVitae, Self::NoNewAccounts];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Unrestricted", "NoPk", "NoPKLite", "NoNPK", "NoSummon", "NoRecall", "OnlyOlthoiPCs", "NoOlthoiPCs", "NoVitae", "NoNewAccounts"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 10, 8, 3, 2, 6, 5, 9, 7, 0, 1];
}

super::support::ace_enum!(PortalBitmask, i32, flags);
super::support::ace_enum_from!(PortalBitmask, i32 => i64);
