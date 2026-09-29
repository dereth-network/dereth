//! ACE: Source/ACE.Entity/Models/PropertiesEnchantmentRegistryExtensions.cs::PropertiesEnchantmentRegistryExtensions
//! Enchantment registry: top layer groups in first-appearance order and picks the best; equal-
//! power cross-set tie goes to the later entry; level 8 aura self spells follow the generic rule;
//! stat-mod filters, lookups/removal, heartbeat tick-down.
//! Fixture: enum values and synthetic entity records.

// V250, V251, V276.

use empyrean_entity::enums::{EnchantmentTypeFlags, EquipmentSet, SpellCategory, SpellId};
use empyrean_entity::models::properties_enchantment_registry_extensions as reg;
use empyrean_entity::models::PropertiesEnchantmentRegistry;

fn e(spell_id: i32, category: u32, power: u32, start: f64) -> PropertiesEnchantmentRegistry {
    PropertiesEnchantmentRegistry {
        spell_id,
        spell_category: SpellCategory(category),
        power_level: power,
        start_time: start,
        duration: 60.0,
        caster_object_id: 0x5000_0001,
        ..Default::default()
    }
}

fn ids(v: &[&PropertiesEnchantmentRegistry]) -> Vec<i32> {
    v.iter().map(|e| e.spell_id).collect()
}

#[test]
fn top_layer_groups_in_first_appearance_order_and_picks_the_best() {
    let list = vec![
        e(100, 1, 5, -10.0),
        e(200, 2, 3, 0.0),
        e(101, 1, 5, -5.0),
        e(102, 1, 4, 0.0),
    ];
    // Category 1: power 5 ties; start -5 > -10 -> 101. Category 2 only has 200.
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [101, 200]
    );

    // V250: an exact tie goes to the later entry (ACE kept the earliest).
    let tie = vec![e(300, 1, 5, 0.0), e(301, 1, 5, 0.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&tie)).unwrap()),
        [301]
    );

    assert!(reg::get_enchantments_top_layer(None).is_none());
}

/// V276: a set spell ranks by its start time like any other (ACE
/// ranked it by its spell id). The accepted quirk: two sets' levels of one spell at equal power
/// (Gauntlet Damage Boost I = 6330 and II = 6331, both power 1) resolve to the later entry, even
/// the lower level re-added after the higher.
#[test]
fn equal_power_cross_set_tie_goes_to_the_later_entry() {
    let list = vec![e(6331, 1, 1, -30.0), e(6330, 1, 1, -5.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [6330]
    );
    // Both added on the same heartbeat: the later entry.
    let list = vec![e(6331, 1, 1, 0.0), e(6330, 1, 1, 0.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [6330]
    );
    // The newer wins whichever level it is.
    let list = vec![e(6330, 1, 1, -30.0), e(6331, 1, 1, -5.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [6331]
    );
}

/// V276: a level 8 aura self spell has no edge over an equal-power
/// spell (ACE ranked it first); the newer wins, and higher power still wins outright.
#[test]
fn level_8_aura_self_spells_follow_the_generic_rule() {
    let aura = SpellId::BloodDrinkerSelf8.0 as i32;
    let list = vec![e(5000, 1, 8, 0.0), e(aura, 1, 8, -100.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [5000]
    );
    let list = vec![e(5000, 1, 8, -100.0), e(aura, 1, 8, 0.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [aura]
    );
    let list = vec![e(5000, 1, 9, -100.0), e(aura, 1, 8, 0.0)];
    assert_eq!(
        ids(&reg::get_enchantments_top_layer(Some(&list)).unwrap()),
        [5000]
    );
}

#[test]
fn stat_mod_filters() {
    let skill = EnchantmentTypeFlags::Skill;
    let single = EnchantmentTypeFlags::SingleStat;
    let multiple = EnchantmentTypeFlags::MultipleStat;
    let mut a = e(1, 1, 1, 0.0);
    a.stat_mod_type = skill | single;
    a.stat_mod_key = 6;
    let mut b = e(2, 2, 1, 0.0);
    b.stat_mod_type = skill | multiple;
    b.stat_mod_key = 0;
    let mut c = e(3, 3, 1, 0.0);
    c.stat_mod_type = skill | multiple | EnchantmentTypeFlags::Vitae;
    c.stat_mod_key = 0;
    let mut d = e(4, 4, 1, 0.0);
    d.stat_mod_type = skill | single;
    d.stat_mod_key = 7;
    let list = vec![a, b, c, d];

    assert_eq!(
        ids(&reg::get_enchantments_by_stat_mod_type(Some(&list), skill).unwrap()),
        [1, 2, 3, 4]
    );
    assert_eq!(
        ids(
            &reg::get_enchantments_top_layer_by_stat_mod_type(Some(&list), skill | multiple)
                .unwrap()
        ),
        [2, 3]
    );
    // Key 6 without multiples: only a.
    assert_eq!(
        ids(&reg::get_enchantments_top_layer_by_stat_mod_type_and_key(
            Some(&list),
            skill,
            6,
            false
        )
        .unwrap()),
        [1]
    );
    // With multiples: a (SingleStat added to the mask) and b (MultipleStat, key 0, no Vitae).
    assert_eq!(
        ids(
            &reg::get_enchantments_top_layer_by_stat_mod_type_and_key(Some(&list), skill, 6, true)
                .unwrap()
        ),
        [1, 2]
    );
    assert_eq!(
        ids(&reg::get_enchantments_by_category(Some(&list), SpellCategory(3)).unwrap()),
        [3]
    );
}

#[test]
fn lookups_and_removal() {
    let mut list = vec![e(10, 1, 1, 0.0), e(20, 1, 1, 0.0)];
    list[1].caster_object_id = 0x8000_0001;
    list[1].spell_set_id = EquipmentSet::Soldiers;
    let mut other = e(10, 1, 1, 0.0);
    other.caster_object_id = 0x8000_0002;
    list.push(other);

    assert!(reg::has_enchantments(Some(&list)));
    assert!(!reg::has_enchantments(None));
    assert!(reg::has_enchantment(Some(&list), 20));
    assert!(!reg::has_enchantment(Some(&list), 30));
    assert_eq!(
        reg::get_enchantment_by_spell(Some(&list), 10, None)
            .unwrap()
            .caster_object_id,
        0x5000_0001
    );
    assert_eq!(
        reg::get_enchantment_by_spell(Some(&list), 10, Some(0x8000_0002))
            .unwrap()
            .caster_object_id,
        0x8000_0002
    );
    assert!(reg::get_enchantment_by_spell(Some(&list), 10, Some(1)).is_none());
    assert_eq!(
        reg::get_enchantment_by_spell_set(Some(&list), 20, EquipmentSet::Soldiers)
            .unwrap()
            .spell_id,
        20
    );
    assert!(reg::get_enchantment_by_spell_set(Some(&list), 20, EquipmentSet(0)).is_none());

    reg::get_enchantment_by_spell_mut(Some(&mut list), 20, None)
        .unwrap()
        .layer_id = 4;
    assert_eq!(list[1].layer_id, 4);

    // F9: the category query as positions, same filter and order as the reference query.
    list[1].spell_category = SpellCategory(2);
    assert_eq!(
        reg::get_enchantments_by_category_indices(Some(&list), SpellCategory(1)),
        Some(vec![0, 2])
    );
    assert_eq!(
        reg::get_enchantments_by_category_indices(Some(&list), SpellCategory(2)),
        Some(vec![1])
    );
    assert_eq!(
        reg::get_enchantments_by_category_indices(None, SpellCategory(1)),
        None
    );
    list[1].spell_category = SpellCategory(1);

    assert!(reg::try_remove_enchantment(
        Some(&mut list),
        10,
        0x8000_0002
    ));
    assert!(!reg::try_remove_enchantment(
        Some(&mut list),
        10,
        0x8000_0002
    ));
    assert_eq!(list.len(), 2);

    reg::add_enchantment(&mut list, e(30, 2, 1, 0.0));
    reg::remove_all_enchantments(Some(&mut list), &[20]);
    assert_eq!(list.iter().map(|e| e.spell_id).collect::<Vec<_>>(), [20]);
}

#[test]
fn heart_beat_ticks_start_time_down_to_minus_duration() {
    let mut list = vec![e(1, 1, 1, 0.0), e(2, 2, 1, 0.0)];
    list[0].duration = 10.0;
    list[1].duration = -1.0; // permanent
    let expired = reg::heart_beat_enchantments_and_return_expired(Some(&mut list), 5.0).unwrap();
    assert!(expired.is_empty());
    assert_eq!(list[0].start_time, -5.0);
    let expired = reg::heart_beat_enchantments_and_return_expired(Some(&mut list), 5.0).unwrap();
    assert_eq!(expired.iter().map(|e| e.spell_id).collect::<Vec<_>>(), [1]);
    assert_eq!(expired[0].start_time, -10.0);
    assert_eq!(list[1].start_time, -10.0);
    assert!(reg::heart_beat_enchantments_and_return_expired(None, 5.0).is_none());
}
