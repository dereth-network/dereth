//! Vectors: local ACE factory-enum switch cases in this module
//! Factories/Enum extension helpers (melee weapon skill, society, treasure weapon type).
//! Fixture: generated table entries and scripted random draws.

use empyrean_entity::enums::Skill;
use empyrean_tables::enums::ext::to_melee_weapon_skill;
use empyrean_tables::enums::{
    MeleeWeaponSkill, SocietyArmorType, SocietyType, TreasureArmorType, TreasureHeritageGroup,
    TreasureItemType, TreasureWeaponType,
};

#[test]
fn melee_weapon_skill() {
    assert_eq!(
        to_melee_weapon_skill(Skill::LightWeapons),
        MeleeWeaponSkill::LightWeapons
    );
    assert_eq!(to_melee_weapon_skill(Skill::Axe), MeleeWeaponSkill::Undef);
    assert_eq!(
        MeleeWeaponSkill::FinesseWeapons.get_script_name(),
        Some("finesse")
    );
    assert_eq!(
        MeleeWeaponSkill::FinesseWeapons.get_script_name_combined(),
        Some("light_finesse")
    );
    assert_eq!(MeleeWeaponSkill::Undef.get_script_name(), None);
}

#[test]
fn society_helpers() {
    assert_eq!(
        TreasureItemType::SocietyHelm.get_society_armor_type(),
        SocietyArmorType::Helm
    );
    assert_eq!(
        TreasureItemType::Armor.get_society_armor_type(),
        SocietyArmorType::Undef
    );
    assert_eq!(
        TreasureHeritageGroup::EldrytchWeb.to_society(),
        SocietyType::EldrytchWeb
    );
    assert_eq!(TreasureHeritageGroup::Sho.to_society(), SocietyType::Undef);
    assert!(TreasureArmorType::RadiantBlood.is_society_armor());
    assert!(!TreasureArmorType::Covenant.is_society_armor());
}

#[test]
fn treasure_weapon_type_helpers() {
    assert!(TreasureWeaponType::TwoHandedSpear.is_melee_weapon());
    assert!(!TreasureWeaponType::Bow.is_melee_weapon());
    assert!(TreasureWeaponType::Atlatl.is_missile_weapon());
    assert!(TreasureWeaponType::Caster.is_caster());
    assert_eq!(
        TreasureWeaponType::TwoHandedSpear.get_script_name(),
        Some("spear")
    );
    assert_eq!(
        TreasureWeaponType::TwoHandedAxe.get_script_name(),
        Some("cleaver")
    );
    assert_eq!(
        TreasureWeaponType::TwoHandedAxe.get_script_short_name(),
        Some("two_handed_axe")
    );
    assert_eq!(
        TreasureWeaponType::DaggerMS.get_script_short_name(),
        Some("dagger")
    );
    assert_eq!(TreasureWeaponType::Caster.get_script_short_name(), None);
    assert_eq!(TreasureWeaponType::MeleeWeapon.get_script_name(), None);
}
