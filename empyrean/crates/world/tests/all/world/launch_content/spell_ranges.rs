//! Divergence: V488
//! A Launch world's war spells roll the damage ranges the 1999 spell descriptions state: every
//! spell whose description in the October 31, 1999 portal names a damage range has that range as
//! its base and variance in the Launch pack. Fixture: the hash-pinned launch dats and pack of
//! `EMPYREAN_TEST_LAUNCH_FIXTURE`.

use super::fixture;

/// The first `<low>-<high> points` range in a spell description, when it has one.
fn stated_range(description: &str) -> Option<(i32, i32)> {
    let words: Vec<&str> = description.split_whitespace().collect();
    words.windows(2).find_map(|w| {
        if !w[1].starts_with("point") {
            return None;
        }
        let (low, high) = w[0].split_once('-')?;
        Some((low.parse().ok()?, high.parse().ok()?))
    })
}

#[test]
fn a_launch_spell_rolls_the_damage_range_its_1999_description_states() {
    let f = fixture();
    let table = f.dats.portal_dat().spell_table();
    let mut compared = 0;
    let mut differ = Vec::new();
    for (&id, spell) in &table.spells {
        let Some((low, high)) = stated_range(&spell.description) else {
            continue;
        };
        let Some(row) = f.content.get_cached_spell(id) else {
            continue;
        };
        let (Some(base), Some(variance)) = (row.base_intensity, row.variance) else {
            continue;
        };
        compared += 1;
        if (base, base + variance) != (low, high) {
            differ.push((id, spell.name.clone(), (low, high), (base, base + variance)));
        }
    }
    assert!(compared > 0, "the descriptions name damage ranges");
    assert!(
        differ.is_empty(),
        "{} of {compared} spells roll another range than their description states: {differ:?}",
        differ.len()
    );
}

#[test]
fn frost_bolt_v_rolls_46_to_90_at_launch() {
    let f = fixture();
    let row = f.content.get_cached_spell(73).expect("Frost Bolt V");
    assert_eq!(row.name, "Frost Bolt V");
    assert_eq!((row.base_intensity, row.variance), (Some(46), Some(44)));
}

/// Divergence: V494
#[test]
fn a_launch_spell_takes_the_target_type_its_formula_names() {
    use dereth_rules::weenie::item_type;
    use empyrean_dat::file_types::spell_table::SpellBaseExt;
    let table = fixture().dats.portal_dat().spell_table();
    let defender = table.spells.get(&1599).expect("Defender I");
    assert_eq!(defender.name, "Defender I");
    assert_eq!(
        defender.non_component_target_type,
        item_type::ITEM_ENCHANTABLE_TARGET,
        "Defender I may be cast at a caster"
    );
    assert_ne!(defender.non_component_target_type & 0x8000, 0);
    for (id, spell) in &table.spells {
        assert_eq!(
            spell.non_component_target_type,
            dereth_rules::weenie::formula_target_type(&spell.formula()),
            "{id} {}",
            spell.name
        );
    }
}
