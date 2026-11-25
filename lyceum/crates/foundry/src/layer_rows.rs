//! An item's row in the layer being edited: what the layer writes so that the world at that layer
//! holds the item as wanted, over the world below it.
//!
//! The fold sees a row as its fields ([`fields`]): each key of a map or a table a field of its own
//! (`int.Level`), every other value whole. A layer's row of an item names the item's key, sets the
//! fields whose value differs from the world below, and unsets (`remove_fields`) those below that
//! the item no longer has ([`delta`]); a field set back to the value below is no longer the
//! layer's. Read back, the layer's row over the world below is the item ([`over`]).
//!
//! The row is written into its pack file as TOML ([`write_row`]): the other rows, the row's own
//! lines that stay the same and every comment are kept, and a field the server writes in
//! hexadecimal is written so. A layer takes an item away with a `[[remove]]` row
//! ([`write_removal`]).

use std::collections::BTreeMap;

use serde_json::Value as J;
use toml::Value as T;

use dereth_pack::records::{item_from, RowKinds};

use crate::repo::RowSchema;

/// The grammar's own fields of a row, which are no field of the item.
const OWN: [&str; 2] = ["prov", "remove_fields"];

/// Row `row`'s fields as the fold sees them: a map's or a table's keys each a field
/// (`int.Level`), the rest whole; but `prov` and `remove_fields`.
#[must_use]
pub fn fields(schema: &RowSchema, kind: &str, row: &toml::Table) -> BTreeMap<String, T> {
    let mut out = BTreeMap::new();
    for (k, v) in row {
        if OWN.contains(&k.as_str()) {
            continue;
        }
        match v {
            T::Table(t) if schema.is_map(kind, k) => {
                for (key, v) in t {
                    out.insert(format!("{k}.{key}"), v.clone());
                }
            }
            v => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Fields gathered back into a row: `int.Level` is `Level` of `int`.
#[must_use]
pub fn row_of(schema: &RowSchema, kind: &str, fields: &BTreeMap<String, T>) -> toml::Table {
    let mut row = toml::Table::new();
    for (name, v) in fields {
        match name.split_once('.') {
            Some((map, key)) if schema.is_map(kind, map) => {
                if let T::Table(t) = row
                    .entry(map)
                    .or_insert_with(|| T::Table(toml::Table::new()))
                {
                    t.insert(key.to_owned(), v.clone());
                }
            }
            _ => {
                row.insert(name.clone(), v.clone());
            }
        }
    }
    row
}

/// The key fields of `kind`, which name its item.
fn keys(schema: &RowSchema, kind: &str) -> Vec<String> {
    schema
        .kinds
        .get(kind)
        .and_then(|k| k.described.get("key"))
        .and_then(J::as_array)
        .map(|a| a.iter().filter_map(J::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// The row a layer writes for the world at it to hold `wanted`, over `below` (the item in the
/// world below; `None`, an item new in the layer): the item's key, the fields that differ, and in
/// `remove_fields` those below that `wanted` lacks. `None` when it would change nothing.
#[must_use]
pub fn delta(
    schema: &RowSchema,
    kind: &str,
    below: Option<&toml::Table>,
    wanted: &toml::Table,
) -> Option<toml::Table> {
    let want = fields(schema, kind, wanted);
    let was = below.map(|b| fields(schema, kind, b)).unwrap_or_default();
    let keys = keys(schema, kind);
    let mut set: BTreeMap<String, T> = want
        .iter()
        .filter(|(k, v)| was.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let unset: Vec<T> = was
        .keys()
        .filter(|k| !want.contains_key(*k))
        .map(|k| T::String(k.clone()))
        .collect();
    if set.is_empty() && unset.is_empty() && below.is_some() {
        return None;
    }
    for k in &keys {
        if let Some(v) = want.get(k) {
            set.insert(k.clone(), v.clone());
        }
    }
    // The key first, as a row is written.
    let mut row = toml::Table::new();
    let body = row_of(schema, kind, &set);
    for k in &keys {
        if let Some(v) = body.get(k) {
            row.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in body {
        row.entry(k).or_insert(v);
    }
    if !unset.is_empty() {
        row.insert("remove_fields".into(), T::Array(unset));
    }
    Some(row)
}

/// The item as the world holds it with a layer's row `layer` over `below`: the fields it sets
/// replace those below, and those it unsets are gone.
#[must_use]
pub fn over(
    schema: &RowSchema,
    kind: &str,
    below: Option<&toml::Table>,
    layer: &toml::Table,
) -> toml::Table {
    let mut f = below.map(|b| fields(schema, kind, b)).unwrap_or_default();
    if let Some(T::Array(gone)) = layer.get("remove_fields") {
        for g in gone.iter().filter_map(T::as_str) {
            f.remove(g);
        }
    }
    f.extend(fields(schema, kind, layer));
    row_of(schema, kind, &f)
}

/// The rows of `kind` in pack file `text` and the place of the one setting `item`.
fn find(
    schema: &RowSchema,
    kind: &str,
    text: &str,
    item: &str,
) -> Result<(toml::Table, Option<usize>), String> {
    let all: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let at = all.get(kind).and_then(T::as_array).and_then(|rows| {
        rows.iter().position(|r| {
            r.as_table()
                .and_then(|r| schema.item(kind, r).ok())
                .is_some_and(|i| i.name == item)
        })
    });
    Ok((all, at))
}

/// Item `item`'s `[[kind]]` row in pack file `text`, when it has one.
///
/// # Errors
/// Text that isn't TOML.
pub fn row_in(
    schema: &RowSchema,
    kind: &str,
    text: &str,
    item: &str,
) -> Result<Option<toml::Table>, String> {
    let (all, at) = find(schema, kind, text, item)?;
    Ok(at.and_then(|i| all[kind].as_array()?.get(i)?.as_table().cloned()))
}

/// Pack file `text` (`None`, no file yet) with item `item`'s `[[kind]]` row made `row`, or taken
/// away (`None`): the other rows, the lines of the row that stay the same, and every comment kept.
/// `None` when the file is left with nothing in it.
///
/// # Errors
/// Text that isn't TOML.
pub fn write_row(
    schema: &RowSchema,
    kind: &str,
    text: Option<&str>,
    item: &str,
    row: Option<&toml::Table>,
) -> Result<Option<String>, String> {
    let text = text.unwrap_or_default();
    let (_, at) = find(schema, kind, text, item)?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    let described = schema
        .kinds
        .get(kind)
        .map(|k| &k.described)
        .unwrap_or(&J::Null);
    let mut empty = false;
    match (at, row) {
        (Some(i), Some(row)) => {
            if let Some(t) = doc[kind]
                .as_array_of_tables_mut()
                .and_then(|a| a.get_mut(i))
            {
                update_row(t, row, described);
            }
        }
        (None, Some(row)) => {
            if doc.get(kind).is_none() {
                doc[kind] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
            }
            let a = doc[kind]
                .as_array_of_tables_mut()
                .ok_or_else(|| format!("{kind} isn't a list of rows"))?;
            a.push(new_row(row, described));
        }
        (Some(i), None) => {
            if let Some(a) = doc[kind].as_array_of_tables_mut() {
                a.remove(i);
                empty = a.is_empty();
            }
        }
        (None, None) => {}
    }
    if empty {
        doc.remove(kind);
    }
    let out = doc.to_string();
    Ok((!out.trim().is_empty()).then_some(out))
}

/// Pack file `text` with item `item` taken away by its `[[remove]]` rows (`gone`), or no longer
/// (`!gone`). `None` when the file is left with nothing in it.
///
/// # Errors
/// Text that isn't TOML.
pub fn write_removal(text: Option<&str>, item: &str, gone: bool) -> Result<Option<String>, String> {
    let mut doc: toml_edit::DocumentMut = text
        .unwrap_or_default()
        .parse()
        .map_err(|e| format!("{e}"))?;
    let names = |t: &toml_edit::Table| -> Vec<String> {
        let mut out: Vec<String> = t
            .get("items")
            .and_then(toml_edit::Item::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        out.extend(
            t.get("item")
                .and_then(toml_edit::Item::as_str)
                .map(str::to_owned),
        );
        out
    };
    let held = doc
        .get("remove")
        .and_then(toml_edit::Item::as_array_of_tables)
        .is_some_and(|a| a.iter().any(|t| names(t).iter().any(|n| n == item)));
    if gone && !held {
        if doc.get("remove").is_none() {
            doc["remove"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }
        let mut t = toml_edit::Table::new();
        t["item"] = toml_edit::value(item);
        doc["remove"]
            .as_array_of_tables_mut()
            .ok_or("remove isn't a list of rows")?
            .push(t);
    } else if !gone && held {
        if let Some(a) = doc["remove"].as_array_of_tables_mut() {
            for t in a.iter_mut() {
                if t.get("item").and_then(toml_edit::Item::as_str) == Some(item) {
                    t.remove("item");
                }
                if let Some(items) = t.get_mut("items").and_then(toml_edit::Item::as_array_mut) {
                    items.retain(|v| v.as_str() != Some(item));
                }
            }
            a.retain(|t| {
                t.get("item").is_some()
                    || t.get("items")
                        .and_then(toml_edit::Item::as_array)
                        .is_some_and(|i| !i.is_empty())
            });
            if a.is_empty() {
                doc.remove("remove");
            }
        }
    }
    let out = doc.to_string();
    Ok((!out.trim().is_empty()).then_some(out))
}

// ---- writing TOML ------------------------------------------------------------------------------

/// A value written as TOML, a number the server writes in hexadecimal (`ty` is `"hex"`) in it; of
/// a type that is one of several (`either`), as the one of its shape.
fn value_of(v: &T, ty: Option<&J>) -> toml_edit::Value {
    if let Some(alts) = ty.and_then(|t| t.get("either")).and_then(J::as_array) {
        let shape = alts.iter().find(|a| match v {
            T::Table(_) => a.get("fields").is_some(),
            T::Array(_) => a.get("list").is_some(),
            _ => a.is_string(),
        });
        return value_of(v, shape);
    }
    match v {
        T::Integer(n) if ty.and_then(J::as_str) == Some("hex") && *n >= 0 => {
            let text = match u32::try_from(*n) {
                Ok(u) => format!("0x{u:08X}"),
                Err(_) => format!("0x{n:X}"),
            };
            text.parse().unwrap_or_else(|_| toml_edit::Value::from(*n))
        }
        T::Integer(n) => toml_edit::Value::from(*n),
        T::Float(f) => toml_edit::Value::from(*f),
        T::Boolean(b) => toml_edit::Value::from(*b),
        T::String(s) => toml_edit::Value::from(s.as_str()),
        T::Datetime(d) => d
            .to_string()
            .parse()
            .unwrap_or_else(|_| toml_edit::Value::from(d.to_string())),
        T::Array(a) => {
            let each = ty.and_then(|t| t.get("list"));
            toml_edit::Value::Array(a.iter().map(|v| value_of(v, each)).collect())
        }
        T::Table(t) => {
            let fields = ty.and_then(|t| t.get("fields"));
            let mut it = toml_edit::InlineTable::new();
            for (k, v) in t {
                it.insert(k, value_of(v, fields.and_then(|f| f.get(k))));
            }
            toml_edit::Value::InlineTable(it)
        }
    }
}

/// What a row's field `k` is, as its kind (or a list's entry) is `described`: a field's type, a
/// map's, a table's or a list's description.
enum Part<'a> {
    Field(Option<&'a J>),
    Map(&'a J),
    Table(&'a J),
    List(&'a J),
}

fn part<'a>(described: &'a J, k: &str) -> Part<'a> {
    if let Some(m) = described.get("maps").and_then(|m| m.get(k)) {
        Part::Map(m)
    } else if let Some(t) = described.get("tables").and_then(|t| t.get(k)) {
        Part::Table(t)
    } else if let Some(l) = described.get("lists").and_then(|l| l.get(k)) {
        Part::List(l)
    } else {
        Part::Field(described.get("fields").and_then(|f| f.get(k)))
    }
}

/// A field `k` of a row (or a list's entry) as written, as its kind is `described`.
fn item_of(described: &J, k: &str, v: &T) -> toml_edit::Item {
    match (part(described, k), v) {
        (Part::Map(m), T::Table(t)) => {
            let mut out = toml_edit::Table::new();
            for (key, v) in t {
                out.insert(key, toml_edit::Item::Value(value_of(v, m.get("value"))));
            }
            toml_edit::Item::Table(out)
        }
        (Part::Table(fields), T::Table(t)) => {
            let mut out = toml_edit::Table::new();
            for (key, v) in t {
                out.insert(key, toml_edit::Item::Value(value_of(v, fields.get(key))));
            }
            toml_edit::Item::Table(out)
        }
        (Part::List(l), T::Array(a)) if a.iter().all(T::is_table) => {
            let mut out = toml_edit::ArrayOfTables::new();
            for e in a.iter().filter_map(T::as_table) {
                out.push(new_row(e, l));
            }
            toml_edit::Item::ArrayOfTables(out)
        }
        (Part::Field(ty), v) => toml_edit::Item::Value(value_of(v, ty)),
        (_, v) => toml_edit::Item::Value(value_of(v, None)),
    }
}

/// A new row (or a list's entry) written from `row`, as its kind is `described`.
fn new_row(row: &toml::Table, described: &J) -> toml_edit::Table {
    let mut t = toml_edit::Table::new();
    for (k, v) in row {
        t.insert(k, item_of(described, k, v));
    }
    t
}

/// What a written item holds, read back as a value.
fn read_back(item: &toml_edit::Item) -> Option<T> {
    let text = match item {
        toml_edit::Item::Value(v) => format!("v = {v}"),
        toml_edit::Item::Table(t) => {
            let mut d = toml_edit::DocumentMut::new();
            d["v"] = toml_edit::Item::Table(t.clone());
            d.to_string()
        }
        toml_edit::Item::ArrayOfTables(a) => {
            let mut d = toml_edit::DocumentMut::new();
            d["v"] = toml_edit::Item::ArrayOfTables(a.clone());
            d.to_string()
        }
        toml_edit::Item::None => return None,
    };
    toml::from_str::<toml::Table>(&text).ok()?.remove("v")
}

/// Written row `t` made `row`: a field that stays the same is left as written, a map's or a
/// table's keys each so, and its `prov` kept.
fn update_row(t: &mut toml_edit::Table, row: &toml::Table, described: &J) {
    let gone: Vec<String> = t
        .iter()
        .map(|(k, _)| k.to_owned())
        .filter(|k| k != "prov" && !row.contains_key(k))
        .collect();
    for k in gone {
        t.remove(&k);
    }
    for (k, v) in row {
        if t.get(k).and_then(read_back).as_ref() == Some(v) {
            continue;
        }
        let keyed = matches!(part(described, k), Part::Map(_) | Part::Table(_));
        match (t.get_mut(k), v) {
            (Some(toml_edit::Item::Table(old)), T::Table(new)) if keyed => {
                let ty = match part(described, k) {
                    Part::Map(m) => m.get("value"),
                    _ => None,
                };
                let gone: Vec<String> = old
                    .iter()
                    .map(|(k, _)| k.to_owned())
                    .filter(|k| !new.contains_key(k))
                    .collect();
                for g in gone {
                    old.remove(&g);
                }
                for (key, v) in new {
                    if old.get(key).and_then(read_back).as_ref() == Some(v) {
                        continue;
                    }
                    let vt = match part(described, k) {
                        Part::Table(f) => f.get(key),
                        _ => ty,
                    };
                    old.insert(key, toml_edit::Item::Value(value_of(v, vt)));
                }
            }
            _ => {
                t.insert(k, item_of(described, k, v));
            }
        }
    }
}

/// The file a new row of `kind` goes in, by its kind's file rule.
///
/// # Errors
/// A kind the schema doesn't know, or a row its rule can't name.
pub fn file_of(schema: &RowSchema, kind: &str, row: &toml::Table) -> Result<String, String> {
    let k = schema
        .kinds
        .get(kind)
        .ok_or_else(|| format!("no kind of row is called {kind}"))?;
    item_from(&k.file, row)
}
