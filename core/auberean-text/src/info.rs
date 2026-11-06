//! String references: a string table entry (or a literal) with variables, and how the client turns
//! one into text.
//!
//! A reference names a string by its token in a table (a record id), or holds a literal; its
//! variables are numbers, numbers with their format, or other references. To write it:
//!
//! - a **literal** is the text (its flags cut off unless they are kept);
//! - else the reference needs a token and a table, the table must be there, and the token in it
//!   (an entry may send the token on to a private table, looked up once more); every variable is
//!   written to text first (a number in the table's or the language's format, a nested reference
//!   recursively with its flags kept, taking this reference's table when it names none), and every
//!   variable the entry uses must be among them; then the entry is rendered with the meta language.
//!   Until it succeeds the text is an error message naming the token and the table;
//! - the text's flags are cut off unless they are kept;
//! - asked for its string, a reference that rendered is unescaped; one that did not is its error
//!   message.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::strings::{StringEntry, StringTable};

use auberean_dat::QualifiedIds;

use crate::crt::{cstr, format_s, hex8};
use crate::language::Language;
use crate::meta::{
    apply_meta_tags_to_name, escape, render_string, strip_meta_letters, unescape, RenderCode,
};
use crate::number::{
    float_to_string, int_to_string, long_to_string, uint_to_string, ulong_to_string, TableNumbers,
};

/// The string table record type.
pub const STRING_TABLE_TYPE: u32 = 0x4000_0026;

/// Why a reference could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Success = 0,
    /// No token or no table.
    Uninitialized = 1,
    /// The table has no entry for the token.
    MissingString = 2,
    /// The table is not there.
    MissingTable = 3,
    /// The private table has no entry for the token.
    MissingPrivateString = 4,
    /// The entry's private table is not there.
    MissingPrivateTable = 5,
    /// The entry uses a variable the reference does not have (or could not write).
    MissingVariable = 6,
    /// The meta language could not render the entry.
    MetaLanguageError = 7,
}

/// One string table, indexed by token.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub numbers: TableNumbers,
    entries: HashMap<u32, Entry>,
}

/// One entry of a table, in UTF-16.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// A private table the token is looked up in instead (0 for none).
    pub private: u32,
    pub fragments: Vec<Option<Vec<u16>>>,
    pub variables: Vec<u32>,
    pub names: Vec<Vec<u16>>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl From<&StringEntry> for Entry {
    fn from(e: &StringEntry) -> Self {
        Self {
            private: e.table,
            fragments: e.strings.iter().map(|s| s.as_deref().map(wide)).collect(),
            variables: e.variables.clone(),
            names: e.variable_names.iter().map(|s| wide(s)).collect(),
        }
    }
}

impl From<&StringTable> for Table {
    fn from(t: &StringTable) -> Self {
        let mut entries = HashMap::with_capacity(t.entries.len());
        for e in &t.entries {
            // The first entry of a token is the one a lookup finds.
            entries.entry(e.token).or_insert_with(|| Entry::from(e));
        }
        #[allow(clippy::cast_possible_wrap)]
        Self {
            numbers: TableNumbers {
                base: t.number_base as i16,
                decimal_digits: t.decimal_digits as i16,
                leading_zero: t.leading_zero as i32,
            },
            entries,
        }
    }
}

impl Table {
    /// A table with these number settings and entries.
    #[must_use]
    pub fn new(numbers: TableNumbers, entries: impl IntoIterator<Item = (u32, Entry)>) -> Self {
        let mut map = HashMap::new();
        for (token, e) in entries {
            map.entry(token).or_insert(e);
        }
        Self {
            numbers,
            entries: map,
        }
    }

    /// The entry of a token.
    // D2-14094
    #[must_use]
    pub fn entry(&self, token: u32) -> Option<&Entry> {
        self.entries.get(&token)
    }

    /// Every entry with its token, in token order.
    pub fn entries(&self) -> impl Iterator<Item = (u32, &Entry)> + '_ {
        let mut tokens: Vec<u32> = self.entries.keys().copied().collect();
        tokens.sort_unstable();
        tokens.into_iter().map(|t| (t, &self.entries[&t]))
    }
}

/// Where references find their tables and their language.
pub trait Source {
    /// The string table with this record id, if the data has it.
    fn table(&mut self, id: u32) -> Option<Arc<Table>>;
    /// The language's writing rules, if the data has them.
    fn language(&mut self) -> Option<Arc<Language>>;
}

/// A reference's table id kept only when it is a string table's (0 otherwise).
// D2-14116
#[must_use]
pub fn table_or_none(id: u32) -> u32 {
    if auberean_dat::type_of_id(id).is_some_and(|t| t.0 == STRING_TABLE_TYPE) {
        id
    } else {
        0
    }
}

/// A string reference.
// D2-14163, D2-14144
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StringInfo {
    pub string_id: u32,
    pub table: u32,
    /// The literal text, when the reference is one.
    pub literal: Option<Vec<u16>>,
    /// The variables, in the reference's own order (a later one with the same id wins).
    pub variables: Vec<Variable>,
}

/// One variable: its id and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    pub id: u32,
    pub value: Value,
}

/// A variable's value, by its kind. A formatted number carries whether it is grouped and its
/// base (negative for the table's or the language's); the others are grouped, in that base.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Kind 2.
    Int(i32),
    /// Kind 3.
    FormattedInt {
        value: i32,
        grouping: i32,
        base: u16,
    },
    /// Kind 4: written as a 32-bit float, with `decimals` decimals.
    Float { value: f64, decimals: u16 },
    /// Kind 5.
    FormattedFloat {
        value: f64,
        decimals: u16,
        grouping: i32,
        base: u16,
    },
    /// Kind 6.
    UInt(u32),
    /// Kind 7.
    FormattedUInt {
        value: u32,
        grouping: i32,
        base: u16,
    },
    /// Kind 9.
    Text(Box<StringInfo>),
    /// Kind 10.
    Long(i64),
    /// Kind 11.
    FormattedLong {
        value: i64,
        grouping: i32,
        base: u16,
    },
    /// Kind 12.
    ULong(u64),
    /// Kind 13.
    FormattedULong {
        value: u64,
        grouping: i32,
        base: u16,
    },
    /// Any other kind: no value.
    Nothing(u16),
}

/// The base a variable names, as the client's 16-bit argument.
#[allow(clippy::cast_possible_wrap)]
fn base_of(b: u16) -> i16 {
    b as i16
}

impl StringInfo {
    /// A literal.
    #[must_use]
    pub fn literal(text: &str) -> Self {
        Self {
            literal: Some(wide(text)),
            ..Self::default()
        }
    }

    /// A table entry.
    #[must_use]
    pub fn entry(table: u32, string_id: u32) -> Self {
        Self {
            string_id,
            table,
            ..Self::default()
        }
    }

    /// Set a variable, replacing one of the same id.
    // D2-14151
    pub fn add_var(&mut self, id: u32, value: Value) {
        self.variables.retain(|v| v.id != id);
        self.variables.push(Variable { id, value });
    }

    /// The variables as the reference holds them: one per id, the last set.
    fn effective_variables(&self) -> impl Iterator<Item = &Variable> + '_ {
        let vars = &self.variables;
        vars.iter()
            .enumerate()
            .filter(move |(i, v)| !vars[i + 1..].iter().any(|w| w.id == v.id))
            .map(|(_, v)| v)
    }

    /// Make the reference a literal: the text as it is, or escaped so the meta language reads it
    /// as plain text.
    // D2-14131
    pub fn set_literal(&mut self, text: &[u16], escaped: bool) {
        self.literal = Some(if escaped { escape(text) } else { text.to_vec() });
    }

    /// A text variable: a nested literal holding the text, escaped.
    // D2-14172
    pub fn add_variable_string(&mut self, id: u32, text: &[u16]) {
        let mut nested = StringInfo::default();
        nested.set_literal(text, true);
        self.add_var(id, Value::Text(Box::new(nested)));
    }

    /// A text variable from narrow text, read in the English Windows code page (1252).
    pub fn add_variable_narrow(&mut self, id: u32, text: &[u8]) {
        let wide: Vec<u16> = text.iter().map(|&b| cp1252(b)).collect();
        self.add_variable_string(id, &wide);
    }

    /// A variable naming another token of this reference's table (kept only when that is a
    /// string table).
    // D2-14169
    pub fn add_variable_string_id(&mut self, id: u32, token: u32) {
        let nested = StringInfo::entry(table_or_none(self.table), token);
        self.add_var(id, Value::Text(Box::new(nested)));
    }

    /// The literal (empty when the reference is none), its flags cut off unless kept, unescaped.
    // D2-14132
    #[must_use]
    pub fn literal_value(&self, keep_meta: bool) -> Vec<u16> {
        let lit = self.literal.clone().unwrap_or_default();
        let lit = if keep_meta {
            lit
        } else {
            strip_meta_letters(&lit)
        };
        unescape(&lit)
    }

    /// Make a literal player name a value of the meta language: its flags replaced by the
    /// player's (`M` or `F`, and `v` for a name beginning with a vowel).
    // D2-14117
    pub fn apply_metas_to_player_name(&mut self, male: bool) {
        if let Some(lit) = &mut self.literal {
            *lit = apply_meta_tags_to_name(lit, male);
        }
    }

    /// The reference's bucket among `n`: a literal's text hashed, else its table's and its
    /// token's remainders added. `n` must not be 0 (the client divides by it); 0 answers 0.
    #[must_use]
    pub fn bucket(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        match &self.literal {
            Some(lit) => string_hash(lit) % n,
            None => (self.table % n + self.string_id % n) % n,
        }
    }

    /// Whether the reference can be written: a literal always; else its table must be there and
    /// hold its token, and (when `deep`) every nested reference must be valid too.
    // D2-14138
    pub fn is_valid(&self, src: &mut dyn Source, deep: bool) -> bool {
        if self.literal.is_some() {
            return true;
        }
        if self.string_id == 0 || self.table == 0 {
            return false;
        }
        let Some(table) = src.table(self.table) else {
            return false;
        };
        if table.entry(self.string_id).is_none() {
            return false;
        }
        if deep {
            for v in self.effective_variables() {
                if let Value::Text(nested) = &v.value {
                    if !nested.is_valid(src, deep) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Whether the reference and every nested one can be written.
    // D2-14148
    pub fn is_ready(&self, src: &mut dyn Source) -> bool {
        self.is_valid(src, true)
    }

    /// Whether the reference writes without an error (the data holds all it needs).
    // D2-01668
    pub fn is_on_disk(&mut self, src: &mut dyn Source) -> bool {
        self.inq_internal(src, false).0 == Code::Success
    }

    /// The string tables the reference reads: its own table (none for a literal, nor anything
    /// nested in one) and every nested reference's, the same way.
    // D2-14139
    pub fn sub_data_ids(&self, out: &mut QualifiedIds) {
        if self.literal.is_some() {
            return;
        }
        out.add(self.table);
        for v in self.effective_variables() {
            if let Value::Text(nested) = &v.value {
                nested.sub_data_ids(out);
            }
        }
    }

    /// The private tables the reference's entries in table `table` send their tokens on to: its
    /// own entry's when it names that table, and every nested reference's the same way.
    // D2-14140
    pub fn sub_private_ids(&self, src: &mut dyn Source, table: u32, out: &mut QualifiedIds) {
        if table == self.table {
            if let Some(t) = src.table(self.table) {
                if let Some(e) = t.entry(self.string_id) {
                    out.add(e.private);
                }
            }
        }
        for v in self.effective_variables() {
            if let Value::Text(nested) = &v.value {
                nested.sub_private_ids(src, table, out);
            }
        }
    }

    /// The error text a reference starts from.
    fn error_text(&self) -> Vec<u16> {
        let fmt = wide("<string table error; token: 0x%s tableDID: 0x%s>");
        format_s(&fmt, &[&hex8(self.string_id), &hex8(self.table)])
    }

    /// Write the reference with its flags (`keep_meta`) or without, and say how it went. A nested
    /// reference that names no table takes this one's (and keeps it, as the client's does).
    // D2-14143
    pub fn inq_internal(&mut self, src: &mut dyn Source, keep_meta: bool) -> (Code, Vec<u16>) {
        if let Some(lit) = &self.literal {
            let out = if keep_meta {
                lit.clone()
            } else {
                strip_meta_letters(lit)
            };
            return (Code::Success, out);
        }
        let out = self.error_text();
        if self.string_id == 0 || self.table == 0 {
            return (Code::Uninitialized, out);
        }
        let Some(table) = src.table(self.table) else {
            return (Code::MissingTable, out);
        };
        let lang = src.language();
        // D2-14103, D2-14113
        let mut vars: HashMap<u32, Vec<u16>> = HashMap::new();
        let parent_table = self.table;
        for v in &mut self.variables {
            set_var_in_table(&mut vars, v, &table, lang.as_deref(), parent_table, src);
        }
        let (code, mut out) = get_string(&table, self.string_id, &vars, out, lang.as_deref(), src);
        if !keep_meta {
            out = strip_meta_letters(&out);
        }
        (code, out)
    }

    /// Write the reference: unescaped when it rendered, else its error text. True when it
    /// rendered.
    // D2-14128
    pub fn inq(&mut self, src: &mut dyn Source, keep_meta: bool) -> (bool, Vec<u16>) {
        let (code, out) = self.inq_internal(src, keep_meta);
        if code == Code::Success {
            (true, unescape(&out))
        } else {
            (false, out)
        }
    }

    /// The reference's text, as the interface shows it (its flags cut off).
    // D2-14126
    pub fn get_string(&mut self, src: &mut dyn Source, keep_meta: bool) -> Vec<u16> {
        self.inq(src, keep_meta).1
    }

    /// [`StringInfo::get_string`] as a Rust string (unpaired surrogates become U+FFFD).
    pub fn text(&mut self, src: &mut dyn Source) -> String {
        String::from_utf16_lossy(&self.get_string(src, false))
    }

    /// The reference for a log: its rendered text (escapes kept) in parentheses, or a line saying
    /// why it could not be.
    // D2-14135
    pub fn log_string(&mut self, src: &mut dyn Source, keep_meta: bool) -> Vec<u16> {
        let (code, out) = self.inq_internal(src, keep_meta);
        if code == Code::Success {
            let mut s = vec![0x28];
            s.extend(out);
            s.push(0x29);
            s
        } else {
            let mut s = wide("<could not render string: table 0x");
            s.extend(hex8(self.table));
            s.extend(wide(" token 0x"));
            s.extend(hex8(self.string_id));
            s.extend(wide(". Reason = "));
            s.extend(wide(&(code as i32).to_string()));
            s.push(0x3E);
            s
        }
    }
}

/// A wide string's hash: each unit (to the first zero) added to the hash times 16, the top four
/// bits folded back in; never all ones. Hash tables keyed by wide strings put a string in bucket
/// `string_hash % buckets`.
// D2-01377
#[must_use]
pub fn string_hash(s: &[u16]) -> u32 {
    let mut h: u32 = 0;
    for &u in cstr(s) {
        h = h.wrapping_mul(16).wrapping_add(u32::from(u));
        let top = h & 0xF000_0000;
        if top != 0 {
            h = ((top >> 24) ^ h) & 0x0FFF_FFFF;
        }
    }
    if h == u32::MAX {
        h - 1
    } else {
        h
    }
}

/// A byte of the English Windows code page as a UTF-16 unit.
fn cp1252(b: u8) -> u16 {
    const HIGH: [u16; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => u16::from(b),
    }
}

/// A string table entry's text without rendering: every non-empty string of the token's entry
/// (through its private table when it names one), joined. The code says why when there is none.
// D2-14097
pub fn raw_string(table: &Table, token: u32, src: &mut dyn Source) -> (Code, Vec<u16>) {
    let Some(entry) = table.entry(token) else {
        return (Code::MissingString, Vec::new());
    };
    let private;
    let entry = if entry.private == 0 {
        entry
    } else {
        let Some(t) = src.table(entry.private) else {
            return (Code::MissingPrivateTable, Vec::new());
        };
        private = t;
        match private.entry(token) {
            Some(e) => e,
            None => return (Code::MissingPrivateString, Vec::new()),
        }
    };
    let mut out = Vec::new();
    for f in entry.fragments.iter().flatten() {
        out.extend_from_slice(f);
    }
    (Code::Success, out)
}

/// Write one variable into the variables' text by id (a later one replaces an earlier).
// D2-14133, D2-14104, D2-14105, D2-14106, D2-14107, D2-14108, D2-14109
fn set_var_in_table(
    vars: &mut HashMap<u32, Vec<u16>>,
    v: &mut Variable,
    table: &Table,
    lang: Option<&Language>,
    parent_table: u32,
    src: &mut dyn Source,
) {
    let t = table.numbers;
    let text = match &mut v.value {
        Value::Int(x) => int_to_string(t, lang, *x, 1, -1),
        Value::FormattedInt {
            value,
            grouping,
            base,
        } => int_to_string(t, lang, *value, *grouping, base_of(*base)),
        Value::UInt(x) => uint_to_string(t, lang, *x, 1, -1),
        Value::FormattedUInt {
            value,
            grouping,
            base,
        } => uint_to_string(t, lang, *value, *grouping, base_of(*base)),
        // The value goes through a 32-bit float on its way.
        #[allow(clippy::cast_possible_truncation)]
        Value::Float { value, decimals } => {
            float_to_string(t, lang, f64::from(*value as f32), *decimals, 1, -1)
        }
        #[allow(clippy::cast_possible_truncation)]
        Value::FormattedFloat {
            value,
            decimals,
            grouping,
            base,
        } => float_to_string(
            t,
            lang,
            f64::from(*value as f32),
            *decimals,
            *grouping,
            base_of(*base),
        ),
        Value::Long(x) => long_to_string(t, lang, *x, 1, -1),
        Value::FormattedLong {
            value,
            grouping,
            base,
        } => long_to_string(t, lang, *value, *grouping, base_of(*base)),
        Value::ULong(x) => ulong_to_string(t, lang, *x, 1, -1),
        Value::FormattedULong {
            value,
            grouping,
            base,
        } => ulong_to_string(t, lang, *value, *grouping, base_of(*base)),
        Value::Text(nested) => {
            if nested.literal.is_none() && nested.string_id != 0 && nested.table == 0 {
                nested.table = table_or_none(parent_table);
            }
            Some(nested.inq_internal(src, true).1)
        }
        Value::Nothing(_) => return,
    };
    // A number that could not be written still sets its variable, to nothing.
    vars.insert(v.id, text.unwrap_or_default());
}

/// Look a token up (through a private table when the entry names one) and render it with the
/// variables' text. `out` is what the caller holds (its error text), kept unless rendering writes.
// D2-14098, D2-14110
fn get_string(
    table: &Table,
    token: u32,
    vars: &HashMap<u32, Vec<u16>>,
    mut out: Vec<u16>,
    lang: Option<&Language>,
    src: &mut dyn Source,
) -> (Code, Vec<u16>) {
    let Some(entry) = table.entry(token) else {
        return (Code::MissingString, out);
    };
    let private;
    let entry = if entry.private == 0 {
        entry
    } else {
        let Some(t) = src.table(entry.private) else {
            return (Code::MissingPrivateTable, out);
        };
        private = t;
        match private.entry(token) {
            Some(e) => e,
            None => return (Code::MissingPrivateString, out),
        }
    };
    let mut values = Vec::with_capacity(entry.variables.len());
    let mut missing = false;
    for id in &entry.variables {
        match vars.get(id) {
            Some(v) => values.push(v.clone()),
            None => {
                missing = true;
                values.push(Vec::new());
            }
        }
    }
    if missing {
        return (Code::MissingVariable, out);
    }
    let mut outside = Vec::new();
    let err = render_string(
        &entry.fragments,
        &values,
        &entry.names,
        &mut out,
        &mut outside,
        1,
        lang,
    );
    if err.code == RenderCode::Success {
        (Code::Success, out)
    } else {
        (Code::MetaLanguageError, out)
    }
}
