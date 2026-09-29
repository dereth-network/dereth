// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/StanceMode.cs
// @generated from ACE's `Source/ACE.Entity/Enum/StanceMode.cs`; do not edit by hand

/// This should be the same as MotionStance & 0xFFFF
///
/// ACE enum `StanceMode`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct StanceMode(pub u16);

#[allow(non_upper_case_globals)]
impl StanceMode {
    pub const Invalid: Self = Self(0);
    pub const HandCombat: Self = Self(60);
    pub const NonCombat: Self = Self(61);
    pub const SwordCombat: Self = Self(62);
    pub const BowCombat: Self = Self(63);
    pub const SwordShieldCombat: Self = Self(64);
    pub const CrossbowCombat: Self = Self(65);
    pub const UnusedCombat: Self = Self(66);
    pub const SlingCombat: Self = Self(67);
    pub const TwoHandedSwordCombat: Self = Self(68);
    pub const TwoHandedStaffCombat: Self = Self(69);
    pub const DualWieldCombat: Self = Self(70);
    pub const ThrownWeaponCombat: Self = Self(71);
    pub const Graze: Self = Self(72);
    pub const Magic: Self = Self(73);
    pub const BowNoAmmo: Self = Self(232);
    pub const CrossBowNoAmmo: Self = Self(233);
    pub const AtlatlCombat: Self = Self(315);
    pub const ThrownShieldCombat: Self = Self(316);
}

impl StanceMode {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::HandCombat, Self::NonCombat, Self::SwordCombat, Self::BowCombat, Self::SwordShieldCombat, Self::CrossbowCombat, Self::UnusedCombat, Self::SlingCombat, Self::TwoHandedSwordCombat, Self::TwoHandedStaffCombat, Self::DualWieldCombat, Self::ThrownWeaponCombat, Self::Graze, Self::Magic, Self::BowNoAmmo, Self::CrossBowNoAmmo, Self::AtlatlCombat, Self::ThrownShieldCombat];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "HandCombat", "NonCombat", "SwordCombat", "BowCombat", "SwordShieldCombat", "CrossbowCombat", "UnusedCombat", "SlingCombat", "TwoHandedSwordCombat", "TwoHandedStaffCombat", "DualWieldCombat", "ThrownWeaponCombat", "Graze", "Magic", "BowNoAmmo", "CrossBowNoAmmo", "AtlatlCombat", "ThrownShieldCombat"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[17, 4, 15, 16, 6, 11, 13, 1, 0, 14, 2, 8, 3, 5, 18, 12, 10, 9, 7];
}

super::support::ace_enum!(StanceMode, u16, plain);
super::support::ace_enum_from!(StanceMode, u16 => u32, u64, i32, i64);
