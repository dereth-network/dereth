//! The shipped taboo table through the mode-1 matcher censors a taboo token and not an ordinary
//! one.
//! Fixture: shipped formula tables, spell data and taboo patterns from the retail DATs.

use dereth_assets::{Decode, TabooTable};
use dereth_client_model::taboo::{censors_chat_token, filter_chat_line};
use dereth_primitives::DataId;
use dereth_protocol::cp1252::Cp1252;

const TABOO_TABLE: DataId = DataId(0x0E00_001E);

fn retail_table() -> TabooTable {
    let store = dereth_dat::testing::open_store_or_fail();
    let bytes = store
        .read_portal(TABOO_TABLE)
        .expect("the shipped taboo table is readable");
    TabooTable::decode_payload(TABOO_TABLE, &bytes).expect("the shipped taboo table decodes")
}

/// Behaviour: chat.taboo.the-shipped-table-censors-a-taboo-word-and-passes-an-ordinary-one
/// The word is an oracle from audience 1 of `client_portal.dat`, not a production word list.
#[test]
fn shipped_audience_one_drives_the_chat_replacement() {
    let table = retail_table();
    let repeated_stars: Vec<&str> = table
        .audiences
        .iter()
        .flat_map(|(_, buckets)| buckets)
        .flat_map(|(_, patterns)| patterns)
        .map(String::as_str)
        .filter(|pattern| pattern.contains("**"))
        .collect();
    assert!(
        repeated_stars.is_empty(),
        "the shipped matcher takes its unusual repeated-star branch for {repeated_stars:?}"
    );
    assert!(censors_chat_token(&table, &Cp1252, "shit"));
    assert!(!censors_chat_token(&table, &Cp1252, "hello"));
    assert_eq!(
        filter_chat_line(&table, &Cp1252, "hello shit there"),
        "hello **** there"
    );
}
