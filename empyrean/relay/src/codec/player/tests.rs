//! Behaviour: none (the 1999 player description, written by the stand-in's half of the codec).

use dereth_protocol::archive::PackedHash;
use dereth_protocol::{read_body, write_blob, write_body};

use super::*;

fn description(ints: Vec<(u32, i32)>, strings: Vec<(u32, String)>) -> PlayerDescription {
    PlayerDescription(LoginPlayerDescription {
        qualities: AcQualities {
            base: AcBaseQualities {
                flags: base_flags::INT | base_flags::STRING,
                weenie_type: 0x0A,
                tables: PropertyTables {
                    ints: Some(PackedHash {
                        table_size: 16,
                        entries: ints,
                    }),
                    strings: Some(PackedHash {
                        table_size: 16,
                        entries: strings,
                    }),
                    ..PropertyTables::default()
                },
            },
            flags: quality_flags::ATTRIBUTE_CACHE,
            has_health: 1,
            attribute_cache: Some(AttributeCache::default()),
            ..AcQualities::default()
        },
        player_module: PlayerModule {
            options: 0x400,
            spell_bars: vec![vec![1, 2]],
            ..PlayerModule::default()
        },
        content_profiles: vec![ContentProfile::default()],
        inventory_placements: Vec::new(),
    })
}

fn through(d: &PlayerDescription) -> LoginPlayerDescription {
    read_body::<PlayerDescription>(&write_body(d).unwrap())
        .unwrap()
        .0
}

fn int(d: &LoginPlayerDescription, key: u32) -> Option<i32> {
    let ints = d.qualities.base.tables.ints.as_ref()?;
    ints.entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

fn int64(d: &LoginPlayerDescription, key: u32) -> Option<i64> {
    let table = d.qualities.base.tables.int64s.as_ref()?;
    table
        .entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

#[test]
fn experience_moves_from_its_int_keys_to_the_later_int64_table() {
    let d = through(&description(
        vec![
            (EXPERIENCE_TOTAL_INT, 500),
            (EXPERIENCE_AVAILABLE_INT, 40),
            (25, 3),
        ],
        Vec::new(),
    ));
    assert_eq!(int64(&d, EXPERIENCE_TOTAL_INT64), Some(500));
    assert_eq!(int64(&d, EXPERIENCE_AVAILABLE_INT64), Some(40));
    assert_eq!(int(&d, EXPERIENCE_TOTAL_INT), None);
    assert_eq!(int(&d, 25), Some(3));
    assert_ne!(d.qualities.base.flags & base_flags::INT64, 0);
}

#[test]
fn the_heritage_becomes_the_later_number() {
    for (aluvian_first, later) in [(0, 1), (1, 2), (2, 3)] {
        let d = through(&description(
            vec![(HERITAGE_INT, aluvian_first)],
            Vec::new(),
        ));
        assert_eq!(int(&d, HERITAGE_INT), Some(later));
    }
}

#[test]
fn the_combat_mode_becomes_the_later_bits() {
    let d = through(&description(vec![(COMBAT_MODE_INT, 4)], Vec::new()));
    assert_eq!(int(&d, COMBAT_MODE_INT), Some(8));
}

#[test]
fn a_combat_mode_round_trips_through_both_numberings() {
    for old in 1..=4 {
        let later = combat_mode_to_later(old);
        assert_eq!(
            combat_mode_from_later(later.cast_unsigned()),
            old.cast_unsigned()
        );
    }
}

#[test]
fn strings_are_read_in_the_1999_form_and_the_result_encodes_as_the_later_message() {
    let d = through(&description(
        Vec::new(),
        vec![(3, "Male".to_owned()), (5, String::new())],
    ));
    let strings = d.qualities.base.tables.strings.as_ref().unwrap();
    assert_eq!(strings.entries[0], (3, "Male".to_owned()));
    assert_eq!(strings.entries[1], (5, String::new()));
    let later = write_blob(&d).unwrap();
    let back = dereth_protocol::read_checked::<LoginPlayerDescription>(&later).unwrap();
    assert_eq!(back, d);
}

#[test]
fn the_short_player_module_keeps_its_options_and_one_spell_bar() {
    let d = through(&description(Vec::new(), Vec::new()));
    assert_eq!(d.player_module.options, 0x400);
    assert_eq!(d.player_module.spell_bars, vec![vec![1, 2]]);
    assert!(d.player_module.shortcuts.is_none());
    assert_eq!(d.content_profiles.len(), 1);
}

#[test]
fn shortcuts_come_when_the_selector_says_so() {
    let mut p = description(Vec::new(), Vec::new());
    p.0.player_module.option_flags = player_module_flags::SHORTCUT;
    p.0.player_module.shortcuts = Some(vec![ShortCutData {
        index: 2,
        ..ShortCutData::default()
    }]);
    let d = through(&p);
    assert_eq!(d.player_module.shortcuts.unwrap()[0].index, 2);
}

#[test]
fn a_table_the_later_reader_cannot_hold_is_an_error_not_a_misread() {
    let mut p = description(Vec::new(), Vec::new());
    p.0.qualities.flags |= quality_flags::BODY;
    assert!(read_body::<PlayerDescription>(&write_body(&p).unwrap()).is_err());
}

#[test]
fn a_truncated_description_is_an_error() {
    let body = write_body(&description(vec![(25, 1)], Vec::new())).unwrap();
    for end in 0..body.len() {
        assert!(
            read_body::<PlayerDescription>(&body[..end]).is_err(),
            "{end}"
        );
    }
}

#[test]
fn a_spell_book_page_keeps_its_charge_in_the_first_of_three_floats() {
    let mut p = description(Vec::new(), Vec::new());
    p.0.qualities.flags |= quality_flags::SPELL_BOOK;
    p.0.qualities.spell_book = Some(PackedHash {
        table_size: 16,
        entries: vec![(
            1234,
            SpellBookPage {
                casting_likelihood: 0.25,
                legacy: Some((0.6, 0)),
            },
        )],
    });
    let d = through(&p);
    // Through the later message too, which is what the client seeds its charges from.
    let later = write_blob(&d).unwrap();
    let back = dereth_protocol::read_checked::<LoginPlayerDescription>(&later).unwrap();
    let book = back.qualities.spell_book.unwrap();
    let page = book.entries[0].1;
    assert_eq!(book.entries[0].0, 1234);
    assert_eq!(page.legacy, Some((0.6, 0)));
    assert_eq!(page.casting_likelihood, 0.25);
}

#[test]
fn the_charges_the_server_can_write_all_read_as_the_three_float_form() {
    // The reader tells the forms apart by the first float: two or more is the one-float form.
    for charge in [0.5_f32, 0.92, 1.0] {
        let mut w = Writer::new();
        SpellBookPage {
            casting_likelihood: 0.0,
            legacy: Some((charge, 0)),
        }
        .write(&mut w);
        let bytes = w.into_inner();
        assert_eq!(bytes.len(), 12);
        let page = SpellBookPage::read(&mut Reader::body(&bytes)).unwrap();
        assert_eq!(page.legacy.map(|l| l.0), Some(charge));
    }
}
