//! Vectors: local scripted ACE table draws and expected selections in this module
//!  table logic with scripted draws: chance-table rolls, armor mods, cloak procs,
//! heritage/treasure profiles, tier clamp, spell progression, gem materials, cantrips, wcid
//! lookups, aetheria, melee weapon draw order.
//! Fixture: generated table entries and scripted random draws.

use empyrean_entity::enums::{MaterialType, SpellId};
use empyrean_tables::entity::ChanceTable;
use empyrean_tables::enums::{
    TreasureArmorType, TreasureHeritageGroup, TreasureItemType, TreasureWeaponType, WeenieClassName,
};
use empyrean_tables::logic::{cantrips, spells, tables, wcids, weapons};
use empyrean_tables::rng::{with_script, Draw};

#[test]
fn chance_table_roll_compares_the_double_draw_with_the_float_total() {
    // ChanceTable.Roll: `rng` is ThreadSafeRandom.Next(float, float), a double; `total` is a
    // float. 0.34f is 0.3400000035762787 as a double, so a draw of exactly 0.34 is still below
    // the first total (Leather); in float arithmetic it would not be.
    let t1 = &empyrean_tables::tables::armor_type_chance::T1_CHANCES;
    let r = with_script(&[Draw::Double(0.34)], || t1.roll(0.0));
    assert_eq!(r, TreasureArmorType::Leather);
    let r = with_script(&[Draw::Double(0.340_000_01)], || t1.roll(0.0));
    assert_eq!(r, TreasureArmorType::StuddedLeather);
    let r = with_script(&[Draw::Double(0.999)], || t1.roll(0.0));
    assert_eq!(r, TreasureArmorType::Chainmail);
}

#[test]
fn chance_table_roll_quality_mod_and_fallback() {
    let t1 = &empyrean_tables::tables::armor_type_chance::T1_CHANCES;
    // qualityMod skips entries whose running total is below it: 0.34 < 0.5, 0.67 >= 0.5
    let r = with_script(&[Draw::Double(0.0)], || t1.roll(0.5));
    assert_eq!(r, TreasureArmorType::StuddedLeather);
    // nothing matches: Last(i => i.chance > 0), skipping the trailing zero-chance entry
    let t = ChanceTable::from_vec(vec![(1, 0.2), (2, 0.3), (3, 0.0)]);
    assert_eq!(with_script(&[Draw::Double(0.9)], || t.roll(0.0)), 2);
}

#[test]
fn armor_mod_vs_type_roll() {
    use tables::armor_mod_vs_type_chance::roll;
    // tier < 2 returns before drawing
    assert!(!with_script(&[], || roll(1)));
    // TierChances[4] = 0.25f
    assert!(with_script(&[Draw::Double(0.2)], || roll(5)));
    assert!(!with_script(&[Draw::Double(0.3)], || roll(5)));
}

#[test]
fn cloak_proc_spell_includes_the_damage_reduction_slot() {
    use tables::cloak_chance::{roll_equipment_set, roll_proc_spell};
    // Next(0, surgeSpells.Count) is inclusive: 12 means the damage reduction proc (Undef)
    assert_eq!(
        with_script(&[Draw::Int(12)], roll_proc_spell),
        SpellId::Undef
    );
    assert_eq!(
        with_script(&[Draw::Int(0)], roll_proc_spell),
        SpellId::AcidRing
    );
    assert_eq!(
        with_script(&[Draw::Int(34)], roll_equipment_set),
        empyrean_entity::enums::EquipmentSet::CloakSummoning
    );
}

#[test]
fn heritage_roll_profiles() {
    use tables::heritage_chance::roll;
    // out of range: (TreasureHeritageGroup)Next(1, 3)
    assert_eq!(
        with_script(&[Draw::Int(2)], || roll(0, false)),
        TreasureHeritageGroup::Gharundim
    );
    // profile 19 is Aluvian/Gharundim/Sho; with addViamontian it becomes profile 21 (4 x 0.25)
    assert_eq!(
        with_script(&[Draw::Double(0.9)], || roll(19, false)),
        TreasureHeritageGroup::Sho
    );
    assert_eq!(
        with_script(&[Draw::Double(0.9)], || roll(19, true)),
        TreasureHeritageGroup::Viamontian
    );
}

#[test]
fn treasure_profiles_bound_check_without_drawing() {
    assert_eq!(
        with_script(&[], || tables::treasure_profile_item::roll(0)),
        TreasureItemType::Undef
    );
    assert_eq!(
        with_script(&[], || tables::treasure_profile_item::roll(12)),
        TreasureItemType::Undef
    );
    assert_eq!(
        with_script(&[], || tables::treasure_profile_magic_item::roll(25)),
        TreasureItemType::Undef
    );
    assert_eq!(
        with_script(&[], || tables::treasure_profile_mundane::roll(9)),
        TreasureItemType::Undef
    );
}

#[test]
fn clamped_tier_rolls() {
    // GemClassChance/WorkmanshipChance clamp the tier to 1-6 before indexing
    assert_eq!(
        with_script(&[Draw::Double(0.0)], || tables::gem_class_chance::roll(8)),
        4
    );
    assert_eq!(
        with_script(&[Draw::Double(0.0)], || tables::workmanship_chance::roll(
            -3
        )),
        1
    );
    // WorkmanshipChance.GetModifier: 1 + workmanship / 9f
    assert_eq!(tables::workmanship_chance::get_modifier(None), 1.0);
    assert_eq!(tables::workmanship_chance::get_modifier(Some(9)), 2.0);
    // WeaponTypeChance.Roll ignores the tier and rolls RetailChances (Sword first)
    let r = with_script(&[Draw::Double(0.0)], || tables::weapon_type_chance::roll(1));
    assert_eq!(r, TreasureWeaponType::Sword);
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn out_of_range_tier_throws() {
    // ArmorTypeChance.Roll(9): armorTiers[8] throws in .NET
    with_script(&[], || tables::armor_type_chance::roll(9));
}

#[test]
fn spell_level_progression_and_descriptor() {
    use tables::spell_level_progression::get_spell_levels;
    let levels = get_spell_levels(SpellId::StrengthOther3).expect("StrengthOther progression");
    assert_eq!(levels.len(), 8);
    assert_eq!(levels[0], SpellId::StrengthOther1);
    assert_eq!(get_spell_levels(SpellId::Undef), None);
    // CasterSlotSpells.GetDescriptor looks up the level-1 spell
    assert_eq!(
        tables::caster_slot_spells::get_descriptor(SpellId::DefenderSelf5),
        " of Defender"
    );
    assert_eq!(
        tables::caster_slot_spells::get_descriptor(SpellId::Undef),
        ""
    );
}

#[test]
fn gem_material_lookups() {
    use tables::gem_material_chance::{contains, gem_value, roll};
    // gemMaterialValue: a material's gem class value (Agate is class 1 -> 10)
    assert_eq!(gem_value(Some(MaterialType::Agate)), 10);
    assert_eq!(gem_value(None), 0);
    assert_eq!(gem_value(Some(MaterialType::Ceramic)), 0);
    assert!(contains(WeenieClassName::gemagate));
    assert!(!contains(WeenieClassName::undef));
    let r = with_script(&[Draw::Double(0.0)], || roll(0));
    assert_eq!(r.class_name, WeenieClassName::gemagate);
    assert_eq!(
        tables::material_table::get_value_mod(Some(MaterialType::Satin)),
        1.2
    );
    assert_eq!(tables::material_table::get_value_mod(None), 1.0);
}

#[test]
fn cantrip_tables_are_built_from_the_progressions() {
    // ArmorCantrips.BuildSpells: row 0 is CANTRIPSTRENGTH1's four levels
    let row = &cantrips::armor_cantrips::TABLE[0];
    assert_eq!(
        row,
        &[
            SpellId::CANTRIPSTRENGTH1,
            SpellId::CANTRIPSTRENGTH2,
            SpellId::CANTRIPSTRENGTH3,
            SpellId::CantripStrength4
        ]
    );
    assert_eq!(cantrips::armor_cantrips::TABLE.len(), 60);
    assert!(cantrips::armor_cantrips::TABLE
        .iter()
        .all(|r| r[0] != SpellId::Undef));
}

#[test]
fn spell_tables_and_creature_life_lists() {
    // ArmorSpells.BuildSpells keeps the banes and Impenetrability out of CreatureLifeTable
    let armor = &*spells::armor_spells::TABLES;
    assert_eq!(armor.table.len(), 63);
    assert!(!armor
        .creature_life_table
        .contains(&SpellId::Impenetrability1));
    assert!(!armor.creature_life_table.contains(&SpellId::BladeBane1));
    assert_eq!(armor.creature_life_table.len(), 63 - 8);
    // GemSpells: 8 tiers x 44 creature spells, row j is tier j+1
    let gem = &*spells::gem_spells::MATRICES;
    assert_eq!(gem.gem_creature_spell_matrix.len(), 8);
    assert_eq!(gem.gem_creature_spell_matrix[0].len(), 44);
    // ScrollSpells: 133 + 57 + 37 + 35 + 8 spells, 7 levels each
    assert_eq!(spells::scroll_spells::num_spells(), 270);
    assert_eq!(spells::scroll_spells::TABLE.len(), 270);
    assert_eq!(spells::scroll_spells::TABLE[0].len(), 7);
}

#[test]
fn cantrip_chance_scaling() {
    use cantrips::cantrip_chance::{rescale, scale_num_cantrips};
    let t1 = &empyrean_tables::tables::cantrips::cantrip_chance::T1_NUM_CANTRIPS; // (0, .95), (1, .05)
                                                                                  // ScaleNumCantrips(x2): 1 cantrip becomes 0.05f * 2 = 0.1f, 0 takes the rest (1 - 0.1f)
    let s = scale_num_cantrips(t1, 2.0);
    assert_eq!(s.entries(), &[(0, 1.0 - 0.1), (1, 0.1)]);
    // T7_T8 (1: .81, 2: .17, 3: .016, 4: .004) x2 sums past 1: rescaled, then 0 gets 0
    let t78 = &empyrean_tables::tables::cantrips::cantrip_chance::T7_T8_NUM_CANTRIPS;
    let s = scale_num_cantrips(t78, 2.0);
    assert_eq!(s.entries()[0], (0, 0.0));
    assert_eq!(s.len(), 5);
    let sum: f32 = s.entries()[1..].iter().map(|e| e.1).sum();
    assert!((sum - 1.0).abs() < 1e-6);
    // Rescale returns the table untouched when its (double-accumulated) sum is the target
    let same = rescale(ChanceTable::from_vec(vec![(0, 0.95), (1, 0.05)]), 1.0);
    assert_eq!(same.entries(), &[(0, 0.95), (1, 0.05)]);
}

#[test]
fn wcid_lookups_built_by_static_constructors() {
    // ArmorWcids: TryAdd, the first table a wcid is in wins (buckler is in LeatherWcids first)
    let buckler = wcids::armor_wcids::try_get_value(WeenieClassName::buckler);
    assert_eq!(buckler, Some(TreasureArmorType::Leather));
    assert_eq!(
        wcids::armor_wcids::try_get_value(WeenieClassName::undef),
        None
    );
    assert!(wcids::aetheria_wcids::contains(
        WeenieClassName::ace42636_coalescedaetheria
    ));
    assert!(wcids::generic_wcids::contains(WeenieClassName::chalice));
    assert!(wcids::scroll_wcids::contains(
        WeenieClassName::scrollfocusself
    ));
    // PetDeviceWcids: 36 distinct lists survive the two Unions
    assert_eq!(wcids::pet_device_wcids::PET_DEVICES.len(), 36);
    assert!(wcids::pet_device_wcids::contains(
        WeenieClassName::ace48942_fireskeletonminionessence50
    ));
    assert_eq!(
        weapons::atlatl_wcids::try_get_value(WeenieClassName::atlatlroyal),
        Some(TreasureWeaponType::Atlatl)
    );
    assert_eq!(
        weapons::finesse_weapon_wcids::try_get_value(WeenieClassName::axehatchet),
        Some(TreasureWeaponType::Axe)
    );
}

#[test]
fn aetheria_roll_by_tier() {
    use wcids::aetheria_wcids::roll;
    assert_eq!(
        with_script(&[], || roll(5)),
        WeenieClassName::ace42635_coalescedaetheria
    );
    // tier 6: Next(0, 1); tier 7/8: Next(0, 2) over blue, yellow, red
    assert_eq!(
        with_script(&[Draw::Int(1)], || roll(6)),
        WeenieClassName::ace42637_coalescedaetheria
    );
    assert_eq!(
        with_script(&[Draw::Int(2)], || roll(8)),
        WeenieClassName::ace42636_coalescedaetheria
    );
    assert_eq!(with_script(&[], || roll(4)), WeenieClassName::undef);
}

#[test]
fn melee_weapon_roll_draw_order() {
    // WeaponWcids.RollMeleeWeapon: Next(1, 3) picks light (2); LightWeaponWcids.Roll then picks a
    // table with Next(0, count - 1) and rolls it (one double): Dolabras, first entry
    let mut weapon_type = TreasureWeaponType::Undef;
    let wcid = with_script(&[Draw::Int(2), Draw::Int(0), Draw::Double(0.0)], || {
        wcids::weapon_wcids::roll_melee_weapon(&mut weapon_type)
    });
    assert_eq!(wcid, WeenieClassName::axedolabra);
    assert_eq!(weapon_type, TreasureWeaponType::Axe);
    // two-handed: GreatAxes first
    let wcid = with_script(&[Draw::Int(0), Draw::Double(0.0)], || {
        wcids::weapon_wcids::roll_two_handed_weapon_wcid(&mut weapon_type)
    });
    assert_eq!(wcid, WeenieClassName::ace41052_greataxe);
    assert_eq!(weapon_type, TreasureWeaponType::TwoHandedAxe);
    // society armor: Next(1, 3) = 3 is Radiant Blood
    let mut armor_type = TreasureArmorType::Society;
    with_script(&[Draw::Int(3), Draw::Double(0.0)], || {
        wcids::armor_wcids::roll_society_armor(&mut armor_type)
    });
    assert_eq!(armor_type, TreasureArmorType::RadiantBlood);
}

#[test]
fn unscripted_draws_use_thread_safe_random() {
    // Unscripted draws go to empyrean_common's ThreadSafeRandom: seeded, they are deterministic and
    // no longer a not_ported! stub.
    use empyrean_common::thread_safe_random::ThreadSafeRandom;
    let table = empyrean_tables::tables::wcids::cloak_wcids::CLOAK_WCIDS;
    empyrean_common::not_ported::take_local();
    ThreadSafeRandom::seed(7);
    let first = wcids::cloak_wcids::roll();
    ThreadSafeRandom::seed(7);
    assert_eq!(wcids::cloak_wcids::roll(), first, "same seed, same roll");
    assert!(table.contains(&first));
    assert!(empyrean_common::not_ported::take_local().is_empty());
}
