//! The plain values that cross between a plugin and its host: event arguments, call arguments and
//! results, UI node properties and stored data. They are JSON's values, with integers kept apart
//! from other numbers, and [`Value::to_json`] / [`Value::from_json`] are the text a store keeps.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One plain value.
///
/// A map's keys are strings and are kept in key order, so the same value always has the same JSON
/// text and a stored file does not change when nothing in it did.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    /// An empty map, to build with [`Value::with`].
    #[must_use]
    pub fn map() -> Self {
        Self::Map(BTreeMap::new())
    }

    /// This map with `key` set to `v`. A value that is not a map becomes one.
    #[must_use]
    pub fn with(mut self, key: &str, v: impl Into<Value>) -> Self {
        if !matches!(self, Self::Map(_)) {
            self = Self::map();
        }
        if let Self::Map(m) = &mut self {
            m.insert(key.to_owned(), v.into());
        }
        self
    }

    /// A map's entry, or `Nil`.
    #[must_use]
    pub fn get(&self, key: &str) -> &Value {
        static NIL: Value = Value::Nil;
        match self {
            Self::Map(m) => m.get(key).unwrap_or(&NIL),
            _ => &NIL,
        }
    }

    #[must_use]
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// An integer, or a number with no fractional part that fits.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the value is checked to be integral and in range before the conversion.
            Self::Num(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
            _ => None,
        }
    }

    /// Any number.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Self::Int(i) => Some(*i as f64),
            Self::Num(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Map(m) => Some(m),
            _ => None,
        }
    }

    /// The value as JSON text. A number that is not finite has no JSON form and is written as
    /// `null`; `Nil` is `null`.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_json(self, &mut out);
        out
    }

    /// Parse JSON text. A number with no fraction and no exponent that fits is an `Int`.
    ///
    /// # Errors
    /// Where the text stops being JSON, as a byte offset and a reason.
    pub fn from_json(text: &str) -> Result<Self, JsonError> {
        Self::from_json_within(text, usize::MAX)
    }

    /// As [`Self::from_json`], refusing text that holds more than `max_values` values (each
    /// list, map, string, number, boolean and null), counted as it is read.
    ///
    /// # Errors
    /// As [`Self::from_json`], or more values than `max_values`.
    pub fn from_json_within(text: &str, max_values: usize) -> Result<Self, JsonError> {
        let mut p = Parser {
            b: text.as_bytes(),
            i: 0,
            depth: 0,
            values_left: max_values,
        };
        p.ws();
        let v = p.value()?;
        p.ws();
        if p.i != p.b.len() {
            return Err(p.err("text after the value"));
        }
        Ok(v)
    }

    /// The approximate number of bytes the value occupies as JSON, for quotas.
    #[must_use]
    pub fn json_len(&self) -> usize {
        self.to_json().len()
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Int(i64::from(v))
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Self::Int(i64::from(v))
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Num(v)
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Self::Num(f64::from(v))
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Str(v.to_owned())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Self::List(v)
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map_or(Self::Nil, Into::into)
    }
}

fn write_json(v: &Value, out: &mut String) {
    match v {
        Value::Nil => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Num(n) => {
            if n.is_finite() {
                // `{:?}` keeps a fraction on integral values ("2.0"), so the number reads back as
                // a number rather than an integer.
                let _ = write!(out, "{n:?}");
            } else {
                out.push_str("null");
            }
        }
        Value::Str(s) => write_str(s, out),
        Value::List(l) => {
            out.push('[');
            for (i, e) in l.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(e, out);
            }
            out.push(']');
        }
        Value::Map(m) => {
            out.push('{');
            for (i, (k, e)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(k, out);
                out.push(':');
                write_json(e, out);
            }
            out.push('}');
        }
    }
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Where JSON text stopped parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub offset: usize,
    pub reason: &'static str,
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.reason, self.offset)
    }
}

impl std::error::Error for JsonError {}

/// Nesting deeper than this is refused rather than recursed into.
const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    depth: usize,
    values_left: usize,
}

impl Parser<'_> {
    fn err(&self, reason: &'static str) -> JsonError {
        JsonError {
            offset: self.i,
            reason,
        }
    }

    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.b[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value, JsonError> {
        let Some(&c) = self.b.get(self.i) else {
            return Err(self.err("end of text where a value was expected"));
        };
        self.values_left = self
            .values_left
            .checked_sub(1)
            .ok_or_else(|| self.err("more values than the reader takes"))?;
        match c {
            b'n' if self.eat(b"null") => Ok(Value::Nil),
            b't' if self.eat(b"true") => Ok(Value::Bool(true)),
            b'f' if self.eat(b"false") => Ok(Value::Bool(false)),
            b'"' => Ok(Value::Str(self.string()?)),
            b'[' => {
                self.nest()?;
                self.i += 1;
                let mut l = Vec::new();
                self.ws();
                if self.eat(b"]") {
                    self.depth -= 1;
                    return Ok(Value::List(l));
                }
                loop {
                    self.ws();
                    l.push(self.value()?);
                    self.ws();
                    if self.eat(b",") {
                        continue;
                    }
                    if self.eat(b"]") {
                        break;
                    }
                    return Err(self.err("expected , or ]"));
                }
                self.depth -= 1;
                Ok(Value::List(l))
            }
            b'{' => {
                self.nest()?;
                self.i += 1;
                let mut m = BTreeMap::new();
                self.ws();
                if self.eat(b"}") {
                    self.depth -= 1;
                    return Ok(Value::Map(m));
                }
                loop {
                    self.ws();
                    if self.b.get(self.i) != Some(&b'"') {
                        return Err(self.err("expected a string key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b":") {
                        return Err(self.err("expected :"));
                    }
                    self.ws();
                    let v = self.value()?;
                    m.insert(k, v);
                    self.ws();
                    if self.eat(b",") {
                        continue;
                    }
                    if self.eat(b"}") {
                        break;
                    }
                    return Err(self.err("expected , or }"));
                }
                self.depth -= 1;
                Ok(Value::Map(m))
            }
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(self.err("not a JSON value")),
        }
    }

    fn nest(&mut self) -> Result<(), JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("nested too deeply"));
        }
        Ok(())
    }

    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.i;
        let mut integral = true;
        if self.b[self.i] == b'-' {
            self.i += 1;
        }
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'0'..=b'9' => {}
                b'.' | b'e' | b'E' | b'+' | b'-' => integral = false,
                _ => break,
            }
            self.i += 1;
        }
        let text =
            std::str::from_utf8(&self.b[start..self.i]).map_err(|_| self.err("bad number"))?;
        if integral {
            if let Ok(i) = text.parse::<i64>() {
                return Ok(Value::Int(i));
            }
        }
        text.parse::<f64>().map(Value::Num).map_err(|_| JsonError {
            offset: start,
            reason: "bad number",
        })
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let h = self
            .b
            .get(self.i..self.i + 4)
            .ok_or_else(|| self.err("short \\u escape"))?;
        let s = std::str::from_utf8(h).map_err(|_| self.err("bad \\u escape"))?;
        let v = u32::from_str_radix(s, 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let Some(&c) = self.b.get(self.i) else {
                return Err(self.err("unterminated string"));
            };
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let Some(&e) = self.b.get(self.i) else {
                        return Err(self.err("unterminated escape"));
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) && self.eat(b"\\u") {
                                let lo = self.hex4()?;
                                cp = 0x10000
                                    + ((cp - 0xD800) << 10)
                                    + (lo.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.err("unknown escape")),
                    }
                }
                _ => {
                    // Copy the run of plain bytes up to the next quote or escape as UTF-8.
                    let start = self.i;
                    while self.i < self.b.len() && !matches!(self.b[self.i], b'"' | b'\\') {
                        self.i += 1;
                    }
                    let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| JsonError {
                        offset: start,
                        reason: "not UTF-8",
                    })?;
                    out.push_str(s);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn json_with_more_values_than_the_reader_takes_is_refused_as_it_is_read() {
        let text = format!("[{}0]", "[],".repeat(100));
        assert!(
            Value::from_json_within(&text, 102).is_ok(),
            "the list, a hundred lists and a number"
        );
        assert!(Value::from_json_within(&text, 101).is_err());
        assert!(Value::from_json(&text).is_ok());
    }

    #[test]
    fn json_round_trips_every_kind_of_value() {
        let v = Value::map()
            .with("n", Value::Nil)
            .with("b", true)
            .with("i", -42_i64)
            .with("f", 2.0_f64)
            .with("s", "a \"quoted\" line\nand a tab\t\u{1}")
            .with(
                "l",
                vec![Value::Int(1), Value::Str("x".into()), Value::map()],
            );
        let text = v.to_json();
        assert_eq!(Value::from_json(&text), Ok(v));
    }

    #[test]
    fn integers_stay_integers_and_fractions_stay_numbers() {
        assert_eq!(Value::from_json("12"), Ok(Value::Int(12)));
        assert_eq!(Value::from_json("12.0"), Ok(Value::Num(12.0)));
        assert_eq!(Value::from_json("1e3"), Ok(Value::Num(1000.0)));
        assert_eq!(Value::Num(12.0).to_json(), "12.0");
    }

    #[test]
    fn a_map_is_written_in_key_order() {
        let v = Value::map().with("b", 1_i64).with("a", 2_i64);
        assert_eq!(v.to_json(), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn broken_json_is_refused_with_where_it_broke() {
        for bad in ["", "{", "[1,]", r#"{"a" 1}"#, "tru", "\"open", "1 2"] {
            assert!(Value::from_json(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(200) + &"]".repeat(200);
        assert!(Value::from_json(&deep).is_err());
    }

    #[test]
    fn unicode_escapes_decode_including_surrogate_pairs() {
        assert_eq!(Value::from_json(r#""é😀""#), Ok(Value::Str("é😀".into())));
    }
}
