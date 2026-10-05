//! Behaviour: none (prints escaped rows in the shipped string tables)
//! Fixture: the retail string tables.

use dereth_primitives::AssetSource;

/// **Ignored census.** Every `StringTable` in `client_local_English.dat`, counted by how many of
/// its rows carry an escape and which escapes they are. Run with
/// `--ignored --nocapture` when the number is wanted; it is a measurement of the exposure, not a
/// guard, and it is deliberately not an assertion so that a dat change cannot break the suite.
#[test]
#[ignore = "instrument: prints escaped rows from the shipped string tables"]
fn print_shipped_string_table_escapes() {
    use dereth_assets::Decode;
    use std::collections::BTreeMap;
    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dats open");
    let mut tables = 0u32;
    let mut rows = 0u32;
    let mut escaped_rows = 0u32;
    let mut by_escape: BTreeMap<char, u32> = BTreeMap::new();
    let mut stray = 0u32;
    for did in store.ids_of(dereth_dat::DbType::StringTable) {
        let Ok(bytes) = store.read(did) else { continue };
        let Ok(t) = dereth_assets::ui::StringTable::decode_payload(did, &bytes) else {
            continue;
        };
        tables += 1;
        for (_, v) in t.strings {
            for piece in v.strings {
                rows += 1;
                if !piece.contains('\\') {
                    continue;
                }
                escaped_rows += 1;
                let mut it = piece.chars();
                while let Some(c) = it.next() {
                    if c != '\\' {
                        continue;
                    }
                    let mut peek = it.clone();
                    match peek.next().and_then(dereth_assets::escape::un_escaped_char) {
                        Some(u) => {
                            *by_escape.entry(u).or_default() += 1;
                            it = peek;
                        }
                        None => stray += 1,
                    }
                }
            }
        }
    }
    println!("{tables} string tables, {rows} rows, {escaped_rows} carrying an escape");
    println!("escapes by produced character: {by_escape:?}");
    println!("backslashes that are not an escape: {stray}");
}
