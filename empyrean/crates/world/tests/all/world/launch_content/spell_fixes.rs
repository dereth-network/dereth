//! Divergence: V512, V513, V514
//! The Launch spell fixes against the real 1999 spell table: they change the spells ac1999's dat
//! patches change, in those numbers, and leave every other spell and field alone. Fixture: the
//! hash-pinned launch dats of `EMPYREAN_TEST_LAUNCH_FIXTURE`.

use empyrean_world::entity::launch_spell_fixes;

use super::fixture;

/// How many spells of the 1999 table `changed` holds for.
fn count(
    changed: impl Fn(&dereth_assets::tables::SpellBase, &dereth_assets::tables::SpellBase) -> bool,
) -> usize {
    let table = fixture().dats.portal_dat().spell_table();
    table
        .spells
        .values()
        .filter(|before| {
            let mut after = (*before).clone();
            launch_spell_fixes::apply(&mut after);
            changed(before, &after)
        })
        .count()
}

/// 1,123 enchantments of the life, item and creature schools change duration.
#[test]
fn the_fixes_change_the_durations_of_1123_enchantments_in_the_1999_table() {
    assert_eq!(count(|b, a| b.duration != a.duration), 1123);
}

/// The six families of six levels.
#[test]
fn the_fixes_shorten_the_range_of_36_spells_in_the_1999_table() {
    assert_eq!(
        count(|b, a| (b.base_range_constant, b.base_range_mod)
            != (a.base_range_constant, a.base_range_mod)),
        36
    );
}

/// Every power named in the fixes differs from the 1999 table's.
#[test]
fn the_fixes_raise_the_power_of_6_spells_in_the_1999_table() {
    assert_eq!(count(|b, a| b.power < a.power), 6);
    assert_eq!(count(|b, a| b.power != a.power), 6);
}

/// Imperil Other I to VI, and nothing else.
#[test]
fn the_fixes_make_6_spells_of_the_1999_table_resistable() {
    assert_eq!(count(|b, a| b.bitfield != a.bitfield), 6);
    assert_eq!(count(|b, a| b.bitfield & 1 == 0 && a.bitfield & 1 == 1), 6);
}

/// A spell's name, text, formula and cost stay the file's: a client reads those from its own copy.
#[test]
fn the_fixes_leave_every_spells_name_text_formula_and_mana_as_the_1999_table_has_them() {
    assert_eq!(
        count(|b, a| b.name != a.name
            || b.description != a.description
            || b.raw_comps != a.raw_comps
            || b.comps != a.comps
            || b.base_mana != a.base_mana),
        0
    );
}
