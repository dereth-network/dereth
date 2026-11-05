//! Property values: the thirteen value types, what each holds, how each is stored, compared,
//! copied from another value and written as text, and the localized-text reference
//! ([`StringInfo`]) one of them holds.
//!
//! A value answers a question about its own type only: asked for a type it is not, it answers
//! nothing (`None`, or `false` from a setter), so a lookup that asks for the wrong type fails as
//! the game's own does.

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};
use auberean_primitives::strings::equal;
use auberean_primitives::{DataId, Vec3};

use crate::common::{same_frame, Color32, Position, Waveform, SAME_FLOAT};
use crate::property::{Names, PropertyDescriptor};
use auberean_dat::QualifiedIds;

/// The value types a property can be declared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueKind {
    Bool,
    Integer,
    Float,
    Vector,
    Color,
    String,
    Enum,
    DataFile,
    Waveform,
    StringInfo,
    Package,
    LongInteger,
    Position,
}

impl ValueKind {
    /// Every kind, in type-number order.
    pub const ALL: [Self; 13] = [
        Self::Bool,
        Self::Integer,
        Self::Float,
        Self::Vector,
        Self::Color,
        Self::String,
        Self::Enum,
        Self::DataFile,
        Self::Waveform,
        Self::StringInfo,
        Self::Package,
        Self::LongInteger,
        Self::Position,
    ];

    /// The kind a declared value-type number names.
    #[must_use]
    pub fn from_number(n: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.number() == n)
    }

    /// The kind's value-type number, as the property list declares it.
    #[must_use]
    pub fn number(self) -> u32 {
        match self {
            Self::Bool => 0x4000_0001,
            Self::Integer => 0x4000_0002,
            Self::Float => 0x4000_0003,
            Self::Vector => 0x4000_0004,
            Self::Color => 0x4000_0005,
            Self::String => 0x4000_0006,
            Self::Enum => 0x4000_0007,
            Self::DataFile => 0x4000_0008,
            Self::Waveform => 0x4000_0009,
            Self::StringInfo => 0x4000_000A,
            Self::Package => 0x4000_000B,
            Self::LongInteger => 0x4000_000D,
            Self::Position => 0x4000_000E,
        }
    }
}

/// One property's value. A copy is a value of the same type holding the same contents.
// D2-16132, D2-16139, D2-16142, D2-16145, D2-16148, D2-16151, D2-16156, D2-16162, D2-16168, D2-16178, D2-16193, D2-16205, D2-16210
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Integer(i32),
    Float(f32),
    Vector(Vec3),
    Color(Color32),
    String(String),
    Enum(u32),
    DataFile(DataId),
    Waveform(Waveform),
    /// A localized-text reference, with the name of its token where one was given (never in the
    /// data files, which store only the reference).
    StringInfo {
        info: StringInfo,
        token_name: String,
    },
    /// A script package id; `u32::MAX` when unset.
    Package(u32),
    LongInteger(i64),
    Position(Position),
}

impl PropertyValue {
    /// A new value of a kind, as the game makes one before reading or setting it: false, zero,
    /// the zero vector, white, the empty string, no file, the default waveform, an empty
    /// reference, no package (`u32::MAX`), the origin of cell 0.
    // D2-16213, D2-16133, D2-16163, D2-16170, D2-16180, D2-16190, D2-16199
    #[must_use]
    pub fn new(kind: ValueKind) -> Self {
        match kind {
            ValueKind::Bool => Self::Bool(false),
            ValueKind::Integer => Self::Integer(0),
            ValueKind::Float => Self::Float(0.0),
            ValueKind::Vector => Self::Vector(Vec3::default()),
            ValueKind::Color => Self::Color(Color32(0xFFFF_FFFF)),
            ValueKind::String => Self::String(String::new()),
            ValueKind::Enum => Self::Enum(0),
            ValueKind::DataFile => Self::DataFile(DataId(0)),
            ValueKind::Waveform => Self::Waveform(Waveform::defaults()),
            ValueKind::StringInfo => Self::StringInfo {
                info: StringInfo::default(),
                token_name: String::new(),
            },
            ValueKind::Package => Self::Package(u32::MAX),
            ValueKind::LongInteger => Self::LongInteger(0),
            ValueKind::Position => Self::Position(Position::default()),
        }
    }

    /// The value's kind.
    #[must_use]
    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Bool(_) => ValueKind::Bool,
            Self::Integer(_) => ValueKind::Integer,
            Self::Float(_) => ValueKind::Float,
            Self::Vector(_) => ValueKind::Vector,
            Self::Color(_) => ValueKind::Color,
            Self::String(_) => ValueKind::String,
            Self::Enum(_) => ValueKind::Enum,
            Self::DataFile(_) => ValueKind::DataFile,
            Self::Waveform(_) => ValueKind::Waveform,
            Self::StringInfo { .. } => ValueKind::StringInfo,
            Self::Package(_) => ValueKind::Package,
            Self::LongInteger(_) => ValueKind::LongInteger,
            Self::Position(_) => ValueKind::Position,
        }
    }

    /// Read a value of a kind: a word for the truth, integer, float, enum, file and package
    /// kinds; three floats for a vector; a packed color word; a narrow string; a waveform's 44
    /// bytes; a reference; 8 bytes; a position's cell and frame.
    // D2-16213, D2-13248, D2-16184, D2-16204, D2-16179, D2-16134, D2-16169, D2-16222
    pub fn read(r: &mut Reader<'_>, kind: ValueKind) -> Result<Self> {
        Ok(match kind {
            ValueKind::Bool => Self::Bool(r.bool32()?),
            ValueKind::Integer => Self::Integer(r.i32()?),
            ValueKind::Float => Self::Float(r.f32()?),
            ValueKind::Vector => Self::Vector(r.vec3()?),
            ValueKind::Color => Self::Color(Color32::read(r)?),
            ValueKind::String => Self::String(r.narrow_string()?),
            ValueKind::Enum => Self::Enum(r.u32()?),
            ValueKind::DataFile => Self::DataFile(r.data_id()?),
            ValueKind::Waveform => Self::Waveform(Waveform::read(r)?),
            ValueKind::StringInfo => Self::StringInfo {
                info: StringInfo::read(r)?,
                token_name: String::new(),
            },
            ValueKind::Package => Self::Package(r.u32()?),
            ValueKind::LongInteger => Self::LongInteger(r.i64()?),
            ValueKind::Position => Self::Position(Position::read(r)?),
        })
    }

    /// Write the value as [`PropertyValue::read`] reads it.
    // D2-16134, D2-16169, D2-16179, D2-16184, D2-16204, D2-16222
    pub fn write(&self, w: &mut Writer) {
        match self {
            Self::Bool(v) => w.bool32(*v),
            Self::Integer(v) => w.i32(*v),
            Self::Float(v) => w.f32(*v),
            Self::Vector(v) => w.vec3(*v),
            Self::Color(c) => c.write(w),
            Self::String(s) => w.narrow_string(s),
            Self::Enum(v) | Self::Package(v) => w.u32(*v),
            Self::DataFile(d) => w.data_id(*d),
            Self::Waveform(v) => v.write(w),
            Self::StringInfo { info, .. } => info.write(w),
            Self::LongInteger(v) => w.i64(*v),
            Self::Position(p) => p.write(w),
        }
    }

    // ---------------------------------------------------------------- one question per type

    // D2-16158
    #[must_use]
    pub fn inq_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16158
    #[must_use]
    pub fn inq_integer(&self) -> Option<i32> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16158
    #[must_use]
    pub fn inq_float(&self) -> Option<f32> {
        match self {
            Self::Float(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16164
    #[must_use]
    pub fn inq_vector(&self) -> Option<Vec3> {
        match self {
            Self::Vector(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16135
    #[must_use]
    pub fn inq_color(&self) -> Option<Color32> {
        match self {
            Self::Color(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16194
    #[must_use]
    pub fn inq_string(&self) -> Option<&str> {
        match self {
            Self::String(v) => Some(v),
            _ => None,
        }
    }

    // D2-16158
    #[must_use]
    pub fn inq_enum(&self) -> Option<u32> {
        match self {
            Self::Enum(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16158
    #[must_use]
    pub fn inq_data_file(&self) -> Option<DataId> {
        match self {
            Self::DataFile(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16172
    #[must_use]
    pub fn inq_waveform(&self) -> Option<&Waveform> {
        match self {
            Self::Waveform(v) => Some(v),
            _ => None,
        }
    }

    /// The reference, and the name of its token.
    // D2-16200, D2-16206
    #[must_use]
    pub fn inq_string_info(&self) -> Option<(&StringInfo, &str)> {
        match self {
            Self::StringInfo { info, token_name } => Some((info, token_name)),
            _ => None,
        }
    }

    // D2-16158
    #[must_use]
    pub fn inq_package(&self) -> Option<u32> {
        match self {
            Self::Package(v) => Some(*v),
            _ => None,
        }
    }

    // D2-16211
    #[must_use]
    pub fn inq_position(&self) -> Option<&Position> {
        match self {
            Self::Position(v) => Some(v),
            _ => None,
        }
    }

    /// Set the value from one of its own type, as the type's setter does; `false` (and nothing
    /// changed) when the value is of another type.
    // D2-16159, D2-16153, D2-16165, D2-16136, D2-16195, D2-16173, D2-16201, D2-16207, D2-16212
    pub fn set(&mut self, v: PropertyValue) -> bool {
        if self.kind() != v.kind() {
            return false;
        }
        match (self, v) {
            (
                Self::StringInfo { info, token_name },
                Self::StringInfo {
                    info: i,
                    token_name: t,
                },
            ) => {
                // The one-argument setter keeps the token's name; the two-argument one sets both.
                *info = i;
                if !t.is_empty() {
                    *token_name = t;
                }
            }
            (me, v) => *me = v,
        }
        true
    }

    /// Take another value's contents, asking it for this value's own type: nothing changes (and
    /// the answer is `false`) when the other is of another type.
    // D2-16130, D2-16137, D2-16140, D2-16143, D2-16146, D2-16149, D2-16154, D2-16160, D2-16166, D2-16174, D2-16182, D2-16191, D2-16202
    pub fn set_from(&mut self, other: &PropertyValue) -> bool {
        if self.kind() != other.kind() {
            return false;
        }
        match (self, other) {
            (
                Self::StringInfo { info, token_name },
                Self::StringInfo {
                    info: i,
                    token_name: t,
                },
            ) => {
                info.clone_from(i);
                token_name.clone_from(t);
            }
            (me, o) => me.clone_from(o),
        }
        true
    }

    /// Whether this value equals another, asking the other for this value's type: integers,
    /// enums, files and packages exactly; floats exactly; vectors, colors, waveforms and
    /// positions to within [`SAME_FLOAT`]; strings ignoring case; references field by field and
    /// their token names exactly.
    // D2-16131, D2-16138, D2-16141, D2-16144, D2-16147, D2-16150, D2-16155, D2-16161, D2-16167, D2-16176, D2-16183, D2-16197, D2-16208
    #[must_use]
    pub fn same(&self, other: &PropertyValue) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::LongInteger(a), Self::LongInteger(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Vector(a), Self::Vector(b)) => {
                (a.x - b.x).abs() < SAME_FLOAT
                    && (a.y - b.y).abs() < SAME_FLOAT
                    && (a.z - b.z).abs() < SAME_FLOAT
            }
            (Self::Color(a), Self::Color(b)) => {
                let (a, b) = (a.channels(), b.channels());
                a.iter().zip(b).all(|(x, y)| (x - y).abs() < SAME_FLOAT)
            }
            (Self::String(a), Self::String(b)) => equal(a.as_bytes(), b.as_bytes(), false),
            (Self::Enum(a), Self::Enum(b)) | (Self::Package(a), Self::Package(b)) => a == b,
            (Self::DataFile(a), Self::DataFile(b)) => a == b,
            (Self::Waveform(a), Self::Waveform(b)) => a.same(b),
            (
                Self::StringInfo {
                    info: a,
                    token_name: ta,
                },
                Self::StringInfo {
                    info: b,
                    token_name: tb,
                },
            ) => a.same(b) && ta == tb,
            (Self::Position(a), Self::Position(b)) => {
                a.cell == b.cell && same_frame(&a.frame, &b.frame)
            }
            _ => false,
        }
    }

    /// Whether the value holds something: a file or enum not 0, a package set, a string not
    /// empty, a reference with a table and a token name, a position in a cell the world has
    /// (`cell_is_valid`, the land's own bounds) with a valid frame; every other kind always does.
    // D2-16157, D2-16171, D2-16181, D2-16192, D2-16203
    #[must_use]
    pub fn has_valid_data(&self, cell_is_valid: &dyn Fn(u32) -> bool) -> bool {
        match self {
            Self::DataFile(d) => d.0 != 0,
            Self::Enum(v) => *v != 0,
            Self::Waveform(w) => w.kind != 0,
            Self::Package(v) => *v != u32::MAX,
            Self::String(s) => !s.is_empty(),
            Self::StringInfo { info, token_name } => info.table.0 != 0 && !token_name.is_empty(),
            Self::Position(p) => cell_is_valid(p.cell.0) && frame_is_valid(&p.frame),
            _ => true,
        }
    }

    /// The value as text, for a property declared by `desc`: `True`/`False`; `%d`; `%.3g`;
    /// three or four `%.3f`s; the string; the enum's name; the file's name or `<file unset>`;
    /// the waveform's shape; `table:token` with `<string table unset>`/`<token unset>`; the
    /// package's name or `<PackageID unset>`; the position. `None` where the game's answer is
    /// that it has none.
    // D2-16214, D2-16215, D2-16216, D2-16217, D2-16218, D2-16196, D2-16260, D2-16261, D2-16175, D2-16219, D2-16262, D2-16220, D2-16226, D2-16254
    #[must_use]
    pub fn as_text(&self, desc: &PropertyDescriptor, names: &dyn Names) -> Option<String> {
        Some(match self {
            Self::Bool(v) => (if *v { "True" } else { "False" }).to_owned(),
            Self::Integer(v) => format!("{v}"),
            // `%d` given a 64-bit value prints its low half.
            #[allow(clippy::cast_possible_truncation)]
            Self::LongInteger(v) => format!("{}", *v as i32),
            Self::Float(v) => format_g(f64::from(*v), 3),
            Self::Vector(v) => format!("{:.3} {:.3} {:.3}", v.x, v.y, v.z),
            Self::Color(c) => {
                let [r, g, b, a] = c.channels();
                format!("{r:.3} {g:.3} {b:.3} {a:.3}")
            }
            Self::String(s) => s.clone(),
            Self::Enum(v) => {
                if desc.value_kind != ValueKind::Enum.number() {
                    return None;
                }
                names.enum_name(desc.data, *v)?
            }
            Self::DataFile(d) => {
                if d.0 == 0 {
                    "<file unset>".to_owned()
                } else {
                    if desc.value_kind != ValueKind::DataFile.number() {
                        return None;
                    }
                    names.file_name(d.0).filter(|n| !n.is_empty())?
                }
            }
            Self::Waveform(w) => w.kind_name().to_owned(),
            Self::StringInfo { info, token_name } => {
                let table = info
                    .table_file_name(names)
                    .unwrap_or_else(|| "<string table unset>".to_owned());
                let token = if token_name.is_empty() {
                    "<token unset>"
                } else {
                    token_name
                };
                format!("{table}:{token}")
            }
            Self::Package(p) => {
                if *p == u32::MAX {
                    "<PackageID unset>".to_owned()
                } else {
                    names.package_name(*p).filter(|n| !n.is_empty())?
                }
            }
            Self::Position(p) => format!(
                "0x{:08X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
                p.cell.0,
                p.frame.origin.x,
                p.frame.origin.y,
                p.frame.origin.z,
                p.frame.rotation.w,
                p.frame.rotation.x,
                p.frame.rotation.y,
                p.frame.rotation.z
            ),
        })
    }

    /// The records the value names: a file; a reference's table and its nested references'.
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        match self {
            Self::DataFile(d) => {
                out.add(d.0);
            }
            Self::StringInfo { info, .. } => info.sub_ids(out),
            _ => {}
        }
    }
}

/// Whether a frame is a valid one: every component a number and the rotation of unit length
/// (within [`SAME_FLOAT`]).
// D2-13157
#[must_use]
pub fn frame_is_valid(f: &auberean_primitives::Frame) -> bool {
    let q = &f.rotation;
    let parts = [f.origin.x, f.origin.y, f.origin.z, q.w, q.x, q.y, q.z];
    let len = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
    !parts.iter().any(|v| v.is_nan()) && !len.is_nan() && (len - 1.0).abs() < SAME_FLOAT
}

/// C's `%.<p>g`: the shorter of `%e` and `%f` at `p` significant digits, trailing zeros removed,
/// the exponent written with at least three digits as the client's runtime writes it.
#[must_use]
pub fn format_g(v: f64, p: usize) -> String {
    if v.is_nan() {
        return "1.#QNAN".to_owned();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-1.#INF" } else { "1.#INF" }.to_owned();
    }
    let p = p.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    // The exponent after rounding to p significant digits.
    let e = format!("{:.*e}", p - 1, v);
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let x: i32 = exp.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    let p = i32::try_from(p).unwrap_or(i32::MAX);
    if x < -4 || x >= p {
        let sign = if x < 0 { '-' } else { '+' };
        format!("{}e{sign}{:03}", strip(mant), x.unsigned_abs())
    } else {
        let decimals = usize::try_from(p - 1 - x).unwrap_or(0);
        strip(&format!("{v:.decimals$}"))
    }
}

// ---------------------------------------------------------------------------- string references

/// A reference to localized text: a token in a string table, or a literal string, and the values
/// of the variables the text substitutes.
// D2-14164, D2-14168, D2-14165, D2-14150, D2-14122, D2-14145
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StringInfo {
    /// The token: the string's id in its table.
    pub token: u32,
    pub table: DataId,
    /// The literal text, when the reference is a literal.
    pub literal: Option<String>,
    /// The variables in their stored order (a later one with a key an earlier one has replaces
    /// it).
    pub variables: Vec<StringVariable>,
}

/// One variable of a [`StringInfo`]: its key and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct StringVariable {
    pub key: u32,
    pub value: VariableValue,
}

/// A text variable's value, by its stored kind.
//
// D2-14208, D2-14210, D2-14211, D2-14212, D2-14214, D2-14216, D2-14218, D2-14220, D2-14222, D2-14223, D2-14224, D2-14166, D2-14118
#[derive(Clone, Debug, PartialEq)]
pub enum VariableValue {
    /// Kind 2.
    Int(i32),
    /// Kind 6.
    UInt(u32),
    /// Kind 4: the value and its number of decimals.
    Float { value: f64, precision: u16 },
    /// Kind 10.
    Long(i64),
    /// Kind 12.
    ULong(u64),
    /// Kind 9: text inside the text.
    Text(Box<StringInfo>),
    /// Kinds 3, 5, 7, 11 and 13: a number with its formatting, kept as stored.
    Formatted { kind: u16, bytes: Vec<u8> },
    /// Any other kind, which stores nothing.
    Empty(u16),
}

impl VariableValue {
    /// The stored kind.
    #[must_use]
    pub fn kind(&self) -> u16 {
        match self {
            Self::Int(_) => 2,
            Self::UInt(_) => 6,
            Self::Float { .. } => 4,
            Self::Long(_) => 10,
            Self::ULong(_) => 12,
            Self::Text(_) => 9,
            Self::Formatted { kind, .. } | Self::Empty(kind) => *kind,
        }
    }

    /// Whether two variables' values are the same: by kind, every stored field equal (a
    /// floating value as a number, so `0` equals `-0` and nothing equals a NaN); a kind that
    /// stores nothing by kind alone.
    // D2-14114, D2-14115, D2-14209, D2-14213, D2-14215, D2-14217, D2-14219, D2-14221, D2-14225
    #[must_use]
    pub fn same(&self, o: &Self) -> bool {
        if self.kind() != o.kind() {
            return false;
        }
        match (self, o) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::UInt(a), Self::UInt(b)) => a == b,
            (Self::Long(a), Self::Long(b)) => a == b,
            (Self::ULong(a), Self::ULong(b)) => a == b,
            (
                Self::Float {
                    value: a,
                    precision: pa,
                },
                Self::Float {
                    value: b,
                    precision: pb,
                },
            ) => a == b && pa == pb,
            (Self::Text(a), Self::Text(b)) => a.same(b),
            (Self::Formatted { kind: 5, bytes: a }, Self::Formatted { bytes: b, .. }) => {
                let f = |x: &[u8]| f64::from_le_bytes(x[..8].try_into().unwrap_or([0; 8]));
                a.len() == 16 && b.len() == 16 && f(a) == f(b) && a[8..] == b[8..]
            }
            (Self::Formatted { bytes: a, .. }, Self::Formatted { bytes: b, .. }) => a == b,
            _ => true,
        }
    }
}

impl StringInfo {
    /// The token word, the table word, a signed 16-bit variable count and a 16-bit literal flag;
    /// the literal (a wide string) when the flag is set; the variables, each a key word, a
    /// 16-bit kind, alignment, the kind's value and alignment; then alignment.
    // D2-14174
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let token = r.u32()?;
        let table = r.data_id()?;
        let count = r.i16()?;
        let has_literal = r.i16()? != 0;
        let literal = if has_literal {
            Some(r.wide_string()?)
        } else {
            None
        };
        let mut variables = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for _ in 0..count.max(0) {
            let key = r.u32()?;
            let kind = r.u16()?;
            r.align()?;
            let value = match kind {
                2 => VariableValue::Int(r.i32()?),
                6 => VariableValue::UInt(r.u32()?),
                4 => VariableValue::Float {
                    value: r.f64()?,
                    precision: r.u16()?,
                },
                10 => VariableValue::Long(r.i64()?),
                12 => VariableValue::ULong(r.u64()?),
                9 => VariableValue::Text(Box::new(Self::read(r)?)),
                3 | 7 => VariableValue::Formatted {
                    kind,
                    bytes: r.bytes(10)?.to_vec(),
                },
                5 => VariableValue::Formatted {
                    kind,
                    bytes: r.bytes(16)?.to_vec(),
                },
                11 | 13 => VariableValue::Formatted {
                    kind,
                    bytes: r.bytes(14)?.to_vec(),
                },
                _ => VariableValue::Empty(kind),
            };
            r.align()?;
            variables.push(StringVariable { key, value });
        }
        r.align()?;
        Ok(Self {
            token,
            table,
            literal,
            variables,
        })
    }

    /// Write the reference as [`StringInfo::read`] reads it.
    // D2-14174
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.token);
        w.data_id(self.table);
        w.i16(i16::try_from(self.variables.len()).unwrap_or(i16::MAX));
        w.i16(i16::from(self.literal.is_some()));
        if let Some(l) = &self.literal {
            w.wide_string(l);
        }
        for v in &self.variables {
            w.u32(v.key);
            w.u16(v.value.kind());
            w.align();
            match &v.value {
                VariableValue::Int(x) => w.i32(*x),
                VariableValue::UInt(x) => w.u32(*x),
                VariableValue::Float { value, precision } => {
                    w.f64(*value);
                    w.u16(*precision);
                }
                VariableValue::Long(x) => w.i64(*x),
                VariableValue::ULong(x) => w.u64(*x),
                VariableValue::Text(t) => t.write(w),
                VariableValue::Formatted { bytes, .. } => w.bytes(bytes),
                VariableValue::Empty(_) => {}
            }
            w.align();
        }
        w.align();
    }

    /// How many bytes [`StringInfo::write`] writes for the reference, starting at an offset
    /// that is a multiple of four.
    // D2-14141
    #[must_use]
    pub fn pack_size(&self) -> usize {
        let pad = |n: usize| n.next_multiple_of(4);
        let mut n = 12;
        if let Some(l) = &self.literal {
            let units = l.encode_utf16().count();
            n += pad(2 + if units >= 0xFFFF { 4 } else { 0 } + units * 2);
        }
        for v in &self.variables {
            n = pad(n + 6);
            n += match &v.value {
                VariableValue::Int(_) | VariableValue::UInt(_) => 4,
                VariableValue::Float { .. } => 10,
                VariableValue::Long(_) | VariableValue::ULong(_) => 8,
                VariableValue::Text(t) => t.pack_size(),
                VariableValue::Formatted { bytes, .. } => bytes.len(),
                VariableValue::Empty(_) => 0,
            };
            n = pad(n);
        }
        pad(n)
    }

    /// The variables as the game holds them: by key, a later one replacing an earlier one.
    #[must_use]
    pub fn variable_map(&self) -> Vec<(u32, &VariableValue)> {
        let mut out: Vec<(u32, &VariableValue)> = Vec::new();
        for v in &self.variables {
            if let Some(slot) = out.iter_mut().find(|(k, _)| *k == v.key) {
                slot.1 = &v.value;
            } else {
                out.push((v.key, &v.value));
            }
        }
        out
    }

    /// Whether two references are the same: the same token, table and literal flag; then the
    /// same literal, or as many variables (by key) and each the same as the other's of its key.
    // D2-14147
    #[must_use]
    pub fn same(&self, o: &Self) -> bool {
        if self.token != o.token || self.table != o.table {
            return false;
        }
        match (&self.literal, &o.literal) {
            (Some(a), Some(b)) => return a == b,
            (None, None) => {}
            _ => return false,
        }
        let (a, b) = (self.variable_map(), o.variable_map());
        a.len() == b.len()
            && a.iter().all(|(k, v)| {
                b.iter()
                    .find(|(kb, _)| kb == k)
                    .is_some_and(|(_, vb)| v.same(vb))
            })
    }

    /// The table's file name, when the reference names a table whose name is known and not
    /// empty.
    // D2-14129
    #[must_use]
    pub fn table_file_name(&self, names: &dyn Names) -> Option<String> {
        if self.table.0 == 0 {
            return None;
        }
        names.file_name(self.table.0).filter(|n| !n.is_empty())
    }

    /// The records a reference names: unless it is a literal, its table and every nested
    /// reference's.
    // D2-14139
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        if self.literal.is_some() {
            return;
        }
        out.add(self.table.0);
        for (_, v) in self.variable_map() {
            if let VariableValue::Text(t) = v {
                t.sub_ids(out);
            }
        }
    }
}
