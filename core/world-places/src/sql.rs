//! Reading a world database from its SQL dump: the tables the places are read from, out of a
//! plain-SQL dump of the world database's schema (a `mysqldump` of it, or any dump written as
//! `CREATE TABLE` and `INSERT` statements).
//!
//! The dump is read statement by statement, each ended by a `;` outside a string, a quoted name
//! and a comment, so its line layout does not matter. A `CREATE TABLE` gives a table's columns in
//! order (less any generated column, which an `INSERT` without a column list leaves out); an
//! `INSERT [IGNORE] [INTO] <table> [(<columns>)] VALUES (…), (…)` gives its rows. Every other
//! statement, and every other table, is passed over. Comments are dropped, an executable comment
//! (`/*!… */`) among them, since a dump keeps no rows in one.
//!
//! Values are numbers, strings (with backslash escapes, and a doubled quote for a quote), `NULL`,
//! and a bit written as a one-byte string, `b'…'` or a number. A row that names a column the
//! table has not declared, or that has another number of values than its columns, is an error.

use std::collections::HashMap;
use std::fmt;
use std::io::BufRead;
use std::sync::Arc;

use crate::content::{GameEvent, Landmark, Placement, Spawn, Spot, Weenie, WorldContent};

/// Why a dump could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlError(pub String);

impl fmt::Display for SqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SqlError {}

/// The tables read.
const TABLES: [&str; 12] = [
    "weenie",
    "weenie_properties_d_i_d",
    "weenie_properties_int",
    "weenie_properties_bool",
    "weenie_properties_float",
    "weenie_properties_string",
    "weenie_properties_position",
    "weenie_properties_generator",
    "landblock_instance",
    "landblock_instance_link",
    "event",
    "points_of_interest",
];

/// Reads the world database dumped in `input`.
///
/// # Errors
/// The input does not read, a statement of a table read does not parse, or the dump holds no
/// weenies or no placed instances.
pub fn read(input: impl BufRead) -> Result<WorldContent, SqlError> {
    let mut reader = Reader::default();
    split(input, |statement| reader.statement(statement))?;
    reader.finish()
}

/// Reads the world database dumped in `text`.
///
/// # Errors
/// As [`read`].
pub fn read_str(text: &str) -> Result<WorldContent, SqlError> {
    read(text.as_bytes())
}

/// A value of a row.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Null,
    Int(i64),
    Num(f64),
    Text(Vec<u8>),
}

impl Value {
    fn int(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            #[allow(clippy::cast_possible_truncation)]
            Self::Num(v) if v.fract() == 0.0 => Some(*v as i64),
            Self::Text(t) => std::str::from_utf8(t).ok()?.trim().parse().ok(),
            _ => None,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn num(&self) -> Option<f64> {
        match self {
            Self::Int(v) => Some(*v as f64),
            Self::Num(v) => Some(*v),
            Self::Text(t) => std::str::from_utf8(t).ok()?.trim().parse().ok(),
            Self::Null => None,
        }
    }

    fn bit(&self) -> Option<bool> {
        match self {
            Self::Text(t) if t.len() == 1 && t[0] <= 1 => Some(t[0] == 1),
            Self::Text(t) => match t.as_slice() {
                b"0" => Some(false),
                b"1" => Some(true),
                _ => None,
            },
            Self::Int(v) => Some(*v != 0),
            _ => None,
        }
    }

    fn text(&self) -> Option<String> {
        match self {
            Self::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
            Self::Int(v) => Some(v.to_string()),
            Self::Num(v) => Some(v.to_string()),
            Self::Null => None,
        }
    }
}

/// One row, by column name.
struct Row<'a> {
    columns: &'a [String],
    values: Vec<Value>,
}

impl Row<'_> {
    fn get(&self, column: &str) -> &Value {
        self.columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(column))
            .and_then(|i| self.values.get(i))
            .unwrap_or(&Value::Null)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn u32(&self, column: &str) -> Option<u32> {
        self.get(column).int().map(|v| v as u32)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn i32(&self, column: &str) -> Option<i32> {
        self.get(column).int().map(|v| v as i32)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn f32(&self, column: &str) -> Option<f32> {
        self.get(column).num().map(|v| v as f32)
    }

    fn spot(&self) -> Option<Spot> {
        let f = |c: &str| self.f32(c).unwrap_or(0.0);
        Some(Spot {
            cell: self.u32("obj_Cell_Id")?,
            origin: [f("origin_X"), f("origin_Y"), f("origin_Z")],
            rotation: [
                self.f32("angles_W").unwrap_or(1.0),
                f("angles_X"),
                f("angles_Y"),
                f("angles_Z"),
            ],
        })
    }
}

#[derive(Default)]
struct Reader {
    /// Each table's declared columns.
    columns: HashMap<String, Vec<String>>,
    weenies: HashMap<u32, Weenie>,
    placements: Vec<Placement>,
    links: Vec<(u32, u32)>,
    events: Vec<GameEvent>,
    landmarks: Vec<Landmark>,
}

impl Reader {
    fn statement(&mut self, s: &[u8]) -> Result<(), SqlError> {
        let mut p = Parser { s, i: 0 };
        p.space();
        if p.keyword("CREATE") {
            p.space();
            if !p.keyword("TABLE") {
                return Ok(());
            }
            p.space();
            if p.keyword("IF") {
                p.space();
                p.keyword("NOT");
                p.space();
                p.keyword("EXISTS");
                p.space();
            }
            let table = p.name()?;
            if TABLES.contains(&table.as_str()) {
                let columns = p.columns()?;
                self.columns.insert(table, columns);
            }
        } else if p.keyword("INSERT") {
            p.space();
            if p.keyword("IGNORE") {
                p.space();
            }
            if p.keyword("INTO") {
                p.space();
            }
            let table = p.name()?;
            if !TABLES.contains(&table.as_str()) {
                return Ok(());
            }
            p.space();
            let named = if p.peek() == Some(b'(') {
                Some(p.name_list()?)
            } else {
                None
            };
            p.space();
            if !p.keyword("VALUES") && !p.keyword("VALUE") {
                return Err(p.error(&format!("an INSERT into {table} without VALUES")));
            }
            let columns =
                match named {
                    Some(c) => c,
                    None => self.columns.get(&table).cloned().ok_or_else(|| {
                        SqlError(format!("rows of {table} before its CREATE TABLE"))
                    })?,
                };
            loop {
                p.space();
                if p.peek() != Some(b'(') {
                    break;
                }
                let values = p.tuple()?;
                if values.len() != columns.len() {
                    return Err(p.error(&format!(
                        "a row of {table} with {} values for {} columns",
                        values.len(),
                        columns.len()
                    )));
                }
                self.row(
                    &table,
                    &Row {
                        columns: &columns,
                        values,
                    },
                );
                p.space();
                if p.peek() == Some(b',') {
                    p.i += 1;
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    fn weenie_mut(&mut self, row: &Row<'_>) -> Option<&mut Weenie> {
        self.weenies.get_mut(&row.u32("object_Id")?)
    }

    fn row(&mut self, table: &str, row: &Row<'_>) {
        match table {
            "weenie" => {
                if let (Some(id), Some(kind)) = (row.u32("class_Id"), row.u32("type")) {
                    let name = row.get("class_Name").text().unwrap_or_default();
                    self.weenies.insert(id, Weenie::new(id, &name, kind));
                }
            }
            "weenie_properties_d_i_d" => {
                let kv = (row.u32("type"), row.u32("value"));
                if let (Some(w), (Some(k), Some(v))) = (self.weenie_mut(row), kv) {
                    w.dids.push((k, v));
                }
            }
            "weenie_properties_int" => {
                let kv = (row.u32("type"), row.i32("value"));
                if let (Some(w), (Some(k), Some(v))) = (self.weenie_mut(row), kv) {
                    w.ints.push((k, v));
                }
            }
            "weenie_properties_bool" => {
                let kv = (row.u32("type"), row.get("value").bit());
                if let (Some(w), (Some(k), Some(v))) = (self.weenie_mut(row), kv) {
                    w.bools.push((k, v));
                }
            }
            "weenie_properties_float" => {
                let kv = (row.u32("type"), row.get("value").num());
                if let (Some(w), (Some(k), Some(v))) = (self.weenie_mut(row), kv) {
                    w.floats.push((k, v));
                }
            }
            "weenie_properties_string" => {
                let kv = (row.u32("type"), row.get("value").text());
                if let (Some(w), (Some(k), Some(v))) = (self.weenie_mut(row), kv) {
                    w.strings.push((k, v));
                }
            }
            "weenie_properties_position" => {
                let kv = (row.u32("position_Type"), row.spot());
                if let (Some(w), (Some(k), Some(v))) = (self.weenie_mut(row), kv) {
                    w.positions.push((k, v));
                }
            }
            "weenie_properties_generator" => {
                let spawn = row.u32("weenie_Class_Id").map(|wcid| Spawn {
                    wcid,
                    probability: row.f32("probability").unwrap_or(1.0),
                    init_create: row.i32("init_Create").unwrap_or(1),
                    max_create: row.i32("max_Create").unwrap_or(1),
                    when_create: row.u32("when_Create").unwrap_or(2),
                    where_create: row.u32("where_Create").unwrap_or(4),
                    at: row.spot().unwrap_or(Spot::at(0, [0.0; 3])),
                });
                if let (Some(w), Some(spawn)) = (self.weenie_mut(row), spawn) {
                    w.spawns.push(spawn);
                }
            }
            "landblock_instance" => {
                if let (Some(guid), Some(wcid), Some(at)) =
                    (row.u32("guid"), row.u32("weenie_Class_Id"), row.spot())
                {
                    self.placements.push(Placement {
                        guid,
                        wcid,
                        at,
                        links: Vec::new(),
                    });
                }
            }
            "landblock_instance_link" => {
                if let (Some(parent), Some(child)) = (row.u32("parent_GUID"), row.u32("child_GUID"))
                {
                    self.links.push((parent, child));
                }
            }
            "event" => {
                if let Some(name) = row.get("name").text() {
                    self.events.push(GameEvent {
                        name,
                        state: row.i32("state").unwrap_or(0),
                        start: row.get("start_Time").int().unwrap_or(-1),
                        end: row.get("end_Time").int().unwrap_or(-1),
                    });
                }
            }
            "points_of_interest" => {
                if let (Some(name), Some(wcid)) =
                    (row.get("name").text(), row.u32("weenie_Class_Id"))
                {
                    self.landmarks.push(Landmark { name, wcid });
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Result<WorldContent, SqlError> {
        if self.weenies.is_empty() {
            return Err(SqlError("the dump holds no weenies".into()));
        }
        if self.placements.is_empty() {
            return Err(SqlError("the dump holds no placed instances".into()));
        }
        self.placements.sort_by_key(|p| p.guid);
        self.placements.dedup_by_key(|p| p.guid);
        self.links.sort_unstable();
        self.links.dedup();
        for (parent, child) in self.links {
            if let Ok(i) = self.placements.binary_search_by_key(&parent, |p| p.guid) {
                self.placements[i].links.push(child);
            }
        }
        Ok(WorldContent {
            weenies: self
                .weenies
                .into_iter()
                .map(|(k, v)| (k, Arc::new(v)))
                .collect(),
            placements: self.placements,
            events: self.events,
            landmarks: self.landmarks,
        })
    }
}

/// Calls `each` with every statement of `input`, comments left out, without its `;`.
fn split(
    mut input: impl BufRead,
    mut each: impl FnMut(&[u8]) -> Result<(), SqlError>,
) -> Result<(), SqlError> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Code,
        Quoted(u8),
        Escaped(u8),
        Name,
        LineComment,
        BlockComment,
    }
    let mut state = State::Code;
    let mut statement: Vec<u8> = Vec::new();
    // A byte held back: a `-`, `/` or `*` that may begin or end a comment.
    let mut held: Option<u8> = None;
    loop {
        let chunk = input
            .fill_buf()
            .map_err(|e| SqlError(format!("reading the dump: {e}")))?;
        if chunk.is_empty() {
            break;
        }
        let n = chunk.len();
        for &b in chunk {
            match state {
                State::Code => {
                    if let Some(h) = held.take() {
                        match (h, b) {
                            (b'-', b'-') => {
                                state = State::LineComment;
                                continue;
                            }
                            (b'/', b'*') => {
                                state = State::BlockComment;
                                continue;
                            }
                            _ => statement.push(h),
                        }
                    }
                    match b {
                        b'-' | b'/' => held = Some(b),
                        b'#' => state = State::LineComment,
                        b'\'' | b'"' => {
                            statement.push(b);
                            state = State::Quoted(b);
                        }
                        b'`' => {
                            statement.push(b);
                            state = State::Name;
                        }
                        b';' => {
                            each(&statement)?;
                            statement.clear();
                        }
                        _ => statement.push(b),
                    }
                }
                State::Quoted(q) => {
                    statement.push(b);
                    if b == b'\\' {
                        state = State::Escaped(q);
                    } else if b == q {
                        state = State::Code;
                    }
                }
                State::Escaped(q) => {
                    statement.push(b);
                    state = State::Quoted(q);
                }
                State::Name => {
                    statement.push(b);
                    if b == b'`' {
                        state = State::Code;
                    }
                }
                State::LineComment => {
                    if b == b'\n' {
                        statement.push(b'\n');
                        state = State::Code;
                    }
                }
                State::BlockComment => {
                    if held.take() == Some(b'*') && b == b'/' {
                        state = State::Code;
                        statement.push(b' ');
                    } else if b == b'*' {
                        held = Some(b);
                    }
                }
            }
        }
        input.consume(n);
    }
    if let Some(h) = held {
        if state == State::Code {
            statement.push(h);
        }
    }
    if statement.iter().any(|b| !b.is_ascii_whitespace()) {
        each(&statement)?;
    }
    Ok(())
}

/// A cursor over one statement.
struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> SqlError {
        let start = self.i.saturating_sub(40);
        let end = (self.i + 40).min(self.s.len());
        SqlError(format!(
            "{what}, at {:?}",
            String::from_utf8_lossy(&self.s[start..end])
        ))
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    /// Takes `word` (any case) when it is next, ended by a byte that is no part of a word.
    fn keyword(&mut self, word: &str) -> bool {
        let end = self.i + word.len();
        let fits = self
            .s
            .get(self.i..end)
            .is_some_and(|w| w.eq_ignore_ascii_case(word.as_bytes()));
        let ended = self
            .s
            .get(end)
            .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
        if fits && ended {
            self.i = end;
        }
        fits && ended
    }

    /// A name, quoted with backticks or bare.
    fn name(&mut self) -> Result<String, SqlError> {
        self.space();
        if self.peek() == Some(b'`') {
            let start = self.i + 1;
            let len = self.s[start..]
                .iter()
                .position(|&b| b == b'`')
                .ok_or_else(|| self.error("an unclosed name"))?;
            self.i = start + len + 1;
            return Ok(String::from_utf8_lossy(&self.s[start..start + len]).into_owned());
        }
        let start = self.i;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            self.i += 1;
        }
        if start == self.i {
            return Err(self.error("a name expected"));
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    /// `(name, name, …)`.
    fn name_list(&mut self) -> Result<Vec<String>, SqlError> {
        self.i += 1;
        let mut out = Vec::new();
        loop {
            self.space();
            out.push(self.name()?);
            self.space();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return Err(self.error("a column list")),
            }
        }
    }

    /// A `CREATE TABLE`'s column names in order, less generated columns, from its `(`.
    fn columns(&mut self) -> Result<Vec<String>, SqlError> {
        self.space();
        if self.peek() != Some(b'(') {
            return Err(self.error("a CREATE TABLE without columns"));
        }
        self.i += 1;
        let mut out = Vec::new();
        loop {
            // One definition: up to a `,` or the closing `)` at the outer depth.
            let start = self.i;
            let mut depth = 0usize;
            let mut quote: Option<u8> = None;
            loop {
                let Some(b) = self.peek() else {
                    return Err(self.error("an unclosed CREATE TABLE"));
                };
                self.i += 1;
                match (quote, b) {
                    (Some(q), b'\\') if q != b'`' => self.i += 1,
                    (Some(q), _) if b == q => quote = None,
                    (Some(_), _) => {}
                    (None, b'\'' | b'"' | b'`') => quote = Some(b),
                    (None, b'(') => depth += 1,
                    (None, b')') if depth > 0 => depth -= 1,
                    (None, b',' | b')') if depth == 0 => break,
                    _ => {}
                }
            }
            let definition = &self.s[start..self.i - 1];
            let text = String::from_utf8_lossy(definition);
            let text = text.trim();
            if let Some(rest) = text.strip_prefix('`') {
                let generated = text.to_ascii_uppercase().contains(" GENERATED ")
                    || text.to_ascii_uppercase().contains(" AS (");
                if let (Some(end), false) = (rest.find('`'), generated) {
                    out.push(rest[..end].to_owned());
                }
            }
            if self.s[self.i - 1] == b')' {
                return Ok(out);
            }
        }
    }

    /// `(value, value, …)`.
    fn tuple(&mut self) -> Result<Vec<Value>, SqlError> {
        self.i += 1;
        let mut out = Vec::new();
        loop {
            self.space();
            out.push(self.value()?);
            self.space();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => return Err(self.error("a value list")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, SqlError> {
        if self.keyword("_binary") || self.keyword("_utf8") || self.keyword("_utf8mb4") {
            self.space();
        }
        match self.peek() {
            Some(q @ (b'\'' | b'"')) => {
                self.i += 1;
                Ok(Value::Text(self.string(q)?))
            }
            Some(b'b' | b'B') if self.s.get(self.i + 1) == Some(&b'\'') => {
                self.i += 2;
                let bits = self.string(b'\'')?;
                let mut v = 0i64;
                for b in bits {
                    v = (v << 1) | i64::from(b == b'1');
                }
                Ok(Value::Int(v))
            }
            _ => {
                let start = self.i;
                while self
                    .peek()
                    .is_some_and(|b| !matches!(b, b',' | b')') && !b.is_ascii_whitespace())
                {
                    self.i += 1;
                }
                let token = std::str::from_utf8(&self.s[start..self.i])
                    .map_err(|_| self.error("a value that is not text"))?;
                if token.eq_ignore_ascii_case("NULL") {
                    Ok(Value::Null)
                } else if let Some(hex) = token
                    .strip_prefix("0x")
                    .or_else(|| token.strip_prefix("0X"))
                {
                    i64::from_str_radix(hex, 16)
                        .map(Value::Int)
                        .map_err(|_| self.error("a hexadecimal value"))
                } else if let Ok(v) = token.parse::<i64>() {
                    Ok(Value::Int(v))
                } else if let Ok(v) = token.parse::<f64>() {
                    Ok(Value::Num(v))
                } else if token.eq_ignore_ascii_case("TRUE") {
                    Ok(Value::Int(1))
                } else if token.eq_ignore_ascii_case("FALSE") {
                    Ok(Value::Int(0))
                } else {
                    Err(self.error("a value that does not read"))
                }
            }
        }
    }

    /// A string's bytes after its opening quote `q`, through its closing one.
    fn string(&mut self, q: u8) -> Result<Vec<u8>, SqlError> {
        let mut out = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.error("an unclosed string"));
            };
            self.i += 1;
            if b == b'\\' {
                let Some(e) = self.peek() else {
                    return Err(self.error("an unclosed string"));
                };
                self.i += 1;
                out.push(match e {
                    b'0' => 0,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 8,
                    b'Z' => 26,
                    other => other,
                });
            } else if b == q {
                if self.peek() == Some(q) {
                    self.i += 1;
                    out.push(q);
                } else {
                    return Ok(out);
                }
            } else {
                out.push(b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (reads the world database's dump format into the places' model).
    use super::*;
    use crate::content::{id, WorldSource};

    const DUMP: &str = "-- a dump\n\
/*!40101 SET NAMES utf8 */;\n\
CREATE TABLE `weenie` (\n  `class_Id` int(10) unsigned NOT NULL COMMENT 'Weenie, (wcid)',\n  `class_Name` varchar(100) NOT NULL,\n  `type` int(5) NOT NULL DEFAULT '0',\n  `last_Modified` datetime NOT NULL,\n  PRIMARY KEY (`class_Id`)\n) ENGINE=InnoDB;\n\
CREATE TABLE `landblock_instance` (\n  `guid` int(10) unsigned NOT NULL,\n  `landblock` int(5) GENERATED ALWAYS AS ((`obj_Cell_Id` >> 16)) VIRTUAL,\n  `weenie_Class_Id` int(10) unsigned NOT NULL,\n  `obj_Cell_Id` int(10) unsigned NOT NULL,\n  `origin_X` float NOT NULL,\n  `origin_Y` float NOT NULL,\n  `origin_Z` float NOT NULL,\n  `angles_W` float NOT NULL,\n  `angles_X` float NOT NULL,\n  `angles_Y` float NOT NULL,\n  `angles_Z` float NOT NULL,\n  `is_Link_Child` bit(1) NOT NULL,\n  `last_Modified` datetime NOT NULL\n);\n\
CREATE TABLE `weenie_properties_bool` (`id` int, `object_Id` int, `type` smallint, `value` bit(1));\n\
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);\n\
CREATE TABLE `recipe` (`id` int);\n\
CREATE TABLE `weenie_properties_d_i_d` (`id` int, `object_Id` int, `type` smallint, `value` int);\n\
INSERT INTO `weenie` VALUES (1000,'portalholtburg',7,'2005-02-09 10:00:00'),(1001,'lifestone',25,'2005-02-09 10:00:00');\n\
INSERT INTO `weenie_properties_d_i_d` VALUES (1,1000,1,33554867),(2,1000,8,100667499);\n\
INSERT INTO `weenie_properties_bool` VALUES (1,1000,24,'\\0'),(2,1001,24,'\x01'),(3,1001,19,b'0');\n\
insert  into `weenie_properties_string`(`id`,`object_Id`,`type`,`value`) values \n(1,1000,1,'Holtburg\\'s; \"Portal\"'),\n(2,1001,1,'Life -- Stone /* not a comment */');\n\
INSERT INTO `recipe` VALUES (1);\n\
# a comment; with a semicolon\n\
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES (2,1001,2847146017,84,7.5,94.005,1,0,0,0,'\\0','2005-02-09'),(1,1000,0xA9B40000,-1.5e1,NULL,0,1,0,0,0,'\\0','2005-02-09');\n\
UPDATE `weenie` SET `type` = 1;\n";

    #[test]
    fn a_dump_reads_its_weenies_properties_and_placements_whatever_its_layout() {
        let world = read_str(DUMP).expect("reads");
        let portal = world.weenie(1000).expect("the portal");
        assert_eq!(
            (portal.class_name.as_str(), portal.weenie_type),
            ("portalholtburg", 7)
        );
        assert_eq!(portal.name(), "Holtburg's; \"Portal\"");
        assert_eq!(portal.flag(id::bool::UI_HIDDEN), Some(false));
        assert_eq!(portal.did(id::did::SETUP), Some(0x0200_01B3));
        assert_eq!(portal.did(8), Some(0x0600_106B));
        let stone = world.weenie(1001).expect("the lifestone");
        assert_eq!(stone.flag(id::bool::UI_HIDDEN), Some(true));
        assert_eq!(stone.flag(id::bool::ATTACKABLE), Some(false));
        assert_eq!(stone.name(), "Life -- Stone /* not a comment */");
        // Placements in guid order, the generated column left out of the declared order, a
        // hexadecimal cell, a float in exponent form and a NULL read.
        let p = world.placements();
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].guid, p[0].wcid, p[0].at.cell), (1, 1000, 0xA9B4_0000));
        assert_eq!(p[0].at.origin, [-15.0, 0.0, 0.0]);
        assert_eq!((p[1].guid, p[1].at.cell), (2, 0xA9B4_0021));
        assert_eq!(p[1].at.origin, [84.0, 7.5, 94.005]);
    }

    #[test]
    fn a_dump_that_does_not_read_says_where() {
        let bad = "CREATE TABLE `weenie` (`class_Id` int, `class_Name` text, `type` int);\n\
INSERT INTO `weenie` VALUES (1,'a');\n";
        let e = read_str(bad).unwrap_err();
        assert!(e.0.contains("1 values") || e.0.contains("2 values"), "{e}");
        let unclosed = "CREATE TABLE `weenie` (`class_Id` int, `class_Name` text, `type` int);\n\
INSERT INTO `weenie` VALUES (1,'a,7);\n";
        assert!(read_str(unclosed).is_err());
        assert_eq!(
            read_str("SET NAMES utf8;").unwrap_err(),
            SqlError("the dump holds no weenies".into())
        );
        let no_create = "INSERT INTO `weenie` VALUES (1,'a',7);";
        assert!(read_str(no_create)
            .unwrap_err()
            .0
            .contains("before its CREATE TABLE"));
    }

    #[test]
    fn a_dump_read_a_few_bytes_at_a_time_reads_the_same() {
        let whole = read_str(DUMP).unwrap();
        let slow = read(std::io::BufReader::with_capacity(3, DUMP.as_bytes())).unwrap();
        assert_eq!(whole.placements, slow.placements);
        assert_eq!(whole.weenies.len(), slow.weenies.len());
        for (k, w) in &whole.weenies {
            assert_eq!(Some(w), slow.weenies.get(k));
        }
    }
}
