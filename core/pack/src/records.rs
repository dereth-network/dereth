//! Rows written as TOML tables, for a game whose rows are records of fields: each pack file holds
//! `[[<kind>]]` arrays of tables, and each row sets one item, named by the game ([`RowKinds`]).
//!
//! A row's fields are its top-level keys, but for two of the grammar's own: `prov` (where it
//! came from, [`crate::provenance`]; a row that says nothing came from where its pack's header
//! says, [`crate::pack::PackHeader::prov`]), and `remove_fields`, the fields it unsets. A sub-table the
//! game marks as a map merges by its own keys, each one field named `<table>.<key>`
//! (`int.ItemType`); arrays and every other value are one field, replaced whole. A `[[remove]]`
//! row takes items away (`items = [...]`, or `item = "..."`).
//!
//! ```toml
//! [[weenie]]
//! class = 509
//! string = { Use = "Use this to set your resurrection point." }
//! remove_fields = ["int.ItemUseable"]
//! prov = { from = "authored", by = "an-author" }
//!
//! [[remove]]
//! items = ["weenie 1234"]
//! ```
//!
//! A layer's rows read this way merge and fold as [`crate::fold`] says, and the folded world comes
//! back as one table for each item ([`tables`]), in the shape its rows are written in, with each
//! field's provenance.

use std::collections::BTreeMap;
use std::path::Path;

use crate::conflict::Item;
use crate::error::{Error, Loc};
use crate::fold::{Entry, Removal, Resolved, Row, Snapshot};
use crate::provenance::Provenance;

/// A game's words for its rows: the item each sets, and which of their sub-tables are maps.
pub trait RowKinds {
    /// The item a row of `kind` sets, read from the row.
    ///
    /// # Errors
    /// A row that names no item, as a sentence.
    fn item(&self, kind: &str, row: &toml::Table) -> Result<Item, String>;

    /// Whether `field` of a row of `kind` is a map, each of whose keys is a field of its own.
    fn is_map(&self, kind: &str, field: &str) -> bool;

    /// An item as a `[[remove]]` writes it, in the one form [`RowKinds::item`] names it.
    ///
    /// # Errors
    /// Text that names no item, as a sentence.
    fn removal(&self, text: &str) -> Result<String, String> {
        Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

/// The kind of each item rows set, by the item's name: later rows' word wins.
pub type Kinds = BTreeMap<String, String>;

/// One pack file's rows, read.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PackFile {
    pub rows: Vec<Row<toml::Value>>,
    pub removals: Vec<Removal>,
    /// The kind of each item its rows set.
    pub kinds: Kinds,
}

/// The lines of each `[[kind]]` header of a file, in order.
fn header_lines(text: &str) -> BTreeMap<String, Vec<usize>> {
    let mut out: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        if let Some((k, _)) = line
            .trim()
            .strip_prefix("[[")
            .and_then(|r| r.split_once("]]"))
        {
            out.entry(k.trim().to_owned()).or_default().push(n + 1);
        }
    }
    out
}

/// The fields of a row of `kind`: its keys, a map's split into its own, without the grammar's.
fn fields<'a>(
    game: &'a dyn RowKinds,
    kind: &'a str,
    row: &'a toml::Table,
) -> impl Iterator<Item = (String, toml::Value)> + 'a {
    row.iter()
        .filter(|(k, _)| !matches!(k.as_str(), "prov" | "remove_fields"))
        .flat_map(move |(k, v)| match v {
            toml::Value::Table(t) if game.is_map(kind, k) => t
                .iter()
                .map(|(sk, sv)| (format!("{k}.{sk}"), sv.clone()))
                .collect::<Vec<_>>(),
            _ => vec![(k.clone(), v.clone())],
        })
}

/// A row's `prov`, read; `default` when it says none.
fn prov_of(row: &toml::Table, at: &Loc, default: &Provenance) -> Result<Provenance, Error> {
    row.get("prov").map_or_else(
        || Ok(default.clone()),
        |p| {
            p.clone()
                .try_into::<Provenance>()
                .map_err(|e| Error::source(at, format!("prov: {}", e.message())))
        },
    )
}

/// A list of names a row gives under `key`.
fn names(row: &toml::Table, key: &str, at: &Loc) -> Result<Vec<String>, Error> {
    match row.get(key) {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Error::source(at, format!("{key}: a list of names")))
            })
            .collect(),
        Some(_) => Err(Error::source(at, format!("{key}: a list of names"))),
    }
}

/// The rows of pack `pack`'s file at `path`, its `text`, read in `game`'s words, and every problem
/// of the file: text that is not TOML, a table that is not of rows, a row that names
/// no item, a `prov` that does not read, a `remove_fields` or `[[remove]]` that is not a list of
/// names. Rows with a problem are left out. A row that says nothing of where it came from was
/// written by a person; [`read_pack_file_from`] gives it the pack's own source instead.
pub fn read_pack_file(
    game: &dyn RowKinds,
    pack: &str,
    path: &Path,
    text: &str,
) -> (PackFile, Vec<Error>) {
    read_pack_file_from(game, pack, path, text, &Provenance::AUTHORED)
}

/// As [`read_pack_file`], a row that says nothing of where it came from given `default`: the
/// pack header's `prov`.
pub fn read_pack_file_from(
    game: &dyn RowKinds,
    pack: &str,
    path: &Path,
    text: &str,
    default: &Provenance,
) -> (PackFile, Vec<Error>) {
    let mut out = PackFile::default();
    let mut problems = Vec::new();
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            problems.push(Error::toml(path, text, &e));
            return (out, problems);
        }
    };
    let lines = header_lines(text);
    for (kind, value) in &table {
        let Some(rows) = value.as_array() else {
            problems.push(Error::source(
                &Loc::new(path, 0),
                format!("{kind} is not a table of rows ([[{kind}]])"),
            ));
            continue;
        };
        for (i, row) in rows.iter().enumerate() {
            let at = Loc::new(
                path,
                lines.get(kind).and_then(|l| l.get(i)).copied().unwrap_or(0),
            );
            let Some(row) = row.as_table() else {
                problems.push(Error::source(
                    &at,
                    format!("a [[{kind}]] row is not a table"),
                ));
                continue;
            };
            let mut read = || -> Result<(), Error> {
                let prov = prov_of(row, &at, default)?;
                if kind == "remove" {
                    let mut items = names(row, "items", &at)?;
                    if let Some(one) = row.get("item") {
                        items.push(
                            one.as_str()
                                .ok_or_else(|| Error::source(&at, "item: a name"))?
                                .to_owned(),
                        );
                    }
                    if let Some(k) = row
                        .keys()
                        .find(|k| !matches!(k.as_str(), "items" | "item" | "prov"))
                    {
                        return Err(Error::source(
                            &at,
                            format!("unknown field `{k}` of a [[remove]]"),
                        ));
                    }
                    for text in items {
                        let item = game.removal(&text).map_err(|why| Error::source(&at, why))?;
                        out.removals.push(Removal {
                            item,
                            pack: pack.to_owned(),
                            at: at.clone(),
                        });
                    }
                    return Ok(());
                }
                let item = game
                    .item(kind, row)
                    .map_err(|why| Error::source(&at, why))?;
                let mut r = Row::new(item, pack, &at, prov);
                r.set = fields(game, kind, row).collect();
                r.unset = names(row, "remove_fields", &at)?.into_iter().collect();
                out.kinds.insert(r.item.name.clone(), kind.clone());
                out.rows.push(r);
                Ok(())
            };
            if let Err(e) = read() {
                problems.push(e);
            }
        }
    }
    (out, problems)
}

/// A whole row of a snapshot, put into `snapshot` and its kind into `kinds`; when it says nothing
/// of where it came from, from `default` (the pack header's `prov`, or a person).
///
/// # Errors
/// A row that names no item or an item the snapshot already has, or a `prov` that does not read.
pub fn snapshot_row(
    game: &dyn RowKinds,
    kind: &str,
    row: &toml::Table,
    at: &Loc,
    default: &Provenance,
    snapshot: &mut Snapshot<toml::Value>,
    kinds: &mut Kinds,
) -> Result<(), Error> {
    let item = game.item(kind, row).map_err(|why| Error::source(at, why))?;
    let prov = prov_of(row, at, default)?;
    let name = item.name.clone();
    snapshot
        .push(item, &prov, fields(game, kind, row))
        .map_err(|why| Error::source(at, why))?;
    kinds.insert(name, kind.to_owned());
    Ok(())
}

/// One item of a folded world as a table: its kind, its fields in the shape rows write them, and
/// each field's provenance by its name (`int.ItemType`).
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub kind: String,
    pub table: toml::Table,
    pub prov: BTreeMap<String, std::sync::Arc<Provenance>>,
}

/// The folded `world` as one table for each item, by its name; its kinds from `kinds` (an item of
/// no kind there is left out).
#[must_use]
pub fn tables(
    game: &dyn RowKinds,
    world: &Resolved<toml::Value>,
    kinds: &Kinds,
) -> BTreeMap<String, Table> {
    let mut out = BTreeMap::new();
    each_table(game, world, kinds, |e, kind, table| {
        let prov = e
            .fields
            .iter()
            .map(|(name, f)| (name.to_string(), f.prov.clone()))
            .collect();
        out.insert(
            e.item.name.clone(),
            Table {
                kind: kind.to_owned(),
                table,
                prov,
            },
        );
    });
    out
}

/// Each item of the folded `world` as its table, in the order of the items' names, one at a
/// time: what [`tables`] holds all at once. An item of no kind in `kinds` is left out.
pub fn each_table(
    game: &dyn RowKinds,
    world: &Resolved<toml::Value>,
    kinds: &Kinds,
    mut each: impl FnMut(&Entry<toml::Value>, &str, toml::Table),
) {
    for e in world.iter_by_name() {
        let Some(kind) = kinds.get(&e.item.name) else {
            continue;
        };
        let mut table = toml::Table::new();
        for (name, f) in &e.fields {
            match name
                .split_once('.')
                .filter(|(head, _)| game.is_map(kind, head))
            {
                Some((head, key)) => {
                    let sub = table
                        .entry(head.to_owned())
                        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
                    if let toml::Value::Table(sub) = sub {
                        sub.insert(key.to_owned(), f.value.clone());
                    }
                }
                None => {
                    table.insert(name.to_string(), f.value.clone());
                }
            }
        }
        each(e, kind, table);
    }
}

/// The item a row sets, from its kind's pattern: the pattern's text, with each `{field}` the row's
/// value of that field. A placeholder may name a list's element (`{cell[0]}`) and say how a number
/// is written: `{f}` in decimal, `{f:04X}` in hexadecimal with four digits at least, `{f:#010X}`
/// with `0x` and eight; and `{f:lower}` is text trimmed and lower-cased. A number the row leaves
/// out is 0, as a field left out is at its default.
///
/// # Errors
/// A pattern that doesn't read, or a row whose field isn't what its placeholder writes.
pub fn item_from(pattern: &str, row: &toml::Table) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..]
            .find('}')
            .map(|c| open + c)
            .ok_or_else(|| format!("{pattern:?}: an open {{"))?;
        let inner = &rest[open + 1..close];
        let (path, spec) = inner.split_once(':').unwrap_or((inner, ""));
        let (field, index) = match path.split_once('[') {
            Some((f, i)) => (
                f,
                Some(
                    i.trim_end_matches(']')
                        .parse::<usize>()
                        .map_err(|_| format!("{pattern:?}: {path} is no list element"))?,
                ),
            ),
            None => (path, None),
        };
        let mut value = row.get(field);
        if let (Some(i), Some(v)) = (index, value) {
            value = v.as_array().and_then(|a| a.get(i));
        }
        if spec == "lower" {
            let text = value
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("no {path}"))?;
            out.push_str(&text.trim().to_lowercase());
        } else {
            let n = match value {
                None => 0,
                Some(v) => v
                    .as_integer()
                    .ok_or_else(|| format!("{path} isn't a number"))?,
            };
            out.push_str(&match spec {
                "" => n.to_string(),
                "04X" => format!("{n:04X}"),
                "#010X" => format!("{n:#010X}"),
                other => return Err(format!("{pattern:?}: {other} is no way to write a number")),
            });
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The item `text` names, written in its one form by its kind's `pattern` ([`item_from`]); `None`
/// when `text` isn't an item of the pattern. Words are separated by any whitespace; a number may be
/// written in decimal or in hexadecimal with `0x`, and one the pattern writes as four hexadecimal
/// digits (`{f:04X}`, a landblock) is hexadecimal with or without it; text the pattern
/// lower-cases (`{f:lower}`) is the rest of `text`, whatever its case.
#[must_use]
pub fn item_in_form(pattern: &str, text: &str) -> Option<String> {
    let mut row = toml::Table::new();
    let mut rest = text.trim();
    let mut pat = pattern;
    // A number's text, read as the placeholder's spec reads it.
    let number = |word: &str, spec: &str| -> Option<i64> {
        let hex = word.strip_prefix("0x").or_else(|| word.strip_prefix("0X"));
        match (hex, spec) {
            (Some(h), _) => i64::from_str_radix(h, 16).ok(),
            (None, "04X") => i64::from_str_radix(word, 16).ok(),
            (None, _) => word.parse().ok(),
        }
    };
    while let Some(open) = pat.find('{') {
        // The literal text before the placeholder, words apart by any whitespace.
        let lit = &pat[..open];
        let words: Vec<&str> = lit.split_whitespace().collect();
        for w in &words {
            rest = rest.trim_start().strip_prefix(w)?;
        }
        if lit.ends_with(char::is_whitespace) {
            rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
        }
        let close = open + pat[open..].find('}')?;
        let inner = &pat[open + 1..close];
        pat = &pat[close + 1..];
        let (path, spec) = inner.split_once(':').unwrap_or((inner, ""));
        if spec == "lower" {
            if !pat.trim().is_empty() {
                return None;
            }
            row.insert(path.to_owned(), toml::Value::String(rest.trim().to_owned()));
            rest = "";
            continue;
        }
        // The number runs to the next literal character, or to a space.
        let stop = pat.chars().next().filter(|c| !c.is_whitespace());
        let end = rest
            .find(|c: char| c.is_whitespace() || Some(c) == stop)
            .unwrap_or(rest.len());
        let n = number(&rest[..end], spec)?;
        rest = &rest[end..];
        match path.split_once('[') {
            Some((f, i)) => {
                let i: usize = i.trim_end_matches(']').parse().ok()?;
                let list = row
                    .entry(f.to_owned())
                    .or_insert_with(|| toml::Value::Array(Vec::new()));
                let a = list.as_array_mut()?;
                if a.len() != i {
                    return None;
                }
                a.push(toml::Value::Integer(n));
            }
            None => {
                row.insert(path.to_owned(), toml::Value::Integer(n));
            }
        }
    }
    for w in pat.split_whitespace() {
        rest = rest.trim_start().strip_prefix(w)?;
    }
    if !rest.trim().is_empty() {
        return None;
    }
    item_from(pattern, &row).ok()
}
