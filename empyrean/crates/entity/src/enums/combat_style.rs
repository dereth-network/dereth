// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CombatStyle.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CombatStyle.cs`; do not edit by hand

/// exported from the retail client.
///
/// ACE enum `CombatStyle` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CombatStyle(pub i32);

#[allow(non_upper_case_globals)]
impl CombatStyle {
    pub const Undef: Self = Self(0x0);
    pub const Unarmed: Self = Self(0x1);
    pub const OneHanded: Self = Self(0x2);
    pub const OneHandedAndShield: Self = Self(0x4);
    pub const TwoHanded: Self = Self(0x8);
    pub const Bow: Self = Self(0x10);
    pub const Crossbow: Self = Self(0x20);
    pub const Sling: Self = Self(0x40);
    pub const ThrownWeapon: Self = Self(0x80);
    pub const DualWield: Self = Self(0x100);
    pub const Magic: Self = Self(0x200);
    pub const Atlatl: Self = Self(0x400);
    pub const ThrownShield: Self = Self(0x800);
    pub const Reserved1: Self = Self(0x1000);
    pub const Reserved2: Self = Self(0x2000);
    pub const Reserved3: Self = Self(0x4000);
    pub const Reserved4: Self = Self(0x8000);
    pub const StubbornMagic: Self = Self(0x10000);
    pub const StubbornProjectile: Self = Self(0x20000);
    pub const StubbornMelee: Self = Self(0x40000);
    pub const StubbornMissile: Self = Self(0x80000);
    pub const Melee: Self = Self(0x10F);
    pub const Missile: Self = Self(0xCF0);
    pub const All: Self = Self(0xFFFF);
}

impl CombatStyle {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Unarmed, Self::OneHanded, Self::OneHandedAndShield, Self::TwoHanded, Self::Bow, Self::Crossbow, Self::Sling, Self::ThrownWeapon, Self::DualWield, Self::Melee, Self::Magic, Self::Atlatl, Self::ThrownShield, Self::Missile, Self::Reserved1, Self::Reserved2, Self::Reserved3, Self::Reserved4, Self::All, Self::StubbornMagic, Self::StubbornProjectile, Self::StubbornMelee, Self::StubbornMissile];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Unarmed", "OneHanded", "OneHandedAndShield", "TwoHanded", "Bow", "Crossbow", "Sling", "ThrownWeapon", "DualWield", "Melee", "Magic", "Atlatl", "ThrownShield", "Missile", "Reserved1", "Reserved2", "Reserved3", "Reserved4", "All", "StubbornMagic", "StubbornProjectile", "StubbornMelee", "StubbornMissile"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[19, 12, 5, 6, 9, 11, 10, 14, 2, 3, 15, 16, 17, 18, 7, 20, 22, 23, 21, 13, 8, 4, 1, 0];
}

super::support::ace_enum!(CombatStyle, i32, flags);
super::support::ace_enum_from!(CombatStyle, i32 => i64);
