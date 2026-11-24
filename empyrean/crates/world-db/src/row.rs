//! A row as written: its fields, its maps (a weenie's properties by name) and its lists (a weenie's
//! create list, an emote's actions), and the TOML text of a row at a path.

use toml::{Table, Value};

use crate::value::{key, R, V};

/// One row: scalar fields first, then each map as a table of its own, then each list as an array of
/// tables, in the order they were added.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Row {
    pub fields: Vec<(String, V)>,
    pub maps: Vec<(String, Vec<(String, V)>)>,
    /// Tables of their own, each a row.
    pub tables: Vec<(String, Row)>,
    pub lists: Vec<(String, Vec<Row>)>,
}

impl Row {
    /// Set `name` to `v`, when there's a value.
    pub fn set(&mut self, name: &str, v: Option<V>) {
        if let Some(v) = v {
            self.fields
                .push((name.trim_start_matches("r#").to_owned(), v));
        }
    }

    /// A map of `entries`, left out when empty.
    pub fn map(&mut self, name: &str, entries: Vec<(String, V)>) {
        if !entries.is_empty() {
            self.maps.push((name.to_owned(), entries));
        }
    }

    /// A list of `rows`, left out when empty.
    pub fn list(&mut self, name: &str, rows: Vec<Self>) {
        if !rows.is_empty() {
            self.lists.push((name.to_owned(), rows));
        }
    }

    /// The row's TOML at `path` (`weenie`, `weenie.emote`), as an element of an array of tables.
    pub fn emit(&self, path: &str, out: &mut String) {
        out.push_str(&format!("[[{path}]]\n"));
        self.body(path, out);
    }

    /// The row's TOML at `path` as a table of its own.
    fn emit_table(&self, path: &str, out: &mut String) {
        out.push_str(&format!("[{path}]\n"));
        self.body(path, out);
    }

    fn body(&self, path: &str, out: &mut String) {
        for (k, v) in &self.fields {
            out.push_str(&format!("{} = {}\n", key(k), v.text()));
        }
        for (name, entries) in &self.maps {
            out.push_str(&format!("\n[{path}.{}]\n", key(name)));
            for (k, v) in entries {
                out.push_str(&format!("{} = {}\n", key(k), v.text()));
            }
        }
        for (name, row) in &self.tables {
            out.push('\n');
            row.emit_table(&format!("{path}.{}", key(name)), out);
        }
        for (name, rows) in &self.lists {
            for r in rows {
                out.push('\n');
                r.emit(&format!("{path}.{}", key(name)), out);
            }
        }
    }
}

impl Row {
    /// The row as the table its text reads back as, made without the text.
    #[must_use]
    pub fn table(&self) -> Table {
        let mut t = Table::new();
        for (k, v) in &self.fields {
            t.insert(k.clone(), v.value());
        }
        for (name, entries) in &self.maps {
            let m: Table = entries
                .iter()
                .map(|(k, v)| (k.clone(), v.value()))
                .collect();
            t.insert(name.clone(), Value::Table(m));
        }
        for (name, row) in &self.tables {
            t.insert(name.clone(), Value::Table(row.table()));
        }
        for (name, rows) in &self.lists {
            t.insert(
                name.clone(),
                Value::Array(rows.iter().map(|r| Value::Table(r.table())).collect()),
            );
        }
        t
    }
}

/// A file of `rows` at `path`, each a `[[path]]` element.
#[must_use]
pub fn file(path: &str, rows: &[Row]) -> String {
    let mut out = String::new();
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        r.emit(path, &mut out);
    }
    out
}

/// The table at `name` in `t`, or an empty one.
#[must_use]
pub fn sub<'a>(t: &'a Table, name: &str) -> Option<&'a Table> {
    t.get(name).and_then(Value::as_table)
}

/// The tables of the list `name` in `t`, or none.
pub fn rows_of<'a>(t: &'a Table, name: &str) -> R<Vec<&'a Table>> {
    match t.get(name) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_table()
                    .ok_or_else(|| format!("{name} holds something not a table"))
            })
            .collect(),
        Some(_) => Err(format!("{name} isn't a list of tables")),
    }
}

/// The rows of kind `kind` in a file's text.
pub fn read_kind(text: &str, kind: &str) -> R<Vec<Table>> {
    let t: Table = toml::from_str(text).map_err(|e| e.to_string())?;
    match t.get(kind) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_table()
                    .cloned()
                    .ok_or_else(|| format!("a {kind} isn't a table"))
            })
            .collect(),
        Some(_) => Err(format!("{kind} isn't a list of rows")),
    }
}
