//! String tables and string states built in memory: what packs, what copies, what a table needs
//! loaded with it.

use auberean_assets::strings::{StringEntry, StringState, StringTable, STRING_DATA_TYPE};
use auberean_assets::{encode, Encode};
use auberean_dat::{QualifiedIds, Writer};

fn entry(token: u32, names: &[&str]) -> StringEntry {
    StringEntry {
        token,
        table: 0,
        strings: vec![Some("a".into()), None, Some("bc".into())],
        variables: vec![1, 2],
        variable_names: names.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// An entry's variable names are stored only when two of them or more are the names a generated
/// item's name is put together from (`PRE`, `NAME`, `MID`, `POST`); otherwise they are dropped
/// when the entry is written. An entry packs to as many bytes as it writes.
///
/// Behaviour: data.string-table.names-worth-packing
#[test]
fn variable_names_are_stored_only_for_an_item_name_and_the_size_says_so() {
    let item = entry(1, &["PRE", "NAME", "x"]);
    let lone = entry(2, &["NAME", "x", "y"]);
    assert!(item.names_worth_packing());
    assert!(!lone.names_worth_packing());
    for e in [&item, &lone] {
        let mut w = Writer::new();
        e.write(&mut w);
        assert_eq!(w.finish().len(), e.pack_size(), "{:?}", e.variable_names);
    }
    let mut a = Writer::new();
    lone.write(&mut a);
    let mut b = Writer::new();
    StringEntry {
        variable_names: Vec::new(),
        ..lone.clone()
    }
    .write(&mut b);
    assert_eq!(a.finish(), b.finish(), "the lone name is not written");
    let table = StringTable {
        entries: vec![item, lone],
        ..StringTable::default()
    };
    assert_eq!(encode(0x2500_0001, &table).len(), table.pack_size());
}

/// A new or emptied table has its number settings unset; a copy of a table drops its entries'
/// empty string slots and keeps the first entry of a token; a table needs the private tables its
/// entries name loaded with it, unless string data is asked for.
///
/// Behaviour: data.string-table.copy-and-sub-ids
#[test]
fn a_copied_table_drops_empty_slots_and_names_its_private_tables() {
    let blank = StringTable::default();
    assert_eq!(
        (blank.number_base, blank.decimal_digits, blank.leading_zero),
        (0xFFFF, 0xFFFF, 0xFFFF_FFFF)
    );
    let mut private = entry(7, &[]);
    private.table = 0x2500_0002;
    let table = StringTable {
        state: 3,
        number_base: 10,
        decimal_digits: 2,
        leading_zero: 1,
        entries: vec![entry(1, &[]), private, entry(1, &["dup"])],
    };
    let mut copy = StringTable::default();
    table.copy_into(&mut copy);
    assert_eq!(
        (
            copy.state,
            copy.number_base,
            copy.decimal_digits,
            copy.leading_zero
        ),
        (3, 10, 2, 1)
    );
    assert_eq!(copy.entries.len(), 2, "a token once");
    assert_eq!(
        copy.entries[0].strings,
        [Some("a".to_owned()), Some("bc".to_owned())]
    );
    assert!(copy.entries[0].variable_names.is_empty());
    assert_eq!(copy.entry(7).map(|e| e.table), Some(0x2500_0002));
    let mut ids = QualifiedIds::new();
    table.sub_ids(0x4000_0026, &mut ids);
    assert_eq!(ids.iter().map(|q| q.id).collect::<Vec<_>>(), [0x2500_0002]);
    let mut none = QualifiedIds::new();
    table.sub_ids(STRING_DATA_TYPE, &mut none);
    assert!(none.is_empty());
    let mut cleared = table.clone();
    cleared.entries[1].clear();
    assert_eq!(cleared.entries[1].token, 7);
    assert!(cleared.entries[1].strings.is_empty() && cleared.entries[1].table == 0);
    cleared.clear();
    assert_eq!(cleared, StringTable::default());
}

/// A string state's twenty settings are the top gore level and blood color, whether the input
/// method is on, its look (bytes where the input method takes a byte), another input method's
/// setting and whether text wraps at spaces. It packs to as many bytes as it writes.
///
/// Behaviour: data.string-state.settings
#[test]
fn a_string_states_settings_are_gore_blood_and_the_input_methods_look() {
    let mut settings = [0u32; 20];
    for (i, s) in settings.iter_mut().enumerate() {
        *s = 0x100 + u32::try_from(i).expect("small");
    }
    let s = StringState {
        settings,
        numerals: "0123456789".into(),
        ..StringState::default()
    };
    assert_eq!(
        (s.top_gore_level(), s.top_blood_color(), s.ime_enabled()),
        (0x100, 0x101, 0x102)
    );
    let ime = s.ime_appearance();
    assert_eq!(ime.symbol_color, 0x103);
    assert_eq!(ime.symbol_height, 0x05, "a byte: the word's low byte");
    assert_eq!(ime.comp_translucence, 0x10);
    assert_eq!(ime.comp_color_text, 0x111);
    assert_eq!(s.other_ime(), 0x112);
    assert!(s.word_wrap_on_space());
    let mut w = Writer::new();
    s.encode(0, &mut w);
    assert_eq!(w.finish().len(), s.pack_size());
}
