//! The `Archive` property system: `PropertyCollection`, `BaseProperty` and the value types.
//!
//! This is the typed property bag the client's UI is built out of, and it reaches the wire in
//! exactly one place: the player module's gameplay-options collection, a
//! [`PackObjPropertyCollection`] carried by `0x0013 Login_PlayerDescription` and by the client's
//! own `0x0005 Character_CharacterOptions`.
//!
//! # Why this needs the schema
//!
//! The property serialiser writes the `u32` property id and then the value, **with no
//! length and no type tag**. The reader recovers the type by looking the id up in the globally
//! cached `MasterProperty 0x39000001` (set the id, look up the descriptor, take its initial
//! value, allocate the value class) and letting that value class read itself. A reader without the schema cannot even skip a property, so the schema is
//! transcribed from the retail dat into [`crate::property_types`] and consulted here.
//!
//! # No alignment
//!
//! Every `Serialize` path in this tree reaches the alignment check, and every check is a **no-op**:
//! the alignment check pads only when `flags & 2` is set, and
//! unpacking init clears bits 0 and 2 of a flags word that started at 0.
//! The stream is therefore densely packed — the `u32`s in it are not 4-aligned — which is what the
//! three stored samples show.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::property_types::PROPERTY_TYPES;

/// The discriminant `MasterProperty` stores per property; the client allocates a property's value
/// by switching on it.
pub use dereth_primitives::property::BasePropertyType;

/// The type the retail `MasterProperty` gives a property name, or `None` if no descriptor declares
/// it — in which case the property setter raises an archive error.
#[must_use]
pub fn property_type(name: u32) -> Option<BasePropertyType> {
    PROPERTY_TYPES
        .binary_search_by_key(&name, |&(id, _)| id)
        .ok()
        .map(|i| PROPERTY_TYPES[i].1)
}

/// A reference to a string table entry.
///
/// Every field of the retail string-table record is kept as written so a round trip is
/// byte-exact, including the variables-table bucket index of a table that is empty in all retail
/// data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StringInfo {
    /// The override mode: 0 = look the string up, 1 = the literal follows, 2 = tool-only "adder" mode.
    /// The client forces 2 to 0 when packing, but only outside `ClientAdder`/`ServerAdder`, which
    /// retail is; it is kept verbatim so a decode/encode round trip reproduces the input.
    pub over: u8,
    /// The literal value, present exactly when `over == 1`.
    pub literal: Option<String>,
    /// The string id, present when `over != 1`.
    pub string_id: u32,
    /// The string table id, a `0x23xxxxxx` DataID, present when `over != 1`.
    pub table_id: u32,
    /// The adder triple `(token, english, comment)`. Always absent in retail: the
    /// flag byte is written as 0 unless the program type is an adder build.
    pub adder: Option<(String, String, String)>,
    /// The bucket-size index of the empty variables table.
    pub variables_bucket_index: u8,
}

impl StringInfo {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let over = r.u8()?;
        let (literal, string_id, table_id) = if over == 1 {
            (Some(read_wstring(r)?), 0, 0)
        } else {
            (None, r.u32()?, r.u32()?)
        };
        let flag = r.u8()?;
        let adder = match flag {
            0 => None,
            1 => Some((r.astring()?, r.astring()?, r.astring()?)),
            // Reject any other override value while unpacking.
            _ => {
                return Err(MessageError::InvalidValue {
                    field: "string-info add flag",
                    value: u64::from(flag),
                })
            }
        };
        let vars = r.intrusive_hash_header()?;
        if vars.count != 0 {
            // The variable table maps DataIDs to typed values. Each element begins with a
            // `u16` type tag and a `u32` variable id, followed by type-dependent data.
            // The observed value variants include string-info, double and long-int data.
            // A nonempty table therefore requires a type dispatch after the common header.
            // Its payload boundaries cannot be inferred from the header alone. Every
            // `StringInfo` in the retail dats has an empty table, so the dispatch is not
            // established and is not guessed at here.
            return Err(MessageError::Unsupported {
                field: "string-info variables",
                note: "non-empty string-info variable table; polymorphic element layout not known",
            });
        }
        Ok(Self {
            over,
            literal,
            string_id,
            table_id,
            adder,
            variables_bucket_index: vars.bucket_index,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.over);
        if self.over == 1 {
            write_wstring(w, self.literal.as_deref().unwrap_or(""))?;
        } else {
            w.u32(self.string_id);
            w.u32(self.table_id);
        }
        match &self.adder {
            None => w.u8(0),
            Some((t, e, c)) => {
                w.u8(1);
                w.astring(t)?;
                w.astring(e)?;
                w.astring(c)?;
            }
        }
        w.u8(self.variables_bucket_index);
        w.compressed_u32(0);
        Ok(())
    }
}

/// A wide archive string: a compressed character count then UTF-16LE.
///
/// This is the retail string-table encoding. It is reachable only through an override flag equal
/// to 1, which no shipped data uses.
fn read_wstring(r: &mut Reader<'_>) -> Result<String, MessageError> {
    let n = r.compressed_u32()? as usize;
    if n.saturating_mul(2) > r.remaining() {
        return Err(MessageError::LengthOverrun {
            field: "UTF-16 string length",
            len: n * 2,
            available: r.remaining(),
        });
    }
    let mut units = Vec::with_capacity(n);
    for _ in 0..n {
        units.push(r.u16()?);
    }
    Ok(String::from_utf16_lossy(&units))
}

fn write_wstring(w: &mut Writer, s: &str) -> Result<(), MessageError> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let n = u32::try_from(units.len()).map_err(|_| MessageError::Unencodable {
        field: "UTF-16 string",
        reason: "string longer than 4 Gchars",
    })?;
    w.compressed_u32(n);
    for u in units {
        w.u16(u);
    }
    Ok(())
}

/// One `BasePropertyValue` subclass's payload, in the type `MasterProperty` gives the property.
///
/// The variant names are the client's class names minus the `PropertyValue` suffix.
#[derive(Debug, Clone, PartialEq)]
pub enum BasePropertyValue {
    /// One byte, which
    /// the archive's object serialiser rejects unless it is 0 or 1.
    Bool(bool),
    /// A 32-bit integer value (folded).
    Integer(i32),
    /// A 64-bit integer value (folded).
    LongInteger(i64),
    /// A 32-bit float.
    Float(f32),
    /// 12 bytes in one `GetBytes`.
    Vector([f32; 3]),
    /// A packed
    /// `u32`. Kept packed: the client's own field is four floats, and re-deriving them would not
    /// round-trip.
    Color(u32),
    /// An `Archive` string.
    String(String),
    /// A `StringInfo`.
    StringInfo(StringInfo),
    /// An enum stored as a `u32`; its names come from the descriptor's
    /// `EnumMapper`.
    Enum(u32),
    /// A `u32` DataID.
    DataFile(u32),
    /// A `0x0Axxxxxx` DataID.
    Waveform(u32),
    /// An object instance id (folded).
    InstanceId(u32),
    /// A 32-bit bitfield.
    Bitfield32(u32),
    /// A 64-bit bitfield.
    Bitfield64(u64),
    /// A `u32` count then that many whole `BaseProperty` records.
    Array(Vec<BaseProperty>),
    /// The same hash table as
    /// [`PropertyCollection`].
    Struct(PropertyCollection),
    /// A string-token value encoded as an `Archive` string.
    StringToken(String),
    /// A `u32` naming another property.
    PropertyName(u32),
    /// One byte, 0/1/0xFF.
    TriState(u8),
}

impl BasePropertyValue {
    /// The type discriminant this value belongs to.
    #[must_use]
    pub fn property_type(&self) -> BasePropertyType {
        use BasePropertyType as T;
        match self {
            Self::Bool(_) => T::Bool,
            Self::Integer(_) => T::Integer,
            Self::LongInteger(_) => T::LongInteger,
            Self::Float(_) => T::Float,
            Self::Vector(_) => T::Vector,
            Self::Color(_) => T::Color,
            Self::String(_) => T::String,
            Self::StringInfo(_) => T::StringInfo,
            Self::Enum(_) => T::Enum,
            Self::DataFile(_) => T::DataFile,
            Self::Waveform(_) => T::Waveform,
            Self::InstanceId(_) => T::InstanceId,
            Self::Bitfield32(_) => T::Bitfield32,
            Self::Bitfield64(_) => T::Bitfield64,
            Self::Array(_) => T::Array,
            Self::Struct(_) => T::Struct,
            Self::StringToken(_) => T::StringToken,
            Self::PropertyName(_) => T::PropertyName,
            Self::TriState(_) => T::TriState,
        }
    }

    fn read(r: &mut Reader<'_>, ty: BasePropertyType, depth: u32) -> Result<Self, MessageError> {
        use BasePropertyType as T;
        Ok(match ty {
            T::Bool => match r.u8()? {
                0 => Self::Bool(false),
                1 => Self::Bool(true),
                v => {
                    return Err(MessageError::InvalidValue {
                        field: "Boolean property value",
                        value: u64::from(v),
                    })
                }
            },
            T::Integer => Self::Integer(r.i32()?),
            T::LongInteger => Self::LongInteger(r.i64()?),
            T::Float => Self::Float(r.f32()?),
            T::Vector => Self::Vector([r.f32()?, r.f32()?, r.f32()?]),
            T::Color => Self::Color(r.u32()?),
            T::String => Self::String(r.astring()?),
            T::StringInfo => Self::StringInfo(StringInfo::read(r)?),
            T::Enum => Self::Enum(r.u32()?),
            T::DataFile => Self::DataFile(r.u32()?),
            T::Waveform => Self::Waveform(r.u32()?),
            T::InstanceId => Self::InstanceId(r.u32()?),
            T::Bitfield32 => Self::Bitfield32(r.u32()?),
            T::Bitfield64 => Self::Bitfield64(r.u64()?),
            T::Array => {
                let n = r.u32()? as usize;
                // Bound the count against the bytes remaining before allocating, as the client
                // does. Every element is at least
                // five bytes, but one is enough to make this safe.
                if n > r.remaining() {
                    return Err(MessageError::LengthOverrun {
                        field: "property array element count",
                        len: n,
                        available: r.remaining(),
                    });
                }
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(BaseProperty::read_at(r, depth + 1)?);
                }
                Self::Array(v)
            }
            T::Struct => Self::Struct(PropertyCollection::read_at(r, depth + 1)?),
            T::StringToken => Self::StringToken(r.astring()?),
            T::PropertyName => Self::PropertyName(r.u32()?),
            T::TriState => Self::TriState(r.u8()?),
            // No retail descriptor declares either type, so neither `Serialize` can be reached
            // from a well-formed stream. `Position` would contain a `u32` cell
            // plus seven floats; `TimeStamp` is a `u64` *or* a relative `f64` biased by
            // depending on whether the descriptor marks the timestamp as absolute
            // (the time-stamp variant and the relative-time serialiser). Neither is guessed at.
            T::Position | T::TimeStamp | T::Invalid => {
                return Err(MessageError::Unsupported {
                    field: "BasePropertyValue",
                    note: "Invalid/Position/TimeStamp: no retail property declares these types",
                })
            }
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        match self {
            Self::Bool(v) => w.u8(u8::from(*v)),
            Self::Integer(v) => w.i32(*v),
            Self::LongInteger(v) => w.i64(*v),
            Self::Float(v) => w.f32(*v),
            Self::Vector(v) => {
                for f in v {
                    w.f32(*f);
                }
            }
            Self::Color(v) | Self::Enum(v) | Self::DataFile(v) => w.u32(*v),
            Self::Waveform(v) | Self::InstanceId(v) => w.u32(*v),
            Self::Bitfield32(v) | Self::PropertyName(v) => w.u32(*v),
            Self::Bitfield64(v) => w.u64(*v),
            Self::String(s) | Self::StringToken(s) => w.astring(s)?,
            Self::StringInfo(si) => si.write(w)?,
            Self::Array(items) => {
                let n = u32::try_from(items.len()).map_err(|_| MessageError::Unencodable {
                    field: "property array element count",
                    reason: "more than 4 G elements",
                })?;
                w.u32(n);
                for p in items {
                    p.write(w)?;
                }
            }
            Self::Struct(t) => t.write(w)?,
            Self::TriState(v) => w.u8(*v),
        }
        Ok(())
    }
}

/// One property record: the `u32` property id, then the value.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseProperty {
    /// The descriptor's property id. The client writes 0 for a property with no descriptor
    /// and skips the value entirely; on read a 0 name means "no value follows".
    pub name: u32,
    /// Absent exactly when `name == 0`.
    pub value: Option<BasePropertyValue>,
}

/// How deep a `Struct`/`Array` nest may go before the decoder gives up. The retail data nests two
/// deep (`Option_PlacementArray` → `Option_Placement`); this only has to stop a malicious stream
/// from recursing without bound.
const MAX_DEPTH: u32 = 16;

impl BaseProperty {
    /// Decode one property.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Self::read_at(r, 0)
    }

    fn read_at(r: &mut Reader<'_>, depth: u32) -> Result<Self, MessageError> {
        if depth > MAX_DEPTH {
            return Err(MessageError::InvalidValue {
                field: "property nesting depth",
                value: u64::from(depth),
            });
        }
        let name = r.u32()?;
        if name == 0 {
            // A zero id clears the descriptor, and the client reads a value only when there is
            // a descriptor and a non-zero id, so nothing follows.
            return Ok(Self { name, value: None });
        }
        let Some(ty) = property_type(name) else {
            // The descriptor lookup misses, the property setter returns false, and the archive
            // latches an error. We stop instead of decoding garbage.
            return Err(MessageError::InvalidValue {
                field: "property name",
                value: u64::from(name),
            });
        };
        let value = BasePropertyValue::read(r, ty, depth)?;
        Ok(Self {
            name,
            value: Some(value),
        })
    }

    /// Encode one property.
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.name);
        if self.name == 0 {
            return Ok(());
        }
        let Some(v) = &self.value else {
            return Err(MessageError::Unencodable {
                field: "property value",
                reason: "a named property must carry a value",
            });
        };
        let Some(ty) = property_type(self.name) else {
            return Err(MessageError::Unencodable {
                field: "property name",
                reason: "no retail MasterProperty descriptor declares this name",
            });
        };
        if v.property_type() != ty {
            return Err(MessageError::Unencodable {
                field: "property value",
                reason: "value type disagrees with the MasterProperty descriptor",
            });
        }
        v.write(w)
    }
}

/// An intrusive hash table of
/// `BaseProperty`, and also the body of a structured property value.
///
/// `u8` bucket-size index, a compressed element count, then per element a `u32` key written by the
/// table and the whole `BaseProperty` (whose property-name field repeats the key). The key is kept
/// separately from the property's name because nothing in the format forces them equal, and a round
/// trip must reproduce whatever was sent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropertyCollection {
    /// Index into `archive::BUCKET_SIZES`. Carried verbatim: it is derived from the sender's live
    /// bucket count, not from the element count, so it is not recomputable.
    pub bucket_index: u8,
    /// `(hash key, property)` in the order the sender walked its buckets.
    pub entries: Vec<(u32, BaseProperty)>,
}

impl PropertyCollection {
    /// Decode a collection.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Self::read_at(r, 0)
    }

    fn read_at(r: &mut Reader<'_>, depth: u32) -> Result<Self, MessageError> {
        if depth > MAX_DEPTH {
            return Err(MessageError::InvalidValue {
                field: "property-collection nesting depth",
                value: u64::from(depth),
            });
        }
        let h = r.intrusive_hash_header()?;
        let mut entries = Vec::with_capacity(h.count as usize);
        for _ in 0..h.count {
            let key = r.u32()?;
            entries.push((key, BaseProperty::read_at(r, depth)?));
        }
        Ok(Self {
            bucket_index: h.bucket_index,
            entries,
        })
    }

    /// Encode a collection.
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let n = u32::try_from(self.entries.len()).map_err(|_| MessageError::Unencodable {
            field: "property-collection element count",
            reason: "more than 4 G elements",
        })?;
        w.u8(self.bucket_index);
        w.compressed_u32(n);
        for (key, p) in &self.entries {
            w.u32(*key);
            p.write(w)?;
        }
        Ok(())
    }

    /// The value of the first entry whose property name matches, if any.
    #[must_use]
    pub fn get(&self, name: u32) -> Option<&BasePropertyValue> {
        self.entries
            .iter()
            .find(|(_, p)| p.name == name)
            .and_then(|(_, p)| p.value.as_ref())
    }
}

/// A [`PropertyCollection`] behind its own versioned archive, matching the client's serialized
/// property collection.
///
/// The client builds a versioned archive over the caller's pointer, lets the collection consume as
/// much as it needs, then advances the caller by the consumed byte count. There is no length prefix
/// anywhere; the archive's own header is
/// all that precedes the table.
///
/// # The header
///
/// The versioned archive's unpacking init installs a version-row
/// initialiser, which reads one `u32`. A negative
/// value is an *offset* to a multi-token version row written at the end of the buffer by
/// the footer; a non-negative one is the single `'Core'` token's version, masked
/// with `0x3FFFFFFF`. The packer only takes the offset form when it has more than one token, and
/// the packing init sets exactly one token, `Core` version 2, so retail always writes
/// the plain `2` that all three stored samples begin with.
#[derive(Debug, Clone, PartialEq)]
pub struct PackObjPropertyCollection {
    /// The `'Core'` token's version. 2 in every sample.
    pub version: u32,
    pub properties: PropertyCollection,
}

impl Default for PackObjPropertyCollection {
    fn default() -> Self {
        Self {
            version: Self::CORE_VERSION,
            properties: PropertyCollection::default(),
        }
    }
}

impl PackObjPropertyCollection {
    /// Set archive version tag `0x436F7265` to 2 during versioned-archive packing initialization.
    pub const CORE_VERSION: u32 = 2;

    /// Decode the archive header and the collection.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let head = r.u32()?;
        if head & 0x8000_0000 != 0 {
            // Retail's header reader would seek to `head & 0x3FFFFFFF` and read a version row there. No
            // retail packer produces it: the versioned archive carries one token.
            return Err(MessageError::Unsupported {
                field: "property archive header",
                note: "multi-token version row (offset form); retail writes a single 'Core' entry",
            });
        }
        let version = head & 0x3FFF_FFFF;
        let properties = PropertyCollection::read(r)?;
        Ok(Self {
            version,
            properties,
        })
    }

    /// Encode the archive header and the collection.
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.version & 0xC000_0000 != 0 {
            return Err(MessageError::Unencodable {
                field: "property archive header",
                reason: "version does not fit the 30 bits the archive footer leaves for it",
            });
        }
        w.u32(self.version);
        self.properties.write(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_types::TYPE_CENSUS;

    /// Oracle: `MasterProperty 0x39000001` in the retail `client_portal.dat`, and the
    /// census of it recorded in the recovered property-table format §1.1. The
    /// generated table is checked against a count that was measured independently of it.
    #[test]
    fn the_schema_matches_the_recorded_census() {
        assert_eq!(PROPERTY_TYPES.len(), 383);
        for &(ty, n) in &TYPE_CENSUS {
            assert_eq!(
                PROPERTY_TYPES.iter().filter(|&&(_, t)| t == ty).count(),
                n,
                "{ty:?} count"
            );
        }
        assert_eq!(TYPE_CENSUS.iter().map(|&(_, n)| n).sum::<usize>(), 383);
    }

    /// Oracle: the same dat object. `property_type` binary-searches, so the table must be sorted
    /// and free of duplicates.
    #[test]
    fn the_schema_is_sorted_and_unique() {
        for w in PROPERTY_TYPES.windows(2) {
            assert!(w[0].0 < w[1].0, "unsorted at 0x{:08X}", w[0].0);
        }
        // The two id bands of §1.1.
        assert!(PROPERTY_TYPES.iter().all(
            |&(id, _)| (0x1..=0x181).contains(&id) || (0x1000_0001..=0x1000_00FF).contains(&id)
        ));
    }

    /// Oracle: the worked example in the recovered property-table format §2, which
    /// was read out of the property collection `0x78000001` in the retail portal dat.
    #[test]
    fn the_documented_property_types_agree() {
        use BasePropertyType as T;
        assert_eq!(property_type(0x1000_0081), Some(T::Float)); // Option_ActiveOpacity
        assert_eq!(property_type(0x1000_007F), Some(T::Bitfield64)); // Option_TextType
        assert_eq!(property_type(0x1000_0080), Some(T::Float)); // Option_DefaultOpacity
        assert_eq!(property_type(0x0000_00D2), Some(T::Array)); // GameplayOptionList
        assert_eq!(property_type(0x0000_00D3), Some(T::Struct));
        assert_eq!(property_type(0x0000_00D4), Some(T::StringInfo));
        assert_eq!(property_type(0x0000_0017), Some(T::StringInfo)); // UICore_Text_entry
        assert_eq!(property_type(0x0000_001B), Some(T::Array)); // UICore_Text_font_colors
        assert_eq!(property_type(0), None);
        assert_eq!(property_type(0xDEAD_BEEF), None);
    }

    /// Oracle: the property collection `0x78000001` (`GameplayOptionDefaults`), whose whole 46-byte
    /// payload is hex-dumped in the recovered property-table format §2. Everything
    /// after the leading DataID is a `PropertyCollection`, so the same decoder must consume it
    /// exactly — a second, independent sample of this grammar with three different value types.
    #[test]
    fn the_gameplay_option_defaults_file_decodes_exactly() {
        let bytes = hex(concat!(
            "01000078", // DID = 0x78000001
            "00",
            "03", // bucket index 0, three properties
            "81000010",
            "81000010",
            "0000803F", // Option_ActiveOpacity  = 1.0f
            "7f000010",
            "7f000010",
            "FFFFFF7B01000000", // Option_TextType
            "80000010",
            "80000010",
            "0000003F", // Option_DefaultOpacity = 0.5f
        ));
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u32().unwrap(), 0x7800_0001);
        let c = PropertyCollection::read(&mut r).unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(bytes.len(), 46);
        assert_eq!(c.get(0x1000_0081), Some(&BasePropertyValue::Float(1.0)));
        assert_eq!(
            c.get(0x1000_007F),
            Some(&BasePropertyValue::Bitfield64(0x1_7BFF_FFFF))
        );
        assert_eq!(c.get(0x1000_0080), Some(&BasePropertyValue::Float(0.5)));

        let mut w = Writer::new();
        w.u32(0x7800_0001);
        c.write(&mut w).unwrap();
        assert_eq!(w.as_slice(), &bytes[..]);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }
}
