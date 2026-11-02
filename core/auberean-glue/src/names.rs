//! The client's table of file names, as scripts ask it: a record's id by its file's name, and a
//! file's name by its id.
//!
//! - **An id by name and type** asks the host's table ([`Host::data_id_by_name`]) with the name
//!   narrowed (a unit past the narrow range as `?`).
//! - **An id by name alone** types the name by its extension first (the part from the first `.`,
//!   after trimming white space): a name with no type known answers 0 without asking the table.
//! - **An id by an enumeration value** is the data cache's, as its own native answers it.
//! - **A file's name by its id** is the host's table's ([`Host::file_name_by_id`]).
//!
//! The client's table is never filled on an install of the relaunched game (it starts without the
//! text file of names the authoring tools kept): every id it gives is 0 and every name empty, and a
//! host that keeps no table answers the same.
//!
//! [`Host::data_id_by_name`]: auberean_script::Host::data_id_by_name
//! [`Host::file_name_by_id`]: auberean_script::Host::file_name_by_id

use auberean_dat::types::type_of_file_name;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::value::{tag, NULL};

use crate::machine::{new_text, text_of};

/// The file-name table's natives' package.
const FILE_ARRAY: u32 = 0x27;

const NATIVES: &[(u32, u32, NativeFn)] = &[
    (FILE_ARRAY, 0x01, id_from_enum),
    (FILE_ARRAY, 0x02, id_for_name),
    (FILE_ARRAY, 0x03, id_for_name_and_type),
    (FILE_ARRAY, 0x04, name_for_id),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The characters a name is trimmed of at both ends before its extension is read.
const WHITE_SPACE: &[char] = &[' ', '\n', '\r', '\t', '\x0C'];

/// A wide text as a narrow one: a unit each, one past the narrow range as `?`.
pub(crate) fn narrowed(s: &[u16]) -> String {
    s.iter()
        .map(|&u| u8::try_from(u).map_or('?', char::from))
        .collect()
}

/// The string at `fp[k]`, narrowed; the null reference stops the run.
fn name_arg(c: &Call<'_>, k: i64) -> Result<String, Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(c.fail("a file name is the null reference"));
    }
    Ok(narrowed(&text_of(c, r)?))
}

/// The record a value (`fp[-2]`) maps to in an enumeration mapping (`fp[-3]`), into `fp[-4]`: the
/// data cache's answer.
// D2-08586
fn id_from_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    crate::enums::did_from_enum(c)
}

/// The id of a named file, typed by its extension: 0 for a name whose extension no record type
/// has; else the table's id for the name and that type.
// D2-14617, D2-14618
pub fn id_by_file_name(c: &mut Call<'_>, wide: &[u16]) -> u32 {
    let name = narrowed(wide);
    match type_of_file_name(name.trim_matches(WHITE_SPACE)) {
        Some(t) => c.host.data_id_by_name(&name, t.0),
        None => 0,
    }
}

/// The id of the file the string at `fp[-2]` names, typed by its extension, into `fp[-3]`.
// D2-08585
fn id_for_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.cx.at(-2)?;
    if r == NULL {
        return Err(c.fail("a file name is the null reference"));
    }
    let wide = text_of(c, r)?;
    let id = id_by_file_name(c, &wide);
    c.write(-3, id, tag::MIXED)?;
    Ok(Status::Success)
}

/// The id of the file the string at `fp[-2]` names, of the record type at `fp[-3]`, into
/// `fp[-4]`.
// D2-08587, D2-14616
fn id_for_name_and_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name = name_arg(c, -2)?;
    let ty = c.cx.at(-3)?;
    let id = c.host.data_id_by_name(&name, ty);
    c.write(-4, id, tag::MIXED)?;
    Ok(Status::Success)
}

/// The name of the file of the id at `fp[-2]`, as a new string at `fp[-3]`: the host's table's
/// (empty for an id it does not name).
// D2-08588, D2-14612
fn name_for_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.cx.at(-2)?;
    let name = c.host.file_name_by_id(id);
    let s = new_text(c, name);
    c.write(-3, s, tag::REFERENCE)?;
    Ok(Status::Success)
}
