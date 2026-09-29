// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MotionStance.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MotionStance.cs`; do not edit by hand

/// The list of stances for players and creatures This is a subset of MotionCommand
///
/// ACE enum `MotionStance`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MotionStance(pub u32);

#[allow(non_upper_case_globals)]
impl MotionStance {
    pub const Invalid: Self = Self(0x80000000);
    pub const HandCombat: Self = Self(0x8000003C);
    pub const NonCombat: Self = Self(0x8000003D);
    pub const SwordCombat: Self = Self(0x8000003E);
    pub const BowCombat: Self = Self(0x8000003F);
    pub const SwordShieldCombat: Self = Self(0x80000040);
    pub const CrossbowCombat: Self = Self(0x80000041);
    pub const UnusedCombat: Self = Self(0x80000042);
    pub const SlingCombat: Self = Self(0x80000043);
    pub const TwoHandedSwordCombat: Self = Self(0x80000044);
    pub const TwoHandedStaffCombat: Self = Self(0x80000045);
    pub const DualWieldCombat: Self = Self(0x80000046);
    pub const ThrownWeaponCombat: Self = Self(0x80000047);
    pub const Graze: Self = Self(0x80000048);
    pub const Magic: Self = Self(0x80000049);
    pub const BowNoAmmo: Self = Self(0x800000E8);
    pub const CrossBowNoAmmo: Self = Self(0x800000E9);
    pub const AtlatlCombat: Self = Self(0x8000013B);
    pub const ThrownShieldCombat: Self = Self(0x8000013C);
}

impl MotionStance {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::HandCombat, Self::NonCombat, Self::SwordCombat, Self::BowCombat, Self::SwordShieldCombat, Self::CrossbowCombat, Self::UnusedCombat, Self::SlingCombat, Self::TwoHandedSwordCombat, Self::TwoHandedStaffCombat, Self::DualWieldCombat, Self::ThrownWeaponCombat, Self::Graze, Self::Magic, Self::BowNoAmmo, Self::CrossBowNoAmmo, Self::AtlatlCombat, Self::ThrownShieldCombat];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "HandCombat", "NonCombat", "SwordCombat", "BowCombat", "SwordShieldCombat", "CrossbowCombat", "UnusedCombat", "SlingCombat", "TwoHandedSwordCombat", "TwoHandedStaffCombat", "DualWieldCombat", "ThrownWeaponCombat", "Graze", "Magic", "BowNoAmmo", "CrossBowNoAmmo", "AtlatlCombat", "ThrownShieldCombat"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[17, 4, 15, 16, 6, 11, 13, 1, 0, 14, 2, 8, 3, 5, 18, 12, 10, 9, 7];
}

super::support::ace_enum!(MotionStance, u32, plain);
super::support::ace_enum_from!(MotionStance, u32 => u64, i64);
