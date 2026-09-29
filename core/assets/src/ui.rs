//! The localisation and UI tables.
//!
//! The reference parser covers string tables, fonts, UI layouts, keymaps, bindings and property
//! tables.
//!
//! Localised text is UTF-16LE with a packed-DWORD **character** count,
//! while the portal-dat `String` type is cp1252 with a packed-DWORD **byte** count. Two tables that
//! look alike, two different length units.
//!
//! `ActionMap` uses the intrusive hash-list header (a packed **bucket count**) for its input maps
//! and the intrusive hash-table header (a `u8` bucket-size **index**) for its conflict table, in
//! the same file.

use std::collections::BTreeMap;

use dereth_dat::archive::{intrusive_hash_list_header, intrusive_hash_table_header};
use dereth_dat::{packobj::read_n, Cursor, DatError, DbType};
use dereth_primitives::{DataId, Position};

use crate::error::AssetError;
use crate::tables::read_position;
use crate::Decode;

/// Intrusive hash-table encoding: the `u8` bucket-size table index, the element count,
/// then the elements in the writer's bucket-then-chain order. Returns the
/// stored index and the elements in stored order, which retail's unpack-time insert preserves.
fn hash_table<T, F>(c: &mut Cursor<'_>, mut value: F) -> Result<(u8, Vec<(u32, T)>), AssetError>
where
    F: FnMut(&mut Cursor<'_>) -> Result<T, AssetError>,
{
    let h = intrusive_hash_table_header(c)?;
    let idx = h.bucket_index.unwrap_or(0);
    let mut v = Vec::new();
    for _ in 0..h.count {
        let k = c.u32()?;
        v.push((k, value(c)?));
    }
    Ok((idx, v))
}

fn hash_list<T, F>(c: &mut Cursor<'_>, mut value: F) -> Result<(u32, Vec<(u32, T)>), AssetError>
where
    F: FnMut(&mut Cursor<'_>) -> Result<T, AssetError>,
{
    let h = intrusive_hash_list_header(c)?;
    // The client's reader refuses a list claiming more than twice as many elements as buckets.
    if u64::from(h.count) > 2 * u64::from(h.buckets) {
        return Err(AssetError::Unsupported {
            what: "intrusive hash list: elements past twice its bucket count",
            value: h.count,
        });
    }
    let mut v = Vec::new();
    for _ in 0..h.count {
        let k = c.u32()?;
        v.push((k, value(c)?));
    }
    Ok((h.buckets, v))
}

// ---------------------------------------------------------------------------------------------
// 0x23xxxxxx StringTable / 0x31xxxxxx String / 0x41000000 language information
// ---------------------------------------------------------------------------------------------

/// One string-table entry: the string itself plus its variable list.
///
/// **Where ACE is wrong**: `Source/ACE.DatLoader/FileTypes/StringTable.cs` splits the `table`
/// DataID into two `u16` counts and calls the variable list "Comments"; it only works because
/// `table` is 0 in every shipped entry.
#[derive(Debug, Clone, PartialEq)]
pub struct StringTableEntry {
    /// A DataID redirecting to a private table. `INVALID_DID` in every retail entry.
    pub table: DataId,
    /// Variant 0 is the singular/default form; the rest are plural/gendered forms.
    pub strings: Vec<String>,
    pub variables: Vec<u32>,
    pub has_var_names: u8,
    pub var_names: Vec<String>,
}

/// `0x23xxxxxx`, `client_local_*.dat`.
#[derive(Debug, Clone, PartialEq)]
pub struct StringTable {
    pub id: DataId,
    /// The client's version field. ACE calls this `Language`; it is not one.
    pub version: u32,
    pub bucket_index: u8,
    pub strings: Vec<(u32, StringTableEntry)>,
}

impl Decode for StringTable {
    const TYPE: DbType = DbType::StringTable;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let version = c.u32()?;
        let (bucket_index, strings) = hash_table(c, |c| {
            let table = c.data_id()?;
            let n = c.u32()? as usize;
            // UTF-16LE with a packed-DWORD *character* count.
            let strings = read_n(c, n, Cursor::archive_wstring)?;
            let n = c.u32()? as usize;
            let variables = read_n(c, n, Cursor::u32)?;
            let has_var_names = c.u8()?;
            let var_names = if has_var_names != 0 {
                let n = c.u32()? as usize;
                read_n(c, n, Cursor::archive_wstring)?
            } else {
                Vec::new()
            };
            Ok(StringTableEntry {
                table,
                strings,
                variables,
                has_var_names,
                var_names,
            })
        })?;
        Ok(Self {
            id,
            version,
            bucket_index,
            strings,
        })
    }
}

/// `String` — `0x31xxxxxx`, `client_portal.dat`.
///
/// cp1252 with a packed-DWORD **byte** count. The character-generation help panels,
/// which were never translated.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageString {
    pub id: DataId,
    pub text: String,
}

impl Decode for LanguageString {
    const TYPE: DbType = DbType::StringType;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        Ok(Self {
            id: c.data_id()?,
            text: c.archive_string()?,
        })
    }
}

/// A `u8` count followed by that many UTF-16 code units — the client's "character list" form,
/// distinct from both string encodings.
fn char_list(c: &mut Cursor<'_>) -> Result<String, DatError> {
    let n = c.u8()? as usize;
    let units: Vec<u16> = read_n(c, n, Cursor::u16)?;
    Ok(String::from_utf16_lossy(&units))
}

/// The `0x41000000` language info, 166 bytes.
///
/// **This file has no `DataID` header**: its serializer skips base-object serialization, so byte 0 is
/// already `version`. Like a surface record, there is nothing to echo-check.
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageInfo {
    pub version: i32,
    pub base: i16,
    pub num_decimal_digits: i16,
    pub leading_zero: u8,
    pub grouping_size: i16,
    pub numerals: String,
    pub decimal_separator: String,
    pub grouping_separator: String,
    pub negative_number_format: String,
    pub is_zero_singular: u8,
    pub is_one_singular: u8,
    pub is_negative_one_singular: u8,
    pub is_two_or_more_singular: u8,
    pub is_negative_two_or_less_singular: u8,
    pub treasure_prefix_letters: String,
    pub treasure_middle_letters: String,
    pub treasure_suffix_letters: String,
    pub male_player_letters: String,
    pub female_player_letters: String,
    /// The 17 IME settings, in file order. Two of them (the other-IME and additional-flags
    /// settings) are the MSVC debug fill `0xCDCDCDCD` in the shipped file; treat those as unset,
    /// not as flags.
    pub ime: [u32; 17],
    pub word_wrap_on_space: i32,
    /// Empty in English; the grammar is unknown.
    pub additional_settings: String,
    pub additional_flags: u32,
}

impl Decode for LanguageInfo {
    const TYPE: DbType = DbType::StringState;
    fn declared_id(&self) -> Option<DataId> {
        None
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let version = c.i32()?;
        let base = c.i16()?;
        let num_decimal_digits = c.i16()?;
        let leading_zero = c.u8()?;
        let grouping_size = c.i16()?;
        let numerals = char_list(c)?;
        let decimal_separator = char_list(c)?;
        let grouping_separator = char_list(c)?;
        let negative_number_format = char_list(c)?;
        let is_zero_singular = c.u8()?;
        let is_one_singular = c.u8()?;
        let is_negative_one_singular = c.u8()?;
        let is_two_or_more_singular = c.u8()?;
        let is_negative_two_or_less_singular = c.u8()?;
        c.align_ptr();
        let treasure_prefix_letters = char_list(c)?;
        let treasure_middle_letters = char_list(c)?;
        let treasure_suffix_letters = char_list(c)?;
        let male_player_letters = char_list(c)?;
        let female_player_letters = char_list(c)?;
        let mut ime = [0u32; 17];
        for v in &mut ime {
            *v = c.u32()?;
        }
        let word_wrap_on_space = c.i32()?;
        let additional_settings = char_list(c)?;
        let additional_flags = c.u32()?;
        Ok(Self {
            version,
            base,
            num_decimal_digits,
            leading_zero,
            grouping_size,
            numerals,
            decimal_separator,
            grouping_separator,
            negative_number_format,
            is_zero_singular,
            is_one_singular,
            is_negative_one_singular,
            is_two_or_more_singular,
            is_negative_two_or_less_singular,
            treasure_prefix_letters,
            treasure_middle_letters,
            treasure_suffix_letters,
            male_player_letters,
            female_player_letters,
            ime,
            word_wrap_on_space,
            additional_settings,
            additional_flags,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x40xxxxxx Font
// ---------------------------------------------------------------------------------------------

/// One glyph's placement in the font's texture pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontChar {
    pub unicode: u16,
    pub offset_x: u16,
    pub offset_y: u16,
    pub width: u8,
    pub height: u8,
    pub h_offset_before: u8,
    pub h_offset_after: u8,
    pub v_offset_before: u8,
}

/// `Font` — `0x40xxxxxx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    pub id: DataId,
    pub max_char_height: u32,
    pub max_char_width: u32,
    pub chars: Vec<FontChar>,
    pub num_horizontal_border_pixels: u32,
    pub num_vertical_border_pixels: u32,
    pub baseline_offset: u32,
    pub foreground_surface: DataId,
    pub background_surface: DataId,
}

impl Decode for Font {
    const TYPE: DbType = DbType::Font;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let max_char_height = c.u32()?;
        let max_char_width = c.u32()?;
        let n = c.u32()? as usize;
        let chars = read_n(c, n, |c| {
            Ok(FontChar {
                unicode: c.u16()?,
                offset_x: c.u16()?,
                offset_y: c.u16()?,
                width: c.u8()?,
                height: c.u8()?,
                h_offset_before: c.u8()?,
                h_offset_after: c.u8()?,
                v_offset_before: c.u8()?,
            })
        })?;
        Ok(Self {
            id,
            max_char_height,
            max_char_width,
            chars,
            num_horizontal_border_pixels: c.u32()?,
            num_vertical_border_pixels: c.u32()?,
            baseline_offset: c.u32()?,
            foreground_surface: c.data_id()?,
            background_surface: c.data_id()?,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x39000001 MasterProperty and the BaseProperty value union
// ---------------------------------------------------------------------------------------------

/// The value's type tag. The value's wire form depends entirely on it, and the mapping from
/// property id to type lives only in the `MasterProperty` table — which is why the layout and
/// property readers take a type map.
pub use dereth_primitives::property::BasePropertyType;

/// property id → its `BasePropertyType`, as read out of the `MasterProperty` table.
pub type PropertyTypes = BTreeMap<u32, u32>;

/// A decoded `StringInfo`: an optional literal plus the variables that fill it.
#[derive(Debug, Clone, PartialEq)]
pub struct StringInfo {
    pub override_flag: u8,
    pub literal: Option<String>,
    pub string_id: Option<u32>,
    pub table_id: Option<DataId>,
    /// Always 0 in the retail client.
    pub is_adder: u8,
    pub adder: Option<(String, String, String)>,
    pub variables: Vec<(u32, StringInfo)>,
}

fn read_string_info(c: &mut Cursor<'_>) -> Result<StringInfo, AssetError> {
    let override_flag = c.u8()?;
    let (literal, string_id, table_id) = if override_flag == 1 {
        (Some(c.archive_wstring()?), None, None)
    } else {
        (None, Some(c.u32()?), Some(c.data_id()?))
    };
    let is_adder = c.u8()?;
    let adder = if is_adder != 0 {
        Some((
            c.archive_string()?,
            c.archive_string()?,
            c.archive_string()?,
        ))
    } else {
        None
    };
    let (_, variables) = hash_table(c, read_string_info)?;
    Ok(StringInfo {
        override_flag,
        literal,
        string_id,
        table_id,
        is_adder,
        adder,
        variables,
    })
}

/// One property value, in whichever shape its type says.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PropertyValue {
    Bool(bool),
    Integer(i32),
    LongInteger(u64),
    Float(f32),
    Vector(f32, f32, f32),
    Color(u32),
    String(String),
    StringInfo(Box<StringInfo>),
    Enum(u32),
    DataFile(DataId),
    Waveform(DataId),
    InstanceId(u32),
    Position(Box<Position>),
    TimeStamp(u64),
    Bitfield32(u32),
    Bitfield64(u64),
    Array(Vec<BaseProperty>),
    Struct(Vec<(u32, BaseProperty)>),
    StringToken(String),
    PropertyName(u32),
    TriState(u8),
}

/// One property record: a `u32` property id, then the value in the type the
/// `MasterProperty` table gives that id.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseProperty {
    pub id: u32,
    pub value: PropertyValue,
}

/// Read one `BaseProperty`, resolving its type through the master table.
pub fn read_base_property(
    c: &mut Cursor<'_>,
    types: &PropertyTypes,
) -> Result<BaseProperty, AssetError> {
    let id = c.u32()?;
    let raw = types.get(&id).copied().ok_or(AssetError::Unsupported {
        what: "BaseProperty id with no MasterProperty row",
        value: id,
    })?;
    let ty = BasePropertyType::from_u32(raw).ok_or(AssetError::Unsupported {
        what: "BasePropertyType",
        value: raw,
    })?;
    let value = match ty {
        BasePropertyType::Bool => PropertyValue::Bool(c.u8()? != 0),
        BasePropertyType::Integer => PropertyValue::Integer(c.i32()?),
        BasePropertyType::LongInteger => PropertyValue::LongInteger(c.u64()?),
        BasePropertyType::Float => PropertyValue::Float(c.f32()?),
        BasePropertyType::Vector => PropertyValue::Vector(c.f32()?, c.f32()?, c.f32()?),
        BasePropertyType::Color => PropertyValue::Color(c.u32()?),
        BasePropertyType::String => PropertyValue::String(c.archive_string()?),
        BasePropertyType::StringInfo => PropertyValue::StringInfo(Box::new(read_string_info(c)?)),
        BasePropertyType::Enum => PropertyValue::Enum(c.u32()?),
        BasePropertyType::DataFile => PropertyValue::DataFile(c.data_id()?),
        BasePropertyType::Waveform => PropertyValue::Waveform(c.data_id()?),
        BasePropertyType::InstanceId => PropertyValue::InstanceId(c.u32()?),
        BasePropertyType::Position => PropertyValue::Position(Box::new(read_position(c)?)),
        BasePropertyType::TimeStamp => PropertyValue::TimeStamp(c.u64()?),
        BasePropertyType::Bitfield32 => PropertyValue::Bitfield32(c.u32()?),
        BasePropertyType::Bitfield64 => PropertyValue::Bitfield64(c.u64()?),
        BasePropertyType::Array => {
            let n = c.u32()?;
            let mut v = Vec::new();
            for _ in 0..n {
                v.push(read_base_property(c, types)?);
            }
            PropertyValue::Array(v)
        }
        BasePropertyType::Struct => {
            let (_, v) = hash_table(c, |c| read_base_property(c, types))?;
            PropertyValue::Struct(v)
        }
        BasePropertyType::StringToken => PropertyValue::StringToken(c.archive_string()?),
        BasePropertyType::PropertyName => PropertyValue::PropertyName(c.u32()?),
        BasePropertyType::TriState => PropertyValue::TriState(c.u8()?),
        BasePropertyType::Invalid => {
            return Err(AssetError::Unsupported {
                what: "BasePropertyType::Invalid",
                value: raw,
            })
        }
    };
    Ok(BaseProperty { id, value })
}

/// One `MasterProperty` row: everything the client knows about one property id.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDesc {
    pub name: u32,
    pub property_type: u32,
    pub group: u32,
    pub provider: u32,
    pub data: u32,
    pub patch_flags: u32,
    pub default: Option<PropertyValue>,
    pub min: Option<PropertyValue>,
    pub max: Option<PropertyValue>,
    pub prediction_timeout: f32,
    pub inheritance_type: u8,
    pub dat_file_type: u8,
    pub propagation_type: u8,
    pub caching_type: u8,
    /// required, read_only, no_checkpoint, recorded, do_not_replay, absolute_time_stamp, groupable,
    /// propagate_to_children.
    pub flags: [u8; 8],
    pub available_properties: Vec<u32>,
}

/// Master-property table payload for data id `0x39000001`.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterProperty {
    pub id: DataId,
    pub unk0: u32,
    pub unk1: u32,
    pub enum_names: Vec<(u32, String)>,
    pub properties: Vec<(u32, PropertyDesc)>,
}

impl MasterProperty {
    /// The property-id → type map every `BaseProperty` reader needs.
    #[must_use]
    pub fn property_types(&self) -> PropertyTypes {
        self.properties
            .iter()
            .map(|(k, d)| (*k, d.property_type))
            .collect()
    }
}

/// The subset of value types a `MasterProperty` row's default/min/max may use.
fn read_desc_value(c: &mut Cursor<'_>, raw: u32) -> Result<PropertyValue, AssetError> {
    Ok(match BasePropertyType::from_u32(raw) {
        Some(BasePropertyType::Bool) => PropertyValue::Bool(c.u8()? != 0),
        Some(BasePropertyType::Color) => PropertyValue::Color(c.u32()?),
        Some(BasePropertyType::DataFile) => PropertyValue::DataFile(c.data_id()?),
        Some(BasePropertyType::Enum) => PropertyValue::Enum(c.u32()?),
        Some(BasePropertyType::Float) => PropertyValue::Float(c.f32()?),
        Some(BasePropertyType::Integer) => PropertyValue::Integer(c.i32()?),
        Some(BasePropertyType::Vector) => PropertyValue::Vector(c.f32()?, c.f32()?, c.f32()?),
        _ => {
            return Err(AssetError::Unsupported {
                what: "MasterProperty default/min/max type",
                value: raw,
            })
        }
    })
}

impl Decode for MasterProperty {
    const TYPE: DbType = DbType::MasterProperty;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let unk0 = c.u32()?;
        let unk1 = c.u32()?;
        let (_, enum_names) = hash_table(c, |c| Ok(c.archive_string()?))?;
        let (_, properties) = hash_table(c, |c| {
            let name = c.u32()?;
            let property_type = c.u32()?;
            let group = c.u32()?;
            let provider = c.u32()?;
            let data = c.u32()?;
            let patch_flags = c.u32()?;
            let opt = |c: &mut Cursor<'_>| -> Result<Option<PropertyValue>, AssetError> {
                if c.u8()? != 0 {
                    Ok(Some(read_desc_value(c, property_type)?))
                } else {
                    Ok(None)
                }
            };
            let default = opt(c)?;
            let min = opt(c)?;
            let max = opt(c)?;
            let prediction_timeout = c.f32()?;
            let inheritance_type = c.u8()?;
            let dat_file_type = c.u8()?;
            let propagation_type = c.u8()?;
            let caching_type = c.u8()?;
            let mut flags = [0u8; 8];
            for f in &mut flags {
                *f = c.u8()?;
            }
            let (_, avail) = hash_table(c, |c| Ok(c.u32()?))?;
            Ok(PropertyDesc {
                name,
                property_type,
                group,
                provider,
                data,
                patch_flags,
                default,
                min,
                max,
                prediction_timeout,
                inheritance_type,
                dat_file_type,
                propagation_type,
                caching_type,
                flags,
                available_properties: avail.into_iter().map(|(k, _)| k).collect(),
            })
        })?;
        Ok(Self {
            id,
            unk0,
            unk1,
            enum_names,
            properties,
        })
    }
}

/// The DAT property collection — `0x78xxxxxx`. Needs the `MasterProperty` type map, so it
/// does not implement [`Decode`].
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAsset {
    pub id: DataId,
    pub properties: Vec<(u32, BaseProperty)>,
}

impl PropertyAsset {
    /// `DB_TYPE_DBPROPERTIES`.
    pub const TYPE: DbType = DbType::DbProperties;

    pub fn decode_payload(
        id: DataId,
        bytes: &[u8],
        types: &PropertyTypes,
    ) -> Result<Self, AssetError> {
        let mut c = Cursor::new(bytes);
        let found = c.data_id()?;
        crate::dbobj::check_id_echo(id, found)?;
        let (_, properties) = hash_table(&mut c, |c| read_base_property(c, types))?;
        c.expect_end()?;
        Ok(Self {
            id: found,
            properties,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x21xxxxxx LayoutDesc
// ---------------------------------------------------------------------------------------------

/// One `MediaDesc`. Its serialized type dword appears twice: the container layer writes the
/// first copy and the concrete media record writes the second.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaDesc {
    pub media_type: u32,
    /// Whether the second copy of the type dword matched the first. True in every shipped file.
    pub type_echo_ok: bool,
    pub fields: MediaFields,
}

/// The per-type payload of a [`MediaDesc`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MediaFields {
    Movie {
        file_name: String,
        stretch_to_full_screen: u8,
    },
    Alpha {
        file: DataId,
    },
    Anim {
        duration: f32,
        draw_mode: u32,
        frames: Vec<DataId>,
    },
    Cursor {
        file: DataId,
        x_hotspot: i32,
        y_hotspot: i32,
    },
    Image {
        file: DataId,
        draw_mode: u32,
    },
    Jump {
        jump_item_index: u32,
        probability: f32,
    },
    Message {
        message_id: u32,
        probability: f32,
    },
    Pause {
        min_duration: f32,
        max_duration: f32,
    },
    Sound {
        file: DataId,
        sound_type: u32,
    },
    State {
        state_id: u32,
        probability: f32,
    },
    Fade {
        start_alpha: f32,
        end_alpha: f32,
        duration: f32,
    },
}

fn read_media_desc(c: &mut Cursor<'_>) -> Result<MediaDesc, AssetError> {
    let t = c.u32()?;
    let echo = c.u32()?;
    let fields = match t {
        1 => MediaFields::Movie {
            file_name: c.archive_string()?,
            stretch_to_full_screen: c.u8()?,
        },
        2 => MediaFields::Alpha { file: c.data_id()? },
        3 => {
            let duration = c.f32()?;
            let draw_mode = c.u32()?;
            let n = c.u32()? as usize;
            MediaFields::Anim {
                duration,
                draw_mode,
                frames: read_n(c, n, Cursor::data_id)?,
            }
        }
        4 => MediaFields::Cursor {
            file: c.data_id()?,
            x_hotspot: c.i32()?,
            y_hotspot: c.i32()?,
        },
        5 => MediaFields::Image {
            file: c.data_id()?,
            draw_mode: c.u32()?,
        },
        6 => MediaFields::Jump {
            jump_item_index: c.u32()?,
            probability: c.f32()?,
        },
        7 => MediaFields::Message {
            message_id: c.u32()?,
            probability: c.f32()?,
        },
        8 => MediaFields::Pause {
            min_duration: c.f32()?,
            max_duration: c.f32()?,
        },
        9 => MediaFields::Sound {
            file: c.data_id()?,
            sound_type: c.u32()?,
        },
        10 => MediaFields::State {
            state_id: c.u32()?,
            probability: c.f32()?,
        },
        11 => MediaFields::Fade {
            start_alpha: c.f32()?,
            end_alpha: c.f32()?,
            duration: c.f32()?,
        },
        other => {
            return Err(AssetError::UnknownTag {
                what: "media descriptor",
                tag: other,
            })
        }
    };
    Ok(MediaDesc {
        media_type: t,
        type_echo_ok: echo == t,
        fields,
    })
}

/// One UI state: the properties and media the state applies to an element.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDesc {
    pub state_id: u32,
    pub pass_to_children: bool,
    pub incorporation_flags: u32,
    pub properties: Vec<(u32, BaseProperty)>,
    pub media: Vec<MediaDesc>,
}

fn read_state_desc(c: &mut Cursor<'_>, types: &PropertyTypes) -> Result<StateDesc, AssetError> {
    let state_id = c.u32()?;
    let pass_to_children = c.u8()? != 0;
    let incorporation_flags = c.u32()?;
    let (_, properties) = hash_table(c, |c| read_base_property(c, types))?;
    let n = c.compressed_u32()?;
    let mut media = Vec::new();
    for _ in 0..n {
        media.push(read_media_desc(c)?);
    }
    Ok(StateDesc {
        state_id,
        pass_to_children,
        incorporation_flags,
        properties,
        media,
    })
}

/// One UI element description. A `StateDesc` followed by the element's own fields, whose
/// optional geometry is selected by the `incorporation_flags` the `StateDesc` already read.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementDesc {
    pub state: StateDesc,
    pub ui_read_order: u32,
    pub element_id: u32,
    pub element_type: u32,
    pub base_element: u32,
    pub base_layout: DataId,
    pub default_state: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub z_level: Option<i32>,
    pub left_edge: u32,
    pub top_edge: u32,
    pub right_edge: u32,
    pub bottom_edge: u32,
    pub states: Vec<(u32, StateDesc)>,
    pub children: Vec<(u32, ElementDesc)>,
    /// The child table's bucket count: the bucket-size table's entry for the `u8` index the
    /// record stores, so a stored `1` is 23. This controls traversal order: the client creates the
    /// children by walking that table bucket by bucket (`element_id % children_buckets`), and
    /// adding a child keeps creation order for siblings that tie on `z_level`
    /// and `ui_read_order` -- which is what decides whether the skills footer's label draws under
    /// or over its meter. `children` is in the stored chain order.
    pub children_buckets: u32,
}

fn read_element_desc(c: &mut Cursor<'_>, types: &PropertyTypes) -> Result<ElementDesc, AssetError> {
    let state = read_state_desc(c, types)?;
    let ui_read_order = c.u32()?;
    let element_id = c.u32()?;
    let element_type = c.u32()?;
    let base_element = c.u32()?;
    let base_layout = c.data_id()?;
    let default_state = c.u32()?;
    let fl = state.incorporation_flags;
    let x = if fl & 0x02 != 0 { Some(c.i32()?) } else { None };
    let y = if fl & 0x04 != 0 { Some(c.i32()?) } else { None };
    let width = if fl & 0x08 != 0 { Some(c.i32()?) } else { None };
    let height = if fl & 0x10 != 0 { Some(c.i32()?) } else { None };
    let z_level = if fl & 0x20 != 0 { Some(c.i32()?) } else { None };
    let left_edge = c.u32()?;
    let top_edge = c.u32()?;
    let right_edge = c.u32()?;
    let bottom_edge = c.u32()?;
    let (_, states) = hash_table(c, |c| read_state_desc(c, types))?;
    let (children_bucket_index, children) = hash_table(c, |c| read_element_desc(c, types))?;
    // The client resizes to the bucket-size table's entry for the index on the reading side; the
    // header reader has already refused an
    // index past the table, so the fallback is unreachable and only keeps the lookup total.
    let children_buckets = dereth_dat::archive::BUCKET_SIZES
        .get(usize::from(children_bucket_index))
        .copied()
        .unwrap_or(0);
    Ok(ElementDesc {
        state,
        ui_read_order,
        element_id,
        element_type,
        base_element,
        base_layout,
        default_state,
        x,
        y,
        width,
        height,
        z_level,
        left_edge,
        top_edge,
        right_edge,
        bottom_edge,
        states,
        children,
        children_buckets,
    })
}

/// `0x21xxxxxx`, `client_local_*.dat`.
///
/// Needs the `MasterProperty` type map, so it does not implement [`Decode`]. That mirrors the
/// client: `MasterProperty` is loaded before any layout can be read.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutDesc {
    pub id: DataId,
    pub display_width: i32,
    pub display_height: i32,
    pub elements: Vec<(u32, ElementDesc)>,
}

impl LayoutDesc {
    /// `DB_TYPE_UI_LAYOUT`.
    pub const TYPE: DbType = DbType::UiLayout;

    pub fn decode_payload(
        id: DataId,
        bytes: &[u8],
        types: &PropertyTypes,
    ) -> Result<Self, AssetError> {
        let mut c = Cursor::new(bytes);
        let found = c.data_id()?;
        crate::dbobj::check_id_echo(id, found)?;
        let display_width = c.i32()?;
        let display_height = c.i32()?;
        let (_, elements) = hash_table(&mut c, |c| read_element_desc(c, types))?;
        c.expect_end()?;
        Ok(Self {
            id: found,
            display_width,
            display_height,
            elements,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x26000000 ActionMap / 0x14xxxxxx master input map
// ---------------------------------------------------------------------------------------------

/// One user binding: the action class and the two string ids that name and describe it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserBindingValue {
    pub action_class: u32,
    pub action_name_strid: u32,
    pub description_strid: u32,
}

/// One action-map entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMapValue {
    /// The serialized magic number must be 0.
    pub magic: u32,
    /// Read into a `bool` that is then discarded.
    pub unused_bool: u8,
    pub toggle_type: u32,
    /// A list the original reader discards after decoding. It remains here so a round trip can
    /// reproduce the serialized bytes.
    pub scratch_list: Vec<u32>,
    pub binding: UserBindingValue,
}

/// A decoded `0x26000000` ActionMap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMap {
    pub id: DataId,
    /// Read with the intrusive hash-list header — a packed **bucket count**, not an index.
    pub input_maps: Vec<(u32, Vec<(u32, ActionMapValue)>)>,
    pub string_table: DataId,
    /// Read with the intrusive hash-table header — a `u8` bucket-size **index**. The
    /// same file uses both headers, so confusing them desynchronises the whole action map.
    pub conflicting_maps: Vec<(u32, (u32, Vec<u32>))>,
    pub conflict_bucket_index: u8,
}

fn read_ulong_list(c: &mut Cursor<'_>) -> Result<Vec<u32>, DatError> {
    let n = c.u32()? as usize;
    read_n(c, n, Cursor::u32)
}

impl Decode for ActionMap {
    const TYPE: DbType = DbType::ActionMap;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let (_, input_maps) = hash_list(c, |c| {
            let (_, inner) = hash_list(c, |c| {
                Ok(ActionMapValue {
                    magic: c.u32()?,
                    unused_bool: c.u8()?,
                    toggle_type: c.u32()?,
                    scratch_list: read_ulong_list(c)?,
                    binding: UserBindingValue {
                        action_class: c.u32()?,
                        action_name_strid: c.u32()?,
                        description_strid: c.u32()?,
                    },
                })
            })?;
            Ok(inner)
        })?;
        let string_table = c.data_id()?;
        let (conflict_bucket_index, conflicting_maps) =
            hash_table(c, |c| Ok((c.u32()?, read_ulong_list(c)?)))?;
        Ok(Self {
            id,
            input_maps,
            string_table,
            conflicting_maps,
            conflict_bucket_index,
        })
    }
}

/// One physical binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlBinding {
    /// `device_index | sub_control << 8 | dik << 16`.
    pub key: u32,
    pub metamode: u32,
    pub activation: u32,
    pub action: u32,
}

impl ControlBinding {
    #[must_use]
    pub fn device_index(&self) -> u8 {
        u8::try_from(self.key & 0xFF).unwrap_or(0)
    }
    #[must_use]
    pub fn sub_control(&self) -> u8 {
        u8::try_from((self.key >> 8) & 0xFF).unwrap_or(0)
    }
    #[must_use]
    pub fn dik(&self) -> u16 {
        u16::try_from(self.key >> 16).unwrap_or(0)
    }
}

/// A decoded `0x14xxxxxx` master input map (`DB_TYPE_KEYMAP`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterInputMap {
    pub id: DataId,
    pub name: String,
    pub map_guid: [u8; 16],
    /// `(device_type, guid)`.
    pub devices: Vec<(u8, [u8; 16])>,
    /// `(control, meta_mode)`.
    pub meta_keys: Vec<(u32, u32)>,
    pub input_maps: Vec<(u32, Vec<ControlBinding>)>,
}

fn read_guid(c: &mut Cursor<'_>) -> Result<[u8; 16], DatError> {
    let mut g = [0u8; 16];
    g.copy_from_slice(c.bytes(16)?);
    Ok(g)
}

impl Decode for MasterInputMap {
    const TYPE: DbType = DbType::Keymap;
    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let name = c.archive_string()?;
        let map_guid = read_guid(c)?;
        let n = c.u32()? as usize;
        let devices = read_n(c, n, |c| Ok((c.u8()?, read_guid(c)?)))?;
        let n = c.u32()? as usize;
        let meta_keys = read_n(c, n, |c| Ok((c.u32()?, c.u32()?)))?;
        let n = c.u32()?;
        let mut input_maps = Vec::new();
        for _ in 0..n {
            let k = c.u32()?;
            let m = c.u32()? as usize;
            let binds = read_n(c, m, |c| {
                Ok(ControlBinding {
                    key: c.u32()?,
                    metamode: c.u32()?,
                    activation: c.u32()?,
                    action: c.u32()?,
                })
            })?;
            input_maps.push((k, binds));
        }
        Ok(Self {
            id,
            name,
            map_guid,
            devices,
            meta_keys,
            input_maps,
        })
    }
}
