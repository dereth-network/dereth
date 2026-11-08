//! Names for numbers: the scripts' questions of an enumeration's names (an enumeration is a data
//! files' map named by its enumeration value: an enum map, which may extend a base map whose names
//! it also answers, or an id map).
//!
//! - **The name of a value** fills a string the scripts hand over (left as it was when the
//!   enumeration has no name for the value) and answers whether it was found.
//! - **The value of a name** (letter case ignored) fills a number the scripts reach through a
//!   reference (left as it was when not found) and answers whether it was found.
//! - **A value's name as a new string** (the plain name and the one for logs are the same): empty
//!   when the enumeration has no name for it.
//! - **The value of a name in a record** named directly (an enum map or an id map), as the value of
//!   a name.
//! - **How many values, every value, every name** of an enumeration: the count, and new lists of
//!   words and of new strings, in the enumeration's order.
//! - **The record of a value** in an enumeration mapping (the master map names the mapping's map,
//!   which names the record): 0 when either map lacks it.

use std::sync::Arc;

use auberean_script::library::Elem;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, Item, NativeObject};
use auberean_script::value::{tag, NULL};

use crate::machine::{new_text, text_of};

/// A list of words.
const WORD_LIST: u32 = 10;
/// A list of references.
const REFERENCE_LIST: u32 = 0x65;

/// The enum maps' natives' package.
const ENUM_MAPPER: u32 = 0x23;
/// The data cache's natives' package.
const DB_CACHE: u32 = 0x1AC0;

const NATIVES: &[(u32, u32, NativeFn)] = &[
    (ENUM_MAPPER, 0x01, inq_string),
    (ENUM_MAPPER, 0x02, get_string),
    (ENUM_MAPPER, 0x03, inq_enum),
    (ENUM_MAPPER, 0x04, get_log_string),
    (ENUM_MAPPER, 0x05, enum_by_record),
    (ENUM_MAPPER, 0x06, count),
    (ENUM_MAPPER, 0x07, all_values),
    (ENUM_MAPPER, 0x08, all_names),
    (DB_CACHE, 0x01, did_from_enum),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The name of a value (`fp[-3]`) of an enumeration (`fp[-2]`), into the string the reference at `fp[-4]`
/// reaches; whether it was found in `fp[-5]`.
// D2-08549
fn inq_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let map = c.cx.at(-2)?;
    let value = c.cx.at(-3)?;
    let out = c.cx.at(-4)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    let name = c
        .host
        .database()
        .and_then(|db| db.enumeration_name(map, value));
    let found = name.is_some();
    if let (Some(n), true) = (name, target != NULL) {
        *c.vm.heap.native_mut(target)? = NativeObject::Text(n.encode_utf16().collect());
    }
    c.write(-5, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

/// The value of a name (the string at `fp[-3]`) of an enumeration (`fp[-2]`), into the number the
/// reference at `fp[-4]` reaches (tagged as written by a native, found or not); whether it was
/// found in `fp[-5]`.
// D2-08548
fn inq_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    let map = c.cx.at(-2)?;
    let name: String = char::decode_utf16(text_of(c, c.cx.at(-3)?)?)
        .map(|r| r.unwrap_or('?'))
        .collect();
    let out = c.cx.at(-4)?;
    let value = c
        .host
        .database()
        .and_then(|db| db.enumeration_value(map, &name))
        .or_else(|| unshipped_value(map, &name));
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, value, tag::MIXED, &prog)?;
    c.write(-5, u32::from(value.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The enumeration of an admin's ethereality overrides, which the game's data files do not carry:
/// the words its command takes, numbered as this game's servers read them.
const ETHEREALITY: u32 = 0x4100_0026;
const ETHEREALITY_WORDS: [(&str, u32); 3] = [
    ("Super", 0x4000_0001),
    ("Normal", 0x4000_0002),
    ("Admin", 0x4000_0003),
];

/// The enumeration of an admin's log types, which the game's data files do not carry either, and
/// the one this game's servers answer an admin under.
const LOG_TYPE: u32 = 0x4000_002C;
const ADMIN_LOG: (u32, &str) = (1, "Admin");

/// The value of a word of an enumeration no install holds.
fn unshipped_value(map: u32, name: &str) -> Option<u32> {
    match map {
        ETHEREALITY => ETHEREALITY_WORDS
            .iter()
            .find(|(w, _)| w.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v),
        LOG_TYPE => ADMIN_LOG
            .1
            .eq_ignore_ascii_case(name)
            .then_some(ADMIN_LOG.0),
        _ => None,
    }
}

/// The name of a value of an enumeration no install holds.
fn unshipped_name(map: u32, value: u32) -> Option<String> {
    (map == LOG_TYPE && value == ADMIN_LOG.0).then(|| ADMIN_LOG.1.to_owned())
}

/// The name of a value (`fp[-3]`) of an enumeration (`fp[-2]`) as a new string at `fp[-4]`, empty
/// when it has none.
fn name_as_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let map = c.cx.at(-2)?;
    let value = c.cx.at(-3)?;
    let name = c
        .host
        .database()
        .and_then(|db| db.enumeration_name(map, value))
        .or_else(|| unshipped_name(map, value))
        .unwrap_or_default();
    let id = new_text(c, name.encode_utf16().collect());
    c.write(-4, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-08545
fn get_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    name_as_string(c)
}

// D2-08544
fn get_log_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    name_as_string(c)
}

/// The value of a name (the string at `fp[-3]`, letter case ignored) in a record (`fp[-2]`: an enum
/// map or an id map), into the number the reference at `fp[-4]` reaches (tagged as written by a
/// native, found or not); whether it was found in `fp[-5]`.
// D2-08547
fn enum_by_record(c: &mut Call<'_>) -> Result<Status, Stop> {
    let did = c.cx.at(-2)?;
    let name: String = char::decode_utf16(text_of(c, c.cx.at(-3)?)?)
        .map(|r| r.unwrap_or('?'))
        .collect();
    let out = c.cx.at(-4)?;
    let value = c.host.database().and_then(|db| db.value_by_did(did, &name));
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, value, tag::MIXED, &prog)?;
    c.write(-5, u32::from(value.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// How many values an enumeration (`fp[-2]`) has, at `fp[-3]`.
// D2-08542
fn count(c: &mut Call<'_>) -> Result<Status, Stop> {
    let map = c.cx.at(-2)?;
    let n = c.host.database().map_or(0, |db| db.enumeration_len(map));
    #[allow(clippy::cast_possible_truncation)]
    c.write(-3, n as u32, tag::MIXED)?;
    Ok(Status::Success)
}

/// Every value of an enumeration (`fp[-2]`), as a new list of words at `fp[-3]`.
// D2-08543, D2-01544
fn all_values(c: &mut Call<'_>) -> Result<Status, Stop> {
    let map = c.cx.at(-2)?;
    let values = c
        .host
        .database()
        .map(|db| db.enumeration_values(map))
        .unwrap_or_default();
    let l = NativeObject::Sequence {
        item: Elem::Word,
        items: values.into_iter().map(Item::Word).collect(),
    };
    let id = c.vm.heap.add_native(WORD_LIST, l);
    c.write(-3, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Every name of an enumeration (`fp[-2]`), each a new string, as a new list of references at
/// `fp[-3]`.
// D2-08546, D2-01053
fn all_names(c: &mut Call<'_>) -> Result<Status, Stop> {
    let map = c.cx.at(-2)?;
    let names = c
        .host
        .database()
        .map(|db| db.enumeration_names(map))
        .unwrap_or_default();
    let items = names
        .into_iter()
        .map(|n| {
            Item::Ref(c.vm.heap.add_native(
                native_type::WP_STRING,
                NativeObject::Text(n.encode_utf16().collect()),
            ))
        })
        .collect();
    let l = NativeObject::Sequence {
        item: Elem::Ref,
        items,
    };
    let id = c.vm.heap.add_native(REFERENCE_LIST, l);
    c.write(-3, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The record a value (`fp[-2]`) maps to in an enumeration mapping (`fp[-3]`), into `fp[-4]`; the
/// receiver is not read.
// D2-08474, D2-12601, D2-12602
pub(crate) fn did_from_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    let value = c.cx.at(-2)?;
    let mapping = c.cx.at(-3)?;
    let did = c.host.data_id_by_enum(value, mapping);
    c.write(-4, did, tag::MIXED)?;
    Ok(Status::Success)
}
