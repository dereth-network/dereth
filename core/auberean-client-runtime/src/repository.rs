//! An object's repository: its qualities as the scripts reach them.
//!
//! Every object the scripts run on keeps its qualities in three places, asked in this order:
//!
//! 1. **its description**, for the qualities the description holds (see [`crate::description`]):
//!    answered only while the description marks them set;
//! 2. **its own tables**, one per kind of quality (whole numbers, 64-bit numbers, yes-or-no
//!    values, floats, times, texts, data ids, object ids, positions, string references and
//!    packages), which the server's updates and the scripts fill;
//! 3. **its qualities record**, the one its own qualities derive from (named by its qualities, or
//!    by its entity's number): the record's description first, then the record's tables.
//!
//! What the scripts ask, by kind:
//!
//! - **asking** for a quality answers whether one was found and, when it was, its value; **get**
//!   answers the value or the default the script gives; **does it exist** answers whether one was
//!   found;
//! - **setting** a quality the description holds sets it there (marking it set), any other in the
//!   object's own table; the record is never written;
//! - **removing** one takes it out of the description when the description has it, else out of
//!   the object's own table; it answers whether one was taken;
//! - **adjusting** a whole number, a 64-bit number or a float adds to the object's own value; with
//!   none of its own, the record's value plus the change becomes its own, and without a record
//!   nothing changes. The truncating forms keep the result between a least and a greatest value,
//!   and a sum that overflowed past either end takes that end. The description is never adjusted.
//!   They answer the new value, or the default the script gives when nothing changed.
//!
//! Texts are kept narrow (a byte a character) and handed to the scripts wide; a wide text a script
//! sets is narrowed a character a byte, a character past the narrow range kept as a question mark.
//!
//! The object's **scale** is its float quality `0x12A` (1 when it has none); setting it also scales
//! its body, between a tenth and ten times. **Edge sliding** is its yes-or-no quality 7 (off when it
//! has none); setting it also tells its body. The **physics types it passes through** are kept in
//! two whole-number qualities the script names, the low and the high half of the mask.

use std::sync::Arc;

use auberean_assets::script::Qualities as Record;
use auberean_client_model::{Object, Qualities};
use auberean_protocol::text::{NarrowString, StringInfo, VariableValue as P, WideString};
use auberean_protocol::types::{Placement, WeenieDesc};
use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{
    native_type, NativeObject, TextInfo, Variable, VariableValue as S,
};
use auberean_script::value::{split, tag, NativeId, NULL};

use crate::description;
use crate::host::{host, receiver_object, ClientHost};

/// The repository's native package.
const REPOSITORY: u32 = 0x81;

/// Qualities from here on are never the description's.
const FIRST_NOT_HELD: u32 = 0x1_0000;

/// The scale quality, and the scale an object without one has.
const SCALE: u32 = 0x12A;
/// The edge-sliding quality.
const EDGE_SLIDES: u32 = 7;

/// A table's value for a key.
#[must_use]
pub(crate) fn find<V: Clone>(table: &[(u32, V)], key: u32) -> Option<V> {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
}

// ---------------------------------------------------------------------------------------------
// The kinds of quality
// ---------------------------------------------------------------------------------------------

/// One kind of quality: what its description, its own table and its record answer, and how the
/// description and the table are written.
#[derive(Debug)]
pub struct Kind<V> {
    /// The kind, in the high word beside a quality's number, as [`description::holds`] takes it.
    pub stat: u32,
    /// The description's value, when it holds the quality and has it set.
    pub desc: fn(&WeenieDesc, u32) -> Option<V>,
    /// Set it in the description: whether the description holds the quality (and took it).
    pub set_desc: fn(&mut WeenieDesc, u32, V) -> bool,
    /// Take it out of the description: its value, when the description had it.
    pub remove_desc: fn(&mut WeenieDesc, u32) -> Option<V>,
    /// The object's own value.
    pub own: fn(&Qualities, u32) -> Option<V>,
    /// Set the object's own value (its table is always there).
    pub set_own: fn(&mut Qualities, u32, V),
    /// Take the object's own value out.
    pub remove_own: fn(&mut Qualities, u32) -> Option<V>,
    /// The record's value: its description's, then its table's.
    pub record: fn(&Record, u32) -> Option<V>,
}

fn never_set<V>(_: &mut WeenieDesc, _: u32, _: V) -> bool {
    false
}

fn never_removed<V>(_: &mut WeenieDesc, _: u32) -> Option<V> {
    None
}

/// The record's description as the description an object carries.
fn record_desc(r: &Record) -> Option<WeenieDesc> {
    r.description.as_ref().map(description::of_record)
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
mod kinds {
    use super::{
        description, find, never_removed, never_set, record_desc, string_info_of, text_info, Kind,
        Placement, Record, TextInfo,
    };

    // D2-07398
    pub const INT: Kind<i32> = Kind {
        stat: description::kind::INT,
        desc: description::int,
        set_desc: description::set_int,
        remove_desc: |w, k| description::int(w, k).filter(|_| description::remove_int(w, k)),
        own: |q, k| q.ints.get(&k).copied(),
        set_own: |q, k, v| {
            q.ints.insert(k, v);
        },
        remove_own: |q, k| q.ints.remove(&k),
        record: |r, k| {
            record_desc(r)
                .and_then(|w| description::int(&w, k))
                .or_else(|| find(&r.ints, k))
        },
    };

    // D2-07399
    pub const LONG_INT: Kind<u64> = Kind {
        stat: description::kind::LONG_INT,
        desc: description::holds_none,
        set_desc: never_set,
        remove_desc: never_removed,
        own: |q, k| q.long_ints.get(&k).copied(),
        set_own: |q, k, v| {
            q.long_ints.insert(k, v);
        },
        remove_own: |q, k| q.long_ints.remove(&k),
        record: |r, k| find(&r.long_ints, k).map(|v| v as u64),
    };

    // D2-07400
    pub const BOOL: Kind<i32> = Kind {
        stat: description::kind::BOOL,
        desc: description::bool,
        set_desc: description::set_bool,
        remove_desc: description::remove_bool,
        own: |q, k| q.bools.get(&k).copied(),
        set_own: |q, k, v| {
            q.bools.insert(k, v);
        },
        remove_own: |q, k| q.bools.remove(&k),
        record: |r, k| {
            record_desc(r)
                .and_then(|w| description::bool(&w, k))
                .or_else(|| find(&r.bools, k))
        },
    };

    // D2-07401
    pub const FLOAT: Kind<f32> = Kind {
        stat: description::kind::FLOAT,
        desc: description::float,
        set_desc: description::set_float,
        remove_desc: description::remove_float,
        own: |q, k| q.floats.get(&k).copied(),
        set_own: |q, k, v| {
            q.floats.insert(k, v);
        },
        remove_own: |q, k| q.floats.remove(&k),
        record: |r, k| {
            record_desc(r)
                .and_then(|w| description::float(&w, k))
                .or_else(|| find(&r.floats, k))
        },
    };

    // D2-07402
    pub const TIMESTAMP: Kind<f64> = Kind {
        stat: description::kind::TIMESTAMP,
        desc: description::holds_none,
        set_desc: never_set,
        remove_desc: never_removed,
        own: |q, k| q.timestamps.get(&k).copied(),
        set_own: |q, k, v| {
            q.timestamps.insert(k, v);
        },
        remove_own: |q, k| q.timestamps.remove(&k),
        record: |r, k| find(&r.times, k),
    };

    // D2-07403
    pub const STRING: Kind<Vec<u8>> = Kind {
        stat: description::kind::STRING,
        desc: description::holds_none,
        set_desc: never_set,
        remove_desc: never_removed,
        own: |q, k| q.strings.get(&k).map(|s| s.0.clone()),
        set_own: |q, k, v| {
            q.strings.insert(k, super::NarrowString(v));
        },
        remove_own: |q, k| q.strings.remove(&k).map(|s| s.0),
        // A record's texts were read a byte a character.
        record: |r, k| find(&r.strings, k).map(|s| s.chars().map(|c| c as u8).collect()),
    };

    // D2-07404
    pub const DATA_ID: Kind<u32> = Kind {
        stat: description::kind::DATA_ID,
        desc: description::data_id,
        set_desc: description::set_data_id,
        remove_desc: description::remove_data_id,
        own: |q, k| q.data_ids.get(&k).copied(),
        set_own: |q, k, v| {
            q.data_ids.insert(k, v);
        },
        remove_own: |q, k| q.data_ids.remove(&k),
        record: |r, k| {
            record_desc(r)
                .and_then(|w| description::data_id(&w, k))
                .or_else(|| find(&r.data_ids, k).map(|d| d.0))
        },
    };

    // D2-07405
    pub const INSTANCE_ID: Kind<u64> = Kind {
        stat: description::kind::INSTANCE_ID,
        desc: description::instance_id,
        set_desc: description::set_instance_id,
        remove_desc: description::remove_instance_id,
        own: |q, k| q.instance_ids.get(&k).copied(),
        set_own: |q, k, v| {
            q.instance_ids.insert(k, v);
        },
        remove_own: |q, k| q.instance_ids.remove(&k),
        record: |r, k| {
            record_desc(r)
                .and_then(|w| description::instance_id(&w, k))
                .or_else(|| find(&r.instance_ids, k))
        },
    };

    // D2-07406
    pub const POSITION: Kind<Placement> = Kind {
        stat: description::kind::POSITION,
        desc: description::holds_none,
        set_desc: never_set,
        remove_desc: never_removed,
        own: |q, k| q.positions.get(&k).copied(),
        set_own: |q, k, v| {
            q.positions.insert(k, v);
        },
        remove_own: |q, k| q.positions.remove(&k),
        record: |r, k| {
            find(&r.positions, k).map(|p| Placement {
                cell: p.cell.0,
                frame: p.frame,
            })
        },
    };

    // D2-07407
    pub const STRING_INFO: Kind<TextInfo> = Kind {
        stat: description::kind::STRING_INFO,
        desc: |w, k| description::string_info(w, k).map(text_info),
        set_desc: |w, k, v| description::set_string_info(w, k, string_info_of(&v)),
        remove_desc: |w, k| description::remove_string_info(w, k).map(|s| text_info(&s)),
        own: |q, k| q.string_infos.get(&k).map(text_info),
        set_own: |q, k, v| {
            q.string_infos.insert(k, string_info_of(&v));
        },
        remove_own: |q, k| q.string_infos.remove(&k).map(|s| text_info(&s)),
        // The record's names are its description's; its table holds the rest.
        record: |r: &Record, k| {
            let d = r.description.as_ref();
            let named = match k {
                0x100 => d.and_then(|d| d.name.as_ref()),
                0x101 => d.and_then(|d| d.plural_name.as_ref()),
                _ => None,
            };
            let s = named.or_else(|| {
                r.string_infos
                    .iter()
                    .find(|(key, _)| *key == k)
                    .map(|(_, v)| v)
            })?;
            auberean_glue::entity::text_info(s).ok()
        },
    };

    // D2-07408
    pub const PACKAGE_ID: Kind<u32> = Kind {
        stat: description::kind::PACKAGE_ID,
        desc: description::holds_none,
        set_desc: never_set,
        remove_desc: never_removed,
        own: |q, k| q.package_ids.get(&k).copied(),
        set_own: |q, k, v| {
            q.package_ids.insert(k, v);
        },
        remove_own: |q, k| q.package_ids.remove(&k),
        record: |r, k| find(&r.packages, k),
    };
}

pub use kinds::{
    BOOL, DATA_ID, FLOAT, INSTANCE_ID, INT, LONG_INT, PACKAGE_ID, POSITION, STRING, STRING_INFO,
    TIMESTAMP,
};

// ---------------------------------------------------------------------------------------------
// The qualities, asked and changed
// ---------------------------------------------------------------------------------------------

/// A quality as the object answers it: its description's (for one it holds and has set), else its
/// own, else its record's.
// D2-07364, D2-07366, D2-07368, D2-07370, D2-07372, D2-07374, D2-07376, D2-07377, D2-07379
// D2-07381, D2-07383
#[must_use]
pub fn inq<V>(k: &Kind<V>, o: Option<&Object>, record: Option<&Record>, key: u32) -> Option<V> {
    o.and_then(|o| (k.desc)(&o.weenie, key).or_else(|| (k.own)(&o.qualities, key)))
        .or_else(|| record.and_then(|r| (k.record)(r, key)))
}

/// Set a quality on the object: in its description when the description holds the quality, else
/// in its own table. Whether it was set.
// D2-07409, D2-07412, D2-07415, D2-07416, D2-07419, D2-07420, D2-07421, D2-07422, D2-07423
// D2-07424, D2-07425, D2-07358
pub fn set<V: Clone>(k: &Kind<V>, o: &mut Object, key: u32, v: V) -> bool {
    if key < FIRST_NOT_HELD
        && description::holds(k.stat | key)
        && (k.set_desc)(&mut o.weenie, key, v.clone())
    {
        return true;
    }
    (k.set_own)(&mut o.qualities, key, v);
    true
}

/// Take a quality off the object: out of its description when the description has it, else out
/// of its own table. The value taken, if any (the record is left alone).
// D2-07363, D2-07365, D2-07367, D2-07369, D2-07371, D2-07373, D2-07375, D2-07380, D2-07382
// D2-07387, D2-07388, D2-07389, D2-07390, D2-07391, D2-07392, D2-07393, D2-07394, D2-07395
// D2-07396, D2-07397
pub fn remove<V>(k: &Kind<V>, o: &mut Object, key: u32) -> Option<V> {
    (k.remove_desc)(&mut o.weenie, key).or_else(|| (k.remove_own)(&mut o.qualities, key))
}

/// The arithmetic of the adjustable kinds.
pub trait Adjust: Copy + PartialOrd + Default {
    /// The sum, wrapping as the client's registers do (floats add as floats).
    fn add(self, other: Self) -> Self;
    /// Whether the change is negative.
    fn negative(self) -> bool;
}

impl Adjust for i32 {
    fn add(self, other: Self) -> Self {
        self.wrapping_add(other)
    }
    fn negative(self) -> bool {
        self < 0
    }
}

impl Adjust for i64 {
    fn add(self, other: Self) -> Self {
        self.wrapping_add(other)
    }
    fn negative(self) -> bool {
        self < 0
    }
}

impl Adjust for f32 {
    fn add(self, other: Self) -> Self {
        self + other
    }
    fn negative(self) -> bool {
        self < 0.0
    }
}

/// Keep an adjusted value between `least` and `most`: a sum that moved against its change
/// overflowed and takes the end it ran past; then the value is held to both ends.
// D2-07355, D2-07356, D2-07357
pub fn truncate<V: Adjust>(change: V, old: V, least: V, most: V, v: &mut V) {
    if change.negative() {
        if old < *v {
            *v = least;
        }
    } else if *v < old {
        *v = most;
    }
    if most < *v {
        *v = most;
    }
    if *v < least {
        *v = least;
    }
}

/// Add `change` to an object's value of a quality, held between `bounds` when given: its own value
/// (`own`) changes; with none of its own, the record's value plus the change (held only when the
/// record had one) becomes its own; with neither, the change itself, but only when there is a
/// record. The new value, to keep as the object's own, and the old one (0 when there was none);
/// `None` when nothing changed.
// D2-07410, D2-07411, D2-07413, D2-07414, D2-07417, D2-07418
pub fn adjust<V: Adjust>(
    own: Option<V>,
    record: Option<V>,
    has_record: bool,
    change: V,
    bounds: Option<(V, V)>,
) -> Option<(V, V)> {
    if let Some(old) = own {
        let mut v = old.add(change);
        if let Some((least, most)) = bounds {
            truncate(change, old, least, most, &mut v);
        }
        return Some((v, old));
    }
    if !has_record {
        return None;
    }
    let (mut v, old) = match record {
        Some(old) => (old.add(change), old),
        None => (change, V::default()),
    };
    if let (Some((least, most)), Some(_)) = (bounds, record) {
        truncate(change, old, least, most, &mut v);
    }
    Some((v, old))
}

// ---------------------------------------------------------------------------------------------
// Conversions between the scripts' values and the model's
// ---------------------------------------------------------------------------------------------

/// A string reference as the scripts hold one.
#[must_use]
pub fn text_info(v: &StringInfo) -> TextInfo {
    #[allow(clippy::cast_sign_loss)]
    TextInfo {
        string_id: v.string_id,
        table: v.table_id,
        count: v.variable_count as u16,
        has_literal: v.has_literal,
        literal: v.literal.as_ref().map(|w| w.0.clone()),
        variables: v
            .variables
            .iter()
            .map(|x| Variable {
                id: x.id,
                kind: x.kind,
                value: match &x.value {
                    P::Empty => S::Word(0),
                    P::Word(a) => S::Word(*a),
                    P::Words(a, b, c) => S::Words(*a, *b, *c),
                    P::Object(a, b) => S::Double(*a, *b),
                    P::ObjectWith(a, b, c, d) => S::DoubleWords(*a, *b, *c, *d),
                    P::Nested(n) => S::Nested(Box::new(text_info(n))),
                    P::Long(a) => S::Long(*a),
                    P::LongWith(a, b, c) => S::LongWords(*a, *b, *c),
                },
            })
            .collect(),
    }
}

/// A string reference the scripts hold, as the model keeps one: a variable of a kind that stores
/// no value keeps none.
#[must_use]
pub fn string_info_of(v: &TextInfo) -> StringInfo {
    use auberean_protocol::text::Variable as PVariable;
    #[allow(clippy::cast_possible_wrap)]
    StringInfo {
        string_id: v.string_id,
        table_id: v.table,
        variable_count: v.count as i16,
        has_literal: v.has_literal,
        literal: v.literal.clone().map(WideString),
        variables: v
            .variables
            .iter()
            .map(|x| PVariable {
                id: x.id,
                kind: x.kind,
                value: match (&x.value, x.kind) {
                    (S::Word(a), 2 | 6) => P::Word(*a),
                    (S::Word(_), _) => P::Empty,
                    (S::Words(a, b, c), _) => P::Words(*a, *b, *c),
                    (S::Double(a, b), _) => P::Object(*a, *b),
                    (S::DoubleWords(a, b, c, d), _) => P::ObjectWith(*a, *b, *c, *d),
                    (S::Nested(n), _) => P::Nested(Box::new(string_info_of(n))),
                    (S::Long(a), _) => P::Long(*a),
                    (S::LongWords(a, b, c), _) => P::LongWith(*a, *b, *c),
                },
            })
            .collect(),
    }
}

/// A narrow text as the scripts see it: a character a byte.
pub(crate) fn widen(s: &[u8]) -> Vec<u16> {
    s.iter().map(|b| u16::from(*b)).collect()
}

/// A wide text kept narrow: a character a byte, one past the narrow range as a question mark.
pub(crate) fn narrow(s: &[u16]) -> Vec<u8> {
    s.iter().map(|c| u8::try_from(*c).unwrap_or(b'?')).collect()
}

/// A position as the scripts hold one: its cell, and its origin and rotation (scalar first) as
/// bits.
pub(crate) fn position_words(p: &Placement) -> (u32, [u32; 7]) {
    let f = &p.frame;
    (
        p.cell,
        [
            f.origin.x,
            f.origin.y,
            f.origin.z,
            f.rotation.w,
            f.rotation.x,
            f.rotation.y,
            f.rotation.z,
        ]
        .map(f32::to_bits),
    )
}

/// A position the scripts hold, as the model keeps one.
fn placement_of(cell: u32, frame: &[u32; 7]) -> Placement {
    let f = frame.map(f32::from_bits);
    Placement {
        cell,
        frame: auberean_primitives::Frame {
            origin: auberean_primitives::Vec3::new(f[0], f[1], f[2]),
            rotation: auberean_primitives::Quat {
                w: f[3],
                x: f[4],
                y: f[5],
                z: f[6],
            },
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The natives
// ---------------------------------------------------------------------------------------------

/// The repository's natives, by id.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(REPOSITORY, *n), *f))
}

const NATIVES: &[(u32, NativeFn)] = &[
    (0x01, inq_int),
    (0x03, get_int),
    (0x04, get_int),
    (0x05, does_int_exist),
    (0x06, set_int),
    (0x08, remove_int),
    (0x09, adj_int),
    (0x0A, adj_and_truncate_int),
    (0x0B, inq_ethereal_type),
    (0x0C, set_ethereal_type),
    (0x0D, set_edge_slides),
    (0x0E, edge_slides),
    (0x0F, inq_bool),
    (0x11, get_bool),
    (0x12, get_bool),
    (0x13, does_bool_exist),
    (0x14, set_bool),
    (0x16, remove_bool),
    (0x17, inq_float),
    (0x19, get_float),
    (0x1A, get_float),
    (0x1B, does_float_exist),
    (0x1C, set_float),
    (0x1E, remove_float),
    (0x1F, adj_float),
    (0x20, adj_and_truncate_float),
    (0x21, inq_string),
    (0x23, get_string),
    (0x24, get_string),
    (0x25, does_string_exist),
    (0x26, set_string),
    (0x28, remove_string),
    (0x29, inq_data_id),
    (0x2B, get_data_id),
    (0x2C, get_data_id),
    (0x2D, does_data_id_exist),
    (0x2E, set_data_id),
    (0x54, load_data_id),
    (0x30, remove_data_id),
    (0x31, inq_instance_id),
    (0x33, get_instance_id),
    (0x34, get_instance_id),
    (0x35, does_instance_id_exist),
    (0x36, set_instance_id),
    (0x38, remove_instance_id),
    (0x39, inq_position),
    (0x3B, get_position),
    (0x3C, get_position),
    (0x3D, does_position_exist),
    (0x3E, set_position),
    (0x40, remove_position),
    (0x41, get_timestamp),
    (0x42, does_timestamp_exist),
    (0x43, set_timestamp),
    (0x45, remove_timestamp),
    (0x46, inq_string_info),
    (0x47, get_string_info),
    (0x48, get_string_info),
    (0x49, does_string_info_exist),
    (0x4A, set_string_info),
    (0x4B, remove_string_info),
    (0x4C, inq_package_id),
    (0x4D, get_package_id),
    (0x4E, get_package_id),
    (0x4F, does_package_id_exist),
    (0x50, set_package_id),
    (0x51, remove_package_id),
    (0x52, set_scale),
    (0x53, get_scale),
    (0x56, get_id),
    (0x61, inq_long_int),
    (0x63, get_long_int),
    (0x64, get_long_int),
    (0x65, does_long_int_exist),
    (0x66, set_long_int),
    (0x68, remove_long_int),
    (0x69, adj_long_int),
    (0x6A, adj_and_truncate_long_int),
    (0x6B, inq_timestamp),
];

/// A quality of the receiver's object, as its repository answers it.
fn look<V>(c: &mut Call<'_>, k: &Kind<V>, key: u32) -> Result<Option<V>, Stop> {
    let id = receiver_object(c)?;
    let h = host(c)?;
    Ok(answer(h, k, id, key))
}

/// A quality of an object, as its repository answers it.
pub fn answer<V>(h: &mut ClientHost, k: &Kind<V>, id: u64, key: u32) -> Option<V> {
    let record = h.record(h.base_of(id));
    inq(k, h.model.object(id), record.as_deref(), key)
}

/// The quality an object's name is held in, and its plural's.
pub const NAME: u32 = 0x100;
pub const PLURAL_NAME: u32 = 0x101;

/// A literal string reference.
fn literal(text: &str) -> TextInfo {
    TextInfo {
        has_literal: 1,
        literal: Some(text.encode_utf16().collect()),
        ..TextInfo::default()
    }
}

/// An object's name as its repository answers it; the literal `I have no name!` when it has
/// none.
// D2-00684, D2-00416
pub fn name(h: &mut ClientHost, id: u64) -> TextInfo {
    answer(h, &STRING_INFO, id, NAME).unwrap_or_else(|| literal("I have no name!"))
}

/// An object's plural name as its repository answers it; the literal `I have no plural name!`
/// when it has none.
// D2-00685
pub fn plural_name(h: &mut ClientHost, id: u64) -> TextInfo {
    answer(h, &STRING_INFO, id, PLURAL_NAME).unwrap_or_else(|| literal("I have no plural name!"))
}

/// The physics record an object is made with: its data id 1, 0 when it has none.
// D2-08292
pub fn phys_obj_id(h: &mut ClientHost, id: u64) -> u32 {
    answer(h, &DATA_ID, id, 1).unwrap_or(0)
}

/// Whether an object moves: its mobile flag (yes-or-no 8), yes when it has none.
// D2-07254
pub fn is_mobile(h: &mut ClientHost, id: u64) -> bool {
    answer(h, &BOOL, id, 8).unwrap_or(1) != 0
}

/// An object's kind: its number 14 (hexadecimal), 0 when it has none.
fn kind_bits(h: &mut ClientHost, id: u64) -> u32 {
    answer(h, &INT, id, 0x14).unwrap_or(0).cast_unsigned()
}

/// Whether an object is a player: every bit of 1F (hexadecimal) set in its kind.
// D2-08058
pub fn is_player(h: &mut ClientHost, id: u64) -> bool {
    kind_bits(h, id) & 0x1F == 0x1F
}

/// Whether an object is a creature: every bit of 4F (hexadecimal) set in its kind.
// D2-08059
pub fn is_creature(h: &mut ClientHost, id: u64) -> bool {
    kind_bits(h, id) & 0x4F == 0x4F
}

/// Whether an object is of a kind a detection reports: the report's 64-bit mask against the
/// object's two detection words (numbers 10 and 11, low word first, 0 when it has none).
// D2-08076
pub fn detection_type_match(h: &mut ClientHost, id: u64, mask: u64) -> bool {
    let low = answer(h, &INT, id, 10).unwrap_or(0).cast_unsigned();
    let high = answer(h, &INT, id, 11).unwrap_or(0).cast_unsigned();
    mask & (u64::from(high) << 32 | u64::from(low)) != 0
}

/// Change the receiver's object: `f` given the object, its result answered; 0 without one.
fn change<T>(c: &mut Call<'_>, f: impl FnOnce(&mut Object) -> T) -> Result<Option<T>, Stop> {
    let id = receiver_object(c)?;
    Ok(host(c)?.model.object_mut(id).map(f))
}

/// A word quality and its default: `fp[-2]` the quality, `fp[-3]` the default, the result below.
fn get_word<V>(c: &mut Call<'_>, k: &Kind<V>, word: fn(V) -> u32) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let default = c.arg(1)?;
    let got = look(c, k, key)?.map_or(default, word);
    c.write(-4, got, tag::MIXED)?;
    Ok(Status::Success)
}

/// A 64-bit quality and its default: `fp[-2]` the quality, `fp[-4..-3]` the default, the result
/// below.
fn get_long<V>(c: &mut Call<'_>, k: &Kind<V>, long: fn(V) -> u64) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let default = c.long_at(-4)?;
    let got = look(c, k, key)?.map_or(default, long);
    write_long(c, -6, got)
}

/// A 64-bit result at `fp[k]` (its low word marked a 64-bit value).
pub(crate) fn write_long(c: &mut Call<'_>, k: i64, v: u64) -> Result<Status, Stop> {
    let (lo, hi) = split(v);
    c.write(k, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(k + 1)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// Whether a quality is found: `fp[-2]` the quality, the result below.
fn exists<V>(c: &mut Call<'_>, k: &Kind<V>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let got = look(c, k, key)?.is_some();
    c.write(-3, u32::from(got), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether a quality is found, and its word through an out slot: `fp[-2]` the quality, `fp[-3]` a
/// reference to the slot, the result below. The slot is marked a native's whether or not the
/// quality is found.
fn inq_word<V>(c: &mut Call<'_>, k: &Kind<V>, word: fn(V) -> u32) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let out = c.arg(1)?;
    let got = look(c, k, key)?.map(word);
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, got, tag::MIXED, &prog)?;
    c.write(-4, u32::from(got.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// As [`inq_word`], for a 64-bit value: the slot marked a 64-bit native value, both its words
/// written when the quality is found.
fn inq_long<V>(c: &mut Call<'_>, k: &Kind<V>, long: fn(V) -> u64) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let out = c.arg(1)?;
    let got = look(c, k, key)?.map(long);
    let prog = Arc::clone(c.vm.program());
    let (lo, hi) = got.map_or((None, None), |v| {
        let (lo, hi) = split(v);
        (Some(lo), Some(hi))
    });
    c.vm.write_slot(c.host, c.cx, out, 0, lo, tag::LONG | tag::MIXED, &prog)?;
    if let Some(hi) = hi {
        c.vm.write_data(c.host, c.cx, out, 1, hi, &prog)?;
    }
    c.write(-4, u32::from(got.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// Set a word quality: `fp[-2]` the quality, `fp[-3]` the value, whether it was set below.
fn set_word<V: Clone>(c: &mut Call<'_>, k: &Kind<V>, value: fn(u32) -> V) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let v = value(c.arg(1)?);
    let done = change(c, |o| set(k, o, key, v))?.unwrap_or(false);
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Set a 64-bit quality: `fp[-2]` the quality, `fp[-4..-3]` the value, whether it was set below.
fn set_long<V: Clone>(c: &mut Call<'_>, k: &Kind<V>, value: fn(u64) -> V) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let v = value(c.long_at(-4)?);
    let done = change(c, |o| set(k, o, key, v))?.unwrap_or(false);
    c.write(-5, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Set a quality to a copy of a native object's value: `fp[-2]` the quality, `fp[-3]` the
/// object, whether it was set below.
fn set_native<V: Clone>(
    c: &mut Call<'_>,
    k: &Kind<V>,
    value: fn(&NativeObject) -> Option<V>,
) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let r = c.arg(1)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let v = value(c.vm.heap.native(r)?).ok_or_else(|| c.fail("not the quality's kind"))?;
    let done = change(c, |o| set(k, o, key, v))?.unwrap_or(false);
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Take a quality off: `fp[-2]` the quality, whether one was taken below.
fn take<V>(c: &mut Call<'_>, k: &Kind<V>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let done = change(c, |o| remove(k, o, key).is_some())?.unwrap_or(false);
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Copy a found quality into the native object a reference's slot names: `fp[-2]` the quality,
/// `fp[-3]` a reference to a slot holding the object, whether it was found below.
fn inq_into<V>(
    c: &mut Call<'_>,
    k: &Kind<V>,
    put: fn(&mut NativeObject, V) -> bool,
) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let out = c.arg(1)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Err(Stop::NullReference);
    }
    let got = look(c, k, key)?;
    let found = got.is_some();
    if let Some(v) = got {
        if !put(c.vm.heap.native_mut(target)?, v) {
            return Err(c.fail("the out reference holds another kind"));
        }
    }
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

#[allow(clippy::cast_sign_loss)]
fn int_word(v: i32) -> u32 {
    v as u32
}

#[allow(clippy::cast_possible_wrap)]
fn word_int(v: u32) -> i32 {
    v as i32
}

fn same<V>(v: V) -> V {
    v
}

// Whole numbers.

// D2-04848, D2-08223
fn inq_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_word(c, &INT, int_word)
}

// D2-04798, D2-08234
fn get_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_word(c, &INT, int_word)
}

// D2-04786, D2-08243
fn does_int_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &INT)
}

// D2-04821, D2-08253
fn set_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, &INT, word_int)
}

// D2-04808, D2-08263
fn remove_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &INT)
}

/// Adjust a quality of the receiver's object through its own value and its record's; the new
/// value is kept as its own.
fn adjust_on<V: Adjust>(
    c: &mut Call<'_>,
    k: &Kind<V>,
    key: u32,
    change_by: V,
    bounds: Option<(V, V)>,
) -> Result<Option<V>, Stop> {
    let id = receiver_object(c)?;
    let h = host(c)?;
    let record = h.record(h.base_of(id));
    let from_record = record.as_deref().and_then(|r| (k.record)(r, key));
    Ok(h.model.object_mut(id).and_then(|o| {
        let own = (k.own)(&o.qualities, key);
        let (v, _) = adjust(own, from_record, record.is_some(), change_by, bounds)?;
        (k.set_own)(&mut o.qualities, key, v);
        Some(v)
    }))
}

/// Add to a whole number: `fp[-2]` the quality, `fp[-3]` the change, `fp[-4]` the default; the new
/// value (or the default) below.
// D2-04780, D2-08274
fn adj_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let by = word_int(c.arg(1)?);
    let default = c.arg(2)?;
    let got = adjust_on(c, &INT, key, by, None)?.map_or(default, int_word);
    c.write(-5, got, tag::MIXED)?;
    Ok(Status::Success)
}

/// As [`adj_int`], held between `fp[-5]` and `fp[-6]`; the result below them.
// D2-04777, D2-08275
fn adj_and_truncate_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let by = word_int(c.arg(1)?);
    let default = c.arg(2)?;
    let least = word_int(c.arg(3)?);
    let most = word_int(c.arg(4)?);
    let got = adjust_on(c, &INT, key, by, Some((least, most)))?.map_or(default, int_word);
    c.write(-7, got, tag::MIXED)?;
    Ok(Status::Success)
}

// 64-bit numbers.

// D2-04849, D2-08224
fn inq_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_long(c, &LONG_INT, same)
}

// D2-04799, D2-08235
fn get_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_long(c, &LONG_INT, same)
}

// D2-04787, D2-08244
fn does_long_int_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &LONG_INT)
}

// D2-04822, D2-08254
fn set_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_long(c, &LONG_INT, same)
}

// D2-04809, D2-08264
fn remove_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &LONG_INT)
}

/// The 64-bit numbers as the adjusting arithmetic sees them: signed.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
const SIGNED_LONG_INT: Kind<i64> = Kind {
    stat: description::kind::LONG_INT,
    desc: description::holds_none,
    set_desc: never_set,
    remove_desc: never_removed,
    own: |q, k| q.long_ints.get(&k).map(|v| *v as i64),
    set_own: |q, k, v| {
        q.long_ints.insert(k, v as u64);
    },
    remove_own: |q, k| q.long_ints.remove(&k).map(|v| v as i64),
    record: |r, k| find(&r.long_ints, k),
};

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn adjust_long(
    c: &mut Call<'_>,
    key: u32,
    change_by: u64,
    bounds: Option<(u64, u64)>,
) -> Result<Option<u64>, Stop> {
    let bounds = bounds.map(|(a, b)| (a as i64, b as i64));
    Ok(adjust_on(c, &SIGNED_LONG_INT, key, change_by as i64, bounds)?.map(|v| v as u64))
}

/// Add to a 64-bit number: `fp[-2]` the quality, `fp[-4..-3]` the change, `fp[-6..-5]` the
/// default; the new value (or the default) at `fp[-8..-7]`.
// D2-04781, D2-08276
fn adj_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let by = c.long_at(-4)?;
    let default = c.long_at(-6)?;
    let got = adjust_long(c, key, by, None)?.unwrap_or(default);
    write_long(c, -8, got)
}

/// As [`adj_long_int`], held between `fp[-8..-7]` and `fp[-10..-9]`; the result at
/// `fp[-12..-11]`.
// D2-04778, D2-08277
fn adj_and_truncate_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let by = c.long_at(-4)?;
    let default = c.long_at(-6)?;
    let least = c.long_at(-8)?;
    let most = c.long_at(-10)?;
    let got = adjust_long(c, key, by, Some((least, most)))?.unwrap_or(default);
    write_long(c, -12, got)
}

// Yes-or-no values.

// D2-04843, D2-08225
fn inq_bool(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_word(c, &BOOL, int_word)
}

// D2-04794, D2-08236
fn get_bool(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_word(c, &BOOL, int_word)
}

// D2-04782, D2-08245
fn does_bool_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &BOOL)
}

// D2-04815, D2-08255
fn set_bool(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, &BOOL, word_int)
}

// D2-04804, D2-08265
fn remove_bool(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &BOOL)
}

// Floats.

// D2-04846, D2-08226
fn inq_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_word(c, &FLOAT, f32::to_bits)
}

// D2-04797, D2-08237
fn get_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_word(c, &FLOAT, f32::to_bits)
}

// D2-04784, D2-08246
fn does_float_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &FLOAT)
}

// D2-04819, D2-08256
fn set_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, &FLOAT, f32::from_bits)
}

// D2-04806, D2-08266
fn remove_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &FLOAT)
}

/// Add to a float: `fp[-2]` the quality, `fp[-3]` the change, `fp[-4]` the default; the new
/// value (or the default) below.
// D2-04779, D2-08278
fn adj_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let by = f32::from_bits(c.arg(1)?);
    let default = c.arg(2)?;
    let got = adjust_on(c, &FLOAT, key, by, None)?.map_or(default, f32::to_bits);
    c.write(-5, got, tag::MIXED)?;
    Ok(Status::Success)
}

/// As [`adj_float`], held between `fp[-5]` and `fp[-6]`; the result below them.
// D2-04776, D2-08279
fn adj_and_truncate_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let by = f32::from_bits(c.arg(1)?);
    let default = c.arg(2)?;
    let least = f32::from_bits(c.arg(3)?);
    let most = f32::from_bits(c.arg(4)?);
    let got = adjust_on(c, &FLOAT, key, by, Some((least, most)))?.map_or(default, f32::to_bits);
    c.write(-7, got, tag::MIXED)?;
    Ok(Status::Success)
}

// Times.

// D2-04854, D2-08231
fn inq_timestamp(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_long(c, &TIMESTAMP, f64::to_bits)
}

// D2-04802, D2-08240
fn get_timestamp(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_long(c, &TIMESTAMP, f64::to_bits)
}

// D2-04792, D2-08250
fn does_timestamp_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &TIMESTAMP)
}

// D2-04828, D2-08260
fn set_timestamp(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_long(c, &TIMESTAMP, f64::from_bits)
}

// D2-04814, D2-08271
fn remove_timestamp(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &TIMESTAMP)
}

// Texts.

/// The text into the string an out reference's slot names (made wide).
// D2-04853, D2-08227, D2-07378
fn inq_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_into(c, &STRING, |o, v| match o {
        NativeObject::Text(t) => {
            *t = widen(&v);
            true
        }
        _ => false,
    })
}

/// The text, or a copy of the default string (`fp[-3]`): a new string at `fp[-4]`.
// D2-04841, D2-08300
fn get_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let d = c.arg(1)?;
    if d == NULL {
        return Err(Stop::NullReference);
    }
    let text = match look(c, &STRING, key)? {
        Some(v) => widen(&v),
        None => match c.vm.heap.native(d)? {
            NativeObject::Text(t) => t.clone(),
            _ => return Err(c.fail("the default is not a string")),
        },
    };
    let r = c.vm.heap.add_native(TEXT, NativeObject::Text(text));
    c.write(-4, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The native type of a wide string.
const TEXT: u32 = 0x76;

// D2-04790, D2-08302
fn does_string_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &STRING)
}

/// Set a text from a wide string (`fp[-3]`), kept narrow.
// D2-04827, D2-08303
fn set_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_native(c, &STRING, |o| match o {
        NativeObject::Text(t) => Some(narrow(t)),
        _ => None,
    })
}

// D2-04812, D2-08267
fn remove_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &STRING)
}

// Data ids.

// D2-04844, D2-08228
fn inq_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_word(c, &DATA_ID, same)
}

// D2-04796, D2-08238
fn get_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_word(c, &DATA_ID, same)
}

// D2-04783, D2-08247
fn does_data_id_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &DATA_ID)
}

// D2-04816, D2-08257
fn set_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, &DATA_ID, same)
}

/// Set a data id (`fp[-2]` the quality) to the record a file's name (`fp[-3]`) names, typed by
/// its extension in the client's table of file names; whether it was set in `fp[-4]`. A name the
/// table does not hold sets nothing and answers 0: on an install of the relaunched game the table
/// holds none.
// D2-04803, D2-08304
fn load_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    // The receiver first, as every quality native: a null one stops the call.
    receiver_object(c)?;
    let key = c.arg(0)?;
    let r = c.arg(1)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let name = match c.vm.heap.native(r)? {
        NativeObject::Text(t) => t.clone(),
        _ => return Err(c.fail("the file's name is not a string")),
    };
    let id = auberean_glue::names::id_by_file_name(c, &name);
    let done = id != 0 && change(c, |o| set(&DATA_ID, o, key, id))?.unwrap_or(false);
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-04805, D2-08268
fn remove_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &DATA_ID)
}

// Object ids.

// D2-04847, D2-08229
fn inq_instance_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_long(c, &INSTANCE_ID, same)
}

// D2-04795, D2-08239
fn get_instance_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_long(c, &INSTANCE_ID, same)
}

// D2-04785, D2-08248
fn does_instance_id_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &INSTANCE_ID)
}

// D2-04820, D2-08258
fn set_instance_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_long(c, &INSTANCE_ID, same)
}

// D2-04807, D2-08269
fn remove_instance_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &INSTANCE_ID)
}

// Positions.

fn put_position(o: &mut NativeObject, v: Placement) -> bool {
    match o {
        NativeObject::Position { cell, frame } => {
            (*cell, *frame) = position_words(&v);
            true
        }
        _ => false,
    }
}

/// The position into the position an out reference's slot names.
// D2-04851, D2-08230
fn inq_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_into(c, &POSITION, put_position)
}

/// The position, or a copy of the default position (`fp[-3]`): a new position at `fp[-4]`.
// D2-04840, D2-08301
fn get_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let d = c.arg(1)?;
    if d == NULL {
        return Err(Stop::NullReference);
    }
    let (cell, frame) = match look(c, &POSITION, key)? {
        Some(p) => position_words(&p),
        None => match c.vm.heap.native(d)? {
            NativeObject::Position { cell, frame } => (*cell, *frame),
            _ => return Err(c.fail("the default is not a position")),
        },
    };
    let r =
        c.vm.heap
            .add_native(POSITION_TYPE, NativeObject::Position { cell, frame });
    c.write(-4, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The native type of a position.
const POSITION_TYPE: u32 = 0x5A;

// D2-04789, D2-08249
fn does_position_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &POSITION)
}

/// Set a position from a position (`fp[-3]`).
// D2-04824, D2-08259
fn set_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_native(c, &POSITION, |o| match o {
        NativeObject::Position { cell, frame } => Some(placement_of(*cell, frame)),
        _ => None,
    })
}

// D2-04811, D2-08270
fn remove_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &POSITION)
}

// String references.

/// The string reference into the string reference an out reference's slot names.
// D2-04852, D2-08233
fn inq_string_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_into(c, &STRING_INFO, |o, v| match o {
        NativeObject::TextInfo(t) => {
            *t = v;
            true
        }
        _ => false,
    })
}

/// A string-reference quality and its default (`fp[-3]`, a string reference): a new string
/// reference holding the object's value, else a copy of the default.
// D2-04842, D2-08241
fn get_string_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    let key = c.arg(0)?;
    let value = match look(c, &STRING_INFO, key)? {
        Some(v) => NativeObject::TextInfo(v),
        None => {
            let d = c.arg(1)?;
            if d == NULL {
                return Ok(Status::Crash);
            }
            c.vm.heap.native(d)?.clone()
        }
    };
    let r = c.vm.heap.add_native(native_type::STRING_INFO, value);
    c.write(-4, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-04791, D2-08251
fn does_string_info_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &STRING_INFO)
}

/// Set a string reference from a string reference (`fp[-3]`).
// D2-04826, D2-08262
fn set_string_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_native(c, &STRING_INFO, |o| match o {
        NativeObject::TextInfo(t) => Some(t.clone()),
        _ => None,
    })
}

// D2-04813, D2-08272
fn remove_string_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &STRING_INFO)
}

// Packages.

// D2-04850, D2-08232
fn inq_package_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    inq_word(c, &PACKAGE_ID, same)
}

// D2-04800, D2-08242
fn get_package_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_word(c, &PACKAGE_ID, same)
}

// D2-04788, D2-08252
fn does_package_id_exist(c: &mut Call<'_>) -> Result<Status, Stop> {
    exists(c, &PACKAGE_ID)
}

// D2-04823, D2-08261
fn set_package_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_word(c, &PACKAGE_ID, same)
}

// D2-04810, D2-08273
fn remove_package_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    take(c, &PACKAGE_ID)
}

// The body's qualities.

/// The object's scale (1 when it has none), at `fp[-2]`.
// D2-04801, D2-08293
fn get_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    let scale = look(c, &FLOAT, SCALE)?.unwrap_or(1.0);
    c.write(-2, scale.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// Set the object's scale (`fp[-2]`), and its body's; whether both took it, below.
// D2-04825, D2-08296
fn set_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    let scale = f32::from_bits(c.arg(0)?);
    let id = receiver_object(c)?;
    let set_here = change(c, |o| set(&FLOAT, o, SCALE, scale))?.unwrap_or(false);
    let done = set_here && crate::body::set_scale(host(c)?, id, scale);
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether the object slides along edges (0 when it has no say), at `fp[-2]`.
// D2-04793, D2-08294
fn edge_slides(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = look(c, &BOOL, EDGE_SLIDES)?.unwrap_or(0);
    c.write(-2, int_word(on), tag::MIXED)?;
    Ok(Status::Success)
}

/// Set whether the object slides along edges (`fp[-2]`), and tell its body; whether both took
/// it, below.
// D2-04817, D2-08297
fn set_edge_slides(c: &mut Call<'_>) -> Result<Status, Stop> {
    let on = word_int(c.arg(0)?);
    let id = receiver_object(c)?;
    let set_here = change(c, |o| set(&BOOL, o, EDGE_SLIDES, on))?.unwrap_or(false);
    let done = set_here && crate::body::set_edge_slide(host(c)?, id, on != 0);
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// The physics types the object passes through, from the two whole numbers that hold the mask's
/// halves (`fp[-3]` the low half's quality, `fp[-4]` the high half's), into the physics types an
/// out reference's slot names (`fp[-2]`): only when both are found. Whether both were found, below.
// D2-04845, D2-08295
fn inq_ethereal_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_glue::missile::PhysicsValue;
    use auberean_physics::ethereal::PhysicsTypes;
    use auberean_script::native_object::Engine;
    let out = c.arg(0)?;
    let low_key = c.arg(1)?;
    let high_key = c.arg(2)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Err(Stop::NullReference);
    }
    let low = look(c, &INT, low_key)?;
    let high = look(c, &INT, high_key)?;
    let both = low.is_some() && high.is_some();
    if let (Some(low), Some(high)) = (low, high) {
        let mask = u64::from(int_word(low)) | u64::from(int_word(high)) << 32;
        *c.vm.heap.native_mut(target)? =
            NativeObject::Engine(Engine::new(PhysicsValue::Types(PhysicsTypes::new(mask))));
    }
    c.write(-5, u32::from(both), tag::MIXED)?;
    Ok(Status::Success)
}

/// Keep the physics types an object passes through (`fp[-2]`) in two whole numbers: the mask's
/// low half in quality `fp[-3]`, its high half in `fp[-4]`. Whether both were set, below.
// D2-04818, D2-08298
fn set_ethereal_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_glue::missile::PhysicsValue;
    let r = c.arg(0)?;
    let low_key = c.arg(1)?;
    let high_key = c.arg(2)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let mask = match PhysicsValue::of(c.vm.heap.native(r)?) {
        Some(PhysicsValue::Types(t)) => t.mask,
        _ => return Err(c.fail("not a physics type")),
    };
    let (low, high) = split(mask);
    let done = change(c, |o| {
        let a = set(&INT, o, low_key, word_int(low));
        let b = set(&INT, o, high_key, word_int(high));
        a && b
    })?
    .unwrap_or(false);
    c.write(-5, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// The object's id, 64 bits, below the receiver.
// D2-08535
fn get_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    write_long(c, -3, id)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the narrow and wide conversions and the position words on hand-built values)
    use super::*;

    #[test]
    fn a_text_widens_a_byte_a_character_and_narrows_back() {
        let s = b"Ab\xE9".to_vec();
        assert_eq!(widen(&s), vec![0x41, 0x62, 0xE9]);
        assert_eq!(narrow(&widen(&s)), s);
        assert_eq!(narrow(&[0x41, 0x20AC]), b"A?".to_vec());
    }

    #[test]
    fn a_position_goes_to_words_and_back_unchanged() {
        let p = placement_of(
            0x0298_0109,
            &[1.5f32, 2.0, -3.0, 1.0, 0.0, 0.0, 0.0].map(f32::to_bits),
        );
        let (cell, frame) = position_words(&p);
        assert_eq!(cell, 0x0298_0109);
        assert_eq!(placement_of(cell, &frame), p);
    }

    #[test]
    fn a_string_reference_goes_to_the_scripts_and_back_unchanged() {
        use auberean_protocol::text::Variable as PVariable;
        let s = StringInfo {
            string_id: 7,
            table_id: 0x2300_0001,
            variable_count: 2,
            has_literal: 0,
            literal: None,
            variables: vec![
                PVariable {
                    id: 1,
                    kind: 2,
                    value: P::Word(5),
                },
                PVariable {
                    id: 2,
                    kind: 1,
                    value: P::Empty,
                },
            ],
        };
        assert_eq!(string_info_of(&text_info(&s)), s);
    }
}
