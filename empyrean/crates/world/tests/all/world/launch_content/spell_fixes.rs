//! Divergence: V512, V513, V514
//! The Launch spell fixes against the real 1999 spell table: they change the spells ac1999's dat
//! patches change, in those numbers, and leave every other spell and field alone. A world whose
//! data overlay carries the fixed table plays every spell as one over the plain files does, and
//! applies nothing twice. Fixture: the hash-pinned launch dats of `EMPYREAN_TEST_LAUNCH_FIXTURE`,
//! and an overlay written over them in a scratch folder.

use std::sync::Arc;

use dereth_dat::overlay::{OverlayDir, OverlayWriter};
use dereth_dat::{decompose, DatFile, ModernDat};
use dereth_primitives::{ContainerEra, DataId};
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::era::{EraExt, EraId};
use empyrean_dat::launch_spell_fixes::TableFixes;
use empyrean_dat::{file_id, DatManager, RealDats};
use empyrean_world::entity::launch_spell_fixes;
use empyrean_world::entity::spell::Spell;
use empyrean_world::World;

use super::fixture;

/// The Launch files with an overlay over the portal file holding the spell table as `fixes` make
/// it, kept in `scratch`.
fn overlaid(scratch: &std::path::Path, fixes: TableFixes) -> Arc<DatManager> {
    let dir = fixture().dat_directory.clone();
    let portal = DatFile::open(&dir.join("portal.dat")).expect("portal");
    let base = portal.base();
    let table = base.read(DataId(file_id::SPELL_TABLE)).expect("the table");
    let fixed = launch_spell_fixes::patch_table(&table, fixes).expect("patched");
    let overlay = OverlayDir::new(&scratch.join("overlay")).expect("overlay folder");
    let revision = decompose::revision_over(&base);
    let mut w = OverlayWriter::open_or_create(
        &overlay.container(ModernDat::Portal),
        &base,
        "portal.dat",
        "spell fixes",
        0,
    )
    .expect("writer");
    w.put(DataId(file_id::SPELL_TABLE), &fixed, 1, revision, 0)
        .expect("put");
    w.add_iteration(revision, 0).expect("revision");
    w.flush(0).expect("flush");
    drop(w);
    let source = RealDats::open_era(&dir, ContainerEra::Classic)
        .expect("the Launch pair")
        .with_overlay(overlay.path())
        .expect("the overlay over it");
    DatManager::initialize(Arc::new(source)).expect("the world's dats")
}

fn launch_world(dats: Arc<DatManager>) -> World {
    let mut w = World::new(ClockSnapshot::take(&VirtualClock::default(), 0.0), dats);
    w.era = EraId::Launch.rules();
    w
}

/// What a player meets of a spell the fixes touch.
fn played(w: &World, id: u32) -> (u64, f32, f32, u32, bool) {
    let s = Spell::new(w, id, false);
    (
        s.duration().to_bits(),
        s.base_range_constant(),
        s.base_range_mod(),
        s.power(),
        s.is_resistable(),
    )
}

/// The table the overlay carries says what the code says over the plain table, for every spell, so
/// the server reads one answer whichever way it is served and the code is not applied twice.
#[test]
fn a_world_whose_overlay_carries_the_fixed_table_plays_every_spell_as_the_fixes_do() {
    let scratch = dereth_dat::testing::ScratchDir::new("launch-spell-overlay").expect("scratch");
    let over = launch_world(overlaid(scratch.path(), TableFixes::ALL));
    let plain = launch_world(Arc::clone(&fixture().dats));
    assert!(launch_spell_fixes::overlay_decides(over.dats.portal_dat()));
    assert!(!launch_spell_fixes::overlay_decides(
        plain.dats.portal_dat()
    ));
    let ids: Vec<u32> = plain
        .dats
        .portal_dat()
        .spell_table()
        .spells
        .keys()
        .copied()
        .collect();
    assert_eq!(ids.len(), 1635);
    let differ: Vec<u32> = ids
        .iter()
        .copied()
        .filter(|id| played(&over, *id) != played(&plain, *id))
        .collect();
    assert_eq!(differ, Vec::<u32>::new());
}

/// An overlay whose table carries only the client's fixes leaves the server's to the code, so a
/// half-fixed table is caught, not played as fixed.
#[test]
fn an_overlay_table_without_the_rule_fixes_plays_differently_from_the_fixes() {
    let scratch =
        dereth_dat::testing::ScratchDir::new("launch-spell-overlay-half").expect("scratch");
    let fixes = TableFixes {
        formulas_and_text: true,
        ..TableFixes::default()
    };
    let over = launch_world(overlaid(scratch.path(), fixes));
    let plain = launch_world(Arc::clone(&fixture().dats));
    let strength_other_ii = 1333;
    assert_ne!(
        played(&over, strength_other_ii),
        played(&plain, strength_other_ii)
    );
}

/// The table the overlay carries fixes 7 formulas and 5 texts, which only a client reads, and
/// every number the code fixes.
#[test]
fn the_fixed_table_changes_7_formulas_5_texts_and_the_codes_numbers() {
    let table = fixture().dats.portal_dat().spell_table();
    let raw = fixture()
        .dats
        .portal_dat()
        .get_reader_for_file(file_id::SPELL_TABLE)
        .expect("the table's bytes");
    let fixed = launch_spell_fixes::patch_table(&raw, TableFixes::ALL).expect("patched");
    let fixed = <dereth_assets::SpellTable as dereth_assets::Decode>::decode_classic(
        &mut dereth_dat::Cursor::new(&fixed),
    )
    .expect("reads");
    let pairs = || {
        table
            .spells
            .iter()
            .map(|(id, before)| (before, &fixed.spells[id]))
    };
    assert_eq!(pairs().filter(|(b, a)| b.comps != a.comps).count(), 7);
    assert_eq!(
        pairs()
            .filter(|(b, a)| b.description != a.description)
            .count(),
        5
    );
    for (before, after) in pairs() {
        let mut coded = before.clone();
        launch_spell_fixes::apply(&mut coded);
        assert_eq!(
            (
                coded.duration,
                coded.base_range_constant,
                coded.base_range_mod,
                coded.power,
                coded.bitfield
            ),
            (
                after.duration,
                after.base_range_constant,
                after.base_range_mod,
                after.power,
                after.bitfield
            ),
            "{}",
            before.name
        );
    }
}

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
