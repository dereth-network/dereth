//! Reading VTank settings profiles (`.usd` files), for a VTank-class plugin's import.
//!
//! A profile is the meta files' typed tables (see [`crate::metfile`]), several in a row: a count,
//! then each table's name and the table (read from the public profiles players share; no
//! program's code was consulted):
//!
//! ```text
//! 8                     tables
//! AntiExtraBuffSpells   a table's name
//! 1                     its columns, names, flags, rows and cells, as a meta's
//! …
//! Settings              the options: Setting, Value
//! …
//! ```
//!
//! The known tables are `AntiExtraBuffSpells`, `AssistItems`, `BuffedItems`, `ExtraBuffSpells`,
//! `GemFoodItems`, `ItemUseSpecifiers`, `MyMonsters` and `Settings`; any table is read. A table
//! comes back as a list of rows, each a map of its columns.

use dereth_plugin_api::Value;

use crate::metfile::Reader;

/// A profile's text, read: `{ <table name> = { row, … }, … }`.
///
/// # Errors
/// Not a profile, or a value that does not read as its type.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut r = Reader::new(text);
    let count = r.int("the table count")?;
    if !(1..=64).contains(&count) {
        return Err(format!("{count} tables is not a settings profile"));
    }
    let mut out = Value::map();
    for _ in 0..count {
        let name = r.line()?.trim().to_owned();
        if name.is_empty() {
            return Err("a table with no name".into());
        }
        let table = r.table().map_err(|e| format!("{name}: {e}"))?;
        out = out.with(&name, table);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the file reader; the import is pinned by the Tank's
    //! scenario).
    use super::parse;

    #[test]
    fn a_profile_reads_each_table_by_name_and_its_rows_by_column() {
        let text = [
            "2",
            "AssistItems",
            "2",
            "Object",
            "Type",
            "n",
            "n",
            "1",
            "s",
            "Handy Healing Kit",
            "i",
            "0",
            "Settings",
            "2",
            "Setting",
            "Value",
            "n",
            "n",
            "2",
            "s",
            "AttackDistance",
            "d",
            "0.0208333",
            "s",
            "EnableNav",
            "b",
            "True",
            "",
        ]
        .join("\r\n");
        let v = parse(&text).expect("reads");
        let settings = v.get("Settings").as_list().expect("rows");
        assert_eq!(settings[0].get("Setting").as_str(), Some("AttackDistance"));
        assert!((settings[0].get("Value").as_f64().unwrap_or(0.0) - 0.020_833_3).abs() < 1e-9);
        assert_eq!(settings[1].get("Value").as_bool(), Some(true));
        let items = v.get("AssistItems").as_list().expect("rows");
        assert_eq!(items[0].get("Object").as_str(), Some("Handy Healing Kit"));
        assert!(parse("9999\r\n").is_err());
    }
}
