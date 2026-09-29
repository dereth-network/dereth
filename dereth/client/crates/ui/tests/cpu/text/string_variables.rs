//! A string-table row decides where each named value goes, not the caller.
//!
//! A value enters a `StringInfo` under a **name**, filed in the info's own variable table keyed by
//! a number derived from that name (`"KEY"`, `"ACTION"`, `"BINDINGS"`, ...). Resolving a string
//! hands that table to the string table's lookup, which walks the **row's** own list of variable
//! ids and asks the table for each in turn, marking the string missing when a value is absent. So
//! the caller's order is invisible and the row's order is everything. The shipped
//! `client_local_English.dat` has the pair that proves it:
//! `ID_ActionKeyMap_OverwriteExistingBinding` lists `KEY, ACTION` and `ID_ActionKeyMap_Binding`
//! lists `ACTION, KEY`, and a localised dat may reorder either one without touching the client.
//!
//! Fixture: a resolver written here, no dat and no socket: two rows whose variable lists are each
//! other's reverse, and a caller that names its two values once. Every assertion fails on a
//! positional API, which has one value order and can only be right about one of the two rows.

use std::collections::BTreeMap;
use std::rc::Rc;

use dereth_primitives::DataId;
use dereth_ui::UiSystem;

fn hash(name: &str) -> u32 {
    dereth_primitives::num::hash::str_hash(name.as_bytes())
}

/// `client_local_English.dat`'s `0x23000004`, as far as this file is concerned.
const EN: DataId = DataId(0x2300_0004);
/// The same table in a hypothetical localisation whose translator moved the variables about.
/// Nothing in the client changes between the two; only the rows' variable lists do.
const LOC: DataId = DataId(0x2300_0104);

/// A `StringTable` that keeps each row's own variable list.
#[derive(Debug, Default)]
struct Rows {
    rows: BTreeMap<(u32, u32), (Vec<String>, Vec<u32>)>,
    /// A resolver that answers `None` to [`dereth_ui::text::StringResolver::resolve_variables`].
    anonymous: bool,
}

impl Rows {
    fn add(&mut self, table: DataId, token: &str, frags: &[&str], vars: &[&str]) {
        self.rows.insert(
            (table.0, hash(token)),
            (
                frags.iter().map(|s| (*s).to_owned()).collect(),
                vars.iter().map(|v| hash(v)).collect(),
            ),
        );
    }
}

impl dereth_ui::text::StringResolver for Rows {
    fn resolve_raw(&self, table: DataId, string_id: u32) -> Option<String> {
        self.rows.get(&(table.0, string_id))?.0.first().cloned()
    }

    fn resolve_variants_raw(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.rows.get(&(table.0, string_id)).map(|r| r.0.clone())
    }

    fn resolve_variables(&self, table: DataId, string_id: u32) -> Option<Vec<u32>> {
        if self.anonymous {
            return None;
        }
        self.rows.get(&(table.0, string_id)).map(|r| r.1.clone())
    }
}

/// The two shipped key-binding rows, plus a localisation that reverses one of them and a row that
/// names the same variable twice (`ID_CharacterInfo_Resists` really is `RESIST, RESIST, REGEN`).
fn env(anonymous: bool) -> UiSystem {
    let mut rows = Rows {
        anonymous,
        ..Rows::default()
    };
    // Measured off `client_local_English.dat` 0x23000004.
    rows.add(
        EN,
        "ID_ActionKeyMap_OverwriteExistingBinding",
        &[
            "'",
            "' is currently bound to '",
            "'. Do you wish to erase that binding?",
        ],
        &["KEY", "ACTION"],
    );
    rows.add(
        EN,
        "ID_ActionKeyMap_Binding",
        &["'", "' ('", "')"],
        &["ACTION", "KEY"],
    );
    rows.add(
        EN,
        "ID_ActionKeyMap_NonUserBindableBinding",
        &["'", "' is not bindable."],
        &["KEY"],
    );
    // The same row id, the same fragments, the **other** variable order: what a translator who
    // rewrote the sentence leaves behind. The client is not rebuilt for this.
    rows.add(
        LOC,
        "ID_ActionKeyMap_Binding",
        &["'", "' ('", "')"],
        &["KEY", "ACTION"],
    );
    rows.add(
        EN,
        "ID_CharacterInfo_Resists",
        &["", " / ", " (", ")"],
        &["RESIST", "RESIST", "REGEN"],
    );

    let mut ui = UiSystem::new((800, 600));
    ui.strings = Some(Rc::new(rows));
    ui
}

/// Behaviour: ui.strings.a-string-row-decides-where-each-named-value-goes
///
/// **One named supply, two rows that order those names oppositely, both right.**
///
/// The positional API cannot pass this: `&[key, action]` renders `ID_ActionKeyMap_Binding` as
/// *"'F7' ('Turn Left')"* and `&[action, key]` renders the overwrite prompt as
/// *"'Turn Left' is currently bound to 'F7'"*. There is no single order that is right about both,
/// which is exactly why retail does not use one.
#[test]
fn one_named_supply_serves_two_rows_that_order_their_variables_oppositely() {
    let ui = env(false);
    let values = [("ACTION", "Turn Left"), ("KEY", "F7")];

    let one = ui
        .resolve_string_named(
            EN,
            hash("ID_ActionKeyMap_OverwriteExistingBinding"),
            &values,
        )
        .expect("the row resolves");
    assert_eq!(
        one, "'F7' is currently bound to 'Turn Left'. Do you wish to erase that binding?",
        "the row lists KEY, ACTION -- the retail resolver reads the row's variables, not the call site's"
    );

    let line = ui
        .resolve_string_named(EN, hash("ID_ActionKeyMap_Binding"), &values)
        .expect("the row resolves");
    assert_eq!(
        line, "'Turn Left' ('F7')",
        "the same two values; this row lists ACTION, KEY"
    );
}

/// Behaviour: ui.strings.a-string-row-decides-where-each-named-value-goes
///
/// A localised dat that reorders a row renders correctly **without a client change** — the whole
/// point of matching by hashed id.
#[test]
fn a_localisation_that_reverses_a_row_still_renders_it_correctly() {
    let ui = env(false);
    let values = [("ACTION", "Turn Left"), ("KEY", "F7")];
    let id = hash("ID_ActionKeyMap_Binding");

    assert_eq!(
        ui.resolve_string_named(EN, id, &values)
            .expect("the English row"),
        "'Turn Left' ('F7')"
    );
    assert_eq!(
        ui.resolve_string_named(LOC, id, &values)
            .expect("the localised row"),
        "'F7' ('Turn Left')",
        "the same call site, the same values: only the row's variable list moved"
    );

    // And the positional entry point, given the order the English row uses, is wrong about the
    // localised one, which is why the lookup goes by name.
    let positional = [String::from("Turn Left"), String::from("F7")];
    assert_eq!(
        ui.resolve_string_rendered(LOC, id, &positional)
            .expect("the localised row"),
        "'Turn Left' ('F7')",
        "positional substitution ignores the row's variable list and prints the values swapped"
    );
}

/// A row may name the same variable twice — `ID_CharacterInfo_Resists` ships as
/// `RESIST, RESIST, REGEN`. `find` answers the same value both times.
#[test]
fn a_row_that_names_one_variable_twice_gets_that_value_twice() {
    let ui = env(false);
    let out = ui
        .resolve_string_named(
            EN,
            hash("ID_CharacterInfo_Resists"),
            &[("RESIST", "Above Average"), ("REGEN", "Good")],
        )
        .expect("the row resolves");
    assert_eq!(out, "Above Average / Above Average (Good)");
}

/// When a row names a variable the caller did not supply, its variable lookup fails; the
/// metalanguage arm of the lookup returns the missing-variable status **before** any substitution,
/// and the caller hands back the empty string it started with. `None` here.
///
/// A value the row does not name is simply never looked up — an extra entry in the client's hash
/// table is not an error either.
#[test]
fn a_variable_the_row_names_and_the_caller_did_not_renders_nothing() {
    let ui = env(false);
    let id = hash("ID_ActionKeyMap_OverwriteExistingBinding");
    assert_eq!(
        ui.resolve_string_named(EN, id, &[("KEY", "F7")]),
        None,
        "ACTION is missing: the retail resolver returns the missing-variable marker and renders nothing"
    );
    assert_eq!(
        ui.resolve_string_named(
            EN,
            hash("ID_ActionKeyMap_NonUserBindableBinding"),
            &[
                ("KEY", "F7"),
                ("ACTION", "never asked for"),
                ("BINDINGS", "nor this")
            ],
        )
        .as_deref(),
        Some("'F7' is not bindable."),
        "the row names KEY alone; the other two are never looked up"
    );
}

/// A resolver that cannot name a row's variables keeps the positional order the caller supplied,
/// so the shim is a fall-back and not a
/// second behaviour for a resolver that *can* answer.
#[test]
fn a_resolver_that_cannot_name_its_variables_falls_back_to_the_supply_order() {
    let ui = env(true);
    assert_eq!(
        ui.resolve_string_variables(EN, hash("ID_ActionKeyMap_Binding")),
        None
    );
    assert_eq!(
        ui.resolve_string_named(
            EN,
            hash("ID_ActionKeyMap_Binding"),
            &[("ACTION", "Turn Left"), ("KEY", "F7")],
        )
        .as_deref(),
        Some("'Turn Left' ('F7')"),
        "no variable list to consult: the pairs are taken in the order they were written"
    );
}
