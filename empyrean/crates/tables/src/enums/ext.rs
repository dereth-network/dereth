// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/*.cs

//! Hand ports of the extension classes next to the `Factories/Enum` enums. ACE's extension
//! methods on these enums are inherent methods; the one on `ACE.Entity.Enum.Skill` (another
//! crate's type) is a free function.

use empyrean_entity::enums::Skill;

use super::{
    MeleeWeaponSkill, SocietyArmorType, SocietyType, TreasureArmorType, TreasureHeritageGroup,
    TreasureItemType, TreasureWeaponType,
};

/// `skill.ToMeleeWeaponSkill()`.
// ACE: MeleeWeaponSkillExtensions.ToMeleeWeaponSkill
pub fn to_melee_weapon_skill(skill: Skill) -> MeleeWeaponSkill {
    match skill {
        Skill::HeavyWeapons => MeleeWeaponSkill::HeavyWeapons,
        Skill::LightWeapons => MeleeWeaponSkill::LightWeapons,
        Skill::FinesseWeapons => MeleeWeaponSkill::FinesseWeapons,
        Skill::TwoHandedCombat => MeleeWeaponSkill::TwoHandedCombat,
        _ => MeleeWeaponSkill::Undef,
    }
}

impl MeleeWeaponSkill {
    /// The skill's script name; `None` (ACE's `null`) for `Undef`.
    // ACE: MeleeWeaponSkillExtensions.GetScriptName
    pub fn get_script_name(self) -> Option<&'static str> {
        match self {
            Self::HeavyWeapons => Some("heavy"),
            Self::LightWeapons => Some("light"),
            Self::FinesseWeapons => Some("finesse"),
            Self::TwoHandedCombat => Some("two_handed"),
            _ => None,
        }
    }

    /// As [`Self::get_script_name`], with light and finesse sharing `light_finesse`.
    // ACE: MeleeWeaponSkillExtensions.GetScriptName_Combined
    pub fn get_script_name_combined(self) -> Option<&'static str> {
        match self {
            Self::HeavyWeapons => Some("heavy"),
            Self::LightWeapons | Self::FinesseWeapons => Some("light_finesse"),
            Self::TwoHandedCombat => Some("two_handed"),
            _ => None,
        }
    }
}

impl TreasureItemType {
    /// The society armor piece of a `Society*` item type; `Undef` otherwise.
    // ACE: SocietyArmorTypeExtensions.GetSocietyArmorType
    pub fn get_society_armor_type(self) -> SocietyArmorType {
        match self {
            Self::SocietyBreastplate => SocietyArmorType::Breastplate,
            Self::SocietyGauntlets => SocietyArmorType::Gauntlets,
            Self::SocietyGirth => SocietyArmorType::Girth,
            Self::SocietyGreaves => SocietyArmorType::Greaves,
            Self::SocietyHelm => SocietyArmorType::Helm,
            Self::SocietyPauldrons => SocietyArmorType::Pauldrons,
            Self::SocietyTassets => SocietyArmorType::Tassets,
            Self::SocietyVambraces => SocietyArmorType::Vambraces,
            Self::SocietySollerets => SocietyArmorType::Sollerets,
            _ => SocietyArmorType::Undef,
        }
    }
}

impl TreasureHeritageGroup {
    /// The society of a society heritage group; `Undef` otherwise.
    // ACE: SocietyTypeExtensions.ToSociety
    pub fn to_society(self) -> SocietyType {
        match self {
            Self::CelestialHand => SocietyType::CelestialHand,
            Self::EldrytchWeb => SocietyType::EldrytchWeb,
            Self::RadiantBlood => SocietyType::RadiantBlood,
            _ => SocietyType::Undef,
        }
    }
}

impl TreasureArmorType {
    // ACE: TreasureArmorTypeHelper.IsSocietyArmor
    pub fn is_society_armor(self) -> bool {
        matches!(
            self,
            Self::Society | Self::CelestialHand | Self::EldrytchWeb | Self::RadiantBlood
        )
    }
}

impl TreasureWeaponType {
    // ACE: TreasureWeaponTypeExtensions.IsMeleeWeapon
    pub fn is_melee_weapon(self) -> bool {
        matches!(
            self,
            Self::MeleeWeapon
                | Self::Axe
                | Self::Dagger
                | Self::DaggerMS
                | Self::Mace
                | Self::MaceJitte
                | Self::Spear
                | Self::Staff
                | Self::Sword
                | Self::SwordMS
                | Self::Unarmed
                | Self::TwoHandedWeapon
                | Self::TwoHandedAxe
                | Self::TwoHandedMace
                | Self::TwoHandedSpear
                | Self::TwoHandedSword
        )
    }

    // ACE: TreasureWeaponTypeExtensions.IsMissileWeapon
    pub fn is_missile_weapon(self) -> bool {
        matches!(
            self,
            Self::MissileWeapon | Self::Bow | Self::Crossbow | Self::Atlatl
        )
    }

    // ACE: TreasureWeaponTypeExtensions.IsCaster
    pub fn is_caster(self) -> bool {
        self == Self::Caster
    }

    /// The weapon type's script name; `None` (ACE's `null`) where ACE has none.
    // ACE: TreasureWeaponTypeExtensions.GetScriptName
    pub fn get_script_name(self) -> Option<&'static str> {
        match self {
            Self::Axe => Some("axe"),
            Self::Dagger => Some("dagger"),
            Self::DaggerMS => Some("dagger_ms"),
            Self::Mace => Some("mace"),
            Self::MaceJitte => Some("mace_jitte"),
            Self::Spear | Self::TwoHandedSpear => Some("spear"),
            Self::Staff => Some("staff"),
            Self::Sword => Some("sword"),
            Self::SwordMS => Some("sword_ms"),
            Self::Unarmed => Some("unarmed"),
            Self::TwoHandedAxe | Self::TwoHandedMace | Self::TwoHandedSword => Some("cleaver"),

            Self::Bow => Some("bow"),
            Self::Crossbow => Some("crossbow"),
            Self::Atlatl => Some("atlatl"),
            Self::Caster => Some("caster"),
            _ => None,
        }
    }

    /// The weapon type's short script name; `None` (ACE's `null`) where ACE has none.
    // ACE: TreasureWeaponTypeExtensions.GetScriptShortName
    pub fn get_script_short_name(self) -> Option<&'static str> {
        match self {
            Self::Axe => Some("axe"),
            Self::Dagger | Self::DaggerMS => Some("dagger"),
            Self::Mace => Some("mace"),
            Self::MaceJitte => Some("mace_jitte"),
            Self::Spear => Some("spear"),
            Self::Staff => Some("staff"),
            Self::Sword | Self::SwordMS => Some("sword"),
            Self::Unarmed => Some("unarmed"),

            Self::TwoHandedAxe => Some("two_handed_axe"),
            Self::TwoHandedMace => Some("two_handed_mace"),
            Self::TwoHandedSpear => Some("two_handed_spear"),
            Self::TwoHandedSword => Some("two_handed_sword"),
            _ => None,
        }
    }
}
