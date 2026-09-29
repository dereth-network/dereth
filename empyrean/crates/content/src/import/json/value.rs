//! A strict JSON reader with `System.Text.Json`'s default deserialization rules, as ACE's
//! `JsonSerializer.Deserialize<T>(string)` applies them to the ACE.Adapter models.
//!
//! Not ACE-derived code: .NET runtime behaviour, checked against ACE's loaders by the
//! `content_json` vectors. The rules that matter:
//! * RFC 8259 only: no comments, no trailing commas, one value, nesting depth at most 64.
//! * Property names are case-sensitive; unknown properties are skipped; a repeated property is
//!   read again (the last one wins).
//! * Integers (`int`, `uint`, `long`, `ulong`, `byte`) take only a plain integer token: `1.0`,
//!   `1e2` and (for unsigned types) `-0` fail, as does a value outside the type's range.
//! * `float` and `double` take any number token, rounded once from its text; a value beyond the
//!   type's range reads as an infinity.
//! * `null` is accepted for reference types and `Nullable<T>` and fails for other value types; a
//!   token of the wrong kind (a string for a number, `1` for a `bool`) fails.

/// One parsed JSON value. Numbers keep their source text so each target type rounds it once.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

/// A load or conversion failure, with what went wrong.
pub type R<T> = Result<T, String>;

/// `JsonSerializerOptions.MaxDepth`'s default.
const MAX_DEPTH: usize = 64;

/// Parse a whole document.
pub fn parse(text: &str) -> R<Json> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return Err(format!("unexpected data after the value at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn err<T>(&self, what: &str) -> R<T> {
        Err(format!("{what} at byte {}", self.i))
    }

    fn lit(&mut self, word: &[u8], v: Json) -> R<Json> {
        if self.b[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            self.err("invalid literal")
        }
    }

    fn value(&mut self, depth: usize) -> R<Json> {
        let Some(&c) = self.b.get(self.i) else {
            return self.err("unexpected end");
        };
        match c {
            b'n' => self.lit(b"null", Json::Null),
            b't' => self.lit(b"true", Json::Bool(true)),
            b'f' => self.lit(b"false", Json::Bool(false)),
            b'"' => self.string().map(Json::Str),
            b'-' | b'0'..=b'9' => self.number(),
            b'[' | b'{' => {
                if depth >= MAX_DEPTH {
                    return self.err("maximum depth exceeded");
                }
                self.i += 1;
                if c == b'[' {
                    self.array(depth + 1)
                } else {
                    self.object(depth + 1)
                }
            }
            _ => self.err("unexpected character"),
        }
    }

    fn array(&mut self, depth: usize) -> R<Json> {
        let mut items = Vec::new();
        self.ws();
        if self.b.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value(depth)?);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return self.err("expected ',' or ']'"),
            }
        }
    }

    fn object(&mut self, depth: usize) -> R<Json> {
        let mut members = Vec::new();
        self.ws();
        if self.b.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Json::Obj(members));
        }
        loop {
            self.ws();
            if self.b.get(self.i) != Some(&b'"') {
                return self.err("expected a property name");
            }
            let key = self.string()?;
            self.ws();
            if self.b.get(self.i) != Some(&b':') {
                return self.err("expected ':'");
            }
            self.i += 1;
            self.ws();
            let v = self.value(depth)?;
            members.push((key, v));
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(members));
                }
                _ => return self.err("expected ',' or '}'"),
            }
        }
    }

    fn number(&mut self) -> R<Json> {
        let start = self.i;
        let b = self.b;
        let digits = |i: &mut usize| {
            let s = *i;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
            *i - s
        };
        if b[self.i] == b'-' {
            self.i += 1;
        }
        match b.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                digits(&mut self.i);
            }
            _ => return self.err("invalid number"),
        }
        if b.get(self.i) == Some(&b'.') {
            self.i += 1;
            if digits(&mut self.i) == 0 {
                return self.err("invalid number");
            }
        }
        if matches!(b.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if digits(&mut self.i) == 0 {
                return self.err("invalid number");
            }
        }
        // The token must end at a delimiter.
        if let Some(&c) = b.get(self.i) {
            if !matches!(c, b',' | b']' | b'}' | b' ' | b'\t' | b'\n' | b'\r') {
                return self.err("invalid number");
            }
        }
        Ok(Json::Num(
            String::from_utf8_lossy(&b[start..self.i]).into_owned(),
        ))
    }

    fn hex4(&mut self) -> R<u32> {
        let Some(h) = self.b.get(self.i..self.i + 4) else {
            return self.err("bad \\u escape");
        };
        let s = core::str::from_utf8(h).map_err(|_| "bad \\u escape".to_owned())?;
        let v =
            u32::from_str_radix(s, 16).map_err(|_| format!("bad \\u escape at byte {}", self.i))?;
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> R<String> {
        self.i += 1; // opening quote
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.i) else {
                return self.err("unterminated string");
            };
            match c {
                b'"' => {
                    self.i += 1;
                    return String::from_utf8(out)
                        .map_err(|_| "invalid UTF-8 in a string".to_owned());
                }
                b'\\' => {
                    self.i += 1;
                    let Some(&e) = self.b.get(self.i) else {
                        return self.err("unterminated string");
                    };
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if self.b.get(self.i..self.i + 2) != Some(b"\\u") {
                                    return self.err("lone surrogate");
                                }
                                self.i += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return self.err("lone surrogate");
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return self.err("lone surrogate");
                            } else {
                                hi
                            };
                            char::from_u32(cp).ok_or_else(|| "bad code point".to_owned())?
                        }
                        _ => return self.err("invalid escape"),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                0..=0x1F => return self.err("control character in a string"),
                _ => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }
}

// ---- typed reads, as System.Text.Json's default converters apply them ----------------------

fn kind(j: &Json) -> &'static str {
    match j {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Num(_) => "a number",
        Json::Str(_) => "a string",
        Json::Arr(_) => "an array",
        Json::Obj(_) => "an object",
    }
}

fn integer(j: &Json, what: &str, signed: bool) -> R<i128> {
    let Json::Num(s) = j else {
        return Err(format!("{what}: expected a number, found {}", kind(j)));
    };
    if s.contains(['.', 'e', 'E']) || (!signed && s.starts_with('-')) {
        return Err(format!(
            "{what}: {s} is not a {}integer",
            if signed { "" } else { "non-negative " }
        ));
    }
    s.parse::<i128>()
        .map_err(|_| format!("{what}: {s} is out of range"))
}

macro_rules! int_readers {
    ($($name:ident: $t:ty, $signed:expr;)*) => {$(
        /// A non-nullable integer of this C# type.
        pub fn $name(j: &Json, what: &str) -> R<$t> {
            let v = integer(j, what, $signed)?;
            <$t>::try_from(v).map_err(|_| format!("{what}: {v} is out of range for {}", stringify!($t)))
        }
    )*};
}

int_readers! {
    i32_: i32, true;
    u32_: u32, false;
    i64_: i64, true;
    u64_: u64, false;
    u8_: u8, false;
    u16_: u16, false;
}

/// A non-nullable `float`.
pub fn f32_(j: &Json, what: &str) -> R<f32> {
    let Json::Num(s) = j else {
        return Err(format!("{what}: expected a number, found {}", kind(j)));
    };
    s.parse::<f32>()
        .map_err(|_| format!("{what}: bad number {s}"))
}

/// A non-nullable `double`.
pub fn f64_(j: &Json, what: &str) -> R<f64> {
    let Json::Num(s) = j else {
        return Err(format!("{what}: expected a number, found {}", kind(j)));
    };
    s.parse::<f64>()
        .map_err(|_| format!("{what}: bad number {s}"))
}

/// A non-nullable `bool`.
pub fn bool_(j: &Json, what: &str) -> R<bool> {
    match j {
        Json::Bool(b) => Ok(*b),
        _ => Err(format!("{what}: expected a boolean, found {}", kind(j))),
    }
}

/// A `string` (a reference type: `null` reads as `None`).
pub fn string(j: &Json, what: &str) -> R<Option<String>> {
    match j {
        Json::Null => Ok(None),
        Json::Str(s) => Ok(Some(s.clone())),
        _ => Err(format!("{what}: expected a string, found {}", kind(j))),
    }
}

/// `Nullable<T>`: `null` reads as `None`, anything else as `T` would.
pub fn opt<T>(j: &Json, what: &str, read: fn(&Json, &str) -> R<T>) -> R<Option<T>> {
    match j {
        Json::Null => Ok(None),
        _ => read(j, what).map(Some),
    }
}

/// A `DateTime` (ISO 8601, as `Utf8JsonReader.TryGetDateTime` takes it). Only validated: no
/// ACE path reads the value it produces.
pub fn datetime(j: &Json, what: &str) -> R<()> {
    let Json::Str(s) = j else {
        return Err(format!("{what}: expected a date string, found {}", kind(j)));
    };
    if iso_8601(s.as_bytes()) {
        Ok(())
    } else {
        Err(format!("{what}: {s:?} is not an ISO 8601 date"))
    }
}

/// `yyyy-MM-dd`, optionally `THH:mm[:ss[.fraction]]` and then `Z` or `±HH:mm`/`±HH`.
fn iso_8601(b: &[u8]) -> bool {
    let num = |r: core::ops::Range<usize>| -> Option<u32> {
        let s = b.get(r)?;
        if !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        core::str::from_utf8(s).ok()?.parse().ok()
    };
    let (Some(y), Some(mo), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || y == 0 || !(1..=12).contains(&mo) {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if d == 0 || d > days[mo as usize - 1] {
        return false;
    }
    if b.len() == 10 {
        return true;
    }
    if b.get(10) != Some(&b'T') {
        return false;
    }
    let (Some(h), Some(mi)) = (num(11..13), num(14..16)) else {
        return false;
    };
    if b.get(13) != Some(&b':') || h > 23 || mi > 59 {
        return false;
    }
    let mut i = 16;
    if b.get(i) == Some(&b':') {
        let Some(s) = num(i + 1..i + 3) else {
            return false;
        };
        if s > 59 {
            return false;
        }
        i += 3;
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            i = start;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return false;
            }
        }
    }
    match b.get(i) {
        None => true,
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+' | b'-') => {
            let Some(oh) = num(i + 1..i + 3) else {
                return false;
            };
            if oh > 14 {
                return false;
            }
            match b.len() - i {
                3 => true,
                6 => b[i + 3] == b':' && num(i + 4..i + 6).is_some_and(|m| m <= 59),
                _ => false,
            }
        }
        _ => false,
    }
}

/// A class-typed property: `null` reads as `None`, an object as its members.
pub fn object<'a>(j: &'a Json, what: &str) -> R<Option<&'a [(String, Json)]>> {
    match j {
        Json::Null => Ok(None),
        Json::Obj(m) => Ok(Some(m)),
        _ => Err(format!("{what}: expected an object, found {}", kind(j))),
    }
}

/// A `List<T>` property: `null` reads as `None`; each element is read by `read`.
pub fn list<T>(j: &Json, what: &str, read: impl Fn(&Json, &str) -> R<T>) -> R<Option<Vec<T>>> {
    match j {
        Json::Null => Ok(None),
        Json::Arr(items) => items
            .iter()
            .map(|e| read(e, what))
            .collect::<R<Vec<T>>>()
            .map(Some),
        _ => Err(format!("{what}: expected an array, found {}", kind(j))),
    }
}
