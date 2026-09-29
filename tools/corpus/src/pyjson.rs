//! The two JSON shapes the committed corpus files were first written in, reproduced exactly.
//!
//! The message corpus and the packet-capture index were written by Python for their first year,
//! and the files in the tree are that output. Generating them from Rust is only a change of tool if
//! the bytes stay put, so this module writes precisely what the old writer wrote:
//!
//! * [`canonical`]: sorted keys, a two-space indent, `", "`-free item separators, `": "` after a
//!   key, every non-ASCII character escaped, LF line endings and a trailing newline -- the shape of
//!   `json.dumps(obj, sort_keys=True, indent=2, ensure_ascii=True, separators=(',', ': '))`;
//! * [`float_repr`]: a float in its shortest round-trip form, fixed-point between `1e-4` and
//!   `1e16` and scientific (`1e-05`, `1.5e+16`) outside it, always with a `.0` on an integral value;
//! * [`round`]: rounding to `n` decimal places on the exact binary value, ties to even.

use std::collections::BTreeMap;

/// A JSON value with the key order the canonical form wants built in.
#[derive(Debug, Clone)]
pub enum Value {
    /// An object. `BTreeMap` orders keys by their UTF-8 bytes, which is code-point order.
    Obj(BTreeMap<String, Value>),
    /// An array.
    Arr(Vec<Value>),
    /// A string.
    Str(String),
    /// An integer, written in decimal.
    Int(i128),
    /// A float, written by [`float_repr`].
    Float(f64),
    /// `true` or `false`.
    Bool(bool),
    /// `null`.
    Null,
}

impl Value {
    /// An object from `(key, value)` pairs, in any order.
    pub fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Self {
        Self::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// A string value.
    pub fn str(s: &str) -> Self {
        Self::Str(s.to_owned())
    }

    /// An unsigned count.
    pub fn int(n: impl Into<i128>) -> Self {
        Self::Int(n.into())
    }

    /// A value read back from a JSON file: an integer stays an integer and any other number is a
    /// float, which is how the canonical form tells them apart.
    pub fn from_json(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(*b),
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(|i| Self::Int(i.into()))
                .or_else(|| n.as_u64().map(|u| Self::Int(u.into())))
                .unwrap_or_else(|| Self::Float(n.as_f64().unwrap_or(0.0))),
            serde_json::Value::String(s) => Self::Str(s.clone()),
            serde_json::Value::Array(a) => Self::Arr(a.iter().map(Self::from_json).collect()),
            serde_json::Value::Object(m) => Self::Obj(
                m.iter()
                    .map(|(k, v)| (k.clone(), Self::from_json(v)))
                    .collect(),
            ),
        }
    }
}

/// `v` in the canonical form, with its trailing newline.
#[must_use]
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0);
    out.push('\n');
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_value(out: &mut String, v: &Value, level: usize) {
    match v {
        Value::Obj(m) if m.is_empty() => out.push_str("{}"),
        Value::Obj(m) => {
            out.push_str("{\n");
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(out, level + 1);
                write_str(out, k);
                out.push_str(": ");
                write_value(out, v, level + 1);
            }
            out.push('\n');
            indent(out, level);
            out.push('}');
        }
        Value::Arr(a) if a.is_empty() => out.push_str("[]"),
        Value::Arr(a) => {
            out.push_str("[\n");
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(out, level + 1);
                write_value(out, v, level + 1);
            }
            out.push('\n');
            indent(out, level);
            out.push(']');
        }
        Value::Str(s) => write_str(out, s),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(x) => out.push_str(&float_repr(*x)),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
    }
}

/// A JSON string with every non-ASCII character escaped, UTF-16 surrogate pairs above the BMP.
fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7E => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `x` rounded to `digits` decimal places: the exact binary value rounded, ties to even, and read
/// back as the nearest float.
#[must_use]
pub fn round(x: f64, digits: usize) -> f64 {
    format!("{x:.digits$}").parse().unwrap_or(x)
}

/// `x` in its shortest round-trip form, laid out the way the corpus files have always carried it.
#[must_use]
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    // `{:e}` is the shortest round-trip digit string in scientific form: `1.58061e2`, `5e0`, `0e0`.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} always has an exponent");
    let exp: i32 = exp.parse().expect("{:e} exponent is an integer");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let n = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let body = if (-4..16).contains(&exp) {
        if exp < 0 {
            let zeros = usize::try_from(-exp - 1).unwrap_or(0);
            format!("0.{}{digits}", "0".repeat(zeros))
        } else if exp + 1 >= n {
            let zeros = usize::try_from(exp + 1 - n).unwrap_or(0);
            format!("{digits}{}.0", "0".repeat(zeros))
        } else {
            let point = usize::try_from(exp + 1).unwrap_or(0);
            format!("{}.{}", &digits[..point], &digits[point..])
        }
    } else {
        let (head, tail) = digits.split_at(1);
        let frac = if tail.is_empty() {
            String::new()
        } else {
            format!(".{tail}")
        };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{head}{frac}e{esign}{:02}", exp.abs())
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The float layout, against values whose old spelling is known.
    #[test]
    fn floats_are_written_as_the_corpus_files_carry_them() {
        for (x, want) in [
            (0.0, "0.0"),
            (5.0, "5.0"),
            (158.061, "158.061"),
            (0.047047, "0.047047"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (0.000015, "1.5e-05"),
            (1e16, "1e+16"),
            (1234567890123456.0, "1234567890123456.0"),
            (-2.5, "-2.5"),
            (100.0, "100.0"),
        ] {
            assert_eq!(float_repr(x), want, "{x}");
        }
    }

    /// Rounding is on the exact binary value, ties to even: `0.0078125` is exactly representable
    /// and sits exactly half-way at six places.
    #[test]
    fn rounding_is_exact_and_ties_go_to_even() {
        assert_eq!(float_repr(round(0.0078125, 6)), "0.007812");
        assert_eq!(float_repr(round(2.675, 2)), "2.67");
        assert_eq!(float_repr(round(158.0614999, 3)), "158.061");
    }

    /// The canonical layout: sorted keys, two-space indent, empty containers inline, non-ASCII
    /// escaped.
    #[test]
    fn canonical_layout() {
        let v = Value::obj([
            ("b", Value::Arr(vec![Value::int(1), Value::Bool(true)])),
            ("a", Value::str("é\"")),
            ("c", Value::Obj(BTreeMap::new())),
            ("d", Value::Arr(Vec::new())),
        ]);
        assert_eq!(
            canonical(&v),
            "{\n  \"a\": \"\\u00e9\\\"\",\n  \"b\": [\n    1,\n    true\n  ],\n  \"c\": {},\n  \"d\": []\n}\n"
        );
    }
}
