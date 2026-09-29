//! Vectors: local expected ACE generated-table entries in this module
//! Generated tables checked against ACE C# values (armor/aetheria/cloak/gem/house/spell
//! progression/weapon/quality tables, dictionary order, commented duplicates skipped).
//! Fixture: generated table entries and scripted random draws.

use empyrean_entity::enums::{EquipmentSet, MaterialType, SpellId};
use empyrean_tables::entity::GemResult;
use empyrean_tables::enums::{
    TreasureArmorType, TreasureHeritageGroup, TreasureItemType, TreasureWeaponType, WeenieClassName,
};
use empyrean_tables::house_cell::{HOUSE_CELLS, MANSION_CELLS, ROOT_GUIDS};
use empyrean_tables::tables;

#[test]
fn armor_type_chance_tiers() {
    use tables::armor_type_chance::*;
    // ArmorTypeChance.cs: T1_Chances
    assert_eq!(
        T1_CHANCES.entries(),
        &[
            (TreasureArmorType::Leather, 0.34),
            (TreasureArmorType::StuddedLeather, 0.33),
            (TreasureArmorType::Chainmail, 0.33),
        ]
    );
    // armorTiers is T1..T8 in order; T8 has 13 entries ending with Overrobe 0.02
    assert_eq!(ARMOR_TIERS.len(), 8);
    assert!(std::ptr::eq(ARMOR_TIERS[0], &T1_CHANCES));
    assert!(std::ptr::eq(ARMOR_TIERS[7], &T8_CHANCES));
    assert_eq!(T8_CHANCES.len(), 13);
    assert_eq!(
        T8_CHANCES.entries()[12],
        (TreasureArmorType::Overrobe, 0.02)
    );
}

#[test]
fn armor_mod_vs_type_lists_hold_null() {
    use tables::armor_mod_vs_type_chance::*;
    // ArmorModVsTypeChance.cs: TierChances and qualityLevels (first entry null, T6_T8 three times)
    assert_eq!(
        TIER_CHANCES,
        [0.00, 0.01, 0.05, 0.08, 0.25, 0.40, 0.40, 0.40]
    );
    assert!(QUALITY_LEVELS[0].is_none());
    assert!(std::ptr::eq(
        QUALITY_LEVELS[1].unwrap(),
        &ARMOR_MOD_VS_TYPE_T2_QUALITY_LEVEL
    ));
    for t in &QUALITY_LEVELS[5..] {
        assert!(std::ptr::eq(
            t.unwrap(),
            &ARMOR_MOD_VS_TYPE_T6_T8_QUALITY_LEVEL
        ));
    }
}

#[test]
fn aetheria_and_cloak_tables() {
    // AetheriaChance.cs: T8_ItemMaxLevel
    assert_eq!(
        tables::aetheria_chance::T8_ITEM_MAX_LEVEL.entries()[1],
        (2, 0.55)
    );
    assert_eq!(
        tables::aetheria_chance::T8_ITEM_MAX_LEVEL.entries()[4],
        (5, 0.0005)
    );
    // CloakChance.cs: cloakSets (35) and surgeSpells (12)
    use tables::cloak_chance::*;
    assert_eq!(CLOAK_SETS.len(), 35);
    assert_eq!(CLOAK_SETS[0], EquipmentSet::CloakAlchemy);
    assert_eq!(CLOAK_SETS[34], EquipmentSet::CloakSummoning);
    assert_eq!(SURGE_SPELLS[0], SpellId::AcidRing);
    assert_eq!(SURGE_SPELLS[11], SpellId::CloakMissileDLower);
}

#[test]
fn gem_material_results_are_constructed() {
    use tables::gem_material_chance::*;
    // GemMaterialChance.cs: class1_materialChance first entry, gemClassValue
    assert_eq!(
        CLASS1_MATERIAL_CHANCE.entries()[0],
        (
            GemResult::new(WeenieClassName::gemagate, MaterialType::Agate),
            0.13
        )
    );
    assert_eq!(GEM_CLASS_VALUE, [10, 50, 100, 250, 500, 1000]);
}

#[test]
fn dictionaries_keep_declaration_order_and_look_up() {
    // MaterialTable.cs: ValueMod
    let vm = &tables::material_table::VALUE_MOD;
    assert_eq!(vm.entries()[0], (MaterialType::Ceramic, 0.2));
    assert_eq!(vm.get(&MaterialType::Satin), Some(&1.2));
    assert_eq!(vm.get(&MaterialType::Amethyst), Some(&2.3));
    assert_eq!(vm.get(&MaterialType::Unknown), None);
    // CasterSlotSpells.cs: descriptors is not declared in key order, so it has a key index
    let d = &tables::caster_slot_spells::DESCRIPTORS;
    assert_eq!(d.len(), 163);
    assert_eq!(d.get(&SpellId::DefenderSelf1), Some(&"Defender"));
    assert_eq!(d.get(&SpellId::NetherBolt1), Some(&"Nether Bolt"));
    assert_eq!(
        d.entries()[162],
        (SpellId::CurseDestructionOther1, "Curse Destruction")
    );
    // PetDeviceWcids.cs: petLevelIndexes
    let p = &tables::wcids::pet_device_wcids::PET_LEVEL_INDEXES;
    assert_eq!(p.get(&50), Some(&0));
    assert_eq!(p.get(&200), Some(&6));
    assert!(!p.contains_key(&60));
}

#[test]
fn house_cell_dictionaries() {
    // HouseCell.cs: three Dictionary<uint, uint> literals of 6490, 320 and 7140 entries
    assert_eq!(HOUSE_CELLS.len(), 6490);
    assert_eq!(HOUSE_CELLS.entries()[0], (0x3B9C000A, 0x73B9C04D));
    assert_eq!(HOUSE_CELLS.get(&0xF754000A), Some(&0x7F75401A)); // the last entry
    assert!(!HOUSE_CELLS.contains_key(&0x3B9C000B));
    assert_eq!(MANSION_CELLS.len(), 320);
    assert_eq!(MANSION_CELLS.get(&0x3ED00024), Some(&14240));
    assert_eq!(MANSION_CELLS.get(&0xEE520033), Some(&10680));
    assert_eq!(ROOT_GUIDS.len(), 7140);
    assert_eq!(ROOT_GUIDS.get(&0x7020101A), Some(&0x7B391041));
    assert_eq!(ROOT_GUIDS.entries()[7139], (0x7F75401A, 0x7F75401A));
}

#[test]
fn spell_level_progression_lists_and_ctor_order() {
    use tables::spell_level_progression::*;
    // SpellLevelProgression.cs: 739 lists, and the static constructor calls AddSpells on each,
    // StrengthOther first
    assert_eq!(STATIC_CTOR_ADD_SPELLS_ARGS.len(), 739);
    assert!(std::ptr::eq(
        STATIC_CTOR_ADD_SPELLS_ARGS[0],
        &STRENGTH_OTHER[..]
    ));
    assert_eq!(STRENGTH_OTHER[0], SpellId::StrengthOther1);
    assert_eq!(STRENGTH_OTHER[7], SpellId::StrengthOther8);
    assert_eq!(
        CANTRIPSTRENGTH,
        [
            SpellId::CANTRIPSTRENGTH1,
            SpellId::CANTRIPSTRENGTH2,
            SpellId::CANTRIPSTRENGTH3,
            SpellId::CantripStrength4
        ]
    );
}

#[test]
fn weenie_class_name_enum() {
    // Factories/Enum/WeenieClassName.cs: 41,884 members; `crate` is renamed but keeps its name
    assert_eq!(WeenieClassName::ALL.len(), 41_884);
    assert_eq!(WeenieClassName::undef.0, 0);
    assert_eq!(WeenieClassName::crate_.0, 147);
    assert_eq!(WeenieClassName::crate_.name(), Some("crate"));
    assert_eq!(WeenieClassName::ace42635_coalescedaetheria.0, 42635);
    assert_eq!(
        WeenieClassName::from_name("olthoiworker"),
        Some(WeenieClassName(3))
    );
    assert_eq!(WeenieClassName(2).name(), None); // 2 is not declared in the Factories copy
                                                 // other Factories enums
    assert_eq!(TreasureHeritageGroup::RadiantBlood.0, 16);
    assert_eq!(TreasureWeaponType::TwoHandedSword.0, 21);
    assert_eq!(TreasureItemType::EncapsulatedSpirit.0, 27);
}

#[test]
fn commented_out_duplicate_is_skipped() {
    // TreasureItemTypeChances.cs has MixedMagicEquipment twice, the first inside /* */
    let t = &tables::treasure_item_type_chances::MIXED_MAGIC_EQUIPMENT;
    assert_eq!(t.entries()[0], (TreasureItemType::Armor, 0.30));
    assert_eq!(t.len(), 5);
}

#[test]
fn weapon_tables() {
    use tables::wcids::weapons::finesse_weapon_wcids::*;
    // FinesseWeaponWcids.cs: finesseWeaponsTables (22), Hatchets first
    assert_eq!(FINESSE_WEAPONS_TABLES.len(), 22);
    assert!(std::ptr::eq(FINESSE_WEAPONS_TABLES[0].0, &HATCHETS));
    assert_eq!(FINESSE_WEAPONS_TABLES[0].1, TreasureWeaponType::Axe);
    assert_eq!(HATCHETS.entries()[0], (WeenieClassName::axehatchet, 0.40));
    // WeaponTypeChance.cs: RetailChances
    let r = &tables::weapon_type_chance::RETAIL_CHANCES;
    assert_eq!(r.entries()[0], (TreasureWeaponType::Sword, 0.09));
    assert_eq!(r.entries()[11], (TreasureWeaponType::TwoHandedWeapon, 0.10));
    // Legacy/AxeWcids.cs and SpellComponentWcids.cs
    let a = &tables::wcids::weapons::legacy::axe_wcids::AXE_WCIDS_ALUVIAN;
    assert_eq!(a.entries()[0], (WeenieClassName::axehand, 0.16));
    let glyphs = &tables::wcids::spell_component_wcids::GLYPHS;
    assert_eq!(glyphs[0], WeenieClassName::ace37343_glyphofalchemy);
    assert_eq!(tables::wcids::scroll_wcids::SCROLL_WCIDS.len(), 1392);
}

#[test]
fn quality_and_heritage_tables() {
    use tables::quality_chance::*;
    // QualityChance.cs: QualityChancePerTier and T1_QualityChances
    assert_eq!(
        QUALITY_CHANCE_PER_TIER,
        [0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.0, 1.0]
    );
    assert_eq!(T1_QUALITY_CHANCES[0], 1.0);
    assert!(T1_QUALITY_CHANCES[1..].iter().all(|&c| c == 0.0));
    // HeritageChance.cs: heritageProfile21
    assert_eq!(
        tables::heritage_chance::HERITAGE_PROFILE21.entries()[3],
        (TreasureHeritageGroup::Viamontian, 0.25)
    );
}
