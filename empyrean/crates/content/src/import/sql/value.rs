//! Values, column types and MySQL's conversions between them: how a literal is stored into a
//! column, how a stored value reads back, and how two values compare.
//!
//! Not ACE-derived: these are MySQL's rules (strict mode unless the file's `sql_mode` drops it),
//! for the column types the ACE world schema uses.

use std::cmp::Ordering;

use super::parse::ColumnDef;
use crate::import::mysqldump::Value;

/// A value while a statement runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Int(i128),
    /// A decimal literal as written (`0.5`, `-12.25`): exact until it meets a column.
    Dec(String),
    Float(f64),
    Bytes(Vec<u8>),
    /// `0x…`: bytes in string context, an unsigned integer in numeric context.
    Hex(Vec<u8>),
}

impl Val {
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }

    /// The value in numeric context, as a double.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Val::Null => None,
            Val::Int(i) => Some(*i as f64),
            Val::Dec(s) => s.parse().ok(),
            Val::Float(f) => Some(*f),
            Val::Bytes(b) => Some(numeric_prefix(b)),
            Val::Hex(h) => Some(hex_uint(h) as f64),
        }
    }

    /// The value as an exact integer, if it is one.
    #[must_use]
    pub fn as_exact_int(&self) -> Option<i128> {
        match self {
            Val::Int(i) => Some(*i),
            Val::Hex(h) => Some(i128::from(hex_uint(h))),
            Val::Dec(s) => {
                let (int, frac) = s.split_once('.').unwrap_or((s, ""));
                if frac.bytes().all(|b| b == b'0') {
                    int.parse().ok()
                } else {
                    None
                }
            }
            #[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
            Val::Float(f) if f.fract() == 0.0 && f.abs() < 1e30 => Some(*f as i128),
            _ => None,
        }
    }

    /// The value in string context.
    #[must_use]
    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Val::Null => None,
            Val::Int(i) => Some(i.to_string().into_bytes()),
            Val::Dec(s) => Some(s.clone().into_bytes()),
            Val::Float(f) => Some(format!("{f}").into_bytes()),
            Val::Bytes(b) | Val::Hex(b) => Some(b.clone()),
        }
    }
}

fn hex_uint(h: &[u8]) -> u64 {
    h.iter()
        .rev()
        .take(8)
        .rev()
        .fold(0u64, |a, &b| (a << 8) | u64::from(b))
}

/// MySQL's string-to-number conversion: the longest numeric prefix, 0 if none.
fn numeric_prefix(b: &[u8]) -> f64 {
    let s = String::from_utf8_lossy(b);
    let t = s.trim_start();
    let mut end = 0;
    let bytes = t.as_bytes();
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_e = false;
    while end < bytes.len() {
        let c = bytes[end];
        let ok = match c {
            b'0'..=b'9' => {
                seen_digit = true;
                true
            }
            b'+' | b'-' => end == 0 || matches!(bytes[end - 1], b'e' | b'E'),
            b'.' if !seen_dot && !seen_e => {
                seen_dot = true;
                true
            }
            b'e' | b'E' if seen_digit && !seen_e => {
                seen_e = true;
                true
            }
            _ => false,
        };
        if !ok {
            break;
        }
        end += 1;
    }
    let mut p = &t[..end];
    while !p.is_empty() && p.parse::<f64>().is_err() {
        p = &p[..p.len() - 1];
    }
    p.parse().unwrap_or(0.0)
}

/// How a column stores its values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Int {
        min: i128,
        max: i128,
    },
    Float,
    Double,
    /// `char`/`varchar`/`text`: at most this many characters.
    Text {
        max_chars: u32,
    },
    DateTime,
    Bit(u32),
}

impl Kind {
    pub fn of(c: &ColumnDef) -> Result<Self, String> {
        let int = |bits: u32| {
            if c.unsigned {
                Kind::Int {
                    min: 0,
                    max: (1i128 << bits) - 1,
                }
            } else {
                Kind::Int {
                    min: -(1i128 << (bits - 1)),
                    max: (1i128 << (bits - 1)) - 1,
                }
            }
        };
        Ok(match c.ty.as_str() {
            "tinyint" | "bool" | "boolean" => int(8),
            "smallint" => int(16),
            "mediumint" => int(24),
            "int" | "integer" => int(32),
            "bigint" => int(64),
            "float" => Kind::Float,
            "double" | "real" => Kind::Double,
            "char" | "varchar" => Kind::Text {
                max_chars: c.len.unwrap_or(1),
            },
            "tinytext" => Kind::Text { max_chars: 255 },
            "text" => Kind::Text { max_chars: 65_535 },
            "mediumtext" => Kind::Text {
                max_chars: 16_777_215,
            },
            "longtext" => Kind::Text {
                max_chars: u32::MAX,
            },
            "datetime" | "timestamp" => Kind::DateTime,
            "bit" => Kind::Bit(c.len.unwrap_or(1)),
            other => {
                return Err(format!(
                    "column `{}`: type {other} is not supported",
                    c.name
                ))
            }
        })
    }
}

/// A stored value read back for comparison.
#[must_use]
pub fn read(kind: Kind, v: &Value<'_>) -> Val {
    let Some(b) = v.bytes() else { return Val::Null };
    match kind {
        Kind::Int { .. } => core::str::from_utf8(b)
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(Val::Null, Val::Int),
        Kind::Bit(_) => match (v, b) {
            (Value::Quoted(_), _) => {
                Val::Int(b.iter().fold(0i128, |a, &x| (a << 8) | i128::from(x)))
            }
            _ => core::str::from_utf8(b)
                .ok()
                .and_then(|s| s.parse().ok())
                .map_or(Val::Null, Val::Int),
        },
        Kind::Float => core::str::from_utf8(b)
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .map_or(Val::Null, |d| {
                #[allow(clippy::cast_possible_truncation)]
                let f = d as f32;
                Val::Float(f64::from(f))
            }),
        Kind::Double => core::str::from_utf8(b)
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(Val::Null, Val::Float),
        Kind::Text { .. } | Kind::DateTime => Val::Bytes(b.to_vec()),
    }
}

/// What storing a value into a column produced: the canonical dump text of the stored value.
pub type Stored = Vec<u8>;

/// Store `v` into a column of `kind`, as MySQL does (`strict`: out-of-range and bad values are
/// errors; otherwise they are clamped or truncated). Returns the dump text (`NULL`, a bare
/// number, or a quoted string).
pub fn store(kind: Kind, v: &Val, strict: bool) -> Result<Stored, String> {
    if v.is_null() {
        return Ok(b"NULL".to_vec());
    }
    match kind {
        Kind::Int { min, max } => {
            let i = match v {
                Val::Int(i) => *i,
                Val::Hex(h) => i128::from(hex_uint(h)),
                Val::Dec(_) | Val::Float(_) => round_half_away(v.as_f64().unwrap_or(0.0)),
                Val::Bytes(b) => {
                    let s = String::from_utf8_lossy(b);
                    match s.trim().parse::<i128>() {
                        Ok(i) => i,
                        Err(_) => {
                            let t = s.trim();
                            if strict && t.parse::<f64>().is_err() {
                                return Err(format!("incorrect integer value '{s}'"));
                            }
                            round_half_away(numeric_prefix(b))
                        }
                    }
                }
                Val::Null => unreachable!("handled above"),
            };
            if i < min || i > max {
                if strict {
                    return Err(format!("out of range value {i}"));
                }
                return Ok(i.clamp(min, max).to_string().into_bytes());
            }
            Ok(i.to_string().into_bytes())
        }
        Kind::Bit(bits) => {
            let i = match v {
                Val::Bytes(b) => b.iter().fold(0i128, |a, &x| (a << 8) | i128::from(x)),
                other => other
                    .as_exact_int()
                    .unwrap_or_else(|| round_half_away(other.as_f64().unwrap_or(0.0))),
            };
            let max = (1i128 << bits) - 1;
            if i < 0 || i > max {
                if strict {
                    return Err(format!("data too long for bit({bits}): {i}"));
                }
                return Ok(max.to_string().into_bytes());
            }
            Ok(i.to_string().into_bytes())
        }
        Kind::Float | Kind::Double => {
            let d = match v {
                Val::Bytes(b) => {
                    let s = String::from_utf8_lossy(b);
                    match s.trim().parse::<f64>() {
                        Ok(d) => d,
                        Err(_) if strict => return Err(format!("incorrect float value '{s}'")),
                        Err(_) => numeric_prefix(b),
                    }
                }
                other => other.as_f64().unwrap_or(0.0),
            };
            if !d.is_finite() {
                return Err(format!("out of range value {d}"));
            }
            // A decimal zero has no sign.
            let d = if d == 0.0 { 0.0 } else { d };
            if kind == Kind::Float {
                #[allow(clippy::cast_possible_truncation)]
                let f = d as f32;
                if !f.is_finite() {
                    return Err(format!("out of range value {d}"));
                }
                Ok(format!("{}", f64::from(f)).into_bytes())
            } else {
                Ok(format!("{d}").into_bytes())
            }
        }
        Kind::Text { max_chars } => {
            let b = v.as_bytes().unwrap_or_default();
            let s = String::from_utf8(b).map_err(|_| "invalid UTF-8 in a text value".to_owned())?;
            let n = s.chars().count();
            let s = if u32::try_from(n).unwrap_or(u32::MAX) > max_chars {
                if strict {
                    return Err(format!(
                        "data too long ({n} characters, the column holds {max_chars})"
                    ));
                }
                s.chars().take(max_chars as usize).collect()
            } else {
                s
            };
            Ok(quote(s.as_bytes()))
        }
        Kind::DateTime => {
            let b = v.as_bytes().unwrap_or_default();
            let s = String::from_utf8_lossy(&b).trim().to_owned();
            let full = if s.len() == 10 {
                format!("{s} 00:00:00")
            } else {
                s.clone()
            };
            let full = full.split('.').next().unwrap_or_default().to_owned();
            let ok = full.len() == 19
                && full.as_bytes()[4] == b'-'
                && full.as_bytes()[7] == b'-'
                && full.as_bytes()[10] == b' '
                && full.as_bytes()[13] == b':'
                && full.as_bytes()[16] == b':';
            if !ok {
                return Err(format!("incorrect datetime value '{s}'"));
            }
            Ok(quote(full.as_bytes()))
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn round_half_away(d: f64) -> i128 {
    d.round() as i128
}

/// A quoted string literal in the dump's escaping.
#[must_use]
pub fn quote(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len() + 2);
    out.push(b'\'');
    for &c in b {
        match c {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\'' => out.extend_from_slice(b"\\'"),
            _ => out.push(c),
        }
    }
    out.push(b'\'');
    out
}

/// `utf8_general_ci` for the world database's names: case-insensitive, trailing spaces ignored
/// (the same rule as `sql_ci_eq`).
#[must_use]
pub fn ci_key(b: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(b);
    s.trim_end_matches(' ').to_lowercase().into_bytes()
}

/// MySQL's comparison of two values: strings compare under the collation, anything else
/// numerically. `None` when either side is NULL.
#[must_use]
pub fn compare(a: &Val, b: &Val) -> Option<Ordering> {
    if a.is_null() || b.is_null() {
        return None;
    }
    match (a, b) {
        (Val::Bytes(x), Val::Bytes(y)) => Some(ci_key(x).cmp(&ci_key(y))),
        (Val::Int(x), Val::Int(y)) => Some(x.cmp(y)),
        _ => {
            if let (Some(x), Some(y)) = (a.as_exact_int(), b.as_exact_int()) {
                if !matches!(a, Val::Float(_) | Val::Bytes(_))
                    && !matches!(b, Val::Float(_) | Val::Bytes(_))
                {
                    return Some(x.cmp(&y));
                }
            }
            a.as_f64()?.partial_cmp(&b.as_f64()?)
        }
    }
}

/// `a LIKE pattern` under the collation, `\` escaping.
#[must_use]
pub fn like(a: &[u8], pattern: &[u8]) -> bool {
    let a: Vec<char> = String::from_utf8_lossy(&ci_key(a)).chars().collect();
    let p: Vec<char> = String::from_utf8_lossy(pattern)
        .to_lowercase()
        .chars()
        .collect();
    fn m(a: &[char], p: &[char]) -> bool {
        match p.first() {
            None => a.is_empty(),
            Some('%') => (0..=a.len()).any(|k| m(&a[k..], &p[1..])),
            Some('_') => !a.is_empty() && m(&a[1..], &p[1..]),
            Some('\\') if p.len() > 1 => a.first() == Some(&p[1]) && m(&a[1..], &p[2..]),
            Some(c) => a.first() == Some(c) && m(&a[1..], &p[1..]),
        }
    }
    m(&a, &p)
}

/// A hashable form of a value for an index: equal values (under [`compare`]) of one column kind
/// hash equally. `None` for NULL.
#[must_use]
pub fn index_part(kind: Kind, v: &Val) -> Option<IndexPart> {
    match kind {
        Kind::Int { .. } | Kind::Bit(_) => match v {
            Val::Null => None,
            other => Some(match other.as_exact_int() {
                Some(i) if !matches!(other, Val::Bytes(_)) => IndexPart::Int(i),
                _ => IndexPart::Other,
            }),
        },
        Kind::Float | Kind::Double => v.as_f64().map(|d| {
            if matches!(v, Val::Bytes(_)) {
                IndexPart::Other
            } else {
                IndexPart::Float(if d == 0.0 { 0 } else { d.to_bits() })
            }
        }),
        Kind::Text { .. } | Kind::DateTime => match v {
            Val::Null => None,
            Val::Bytes(b) => Some(IndexPart::Text(ci_key(b))),
            _ => Some(IndexPart::Other),
        },
    }
}

/// One column's part of an index key. `Other` means the value cannot be looked up through the
/// index (for example a number compared with a text column) and the caller must scan.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexPart {
    Int(i128),
    Float(u64),
    Text(Vec<u8>),
    Other,
}
