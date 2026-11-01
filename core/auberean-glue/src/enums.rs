//! Names for numbers: the scripts' questions of an enumeration's names (an enumeration is a data
//! files' map named by its enumeration value: an enum map, which may extend a base map whose names
//! it also answers, or an id map).
//!
//! - **The name of a value** fills a string the scripts hand over (left as it was when the
//!   enumeration has no name for the value) and answers whether it was found.
//! - **The value of a name** (letter case ignored) fills a number the scripts reach through a
//!   reference (left as it was when not found) and answers whether it was found.
//! - **The record of a value** in an enumeration mapping (the master map names the mapping's map,
//!   which names the record): 0 when either map lacks it.

use std::sync::Arc;

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::NativeObject;
use auberean_script::value::{tag, NULL};

use crate::machine::text_of;

/// The enum maps' natives' package.
const ENUM_MAPPER: u32 = 0x23;
/// The data cache's natives' package.
const DB_CACHE: u32 = 0x1AC0;

const NATIVES: &[(u32, u32, NativeFn)] = &[
    (ENUM_MAPPER, 0x01, inq_string),
    (ENUM_MAPPER, 0x03, inq_enum),
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
        .and_then(|db| db.enumeration_value(map, &name));
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, value, tag::MIXED, &prog)?;
    c.write(-5, u32::from(value.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The record a value (`fp[-2]`) maps to in an enumeration mapping (`fp[-3]`), into `fp[-4]`; the
/// receiver is not read.
// D2-08474, D2-12601, D2-12602
fn did_from_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    let value = c.cx.at(-2)?;
    let mapping = c.cx.at(-3)?;
    let did = c.host.data_id_by_enum(value, mapping);
    c.write(-4, did, tag::MIXED)?;
    Ok(Status::Success)
}
