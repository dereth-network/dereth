//! The game's own strings, looked up by token in its string tables: the option labels and the
//! messages a disconnection shows.
//!
//! A token's row is the hash of the token's text, in the table a table enum names through the
//! game's data-id mapper (group 4).

use std::collections::BTreeMap;

use dereth_assets::Decode as _;
use dereth_primitives::AssetSource as _;

/// The data-id mapper group the string tables are named in.
const STRING_TABLE_GROUP: u32 = 4;

/// The tables read so far, by table enum.
#[derive(Debug, Default)]
pub struct Strings {
    tables: BTreeMap<u32, Option<dereth_assets::ui::StringTable>>,
    /// The tables read by their own data id.
    direct: BTreeMap<u32, Option<dereth_assets::ui::StringTable>>,
}

impl Strings {
    /// The string for `token` in the table enum `table` names, or `None` when there is none.
    pub fn lookup(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        table: u32,
        token: &str,
    ) -> Option<String> {
        let loaded = self.tables.entry(table).or_insert_with(|| {
            dereth_client_runtime::assets::enum_did(store, STRING_TABLE_GROUP, table).and_then(
                |id| {
                    let bytes = store.read(id).ok()?;
                    dereth_assets::ui::StringTable::decode_payload(id, &bytes).ok()
                },
            )
        });
        let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
        loaded
            .as_ref()?
            .strings
            .iter()
            .find(|(k, _)| *k == hash)
            .and_then(|(_, e)| e.strings.first().cloned())
            .map(dereth_assets::escape::unescape)
            .filter(|s| !s.is_empty())
    }

    /// The string for `token` in the string table `table` (a data id), with its named variables
    /// filled from `values`, as the game fills a message. `None` when the row or a variable is
    /// missing.
    pub fn fill(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        table: u32,
        token: &str,
        values: &[(&str, &str)],
    ) -> Option<String> {
        let loaded = self
            .direct
            .entry(table)
            .or_insert_with(|| {
                let id = dereth_primitives::DataId(table);
                let bytes = store.read(id).ok()?;
                dereth_assets::ui::StringTable::decode_payload(id, &bytes).ok()
            })
            .as_ref()?;
        let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
        let entry = &loaded.strings.iter().find(|(k, _)| *k == hash)?.1;
        let mut vals = Vec::with_capacity(entry.variables.len());
        for id in &entry.variables {
            let hit = values
                .iter()
                .find(|(name, _)| dereth_primitives::num::hash::str_hash(name.as_bytes()) == *id)?;
            vals.push(hit.1.to_owned());
        }
        Some(dereth_assets::escape::unescape(
            dereth_ui::text::metalanguage::render(&entry.strings, &vals),
        ))
    }

    /// As [`Self::lookup`], falling back to the token's own words.
    pub fn text(&mut self, store: &dereth_dat::RetailDatStore, table: u32, token: &str) -> String {
        self.lookup(store, table, token)
            .unwrap_or_else(|| token.trim_start_matches("ID_").replace('_', " "))
    }
}
