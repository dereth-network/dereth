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
];

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
// D2-09552, D2-16499
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
