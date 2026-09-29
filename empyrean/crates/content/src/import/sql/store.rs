//! The tables: rows kept as their dump text in one arena per table, lazily built hash indexes,
//! and the statement semantics (see the [module](super) documentation).
//!
//! Not ACE-derived.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::parse::{
    AlterOp, ColumnDef, Cond, CreateTable, Expr, KeyDef, RefAction, SetTarget, Stmt,
};
use super::value::{compare, index_part, like, read, store, IndexPart, Kind, Val};
use crate::error::ImportError;
use crate::import::mysqldump::{parse_tuple, Column as DumpColumn, DumpSink, Row, TableDef, Value};

/// One column of a table.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub kind: Kind,
    pub not_null: bool,
    pub default: Option<Expr>,
    pub auto_increment: bool,
    pub on_update_now: bool,
    pub generated: Option<Expr>,
}

impl Column {
    fn from_def(d: &ColumnDef) -> Result<Self, String> {
        Ok(Self {
            name: d.name.clone(),
            kind: Kind::of(d)?,
            not_null: d.not_null,
            default: d.default.clone(),
            auto_increment: d.auto_increment,
            on_update_now: d.on_update_now,
            generated: d.generated.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct ForeignKey {
    columns: Vec<usize>,
    parent: String,
    parent_columns: Vec<String>,
    on_delete: RefAction,
}

#[derive(Debug, Clone, Copy)]
struct RowRef {
    off: usize,
    len: u32,
    live: bool,
}

/// One table.
#[derive(Debug)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    /// Column indices in stored order (every column but the generated ones).
    stored: Vec<usize>,
    /// Column index to its position in a stored tuple.
    position: Vec<Option<usize>>,
    by_name: HashMap<String, usize>,
    primary: Option<Vec<usize>>,
    uniques: Vec<(String, Vec<usize>)>,
    foreign: Vec<ForeignKey>,
    auto_column: Option<usize>,
    /// The `AUTO_INCREMENT=` table option.
    auto_option: Option<u64>,
    /// The next generated value, once known.
    auto_next: Option<u64>,
    arena: Vec<u8>,
    rows: Vec<RowRef>,
    live: usize,
    indexes: HashMap<Vec<usize>, HashMap<u64, Vec<u32>>>,
    /// Rows were added out of primary-key order.
    needs_sort: bool,
    /// The `CREATE TABLE` text, handed on with the rows.
    pub create_sql: String,
    /// Dropped by a `DROP TABLE`: kept only so earlier events can still be read.
    dropped: bool,
}

impl Table {
    /// A table from its `CREATE TABLE`.
    pub fn new(c: &CreateTable, create_sql: String) -> Result<Self, String> {
        let columns: Vec<Column> = c
            .columns
            .iter()
            .map(Column::from_def)
            .collect::<Result<_, _>>()?;
        let by_name: HashMap<String, usize> = columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.to_ascii_lowercase(), i))
            .collect();
        let stored: Vec<usize> = (0..columns.len())
            .filter(|&i| columns[i].generated.is_none())
            .collect();
        let mut position = vec![None; columns.len()];
        for (p, &i) in stored.iter().enumerate() {
            position[i] = Some(p);
        }
        let auto_column = columns.iter().position(|c| c.auto_increment);
        let mut t = Self {
            name: c.name.clone(),
            columns,
            stored,
            position,
            by_name,
            primary: None,
            uniques: Vec::new(),
            foreign: Vec::new(),
            auto_column,
            auto_option: c.auto_increment,
            auto_next: None,
            arena: Vec::new(),
            rows: Vec::new(),
            live: 0,
            indexes: HashMap::new(),
            needs_sort: false,
            create_sql,
            dropped: false,
        };
        for k in &c.keys {
            t.add_key(k)?;
        }
        Ok(t)
    }

    fn col(&self, name: &str) -> Result<usize, String> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .copied()
            .ok_or_else(|| format!("unknown column `{name}` in `{}`", self.name))
    }

    fn cols(&self, names: &[String]) -> Result<Vec<usize>, String> {
        names.iter().map(|n| self.col(n)).collect()
    }

    fn add_key(&mut self, k: &KeyDef) -> Result<(), String> {
        match k {
            KeyDef::Primary(c) => {
                self.primary = Some(self.cols(c)?);
                self.needs_sort = true;
            }
            KeyDef::Unique(n, c) => {
                let c = self.cols(c)?;
                self.uniques.push((n.clone(), c));
            }
            KeyDef::Index(_, c) => {
                self.cols(c)?;
            }
            KeyDef::Foreign {
                columns,
                table,
                ref_columns,
                on_delete,
                ..
            } => {
                let columns = self.cols(columns)?;
                self.foreign.push(ForeignKey {
                    columns,
                    parent: table.clone(),
                    parent_columns: ref_columns.clone(),
                    on_delete: *on_delete,
                });
            }
        }
        Ok(())
    }

    /// The stored column names, in stored order.
    #[must_use]
    pub fn stored_names(&self) -> Vec<String> {
        self.stored
            .iter()
            .map(|&i| self.columns[i].name.clone())
            .collect()
    }

    /// Live rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    fn text(&self, r: u32) -> &[u8] {
        let rr = self.rows[r as usize];
        &self.arena[rr.off..rr.off + rr.len as usize]
    }

    fn push_text(&mut self, text: &[u8]) -> u32 {
        let off = self.arena.len();
        self.arena.extend_from_slice(text);
        let r = u32::try_from(self.rows.len()).expect("fewer than 2^32 rows per table");
        self.rows.push(RowRef {
            off,
            len: u32::try_from(text.len()).expect("row under 4 GiB"),
            live: true,
        });
        self.live += 1;
        r
    }

    /// Every column's value of row `r` (generated columns computed).
    fn values(&self, r: u32) -> Result<Vec<Val>, String> {
        let mut raw = Vec::new();
        parse_tuple(self.text(r), 0, &mut raw)?;
        if raw.len() != self.stored.len() {
            return Err(format!(
                "`{}`: a stored row has {} values for {} columns",
                self.name,
                raw.len(),
                self.stored.len()
            ));
        }
        let mut vals: Vec<Val> = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| self.position[i].map_or(Val::Null, |p| read(c.kind, &raw[p])))
            .collect();
        for (i, c) in self.columns.iter().enumerate() {
            if let Some(g) = &c.generated {
                vals[i] = eval_pure(g, &|name| {
                    self.col(name)
                        .map(|j| vals.get(j).cloned().unwrap_or(Val::Null))
                })?;
            }
        }
        Ok(vals)
    }

    fn key_hash(parts: &[IndexPart]) -> u64 {
        let mut h = DefaultHasher::new();
        parts.hash(&mut h);
        h.finish()
    }

    fn index_key(&self, cols: &[usize], vals: &[Val]) -> Option<Vec<IndexPart>> {
        cols.iter()
            .map(|&c| index_part(self.columns[c].kind, &vals[c]))
            .collect()
    }

    fn ensure_index(&mut self, cols: &[usize]) -> Result<(), String> {
        if self.indexes.contains_key(cols) {
            return Ok(());
        }
        let mut idx: HashMap<u64, Vec<u32>> = HashMap::new();
        for r in 0..self.rows.len() {
            let r = u32::try_from(r).expect("row count fits");
            if !self.rows[r as usize].live {
                continue;
            }
            let vals = self.values(r)?;
            if let Some(k) = self.index_key(cols, &vals) {
                idx.entry(Self::key_hash(&k)).or_default().push(r);
            }
        }
        self.indexes.insert(cols.to_vec(), idx);
        Ok(())
    }

    /// Live rows whose `cols` equal `key` (verified, not just hashed).
    fn lookup(&mut self, cols: &[usize], key: &[Val]) -> Result<Vec<u32>, String> {
        let Some(parts): Option<Vec<IndexPart>> = cols
            .iter()
            .zip(key)
            .map(|(&c, v)| index_part(self.columns[c].kind, v))
            .collect()
        else {
            return Ok(Vec::new());
        };
        if parts.contains(&IndexPart::Other) {
            return self.scan_equal(cols, key);
        }
        self.ensure_index(cols)?;
        let cand = self.indexes[cols]
            .get(&Self::key_hash(&parts))
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::new();
        for r in cand {
            if !self.rows[r as usize].live || out.contains(&r) {
                continue;
            }
            let vals = self.values(r)?;
            if cols
                .iter()
                .zip(key)
                .all(|(&c, k)| compare(&vals[c], k) == Some(std::cmp::Ordering::Equal))
            {
                out.push(r);
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    fn scan_equal(&self, cols: &[usize], key: &[Val]) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        for r in self.live_rows() {
            let vals = self.values(r)?;
            if cols
                .iter()
                .zip(key)
                .all(|(&c, k)| compare(&vals[c], k) == Some(std::cmp::Ordering::Equal))
            {
                out.push(r);
            }
        }
        Ok(out)
    }

    fn live_rows(&self) -> Vec<u32> {
        (0..self.rows.len())
            .filter(|&r| self.rows[r].live)
            .map(|r| u32::try_from(r).expect("row count fits"))
            .collect()
    }

    /// Add a row to every built index.
    fn index_row(&mut self, r: u32, vals: &[Val]) {
        let keys: Vec<(Vec<usize>, Option<Vec<IndexPart>>)> = self
            .indexes
            .keys()
            .map(|cols| (cols.clone(), self.index_key(cols, vals)))
            .collect();
        for (cols, key) in keys {
            if let Some(k) = key {
                let h = Self::key_hash(&k);
                self.indexes
                    .get_mut(&cols)
                    .expect("listed")
                    .entry(h)
                    .or_default()
                    .push(r);
            }
        }
    }

    /// The next `AUTO_INCREMENT` value: the table option, or past the largest value stored.
    fn auto_next(&mut self) -> Result<u64, String> {
        if let Some(n) = self.auto_next {
            return Ok(n);
        }
        let Some(c) = self.auto_column else {
            return Err(format!("`{}` has no AUTO_INCREMENT column", self.name));
        };
        let mut max: i128 = 0;
        for r in self.live_rows() {
            if let Val::Int(i) = self.values(r)?[c] {
                max = max.max(i);
            }
        }
        let past = u64::try_from(max + 1).unwrap_or(1);
        let n = self.auto_option.unwrap_or(1).max(past).max(1);
        self.auto_next = Some(n);
        Ok(n)
    }

    /// Live rows in the order a table scan returns them: primary-key order, else insertion order.
    pub fn scan_order(&self) -> Result<Vec<u32>, String> {
        self.in_scan_order(self.live_rows())
    }

    /// `rows` (ascending slots) in the order a table scan would return them.
    fn in_scan_order(&self, mut rows: Vec<u32>) -> Result<Vec<u32>, String> {
        if let (true, Some(pk)) = (self.needs_sort, &self.primary) {
            let mut keyed: Vec<(Vec<Val>, u32)> = Vec::with_capacity(rows.len());
            for r in rows {
                let v = self.values(r)?;
                keyed.push((pk.iter().map(|&c| v[c].clone()).collect(), r));
            }
            keyed.sort_by(|a, b| {
                for (x, y) in a.0.iter().zip(&b.0) {
                    let o =
                        compare(x, y).unwrap_or_else(|| x.is_null().cmp(&y.is_null()).reverse());
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            rows = keyed.into_iter().map(|(_, r)| r).collect();
        }
        Ok(rows)
    }
}

/// Evaluate an expression that needs no session (a generated column, a default).
fn eval_pure(e: &Expr, col: &dyn Fn(&str) -> Result<Val, String>) -> Result<Val, String> {
    Ok(match e {
        Expr::Null => Val::Null,
        Expr::Num(n) => num(n),
        Expr::Str(s) => Val::Bytes(s.clone()),
        Expr::Hex(h) => Val::Hex(h.clone()),
        Expr::Bits(b) => Val::Int(i128::from(*b)),
        Expr::Bool(b) => Val::Int(i128::from(*b)),
        Expr::Column(c) => col(c)?,
        Expr::Neg(x) => neg(eval_pure(x, col)?),
        Expr::Binary(a, op, b) => binary(eval_pure(a, col)?, op, eval_pure(b, col)?)?,
        other => return Err(format!("{other:?} is not allowed here")),
    })
}

fn num(n: &str) -> Val {
    if n.bytes().all(|b| b.is_ascii_digit()) {
        n.parse().map_or_else(|_| Val::Dec(n.to_owned()), Val::Int)
    } else if n.contains(['e', 'E']) {
        Val::Float(n.parse().unwrap_or(0.0))
    } else {
        Val::Dec(n.to_owned())
    }
}

fn neg(v: Val) -> Val {
    match v {
        Val::Int(i) => Val::Int(-i),
        Val::Dec(s) => Val::Dec(
            s.strip_prefix('-')
                .map_or_else(|| format!("-{s}"), str::to_owned),
        ),
        Val::Null => Val::Null,
        other => Val::Float(-other.as_f64().unwrap_or(0.0)),
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn binary(a: Val, op: &str, b: Val) -> Result<Val, String> {
    if a.is_null() || b.is_null() {
        return Ok(Val::Null);
    }
    let ints = (a.as_exact_int(), b.as_exact_int());
    Ok(match (op, ints) {
        (">>" | "<<" | "|" | "&", (Some(x), Some(y))) => {
            let (x, y) = (x as u64, y as u64);
            Val::Int(i128::from(match op {
                ">>" => x.checked_shr(u32::try_from(y).unwrap_or(64)).unwrap_or(0),
                "<<" => x.checked_shl(u32::try_from(y).unwrap_or(64)).unwrap_or(0),
                "|" => x | y,
                _ => x & y,
            }))
        }
        ("+" | "-" | "*", (Some(x), Some(y)))
            if matches!(a, Val::Int(_) | Val::Hex(_)) && matches!(b, Val::Int(_) | Val::Hex(_)) =>
        {
            Val::Int(match op {
                "+" => x + y,
                "-" => x - y,
                _ => x * y,
            })
        }
        _ => {
            let (x, y) = (a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0));
            match op {
                "+" => Val::Float(x + y),
                "-" => Val::Float(x - y),
                "*" => Val::Float(x * y),
                "/" if y == 0.0 => Val::Null,
                "/" => Val::Float(x / y),
                _ => return Err(format!("operator {op} needs integers")),
            }
        }
    })
}

/// Session state: reset at the start of every file (ACE runs each file on a fresh session).
#[derive(Debug, Clone)]
pub struct Session {
    pub vars: HashMap<String, Val>,
    pub last_insert_id: u64,
    pub foreign_key_checks: bool,
    pub unique_checks: bool,
    pub strict: bool,
    pub no_auto_value_on_zero: bool,
}

impl Default for Session {
    /// MySQL 5.7's and MariaDB 10.2.4+'s defaults: strict mode, keys checked.
    fn default() -> Self {
        Self {
            vars: HashMap::new(),
            last_insert_id: 0,
            foreign_key_checks: true,
            unique_checks: true,
            strict: true,
            no_auto_value_on_zero: false,
        }
    }
}

/// What a statement did to one row, for the report.
#[derive(Debug, Clone)]
pub struct Event {
    pub table: usize,
    /// The row's slot in its table (slots only grow; a row keeps its slot when updated).
    pub row: u32,
    pub kind: EventKind,
    /// Every column's value (the old values for a delete, the new ones otherwise).
    pub values: Vec<Val>,
    /// The event whose `ON DELETE CASCADE` caused this one.
    pub cause: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Insert,
    Delete,
    /// The old row of an update; the new row follows as `Insert`-like `Updated`.
    UpdateOld,
    Updated,
}

/// Every table, in the order they were declared.
#[derive(Debug, Default)]
pub struct Store {
    tables: Vec<Table>,
    by_name: HashMap<String, usize>,
    /// `CURRENT_TIMESTAMP` for the whole import, `YYYY-MM-DD HH:MM:SS`.
    pub now: String,
    pub events: Vec<Event>,
}

type R<T> = Result<T, String>;

impl Store {
    #[must_use]
    pub fn new(now: String) -> Self {
        Self {
            now,
            ..Self::default()
        }
    }

    /// The table index of `name` (case-insensitive, as on Windows).
    pub fn table_index(&self, name: &str) -> R<usize> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .copied()
            .ok_or_else(|| format!("table `{name}` doesn't exist"))
    }

    #[must_use]
    pub fn table(&self, i: usize) -> Option<&Table> {
        self.tables.get(i).filter(|t| !t.dropped)
    }

    /// The name table `i` was created with (also for a dropped table).
    #[must_use]
    pub fn table_name(&self, i: usize) -> &str {
        &self.tables[i].name
    }

    fn t(&mut self, i: usize) -> &mut Table {
        &mut self.tables[i]
    }

    fn tr(&self, i: usize) -> &Table {
        &self.tables[i]
    }

    /// Each table's slot count (rows ever stored), by table index; a later slot is a newer row.
    #[must_use]
    pub fn slot_counts(&self) -> Vec<usize> {
        self.tables.iter().map(|t| t.rows.len()).collect()
    }

    /// Every live table index, in declaration order.
    #[must_use]
    pub fn table_indices(&self) -> Vec<usize> {
        (0..self.tables.len())
            .filter(|&i| !self.tables[i].dropped)
            .collect()
    }

    /// Add a table; an existing one of the same name is an error.
    pub fn create(&mut self, c: &CreateTable, sql: String) -> R<usize> {
        if self.by_name.contains_key(&c.name.to_ascii_lowercase()) {
            return Err(format!("table `{}` already exists", c.name));
        }
        let t = Table::new(c, sql)?;
        self.tables.push(t);
        let i = self.tables.len() - 1;
        self.by_name.insert(c.name.to_ascii_lowercase(), i);
        Ok(i)
    }

    /// Row `r` of table `t`, every column (generated ones computed).
    pub fn values(&self, t: usize, r: u32) -> R<Vec<Val>> {
        self.tr(t).values(r)
    }

    /// Live rows of `t` whose `columns` equal `key`.
    pub fn find(&mut self, t: usize, columns: &[&str], key: &[Val]) -> R<Vec<u32>> {
        let cols: Vec<usize> = columns
            .iter()
            .map(|c| self.tr(t).col(c))
            .collect::<R<_>>()?;
        self.t(t).lookup(&cols, key)
    }

    /// The index of a column of table `t`.
    pub fn column(&self, t: usize, name: &str) -> R<usize> {
        self.tr(t).col(name)
    }

    /// Hand every table and row to `sink` in scan order, as a dump would.
    pub fn emit<S: DumpSink>(&self, sink: &mut S) -> Result<(), ImportError> {
        let sql = |what: String| ImportError::Sql { line: 0, what };
        for i in self.table_indices() {
            let t = self.tr(i);
            let def = TableDef {
                name: t.name.clone(),
                columns: t
                    .columns
                    .iter()
                    .map(|c| DumpColumn {
                        name: c.name.clone(),
                        generated: c.generated.is_some(),
                    })
                    .collect(),
                sql: t.create_sql.clone(),
            };
            sink.create_table(&def)?;
            let names = t.stored_names();
            let mut raw: Vec<Value<'_>> = Vec::new();
            for (n, r) in t.scan_order().map_err(sql)?.into_iter().enumerate() {
                let text = t.text(r);
                parse_tuple(text, 0, &mut raw).map_err(sql)?;
                let row = Row::new(
                    &t.name,
                    u64::try_from(n).unwrap_or(u64::MAX) + 1,
                    &names,
                    &raw,
                );
                sink.row_text(&row, text)?;
            }
        }
        Ok(())
    }

    /// [`Store::emit`] restricted to some rows: every live table is declared, then only the
    /// `selected` slots of each (live ones), in the order a full scan would give them. The content
    /// overlay materialises the records a content file touched this way.
    pub fn emit_rows<S: DumpSink>(
        &self,
        sink: &mut S,
        selected: &std::collections::BTreeMap<usize, Vec<u32>>,
    ) -> Result<(), ImportError> {
        let sql = |what: String| ImportError::Sql { line: 0, what };
        for i in self.table_indices() {
            let t = self.tr(i);
            let def = TableDef {
                name: t.name.clone(),
                columns: t
                    .columns
                    .iter()
                    .map(|c| DumpColumn {
                        name: c.name.clone(),
                        generated: c.generated.is_some(),
                    })
                    .collect(),
                sql: t.create_sql.clone(),
            };
            sink.create_table(&def)?;
            let Some(rows) = selected.get(&i) else {
                continue;
            };
            let mut rows: Vec<u32> = rows
                .iter()
                .copied()
                .filter(|&r| t.rows[r as usize].live)
                .collect();
            rows.sort_unstable();
            rows.dedup();
            let names = t.stored_names();
            let mut raw: Vec<Value<'_>> = Vec::new();
            for (n, r) in t.in_scan_order(rows).map_err(sql)?.into_iter().enumerate() {
                let text = t.text(r);
                parse_tuple(text, 0, &mut raw).map_err(sql)?;
                let row = Row::new(
                    &t.name,
                    u64::try_from(n).unwrap_or(u64::MAX) + 1,
                    &names,
                    &raw,
                );
                sink.row_text(&row, text)?;
            }
        }
        Ok(())
    }

    // ---- statements ------------------------------------------------------------------------

    /// Run one statement.
    pub fn execute(&mut self, s: &Stmt, ses: &mut Session) -> R<()> {
        match s {
            Stmt::Ignored => Ok(()),
            Stmt::Set(v) => self.set(v, ses),
            Stmt::Insert {
                replace,
                ignore,
                table,
                columns,
                rows,
            } => self.insert(table, columns.as_deref(), rows, *replace, *ignore, ses),
            Stmt::Delete { table, filter } => {
                let t = self.table_index(table)?;
                let rows = self.select(t, filter.as_ref(), ses)?;
                self.delete_rows(t, &rows, None, ses)
            }
            Stmt::Update {
                table,
                sets,
                filter,
            } => self.update(table, sets, filter.as_ref(), ses),
            Stmt::DropTable { tables, if_exists } => {
                for name in tables {
                    match self.table_index(name) {
                        Ok(i) => self.drop_table(i, ses)?,
                        Err(e) if !*if_exists => return Err(e),
                        Err(_) => {}
                    }
                }
                Ok(())
            }
            Stmt::Truncate(name) => {
                let i = self.table_index(name)?;
                if ses.foreign_key_checks && !self.referencing(i).is_empty() {
                    return Err(format!(
                        "cannot truncate `{name}`: a foreign key references it"
                    ));
                }
                let rows = self.tr(i).live_rows();
                self.delete_rows(
                    i,
                    &rows,
                    None,
                    &Session {
                        foreign_key_checks: false,
                        ..ses.clone()
                    },
                )?;
                let t = self.t(i);
                t.auto_next = Some(1);
                t.auto_option = None;
                Ok(())
            }
            Stmt::CreateTable(c) => self
                .create(c, format!("CREATE TABLE `{}`", c.name))
                .map(|_| ()),
            Stmt::AlterTable { table, ops } => self.alter(table, ops),
        }
    }

    fn set(&mut self, v: &[(SetTarget, Expr)], ses: &mut Session) -> R<()> {
        for (target, e) in v {
            let val = match (target, e) {
                // `SET character_set_client = utf8mb4`: a bare word is the value itself.
                (SetTarget::System(_), Expr::Column(w)) => Val::Bytes(w.clone().into_bytes()),
                _ => self.eval(e, ses, None)?,
            };
            match target {
                SetTarget::User(n) => {
                    ses.vars.insert(n.to_ascii_lowercase(), val);
                }
                SetTarget::System(n) => {
                    let text = val
                        .as_bytes()
                        .map(|b| String::from_utf8_lossy(&b).to_ascii_uppercase());
                    let on = |t: &Option<String>| matches!(t.as_deref(), Some("1" | "ON" | "TRUE"));
                    match n.as_str() {
                        "foreign_key_checks" => ses.foreign_key_checks = on(&text),
                        "unique_checks" => ses.unique_checks = on(&text),
                        "sql_mode" => {
                            let t = text.unwrap_or_default();
                            let modes: Vec<&str> = t.split(',').map(str::trim).collect();
                            ses.strict = modes.iter().any(|m| {
                                matches!(
                                    *m,
                                    "STRICT_TRANS_TABLES" | "STRICT_ALL_TABLES" | "TRADITIONAL"
                                )
                            });
                            ses.no_auto_value_on_zero = modes.contains(&"NO_AUTO_VALUE_ON_ZERO");
                        }
                        // Character sets, time zone, notes: no effect on stored values here.
                        _ => {}
                    }
                    ses.vars.insert(format!("@@{n}"), val);
                }
            }
        }
        Ok(())
    }

    fn eval(&self, e: &Expr, ses: &Session, row: Option<(usize, &[Val])>) -> R<Val> {
        Ok(match e {
            Expr::UserVar(n) => ses
                .vars
                .get(&n.to_ascii_lowercase())
                .cloned()
                .unwrap_or(Val::Null),
            Expr::SysVar(n) => match n.as_str() {
                "foreign_key_checks" => Val::Int(i128::from(ses.foreign_key_checks)),
                "unique_checks" => Val::Int(i128::from(ses.unique_checks)),
                "sql_mode" => {
                    let mut modes = Vec::new();
                    if ses.strict {
                        modes.push("STRICT_TRANS_TABLES");
                    }
                    if ses.no_auto_value_on_zero {
                        modes.push("NO_AUTO_VALUE_ON_ZERO");
                    }
                    Val::Bytes(modes.join(",").into_bytes())
                }
                _ => ses
                    .vars
                    .get(&format!("@@{n}"))
                    .cloned()
                    .unwrap_or(Val::Null),
            },
            Expr::If(c, a, b) => {
                if self.truth(c, ses, row)? == Some(true) {
                    self.eval(a, ses, row)?
                } else {
                    self.eval(b, ses, row)?
                }
            }
            Expr::Coalesce(v) => {
                let mut out = Val::Null;
                for x in v {
                    out = self.eval(x, ses, row)?;
                    if !out.is_null() {
                        break;
                    }
                }
                out
            }
            Expr::LastInsertId => Val::Int(i128::from(ses.last_insert_id)),
            Expr::Now => Val::Bytes(self.now.clone().into_bytes()),
            Expr::Default => return Err("DEFAULT is not allowed here".into()),
            Expr::Neg(x) => neg(self.eval(x, ses, row)?),
            Expr::Binary(a, op, b) => binary(self.eval(a, ses, row)?, op, self.eval(b, ses, row)?)?,
            Expr::Column(c) => {
                let Some((t, vals)) = row else {
                    return Err(format!("unknown column `{c}`"));
                };
                vals[self.tr(t).col(c)?].clone()
            }
            other => eval_pure(other, &|c| Err(format!("unknown column `{c}`")))?,
        })
    }

    fn truth(&self, c: &Cond, ses: &Session, row: Option<(usize, &[Val])>) -> R<Option<bool>> {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let ev = |e: &Expr| self.eval(e, ses, row);
        Ok(match c {
            Cond::And(a, b) => match (self.truth(a, ses, row)?, self.truth(b, ses, row)?) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Cond::Or(a, b) => match (self.truth(a, ses, row)?, self.truth(b, ses, row)?) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
            Cond::Not(a) => self.truth(a, ses, row)?.map(|b| !b),
            Cond::Cmp(a, op, b) => {
                let (x, y) = (ev(a)?, ev(b)?);
                if *op == "<=>" {
                    return Ok(Some(match (x.is_null(), y.is_null()) {
                        (true, true) => true,
                        (false, false) => compare(&x, &y) == Some(Equal),
                        _ => false,
                    }));
                }
                compare(&x, &y).map(|o| match *op {
                    "=" => o == Equal,
                    "<>" => o != Equal,
                    "<" => o == Less,
                    "<=" => o != Greater,
                    ">" => o == Greater,
                    _ => o != Less,
                })
            }
            Cond::In(a, list, negated) => {
                let x = ev(a)?;
                if x.is_null() {
                    return Ok(None);
                }
                let mut any_null = false;
                let mut found = false;
                for e in list {
                    match compare(&x, &ev(e)?) {
                        Some(Equal) => found = true,
                        None => any_null = true,
                        _ => {}
                    }
                }
                if found {
                    Some(!negated)
                } else if any_null {
                    None
                } else {
                    Some(*negated)
                }
            }
            Cond::IsNull(a, is) => Some(ev(a)?.is_null() == *is),
            Cond::Between(a, lo, hi, negated) => {
                let x = ev(a)?;
                match (compare(&x, &ev(lo)?), compare(&x, &ev(hi)?)) {
                    (Some(l), Some(h)) => Some((l != Less && h != Greater) != *negated),
                    _ => None,
                }
            }
            Cond::Like(a, p, negated) => match (ev(a)?.as_bytes(), ev(p)?.as_bytes()) {
                (Some(x), Some(y)) => Some(like(&x, &y) != *negated),
                _ => None,
            },
            Cond::Truth(e) => {
                let v = ev(e)?;
                if v.is_null() {
                    None
                } else {
                    Some(v.as_f64().unwrap_or(0.0) != 0.0)
                }
            }
        })
    }

    /// The rows a `WHERE` selects, through an index when one `col = constant` (or `col IN (…)`)
    /// is a conjunct, else by scanning; in row-slot order.
    fn select(&mut self, t: usize, filter: Option<&Cond>, ses: &Session) -> R<Vec<u32>> {
        let Some(filter) = filter else {
            return Ok(self.tr(t).live_rows());
        };
        let mut conjuncts = Vec::new();
        flatten_and(filter, &mut conjuncts);
        let mut candidates: Option<Vec<u32>> = None;
        for c in conjuncts {
            let (col, consts): (&str, Vec<&Expr>) = match c {
                Cond::Cmp(Expr::Column(col), "=", e) | Cond::Cmp(e, "=", Expr::Column(col))
                    if is_const(e) =>
                {
                    (col, vec![e])
                }
                Cond::In(Expr::Column(col), list, false) if list.iter().all(is_const) => {
                    (col, list.iter().collect())
                }
                _ => continue,
            };
            let ci = self.tr(t).col(col)?;
            let kind = self.tr(t).columns[ci].kind;
            let mut keys = Vec::new();
            for e in consts {
                keys.push(self.eval(e, ses, None)?);
            }
            if keys
                .iter()
                .any(|k| matches!(index_part(kind, k), Some(IndexPart::Other)))
            {
                continue;
            }
            let mut rows = Vec::new();
            for k in keys {
                rows.extend(self.t(t).lookup(&[ci], &[k])?);
            }
            rows.sort_unstable();
            rows.dedup();
            candidates = Some(rows);
            break;
        }
        let candidates = candidates.unwrap_or_else(|| self.tr(t).live_rows());
        let mut out = Vec::new();
        for r in candidates {
            let vals = self.tr(t).values(r)?;
            if self.truth(filter, ses, Some((t, &vals)))? == Some(true) {
                out.push(r);
            }
        }
        Ok(out)
    }

    /// `(child table, foreign key index)` for every foreign key that references table `t`.
    fn referencing(&self, t: usize) -> Vec<(usize, usize)> {
        let name = self.tr(t).name.to_ascii_lowercase();
        let mut v = Vec::new();
        for c in self.table_indices() {
            for (k, fk) in self.tr(c).foreign.iter().enumerate() {
                if fk.parent.eq_ignore_ascii_case(&name) {
                    v.push((c, k));
                }
            }
        }
        v
    }

    fn delete_rows(
        &mut self,
        t: usize,
        rows: &[u32],
        cause: Option<usize>,
        ses: &Session,
    ) -> R<()> {
        let refs = if ses.foreign_key_checks {
            self.referencing(t)
        } else {
            Vec::new()
        };
        for &r in rows {
            if !self.tr(t).rows[r as usize].live {
                continue;
            }
            let vals = self.tr(t).values(r)?;
            {
                let tb = self.t(t);
                tb.rows[r as usize].live = false;
                tb.live -= 1;
            }
            self.events.push(Event {
                table: t,
                row: r,
                kind: EventKind::Delete,
                values: vals.clone(),
                cause,
            });
            let ev = self.events.len() - 1;
            for &(c, k) in &refs {
                let fk = self.tr(c).foreign[k].clone();
                let key: Vec<Val> = fk
                    .parent_columns
                    .iter()
                    .map(|n| self.tr(t).col(n).map(|i| vals[i].clone()))
                    .collect::<R<_>>()?;
                if key.iter().any(Val::is_null) {
                    continue;
                }
                let children = self.t(c).lookup(&fk.columns, &key)?;
                if children.is_empty() {
                    continue;
                }
                match fk.on_delete {
                    RefAction::Cascade => self.delete_rows(c, &children, Some(ev), ses)?,
                    RefAction::SetNull => {
                        for ch in children {
                            let mut v = self.tr(c).values(ch)?;
                            for &col in &fk.columns {
                                v[col] = Val::Null;
                            }
                            self.rewrite(c, ch, &v, ses, Some(ev))?;
                        }
                    }
                    RefAction::Restrict => {
                        return Err(format!(
                        "cannot delete a parent row of `{}`: a foreign key of `{}` references it",
                        self.tr(t).name,
                        self.tr(c).name
                    ))
                    }
                }
            }
        }
        Ok(())
    }

    fn drop_table(&mut self, i: usize, ses: &Session) -> R<()> {
        if ses.foreign_key_checks {
            if let Some(&(c, _)) = self.referencing(i).iter().find(|(c, _)| *c != i) {
                return Err(format!(
                    "cannot drop `{}`: `{}` has a foreign key to it",
                    self.tr(i).name,
                    self.tr(c).name
                ));
            }
        }
        let rows = self.tr(i).live_rows();
        for r in rows {
            let values = self.tr(i).values(r)?;
            self.events.push(Event {
                table: i,
                row: r,
                kind: EventKind::Delete,
                values,
                cause: None,
            });
        }
        let name = self.tr(i).name.to_ascii_lowercase();
        self.by_name.remove(&name);
        let t = self.t(i);
        for r in &mut t.rows {
            r.live = false;
        }
        t.live = 0;
        t.indexes.clear();
        t.dropped = true;
        Ok(())
    }

    fn alter(&mut self, table: &str, ops: &[AlterOp]) -> R<()> {
        let i = self.table_index(table)?;
        for op in ops {
            match op {
                AlterOp::Add(k) => {
                    if let KeyDef::Primary(cols) | KeyDef::Unique(_, cols) = k {
                        self.check_unique_existing(i, cols)?;
                    }
                    self.t(i).add_key(k)?;
                }
                AlterOp::Modify(def) => {
                    let t = self.t(i);
                    let c = t.col(&def.name)?;
                    let new = Column::from_def(def)?;
                    if new.kind != t.columns[c].kind
                        || new.generated.is_some() != t.columns[c].generated.is_some()
                    {
                        return Err(format!(
                            "ALTER TABLE `{table}` MODIFY `{}`: changing the type is not supported",
                            def.name
                        ));
                    }
                    if new.auto_increment {
                        t.auto_column = Some(c);
                    } else if t.auto_column == Some(c) {
                        t.auto_column = None;
                    }
                    t.columns[c] = new;
                }
                AlterOp::AutoIncrement(n) => {
                    let t = self.t(i);
                    t.auto_option = Some(*n);
                    t.auto_next = None;
                    // Never below the largest stored value.
                    if t.auto_column.is_some() {
                        t.auto_next()?;
                    }
                }
                AlterOp::DropKey(name) => {
                    let t = self.t(i);
                    if name == "PRIMARY" {
                        t.primary = None;
                    } else {
                        t.uniques.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
                        t.foreign.retain(|_| true);
                    }
                }
            }
        }
        Ok(())
    }

    fn check_unique_existing(&mut self, t: usize, names: &[String]) -> R<()> {
        let cols = self.tr(t).cols(names)?;
        let mut seen: HashMap<u64, Vec<Vec<Val>>> = HashMap::new();
        for r in self.tr(t).live_rows() {
            let v = self.tr(t).values(r)?;
            let Some(parts) = self.tr(t).index_key(&cols, &v) else {
                continue;
            };
            let key: Vec<Val> = cols.iter().map(|&c| v[c].clone()).collect();
            let bucket = seen.entry(Table::key_hash(&parts)).or_default();
            if bucket.iter().any(|k| {
                k.iter()
                    .zip(&key)
                    .all(|(a, b)| compare(a, b) == Some(std::cmp::Ordering::Equal))
            }) {
                return Err(format!(
                    "duplicate entry for a new key on `{}`",
                    self.tr(t).name
                ));
            }
            bucket.push(key);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        table: &str,
        columns: Option<&[String]>,
        rows: &[Vec<Expr>],
        replace: bool,
        ignore: bool,
        ses: &mut Session,
    ) -> R<()> {
        let t = self.table_index(table)?;
        let cols: Vec<usize> = match columns {
            Some(names) => self.tr(t).cols(names)?,
            None => self.tr(t).stored.clone(),
        };
        let mut first_generated: Option<u64> = None;
        for exprs in rows {
            if exprs.len() != cols.len() {
                return Err(format!(
                    "`{table}`: {} values for {} columns",
                    exprs.len(),
                    cols.len()
                ));
            }
            let mut given: Vec<Option<Val>> = vec![None; self.tr(t).columns.len()];
            for (&c, e) in cols.iter().zip(exprs) {
                if matches!(e, Expr::Default) {
                    continue;
                }
                if self.tr(t).columns[c].generated.is_some() {
                    return Err(format!(
                        "`{table}`: a value for generated column `{}`",
                        self.tr(t).columns[c].name
                    ));
                }
                given[c] = Some(self.eval(e, ses, None)?);
            }
            if let Some(id) = self.insert_one(t, given, replace, ignore, ses)? {
                first_generated.get_or_insert(id);
            }
        }
        if let Some(id) = first_generated {
            ses.last_insert_id = id;
        }
        Ok(())
    }

    /// The value a column takes when an `INSERT` does not give one.
    fn default_of(&self, t: usize, c: usize, ses: &Session) -> R<Val> {
        let col = &self.tr(t).columns[c];
        match &col.default {
            Some(e) => self.eval(e, ses, None),
            None if !col.not_null => Ok(Val::Null),
            None if ses.strict => Err(format!("field `{}` doesn't have a default value", col.name)),
            None => Ok(match col.kind {
                Kind::Text { .. } => Val::Bytes(Vec::new()),
                Kind::DateTime => {
                    return Err(format!("field `{}` doesn't have a default value", col.name))
                }
                _ => Val::Int(0),
            }),
        }
    }

    /// Insert one row; the `AUTO_INCREMENT` value it generated, if any.
    // Indexing: the loop body calls `&mut self` methods between reads of `given`.
    #[allow(clippy::needless_range_loop)]
    fn insert_one(
        &mut self,
        t: usize,
        mut given: Vec<Option<Val>>,
        replace: bool,
        ignore: bool,
        ses: &Session,
    ) -> R<Option<u64>> {
        let ncols = self.tr(t).columns.len();
        let mut generated_id = None;
        let auto = self.tr(t).auto_column;
        for c in 0..ncols {
            if self.tr(t).columns[c].generated.is_some() {
                continue;
            }
            if Some(c) == auto {
                let v = given[c].take();
                let zero = matches!(v.as_ref().and_then(Val::as_exact_int), Some(0));
                let v = match v {
                    None | Some(Val::Null) => None,
                    Some(_) if zero && !ses.no_auto_value_on_zero => None,
                    Some(v) => Some(v),
                };
                match v {
                    None => {
                        let n = self.t(t).auto_next()?;
                        self.t(t).auto_next = Some(n + 1);
                        generated_id = Some(n);
                        given[c] = Some(Val::Int(i128::from(n)));
                    }
                    Some(v) => {
                        if let Some(i) = v.as_exact_int() {
                            let next = self.t(t).auto_next()?;
                            if i >= i128::from(next) {
                                self.t(t).auto_next =
                                    Some(u64::try_from(i + 1).unwrap_or(u64::MAX));
                            }
                        }
                        given[c] = Some(v);
                    }
                }
                continue;
            }
            if given[c].is_none() {
                given[c] = Some(self.default_of(t, c, ses)?);
            }
        }
        // Store each value into its column, then read it back as the table holds it.
        let mut text = vec![b'('];
        let stored = self.tr(t).stored.clone();
        for (p, &c) in stored.iter().enumerate() {
            let col = &self.tr(t).columns[c];
            let v = given[c].clone().unwrap_or(Val::Null);
            if v.is_null() && col.not_null {
                return Err(format!("column `{}` cannot be null", col.name));
            }
            let s = store(col.kind, &v, ses.strict)
                .map_err(|e| format!("column `{}`: {e}", col.name))?;
            if p > 0 {
                text.push(b',');
            }
            text.extend_from_slice(&s);
        }
        text.push(b')');
        let vals = self.values_of_text(t, &text)?;
        // Unique keys: the primary key (unless its value was just generated) and every UNIQUE.
        let mut keys: Vec<Vec<usize>> = Vec::new();
        if let Some(pk) = &self.tr(t).primary {
            if !(generated_id.is_some() && pk.len() == 1 && Some(pk[0]) == auto) {
                keys.push(pk.clone());
            }
        }
        keys.extend(self.tr(t).uniques.iter().map(|(_, c)| c.clone()));
        for key in keys {
            let kv: Vec<Val> = key.iter().map(|&c| vals[c].clone()).collect();
            if kv.iter().any(Val::is_null) {
                continue;
            }
            let hits = self.t(t).lookup(&key, &kv)?;
            if hits.is_empty() {
                continue;
            }
            if replace {
                self.delete_rows(t, &hits, None, ses)?;
            } else if ignore {
                return Ok(None);
            } else {
                return Err(format!(
                    "duplicate entry {:?} for a unique key of `{}`",
                    show(&kv),
                    self.tr(t).name
                ));
            }
        }
        if ses.foreign_key_checks {
            self.check_parents(t, &vals)?;
        }
        let r = self.t(t).push_text(&text);
        self.t(t).index_row(r, &vals);
        if let Some(pk) = &self.tr(t).primary {
            if !(generated_id.is_some() && pk.len() == 1 && Some(pk[0]) == auto) {
                self.t(t).needs_sort = true;
            }
        }
        self.events.push(Event {
            table: t,
            row: r,
            kind: EventKind::Insert,
            values: vals,
            cause: None,
        });
        Ok(generated_id)
    }

    fn values_of_text(&self, t: usize, text: &[u8]) -> R<Vec<Val>> {
        let tb = self.tr(t);
        let mut raw = Vec::new();
        parse_tuple(text, 0, &mut raw)?;
        let mut vals: Vec<Val> = tb
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| tb.position[i].map_or(Val::Null, |p| read(c.kind, &raw[p])))
            .collect();
        for (i, c) in tb.columns.iter().enumerate() {
            if let Some(g) = &c.generated {
                vals[i] = eval_pure(g, &|name| tb.col(name).map(|j| vals[j].clone()))?;
            }
        }
        Ok(vals)
    }

    fn check_parents(&mut self, t: usize, vals: &[Val]) -> R<()> {
        let fks = self.tr(t).foreign.clone();
        for fk in fks {
            let key: Vec<Val> = fk.columns.iter().map(|&c| vals[c].clone()).collect();
            if key.iter().any(Val::is_null) {
                continue;
            }
            let p = self.table_index(&fk.parent)?;
            let pcols = self.tr(p).cols(&fk.parent_columns)?;
            if self.t(p).lookup(&pcols, &key)?.is_empty() {
                return Err(format!(
                    "cannot add a child row to `{}`: no `{}` row has {:?}",
                    self.tr(t).name,
                    fk.parent,
                    show(&key)
                ));
            }
        }
        Ok(())
    }

    fn update(
        &mut self,
        table: &str,
        sets: &[(String, Expr)],
        filter: Option<&Cond>,
        ses: &Session,
    ) -> R<()> {
        let t = self.table_index(table)?;
        let targets: Vec<usize> = sets
            .iter()
            .map(|(c, _)| self.tr(t).col(c))
            .collect::<R<_>>()?;
        for &c in &targets {
            if self.tr(t).columns[c].generated.is_some() {
                return Err(format!(
                    "`{table}`: cannot update generated column `{}`",
                    self.tr(t).columns[c].name
                ));
            }
        }
        let rows = self.select(t, filter, ses)?;
        for r in rows {
            let old = self.tr(t).values(r)?;
            let mut new = old.clone();
            for (&c, (_, e)) in targets.iter().zip(sets) {
                new[c] = if matches!(e, Expr::Default) {
                    self.default_of(t, c, ses)?
                } else {
                    self.eval(e, ses, Some((t, &old)))?
                };
            }
            self.rewrite(t, r, &new, ses, None)
                .map_err(|e| format!("UPDATE `{table}`: {e}"))?;
        }
        Ok(())
    }

    /// Replace row `r` of `t` with `new` (an `UPDATE`): unchanged rows are left alone; a change
    /// stamps `ON UPDATE CURRENT_TIMESTAMP` columns the statement did not set.
    fn rewrite(
        &mut self,
        t: usize,
        r: u32,
        new: &[Val],
        ses: &Session,
        cause: Option<usize>,
    ) -> R<bool> {
        let old = self.tr(t).values(r)?;
        // Store the new values and read them back.
        let stored = self.tr(t).stored.clone();
        let text_of = |vals: &[Val]| -> R<Vec<u8>> {
            let mut text = vec![b'('];
            for (p, &c) in stored.iter().enumerate() {
                let col = &self.tr(t).columns[c];
                if vals[c].is_null() && col.not_null {
                    return Err(format!("column `{}` cannot be null", col.name));
                }
                let s = store(col.kind, &vals[c], ses.strict)
                    .map_err(|e| format!("column `{}`: {e}", col.name))?;
                if p > 0 {
                    text.push(b',');
                }
                text.extend_from_slice(&s);
            }
            text.push(b')');
            Ok(text)
        };
        let mut text = text_of(new)?;
        let mut vals = self.values_of_text(t, &text)?;
        let same = |a: &[Val], b: &[Val]| a.iter().zip(b).all(|(x, y)| x == y);
        if same(&vals, &old) {
            return Ok(false);
        }
        let stamp: Vec<usize> = (0..vals.len())
            .filter(|&c| self.tr(t).columns[c].on_update_now && vals[c] == old[c])
            .collect();
        if !stamp.is_empty() {
            let mut v2 = vals.clone();
            for c in stamp {
                v2[c] = Val::Bytes(self.now.clone().into_bytes());
            }
            text = text_of(&v2)?;
            vals = self.values_of_text(t, &text)?;
        }
        // Keys that changed must stay unique; referenced keys may not change under children.
        let mut keys: Vec<Vec<usize>> = self.tr(t).primary.iter().cloned().collect();
        keys.extend(self.tr(t).uniques.iter().map(|(_, c)| c.clone()));
        for key in keys {
            if key.iter().all(|&c| vals[c] == old[c]) {
                continue;
            }
            let kv: Vec<Val> = key.iter().map(|&c| vals[c].clone()).collect();
            if kv.iter().any(Val::is_null) {
                continue;
            }
            if self.t(t).lookup(&key, &kv)?.iter().any(|&h| h != r) {
                return Err(format!(
                    "duplicate entry {:?} for a unique key of `{}`",
                    show(&kv),
                    self.tr(t).name
                ));
            }
        }
        if ses.foreign_key_checks {
            for (c, k) in self.referencing(t) {
                let fk = self.tr(c).foreign[k].clone();
                let pcols = self.tr(t).cols(&fk.parent_columns)?;
                if pcols.iter().any(|&p| vals[p] != old[p]) {
                    let key: Vec<Val> = pcols.iter().map(|&p| old[p].clone()).collect();
                    if !self.t(c).lookup(&fk.columns, &key)?.is_empty() {
                        return Err(format!(
                            "cannot change a key of `{}` that `{}` references",
                            self.tr(t).name,
                            self.tr(c).name
                        ));
                    }
                }
            }
            let changed_fk = self
                .tr(t)
                .foreign
                .iter()
                .any(|fk| fk.columns.iter().any(|&c| vals[c] != old[c]));
            if changed_fk {
                self.check_parents(t, &vals)?;
            }
        }
        let pk_changed = self
            .tr(t)
            .primary
            .as_ref()
            .is_some_and(|pk| pk.iter().any(|&c| vals[c] != old[c]));
        {
            let tb = self.t(t);
            let off = tb.arena.len();
            tb.arena.extend_from_slice(&text);
            tb.rows[r as usize].off = off;
            tb.rows[r as usize].len = u32::try_from(text.len()).expect("row under 4 GiB");
            if pk_changed {
                tb.needs_sort = true;
            }
        }
        self.t(t).index_row(r, &vals);
        self.events.push(Event {
            table: t,
            row: r,
            kind: EventKind::UpdateOld,
            values: old,
            cause,
        });
        self.events.push(Event {
            table: t,
            row: r,
            kind: EventKind::Updated,
            values: vals,
            cause,
        });
        Ok(true)
    }
}

fn flatten_and<'a>(c: &'a Cond, out: &mut Vec<&'a Cond>) {
    if let Cond::And(a, b) = c {
        flatten_and(a, out);
        flatten_and(b, out);
    } else {
        out.push(c);
    }
}

fn is_const(e: &Expr) -> bool {
    match e {
        Expr::Column(_) | Expr::Default => false,
        Expr::Neg(x) => is_const(x),
        Expr::Binary(a, _, b) => is_const(a) && is_const(b),
        _ => true,
    }
}

fn show(v: &[Val]) -> Vec<String> {
    v.iter()
        .map(|x| match x {
            Val::Null => "NULL".to_owned(),
            other => other
                .as_bytes()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default(),
        })
        .collect()
}

/// Loads a base dump into a [`Store`]: every table from its `CREATE TABLE`, every row kept as
/// its dump text (no key checks, as a dump load runs with them off).
#[derive(Debug)]
pub struct Loader<'s> {
    pub store: &'s mut Store,
    current: Option<(usize, Vec<String>)>,
}

impl<'s> Loader<'s> {
    pub fn new(store: &'s mut Store) -> Self {
        Self {
            store,
            current: None,
        }
    }
}

impl DumpSink for Loader<'_> {
    fn create_table(&mut self, def: &TableDef) -> Result<(), ImportError> {
        let sql = |what: String| ImportError::Sql {
            line: 0,
            what: format!("CREATE TABLE `{}`: {what}", def.name),
        };
        let c = super::parse::parse_create_table(def.sql.as_bytes()).map_err(sql)?;
        if let Ok(old) = self.store.table_index(&def.name) {
            // mysqldump drops and re-creates; a second definition replaces the first.
            self.store
                .drop_table(
                    old,
                    &Session {
                        foreign_key_checks: false,
                        ..Session::default()
                    },
                )
                .map_err(sql)?;
            self.store.events.clear();
        }
        let i = self.store.create(&c, def.sql.clone()).map_err(sql)?;
        self.current = Some((i, self.store.tr(i).stored_names()));
        Ok(())
    }

    fn row(&mut self, _row: &Row<'_>) -> Result<(), ImportError> {
        unreachable!("the scanner calls row_text")
    }

    fn row_text(&mut self, row: &Row<'_>, text: &[u8]) -> Result<(), ImportError> {
        let err = |what: String| ImportError::Sql {
            line: 0,
            what: format!("`{}` row {}: {what}", row.table, row.row_no),
        };
        let i = self.store.table_index(row.table).map_err(err)?;
        let canonical = matches!(&self.current, Some((c, names)) if *c == i
            && names.len() == row.columns().len()
            && names.iter().zip(row.columns()).all(|(a, b)| a.eq_ignore_ascii_case(b)));
        if canonical {
            let text = text.trim_ascii_start();
            let text = text.strip_prefix(b",").unwrap_or(text).trim_ascii_start();
            self.store.t(i).push_text(text);
            return Ok(());
        }
        // An explicit column list in another order: re-render the tuple in stored order.
        let names = self.store.tr(i).stored_names();
        let mut out = vec![b'('];
        for (p, n) in names.iter().enumerate() {
            let at = row.columns().iter().position(|c| c.eq_ignore_ascii_case(n));
            if p > 0 {
                out.push(b',');
            }
            match at.map(|a| &row.values()[a]) {
                None | Some(Value::Null) => out.extend_from_slice(b"NULL"),
                Some(Value::Bare(b)) => out.extend_from_slice(b),
                Some(Value::Quoted(b)) => out.extend_from_slice(&super::value::quote(b)),
            }
        }
        out.push(b')');
        self.store.t(i).push_text(&out);
        Ok(())
    }
}
