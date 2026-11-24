//! Behaviour: none (tooling: a world repository's rows viewed as the server's SQL and read back)
//!
//! Vectors: `row_sql_pair/`, the server's fixture world's weenies, quests and treasures as its rows
//! and as its SQL, with a schema of the names they use; the server's own import test writes them
//! and keeps them its own.

use std::collections::BTreeMap;
use std::path::Path;

use lyceum_foundry::content::{bare, unquote, RowRef, Schema, SqlDoc};
use lyceum_foundry::row_sql::RowSql;
use serde_json::Value as J;

fn pair_dir() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/cpu/world/row_sql_pair"
    ))
}

/// One file of the pair: its kind, its name, its rows and the server's SQL of them.
struct Item {
    kind: String,
    name: String,
    rows: Vec<toml::Table>,
    sql: SqlDoc,
}

/// The pair's schema, and each of its files.
fn pair() -> (Schema, Vec<Item>) {
    let dir = pair_dir();
    let schema =
        Schema::read(&std::fs::read_to_string(dir.join("schema.json")).expect("the schema"))
            .expect("a schema");
    let mut items = Vec::new();
    for e in std::fs::read_dir(dir).expect("the pair") {
        let p = e.expect("a file").path();
        let file = p.file_name().unwrap().to_string_lossy().into_owned();
        let Some(name) = file.strip_suffix(".toml") else {
            continue;
        };
        let kind = name.rsplit_once('-').expect("<kind>-<key>").0.to_owned();
        let all: toml::Table =
            toml::from_str(&std::fs::read_to_string(&p).expect("rows")).expect("TOML");
        let rows = all[&kind]
            .as_array()
            .expect("rows")
            .iter()
            .map(|r| r.as_table().expect("a row").clone())
            .collect();
        let sql = std::fs::read_to_string(dir.join(format!("{name}.sql"))).expect("SQL");
        items.push(Item {
            kind,
            name: name.to_owned(),
            rows,
            sql: SqlDoc::parse(&sql),
        });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    for kind in ["weenie", "quest", "treasure_death", "treasure_wielded"] {
        assert!(
            items.iter().any(|i| i.kind == kind),
            "the pair has a {kind}"
        );
    }
    (schema, items)
}

/// A text as SQL reads it back from the server's spelling, which doubles a quote and writes a
/// backslash as it is (so SQL reads it, and what follows it, as an escape).
fn as_server_spells(t: &str) -> String {
    unquote(&format!("'{}'", t.replace('\'', "''")))
}

/// A value as written, told apart only by what it is: a number (decimal or hexadecimal, a float
/// written whole as an integer), a yes or no, a text (`respelled` as the server spells it), or
/// nothing.
fn norm(raw: &str, respelled: bool) -> String {
    let b = bare(raw);
    if b.eq_ignore_ascii_case("NULL") {
        return "NULL".into();
    }
    if let Some(h) = b.strip_prefix("0x") {
        if let Ok(n) = i64::from_str_radix(h, 16) {
            return n.to_string();
        }
    }
    if let Ok(n) = b.parse::<i64>() {
        return n.to_string();
    }
    if let Ok(f) = b.parse::<f64>() {
        return format!("{f}");
    }
    match b.as_str() {
        "True" | "False" => b.to_lowercase(),
        _ if respelled => format!("'{}'", as_server_spells(&unquote(&b))),
        _ => format!("'{}'", unquote(&b)),
    }
}

/// Every table the schema writes, with its columns (a split field's each), and whether it is an
/// emote's actions.
fn tables(sql: &J, out: &mut BTreeMap<String, Vec<String>>) {
    if let Some(t) = sql.get("table").and_then(J::as_str) {
        let mut cols: Vec<String> = sql
            .get("columns")
            .and_then(J::as_object)
            .into_iter()
            .flat_map(|m| m.values())
            .flat_map(|c| match c {
                J::Array(a) => a.iter().map(|c| c.as_str().unwrap().to_owned()).collect(),
                c => vec![c.as_str().unwrap().to_owned()],
            })
            .collect();
        for k in ["key", "value"] {
            if let Some(c) = sql.get(k).and_then(J::as_str) {
                cols.push(c.to_owned());
            }
        }
        out.insert(t.to_owned(), cols);
    }
    for part in ["maps", "tables", "lists"] {
        for (_, v) in sql.get(part).and_then(J::as_object).into_iter().flatten() {
            tables(v, out);
        }
    }
}

/// Each of `doc`'s rows of `table` as its columns' values; an emote's actions under their set.
fn rows_of(doc: &SqlDoc, table: &str, cols: &[String], respelled: bool) -> Vec<String> {
    let row = |r: RowRef| {
        cols.iter()
            .map(|c| format!("{c}={}", norm(doc.raw(r, c).unwrap_or("NULL"), respelled)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut out: Vec<String> = if table == lyceum_foundry::content::EMOTE_ACTIONS {
        doc.emote_sets()
            .iter()
            .enumerate()
            .flat_map(|(i, s)| doc.actions(s).into_iter().map(move |r| (i, r)))
            .map(|(i, r)| format!("set {i}: {}", row(r)))
            .collect()
    } else {
        doc.rows(table).into_iter().map(row).collect()
    };
    // The server writes some tables in an order of its own; a row keeps its lists' order.
    out.sort();
    out
}

/// A row with its lists' entries in one order, as the server's SQL may write them in another, and
/// its texts as SQL reads them from the server's spelling ([`as_server_spells`]).
fn sorted(v: &toml::Value) -> toml::Value {
    match v {
        toml::Value::String(s) => toml::Value::String(as_server_spells(s)),
        toml::Value::Table(t) => {
            // Keys in name order, so a table's text is one however its keys were kept.
            let mut keys: Vec<&String> = t.keys().collect();
            keys.sort();
            toml::Value::Table(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&t[k])))
                    .collect(),
            )
        }
        toml::Value::Array(a) => {
            let mut a: Vec<toml::Value> = a.iter().map(sorted).collect();
            a.sort_by_key(ToString::to_string);
            toml::Value::Array(a)
        }
        v => v.clone(),
    }
}

/// Each item's rows (a weenie's, a quest's, a death or wielded treasure's), viewed as SQL, are what
/// the server writes as its SQL, table by table and value by value: every property by its number,
/// every enum by its value, every list entry, every emote's actions and every wielded entry.
#[test]
fn an_items_rows_viewed_as_sql_are_the_servers_own_sql_of_it() {
    let (schema, items) = pair();
    let mut why = Vec::new();
    for item in &items {
        let mut all = BTreeMap::new();
        tables(&schema.rows.as_ref().unwrap()[&item.kind]["sql"], &mut all);
        let view = RowSql::new(&schema, &item.kind, None).expect("a view");
        let mut ours = SqlDoc::default();
        for r in &item.rows {
            view.add_to(&mut ours, r).expect("its SQL");
        }
        for (table, cols) in &all {
            let (a, b) = (
                rows_of(&ours, table, cols, true),
                rows_of(&item.sql, table, cols, false),
            );
            if a != b {
                why.push(format!(
                    "{} {table}:\n  ours   {a:?}\n  theirs {b:?}",
                    item.name
                ));
            }
        }
    }
    assert!(why.is_empty(), "{}", why.join("\n"));
}

/// The server's SQL of each item, read back, is its rows (their lists in any order, as the
/// server's SQL orders some of them).
#[test]
fn the_servers_sql_of_an_item_read_back_is_its_rows() {
    let (schema, items) = pair();
    for item in &items {
        let view = RowSql::new(&schema, &item.kind, None).expect("a view");
        let back = view.rows(&item.sql).expect("its rows");
        let as_value = |rows: &[toml::Table]| {
            sorted(&toml::Value::Array(
                rows.iter().cloned().map(toml::Value::Table).collect(),
            ))
        };
        assert_eq!(as_value(&back), as_value(&item.rows), "{}", item.name);
    }
}

/// An item's rows, viewed as SQL and read back unchanged, are the same rows, in the same order:
/// what a form leaves alone is never written again.
#[test]
fn an_items_rows_viewed_and_read_back_are_the_same_rows() {
    let (schema, items) = pair();
    for item in &items {
        let view = RowSql::new(&schema, &item.kind, None).expect("a view");
        let mut doc = SqlDoc::default();
        for r in &item.rows {
            view.add_to(&mut doc, r).expect("its SQL");
        }
        let back = view.rows(&SqlDoc::parse(&doc.render())).expect("its rows");
        assert_eq!(back, item.rows, "{}:\n{}", item.name, doc.render());
    }
}
