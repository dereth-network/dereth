//! A row's SQL view: an item of a world repository (a weenie) written as the server's SQL, the
//! document the forms edit ([`SqlDoc`]), and that document read back as the item's row.
//!
//! The tables and columns come from the server's schema, as its `rows` member describes the kind
//! (its `sql`), and the names of properties and enum values from its `properties` and `enums`;
//! nothing of them is held here. A row leaves a field out at its default, as the server writes
//! rows: written as SQL it is `NULL` when the schema calls it nullable and its default (0, false,
//! empty) otherwise, and read back a value at its default is left out again. A property's value,
//! an enum's name and a time are always written, but a time at the world's stamp, which a row
//! leaves out.
//!
//! The view keeps a row's order: its lists' entries, and their fields, are written as the row has
//! them, so a row viewed and read back unchanged is the same row.

use serde_json::Value as J;
use toml::Value as T;

use crate::content::{bare, quote, unquote, Schema, SqlDoc};

/// How a field a row leaves out is written, and when one read back is left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Absent {
    /// Always written: a property's value, an enum's name.
    Never,
    /// Left out at its default (0, false, empty).
    Default,
    /// Left out unset (`NULL`); any value, its default too, is written.
    Null,
}

/// The view of one kind's rows as SQL, with the server's names.
#[derive(Clone, Copy, Debug)]
pub struct RowSql<'a> {
    names: &'a Schema,
    described: &'a J,
    sql: &'a J,
    stamp: Option<&'a str>,
}

/// A row's fields: `(field, column)` by the `columns` of `sql`.
fn columns(sql: &J) -> Vec<(&str, &J)> {
    sql.get("columns")
        .and_then(J::as_object)
        .map(|m| m.iter().map(|(f, c)| (f.as_str(), c)).collect())
        .unwrap_or_default()
}

fn nullable(sql: &J, field: &str) -> bool {
    sql.get("nullable")
        .and_then(J::as_array)
        .is_some_and(|n| n.iter().any(|x| x == field))
}

/// How field `field` of an entry described as `sql` is left out, of type `ty`.
fn absent(sql: &J, field: &str, ty: &J) -> Absent {
    if ty.get("name").is_some() {
        Absent::Never
    } else if nullable(sql, field) {
        Absent::Null
    } else {
        Absent::Default
    }
}

fn s<'j>(v: &'j J, key: &str) -> Result<&'j str, String> {
    v.get(key)
        .and_then(J::as_str)
        .ok_or_else(|| format!("the schema's SQL names no {key}"))
}

/// An integer as written: decimal, `0x` hexadecimal, or either quoted.
fn int_of(text: &str) -> Option<i64> {
    let t = unquote(text);
    let t = t.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => t.parse::<i64>().ok(),
    }
}

impl<'a> RowSql<'a> {
    /// The view of rows of `kind`, with `names` (the server's schema, its `rows` among them) and
    /// the world's `stamp` (a time a row leaves out).
    ///
    /// # Errors
    /// A schema with no rows, or none of `kind`, or none written as SQL.
    pub fn new(names: &'a Schema, kind: &str, stamp: Option<&'a str>) -> Result<Self, String> {
        let described = names
            .rows
            .as_ref()
            .and_then(|r| r.get(kind))
            .ok_or_else(|| format!("the server's schema has no rows of {kind}"))?;
        let sql = described
            .get("sql")
            .ok_or_else(|| format!("the server's schema doesn't write {kind} as SQL"))?;
        Ok(Self {
            names,
            described,
            sql,
            stamp,
        })
    }

    /// How field `f` of an entry described as `sql` is left out: never, for a field of the kind's
    /// key, which every row names.
    fn absent_of(&self, sql: &J, f: &str, ty: &J) -> Absent {
        let key = std::ptr::eq(sql, self.sql)
            && self.described["key"]
                .as_array()
                .is_some_and(|k| k.iter().any(|x| x == f));
        if key {
            Absent::Never
        } else {
            absent(sql, f, ty)
        }
    }

    // ---- values -------------------------------------------------------------------------------

    /// `v` (or its absence) of type `ty` as SQL.
    fn put(&self, ty: &J, v: Option<&T>, absent: Absent) -> Result<String, String> {
        let kind = ty.as_str();
        let Some(v) = v else {
            return Ok(match (absent, kind) {
                (Absent::Null, _) => "NULL".into(),
                (_, Some("bool")) => "False".into(),
                (_, Some("text")) => "''".into(),
                (_, Some("time")) => self.stamp.map_or_else(|| "NULL".into(), quote),
                _ => "0".into(),
            });
        };
        let wrong = || format!("{v} isn't {ty}");
        if let Some(e) = ty.get("name").and_then(J::as_str) {
            return match v {
                T::Integer(n) => Ok(n.to_string()),
                T::String(name) => self
                    .names
                    .enums
                    .get(e)
                    .and_then(|m| m.iter().find(|(_, n)| *n == name))
                    .map(|(k, _)| k.to_string())
                    .ok_or_else(|| format!("{name:?} isn't a {e}")),
                _ => Err(wrong()),
            };
        }
        match (kind, v) {
            (Some("int"), T::Integer(n)) => Ok(n.to_string()),
            (Some("hex"), T::Integer(n)) => Ok(match u32::try_from(*n) {
                Ok(u) => format!("0x{u:08X}"),
                Err(_) => n.to_string(),
            }),
            (Some("float"), T::Float(f)) => Ok(format!("{f}")),
            (Some("float"), T::Integer(n)) => Ok(n.to_string()),
            (Some("bool"), T::Boolean(b)) => Ok(if *b { "True" } else { "False" }.into()),
            (Some("text" | "time"), T::String(t)) => Ok(quote(t)),
            (Some("time"), T::Integer(n)) => Ok(n.to_string()),
            _ => Err(wrong()),
        }
    }

    /// The SQL value `raw` of type `ty` read back; `None` when a row leaves it out.
    fn take(&self, ty: &J, raw: &str, absent: Absent) -> Result<Option<T>, String> {
        let b = bare(raw);
        if b.eq_ignore_ascii_case("NULL") {
            return Ok(None);
        }
        let wrong = || format!("{b} isn't {ty}");
        let dflt = absent == Absent::Default;
        if let Some(e) = ty.get("name").and_then(J::as_str) {
            let n = int_of(&b).ok_or_else(wrong)?;
            let name = u32::try_from(n)
                .ok()
                .and_then(|k| self.names.enums.get(e)?.get(&k));
            return Ok(Some(
                name.map_or(T::Integer(n), |name| T::String(name.clone())),
            ));
        }
        Ok(match ty.as_str() {
            Some("int" | "hex") => {
                let n = int_of(&b).ok_or_else(wrong)?;
                (!(dflt && n == 0)).then_some(T::Integer(n))
            }
            Some("float") => {
                let f = unquote(&b).trim().parse::<f64>().map_err(|_| wrong())?;
                (!(dflt && f == 0.0)).then_some(T::Float(f))
            }
            Some("bool") => {
                let v = match unquote(&b).to_ascii_lowercase().as_str() {
                    "true" | "1" | "\u{1}" => true,
                    "false" | "0" | "\u{0}" => false,
                    _ => return Err(wrong()),
                };
                (!(dflt && !v)).then_some(T::Boolean(v))
            }
            Some("text") => {
                let t = unquote(&b);
                (!(dflt && t.is_empty())).then_some(T::String(t))
            }
            Some("time") => {
                let t = unquote(&b);
                (self.stamp != Some(t.as_str())).then_some(match t.parse::<i64>() {
                    Ok(n) => T::Integer(n),
                    Err(_) => T::String(t),
                })
            }
            _ => return Err(wrong()),
        })
    }

    /// A map's key `key` (as a row writes it) as SQL, keyed as `ty` says.
    fn put_key(&self, ty: &J, key: &str) -> Result<String, String> {
        let number = || key.parse::<i64>().map(|n| n.to_string());
        let named = |m: Option<&std::collections::BTreeMap<u32, String>>| {
            m.and_then(|m| m.iter().find(|(_, n)| *n == key))
                .map(|(k, _)| k.to_string())
                .or_else(|| number().ok())
                .ok_or_else(|| format!("{key:?} isn't a {ty}"))
        };
        if let Some(p) = ty.get("property").and_then(J::as_str) {
            return named(self.names.properties.get(p));
        }
        if let Some(e) = ty.get("name").and_then(J::as_str) {
            return named(self.names.enums.get(e));
        }
        match ty.as_str() {
            Some("text") => Ok(quote(key)),
            _ => number().map_err(|_| format!("{key:?} isn't a number")),
        }
    }

    /// A map's key read back from SQL, as a row writes it.
    fn take_key(&self, ty: &J, raw: &str) -> Result<String, String> {
        let b = bare(raw);
        let named = |m: Option<&std::collections::BTreeMap<u32, String>>| {
            let n = int_of(&b).ok_or_else(|| format!("{b} isn't a key"))?;
            Ok(u32::try_from(n)
                .ok()
                .and_then(|k| m?.get(&k).cloned())
                .unwrap_or_else(|| n.to_string()))
        };
        if let Some(p) = ty.get("property").and_then(J::as_str) {
            return named(self.names.properties.get(p));
        }
        if let Some(e) = ty.get("name").and_then(J::as_str) {
            return named(self.names.enums.get(e));
        }
        match ty.as_str() {
            Some("text") => Ok(unquote(&b)),
            _ => int_of(&b)
                .map(|n| n.to_string())
                .ok_or_else(|| format!("{b} isn't a number")),
        }
    }

    // ---- entries ------------------------------------------------------------------------------

    /// An entry's fields (`fields`, as described) as its columns' SQL values, by `sql`.
    fn put_entry(
        &self,
        fields: &J,
        sql: &J,
        row: &toml::Table,
    ) -> Result<Vec<(String, String)>, String> {
        let mut out = Vec::new();
        for (f, col) in columns(sql) {
            let ty = fields
                .get(f)
                .ok_or_else(|| format!("{f} isn't described"))?;
            let how = self.absent_of(sql, f, ty);
            match col {
                J::String(c) => out.push((c.clone(), self.put(ty, row.get(f), how)?)),
                J::Array(cs) => {
                    let each = ty.get("list").unwrap_or(&J::Null);
                    let items = row.get(f).and_then(T::as_array);
                    for (i, c) in cs.iter().enumerate() {
                        let c = c.as_str().ok_or("a column isn't named")?;
                        let v = items.and_then(|a| a.get(i));
                        let how = if items.is_none() && how == Absent::Null {
                            Absent::Null
                        } else {
                            Absent::Default
                        };
                        out.push((c.to_owned(), self.put(each, v, how)?));
                    }
                }
                _ => return Err(format!("{f}: a column isn't named")),
            }
        }
        Ok(out)
    }

    /// An entry read back from row `r` of `doc`: its fields, as described, by `sql`.
    fn take_entry(
        &self,
        fields: &J,
        sql: &J,
        doc: &SqlDoc,
        r: crate::content::RowRef,
    ) -> Result<toml::Table, String> {
        let mut out = toml::Table::new();
        for (f, col) in columns(sql) {
            let ty = fields
                .get(f)
                .ok_or_else(|| format!("{f} isn't described"))?;
            let how = self.absent_of(sql, f, ty);
            match col {
                J::String(c) => {
                    let raw = doc.raw(r, c).unwrap_or("NULL");
                    if let Some(v) = self.take(ty, raw, how)? {
                        out.insert(f.to_owned(), v);
                    }
                }
                J::Array(cs) => {
                    let each = ty.get("list").unwrap_or(&J::Null);
                    let mut items = Vec::new();
                    let mut any = false;
                    for c in cs {
                        let raw = doc.raw(r, c.as_str().unwrap_or_default()).unwrap_or("NULL");
                        any |= !bare(raw).eq_ignore_ascii_case("NULL");
                        let v = self.take(each, raw, Absent::Never)?;
                        items.push(v.unwrap_or(T::Integer(0)));
                    }
                    if any || how != Absent::Null {
                        out.insert(f.to_owned(), T::Array(items));
                    }
                }
                _ => return Err(format!("{f}: a column isn't named")),
            }
        }
        Ok(out)
    }

    // ---- the whole row ------------------------------------------------------------------------

    /// Row `row` of the kind as SQL.
    ///
    /// # Errors
    /// A field or key the schema doesn't describe, or a value not of its type.
    pub fn to_sql(&self, row: &toml::Table) -> Result<SqlDoc, String> {
        let mut doc = SqlDoc::default();
        self.add_to(&mut doc, row)?;
        Ok(doc)
    }

    /// The list a kind is written as the rows of (a wielded treasure's entries), when it is.
    fn group(&self) -> Option<&str> {
        self.sql.get("group").and_then(J::as_str)
    }

    /// Whether a document holds one item of the kind across tables (a weenie), rather than one
    /// for each row (a quest) or each group of rows (a wielded treasure).
    fn whole(&self) -> bool {
        self.group().is_none()
            && ["maps", "tables", "lists"]
                .iter()
                .any(|p| self.sql.get(*p).is_some())
    }

    /// Row `row` of the kind written into `doc`, after what it holds.
    ///
    /// # Errors
    /// As [`Self::to_sql`].
    pub fn add_to(&self, doc: &mut SqlDoc, row: &toml::Table) -> Result<(), String> {
        let own = self.put_entry(&self.described["fields"], self.sql, row)?;
        if let Some(g) = self.group() {
            let l = &self.sql["lists"][g];
            let d = &self.described["lists"][g]["fields"];
            for e in row.get(g).and_then(T::as_array).into_iter().flatten() {
                let e = e.as_table().ok_or_else(|| format!("a {g} isn't a table"))?;
                let mut values = own.clone();
                values.extend(self.put_entry(d, l, e)?);
                add(doc, s(self.sql, "table")?, None, &values, None);
            }
            return Ok(());
        }
        add(doc, s(self.sql, "table")?, None, &own, None);
        let maps = self.sql.get("maps").and_then(J::as_object);
        for (name, m) in maps.into_iter().flatten() {
            let Some(entries) = row.get(name).and_then(T::as_table) else {
                continue;
            };
            let d = &self.described["maps"][name];
            let table = s(m, "table")?;
            let key_col = s(m, "key")?;
            // In the order of their keys' numbers, as the server writes them.
            let mut keyed = entries
                .iter()
                .map(|(k, v)| Ok((self.put_key(&d["key"], k)?, k, v)))
                .collect::<Result<Vec<_>, String>>()?;
            keyed.sort_by_key(|(raw, _, _)| int_of(raw));
            for (raw, k, v) in keyed {
                let mut values = vec![(key_col.to_owned(), raw)];
                if let Some(vc) = m.get("value").and_then(J::as_str) {
                    values.push((
                        vc.to_owned(),
                        self.put(&d["value"], Some(v), Absent::Never)?,
                    ));
                } else {
                    let t = v
                        .as_table()
                        .ok_or_else(|| format!("{name}.{k} isn't a table"))?;
                    values.extend(self.put_entry(&d["value"]["fields"], m, t)?);
                }
                let named = d["key"].get("property").is_some() || d["key"].get("name").is_some();
                add(
                    doc,
                    table,
                    m.get("parent"),
                    &values,
                    named.then(|| k.clone()),
                );
            }
        }
        let tables = self.sql.get("tables").and_then(J::as_object);
        for (name, t) in tables.into_iter().flatten() {
            if let Some(v) = row.get(name).and_then(T::as_table) {
                let values = self.put_entry(&self.described["tables"][name], t, v)?;
                add(doc, s(t, "table")?, t.get("parent"), &values, None);
            }
        }
        let lists = self.sql.get("lists").and_then(J::as_object);
        for (name, l) in lists.into_iter().flatten() {
            let Some(entries) = row.get(name).and_then(T::as_array) else {
                continue;
            };
            let d = &self.described["lists"][name];
            for e in entries {
                let e = e
                    .as_table()
                    .ok_or_else(|| format!("a {name} isn't a table"))?;
                let values = self.put_entry(&d["fields"], l, e)?;
                let Some(subs) = l.get("lists").and_then(J::as_object) else {
                    add(doc, s(l, "table")?, l.get("parent"), &values, None);
                    continue;
                };
                // A list of lists is an emote's: its set, then its actions.
                let pairs: Vec<(&str, String)> = values
                    .iter()
                    .map(|(c, v)| (c.as_str(), v.clone()))
                    .collect();
                let category = pairs
                    .iter()
                    .find(|(c, _)| *c == "category")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                let set = doc.add_emote_set(&category, &pairs);
                for (sub, sl) in subs {
                    let actions = e.get(sub).and_then(T::as_array);
                    let mut all = Vec::new();
                    for (i, a) in actions.into_iter().flatten().enumerate() {
                        let a = a
                            .as_table()
                            .ok_or_else(|| format!("a {sub} isn't a table"))?;
                        let values = self.put_entry(&d["lists"][sub]["fields"], sl, a)?;
                        let pairs: Vec<(&str, String)> = values
                            .iter()
                            .map(|(c, v)| (c.as_str(), v.clone()))
                            .collect();
                        doc.add_action(set, i, &pairs);
                        all.push(values);
                    }
                    // Numbered again as each was added: each its own number back.
                    let es = doc.emote_sets();
                    let rows = es.get(set).map(|e| doc.actions(e)).unwrap_or_default();
                    for (r, values) in rows.into_iter().zip(&all) {
                        for (c, v) in values {
                            doc.set_cell(r, c, v);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Document `doc` read back as the kind's row: its first item ([`Self::rows`]).
    ///
    /// # Errors
    /// A document with none, or as [`Self::rows`].
    pub fn to_row(&self, doc: &SqlDoc) -> Result<toml::Table, String> {
        self.rows(doc)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("no {} row", self.sql["table"]))
    }

    /// Every item of the kind document `doc` holds, as its rows, in the order written.
    ///
    /// # Errors
    /// A value not of its field's type, or a key not of its map's.
    pub fn rows(&self, doc: &SqlDoc) -> Result<Vec<toml::Table>, String> {
        let own_table = s(self.sql, "table")?;
        let fields = &self.described["fields"];
        if let Some(g) = self.group() {
            // The rows of one key are an item; its entries are those rows.
            let l = &self.sql["lists"][g];
            let d = &self.described["lists"][g]["fields"];
            let mut out: Vec<toml::Table> = Vec::new();
            for r in doc.rows(own_table) {
                let own = self.take_entry(fields, self.sql, doc, r)?;
                let entry = T::Table(self.take_entry(d, l, doc, r)?);
                let at = out.iter_mut().find(|o| {
                    let mut k = (*o).clone();
                    k.remove(g);
                    k == own
                });
                match at {
                    Some(o) => {
                        if let Some(T::Array(a)) = o.get_mut(g) {
                            a.push(entry);
                        }
                    }
                    None => {
                        let mut o = own;
                        o.insert(g.to_owned(), T::Array(vec![entry]));
                        out.push(o);
                    }
                }
            }
            return Ok(out);
        }
        if !self.whole() {
            return doc
                .rows(own_table)
                .into_iter()
                .map(|r| self.take_entry(fields, self.sql, doc, r))
                .collect();
        }
        let Some(own) = doc.rows(own_table).first().copied() else {
            return Ok(Vec::new());
        };
        let mut row = self.take_entry(fields, self.sql, doc, own)?;
        let maps = self.sql.get("maps").and_then(J::as_object);
        for (name, m) in maps.into_iter().flatten() {
            let d = &self.described["maps"][name];
            let key_col = s(m, "key")?;
            let mut out = toml::Table::new();
            for r in doc.rows(s(m, "table")?) {
                let key = self.take_key(&d["key"], doc.raw(r, key_col).unwrap_or("NULL"))?;
                let v = if let Some(vc) = m.get("value").and_then(J::as_str) {
                    self.take(&d["value"], doc.raw(r, vc).unwrap_or("NULL"), Absent::Never)?
                } else {
                    Some(T::Table(self.take_entry(
                        &d["value"]["fields"],
                        m,
                        doc,
                        r,
                    )?))
                };
                if let Some(v) = v {
                    out.insert(key, v);
                }
            }
            if !out.is_empty() {
                row.insert(name.clone(), T::Table(out));
            }
        }
        let tables = self.sql.get("tables").and_then(J::as_object);
        for (name, t) in tables.into_iter().flatten() {
            if let Some(r) = doc.rows(s(t, "table")?).first() {
                let v = self.take_entry(&self.described["tables"][name], t, doc, *r)?;
                if !v.is_empty() {
                    row.insert(name.clone(), T::Table(v));
                }
            }
        }
        let lists = self.sql.get("lists").and_then(J::as_object);
        for (name, l) in lists.into_iter().flatten() {
            let d = &self.described["lists"][name];
            let mut out = Vec::new();
            if let Some(subs) = l.get("lists").and_then(J::as_object) {
                for set in doc.emote_sets() {
                    let mut e = self.take_entry(&d["fields"], l, doc, set.emote)?;
                    for (sub, sl) in subs {
                        let actions = doc
                            .actions(&set)
                            .into_iter()
                            .map(|r| {
                                self.take_entry(&d["lists"][sub]["fields"], sl, doc, r)
                                    .map(T::Table)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        if !actions.is_empty() {
                            e.insert(sub.clone(), T::Array(actions));
                        }
                    }
                    out.push(T::Table(e));
                }
            } else {
                for r in doc.rows(s(l, "table")?) {
                    out.push(T::Table(self.take_entry(&d["fields"], l, doc, r)?));
                }
            }
            if !out.is_empty() {
                row.insert(name.clone(), T::Array(out));
            }
        }
        Ok(vec![row])
    }
}

/// Add a row of `values` to `doc`'s `table`, linked by its `parent` column (filled in by the
/// document), with `comment`.
fn add(
    doc: &mut SqlDoc,
    table: &str,
    parent: Option<&J>,
    values: &[(String, String)],
    comment: Option<String>,
) {
    let mut cols: Vec<&str> = parent.and_then(J::as_str).into_iter().collect();
    cols.extend(values.iter().map(|(c, _)| c.as_str()));
    let pairs: Vec<(&str, String)> = values
        .iter()
        .map(|(c, v)| (c.as_str(), v.clone()))
        .collect();
    doc.add_row(table, &cols, &pairs, None, comment);
}
