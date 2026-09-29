//! A streaming reader for a MySQL dump: `mysqldump`'s output, SQLyog's, or any other dump written
//! as plain SQL statements (a parser for the dump formats, not for anything ACE wrote).
//!
//! The dump is split into statements the way MySQL's client splits it (at each `;` outside a
//! string, a quoted identifier and a comment), so the line layout does not matter: `mysqldump`
//! writes each statement on one line, SQLyog writes `insert  into \`t\`(…) values` with one tuple
//! per line. Comments are dropped; an executable comment `/*!NNNNN … */` is read as code, as MySQL
//! runs it. Recognised, keywords in any case:
//!
//! * `CREATE TABLE [IF NOT EXISTS] \`name\` (` … `) …`: column names in declaration order, with
//!   generated columns flagged. A generated column is **not** part of the implicit `INSERT` column
//!   order, which is why `landblock_instance` (whose `landblock` is `GENERATED ALWAYS AS`) is
//!   dumped with an explicit column list.
//! * `INSERT [IGNORE] [INTO] \`name\` [(\`c\`, …)] VALUES (…),(…)`.
//! * Statements that carry no data (`SET`, `USE`, `DROP`, `LOCK`/`UNLOCK TABLES`, `CREATE
//!   DATABASE`, `ALTER TABLE … DISABLE`/`ENABLE KEYS`, transactions) are passed over.
//!
//! Any other statement (`REPLACE`, `UPDATE`, `INSERT … SELECT`, other `ALTER`s, views, routines)
//! is counted by kind in [`Scanned::skipped`], for the importer to refuse or report.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::BufRead;

use empyrean_common::dotnet::DotNetDateTime;

use crate::error::ImportError;

/// One column of a `CREATE TABLE`.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    /// `GENERATED ALWAYS AS (…)`: absent from the implicit column order.
    pub generated: bool,
}

/// One table's declared shape.
#[derive(Debug, Clone, Default)]
pub struct TableDef {
    pub name: String,
    pub columns: Vec<Column>,
    /// The whole `CREATE TABLE` statement as the dump wrote it (keys, constraints, options), with
    /// its comments dropped and executable comments kept as code.
    pub sql: String,
}

impl TableDef {
    /// The column order `mysqldump` uses when an `INSERT` carries no explicit list.
    #[must_use]
    pub fn insert_order(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| !c.generated)
            .map(|c| c.name.clone())
            .collect()
    }
}

/// One field of one row, exactly as the dump wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value<'a> {
    /// The bare token `NULL`.
    Null,
    /// An unquoted token: a number.
    Bare(&'a [u8]),
    /// A quoted string with `mysqldump`'s escapes resolved.
    Quoted(Cow<'a, [u8]>),
}

impl Value<'_> {
    /// The raw bytes, or `None` for `NULL`.
    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Null => None,
            Self::Bare(b) => Some(b),
            Self::Quoted(b) => Some(b),
        }
    }
}

/// One row, with its column names; values are read by column name, never by position, because
/// `mysqldump` writes an explicit column list for some tables and not for others.
#[derive(Debug)]
pub struct Row<'a> {
    pub table: &'a str,
    /// 1-based within the table, across every `INSERT` for it.
    pub row_no: u64,
    columns: &'a [String],
    values: &'a [Value<'a>],
}

impl<'a> Row<'a> {
    /// A row over `columns` with `values` in the same order.
    #[must_use]
    pub fn new(
        table: &'a str,
        row_no: u64,
        columns: &'a [String],
        values: &'a [Value<'a>],
    ) -> Self {
        Self {
            table,
            row_no,
            columns,
            values,
        }
    }

    /// The values, in the order of [`Row::columns`].
    #[must_use]
    pub fn values(&self) -> &'a [Value<'a>] {
        self.values
    }

    /// The raw value of one column.
    pub fn raw(&self, column: &'static str) -> Result<&Value<'a>, ImportError> {
        self.columns
            .iter()
            .position(|c| c == column)
            .and_then(|i| self.values.get(i))
            .ok_or_else(|| ImportError::MissingColumn {
                table: self.table.to_owned(),
                row: self.row_no,
                column,
            })
    }

    /// The column names, in the order the dump wrote the values.
    #[must_use]
    pub fn columns(&self) -> &'a [String] {
        self.columns
    }

    /// One column, converted as MySQL's client would hand it to EF.
    pub fn get<T: FromSql>(&self, column: &'static str) -> Result<T, ImportError> {
        T::from_sql(self.raw(column)?.bytes()).map_err(|expected| {
            match self.raw(column).ok().and_then(Value::bytes) {
                None => ImportError::UnexpectedNull {
                    table: self.table.to_owned(),
                    row: self.row_no,
                    column,
                },
                Some(b) => ImportError::BadValue {
                    table: self.table.to_owned(),
                    row: self.row_no,
                    column,
                    value: String::from_utf8_lossy(b).into_owned(),
                    expected,
                },
            }
        })
    }
}

/// A column value as a model field. `None` input is SQL `NULL`; the error is what was expected.
pub trait FromSql: Sized {
    fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str>;
}

fn text<'a>(v: Option<&'a [u8]>, what: &'static str) -> Result<&'a str, &'static str> {
    v.and_then(|b| core::str::from_utf8(b).ok()).ok_or(what)
}

macro_rules! from_sql_parse {
    ($($ty:ty),*) => {$(
        impl FromSql for $ty {
            fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str> {
                text(v, stringify!($ty))?.parse().map_err(|_| stringify!($ty))
            }
        }
    )*};
}

from_sql_parse!(u8, i8, u16, i16, u32, i32, u64, i64, f64);

/// MySQL `FLOAT`: the dump prints it in decimal; MySQL parses decimal text to a `double` and then
/// narrows it to the stored `float`, so the same two steps are taken here (a direct decimal to
/// `f32` parse can round differently in the last bit).
impl FromSql for f32 {
    fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str> {
        #[allow(clippy::cast_possible_truncation)]
        text(v, "f32")?
            .parse::<f64>()
            .map(|d| d as f32)
            .map_err(|_| "f32")
    }
}

/// `bit(1)`: `mysqldump` writes a one-byte quoted string, a raw `0x01` for true and `\0` for false.
impl FromSql for bool {
    fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str> {
        match v {
            Some([0] | b"0") => Ok(false),
            Some([1] | b"1") => Ok(true),
            _ => Err("a bit(1)"),
        }
    }
}

/// Text, decoded as UTF-8 (the dump declares `utf8`); invalid bytes are an error, not replaced.
impl FromSql for String {
    fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str> {
        text(v, "UTF-8 text").map(str::to_owned)
    }
}

/// `datetime`: `'YYYY-MM-DD HH:MM:SS'`.
impl FromSql for DotNetDateTime {
    fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str> {
        const WHAT: &str = "a datetime";
        let s = text(v, WHAT)?;
        let b = s.as_bytes();
        if b.len() != 19
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b' '
            || b[13] != b':'
            || b[16] != b':'
        {
            return Err(WHAT);
        }
        let n = |r: core::ops::Range<usize>| s[r].parse::<i32>().map_err(|_| WHAT);
        let (y, mo, d, h, mi, se) = (
            n(0..4)?,
            n(5..7)?,
            n(8..10)?,
            n(11..13)?,
            n(14..16)?,
            n(17..19)?,
        );
        if !(1..=12).contains(&mo)
            || !(1..=31).contains(&d)
            || h > 23
            || mi > 59
            || se > 59
            || y < 1
        {
            return Err(WHAT);
        }
        Ok(DotNetDateTime::new_hms(y, mo, d, h, mi, se))
    }
}

impl<T: FromSql> FromSql for Option<T> {
    fn from_sql(v: Option<&[u8]>) -> Result<Self, &'static str> {
        v.map(|b| T::from_sql(Some(b))).transpose()
    }
}

/// What a [`scan`] hands back to its caller.
pub trait DumpSink {
    /// One `CREATE TABLE`, before any of its rows.
    fn create_table(&mut self, table: &TableDef) -> Result<(), ImportError>;
    /// One row of one `INSERT`.
    fn row(&mut self, row: &Row<'_>) -> Result<(), ImportError>;
    /// One row with the dump text of its tuple (`(…)`), for sinks that keep rows as text.
    fn row_text(&mut self, row: &Row<'_>, _text: &[u8]) -> Result<(), ImportError> {
        self.row(row)
    }
}

/// Statements of one kind that the reader could not read: how many, and where the first starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Skipped {
    pub count: u64,
    /// The 1-based line the first one starts on.
    pub first_line: u64,
}

/// What a [`scan`] read, besides what it handed to its sink.
#[derive(Debug, Clone, Default)]
pub struct Scanned {
    /// Statements read, not counting empty ones.
    pub statements: u64,
    /// Statements the reader cannot read, by kind (`REPLACE`, `INSERT … SELECT`, `UPDATE`,
    /// `ALTER`, `CREATE VIEW`, …). Statements that carry no data (`SET`, `USE`, `DROP`,
    /// `LOCK TABLES`, `CREATE DATABASE`, `ALTER TABLE … DISABLE KEYS`, …) are not counted here.
    pub skipped: BTreeMap<String, Skipped>,
}

impl Scanned {
    fn skip(&mut self, kind: impl Into<String>, line: u64) {
        let e = self.skipped.entry(kind.into()).or_insert(Skipped {
            count: 0,
            first_line: line,
        });
        e.count += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Code,
    /// Inside a string literal opened by this quote.
    Str(u8),
    /// Inside a `` `quoted` `` identifier.
    Ident,
    /// Inside a `/* … */` comment.
    Block,
}

/// Splits a dump into statements as MySQL's client does: at each `;` outside a string, a quoted
/// identifier and a comment, whatever the line layout. Comments are dropped (each leaves a space),
/// and an executable comment `/*!NNNNN … */` keeps its body as code, since MySQL runs it.
/// MariaDB's `/*M!… */` is a plain comment, as it is to MySQL.
struct Statements<R> {
    reader: R,
    line: Vec<u8>,
    /// How far into `line` the splitter has read.
    at: usize,
    line_no: u64,
    state: State,
    /// Inside an executable comment, whose `*/` ends it.
    in_exec: bool,
    /// Where the open string or comment started, for an error at the end of the file.
    opened_at: u64,
}

impl<R: BufRead> Statements<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            line: Vec::new(),
            at: 0,
            line_no: 0,
            state: State::Code,
            in_exec: false,
            opened_at: 0,
        }
    }

    /// The next statement's text, without its `;`, into `out`, with the line it starts on; `None`
    /// at the end of the dump.
    fn next(&mut self, out: &mut Vec<u8>) -> Result<Option<u64>, ImportError> {
        out.clear();
        let mut start: Option<u64> = None;
        loop {
            if self.at >= self.line.len() {
                self.line.clear();
                self.at = 0;
                let n = self.reader.read_until(b'\n', &mut self.line).map_err(|e| {
                    ImportError::Sql {
                        line: self.line_no,
                        what: format!("read failed: {e}"),
                    }
                })?;
                if n == 0 {
                    let open = match self.state {
                        State::Code if self.in_exec => Some("/*! comment"),
                        State::Code => None,
                        State::Str(_) => Some("string literal"),
                        State::Ident => Some("`quoted` identifier"),
                        State::Block => Some("/* comment"),
                    };
                    if let Some(what) = open {
                        return Err(ImportError::Sql {
                            line: self.opened_at,
                            what: format!("unterminated {what}"),
                        });
                    }
                    // A last statement without its `;`.
                    return Ok(start);
                }
                self.line_no += 1;
            }
            let line = &self.line;
            let n = line.len();
            let mut i = self.at;
            while i < n {
                let c = line[i];
                match self.state {
                    State::Str(q) => {
                        // Copy up to the next backslash or quote in one go.
                        let run = line[i..]
                            .iter()
                            .position(|&b| b == b'\\' || b == q)
                            .map_or(n, |p| i + p);
                        out.extend_from_slice(&line[i..run]);
                        i = run;
                        if i >= n {
                            break;
                        }
                        if line[i] == b'\\' {
                            let end = (i + 2).min(n);
                            out.extend_from_slice(&line[i..end]);
                            i = end;
                        } else if line.get(i + 1) == Some(&q) {
                            // A doubled quote is a quote.
                            out.extend_from_slice(&line[i..i + 2]);
                            i += 2;
                        } else {
                            out.push(q);
                            i += 1;
                            self.state = State::Code;
                        }
                    }
                    State::Ident => {
                        out.push(c);
                        i += 1;
                        if c == b'`' {
                            self.state = State::Code;
                        }
                    }
                    State::Block => {
                        if c == b'*' && line.get(i + 1) == Some(&b'/') {
                            self.state = State::Code;
                            if start.is_some() {
                                out.push(b' ');
                            }
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    State::Code => match c {
                        b';' => {
                            if let Some(s) = start {
                                self.at = i + 1;
                                return Ok(Some(s));
                            }
                            out.clear();
                            i += 1;
                        }
                        b'\'' | b'"' | b'`' => {
                            self.state = if c == b'`' {
                                State::Ident
                            } else {
                                State::Str(c)
                            };
                            self.opened_at = self.line_no;
                            start.get_or_insert(self.line_no);
                            out.push(c);
                            i += 1;
                        }
                        b'/' if line.get(i + 1) == Some(&b'*') => {
                            self.opened_at = self.line_no;
                            if line.get(i + 2) == Some(&b'!') && !self.in_exec {
                                let mut j = i + 3;
                                while j < n && line[j].is_ascii_digit() {
                                    j += 1;
                                }
                                self.in_exec = true;
                                if start.is_some() {
                                    out.push(b' ');
                                }
                                i = j;
                            } else {
                                self.state = State::Block;
                                i += 2;
                            }
                        }
                        b'*' if self.in_exec && line.get(i + 1) == Some(&b'/') => {
                            self.in_exec = false;
                            if start.is_some() {
                                out.push(b' ');
                            }
                            i += 2;
                        }
                        b'-' if line.get(i + 1) == Some(&b'-')
                            && line.get(i + 2).is_none_or(|b| {
                                b.is_ascii_whitespace() || b.is_ascii_control()
                            }) =>
                        {
                            i = skip_to_newline(line, i);
                        }
                        b'#' => i = skip_to_newline(line, i),
                        _ => {
                            if !c.is_ascii_whitespace() {
                                start.get_or_insert(self.line_no);
                            }
                            if start.is_some() {
                                out.push(c);
                            }
                            i += 1;
                        }
                    },
                }
            }
            self.at = n;
        }
    }
}

/// The offset of the line's own newline (kept as whitespace), or its end.
fn skip_to_newline(line: &[u8], from: usize) -> usize {
    line[from..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(line.len(), |p| from + p)
}

/// A cursor over one statement's text.
struct Cur<'a> {
    s: &'a [u8],
    i: usize,
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

impl<'a> Cur<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    /// The next bare word, without moving past it.
    fn peek_word(&mut self) -> Option<&'a [u8]> {
        self.ws();
        let len = self.s[self.i..]
            .iter()
            .position(|&b| !is_word_byte(b))
            .unwrap_or(self.s.len() - self.i);
        (len > 0).then(|| &self.s[self.i..self.i + len])
    }

    /// Moves past the next word if it is the keyword `k`.
    fn kw(&mut self, k: &str) -> bool {
        match self.peek_word() {
            Some(w) if w.eq_ignore_ascii_case(k.as_bytes()) => {
                self.i += w.len();
                true
            }
            _ => false,
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// One identifier: `` `quoted` `` or bare.
    fn one_ident(&mut self) -> Option<String> {
        self.ws();
        if self.s.get(self.i) == Some(&b'`') {
            let len = self.s[self.i + 1..].iter().position(|&b| b == b'`')?;
            let name = core::str::from_utf8(&self.s[self.i + 1..self.i + 1 + len]).ok()?;
            self.i += len + 2;
            return Some(name.to_owned());
        }
        let w = self.peek_word()?;
        self.i += w.len();
        core::str::from_utf8(w).ok().map(str::to_owned)
    }

    /// A table name, `name` or `db`.`name` (the database is not kept).
    fn table_name(&mut self) -> Option<String> {
        let mut name = self.one_ident()?;
        if self.s.get(self.i) == Some(&b'.') {
            self.i += 1;
            name = self.one_ident()?;
        }
        Some(name)
    }

    fn at_end(&mut self) -> bool {
        self.ws();
        self.i >= self.s.len()
    }
}

/// Words that open a table element that is not a column.
const NOT_COLUMNS: &[&str] = &[
    "PRIMARY",
    "KEY",
    "INDEX",
    "UNIQUE",
    "CONSTRAINT",
    "FOREIGN",
    "FULLTEXT",
    "SPATIAL",
    "CHECK",
];

/// Splits the body of `( … )`, starting just after its `(`, at top-level commas: each element's
/// range, or `None` when the `)` never comes.
fn split_elements(s: &[u8], from: usize) -> Option<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut i = from;
    let mut el = from;
    while i < s.len() {
        match s[i] {
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < s.len() && s[i] != q {
                    if s[i] == b'\\' && q != b'`' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'(' => depth += 1,
            b')' if depth == 0 => {
                out.push((el, i));
                return Some(out);
            }
            b')' => depth -= 1,
            b',' if depth == 0 => {
                out.push((el, i));
                el = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The text upper-cased, with string literals emptied (so a `COMMENT` cannot match a keyword).
fn code_upper(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c == b'\'' || c == b'"' {
            i += 1;
            while i < s.len() && s[i] != c {
                if s[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            out.extend_from_slice(b"''");
        } else {
            out.push(c.to_ascii_uppercase());
        }
        i += 1;
    }
    out
}

/// `CREATE TABLE`'s body: its columns, in declaration order.
fn table_columns(s: &[u8], open: usize) -> Result<Vec<Column>, String> {
    let elements =
        split_elements(s, open).ok_or_else(|| "the column list is unterminated".to_owned())?;
    let mut columns = Vec::new();
    for (a, b) in elements {
        let mut cur = Cur { s: &s[a..b], i: 0 };
        cur.ws();
        if cur.s.get(cur.i) != Some(&b'`') {
            match cur.peek_word() {
                None => continue,
                Some(w)
                    if NOT_COLUMNS
                        .iter()
                        .any(|k| w.eq_ignore_ascii_case(k.as_bytes())) =>
                {
                    continue
                }
                Some(_) => {}
            }
        }
        let Some(name) = cur.one_ident() else {
            continue;
        };
        let shape = code_upper(&cur.s[cur.i..]);
        let generated = find(&shape, b"GENERATED ALWAYS AS").is_some()
            || (find(&shape, b" AS (").is_some()
                && (find(&shape, b"VIRTUAL").is_some() || find(&shape, b"STORED").is_some()));
        columns.push(Column { name, generated });
    }
    Ok(columns)
}

/// The line `offset` into a statement that starts on line `start` falls on.
fn line_at(stmt: &[u8], start: u64, offset: usize) -> u64 {
    start
        + stmt[..offset.min(stmt.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u64
}

/// Read the dump start to finish, statement by statement, handing every `CREATE TABLE` and every
/// row to `sink`. Any line layout is read: one statement over many lines (SQLyog writes one tuple
/// per line), several statements on one line, comments anywhere. A statement it cannot read is
/// counted in the result by kind, never passed over silently.
pub fn scan<R: BufRead, S: DumpSink>(reader: R, sink: &mut S) -> Result<Scanned, ImportError> {
    let mut tables: Vec<TableDef> = Vec::new();
    let mut row_counts: Vec<u64> = Vec::new();
    let mut stmt: Vec<u8> = Vec::new();
    let mut canonical: Vec<u8> = Vec::new();
    let mut scanned = Scanned::default();
    let mut statements = Statements::new(reader);
    let sql = |line: u64, what: String| ImportError::Sql { line, what };

    while let Some(line_no) = statements.next(&mut stmt)? {
        scanned.statements += 1;
        let s = stmt.as_slice();
        let mut cur = Cur { s, i: 0 };
        let Some(first) = cur.peek_word().map(<[u8]>::to_ascii_uppercase) else {
            scanned.skip("a statement that does not start with a keyword", line_no);
            continue;
        };
        match first.as_slice() {
            b"CREATE" => {
                cur.kw("CREATE");
                if cur.kw("OR") {
                    cur.kw("REPLACE");
                }
                let temporary = cur.kw("TEMPORARY");
                if !cur.kw("TABLE") {
                    match cur.peek_word().map(<[u8]>::to_ascii_uppercase) {
                        Some(w) if !temporary && (w == b"DATABASE" || w == b"SCHEMA") => {}
                        Some(w) => {
                            scanned.skip(format!("CREATE {}", String::from_utf8_lossy(&w)), line_no)
                        }
                        None => scanned.skip("CREATE", line_no),
                    }
                    continue;
                }
                if cur.kw("IF") {
                    cur.kw("NOT");
                    cur.kw("EXISTS");
                }
                let name = cur
                    .table_name()
                    .ok_or_else(|| sql(line_no, "CREATE TABLE without a table name".into()))?;
                if temporary || !cur.eat(b'(') {
                    scanned.skip("CREATE TABLE … LIKE / SELECT", line_no);
                    continue;
                }
                let columns = table_columns(s, cur.i)
                    .map_err(|what| sql(line_no, format!("CREATE TABLE `{name}`: {what}")))?;
                let def = TableDef {
                    name,
                    columns,
                    sql: String::from_utf8_lossy(s.trim_ascii()).into_owned(),
                };
                sink.create_table(&def)?;
                if let Some(i) = tables.iter().position(|t| t.name == def.name) {
                    tables[i] = def;
                    row_counts[i] = 0;
                } else {
                    tables.push(def);
                    row_counts.push(0);
                }
            }
            b"INSERT" => {
                cur.kw("INSERT");
                while cur.kw("LOW_PRIORITY")
                    || cur.kw("DELAYED")
                    || cur.kw("HIGH_PRIORITY")
                    || cur.kw("IGNORE")
                {}
                cur.kw("INTO");
                let name = cur
                    .table_name()
                    .ok_or_else(|| sql(line_no, "INSERT INTO without a table name".into()))?;
                let bad_list = || sql(line_no, format!("INSERT INTO `{name}`: bad column list"));
                let explicit: Option<Vec<String>> = if cur.eat(b'(') {
                    if cur.kw("SELECT") {
                        scanned.skip("INSERT … SELECT", line_no);
                        continue;
                    }
                    let mut cols = Vec::new();
                    if !cur.eat(b')') {
                        loop {
                            cols.push(cur.one_ident().ok_or_else(bad_list)?);
                            if cur.eat(b')') {
                                break;
                            }
                            if !cur.eat(b',') {
                                return Err(bad_list());
                            }
                        }
                    }
                    Some(cols)
                } else {
                    None
                };
                if !(cur.kw("VALUES") || cur.kw("VALUE")) {
                    if cur.kw("SELECT") || cur.kw("TABLE") || cur.kw("WITH") || cur.eat(b'(') {
                        scanned.skip("INSERT … SELECT", line_no);
                    } else if cur.kw("SET") {
                        scanned.skip("INSERT … SET", line_no);
                    } else {
                        return Err(sql(line_no, "INSERT without VALUES".into()));
                    }
                    continue;
                }
                let ti = tables.iter().position(|t| t.name == name).ok_or_else(|| {
                    sql(
                        line_no,
                        format!("INSERT INTO `{name}` before its CREATE TABLE"),
                    )
                })?;
                if let Some(cols) = &explicit {
                    let declared = &tables[ti].columns;
                    if let Some(c) = cols
                        .iter()
                        .find(|c| !declared.iter().any(|d| d.name.eq_ignore_ascii_case(c)))
                    {
                        return Err(sql(
                            line_no,
                            format!(
                                "INSERT INTO `{name}` names column `{c}`, which its CREATE TABLE does not declare"
                            ),
                        ));
                    }
                }
                let columns: Vec<String> = explicit.unwrap_or_else(|| tables[ti].insert_order());
                let values_at = cur.i;
                let body = &s[values_at..];
                let mut values: Vec<Value<'_>> = Vec::new();
                let mut pos = 0usize;
                loop {
                    // An error names the line the tuple (or whatever stands there) starts on.
                    let tuple_at = || {
                        values_at
                            + pos
                            + body[pos..]
                                .iter()
                                .position(|&b| !(b == b',' || b.is_ascii_whitespace()))
                                .unwrap_or(0)
                    };
                    let parsed = parse_tuple_ext(body, pos, &mut values)
                        .map_err(|what| sql(line_at(s, line_no, tuple_at()), what))?;
                    let Some((consumed, plain)) = parsed else {
                        break;
                    };
                    row_counts[ti] += 1;
                    if values.len() != columns.len() {
                        return Err(sql(
                            line_at(s, line_no, tuple_at()),
                            format!(
                                "row {} of `{}` has {} values for {} columns",
                                row_counts[ti],
                                tables[ti].name,
                                values.len(),
                                columns.len()
                            ),
                        ));
                    }
                    let row = Row {
                        table: &tables[ti].name,
                        row_no: row_counts[ti],
                        columns: &columns,
                        values: &values,
                    };
                    if plain {
                        sink.row_text(&row, &body[pos..consumed])?;
                    } else {
                        // Hex, bit and introduced literals, double quotes, spacing: the sink gets
                        // the tuple in the plain form a row store keeps.
                        canonical_tuple(&values, &mut canonical);
                        sink.row_text(&row, &canonical)?;
                    }
                    pos = consumed;
                }
            }
            b"ALTER" => {
                cur.kw("ALTER");
                let keys = cur.kw("TABLE")
                    && cur.table_name().is_some()
                    && (cur.kw("DISABLE") || cur.kw("ENABLE"))
                    && cur.kw("KEYS")
                    && cur.at_end();
                if !keys {
                    scanned.skip("ALTER", line_no);
                }
            }
            // No data and no schema: session settings, locks, transactions, and dropping a table
            // before its `CREATE TABLE`.
            b"SET" | b"USE" | b"DROP" | b"LOCK" | b"UNLOCK" | b"START" | b"BEGIN" | b"COMMIT" => {}
            other => scanned.skip(String::from_utf8_lossy(other).into_owned(), line_no),
        }
    }
    Ok(scanned)
}

/// A tuple in the plain form: `NULL`, bare numbers and `'…'` strings, comma-separated.
fn canonical_tuple(values: &[Value<'_>], out: &mut Vec<u8>) {
    out.clear();
    out.push(b'(');
    for (k, v) in values.iter().enumerate() {
        if k > 0 {
            out.push(b',');
        }
        match v {
            Value::Null => out.extend_from_slice(b"NULL"),
            Value::Bare(b) => out.extend_from_slice(b),
            Value::Quoted(b) => out.extend_from_slice(&crate::import::sql::value::quote(b)),
        }
    }
    out.push(b')');
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse one `(v, v, …)` tuple starting at or after `pos`, filling `out`. Returns the offset just
/// past the tuple, or `None` when there are no more tuples.
pub(crate) fn parse_tuple<'a>(
    body: &'a [u8],
    pos: usize,
    out: &mut Vec<Value<'a>>,
) -> Result<Option<usize>, String> {
    Ok(parse_tuple_ext(body, pos, out)?.map(|(end, _)| end))
}

fn hex_bytes(digits: &[u8]) -> Result<Vec<u8>, String> {
    let nib = |d: u8| match d {
        b'0'..=b'9' => Ok(d - b'0'),
        b'a'..=b'f' => Ok(d - b'a' + 10),
        b'A'..=b'F' => Ok(d - b'A' + 10),
        _ => Err(format!("bad hex digit {:?}", d as char)),
    };
    // An odd count reads as if it had a leading zero, as MySQL reads `0xABC`.
    let mut out = Vec::with_capacity(digits.len().div_ceil(2));
    let mut rest = digits;
    if rest.len() % 2 == 1 {
        out.push(nib(rest[0])?);
        rest = &rest[1..];
    }
    for p in rest.chunks(2) {
        out.push((nib(p[0])? << 4) | nib(p[1])?);
    }
    Ok(out)
}

/// A bit literal's value, as the fewest big-endian bytes that hold it (at least one).
fn bit_bytes(digits: &[u8]) -> Result<Vec<u8>, String> {
    if digits.len() > 64 {
        return Err("bit literal wider than 64 bits".into());
    }
    let mut v: u64 = 0;
    for &d in digits {
        if !matches!(d, b'0' | b'1') {
            return Err(format!("bad bit digit {:?}", d as char));
        }
        v = (v << 1) | u64::from(d - b'0');
    }
    let bytes = v.to_be_bytes();
    let skip = bytes.iter().take(7).take_while(|&&b| b == 0).count();
    Ok(bytes[skip..].to_vec())
}

/// [`parse_tuple`], also saying whether the tuple was in the plain form (`NULL`, bare numbers and
/// `'…'` strings, no spacing): only then is its text what a row store keeps as it is.
///
/// Also read: spacing around values, `null` in any case, `"…"` strings, a doubled quote inside a
/// string, `_charset'…'` introducers, and `0x…` / `X'…'` hex and `b'…'` / `0b…` bit literals (as
/// bytes).
fn parse_tuple_ext<'a>(
    body: &'a [u8],
    pos: usize,
    out: &mut Vec<Value<'a>>,
) -> Result<Option<(usize, bool)>, String> {
    out.clear();
    let n = body.len();
    let mut plain = true;
    let mut i = pos;
    while i < n && (matches!(body[i], b',' | b';') || body[i].is_ascii_whitespace()) {
        i += 1;
    }
    if i >= n {
        return Ok(None);
    }
    if body[i] != b'(' {
        return Err(format!(
            "expected '(' at offset {i}, found {:?}",
            body[i] as char
        ));
    }
    i += 1;
    let skip_ws = |i: &mut usize, plain: &mut bool| {
        while *i < n && body[*i].is_ascii_whitespace() {
            *i += 1;
            *plain = false;
        }
    };
    loop {
        skip_ws(&mut i, &mut plain);
        if i >= n {
            return Err("unterminated tuple".into());
        }
        let c = body[i];
        let next = body.get(i + 1).copied();
        if c == b'\'' || c == b'"' {
            let (value, end) = parse_quoted(body, i)?;
            plain &= c == b'\'';
            out.push(Value::Quoted(value));
            i = end;
        } else if c == b'_' && is_introducer(body, i) {
            let mut j = i + 1;
            while j < n && is_word_byte(body[j]) {
                j += 1;
            }
            while j < n && body[j].is_ascii_whitespace() {
                j += 1;
            }
            let (value, end) = parse_quoted(body, j)?;
            plain = false;
            out.push(Value::Quoted(value));
            i = end;
        } else if matches!(c, b'x' | b'X' | b'b' | b'B') && next == Some(b'\'') {
            let close = body[i + 2..]
                .iter()
                .position(|&b| b == b'\'')
                .ok_or_else(|| "unterminated literal".to_owned())?
                + i
                + 2;
            let digits = &body[i + 2..close];
            let bytes = if matches!(c, b'x' | b'X') {
                if digits.len() % 2 == 1 {
                    return Err("X'…' needs an even number of hex digits".into());
                }
                hex_bytes(digits)?
            } else {
                bit_bytes(digits)?
            };
            plain = false;
            out.push(Value::Quoted(Cow::Owned(bytes)));
            i = close + 1;
        } else if c == b'0'
            && matches!(next, Some(b'x' | b'b'))
            && body.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        {
            let mut j = i + 2;
            while j < n && body[j].is_ascii_hexdigit() {
                j += 1;
            }
            let digits = &body[i + 2..j];
            let bytes = if next == Some(b'x') {
                hex_bytes(digits)?
            } else {
                bit_bytes(digits)?
            };
            plain = false;
            out.push(Value::Quoted(Cow::Owned(bytes)));
            i = j;
        } else {
            let start = i;
            while i < n && !matches!(body[i], b',' | b')') && !body[i].is_ascii_whitespace() {
                i += 1;
            }
            let raw = &body[start..i];
            if raw.is_empty() {
                return Err(format!("expected a value, found {:?}", c as char));
            }
            out.push(if raw.eq_ignore_ascii_case(b"NULL") {
                plain &= raw == b"NULL";
                Value::Null
            } else {
                Value::Bare(raw)
            });
        }
        skip_ws(&mut i, &mut plain);
        if i >= n {
            return Err("unterminated tuple".into());
        }
        match body[i] {
            b',' => i += 1,
            b')' => return Ok(Some((i + 1, plain))),
            other => return Err(format!("expected ',' or ')', found {:?}", other as char)),
        }
    }
}

/// `_utf8mb4'…'`, `_binary '…'`: a character set introducer before a string.
fn is_introducer(body: &[u8], at: usize) -> bool {
    let mut j = at + 1;
    while j < body.len() && is_word_byte(body[j]) {
        j += 1;
    }
    let word_end = j;
    while j < body.len() && body[j].is_ascii_whitespace() {
        j += 1;
    }
    word_end > at + 1 && matches!(body.get(j), Some(b'\'' | b'"'))
}

/// Parse a `'…'` or `"…"` literal starting at its quote, resolving `mysqldump`'s escapes
/// (`\0 \' \" \b \n \r \t \Z \\`, and any other `\X` is the literal `X`) and a doubled quote.
fn parse_quoted(body: &[u8], start: usize) -> Result<(Cow<'_, [u8]>, usize), String> {
    let q = body[start];
    let mut i = start + 1;
    let mut has_escape = false;
    while i < body.len() {
        match body[i] {
            b'\\' => {
                has_escape = true;
                i += 2;
            }
            b if b == q && body.get(i + 1) == Some(&q) => {
                has_escape = true;
                i += 2;
            }
            b if b == q => {
                let raw = &body[start + 1..i];
                let v = if has_escape {
                    Cow::Owned(unescape(raw, q))
                } else {
                    Cow::Borrowed(raw)
                };
                return Ok((v, i + 1));
            }
            _ => i += 1,
        }
    }
    Err("unterminated string literal".into())
}

fn unescape(raw: &[u8], q: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == q && raw.get(i + 1) == Some(&q) {
            out.push(q);
            i += 2;
            continue;
        }
        if raw[i] != b'\\' || i + 1 >= raw.len() {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        out.push(match raw[i + 1] {
            b'0' => 0,
            b'b' => 0x08,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'Z' => 0x1A,
            other => other,
        });
        i += 2;
    }
    out
}
