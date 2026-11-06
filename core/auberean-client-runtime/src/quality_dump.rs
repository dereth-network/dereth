//! An object's qualities written out as text, for an admin's look at it: every value of one kind of
//! quality, or its description's contents, level by level.
//!
//! The levels are the object's own qualities (`LOCAL`, level 0) and, under them, its qualities
//! record (`BASE`, level 1). Each level says how many values it holds and lists them, each named as
//! its kind's enumeration names it; a level ends with an empty line. A value whose number the
//! enumeration does not name is listed under the last name found (nothing before the first), so a
//! gap in the data shows as a repeated name.
//!
//! The tables list in the client's hash order: bucket by bucket, a key's bucket the key modulo the
//! bucket count. A record's tables keep the order they are stored in. An object's own tables are
//! kept sorted, so they list by bucket over the 47 buckets every table in the recorded sessions
//! uses, and two keys in one bucket list by key.
//!
//! Asked for the object's own level alone, the original client never answers (it lists that level
//! again and again until it runs out of memory); this one lists the level once.

use auberean_assets::script::{ObjectDescription, Qualities as Record};
use auberean_client_model::{Object, Qualities};
use auberean_protocol::table::BUCKET_COUNTS;
use auberean_protocol::types::{Placement, WeenieDesc};
use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};
use auberean_text::crt::fixed;
use auberean_text::Source;

use crate::description::{self, int};
use crate::host::{host, receiver_object};
use crate::repository::{narrow, position_words, widen};
use crate::text::from_wire;

/// The repository's native package.
const REPOSITORY: u32 = 0x81;

/// The object table's native package.
const OBJECT_TABLE: u32 = 0x95;

/// The buckets of an object's own tables.
const OWN_BUCKETS: u32 = BUCKET_COUNTS[2];

/// The names of an enumeration's values: the enumeration, the value.
pub type Names<'a> = dyn FnMut(u32, u32) -> Option<String> + 'a;

/// What a dump reads: the object's own qualities, its record, and whether to go on to the record.
#[derive(Clone, Copy, Debug)]
pub struct Levels<'a> {
    pub object: Option<&'a Object>,
    pub record: Option<&'a Record>,
    pub recurse: bool,
}

/// One level to list.
#[derive(Clone, Copy)]
enum Level<'a> {
    Own(&'a Object),
    Record(&'a Record),
}

impl<'a> Levels<'a> {
    fn levels(self) -> Vec<Level<'a>> {
        let mut out = Vec::new();
        if let Some(o) = self.object {
            out.push(Level::Own(o));
            if self.recurse {
                out.extend(self.record.map(Level::Record));
            }
        }
        out
    }
}

/// A level's label: `LOCAL`, and from the next level on `BASE`.
fn label(level: usize) -> &'static str {
    if level == 0 {
        "LOCAL"
    } else {
        "BASE"
    }
}

/// A table of an object's own, in hash order.
fn own<V>(t: &std::collections::BTreeMap<u32, V>) -> Vec<(u32, &V)> {
    let mut out: Vec<(u32, &V)> = t.iter().map(|(k, v)| (*k, v)).collect();
    out.sort_by_key(|(k, _)| k % OWN_BUCKETS);
    out
}

/// A record's table, as stored.
fn stored<V>(t: &[(u32, V)]) -> Vec<(u32, &V)> {
    t.iter().map(|(k, v)| (*k, v)).collect()
}

/// A level's values of one kind, written.
type Rows = Vec<(u32, Vec<u8>)>;

/// Write the levels of one kind of quality: how many each holds, then `name(number): value` for
/// each. The text is narrow, a byte a character, until it is handed back wide.
fn table_text(
    word: &str,
    enumeration: u32,
    levels: Levels<'_>,
    names: &mut Names<'_>,
    rows: &mut dyn FnMut(Level<'_>) -> Rows,
) -> Vec<u16> {
    let mut out: Vec<u8> = Vec::new();
    let mut name = String::new();
    for (i, level) in levels.levels().into_iter().enumerate() {
        let values = rows(level);
        out.extend_from_slice(
            format!(
                "{} {word} stats in {} qualities level {i}\n",
                values.len(),
                label(i)
            )
            .as_bytes(),
        );
        for (key, value) in values {
            if let Some(n) = names(enumeration, key) {
                name = n;
            }
            out.extend_from_slice(format!("{name}({key}): ").as_bytes());
            out.extend_from_slice(&value);
            out.push(b'\n');
        }
        out.push(b'\n');
    }
    widen(&out)
}

/// The line a dump of an object with no qualities writes.
fn missing(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// A level's table of a kind, each value through `write`: `of_own` and `of_record` pick the table.
fn listed<'l, O: 'l, R: 'l>(
    level: Level<'l>,
    of_own: impl FnOnce(&'l Qualities) -> &'l std::collections::BTreeMap<u32, O>,
    of_record: impl FnOnce(&'l Record) -> &'l [(u32, R)],
    mut own_value: impl FnMut(&O) -> Vec<u8>,
    mut record_value: impl FnMut(&R) -> Vec<u8>,
) -> Rows {
    match level {
        Level::Own(o) => own(of_own(&o.qualities))
            .into_iter()
            .map(|(k, v)| (k, own_value(v)))
            .collect(),
        Level::Record(r) => stored(of_record(r))
            .into_iter()
            .map(|(k, v)| (k, record_value(v)))
            .collect(),
    }
}

fn yes_or_no(v: i32) -> &'static str {
    if v == 0 {
        "false"
    } else {
        "true"
    }
}

/// The yes-or-no qualities: `true` or `false`.
// D2-08308, D2-07271, D2-07281
#[must_use]
pub fn bools_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpBoolsToString: ERROR: missing qualities!!");
    }
    let v = |x: &i32| yes_or_no(*x).as_bytes().to_vec();
    table_text("bool", 0x4000_0020, levels, names, &mut |l| {
        listed(l, |q| &q.bools, |r| &r.bools, v, v)
    })
}

/// The data ids: `0x` and eight hexadecimal digits.
// D2-08309, D2-07275, D2-07285
#[must_use]
pub fn data_ids_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpDIDsToString: ERROR: missing qualities!!");
    }
    let v = |x: u32| format!("0x{x:08X}").into_bytes();
    table_text("did", 0x4000_0023, levels, names, &mut |l| {
        listed(l, |q| &q.data_ids, |r| &r.data_ids, |x| v(*x), |x| v(x.0))
    })
}

/// The floats: two decimals.
// D2-08310, D2-07272, D2-07282
#[must_use]
pub fn floats_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpFloatsToString: ERROR: missing qualities!!");
    }
    let v = |x: &f32| fixed(f64::from(*x), 2).into_bytes();
    table_text("float", 0x4000_0021, levels, names, &mut |l| {
        listed(l, |q| &q.floats, |r| &r.floats, v, v)
    })
}

/// The object ids: `0x` and sixteen hexadecimal digits.
// D2-08311, D2-07276, D2-07286
#[must_use]
pub fn instance_ids_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpIIDsToString: ERROR: missing qualities!!");
    }
    let v = |x: &u64| format!("0x{x:016X}").into_bytes();
    table_text("iid", 0x4000_0024, levels, names, &mut |l| {
        listed(l, |q| &q.instance_ids, |r| &r.instance_ids, v, v)
    })
}

/// The whole numbers, read unsigned.
// D2-08312, D2-07269, D2-07279
#[must_use]
pub fn ints_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumIntsToString: ERROR: missing qualities!!");
    }
    let v = |x: &i32| x.cast_unsigned().to_string().into_bytes();
    table_text("int", 0x4000_001F, levels, names, &mut |l| {
        listed(l, |q| &q.ints, |r| &r.ints, v, v)
    })
}

/// The 64-bit numbers: their low 32 bits, read unsigned, which is all the client's format prints.
// D2-08313, D2-07270, D2-07280
#[must_use]
pub fn long_ints_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumLongIntsToString: ERROR: missing qualities!!");
    }
    let v = |x: u64| (x & 0xFFFF_FFFF).to_string().into_bytes();
    table_text("longint", 0x4000_0050, levels, names, &mut |l| {
        listed(
            l,
            |q| &q.long_ints,
            |r| &r.long_ints,
            |x| v(*x),
            |x| v(x.cast_unsigned()),
        )
    })
}

/// The texts, as held.
// D2-08314, D2-07274, D2-07284
#[must_use]
pub fn strings_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpStringsToString: ERROR: missing qualities!");
    }
    table_text("string", 0x4000_0022, levels, names, &mut |l| {
        listed(
            l,
            |q| &q.strings,
            |r| &r.strings,
            |x| x.0.clone(),
            // A record's texts were read a byte a character.
            |x: &String| x.chars().map(|c| c as u8).collect(),
        )
    })
}

/// The times: two decimals.
// D2-08315, D2-07273, D2-07283
#[must_use]
pub fn timestamps_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpTSsToString: ERROR: missing qualities!");
    }
    let v = |x: &f64| fixed(*x, 2).into_bytes();
    table_text("ts", 0x4000_0026, levels, names, &mut |l| {
        listed(l, |q| &q.timestamps, |r| &r.times, v, v)
    })
}

/// The positions, written as a position writes itself.
// D2-08316, D2-07277, D2-07287
#[must_use]
pub fn positions_text(levels: Levels<'_>, names: &mut Names<'_>) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpPositionsToString: ERROR: missing qualities!");
    }
    let v = |p: &Placement| {
        let (cell, frame) = position_words(p);
        auberean_glue::position::position_text(cell, &frame).into_bytes()
    };
    table_text("position", 0x4000_0025, levels, names, &mut |l| {
        listed(
            l,
            |q| &q.positions,
            |r| &r.positions,
            v,
            |p| {
                v(&Placement {
                    cell: p.cell.0,
                    frame: p.frame,
                })
            },
        )
    })
}

/// A string reference for a log: its text with its flags, or a line saying it cannot show.
fn info_text(mut info: auberean_text::StringInfo, src: &mut dyn Source) -> Vec<u8> {
    if info.is_ready(src) {
        narrow(&info.log_string(src, true))
    } else {
        b"Unable to display StringInfo".to_vec()
    }
}

/// The string references, written for a log.
// D2-08317, D2-07278, D2-07288
pub fn string_infos_text(
    levels: Levels<'_>,
    src: &mut dyn Source,
    names: &mut Names<'_>,
) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpStringInfosToString: ERROR: missing qualities!");
    }
    table_text("stringinfo", 0x4000_0046, levels, names, &mut |l| match l {
        Level::Own(o) => own(&o.qualities.string_infos)
            .into_iter()
            .map(|(k, s)| (k, info_text(from_wire(s), src)))
            .collect(),
        Level::Record(r) => stored(&r.string_infos)
            .into_iter()
            .map(|(k, s)| {
                let v = auberean_glue::entity::text_info(s)
                    .map(|t| info_text(crate::text::from_script(&t), src))
                    .unwrap_or_default();
                (k, v)
            })
            .collect(),
    })
}

/// A description's contents: each quality it has set, in the client's own order, under a heading.
/// Its name and plural name are written for a log, its icon and its physics numbers in eight
/// hexadecimal digits, its objects in sixteen, its other numbers in decimal.
// D2-12076
pub fn description_text(w: &WeenieDesc, src: &mut dyn Source, names: &mut Names<'_>) -> Vec<u8> {
    const STRING_INFO: u32 = 0x4000_0046;
    const DATA_ID: u32 = 0x4000_0023;
    const OBJECT: u32 = 0x4000_0024;
    const FLOAT: u32 = 0x4000_0021;
    const BOOL: u32 = 0x4000_0020;
    const INT: u32 = 0x4000_001F;
    let mut out = b"Weenie description contains:\n".to_vec();
    let mut name = String::new();
    let mut line = |enumeration: u32, q: u32, value: &[u8]| {
        if let Some(n) = names(enumeration, q) {
            name = n;
        }
        out.extend_from_slice(format!("{name}({q}): ").as_bytes());
        out.extend_from_slice(value);
        out.push(b'\n');
    };
    for q in [0x100, 0x101] {
        if let Some(s) = description::string_info(w, q) {
            line(STRING_INFO, q, &narrow(&from_wire(s).log_string(src, true)));
        }
    }
    if let Some(v) = description::data_id(w, 0x101) {
        line(DATA_ID, 0x101, format!("{v:08X}").as_bytes());
    }
    let object = |line: &mut dyn FnMut(u32, u32, &[u8]), q: u32| {
        if let Some(v) = description::instance_id(w, q) {
            line(OBJECT, q, format!("0x{v:016X}").as_bytes());
        }
    };
    object(&mut line, 0x102);
    object(&mut line, 0x105);
    if let Some(v) = description::float(w, 0x12A) {
        line(FLOAT, 0x12A, fixed_eight(f64::from(v)).as_bytes());
    }
    for q in [0x1C4, 0x141, 0x142, 0x140, 0x1F9] {
        object(&mut line, q);
    }
    if let Some(v) = description::bool(w, 0x2BE) {
        line(BOOL, 0x2BE, yes_or_no(v).as_bytes());
    }
    for q in [0x106, 0x264, 0x26B, 0x26A, 2000, 0x226, 0x227] {
        if let Some(v) = int(w, q) {
            line(INT, q, v.cast_unsigned().to_string().as_bytes());
        }
    }
    for q in [3, 4, 7, 8, 5, 6] {
        if let Some(v) = int(w, q) {
            line(INT, q, format!("{v:08X}").as_bytes());
        }
    }
    for q in [0x270, 10, 0x117] {
        if let Some(v) = description::bool(w, q) {
            line(BOOL, q, yes_or_no(v).as_bytes());
        }
    }
    out
}

/// Six decimals, padded with zeros to eight characters.
fn fixed_eight(x: f64) -> String {
    let s = fixed(x, 6);
    match s.strip_prefix('-') {
        Some(rest) => format!("-{rest:0>7}"),
        None => format!("{s:0>8}"),
    }
}

/// A record's description with its names, as an object carries one.
fn record_description(d: &ObjectDescription) -> WeenieDesc {
    let wire = |s: &auberean_assets::property::StringInfo| {
        auberean_glue::entity::text_info(s)
            .ok()
            .map(|t| crate::repository::string_info_of(&t))
    };
    let mut w = description::of_record(d);
    if let Some(n) = d.name.as_ref().and_then(wire) {
        w.name = n;
        w.mask |= WeenieDesc::NAME;
    }
    if let Some(n) = d.plural_name.as_ref().and_then(wire) {
        w.plural_name = n;
        w.mask |= WeenieDesc::PLURAL_NAME;
    }
    w
}

/// An object's description as its repository answers it: none for an object the client does not
/// hold.
// D2-08222, D2-07352
#[must_use]
pub fn inq_description(object: Option<&Object>) -> Option<WeenieDesc> {
    object.map(|o| o.weenie.clone())
}

/// What an object says of its description: the contents, or a line saying it has none.
// D2-00683
pub fn own_description_text(
    object: Option<&Object>,
    src: &mut dyn Source,
    names: &mut Names<'_>,
) -> Vec<u8> {
    match inq_description(object) {
        Some(w) => description_text(&w, src, names),
        None => b"No Weenie Desc.".to_vec(),
    }
}

/// What each level's description holds: a heading and its contents, or a line saying a record has
/// none (an object always has one here).
// D2-08306, D2-08307, D2-07361
pub fn descriptions_text(
    levels: Levels<'_>,
    src: &mut dyn Source,
    names: &mut Names<'_>,
) -> Vec<u16> {
    if levels.object.is_none() {
        return missing("WREPOSITORY::DumpWeenieDesc: ERROR: missing qualities!");
    }
    let mut out: Vec<u8> = Vec::new();
    for (i, level) in levels.levels().into_iter().enumerate() {
        out.extend_from_slice(
            format!("WeenieDesc contents in {} qualities level {i}\n", label(i)).as_bytes(),
        );
        match level {
            Level::Own(o) => out.extend(description_text(&o.weenie, src, names)),
            Level::Record(r) => match &r.description {
                Some(d) => out.extend(description_text(&record_description(d), src, names)),
                None => out.extend_from_slice(b"No WeenieDesc"),
            },
        }
        out.push(b'\n');
    }
    widen(&out)
}

/// The natives: the object (the receiver; a null one crashes the call), whether to go on to its
/// record (`fp[-2]`), a new string below them.
fn native(
    c: &mut Call<'_>,
    write: fn(Levels<'_>, &mut dyn Source, &mut Names<'_>) -> Vec<u16>,
) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let recurse = c.arg(0)? != 0;
    let id = receiver_object(c)?;
    let h = host(c)?;
    let record = h.record(h.base_of(id));
    let levels = Levels {
        object: h.model.object(id),
        record: record.as_deref(),
        recurse,
    };
    let database = h
        .cache
        .as_ref()
        .filter(|d| !d.is_empty())
        .map(crate::cache::DataCache::database);
    let mut names = |e: u32, v: u32| database.and_then(|d| d.enumeration_name(e, v));
    let text = write(levels, &mut h.text, &mut names);
    let r =
        c.vm.heap
            .add_native(native_type::WP_STRING, NativeObject::Text(text));
    c.write(-3, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-04829
fn dump_bools(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| bools_text(l, n))
}

// D2-04830
fn dump_data_ids(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| data_ids_text(l, n))
}

// D2-04831
fn dump_floats(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| floats_text(l, n))
}

// D2-04832
fn dump_instance_ids(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| instance_ids_text(l, n))
}

// D2-04833
fn dump_ints(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| ints_text(l, n))
}

// D2-04834
fn dump_long_ints(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| long_ints_text(l, n))
}

// D2-04835
fn dump_positions(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| positions_text(l, n))
}

// D2-04836
fn dump_string_infos(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, string_infos_text)
}

// D2-04837
fn dump_strings(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| strings_text(l, n))
}

// D2-04838
fn dump_timestamps(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, |l, _, n| timestamps_text(l, n))
}

// D2-04839
fn dump_descriptions(c: &mut Call<'_>) -> Result<Status, Stop> {
    native(c, descriptions_text)
}

/// The dumps' natives, by id.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    const NATIVES: &[(u32, NativeFn)] = &[
        (0x57, dump_bools),
        (0x58, dump_data_ids),
        (0x59, dump_floats),
        (0x5A, dump_ints),
        (0x5B, dump_instance_ids),
        (0x5C, dump_positions),
        (0x5D, dump_strings),
        (0x5E, dump_timestamps),
        (0x5F, dump_string_infos),
        (0x60, dump_descriptions),
        (0x6C, dump_long_ints),
    ];
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(REPOSITORY, *n), *f))
        .chain(std::iter::once((
            NativeId::new(OBJECT_TABLE, 0x03),
            dump_object_table as NativeFn,
        )))
}

/// The client's object table as text, into the string the reference at `fp[-2]` reaches: for each object, its id and name,
/// its package and entity, and its place.
// D2-00979, D2-05682
fn dump_object_table(c: &mut Call<'_>) -> Result<Status, Stop> {
    let mut text = String::from("\n***Object Maintainter Dump***\n\n");
    {
        let h = host(c)?;
        let mut objects: Vec<&Object> = h.model.objects().collect();
        objects.sort_by_key(|o| o.id);
        for o in objects {
            let name = o
                .weenie
                .name
                .literal
                .as_ref()
                .map_or_else(|| "no weenie object".to_owned(), |l| l.text());
            text.push_str(&format!("Instance: 0x{:016X} {name}\n", o.id));
            text.push_str(&format!(
                "Package: 0x{:08X} DID: 0x{:08X} \n",
                o.weenie.package_id, o.weenie.entity_did
            ));
            match o.place {
                Some(p) => text.push_str(&format!("Position: {:08X} {:?}\n\n", p.cell, p.origin)),
                None => text.push_str("Position: no physics object\n\n"),
            }
        }
    }
    let out = c.cx.at(-2)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target != NULL {
        *c.vm.heap.native_mut(target)? = NativeObject::Text(text.encode_utf16().collect());
    }
    Ok(Status::Success)
}
