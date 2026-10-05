//! String-table composition without a UI: a row's pieces and the caller's values rendered the way
//! the client renders them, over any [`StringResolver`], and the resolver that reads the rows out
//! of the dats.
//!
//! Interfaces supply their resolver to the named or positional render; no widget or
//! application state is required.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::{AssetSource, DataId};

use {crate::metalanguage, crate::StringResolver, dereth_assets::escape::unescape};

/// One `StringInfo` rendered with its values matched **by name**: the row's own variable list is
/// walked and each id looked up among `values` by the string hash of its name, then the pieces and
/// values go through the metalanguage render and the unescape.
///
/// `None` when the row is missing or names a variable `values` does not carry (the client's
/// missing-variable arm, which renders nothing). When the resolver cannot name the row's
/// variables, this falls back to [`render_positional`] with the values in the order given.
#[must_use]
pub fn render_named(
    strings: &dyn StringResolver,
    table: DataId,
    string_id: u32,
    values: &[(&str, &str)],
) -> Option<String> {
    let pieces = strings.resolve_variants_raw(table, string_id)?;
    let Some(ids) = strings.resolve_variables(table, string_id) else {
        let owned: Vec<String> = values.iter().map(|(_, v)| (*v).to_owned()).collect();
        return render_positional(strings, table, string_id, &owned);
    };
    let mut vals: Vec<String> = Vec::with_capacity(ids.len());
    for id in &ids {
        // Each variable is matched by its name hash; a missing one fails the resolve.
        let hit = values
            .iter()
            .find(|(name, _)| dereth_primitives::num::hash::str_hash(name.as_bytes()) == *id)?;
        vals.push(hit.1.to_owned());
    }
    Some(unescape(metalanguage::render(&pieces, &vals)))
}

/// [`render_named`] for a row named by its token rather than its id.
#[must_use]
pub fn render_token(
    strings: &dyn StringResolver,
    table: DataId,
    token: &str,
    values: &[(&str, &str)],
) -> Option<String> {
    let id = dereth_primitives::num::hash::str_hash(token.as_bytes());
    render_named(strings, table, id, values)
}

/// One `StringInfo` rendered with its values **by position**: the positional shim for a resolver
/// that cannot name a row's variables, or a row with exactly one variable.
#[must_use]
pub fn render_positional(
    strings: &dyn StringResolver,
    table: DataId,
    string_id: u32,
    values: &[String],
) -> Option<String> {
    let pieces = strings.resolve_variants_raw(table, string_id)?;
    // String-table lookup builds `values` from the row's **own** variable list, so there
    // is always exactly one fewer value than fragment. A caller that offers more is offering
    // a variable the row does not have, and retail's tokeniser would never see it.
    //
    // **A caller that offers fewer is not offering "no sentinel".**
    // The table lookup fills the array from the row's variables and writes the **empty string** for a
    // variable the caller did not name, which the tokenizer still wraps in its
    // two `U+0001`s. That empty value is what earns the auto-derived `'b'` flag, and `'b'` is
    // what `ID_DurationFormat`'s `{ [!b]}` separators test — so padding is not tidiness, it
    // is the difference between *"5 days"* and *"5 days   "*.
    let n = pieces.len().saturating_sub(1);
    let mut vals: Vec<String> = values.iter().take(n).cloned().collect();
    vals.resize(n, String::new());
    Some(unescape(metalanguage::render(&pieces, &vals)))
}

/// One decoded `StringTable`: string id -> the row's fragments **and** its own variable list.
///
/// The variable list is not decoration: substitution matches the caller's values against it by
/// hashed id, so dropping it would force every caller to hard-code the order it believes the row
/// uses.
type StringRows = BTreeMap<u32, (Vec<String>, Vec<u32>)>;

/// `StringTable` lookup in `client_local_<Language>.dat`, memoised per table, over any asset
/// source.
///
/// A string-table entry holds a list of variants; a label shows variant 0, the singular/default
/// form. The rows come back **as the dat stores them**: unescaping is [`StringResolver`]'s job,
/// not this resolver's, so every resolver agrees about what a label says.
pub struct DatStringResolver<S: ?Sized> {
    store: Arc<S>,
    tables: RefCell<BTreeMap<DataId, Option<StringRows>>>,
}

impl<S: ?Sized> std::fmt::Debug for DatStringResolver<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatStringResolver")
            .field("tables", &self.tables.borrow().len())
            .finish_non_exhaustive()
    }
}

impl<S: AssetSource + ?Sized> DatStringResolver<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            tables: RefCell::new(BTreeMap::new()),
        }
    }

    fn load(&self, table: DataId) {
        if self.tables.borrow().contains_key(&table) {
            return;
        }
        use dereth_assets::Decode;
        let loaded = self.store.read(table).ok().and_then(|b| {
            dereth_assets::ui::StringTable::decode_payload(table, &b)
                .ok()
                .map(|t| {
                    t.strings
                        .into_iter()
                        .map(|(k, v)| (k, (v.strings, v.variables)))
                        .collect::<StringRows>()
                })
        });
        if loaded.is_none() {
            tracing::warn!("string table {table:?} would not load");
        }
        self.tables.borrow_mut().insert(table, loaded);
    }
}

impl<S: AssetSource + ?Sized> StringResolver for DatStringResolver<S> {
    fn resolve_raw(&self, table: DataId, string_id: u32) -> Option<String> {
        self.load(table);
        let t = self.tables.borrow();
        t.get(&table)?.as_ref()?.get(&string_id)?.0.first().cloned()
    }

    fn resolve_variants_raw(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.load(table);
        let t = self.tables.borrow();
        t.get(&table)?
            .as_ref()?
            .get(&string_id)
            .map(|r| r.0.clone())
    }

    /// Variable substitutions straight off the row.
    fn resolve_variables(&self, table: DataId, string_id: u32) -> Option<Vec<u32>> {
        self.load(table);
        let t = self.tables.borrow();
        t.get(&table)?
            .as_ref()?
            .get(&string_id)
            .map(|r| r.1.clone())
    }
}
