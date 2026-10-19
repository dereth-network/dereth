//! A pack's server content as the world editor edits it: ACE SQL, held as the statements it was
//! written with.
//!
//! [`SqlDoc`] reads a file into its pieces: the text between statements (comments, `DELETE`,
//! `SET @parent_id = LAST_INSERT_ID();`) kept as it is, and each `INSERT` with a column list kept as
//! its table, its columns and its rows. A row keeps the text it was written with and its trailing
//! comment, so a document written back unchanged is the same text, byte for byte. A row that changes
//! is written again in the layout of ACE's own export (`VALUES (...) /* comment */`, a row to a
//! line), and so is a row or a statement added.
//!
//! Over the document a weenie is read and edited ([`SqlDoc::property`], [`SqlDoc::set_property`],
//! the rows of any of its tables, and its emotes as a tree, [`SqlDoc::emote_sets`]): a statement a
//! change needs is added in ACE's order of the weenie's tables ([`TABLE_ORDER`]), and one left with
//! no rows goes.
//!
//! [`Schema`] is the names: what the server calls each property and the values of the enums the
//! forms show. It is data a server tool writes; without it the forms show numbers.

use std::collections::BTreeMap;

/// The order ACE's export writes a weenie's tables in; a statement a change adds goes before the
/// first statement of a table later in this order.
pub const TABLE_ORDER: [&str; 23] = [
    "weenie",
    "weenie_properties_int",
    "weenie_properties_int64",
    "weenie_properties_bool",
    "weenie_properties_float",
    "weenie_properties_string",
    "weenie_properties_d_i_d",
    "weenie_properties_position",
    "weenie_properties_i_i_d",
    "weenie_properties_attribute",
    "weenie_properties_attribute_2nd",
    "weenie_properties_skill",
    "weenie_properties_body_part",
    "weenie_properties_spell_book",
    "weenie_properties_event_filter",
    "weenie_properties_emote",
    "weenie_properties_create_list",
    "weenie_properties_book",
    "weenie_properties_book_page_data",
    "weenie_properties_generator",
    "weenie_properties_palette",
    "weenie_properties_texture_map",
    "weenie_properties_anim_part",
];

/// The table emote actions are written in, after the emote they belong to.
pub const EMOTE_ACTIONS: &str = "weenie_properties_emote_action";

/// A property's type: which table holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyKind {
    Int,
    Int64,
    Bool,
    Float,
    String,
    DataId,
    InstanceId,
}

impl PropertyKind {
    pub const ALL: [Self; 7] = [
        Self::Int,
        Self::Int64,
        Self::Bool,
        Self::Float,
        Self::String,
        Self::DataId,
        Self::InstanceId,
    ];

    /// The table that holds properties of this type.
    #[must_use]
    pub const fn table(self) -> &'static str {
        match self {
            Self::Int => "weenie_properties_int",
            Self::Int64 => "weenie_properties_int64",
            Self::Bool => "weenie_properties_bool",
            Self::Float => "weenie_properties_float",
            Self::String => "weenie_properties_string",
            Self::DataId => "weenie_properties_d_i_d",
            Self::InstanceId => "weenie_properties_i_i_d",
        }
    }

    /// The schema's key for this type.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Int64 => "int64",
            Self::Bool => "bool",
            Self::Float => "float",
            Self::String => "string",
            Self::DataId => "did",
            Self::InstanceId => "iid",
        }
    }

    /// The type a table holds, for a properties table.
    #[must_use]
    pub fn of_table(table: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.table() == table)
    }

    /// `raw` as this type's value, written as the export writes it; `None` when it is not one.
    #[must_use]
    pub fn value(self, raw: &str) -> Option<String> {
        let v = raw.trim();
        match self {
            Self::Int => v.parse::<i32>().ok().map(|n| n.to_string()),
            Self::Int64 => v.parse::<i64>().ok().map(|n| n.to_string()),
            // A dump writes a bit as its byte, quoted.
            Self::Bool => match unquote(v).to_ascii_lowercase().as_str() {
                "true" | "1" | "\u{1}" => Some("True".into()),
                "false" | "0" | "\u{0}" => Some("False".into()),
                _ => None,
            },
            Self::Float => v
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .map(|_| v.to_owned()),
            Self::String => Some(quote(&unquote(v))),
            Self::DataId | Self::InstanceId => {
                if v.starts_with("{{") && v.ends_with("}}") {
                    return Some(v.to_owned());
                }
                crate::sql_ids::number(v).map(|n| format!("0x{n:08X}"))
            }
        }
    }
}

/// A string as an SQL literal: single quotes, a quote or a backslash in it escaped.
#[must_use]
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// A value as written, without its comments and its quotes: the text of a string, `NULL` as empty.
#[must_use]
pub fn unquote(raw: &str) -> String {
    let v = bare(raw);
    let Some(inner) = v
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| v.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
    else {
        return if v.eq_ignore_ascii_case("NULL") {
            String::new()
        } else {
            v
        };
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some(n) => out.push(n),
                None => {}
            },
            '\'' if chars.peek() == Some(&'\'') => {
                chars.next();
                out.push('\'');
            }
            _ => out.push(c),
        }
    }
    out
}

/// A value as written, its comments taken out and its ends trimmed.
#[must_use]
pub fn bare(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let b = raw.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            out.push(char::from(c));
            if c == b'\\' && i + 1 < b.len() {
                out.push(char::from(b[i + 1]));
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'\'' || c == b'"' {
            quote = Some(c);
            out.push(char::from(c));
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = raw[i + 2..].find("*/").map_or(b.len(), |j| i + 2 + j + 2);
        } else {
            // Copy the whole character, not one byte of it.
            let ch = raw[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out.trim().to_owned()
}

/// One row of an `INSERT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Each value exactly as written between its commas: the white space that pads it and the
    /// comments in it kept.
    pub values: Vec<String>,
    /// The comment written after the row, without its `/*` and `*/`.
    pub comment: Option<String>,
    /// What stands between the row before and this one (`\n     , `), as written.
    sep: String,
    /// The row as written (its values and its comment), until it changes.
    text: Option<String>,
}

impl Row {
    fn new(values: Vec<String>, comment: Option<String>) -> Self {
        Self {
            values: values
                .into_iter()
                .enumerate()
                .map(|(i, v)| lead(i, v))
                .collect(),
            comment,
            sep: "\n     , ".into(),
            text: None,
        }
    }

    fn render(&self) -> String {
        if let Some(t) = &self.text {
            return t.clone();
        }
        let mut s = format!("({})", self.values.join(","));
        if let Some(c) = &self.comment {
            s.push_str(&format!(" /* {c} */"));
        }
        s
    }
}

/// A value as a row writes it after the one before: set off by a space, unless it is the first or
/// already padded.
fn lead(i: usize, v: String) -> String {
    if i > 0 && !v.starts_with(char::is_whitespace) {
        format!(" {v}")
    } else {
        v
    }
}

/// An `INSERT` with its column list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Insert {
    pub table: String,
    pub columns: Vec<String>,
    /// From `INSERT` up to the first row, as written.
    head: String,
    pub rows: Vec<Row>,
    /// From the last row to the `;`, as written.
    tail: String,
}

impl Insert {
    /// A new statement of `table`, laid out as ACE's export lays it out.
    #[must_use]
    pub fn new(table: &str, columns: &[&str]) -> Self {
        let cols: Vec<String> = columns.iter().map(|c| format!("`{c}`")).collect();
        Self {
            table: table.to_owned(),
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            head: format!("INSERT INTO `{table}` ({})\nVALUES ", cols.join(", ")),
            rows: Vec::new(),
            tail: ";".into(),
        }
    }

    /// The place of column `name`.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    fn render(&self, out: &mut String) {
        out.push_str(&self.head);
        for (i, r) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push_str(&r.sep);
            }
            out.push_str(&r.render());
        }
        out.push_str(&self.tail);
    }
}

/// A piece of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Insert(Insert),
}

/// Where a row is: its statement's place in the document and its own in the statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowRef {
    pub piece: usize,
    pub row: usize,
}

/// One emote set: its row, and the statement of its actions when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmoteSet {
    pub emote: RowRef,
    pub actions: Option<usize>,
}

/// A file of ACE SQL, held as its statements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlDoc {
    pub pieces: Vec<Piece>,
}

/// The end of the comment, string or other text that starts at `i`, for the scanners: `i` itself
/// when nothing starts there.
fn skip_quoted(b: &[u8], i: usize) -> usize {
    match b[i] {
        q @ (b'\'' | b'"' | b'`') => {
            let mut j = i + 1;
            while j < b.len() {
                if b[j] == b'\\' && q != b'`' {
                    j += 2;
                    continue;
                }
                if b[j] == q {
                    if b.get(j + 1) == Some(&q) && q != b'`' {
                        j += 2;
                        continue;
                    }
                    return j + 1;
                }
                j += 1;
            }
            b.len()
        }
        b'/' if b.get(i + 1) == Some(&b'*') => {
            let mut j = i + 2;
            while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                j += 1;
            }
            (j + 2).min(b.len())
        }
        b'-' if b.get(i + 1) == Some(&b'-') => {
            let mut j = i;
            while j < b.len() && b[j] != b'\n' {
                j += 1;
            }
            j
        }
        b'#' => {
            let mut j = i;
            while j < b.len() && b[j] != b'\n' {
                j += 1;
            }
            j
        }
        _ => i,
    }
}

/// The first byte from `i` that is not white space or a comment.
fn skip_space(b: &[u8], mut i: usize) -> usize {
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && (b[i] == b'/' || b[i] == b'-' || b[i] == b'#') {
            let j = skip_quoted(b, i);
            if j > i {
                i = j;
                continue;
            }
        }
        return i;
    }
}

/// The byte after the `;` that ends the statement at `i` (or the end).
fn statement_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        let j = skip_quoted(b, i);
        if j > i {
            i = j;
            continue;
        }
        if b[i] == b';' {
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

/// The statements of `text`, in order: each one's bytes, from the end of the one before (the
/// space and comments before it with it) through its `;`, and where its first word starts. Text
/// after the last statement is a piece of its own whose first word is its end. Together the
/// pieces are the whole text.
pub(crate) fn statements(text: &str) -> Vec<(std::ops::Range<usize>, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let s = skip_space(b, i);
        if s >= b.len() {
            out.push((i..b.len(), b.len()));
            break;
        }
        let end = statement_end(b, s);
        out.push((i..end, s));
        i = end;
    }
    out
}

/// Whether the word at `i` is `word`, whatever its case, with no word character after it.
fn word_at(b: &[u8], i: usize, word: &str) -> bool {
    let w = word.as_bytes();
    b.len() >= i + w.len()
        && b[i..i + w.len()].eq_ignore_ascii_case(w)
        && b.get(i + w.len())
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
}

/// A name, backquoted or not, at `i`: the name and the byte after it.
fn name_at(t: &str, i: usize) -> Option<(String, usize)> {
    let b = t.as_bytes();
    if b.get(i) == Some(&b'`') {
        let end = t[i + 1..].find('`')? + i + 1;
        return Some((t[i + 1..end].to_owned(), end + 1));
    }
    let mut j = i;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    (j > i).then(|| (t[i..j].to_owned(), j))
}

/// The values of a parenthesised list whose `(` is at `i`, as written, and the byte after its `)`.
fn list_at(t: &str, i: usize) -> Option<(Vec<String>, usize)> {
    let b = t.as_bytes();
    if b.get(i) != Some(&b'(') {
        return None;
    }
    let mut values = Vec::new();
    let mut start = i + 1;
    let mut depth = 0;
    let mut j = i + 1;
    while j < b.len() {
        let k = skip_quoted(b, j);
        if k > j {
            j = k;
            continue;
        }
        match b[j] {
            b'(' => depth += 1,
            b')' if depth > 0 => depth -= 1,
            b')' => {
                values.push(t[start..j].to_owned());
                return Some((values, j + 1));
            }
            b',' if depth == 0 => {
                values.push(t[start..j].to_owned());
                start = j + 1;
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// The `INSERT` at `start` with a column list: the statement and the byte after its `;`. `None`
/// for any other statement, which is kept as text.
fn insert_at(t: &str, start: usize) -> Option<(Insert, usize)> {
    let b = t.as_bytes();
    if !word_at(b, start, "INSERT") {
        return None;
    }
    let mut i = skip_space(b, start + 6);
    if !word_at(b, i, "INTO") {
        return None;
    }
    i = skip_space(b, i + 4);
    let (table, after) = name_at(t, i)?;
    i = skip_space(b, after);
    let (cols, after) = list_at(t, i)?;
    let columns: Vec<String> = cols
        .iter()
        .map(|c| c.trim().trim_matches('`').to_owned())
        .collect();
    i = skip_space(b, after);
    if !word_at(b, i, "VALUES") {
        return None;
    }
    i = skip_space(b, i + 6);
    let head = t[start..i].to_owned();
    let mut rows = Vec::new();
    let mut sep = String::new();
    loop {
        let row_start = i;
        let (values, after) = list_at(t, i)?;
        // The row's own comment: on its line, before the `,` or `;` that ends it.
        let mut end = after;
        let mut comment = None;
        let mut k = after;
        while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
            k += 1;
        }
        if t[k..].starts_with("/*") {
            let close = t[k + 2..].find("*/")? + k + 2;
            comment = Some(t[k + 2..close].trim().to_owned());
            end = close + 2;
        }
        rows.push(Row {
            values,
            comment,
            sep: std::mem::take(&mut sep),
            text: Some(t[row_start..end].to_owned()),
        });
        let next = skip_space(b, end);
        match b.get(next) {
            Some(b',') => {
                let after_comma = skip_space(b, next + 1);
                sep = t[end..after_comma].to_owned();
                i = after_comma;
            }
            Some(b';') => {
                return Some((
                    Insert {
                        table,
                        columns,
                        head,
                        rows,
                        tail: t[end..=next].to_owned(),
                    },
                    next + 1,
                ));
            }
            _ => return None,
        }
    }
}

impl SqlDoc {
    /// `text` read into its pieces.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let b = text.as_bytes();
        let mut pieces = Vec::new();
        let mut text_start = 0;
        let mut i = 0;
        while i < b.len() {
            let s = skip_space(b, i);
            if s >= b.len() {
                break;
            }
            if let Some((insert, end)) = insert_at(text, s) {
                if s > text_start {
                    pieces.push(Piece::Text(text[text_start..s].to_owned()));
                }
                pieces.push(Piece::Insert(insert));
                text_start = end;
                i = end;
            } else {
                i = statement_end(b, s);
            }
        }
        if text_start < text.len() {
            pieces.push(Piece::Text(text[text_start..].to_owned()));
        }
        Self { pieces }
    }

    /// The document as text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for p in &self.pieces {
            match p {
                Piece::Text(t) => out.push_str(t),
                Piece::Insert(i) => i.render(&mut out),
            }
        }
        out
    }

    fn insert(&self, piece: usize) -> Option<&Insert> {
        match self.pieces.get(piece)? {
            Piece::Insert(i) => Some(i),
            Piece::Text(_) => None,
        }
    }

    fn insert_mut(&mut self, piece: usize) -> Option<&mut Insert> {
        match self.pieces.get_mut(piece)? {
            Piece::Insert(i) => Some(i),
            Piece::Text(_) => None,
        }
    }

    /// The statements of `table`, by their place in the document.
    #[must_use]
    pub fn inserts(&self, table: &str) -> Vec<usize> {
        self.pieces
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                Piece::Insert(s) if s.table == table => Some(i),
                _ => None,
            })
            .collect()
    }

    /// Every row of `table`, in order.
    #[must_use]
    pub fn rows(&self, table: &str) -> Vec<RowRef> {
        self.inserts(table)
            .into_iter()
            .flat_map(|piece| {
                let n = self.insert(piece).map_or(0, |s| s.rows.len());
                (0..n).map(move |row| RowRef { piece, row })
            })
            .collect()
    }

    /// The value of `column` in row `r`, as written (its padding trimmed).
    #[must_use]
    pub fn raw(&self, r: RowRef, column: &str) -> Option<&str> {
        let s = self.insert(r.piece)?;
        let c = s.column(column)?;
        s.rows.get(r.row)?.values.get(c).map(|v| v.trim())
    }

    /// The value of `column` in row `r`, its comments taken out.
    #[must_use]
    pub fn cell(&self, r: RowRef, column: &str) -> Option<String> {
        self.raw(r, column).map(bare)
    }

    /// The row `r`'s comment.
    #[must_use]
    pub fn comment(&self, r: RowRef) -> Option<&str> {
        self.insert(r.piece)?.rows.get(r.row)?.comment.as_deref()
    }

    /// Set `column` of row `r` to `raw` (as it is to be written); the row is written again.
    pub fn set_cell(&mut self, r: RowRef, column: &str, raw: &str) -> bool {
        let Some(s) = self.insert_mut(r.piece) else {
            return false;
        };
        let Some(c) = s.column(column) else {
            return false;
        };
        let Some(row) = s.rows.get_mut(r.row) else {
            return false;
        };
        let v = lead(c, raw.to_owned());
        if row.values[c] == v {
            return true;
        }
        row.values[c] = v;
        row.text = None;
        true
    }

    /// Set row `r`'s comment; the row is written again.
    pub fn set_comment(&mut self, r: RowRef, comment: Option<String>) {
        if let Some(row) = self.insert_mut(r.piece).and_then(|s| s.rows.get_mut(r.row)) {
            if row.comment != comment {
                row.comment = comment;
                row.text = None;
            }
        }
    }

    /// The class the document's `weenie` row names, as written (`{{wcid:guide}}` or a number).
    #[must_use]
    pub fn class(&self) -> Option<String> {
        self.rows("weenie")
            .first()
            .and_then(|r| self.cell(*r, "class_Id"))
    }

    /// Where a new statement of `table` goes: before the first piece of a later table (and the
    /// text that leads to it), else at the end.
    fn place_for(&self, table: &str) -> usize {
        let rank = |t: &str| {
            if t == EMOTE_ACTIONS {
                return TABLE_ORDER
                    .iter()
                    .position(|x| *x == "weenie_properties_emote");
            }
            TABLE_ORDER.iter().position(|x| *x == t)
        };
        let Some(mine) = rank(table) else {
            return self.pieces.len();
        };
        for (i, p) in self.pieces.iter().enumerate() {
            if let Piece::Insert(s) = p {
                if rank(&s.table).is_some_and(|r| r > mine) {
                    // Back over the text that leads to it, comments and the blank line.
                    let mut at = i;
                    while at > 0 && matches!(self.pieces[at - 1], Piece::Text(_)) {
                        at -= 1;
                    }
                    return at;
                }
            }
        }
        self.pieces.len()
    }

    /// Put `new` in at `at`, set off from what is around it by a blank line.
    fn put(&mut self, at: usize, new: Vec<Piece>) {
        let mut list = Vec::with_capacity(new.len() + 2);
        if at > 0 {
            list.push(Piece::Text("\n\n".into()));
        }
        list.extend(new);
        list.push(Piece::Text(if at < self.pieces.len() {
            "\n\n".into()
        } else {
            "\n".into()
        }));
        self.pieces.splice(at..at, list);
        self.merge_text();
    }

    /// Text pieces side by side made one, and a piece of nothing dropped.
    fn merge_text(&mut self) {
        let mut out: Vec<Piece> = Vec::with_capacity(self.pieces.len());
        for p in std::mem::take(&mut self.pieces) {
            match (out.last_mut(), p) {
                (_, Piece::Text(t)) if t.is_empty() => {}
                (Some(Piece::Text(a)), Piece::Text(b)) => {
                    // Never more than one blank line between statements.
                    a.push_str(&b);
                    while a.contains("\n\n\n") {
                        *a = a.replace("\n\n\n", "\n\n");
                    }
                }
                (_, p) => out.push(p),
            }
        }
        // A file ends as ACE's export ends it: its last statement and a line break.
        let many = out.len() > 1;
        if let Some(Piece::Text(t)) = out.last_mut() {
            if t.trim().is_empty() && many {
                "\n".clone_into(t);
            }
        }
        self.pieces = out;
    }

    /// Add a row of `values` (`column` to value as written; every other column `NULL`, the class
    /// column the document's class) to `table`: into its statement, in order of `order_by` when
    /// given, else after its last row; or into a new statement put in ACE's order.
    pub fn add_row(
        &mut self,
        table: &str,
        columns: &[&str],
        values: &[(&str, String)],
        order_by: Option<&str>,
        comment: Option<String>,
    ) -> RowRef {
        let class = self.class().unwrap_or_default();
        let build = |cols: &[String]| -> Vec<String> {
            cols.iter()
                .map(|c| {
                    values
                        .iter()
                        .find(|(k, _)| *k == c)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| {
                            if c == "object_Id" {
                                class.clone()
                            } else {
                                "NULL".into()
                            }
                        })
                })
                .collect()
        };
        if let Some(&piece) = self.inserts(table).last() {
            let key = |doc: &Self, r: RowRef| {
                order_by
                    .and_then(|c| doc.cell(r, c))
                    .and_then(|v| v.parse::<i64>().ok())
            };
            let mine = order_by.and_then(|c| {
                values
                    .iter()
                    .find(|(k, _)| *k == c)
                    .and_then(|(_, v)| bare(v).parse::<i64>().ok())
            });
            let s = self.insert(piece).expect("an insert");
            let row = Row::new(build(&s.columns), comment);
            let at = match mine {
                Some(m) => (0..s.rows.len())
                    .find(|i| key(self, RowRef { piece, row: *i }).is_some_and(|k| k > m))
                    .unwrap_or(s.rows.len()),
                None => s.rows.len(),
            };
            let s = self.insert_mut(piece).expect("an insert");
            if at == 0 && !s.rows.is_empty() {
                // The new row is the first: it takes the old first row's place after the head.
                let mut row = row;
                row.sep = std::mem::take(&mut s.rows[0].sep);
                s.rows[0].sep = "\n     , ".into();
                s.rows.insert(0, row);
            } else {
                let mut row = row;
                if s.rows.is_empty() {
                    row.sep = String::new();
                }
                s.rows.insert(at, row);
            }
            return RowRef { piece, row: at };
        }
        let mut s = Insert::new(table, columns);
        let mut row = Row::new(build(&s.columns), comment);
        row.sep = String::new();
        s.rows.push(row);
        let at = self.place_for(table);
        self.put(at, vec![Piece::Insert(s)]);
        let piece = *self.inserts(table).last().expect("the statement just put");
        RowRef { piece, row: 0 }
    }

    /// Take row `r` away; its statement goes when it has no row left (with the text that leads
    /// to it, up to the blank line before).
    pub fn remove_row(&mut self, r: RowRef) {
        let Some(s) = self.insert_mut(r.piece) else {
            return;
        };
        if r.row >= s.rows.len() {
            return;
        }
        let gone = s.rows.remove(r.row);
        if r.row == 0 {
            if let Some(first) = s.rows.first_mut() {
                first.sep = gone.sep;
            }
        }
        if s.rows.is_empty() {
            self.remove_piece(r.piece);
        }
    }

    /// Take statement `piece` away, with the blank line before it.
    fn remove_piece(&mut self, piece: usize) {
        self.pieces.remove(piece);
        if let Some(Piece::Text(t)) = piece.checked_sub(1).and_then(|i| self.pieces.get_mut(i)) {
            if t.ends_with("\n\n") {
                t.truncate(t.len() - 1);
            }
        }
        self.merge_text();
    }

    // -------------------------------------------------------------------------------------------
    // properties
    // -------------------------------------------------------------------------------------------

    /// The row of property `id` of type `kind`.
    #[must_use]
    pub fn property_row(&self, kind: PropertyKind, id: u32) -> Option<RowRef> {
        self.rows(kind.table())
            .into_iter()
            .find(|r| self.cell(*r, "type").and_then(|v| v.parse::<u32>().ok()) == Some(id))
    }

    /// Property `id` of type `kind`, as written (comments taken out).
    #[must_use]
    pub fn property(&self, kind: PropertyKind, id: u32) -> Option<String> {
        self.property_row(kind, id)
            .and_then(|r| self.cell(r, "value"))
    }

    /// Every property of type `kind`: its id and its value as written.
    #[must_use]
    pub fn properties(&self, kind: PropertyKind) -> Vec<(u32, String)> {
        self.rows(kind.table())
            .into_iter()
            .filter_map(|r| Some((self.cell(r, "type")?.parse().ok()?, self.cell(r, "value")?)))
            .collect()
    }

    /// Set property `id` of type `kind` to `value` (a value of that type, written as the export
    /// writes it); `comment` names it. Refused (false) for a value that is not of the type.
    pub fn set_property(
        &mut self,
        kind: PropertyKind,
        id: u32,
        value: &str,
        comment: Option<String>,
    ) -> bool {
        let Some(v) = kind.value(value) else {
            return false;
        };
        let padded = |v: &str| match kind {
            PropertyKind::Bool => format!(" {v:<5}"),
            PropertyKind::String | PropertyKind::DataId | PropertyKind::InstanceId => v.to_owned(),
            _ => format!(" {v:>10}"),
        };
        if let Some(r) = self.property_row(kind, id) {
            self.set_cell(r, "value", &padded(&v));
            if comment.is_some() {
                self.set_comment(r, comment);
            } else if let Some(c) = self.comment(r).map(str::to_owned) {
                // A name stays; a value's description after it may no longer hold.
                let name = c.split(" - ").next().unwrap_or("").to_owned();
                self.set_comment(r, (!name.is_empty()).then_some(name));
            }
            return true;
        }
        self.add_row(
            kind.table(),
            &["object_Id", "type", "value"],
            &[("type", format!(" {id:>3}")), ("value", padded(&v))],
            Some("type"),
            comment,
        );
        true
    }

    /// Take property `id` of type `kind` away; false when there is none.
    pub fn remove_property(&mut self, kind: PropertyKind, id: u32) -> bool {
        let Some(r) = self.property_row(kind, id) else {
            return false;
        };
        self.remove_row(r);
        true
    }

    // -------------------------------------------------------------------------------------------
    // emotes
    // -------------------------------------------------------------------------------------------

    /// The emote sets, in the order they are written: each emote row and the statement of actions
    /// that follows it (after `SET @parent_id = LAST_INSERT_ID();`), when there is one.
    #[must_use]
    pub fn emote_sets(&self) -> Vec<EmoteSet> {
        let mut out: Vec<EmoteSet> = Vec::new();
        for (i, p) in self.pieces.iter().enumerate() {
            let Piece::Insert(s) = p else { continue };
            if s.table == "weenie_properties_emote" {
                for row in 0..s.rows.len() {
                    out.push(EmoteSet {
                        emote: RowRef { piece: i, row },
                        actions: None,
                    });
                }
            } else if s.table == EMOTE_ACTIONS {
                // `LAST_INSERT_ID()` is the first row of the emote statement before.
                if let Some(set) = out
                    .iter_mut()
                    .rev()
                    .find(|e| e.emote.row == 0 && e.emote.piece < i)
                {
                    if set.actions.is_none() {
                        set.actions = Some(i);
                    }
                }
            }
        }
        out
    }

    /// The actions of emote set `set`, in order.
    #[must_use]
    pub fn actions(&self, set: &EmoteSet) -> Vec<RowRef> {
        let Some(piece) = set.actions else {
            return Vec::new();
        };
        let n = self.insert(piece).map_or(0, |s| s.rows.len());
        (0..n).map(|row| RowRef { piece, row }).collect()
    }

    /// Add an emote set of `category` (as written, with its comment) after the last one (or in
    /// ACE's order), with `fields` set and no actions. Answers its place in [`Self::emote_sets`].
    pub fn add_emote_set(&mut self, category: &str, fields: &[(&str, String)]) -> usize {
        let mut values: Vec<(&str, String)> = vec![
            ("category", category.to_owned()),
            ("probability", "1".into()),
        ];
        for (k, v) in fields {
            values.retain(|(c, _)| c != k);
            values.push((k, v.clone()));
        }
        let mut s = Insert::new("weenie_properties_emote", &EMOTE_COLUMNS);
        let class = self.class().unwrap_or_default();
        let mut row = Row::new(
            EMOTE_COLUMNS
                .iter()
                .map(|c| {
                    values
                        .iter()
                        .find(|(k, _)| k == c)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| {
                            if *c == "object_Id" {
                                class.clone()
                            } else {
                                "NULL".into()
                            }
                        })
                })
                .collect(),
            None,
        );
        row.sep = String::new();
        s.rows.push(row);
        // After the last set's actions, else where the table goes.
        let at = match self.emote_sets().last() {
            Some(last) => last.actions.unwrap_or(last.emote.piece) + 1,
            None => self.place_for("weenie_properties_emote"),
        };
        self.put(at, vec![Piece::Insert(s)]);
        self.emote_sets().len() - 1
    }

    /// Take emote set `index` away, with its actions.
    pub fn remove_emote_set(&mut self, index: usize) {
        let Some(set) = self.emote_sets().get(index).copied() else {
            return;
        };
        if let Some(a) = set.actions {
            self.remove_piece(a);
            // The `SET @parent_id` line before it goes too.
            if let Some(Piece::Text(t)) = a.checked_sub(1).and_then(|i| self.pieces.get_mut(i)) {
                if let Some(at) = t.rfind("SET @parent_id = LAST_INSERT_ID();") {
                    t.replace_range(at.., "");
                }
            }
            self.merge_text();
        }
        self.remove_row(set.emote);
    }

    /// Add an action to emote set `index` at `at` (its order), with `fields` set and every other
    /// column `NULL`; the actions after it are numbered again.
    pub fn add_action(&mut self, index: usize, at: usize, fields: &[(&str, String)]) {
        let Some(set) = self.emote_sets().get(index).copied() else {
            return;
        };
        let piece = if let Some(p) = set.actions {
            p
        } else {
            let mut s = Insert::new(EMOTE_ACTIONS, &ACTION_COLUMNS);
            s.rows.clear();
            // Its own statement after the set's, as the export writes it.
            let after = set.emote.piece + 1;
            self.put(
                after,
                vec![
                    Piece::Text("SET @parent_id = LAST_INSERT_ID();\n\n".into()),
                    Piece::Insert(s),
                ],
            );
            self.emote_sets()[index]
                .actions
                .expect("the actions just added")
        };
        let s = self.insert(piece).expect("the actions");
        let cols = s.columns.clone();
        let n = s.rows.len();
        let at = at.min(n);
        let mut row = Row::new(
            cols.iter()
                .map(|c| match c.as_str() {
                    "emote_Id" => "@parent_id".into(),
                    "order" => format!(" {at:>2}"),
                    _ => fields
                        .iter()
                        .find(|(k, _)| *k == c)
                        .map_or_else(|| "NULL".into(), |(_, v)| v.clone()),
                })
                .collect(),
            None,
        );
        let s = self.insert_mut(piece).expect("the actions");
        if s.rows.is_empty() {
            row.sep = String::new();
        } else if at == 0 {
            row.sep = std::mem::take(&mut s.rows[0].sep);
            s.rows[0].sep = "\n     , ".into();
        }
        s.rows.insert(at, row);
        self.renumber(piece);
    }

    /// Take action `at` of emote set `index` away; the rest are numbered again.
    pub fn remove_action(&mut self, index: usize, at: usize) {
        let Some(piece) = self.emote_sets().get(index).and_then(|s| s.actions) else {
            return;
        };
        let last = self.insert(piece).is_some_and(|s| s.rows.len() == 1);
        if last {
            // The set keeps no actions: its statement and the `SET` before it go.
            self.pieces.remove(piece);
            if let Some(Piece::Text(t)) = piece.checked_sub(1).and_then(|i| self.pieces.get_mut(i))
            {
                if let Some(p) = t.rfind("SET @parent_id = LAST_INSERT_ID();") {
                    t.replace_range(p.., "");
                }
            }
            self.merge_text();
            return;
        }
        self.remove_row(RowRef { piece, row: at });
        self.renumber(piece);
    }

    /// Move action `from` of emote set `index` to `to`; the actions are numbered again.
    pub fn move_action(&mut self, index: usize, from: usize, to: usize) {
        let Some(piece) = self.emote_sets().get(index).and_then(|s| s.actions) else {
            return;
        };
        let Some(s) = self.insert_mut(piece) else {
            return;
        };
        if from >= s.rows.len() || to >= s.rows.len() || from == to {
            return;
        }
        // The separators stay where they are; the rows move between them.
        let seps: Vec<String> = s.rows.iter().map(|r| r.sep.clone()).collect();
        let row = s.rows.remove(from);
        s.rows.insert(to, row);
        for (r, sep) in s.rows.iter_mut().zip(seps) {
            r.sep = sep;
        }
        self.renumber(piece);
    }

    /// The `order` column of statement `piece`'s rows set to their places.
    fn renumber(&mut self, piece: usize) {
        let Some(s) = self.insert(piece) else { return };
        let n = s.rows.len();
        for row in 0..n {
            let r = RowRef { piece, row };
            if self.cell(r, "order").and_then(|v| v.parse::<usize>().ok()) != Some(row) {
                self.set_cell(r, "order", &format!(" {row:>2}"));
            }
        }
    }
}

/// The columns of an emote row, as ACE's export writes them.
pub const EMOTE_COLUMNS: [&str; 10] = [
    "object_Id",
    "category",
    "probability",
    "weenie_Class_Id",
    "style",
    "substyle",
    "quest",
    "vendor_Type",
    "min_Health",
    "max_Health",
];

/// The columns of an emote action row, as ACE's export writes them.
pub const ACTION_COLUMNS: [&str; 40] = [
    "emote_Id",
    "order",
    "type",
    "delay",
    "extent",
    "motion",
    "message",
    "test_String",
    "min",
    "max",
    "min_64",
    "max_64",
    "min_Dbl",
    "max_Dbl",
    "stat",
    "display",
    "amount",
    "amount_64",
    "hero_X_P_64",
    "percent",
    "spell_Id",
    "wealth_Rating",
    "treasure_Class",
    "treasure_Type",
    "p_Script",
    "sound",
    "destination_Type",
    "weenie_Class_Id",
    "stack_Size",
    "palette",
    "shade",
    "try_To_Bond",
    "obj_Cell_Id",
    "origin_X",
    "origin_Y",
    "origin_Z",
    "angles_W",
    "angles_X",
    "angles_Y",
    "angles_Z",
];

/// The server's names: what it calls each property of each type, and the values of the enums the
/// forms show. Read from what `empyrean-import schema` writes (JSON).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Schema {
    /// By type key (`int`, `did`, ...), each property id's name.
    #[serde(default)]
    pub properties: BTreeMap<String, BTreeMap<u32, String>>,
    /// By enum name (`WeenieType`, `EmoteCategory`, `EmoteType`, ...), each value's name.
    #[serde(default)]
    pub enums: BTreeMap<String, BTreeMap<u32, String>>,
}

impl Schema {
    /// Read the schema JSON `text`.
    ///
    /// # Errors
    /// Text that is not one, with the reason.
    pub fn read(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// The name of property `id` of type `kind`.
    #[must_use]
    pub fn property(&self, kind: PropertyKind, id: u32) -> Option<&str> {
        self.properties
            .get(kind.key())?
            .get(&id)
            .map(String::as_str)
    }

    /// Property `name`'s type and id: what a form's "add" field takes besides a number.
    #[must_use]
    pub fn property_named(&self, name: &str) -> Option<(PropertyKind, u32)> {
        PropertyKind::ALL.into_iter().find_map(|k| {
            self.properties
                .get(k.key())?
                .iter()
                .find_map(|(id, n)| n.eq_ignore_ascii_case(name).then_some((k, *id)))
        })
    }

    /// The name of value `v` of enum `name`.
    #[must_use]
    pub fn value(&self, name: &str, v: u32) -> Option<&str> {
        self.enums.get(name)?.get(&v).map(String::as_str)
    }
}

/// A kind of blank weenie a form can start from, and the world database's class it is copied
/// from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blank {
    pub kind: &'static str,
    pub class: u32,
}

/// The blanks: one class of each kind, copied and stripped ([`template`] with `blank`).
pub const BLANKS: [Blank; 7] = [
    Blank {
        kind: "NPC",
        class: 5773,
    },
    Blank {
        kind: "creature",
        class: 7,
    },
    Blank {
        kind: "item",
        class: 280,
    },
    Blank {
        kind: "door",
        class: 278,
    },
    Blank {
        kind: "portal",
        class: 371,
    },
    Blank {
        kind: "chest",
        class: 143,
    },
    Blank {
        kind: "sign",
        class: 459,
    },
];

/// The tables a blank leaves out: what the class says and does, carries and spawns.
const BLANK_LEAVES: [&str; 6] = [
    "weenie_properties_emote",
    "weenie_properties_create_list",
    "weenie_properties_generator",
    "weenie_properties_event_filter",
    "weenie_properties_book",
    "weenie_properties_book_page_data",
];

/// One table's rows of one class in a world database's text.
#[derive(Default)]
struct Rows {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

/// The rows of class `class` in `world`: by table, its emotes by their row ids, and every emote
/// action by the emote it belongs to.
#[derive(Default)]
struct ClassRows {
    tables: BTreeMap<String, Rows>,
    emotes: Vec<(u32, Vec<String>)>,
    emote_columns: Vec<String>,
    actions: BTreeMap<u32, Vec<Vec<String>>>,
    action_columns: Vec<String>,
}

fn class_rows(world: &str, class: u32) -> ClassRows {
    let b = world.as_bytes();
    let mut c = ClassRows::default();
    let mut creates: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut emote_ids: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < b.len() {
        let s = skip_space(b, i);
        if s >= b.len() {
            break;
        }
        let end = statement_end(b, s);
        i = end;
        if word_at(b, s, "CREATE") {
            // A dump's table: its columns, for the inserts that list none.
            let at = skip_space(b, s + 6);
            if !word_at(b, at, "TABLE") {
                continue;
            }
            let at = skip_space(b, at + 5);
            let Some((name, _)) = name_at(world, at) else {
                continue;
            };
            let cols = world[s..end]
                .lines()
                .skip(1)
                .filter_map(|l| {
                    let l = l.trim_start();
                    l.strip_prefix('`')
                        .and_then(|r| r.split('`').next())
                        .map(str::to_owned)
                })
                .collect();
            creates.insert(name, cols);
            continue;
        }
        if !word_at(b, s, "INSERT") {
            continue;
        }
        let at = skip_space(b, s + 6);
        if !word_at(b, at, "INTO") {
            continue;
        }
        let at = skip_space(b, at + 4);
        let Some((table, after)) = name_at(world, at) else {
            continue;
        };
        if !table.starts_with("weenie") {
            continue;
        }
        let mut at = skip_space(b, after);
        let columns: Vec<String> = if b.get(at) == Some(&b'(') {
            let Some((cols, after)) = list_at(world, at) else {
                continue;
            };
            at = skip_space(b, after);
            cols.iter()
                .map(|c| c.trim().trim_matches('`').to_owned())
                .collect()
        } else {
            creates.get(&table).cloned().unwrap_or_default()
        };
        if !word_at(b, at, "VALUES") {
            continue;
        }
        let owner = match table.as_str() {
            "weenie" => "class_Id",
            EMOTE_ACTIONS => "emote_Id",
            _ => "object_Id",
        };
        let Some(k) = columns.iter().position(|x| x == owner) else {
            continue;
        };
        let mut r = skip_space(b, at + 6);
        while r < end && b[r] == b'(' {
            // The owner's value: the values before it are numbers or quoted text.
            let mut j = r + 1;
            let mut commas = 0;
            let mut start = j;
            while j < end && commas < k {
                let q = skip_quoted(b, j);
                if q > j {
                    j = q;
                    continue;
                }
                if b[j] == b',' {
                    commas += 1;
                    start = j + 1;
                }
                j += 1;
            }
            let mut stop = start;
            while stop < end && b[stop] != b',' && b[stop] != b')' {
                stop += 1;
            }
            let id = crate::sql_ids::number(world[start..stop].trim());
            let wanted = match table.as_str() {
                EMOTE_ACTIONS => id.is_some_and(|e| emote_ids.contains(&e)),
                _ => id == Some(class),
            };
            let Some((values, after)) = list_at(world, r) else {
                break;
            };
            if wanted {
                let values: Vec<String> = values.into_iter().map(|v| v.trim().to_owned()).collect();
                if table == EMOTE_ACTIONS {
                    if c.action_columns.is_empty() {
                        c.action_columns.clone_from(&columns);
                    }
                    if let Some(e) = id {
                        c.actions.entry(e).or_default().push(values);
                    }
                } else if table == "weenie_properties_emote" {
                    if c.emote_columns.is_empty() {
                        c.emote_columns.clone_from(&columns);
                    }
                    let row_id = columns
                        .iter()
                        .position(|x| x == "id")
                        .and_then(|p| values.get(p))
                        .and_then(|v| crate::sql_ids::number(v))
                        .unwrap_or(u32::MAX);
                    emote_ids.insert(row_id);
                    c.emotes.push((row_id, values));
                } else {
                    let t = c.tables.entry(table.clone()).or_default();
                    if t.columns.is_empty() {
                        t.columns.clone_from(&columns);
                    }
                    t.rows.push(values);
                }
            }
            r = skip_space(b, after);
            if b.get(r) == Some(&b',') {
                r = skip_space(b, r + 1);
            } else {
                break;
            }
        }
    }
    c
}

/// A new weenie's SQL, in ACE's export layout: every row class `class` has in `world` (ACE SQL, a
/// dump or an export), its class id replaced by `{{wcid:<name>}}` and its class name by `name`.
/// Its emotes' actions follow their emotes. A `blank` leaves out the emotes, the inventory, the
/// generators, the books and the event filters, and every string but the name. `None` when
/// `world` has no such class.
#[must_use]
pub fn template(world: &str, class: u32, name: &str, blank: bool) -> Option<String> {
    copy(
        world,
        class,
        &format!("{{{{wcid:{name}}}}}"),
        Some(name),
        blank,
    )
}

/// Every row of class `class` of `world` as an override of it: its own class id and class name
/// kept, so a pack that writes it takes the server's class's place. `None` when `world` has no
/// such class.
#[must_use]
pub fn override_of(world: &str, class: u32) -> Option<String> {
    copy(world, class, &class.to_string(), None, false)
}

/// Class `class`'s rows of `world`, its class id written as `token`, its class name as `name`
/// when given; `blank` as [`template`] takes it.
fn copy(world: &str, class: u32, token: &str, name: Option<&str>, blank: bool) -> Option<String> {
    let token = token.to_owned();
    let mut c = class_rows(world, class);
    let weenie = c.tables.get("weenie")?;
    // The columns without the database's own row id.
    let keep =
        |cols: &[String]| -> Vec<usize> { (0..cols.len()).filter(|i| cols[*i] != "id").collect() };
    let value = |table: &str, col: &str, v: &str| -> String {
        match (table, col) {
            (_, "object_Id" | "class_Id") => token.clone(),
            ("weenie", "class_Name") => name.map_or_else(|| v.to_owned(), quote),
            // A dump writes a bit as the byte it is, quoted.
            ("weenie_properties_bool", "value") => match unquote(v).as_str() {
                "1" | "\u{1}" | "True" | "true" => "True".into(),
                "0" | "\u{0}" | "False" | "false" => "False".into(),
                _ => v.to_owned(),
            },
            ("weenie_properties_d_i_d", "value") => {
                crate::sql_ids::number(v).map_or_else(|| v.to_owned(), |n| format!("0x{n:08X}"))
            }
            _ => v.to_owned(),
        }
    };
    let statement = |table: &str, cols: &[String], rows: &[Vec<String>], out: &mut String| {
        let k = keep(cols);
        let names: Vec<String> = k.iter().map(|i| format!("`{}`", cols[*i])).collect();
        out.push_str(&format!(
            "\nINSERT INTO `{table}` ({})\nVALUES ",
            names.join(", ")
        ));
        for (n, r) in rows.iter().enumerate() {
            if n > 0 {
                out.push_str("\n     , ");
            }
            let vals: Vec<String> = k
                .iter()
                .map(|i| value(table, &cols[*i], r.get(*i).map_or("NULL", String::as_str)))
                .collect();
            out.push_str(&format!("({})", vals.join(", ")));
        }
        out.push_str(";\n");
    };
    let mut out = format!("DELETE FROM `weenie` WHERE `class_Id` = {token};\n");
    statement("weenie", &weenie.columns, &weenie.rows, &mut out);
    c.emotes.sort_by_key(|(id, _)| *id);
    for table in TABLE_ORDER.iter().skip(1) {
        if blank && BLANK_LEAVES.contains(table) {
            continue;
        }
        if *table == "weenie_properties_emote" {
            for (id, row) in &c.emotes {
                statement(table, &c.emote_columns, std::slice::from_ref(row), &mut out);
                let Some(acts) = c.actions.get(id) else {
                    continue;
                };
                let mut acts = acts.clone();
                let order = c.action_columns.iter().position(|x| x == "order");
                acts.sort_by_key(|a| {
                    order
                        .and_then(|o| a.get(o))
                        .and_then(|v| v.trim().parse::<i64>().ok())
                        .unwrap_or(0)
                });
                if let Some(e) = c.action_columns.iter().position(|x| x == "emote_Id") {
                    for a in &mut acts {
                        "@parent_id".clone_into(&mut a[e]);
                    }
                }
                out.push_str("\nSET @parent_id = LAST_INSERT_ID();\n");
                statement(EMOTE_ACTIONS, &c.action_columns, &acts, &mut out);
            }
            continue;
        }
        let Some(t) = c.tables.get(*table) else {
            continue;
        };
        let mut rows = t.rows.clone();
        if blank && *table == "weenie_properties_string" {
            // A blank's words are its own: of the class's strings only its name stays.
            let ty = t.columns.iter().position(|x| x == "type");
            rows.retain(|r| {
                ty.and_then(|i| r.get(i))
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    == Some(1)
            });
        }
        if !rows.is_empty() {
            statement(table, &t.columns, &rows, &mut out);
        }
    }
    Some(out)
}

/// Every class id the `weenie` table of `world` (ACE SQL) holds, with its class name.
#[must_use]
pub fn classes(world: &str) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    crate::sql_ids::rows(world, |row| {
        if row.table != "weenie" {
            return;
        }
        if let Some(c) = row.get("class_Id").and_then(crate::sql_ids::number) {
            out.insert(c, row.get("class_Name").map(unquote).unwrap_or_default());
        }
    });
    out
}
