// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Skill.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Skill.cs`; do not edit by hand

/// note: even though these are unnumbered, order is very important. values of "none" or commented as retired or unused --ABSOLUTELY CANNOT-- be removed. Skills that are none, retired, or not implemented have been removed from the SkillHelper.ValidSkills hashset below.
///
/// ACE enum `Skill`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Skill(pub i32);

#[allow(non_upper_case_globals)]
impl Skill {
    pub const None: Self = Self(0);
    pub const Axe: Self = Self(1);
    pub const Bow: Self = Self(2);
    pub const Crossbow: Self = Self(3);
    pub const Dagger: Self = Self(4);
    pub const Mace: Self = Self(5);
    pub const MeleeDefense: Self = Self(6);
    pub const MissileDefense: Self = Self(7);
    pub const Sling: Self = Self(8);
    pub const Spear: Self = Self(9);
    pub const Staff: Self = Self(10);
    pub const Sword: Self = Self(11);
    pub const ThrownWeapon: Self = Self(12);
    pub const UnarmedCombat: Self = Self(13);
    pub const ArcaneLore: Self = Self(14);
    pub const MagicDefense: Self = Self(15);
    pub const ManaConversion: Self = Self(16);
    pub const Spellcraft: Self = Self(17);
    pub const ItemTinkering: Self = Self(18);
    pub const AssessPerson: Self = Self(19);
    pub const Deception: Self = Self(20);
    pub const Healing: Self = Self(21);
    pub const Jump: Self = Self(22);
    pub const Lockpick: Self = Self(23);
    pub const Run: Self = Self(24);
    pub const Awareness: Self = Self(25);
    pub const ArmsAndArmorRepair: Self = Self(26);
    pub const AssessCreature: Self = Self(27);
    pub const WeaponTinkering: Self = Self(28);
    pub const ArmorTinkering: Self = Self(29);
    pub const MagicItemTinkering: Self = Self(30);
    pub const CreatureEnchantment: Self = Self(31);
    pub const ItemEnchantment: Self = Self(32);
    pub const LifeMagic: Self = Self(33);
    pub const WarMagic: Self = Self(34);
    pub const Leadership: Self = Self(35);
    pub const Loyalty: Self = Self(36);
    pub const Fletching: Self = Self(37);
    pub const Alchemy: Self = Self(38);
    pub const Cooking: Self = Self(39);
    pub const Salvaging: Self = Self(40);
    pub const TwoHandedCombat: Self = Self(41);
    pub const Gearcraft: Self = Self(42);
    pub const VoidMagic: Self = Self(43);
    pub const HeavyWeapons: Self = Self(44);
    pub const LightWeapons: Self = Self(45);
    pub const FinesseWeapons: Self = Self(46);
    pub const MissileWeapons: Self = Self(47);
    pub const Shield: Self = Self(48);
    pub const DualWield: Self = Self(49);
    pub const Recklessness: Self = Self(50);
    pub const SneakAttack: Self = Self(51);
    pub const DirtyFighting: Self = Self(52);
    pub const Challenge: Self = Self(53);
    pub const Summoning: Self = Self(54);
}

impl Skill {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Axe, Self::Bow, Self::Crossbow, Self::Dagger, Self::Mace, Self::MeleeDefense, Self::MissileDefense, Self::Sling, Self::Spear, Self::Staff, Self::Sword, Self::ThrownWeapon, Self::UnarmedCombat, Self::ArcaneLore, Self::MagicDefense, Self::ManaConversion, Self::Spellcraft, Self::ItemTinkering, Self::AssessPerson, Self::Deception, Self::Healing, Self::Jump, Self::Lockpick, Self::Run, Self::Awareness, Self::ArmsAndArmorRepair, Self::AssessCreature, Self::WeaponTinkering, Self::ArmorTinkering, Self::MagicItemTinkering, Self::CreatureEnchantment, Self::ItemEnchantment, Self::LifeMagic, Self::WarMagic, Self::Leadership, Self::Loyalty, Self::Fletching, Self::Alchemy, Self::Cooking, Self::Salvaging, Self::TwoHandedCombat, Self::Gearcraft, Self::VoidMagic, Self::HeavyWeapons, Self::LightWeapons, Self::FinesseWeapons, Self::MissileWeapons, Self::Shield, Self::DualWield, Self::Recklessness, Self::SneakAttack, Self::DirtyFighting, Self::Challenge, Self::Summoning];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Axe", "Bow", "Crossbow", "Dagger", "Mace", "MeleeDefense", "MissileDefense", "Sling", "Spear", "Staff", "Sword", "ThrownWeapon", "UnarmedCombat", "ArcaneLore", "MagicDefense", "ManaConversion", "Spellcraft", "ItemTinkering", "AssessPerson", "Deception", "Healing", "Jump", "Lockpick", "Run", "Awareness", "ArmsAndArmorRepair", "AssessCreature", "WeaponTinkering", "ArmorTinkering", "MagicItemTinkering", "CreatureEnchantment", "ItemEnchantment", "LifeMagic", "WarMagic", "Leadership", "Loyalty", "Fletching", "Alchemy", "Cooking", "Salvaging", "TwoHandedCombat", "Gearcraft", "VoidMagic", "HeavyWeapons", "LightWeapons", "FinesseWeapons", "MissileWeapons", "Shield", "DualWield", "Recklessness", "SneakAttack", "DirtyFighting", "Challenge", "Summoning"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[38, 14, 29, 26, 27, 19, 25, 1, 2, 53, 39, 31, 3, 4, 20, 52, 49, 46, 37, 42, 21, 44, 32, 18, 22, 35, 33, 45, 23, 36, 5, 15, 30, 16, 6, 7, 47, 0, 50, 24, 40, 48, 8, 51, 9, 17, 10, 54, 11, 12, 41, 13, 43, 34, 28];
}

super::support::ace_enum!(Skill, i32, plain);
super::support::ace_enum_from!(Skill, i32 => i64);
