// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/TreasureClass.cs
// @generated from ACE's `Source/ACE.Entity/Enum/TreasureClass.cs`; do not edit by hand

/// ACE enum `TreasureClass`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreasureClass(pub i32);

#[allow(non_upper_case_globals)]
impl TreasureClass {
    pub const Undef: Self = Self(0);
    pub const Pyreal: Self = Self(1);
    pub const Gem: Self = Self(2);
    pub const Jewelry: Self = Self(3);
    pub const ArtObject: Self = Self(4);
    pub const Weapon: Self = Self(5);
    pub const Armor: Self = Self(6);
    pub const Clothing: Self = Self(7);
    pub const Scroll: Self = Self(8);
    pub const Caster: Self = Self(9);
    pub const ManaStone: Self = Self(10);
    pub const Consumable: Self = Self(11);
    pub const HealKit: Self = Self(12);
    pub const Lockpick: Self = Self(13);
    pub const SpellComponent: Self = Self(14);
    pub const SocietyArmor: Self = Self(15);
    pub const SocietyBreastplate: Self = Self(16);
    pub const SocietyGauntlets: Self = Self(17);
    pub const SocietyGirth: Self = Self(18);
    pub const SocietyGreaves: Self = Self(19);
    pub const SocietyHelm: Self = Self(20);
    pub const SocietyPauldrons: Self = Self(21);
    pub const SocietyTassets: Self = Self(22);
    pub const SocietyVambraces: Self = Self(23);
    pub const SocietySollerets: Self = Self(24);
    pub const Cloak: Self = Self(25);
    pub const PetDevice: Self = Self(26);
    pub const EncapsulatedSpirit: Self = Self(27);
}

impl TreasureClass {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Pyreal, Self::Gem, Self::Jewelry, Self::ArtObject, Self::Weapon, Self::Armor, Self::Clothing, Self::Scroll, Self::Caster, Self::ManaStone, Self::Consumable, Self::HealKit, Self::Lockpick, Self::SpellComponent, Self::SocietyArmor, Self::SocietyBreastplate, Self::SocietyGauntlets, Self::SocietyGirth, Self::SocietyGreaves, Self::SocietyHelm, Self::SocietyPauldrons, Self::SocietyTassets, Self::SocietyVambraces, Self::SocietySollerets, Self::Cloak, Self::PetDevice, Self::EncapsulatedSpirit];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Pyreal", "Gem", "Jewelry", "ArtObject", "Weapon", "Armor", "Clothing", "Scroll", "Caster", "ManaStone", "Consumable", "HealKit", "Lockpick", "SpellComponent", "SocietyArmor", "SocietyBreastplate", "SocietyGauntlets", "SocietyGirth", "SocietyGreaves", "SocietyHelm", "SocietyPauldrons", "SocietyTassets", "SocietyVambraces", "SocietySollerets", "Cloak", "PetDevice", "EncapsulatedSpirit"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 4, 9, 25, 7, 11, 27, 2, 12, 3, 13, 10, 26, 1, 8, 15, 16, 17, 18, 19, 20, 21, 24, 22, 23, 14, 0, 5];
}

super::support::ace_enum!(TreasureClass, i32, plain);
super::support::ace_enum_from!(TreasureClass, i32 => i64);
