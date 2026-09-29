// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/WeenieType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/WeenieType.cs`; do not edit by hand

/// ACE enum `WeenieType`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WeenieType(pub u32);

#[allow(non_upper_case_globals)]
impl WeenieType {
    pub const Undef: Self = Self(0);
    pub const Generic: Self = Self(1);
    pub const Clothing: Self = Self(2);
    pub const MissileLauncher: Self = Self(3);
    pub const Missile: Self = Self(4);
    pub const Ammunition: Self = Self(5);
    pub const MeleeWeapon: Self = Self(6);
    pub const Portal: Self = Self(7);
    pub const Book: Self = Self(8);
    pub const Coin: Self = Self(9);
    pub const Creature: Self = Self(10);
    pub const Admin: Self = Self(11);
    pub const Vendor: Self = Self(12);
    pub const HotSpot: Self = Self(13);
    pub const Corpse: Self = Self(14);
    pub const Cow: Self = Self(15);
    pub const AI: Self = Self(16);
    pub const Machine: Self = Self(17);
    pub const Food: Self = Self(18);
    pub const Door: Self = Self(19);
    pub const Chest: Self = Self(20);
    pub const Container: Self = Self(21);
    pub const Key: Self = Self(22);
    pub const Lockpick: Self = Self(23);
    pub const PressurePlate: Self = Self(24);
    pub const LifeStone: Self = Self(25);
    pub const Switch: Self = Self(26);
    pub const PKModifier: Self = Self(27);
    pub const Healer: Self = Self(28);
    pub const LightSource: Self = Self(29);
    pub const Allegiance: Self = Self(30);
    pub const UNKNOWN__GUESSEDNAME32: Self = Self(31);
    pub const SpellComponent: Self = Self(32);
    pub const ProjectileSpell: Self = Self(33);
    pub const Scroll: Self = Self(34);
    pub const Caster: Self = Self(35);
    pub const Channel: Self = Self(36);
    pub const ManaStone: Self = Self(37);
    pub const Gem: Self = Self(38);
    pub const AdvocateFane: Self = Self(39);
    pub const AdvocateItem: Self = Self(40);
    pub const Sentinel: Self = Self(41);
    pub const GSpellEconomy: Self = Self(42);
    pub const LSpellEconomy: Self = Self(43);
    pub const CraftTool: Self = Self(44);
    pub const LScoreKeeper: Self = Self(45);
    pub const GScoreKeeper: Self = Self(46);
    pub const GScoreGatherer: Self = Self(47);
    pub const ScoreBook: Self = Self(48);
    pub const EventCoordinator: Self = Self(49);
    pub const Entity: Self = Self(50);
    pub const Stackable: Self = Self(51);
    pub const HUD: Self = Self(52);
    pub const House: Self = Self(53);
    pub const Deed: Self = Self(54);
    pub const SlumLord: Self = Self(55);
    pub const Hook: Self = Self(56);
    pub const Storage: Self = Self(57);
    pub const BootSpot: Self = Self(58);
    pub const HousePortal: Self = Self(59);
    pub const Game: Self = Self(60);
    pub const GamePiece: Self = Self(61);
    pub const SkillAlterationDevice: Self = Self(62);
    pub const AttributeTransferDevice: Self = Self(63);
    pub const Hooker: Self = Self(64);
    pub const AllegianceBindstone: Self = Self(65);
    pub const InGameStatKeeper: Self = Self(66);
    pub const AugmentationDevice: Self = Self(67);
    pub const SocialManager: Self = Self(68);
    pub const Pet: Self = Self(69);
    pub const PetDevice: Self = Self(70);
    pub const CombatPet: Self = Self(71);
}

impl WeenieType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Generic, Self::Clothing, Self::MissileLauncher, Self::Missile, Self::Ammunition, Self::MeleeWeapon, Self::Portal, Self::Book, Self::Coin, Self::Creature, Self::Admin, Self::Vendor, Self::HotSpot, Self::Corpse, Self::Cow, Self::AI, Self::Machine, Self::Food, Self::Door, Self::Chest, Self::Container, Self::Key, Self::Lockpick, Self::PressurePlate, Self::LifeStone, Self::Switch, Self::PKModifier, Self::Healer, Self::LightSource, Self::Allegiance, Self::UNKNOWN__GUESSEDNAME32, Self::SpellComponent, Self::ProjectileSpell, Self::Scroll, Self::Caster, Self::Channel, Self::ManaStone, Self::Gem, Self::AdvocateFane, Self::AdvocateItem, Self::Sentinel, Self::GSpellEconomy, Self::LSpellEconomy, Self::CraftTool, Self::LScoreKeeper, Self::GScoreKeeper, Self::GScoreGatherer, Self::ScoreBook, Self::EventCoordinator, Self::Entity, Self::Stackable, Self::HUD, Self::House, Self::Deed, Self::SlumLord, Self::Hook, Self::Storage, Self::BootSpot, Self::HousePortal, Self::Game, Self::GamePiece, Self::SkillAlterationDevice, Self::AttributeTransferDevice, Self::Hooker, Self::AllegianceBindstone, Self::InGameStatKeeper, Self::AugmentationDevice, Self::SocialManager, Self::Pet, Self::PetDevice, Self::CombatPet];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Generic", "Clothing", "MissileLauncher", "Missile", "Ammunition", "MeleeWeapon", "Portal", "Book", "Coin", "Creature", "Admin", "Vendor", "HotSpot", "Corpse", "Cow", "AI", "Machine", "Food", "Door", "Chest", "Container", "Key", "Lockpick", "PressurePlate", "LifeStone", "Switch", "PKModifier", "Healer", "LightSource", "Allegiance", "UNKNOWN__GUESSEDNAME32", "SpellComponent", "ProjectileSpell", "Scroll", "Caster", "Channel", "ManaStone", "Gem", "AdvocateFane", "AdvocateItem", "Sentinel", "GSpellEconomy", "LSpellEconomy", "CraftTool", "LScoreKeeper", "GScoreKeeper", "GScoreGatherer", "ScoreBook", "EventCoordinator", "Entity", "Stackable", "HUD", "House", "Deed", "SlumLord", "Hook", "Storage", "BootSpot", "HousePortal", "Game", "GamePiece", "SkillAlterationDevice", "AttributeTransferDevice", "Hooker", "AllegianceBindstone", "InGameStatKeeper", "AugmentationDevice", "SocialManager", "Pet", "PetDevice", "CombatPet"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[16, 11, 39, 40, 30, 65, 5, 63, 67, 8, 58, 35, 36, 20, 2, 9, 71, 21, 14, 15, 44, 10, 54, 19, 50, 49, 18, 47, 46, 42, 60, 61, 38, 1, 52, 28, 56, 64, 13, 53, 59, 66, 22, 45, 43, 25, 29, 23, 17, 37, 6, 4, 3, 27, 69, 70, 7, 24, 33, 48, 34, 41, 62, 55, 68, 32, 51, 57, 26, 31, 0, 12];
}

super::support::ace_enum!(WeenieType, u32, plain);
super::support::ace_enum_from!(WeenieType, u32 => u64, i64);
