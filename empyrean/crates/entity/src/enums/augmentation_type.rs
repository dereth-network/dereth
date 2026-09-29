// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AugmentationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AugmentationType.cs`; do not edit by hand

/// ACE enum `AugmentationType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AugmentationType(pub i32);

#[allow(non_upper_case_globals)]
impl AugmentationType {
    pub const None: Self = Self(0);
    pub const Strength: Self = Self(1);
    pub const Endurance: Self = Self(2);
    pub const Coordination: Self = Self(3);
    pub const Quickness: Self = Self(4);
    pub const Focus: Self = Self(5);
    /// `Self` in ACE; `Self` is a Rust keyword. DIVERGE: renamed.
    pub const Self_: Self = Self(6);
    pub const Salvage: Self = Self(7);
    pub const ItemTinkering: Self = Self(8);
    pub const ArmorTinkering: Self = Self(9);
    pub const MagicItemTinkering: Self = Self(10);
    pub const WeaponTinkering: Self = Self(11);
    pub const PackSlot: Self = Self(12);
    pub const BurdenLimit: Self = Self(13);
    pub const DeathItemLoss: Self = Self(14);
    pub const DeathSpellLoss: Self = Self(15);
    pub const CritProtect: Self = Self(16);
    pub const BonusXP: Self = Self(17);
    pub const BonusSalvage: Self = Self(18);
    pub const ImbueChance: Self = Self(19);
    pub const RegenBonus: Self = Self(20);
    pub const SpellDuration: Self = Self(21);
    pub const ResistSlash: Self = Self(22);
    pub const ResistPierce: Self = Self(23);
    pub const ResistBludgeon: Self = Self(24);
    pub const ResistAcid: Self = Self(25);
    pub const ResistFire: Self = Self(26);
    pub const ResistCold: Self = Self(27);
    pub const ResistElectric: Self = Self(28);
    pub const FociCreature: Self = Self(29);
    pub const FociItem: Self = Self(30);
    pub const FociLife: Self = Self(31);
    pub const FociWar: Self = Self(32);
    pub const CritChance: Self = Self(33);
    pub const CritDamage: Self = Self(34);
    pub const Melee: Self = Self(35);
    pub const Missile: Self = Self(36);
    pub const Magic: Self = Self(37);
    pub const Damage: Self = Self(38);
    pub const DamageResist: Self = Self(39);
    pub const AllStats: Self = Self(40);
    pub const FociVoid: Self = Self(42);
}

impl AugmentationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Strength, Self::Endurance, Self::Coordination, Self::Quickness, Self::Focus, Self::Self_, Self::Salvage, Self::ItemTinkering, Self::ArmorTinkering, Self::MagicItemTinkering, Self::WeaponTinkering, Self::PackSlot, Self::BurdenLimit, Self::DeathItemLoss, Self::DeathSpellLoss, Self::CritProtect, Self::BonusXP, Self::BonusSalvage, Self::ImbueChance, Self::RegenBonus, Self::SpellDuration, Self::ResistSlash, Self::ResistPierce, Self::ResistBludgeon, Self::ResistAcid, Self::ResistFire, Self::ResistCold, Self::ResistElectric, Self::FociCreature, Self::FociItem, Self::FociLife, Self::FociWar, Self::CritChance, Self::CritDamage, Self::Melee, Self::Missile, Self::Magic, Self::Damage, Self::DamageResist, Self::AllStats, Self::FociVoid];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Strength", "Endurance", "Coordination", "Quickness", "Focus", "Self", "Salvage", "ItemTinkering", "ArmorTinkering", "MagicItemTinkering", "WeaponTinkering", "PackSlot", "BurdenLimit", "DeathItemLoss", "DeathSpellLoss", "CritProtect", "BonusXP", "BonusSalvage", "ImbueChance", "RegenBonus", "SpellDuration", "ResistSlash", "ResistPierce", "ResistBludgeon", "ResistAcid", "ResistFire", "ResistCold", "ResistElectric", "FociCreature", "FociItem", "FociLife", "FociWar", "CritChance", "CritDamage", "Melee", "Missile", "Magic", "Damage", "DamageResist", "AllStats", "FociVoid"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[40, 9, 18, 17, 13, 3, 33, 34, 16, 38, 39, 14, 15, 2, 29, 30, 31, 41, 32, 5, 19, 8, 37, 10, 35, 36, 0, 12, 4, 20, 25, 24, 27, 28, 26, 23, 22, 7, 6, 21, 1, 11];
}

super::support::ace_enum!(AugmentationType, i32, plain);
super::support::ace_enum_from!(AugmentationType, i32 => i64);
