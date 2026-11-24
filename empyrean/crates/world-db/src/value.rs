//! Values as a row file writes them, and each model field's reading of them.
//!
//! The files are written by this crate's own emitter, which keeps a data id in hexadecimal and a
//! single-precision number as short as it reads back, and read by the `toml` crate.

use empyrean_common::dotnet::DotNetDateTime;
use toml::Value;

/// A value as written.
#[derive(Debug, Clone, PartialEq)]
pub enum V {
    Int(i64),
    /// An id written in hexadecimal.
    Hex(u64),
    /// A landblock, four hexadecimal digits.
    Block(u32),
    /// A float, already spelled.
    Float(String),
    Bool(bool),
    Str(String),
    Array(Vec<V>),
    /// An inline table, its keys in this order.
    Inline(Vec<(String, V)>),
}

/// A key: bare where TOML allows, quoted otherwise.
#[must_use]
pub fn key(k: &str) -> String {
    if !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        k.to_owned()
    } else {
        quote(k)
    }
}

/// A basic string, escaped.
#[must_use]
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A float's spelling: the shortest that reads back, always with a point or an exponent.
#[must_use]
pub fn float_text(text: String) -> String {
    match text.as_str() {
        "NaN" => "nan".into(),
        "inf" => "inf".into(),
        "-inf" => "-inf".into(),
        t if t.contains(['.', 'e', 'E']) => text,
        _ => format!("{text}.0"),
    }
}

impl V {
    /// The value its text reads back as, made without the text.
    #[must_use]
    pub fn value(&self) -> Value {
        match self {
            Self::Int(i) => Value::Integer(*i),
            #[allow(clippy::cast_possible_wrap)]
            Self::Hex(h) => Value::Integer(*h as i64),
            Self::Block(b) => Value::Integer(i64::from(*b)),
            Self::Float(f) => Value::Float(f.parse().unwrap_or(f64::NAN)),
            Self::Bool(b) => Value::Boolean(*b),
            Self::Str(s) => Value::String(s.clone()),
            Self::Array(items) => Value::Array(items.iter().map(Self::value).collect()),
            Self::Inline(fields) => {
                Value::Table(fields.iter().map(|(k, v)| (k.clone(), v.value())).collect())
            }
        }
    }

    /// The value's text.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Int(i) => i.to_string(),
            Self::Hex(h) => format!("0x{h:08X}"),
            Self::Block(b) => format!("0x{b:04X}"),
            Self::Float(f) => f.clone(),
            Self::Bool(b) => b.to_string(),
            Self::Str(s) => quote(s),
            Self::Array(items) => {
                let inner: Vec<String> = items.iter().map(Self::text).collect();
                format!("[{}]", inner.join(", "))
            }
            Self::Inline(fields) => {
                if fields.is_empty() {
                    return "{}".into();
                }
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{} = {}", key(k), v.text()))
                    .collect();
                format!("{{ {} }}", inner.join(", "))
            }
        }
    }
}

/// What a read stops on: the file, and what was wrong.
pub type R<T> = Result<T, String>;

/// A model field: how it's written (nothing at its default), and read back (its default when
/// absent).
pub trait Field: Sized {
    /// The value's spelling.
    fn spell(&self) -> V;
    /// Whether it's the type's default, which a row leaves out.
    fn is_default(&self) -> bool;
    /// The field read back from `v`, or its default when `v` is absent.
    fn take(v: Option<&Value>) -> R<Self>;
    /// The value to write, or `None` to leave the field out.
    fn put(&self) -> Option<V> {
        (!self.is_default()).then(|| self.spell())
    }
}

fn int_of(v: &Value) -> R<i64> {
    match v {
        Value::Integer(i) => Ok(*i),
        other => Err(format!("{other} isn't an integer")),
    }
}

macro_rules! ints {
    ($($t:ty),*) => {$(
        impl Field for $t {
            fn spell(&self) -> V {
                V::Int(i64::from(*self))
            }
            fn is_default(&self) -> bool {
                *self == 0
            }
            fn take(v: Option<&Value>) -> R<Self> {
                v.map_or(Ok(0), |v| {
                    let i = int_of(v)?;
                    <$t>::try_from(i).map_err(|_| format!("{i} is out of range for {}", stringify!($t)))
                })
            }
        }
    )*};
}
ints!(u8, u16, u32, i8, i16, i32, i64);

impl Field for f32 {
    fn spell(&self) -> V {
        V::Float(float_text(format!("{self}")))
    }
    fn is_default(&self) -> bool {
        self.to_bits() == 0
    }
    fn take(v: Option<&Value>) -> R<Self> {
        #[allow(clippy::cast_possible_truncation)] // written from an f32, read back exactly
        v.map_or(Ok(0.0), |v| f64::take(Some(v)).map(|f| f as f32))
    }
}

impl Field for f64 {
    fn spell(&self) -> V {
        V::Float(float_text(format!("{self}")))
    }
    fn is_default(&self) -> bool {
        self.to_bits() == 0
    }
    fn take(v: Option<&Value>) -> R<Self> {
        match v {
            None => Ok(0.0),
            Some(Value::Float(f)) => Ok(*f),
            #[allow(clippy::cast_precision_loss)] // a whole number written for a float
            Some(Value::Integer(i)) => Ok(*i as f64),
            Some(other) => Err(format!("{other} isn't a number")),
        }
    }
}

impl Field for bool {
    fn spell(&self) -> V {
        V::Bool(*self)
    }
    fn is_default(&self) -> bool {
        !*self
    }
    fn take(v: Option<&Value>) -> R<Self> {
        match v {
            None => Ok(false),
            Some(Value::Boolean(b)) => Ok(*b),
            Some(other) => Err(format!("{other} isn't true or false")),
        }
    }
}

impl Field for String {
    fn spell(&self) -> V {
        V::Str(self.clone())
    }
    fn is_default(&self) -> bool {
        self.is_empty()
    }
    fn take(v: Option<&Value>) -> R<Self> {
        match v {
            None => Ok(Self::new()),
            Some(Value::String(s)) => Ok(s.clone()),
            Some(other) => Err(format!("{other} isn't a string")),
        }
    }
}

/// An optional field is written whenever it's set, at its default too: `Some(0)` isn't `None`.
impl<T: Field> Field for Option<T> {
    fn spell(&self) -> V {
        self.as_ref().map_or(V::Int(0), Field::spell)
    }
    fn is_default(&self) -> bool {
        self.is_none()
    }
    fn take(v: Option<&Value>) -> R<Self> {
        v.map(|v| T::take(Some(v))).transpose()
    }
}

/// When the world says nothing else: ACE's world database stamps every row with one time.
pub const STAMP: &str = "2005-02-09 10:00:00";

/// A time as `YYYY-MM-DD HH:MM:SS`, or its ticks when it has a fraction of a second.
#[must_use]
pub fn time_text(t: DotNetDateTime) -> V {
    let whole = DotNetDateTime::new_hms(
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    );
    if whole == t {
        V::Str(t.format("yyyy-MM-dd HH:mm:ss"))
    } else {
        V::Int(t.ticks())
    }
}

/// A time read back from either spelling.
pub fn time_of(v: &Value) -> R<DotNetDateTime> {
    match v {
        Value::Integer(t) => Ok(DotNetDateTime::from_ticks(*t)),
        Value::String(s) => {
            let n: Vec<i32> = s
                .split(['-', ' ', ':'])
                .map(|p| p.parse::<i32>().map_err(|_| format!("{s:?} isn't a time")))
                .collect::<R<_>>()?;
            match n.as_slice() {
                [y, mo, d, h, mi, se] => Ok(DotNetDateTime::new_hms(*y, *mo, *d, *h, *mi, *se)),
                _ => Err(format!("{s:?} isn't YYYY-MM-DD HH:MM:SS")),
            }
        }
        other => Err(format!("{other} isn't a time")),
    }
}

/// A row's `last_modified`, left out when it's the world's stamp.
#[must_use]
pub fn stamp_put(t: DotNetDateTime, stamp: DotNetDateTime) -> Option<V> {
    (t != stamp).then(|| time_text(t))
}

/// A row's `last_modified` read back: the world's stamp when absent.
pub fn stamp_take(v: Option<&Value>, stamp: DotNetDateTime) -> R<DotNetDateTime> {
    v.map_or(Ok(stamp), time_of)
}

/// A data id, cell or guid, written in hexadecimal; read back from either spelling.
#[must_use]
pub fn hex(v: u32) -> V {
    V::Hex(u64::from(v))
}

/// An id read back.
pub fn id_of(v: &Value) -> R<u32> {
    let i = int_of(v)?;
    u32::try_from(i).map_err(|_| format!("{i} isn't an id"))
}
