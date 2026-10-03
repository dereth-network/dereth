//! Spellbook panel fills, skips an identical frame, and rebuilds on display-order, level, bitfield,
//! name or icon change alone.
//! Fixture: synthetic views and UI state; the catalogue check also reads production source files.

use super::common::*;
use dereth_primitives::DataId;
use dereth_ui_screens::panels::spellbook::{SpellbookPanel, DEFAULT_SPELL_FILTERS};
use dereth_ui_screens::view::{GameView, SpellEntry};

/// The spell school: 4 is Creature, 2 is Life — the two values the spellbook filter switches on.
const CREATURE: u32 = 4;
const LIFE: u32 = 2;

/// The spell bitfield's `FellowshipSpell` flag, which turns into
/// row 4 of `0x10000007 UISpellOverlays`.
const FELLOWSHIP: u32 = 0x2000;

/// A spellbook this file owns outright, so that one field of one entry can move with the id list
/// frozen — the frame `Hud::build_spells` cannot produce, because it derives every one of these
/// fields from the id through the static spell table.
#[derive(Debug)]
struct Book(Vec<SpellEntry>, u32);

impl GameView for Book {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.0
    }
    fn spell_filters(&self) -> u32 {
        self.1
    }
}

/// `(id, name, icon, school, level, display_order, bitfield)` — every field, so a station names
/// the one it moves rather than relying on a `..Default`.
fn entry(
    id: u32,
    name: &str,
    school: u32,
    level: u32,
    display_order: i32,
    bitfield: u32,
) -> SpellEntry {
    SpellEntry {
        id,
        name: name.to_owned(),
        icon: Some(DataId(0x0600_1000 + id)),
        school,
        level,
        display_order,
        bitfield,
    }
}

/// Two Creature spells and one Life spell, in `_display_order` 10, 20, 30.
fn book() -> Book {
    Book(
        vec![
            entry(157, "Strength Self I", CREATURE, 1, 10, 0),
            entry(1074, "Heal Self I", LIFE, 1, 20, 0),
            entry(158, "Strength Self II", CREATURE, 2, 30, 0),
        ],
        DEFAULT_SPELL_FILTERS,
    )
}

/// Assert that two frames differ in **exactly** the field the station is about: the same ids, in
/// the same order, and the same filter mask. A premise, not a measurement.
fn only_one_thing_moved(a: &Book, b: &Book) {
    assert_eq!(
        a.0.iter().map(|s| s.id).collect::<Vec<_>>(),
        b.0.iter().map(|s| s.id).collect::<Vec<_>>(),
        "the spell id list is identical across the station -- which is the whole point: the old \
         guard held nothing else"
    );
    assert_eq!(a.spell_filters(), b.spell_filters(), "and the filter mask");
}

// ---------------------------------------------------------------------------------------------
// 0. The premise, and the guard's liveness
// ---------------------------------------------------------------------------------------------

/// Behaviour: spellbook.redraw.an-identical-frame-does-not-rebuild-and-a-changed-spell-list-does
/// **The panel builds a list, and its gate is alive rather than welded.**
///
/// Without this every station below is satisfiable by a panel that rebuilds unconditionally (which
/// would pass the "changed" half of each and fail the "unchanged" half) or by one that shows
/// nothing at all.
#[test]
fn the_book_fills_and_an_identical_frame_does_not_rebuild() {
    let mut ui = ui();
    let mut p = SpellbookPanel::default();

    let v = book();
    assert!(p.update(&mut ui, &v), "the first drive is always a rebuild");
    assert_eq!(
        p.shown,
        vec![157, 1074, 158],
        "the premise: all three spells passed the default filters, in `_display_order`"
    );
    assert!(
        !p.update(&mut ui, &v),
        "an identical frame does not rebuild"
    );

    // The discriminator: the gate is shut because nothing moved, not because it is welded.
    let mut moved = book();
    moved.0.remove(1);
    assert!(
        p.update(&mut ui, &moved),
        "an id-list change goes straight through"
    );
    assert_eq!(p.shown, vec![157, 158]);
    assert!(
        !p.update(&mut ui, &moved),
        "and the gate closes again behind it"
    );
}

// ---------------------------------------------------------------------------------------------
// 1. `display_order` alone — and its consequence is the order on screen
// ---------------------------------------------------------------------------------------------

/// Behaviour: spellbook.redraw.the-rows-re-sort-when-a-spells-place-in-the-order-moves
/// **A `_display_order` that moves under an unchanged id list re-sorts the list.**
///
/// Retail scans for the first row whose display order
/// is on the far side of the new spell's; `SpellbookPanel::sorted` does the same as one stable
/// sort. The old guard held the ids and could not see the number the sort is *by*, so a book whose
/// entries arrived in a different order under the same ids would have kept the old order on
/// screen.
///
/// The consequence is asserted, not only the rebuild: `SpellbookPanel::shown` is the list order,
/// and it inverts.
///
/// **Falsified by** narrowing the guard back to the id list.
#[test]
fn a_display_order_change_alone_rebuilds_and_re_sorts_the_list() {
    let mut ui = ui();
    let mut p = SpellbookPanel::default();

    let before = book();
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert!(!p.update(&mut ui, &before), "the guard is live");
    assert_eq!(
        p.shown,
        vec![157, 1074, 158],
        "in `_display_order` 10, 20, 30"
    );

    // Spell 157 moves from the front of the order to the back. Nothing else about it changes.
    let mut after = book();
    after.0[0].display_order = 40;
    only_one_thing_moved(&before, &after);
    assert_eq!(
        before
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        after
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        "only `_display_order` moves -- level, school and bitfield are all held"
    );
    assert!(
        p.update(&mut ui, &after),
        "`display_order` is inside the guard"
    );
    assert_eq!(
        p.shown,
        vec![1074, 158, 157],
        "and the list on screen re-sorted behind it"
    );
    assert!(
        !p.update(&mut ui, &after),
        "the guard closes again behind it"
    );

    // And back: a guard is a claim about an edge, and an edge has two of them.
    assert!(p.update(&mut ui, &before), "moving it back is a change too");
    assert_eq!(p.shown, vec![157, 1074, 158]);
    assert!(
        !p.update(&mut ui, &before),
        "and the guard closes again behind that"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. `level` alone — and its consequence is whether the row is on the list at all
// ---------------------------------------------------------------------------------------------

/// **A `level` that moves under an unchanged id list re-runs the filter.**
///
/// The spell filter's second gate is the level bit, and the client's spell level is not a
/// stored field — it is derived from
/// the spell's first formula slot, so a host whose table read changed would move it with the id
/// standing still. The station drives it with a filter that admits levels 1 and 2 and refuses
/// everything above, so the moved entry **leaves the list**.
///
/// **Falsified by** narrowing the guard back to the id list.
#[test]
fn a_level_change_alone_rebuilds_and_re_filters_the_list() {
    let mut ui = ui();
    let mut p = SpellbookPanel::default();

    // Levels 1 and 2 only: bits 4 and 5 of `spell_filters_`, plus all five schools.
    let mut before = book();
    before.1 = 0x2000 | 0b1111 | (0b11 << 4);
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert!(!p.update(&mut ui, &before), "the guard is live");
    assert_eq!(
        p.shown,
        vec![157, 1074, 158],
        "all three are level 1 or 2, so all three show"
    );

    // Spell 158 becomes a level-3 spell. Its id, school, order and bitfield do not move.
    let mut after = book();
    after.1 = before.1;
    after.0[2].level = 3;
    only_one_thing_moved(&before, &after);
    assert_eq!(
        before
            .0
            .iter()
            .map(|s| (s.id, s.display_order, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        after
            .0
            .iter()
            .map(|s| (s.id, s.display_order, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        "only `level` moves"
    );
    assert!(p.update(&mut ui, &after), "`level` is inside the guard");
    assert_eq!(
        p.shown,
        vec![157, 1074],
        "and the filter dropped it from the list"
    );
    assert!(
        !p.update(&mut ui, &after),
        "the guard closes again behind it"
    );

    assert!(
        p.update(&mut ui, &before),
        "and it comes back when the level does"
    );
    assert_eq!(p.shown, vec![157, 1074, 158]);
    assert!(
        !p.update(&mut ui, &before),
        "the guard closes again behind that"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. `bitfield` alone — the field with no consequence this file can see
// ---------------------------------------------------------------------------------------------

/// Behaviour: spellbook.redraw.a-badge-follows-a-spells-own-flags-under-an-unchanged-list
/// **A `_bitfield` that moves under an unchanged id list still rebuilds.**
///
/// This is the third field the row names, and it is the one whose consequence is **invisible
/// here**: the spell bitfield reaches the screen only through
/// the composited spell icon, whose `FellowshipSpell (0x2000)` bit selects
/// row 4 of `0x10000007 UISpellOverlays` — a surface, on an element this dat-free file does not
/// have. So the assertion here is the guard's, and the drawn half is
/// `dereth/client/tests/o567_spellbook_icon_gate.rs`. Saying which half is which is the point;
/// asserting `shown` again would look like a measurement and be a tautology, because the
/// membership and the order do not move.
///
/// **Falsified by** narrowing the guard back to the id list.
#[test]
fn a_bitfield_change_alone_rebuilds_even_though_the_list_shape_is_unchanged() {
    let mut ui = ui();
    let mut p = SpellbookPanel::default();

    let before = book();
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert!(!p.update(&mut ui, &before), "the guard is live");
    let order_before = p.shown.clone();

    let mut after = book();
    after.0[1].bitfield = FELLOWSHIP;
    only_one_thing_moved(&before, &after);
    assert_eq!(
        before
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.display_order))
            .collect::<Vec<_>>(),
        after
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.display_order))
            .collect::<Vec<_>>(),
        "only `bitfield` moves"
    );
    assert!(p.update(&mut ui, &after), "`bitfield` is inside the guard");
    assert_eq!(
        p.shown, order_before,
        "and the list's membership and order are unchanged -- which is exactly why this station \
         needs the element file to say what the change DID"
    );
    assert!(
        !p.update(&mut ui, &after),
        "the guard closes again behind it"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. `name` and `icon` — the two the row does not name, and the reason the whole value is compared
// ---------------------------------------------------------------------------------------------

/// A name or icon change alone rebuilds too.
#[test]
fn a_name_or_icon_change_alone_rebuilds_too() {
    let mut ui = ui();
    let mut p = SpellbookPanel::default();

    let before = book();
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert!(!p.update(&mut ui, &before), "the guard is live");

    let mut renamed = book();
    renamed.0[0].name = "Strength Other I".to_owned();
    only_one_thing_moved(&before, &renamed);
    assert_eq!(
        before.0.iter().map(|s| s.icon).collect::<Vec<_>>(),
        renamed.0.iter().map(|s| s.icon).collect::<Vec<_>>(),
        "only `name` moves"
    );
    assert!(p.update(&mut ui, &renamed), "`name` is inside the guard");
    assert!(
        !p.update(&mut ui, &renamed),
        "the guard closes again behind it"
    );

    let mut re_iconed = book();
    re_iconed.0[0].name = "Strength Other I".to_owned();
    re_iconed.0[0].icon = Some(DataId(0x0600_4444));
    only_one_thing_moved(&renamed, &re_iconed);
    assert_eq!(
        renamed.0.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        re_iconed
            .0
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>(),
        "and here only `icon` moves"
    );
    assert!(p.update(&mut ui, &re_iconed), "`icon` is inside the guard");
    assert!(
        !p.update(&mut ui, &re_iconed),
        "the guard closes again behind it"
    );
}
