//! `System.Text.Json` serialization as ACE runs it with `LifestonedConverter.SerializerSettings`
//! (`WriteIndented`, `DefaultIgnoreCondition = WhenWritingNull`,
//! `Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping`).
//!
//! Not ACE-derived code: .NET runtime behaviour, checked against ACE's own export by the
//! `content_export_json` vectors. The rules that matter:
//! * Properties in declaration order; a property whose value is `null` is left out (array
//!   elements are not: a `null` element is written).
//! * Indentation is two spaces per level, `"name": value`, and `Environment.NewLine` between
//!   lines ([`NEW_LINE`]); an empty object or array is `{}` / `[]`; no final newline.
//! * Strings: see [`escape_into`].
//! * `float` / `double`: the shortest round-trippable text (`1E-07`, `0.30000000000000004`, `-0`);
//!   NaN and the infinities make `Utf8JsonWriter` throw `ArgumentException`.
//! * `DateTime`: `yyyy-MM-ddTHH:mm:ss`, then the tick fraction without trailing zeros, then `Z` for
//!   `DateTimeKind.Utc` or the offset for `Local`.
//! * Enums are numbers (the models hold them as integers).

use empyrean_common::dotnet::{to_string, DotNetDateTime};

use super::escape_table::ESCAPED_BMP;

/// `Environment.NewLine`, which `Utf8JsonWriter` puts between indented lines.
pub const NEW_LINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// `DateTime.Kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeKind {
    Unspecified,
    Utc,
    /// DIVERGE: `Local` values are held (and written) as if the host's time zone were UTC; ACE's
    /// text depends on the machine it runs on.
    Local,
}

/// A `DateTime` with its `Kind`, which decides the suffix `System.Text.Json` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonDateTime {
    pub value: DotNetDateTime,
    pub kind: DateTimeKind,
}

impl JsonDateTime {
    /// `default(DateTime)`: `DateTime.MinValue`, `Unspecified`.
    pub const MIN_VALUE: Self = Self {
        value: DotNetDateTime::MIN_VALUE,
        kind: DateTimeKind::Unspecified,
    };

    /// A UTC time (`DateTime.UtcNow`).
    #[must_use]
    pub fn utc(value: DotNetDateTime) -> Self {
        Self {
            value,
            kind: DateTimeKind::Utc,
        }
    }
}

impl Default for JsonDateTime {
    fn default() -> Self {
        Self::MIN_VALUE
    }
}

/// A value as `JsonSerializer` sees it: the models turn themselves into this tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Int(i128),
    F32(f32),
    F64(f64),
    Str(String),
    Date(JsonDateTime),
    Arr(Vec<Node>),
    /// Properties in declaration order (JSON names).
    Obj(Vec<(&'static str, Node)>),
}

/// A type `JsonSerializer.Serialize` can write.
pub trait ToJson {
    fn to_json(&self) -> Node;
}

impl<T: ToJson> ToJson for Option<T> {
    fn to_json(&self) -> Node {
        self.as_ref().map_or(Node::Null, ToJson::to_json)
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn to_json(&self) -> Node {
        Node::Arr(self.iter().map(ToJson::to_json).collect())
    }
}

impl<T: ToJson> ToJson for Box<T> {
    fn to_json(&self) -> Node {
        (**self).to_json()
    }
}

macro_rules! int_to_json {
    ($($t:ty),*) => {$(
        impl ToJson for $t {
            fn to_json(&self) -> Node {
                Node::Int(i128::from(*self))
            }
        }
    )*};
}

int_to_json!(u8, i8, u16, i16, u32, i32, u64, i64);

impl ToJson for bool {
    fn to_json(&self) -> Node {
        Node::Bool(*self)
    }
}

impl ToJson for f32 {
    fn to_json(&self) -> Node {
        Node::F32(*self)
    }
}

impl ToJson for f64 {
    fn to_json(&self) -> Node {
        Node::F64(*self)
    }
}

impl ToJson for String {
    fn to_json(&self) -> Node {
        Node::Str(self.clone())
    }
}

impl ToJson for JsonDateTime {
    fn to_json(&self) -> Node {
        Node::Date(*self)
    }
}

/// What makes `JsonSerializer.Serialize` throw.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SerializeError {
    /// `Utf8JsonWriter` refuses NaN and the infinities (`ArgumentException`).
    #[error(".NET number values cannot be infinity or NaN: {0}")]
    NonFinite(String),
}

/// Write `node` indented, with `new_line` between lines.
pub fn write_indented(node: &Node, new_line: &str) -> Result<String, SerializeError> {
    let mut out = String::new();
    write_node(&mut out, node, 0, new_line)?;
    Ok(out)
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_node(out: &mut String, node: &Node, depth: usize, nl: &str) -> Result<(), SerializeError> {
    match node {
        Node::Null => out.push_str("null"),
        Node::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Node::Int(i) => out.push_str(&i.to_string()),
        Node::F32(f) => {
            if !f.is_finite() {
                return Err(SerializeError::NonFinite(f.to_string()));
            }
            out.push_str(&to_string(*f));
        }
        Node::F64(f) => {
            if !f.is_finite() {
                return Err(SerializeError::NonFinite(f.to_string()));
            }
            out.push_str(&to_string(*f));
        }
        Node::Str(s) => escape_into(out, s),
        Node::Date(d) => {
            out.push('"');
            out.push_str(&date_text(*d));
            out.push('"');
        }
        Node::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(nl);
                indent(out, depth + 1);
                write_node(out, item, depth + 1, nl)?;
            }
            out.push_str(nl);
            indent(out, depth);
            out.push(']');
        }
        Node::Obj(props) => {
            // JsonIgnoreCondition.WhenWritingNull.
            let mut first = true;
            for (name, value) in props {
                if *value == Node::Null {
                    continue;
                }
                out.push_str(if first { "{" } else { "," });
                first = false;
                out.push_str(nl);
                indent(out, depth + 1);
                escape_into(out, name);
                out.push_str(": ");
                write_node(out, value, depth + 1, nl)?;
            }
            if first {
                out.push_str("{}");
            } else {
                out.push_str(nl);
                indent(out, depth);
                out.push('}');
            }
        }
    }
    Ok(())
}

/// Whether the relaxed encoder escapes this BMP code point.
fn escaped_bmp(c: u16) -> bool {
    ESCAPED_BMP
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                core::cmp::Ordering::Less
            } else if lo > c {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// A JSON string as `UnsafeRelaxedJsonEscaping` writes it: HTML-sensitive characters (`<`, `>`,
/// `&`, `'`, `+`, `` ` ``) and all printable non-ASCII stay as they are; `"` and `\` become `\"`
/// and `\\`; `\b`, `\f`, `\n`, `\r`, `\t` their short forms; every other escaped code point
/// `\uXXXX` (upper-case hex), a supplementary-plane character as its UTF-16 surrogate pair.
pub fn escape_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        let cp = u32::from(c);
        match u16::try_from(cp) {
            Ok(u) if !escaped_bmp(u) => out.push(c),
            Ok(_) => match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => push_u(out, cp),
            },
            Err(_) => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    push_u(out, u32::from(*u));
                }
            }
        }
    }
    out.push('"');
}

fn push_u(out: &mut String, unit: u32) {
    out.push_str(&format!("\\u{unit:04X}"));
}

/// `DateTime` as `System.Text.Json` writes it (`JsonWriterHelper.WriteDateTimeTrimmed`).
fn date_text(d: JsonDateTime) -> String {
    let v = d.value;
    let mut s = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        v.year(),
        v.month(),
        v.day(),
        v.hour(),
        v.minute(),
        v.second()
    );
    let fraction = v.ticks() % empyrean_common::dotnet::datetime::TICKS_PER_SECOND;
    if fraction != 0 {
        let digits = format!("{fraction:07}");
        s.push('.');
        s.push_str(digits.trim_end_matches('0'));
    }
    match d.kind {
        DateTimeKind::Unspecified => {}
        DateTimeKind::Utc => s.push('Z'),
        // DIVERGE: the host's offset from UTC is taken as zero (see `DateTimeKind::Local`).
        DateTimeKind::Local => s.push_str("+00:00"),
    }
    s
}
