//! The machine's own natives: singletons, packages and their names, copying an object, the
//! instance of the running process.
//!
//! A native finds its arguments below its receiver (`fp[-2]` the first) and writes its result
//! into the slot below them, with the tag the client gives it: a reference result is tagged as a
//! reference, every other result as written by a native.

use std::sync::Arc;

use auberean_script::library::STATE_BASE;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::value::{tag, NULL};

/// The machine's native package.
const MACHINE: u32 = 0x72;

/// The natives that make an object from a type's name, and those that tell ids apart.
const TYPE_INFO: u32 = 0x6E;
const RESERVED_IDS: u32 = 0x63;
const DISTRIBUTOR: u32 = 0x7E;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (MACHINE, 2, singleton_by_name),
    (MACHINE, 3, singleton_by_package),
    (MACHINE, 4, singleton_by_package),
    (MACHINE, 5, package_by_name),
    (MACHINE, 6, package_of_reference),
    (MACHINE, 7, package_derives_from),
    (MACHINE, 0xD, collect_unused),
    (MACHINE, 0xE, package_name),
    (MACHINE, 0x10, collect_now),
    (MACHINE, 0x11, collect_setting),
    (MACHINE, 0x12, collect_setting),
    (MACHINE, 0x13, copy_object),
    (MACHINE, 0x14, current_instance),
    (MACHINE, 0x15, instance_of),
    (MACHINE, 0x18, data_object),
    (MACHINE, 0x19, state_of),
    (MACHINE, 0x08, query_process_table),
    (MACHINE, 0x09, query_processes),
    (MACHINE, 0x0A, query_process),
    (MACHINE, 0x0B, query_references),
    (MACHINE, 0x0C, query_perf),
    (MACHINE, 0x16, query_reference_tree),
    (MACHINE, 0x17, query_reference_tree_for_process),
    (DISTRIBUTOR, 0x01, process_id),
    (TYPE_INFO, 0x01, get_type),
    (TYPE_INFO, 0x02, new_type),
    (TYPE_INFO, 0x03, new_validated_type),
    (RESERVED_IDS, 0x01, is_player_id),
];

/// A new string, for a native's answer.
pub(crate) fn new_text_in(c: &mut Call<'_>, units: Vec<u16>) -> u32 {
    new_text(c, units)
}

/// The process an object runs in (at `fp[-2]`): the one whose instance it is, or none.
// D2-04656, D2-08033
fn process_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let id =
        c.vm.process_slots()
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.instance == r))
            .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
    c.write(-2, id, tag::MIXED)?;
    Ok(Status::Success)
}

/// Is an id (a 64-bit value at `fp[-3]`) a player's? Its top byte's marks say, dynamic and static
/// ids being told first.
// D2-08943
fn is_player_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.long_at(-3)?;
    let top = (id >> 56) as u32;
    let player = top & 0x80 == 0 && top & 0x40 == 0 && top & 0x20 != 0;
    c.write(-4, u32::from(player), tag::MIXED)?;
    Ok(Status::Success)
}

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// Write a result slot below `args` arguments.
pub(crate) fn ret(c: &mut Call<'_>, args: i64, v: u32, t: u8) -> Result<(), Stop> {
    c.write(-2 - args, v, t)
}

/// The text of a string object.
pub(crate) fn text_of(c: &Call<'_>, id: u32) -> Result<Vec<u16>, Stop> {
    match c.vm.heap.native(id)? {
        NativeObject::Text(t) => Ok(t.clone()),
        _ => Err(c.fail(format!("{id:#x} is not a string"))),
    }
}

/// A new string object.
pub(crate) fn new_text(c: &mut Call<'_>, units: Vec<u16>) -> u32 {
    c.vm.heap
        .add_native(native_type::WP_STRING, NativeObject::Text(units))
}

/// The singleton of a package: the default instance's saved state's singleton.
fn singleton_of(c: &mut Call<'_>, package: u32) -> u32 {
    if package == NULL {
        return NULL;
    }
    c.vm.heap.singleton(STATE_BASE | (package & 0xFFFF))
}

/// The singleton of a package named by a string.
// D2-09559, D2-16483, D2-16482
fn singleton_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name: String = char::decode_utf16(text_of(c, c.arg(0)?)?)
        .map(|r| r.unwrap_or('?'))
        .collect();
    let id = match c.vm.program().library.package_id(&name) {
        Some(p) if p != NULL => singleton_of(c, p),
        _ => NULL,
    };
    ret(c, 1, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The singleton of a package by id (`GetInstance`).
// D2-09558, D2-16484
fn singleton_by_package(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = c.arg(0)?;
    let id = singleton_of(c, p);
    ret(c, 1, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A package id by name, none when the library has no such package.
// D2-09556, D2-16480
fn package_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name: String = char::decode_utf16(text_of(c, c.arg(0)?)?)
        .map(|r| r.unwrap_or('?'))
        .collect();
    let id = c.vm.program().library.package_id(&name).unwrap_or(NULL);
    ret(c, 1, id, tag::MIXED)?;
    Ok(Status::Success)
}

/// The package of the object a reference names (filling a singleton first).
// D2-09565, D2-00973
fn package_of_reference(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let prog = Arc::clone(c.vm.program());
    let p = c.vm.package_of(c.host, r, &prog)?;
    ret(c, 1, p, tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether a package is a second package or derives from it.
// D2-09560, D2-16444
fn package_derives_from(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (p, base) = (c.arg(0)?, c.arg(1)?);
    let yes = c.vm.program().library.derives_from(p, base);
    ret(c, 2, u32::from(yes), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether the object a reference names is of a package or derives from it.
// D2-09561, D2-16498
fn instance_of(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (r, base) = (c.arg(0)?, c.arg(1)?);
    let yes = if r == NULL {
        false
    } else {
        let prog = Arc::clone(c.vm.program());
        let p = c.vm.package_of(c.host, r, &prog)?;
        prog.library.derives_from(p, base)
    };
    ret(c, 2, u32::from(yes), tag::MIXED)?;
    Ok(Status::Success)
}

/// Free what is unused: collection runs between calls here, so nothing is freed mid-run.
// D2-09553, D2-16471
fn collect_unused(c: &mut Call<'_>) -> Result<Status, Stop> {
    ret(c, 0, 0, tag::MIXED)?;
    Ok(Status::Success)
}

/// Collect now: collection runs between calls here.
// D2-09549
fn collect_now(_: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(Status::Success)
}

/// The collector's settings: collection runs between calls here, so they change nothing.
// D2-09550, D2-09551, D2-16453, D2-16454
fn collect_setting(_: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(Status::Success)
}

/// A package's name as a new string (empty when the library has none for the id).
// D2-09563, D2-16475, D2-16476
fn package_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = c.arg(0)?;
    let name =
        c.vm.program()
            .library
            .package(p)
            .map(|x| x.name.clone())
            .unwrap_or_default();
    let id = new_text(c, name.encode_utf16().collect());
    ret(c, 1, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A deep copy of the object a reference names: its graph packed and read back under new ids.
// D2-09552, D2-16499, D2-13895, D2-13896
fn copy_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let id = if r == NULL || c.vm.heap.entry(r).is_none() {
        NULL
    } else {
        let g = c.vm.heap.export(r)?;
        c.vm.heap.import(&g, None)?
    };
    ret(c, 1, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The instance of the process the running context belongs to.
// D2-09562, D2-16455
fn current_instance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.vm.instance_of(c.cx.process);
    ret(c, 0, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The singleton filled from a saved state, when that state's package is a given package or
/// derives from it; none otherwise.
// D2-09557, D2-16486
fn data_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (state, package) = (c.arg(0)?, c.arg(1)?);
    let mut id = NULL;
    if state != 0 && package != NULL {
        if let Some(s) = c.host.saved_state(state) {
            if c.vm.program().library.derives_from(s.package, package) {
                id = c.vm.heap.singleton(state);
            }
        }
    }
    ret(c, 2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The saved state an object came from (0 for a reference that is not to an object).
// D2-09555
fn state_of(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.arg(0)?;
    let s = match c.vm.heap.entry(r) {
        Some(e)
            if e.field & auberean_script::graph::field::KIND
                == auberean_script::graph::field::OBJECT =>
        {
            c.vm.heap.object(r)?.state
        }
        _ => 0,
    };
    ret(c, 1, s, tag::MIXED)?;
    Ok(Status::Success)
}

/// Put a query's text in the string an out reference (`fp[k]`) holds; the query then answers `v`
/// below its arguments. A null string stops the call.
fn query_answer(c: &mut Call<'_>, k: i64, args: i64, text: &str, v: u32) -> Result<Status, Stop> {
    let out = c.cx.at(k)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Ok(Status::Crash);
    }
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => *t = text.encode_utf16().collect(),
        _ => return Err(c.fail("the out reference holds no string")),
    }
    ret(c, args, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The process table's counts as text: how many it holds and how many it has room for, 128 at
/// first and doubled when full.
// D2-09567, D2-16465, D2-16428
fn query_process_table(c: &mut Call<'_>) -> Result<Status, Stop> {
    let total = c.vm.process_slots().len();
    let mut size = 128;
    while size < total {
        size *= 2;
    }
    let text = format!("process table:\n  total: {total}, size: {size}\n");
    query_answer(c, -2, 1, &text, 1)
}

/// How many contexts the processes have, as text: each one's it keeps and the one running.
// D2-09569, D2-16466, D2-16429
fn query_processes(c: &mut Call<'_>) -> Result<Status, Stop> {
    let kept: usize =
        c.vm.process_slots()
            .iter()
            .flatten()
            .map(|p| p.suspended.len())
            .sum();
    let total = kept + usize::from(c.vm.process(c.vm.running_process).is_some());
    let text = format!("contexts:\n  total: {total}\n");
    query_answer(c, -3, 2, &text, 1)
}

/// Whether a process (`fp[-2]`) is in the table; the text says nothing of it.
// D2-09568, D2-16467, D2-16425
fn query_process(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = c.arg(0)?;
    let there = usize::try_from(p)
        .ok()
        .is_some_and(|p| c.vm.process(p).is_some());
    query_answer(c, -3, 2, "", u32::from(there))
}

/// The references, as nothing: the shipped client's listing is empty.
// D2-09572, D2-16468
fn query_references(c: &mut Call<'_>) -> Result<Status, Stop> {
    query_answer(c, -2, 1, "", 1)
}

/// The performance counters: a heading and a blank line.
// D2-09566, D2-16472
fn query_perf(c: &mut Call<'_>) -> Result<Status, Stop> {
    query_answer(c, -2, 1, "perf:\n\n", 1)
}

/// A reference's tree, as nothing: the shipped client's listing is empty.
// D2-09571, D2-16469
fn query_reference_tree(c: &mut Call<'_>) -> Result<Status, Stop> {
    query_answer(c, -2, 2, "", 1)
}

/// A process's reference tree, as nothing, whether or not there is such a process.
// D2-09570, D2-16470
fn query_reference_tree_for_process(c: &mut Call<'_>) -> Result<Status, Stop> {
    query_answer(c, -2, 2, "", 1)
}

/// A new string holding the name of a package (empty when it has none).
// D2-09546, D2-16438
fn get_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    let package = c.arg(0)?;
    let name =
        c.vm.program()
            .library
            .package(package)
            .map(|x| x.name.clone())
            .unwrap_or_default();
    let id = new_text(c, name.encode_utf16().collect());
    ret(c, 1, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A new instance of the package a string names; a name no package has stops the call.
// D2-09547, D2-16439
fn new_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    let name: String = char::decode_utf16(text_of(c, c.arg(0)?)?)
        .map(|r| r.unwrap_or('?'))
        .collect();
    let prog = Arc::clone(c.vm.program());
    let Some(package) = prog.library.package_id(&name) else {
        ret(c, 1, NULL, tag::REFERENCE)?;
        return Ok(Status::Crash);
    };
    let id = c.vm.new_object(c.host, package, &prog)?;
    ret(c, 1, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// What a validated new type answers: always -1, and the status is a success.
// D2-09548, D2-00031
fn new_validated_type(c: &mut Call<'_>) -> Result<Status, Stop> {
    c.write(-4, u32::MAX, tag::REFERENCE)?;
    Ok(Status::Success)
}
