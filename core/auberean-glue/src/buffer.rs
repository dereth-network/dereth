//! Argument buffers, and sending a script call to the other side.
//!
//! A sending function makes a buffer native, packs its parameters into it last first (each a tag
//! word and the value, an object as its packed graph), and hands the buffer and the other side's
//! function id to the interface's send native, which passes both to the host.

use auberean_script::args::pack_tag;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::NativeObject;
use auberean_script::value::{join, split, tag, NULL};
use auberean_script::wire::In;

use crate::machine::ret;

/// The buffer native package.
const BUFFER: u32 = 0x50;
/// The interface native package (the engine object scripts talk to the world through).
const INTERFACE: u32 = 0x7F;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (BUFFER, 1, pack_object),
    (BUFFER, 2, pack_int),
    (BUFFER, 3, pack_long_int),
    (BUFFER, 4, pack_float),
    (BUFFER, 5, pack_long_float),
    (BUFFER, 6, unpack_object),
    (BUFFER, 7, unpack_int),
    (BUFFER, 8, unpack_long_int),
    (BUFFER, 9, unpack_float),
    (BUFFER, 0xA, unpack_long_float),
    (INTERFACE, 0x36, send),
    (INTERFACE, 0x37, send_encrypted),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

fn buffer<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut Vec<u8>, Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Buffer(b) => Ok(b),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not an argument buffer".into(),
        }),
    }
}

// D2-16742
fn append(c: &mut Call<'_>, words: &[u32]) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let b = buffer(c, r)?;
    for w in words {
        b.extend_from_slice(&w.to_le_bytes());
    }
    Ok(Status::Success)
}

/// Pack an integer.
// D2-08775
fn pack_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    append(c, &[pack_tag::INT, v])
}

/// Pack a float.
// D2-08774
fn pack_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.arg(0)?;
    append(c, &[pack_tag::FLOAT, v])
}

/// Pack a 64-bit integer (two slots, low word first).
// D2-08777
fn pack_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (lo, hi) = (c.cx.at(-3)?, c.cx.at(-2)?);
    append(c, &[pack_tag::LONG_INT, lo, hi])
}

/// Pack a 64-bit float.
// D2-08776
fn pack_long_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (lo, hi) = (c.cx.at(-3)?, c.cx.at(-2)?);
    append(c, &[pack_tag::LONG_FLOAT, lo, hi])
}

/// Pack an object: its tag, then the graph its reference reaches.
// D2-08778, D2-16746
fn pack_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let obj = c.arg(0)?;
    let graph = c.vm.heap.export(obj)?;
    let mut bytes = pack_tag::OBJECT.to_le_bytes().to_vec();
    bytes.extend(graph.encode().map_err(Stop::Codec)?);
    buffer(c, r)?.extend(bytes);
    Ok(Status::Success)
}

/// Take the next value off the front of a buffer when its tag is `want`; the out reference says
/// whether it was.
fn take(c: &mut Call<'_>, want: u32, words: usize) -> Result<Option<Vec<u32>>, Stop> {
    let r = c.receiver()?;
    let ok_ref = c.arg(0)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, ok_ref, 0, None, tag::MIXED, &prog)?;
    let b = buffer(c, r)?;
    let mut rd = In::new(b);
    let got = (|| -> Option<Vec<u32>> {
        if rd.u32().ok()? != want {
            return None;
        }
        (0..words).map(|_| rd.u32().ok()).collect()
    })();
    if got.is_some() {
        let used = 4 + 4 * words;
        b.drain(..used);
    }
    let ok = u32::from(got.is_some());
    c.vm.write_data(c.host, c.cx, ok_ref, 0, ok, &prog)?;
    Ok(got)
}

/// A 64-bit result below one argument: both words, the first slot tagged as a native's long.
fn write_long_result(c: &mut Call<'_>, v: u64) -> Result<Status, Stop> {
    let (lo, hi) = split(v);
    c.write(-4, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-3)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// Take an integer off the front.
// D2-08780, D2-16737
fn unpack_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = take(c, pack_tag::INT, 1)?.map_or(0, |w| w[0]);
    ret(c, 1, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Take a float off the front.
// D2-08779, D2-16739
fn unpack_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = take(c, pack_tag::FLOAT, 1)?.map_or(0, |w| w[0]);
    ret(c, 1, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Take a 64-bit integer off the front.
// D2-08782, D2-16738
fn unpack_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = take(c, pack_tag::LONG_INT, 2)?.map_or(0, |w| join(w[0], w[1]));
    write_long_result(c, v)
}

/// Take a 64-bit float off the front.
// D2-08781, D2-16740
fn unpack_long_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = take(c, pack_tag::LONG_FLOAT, 2)?.map_or(0, |w| join(w[0], w[1]));
    write_long_result(c, v)
}

/// Take an object off the front: its graph read into the table.
// D2-08783, D2-16741
fn unpack_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let ok_ref = c.arg(0)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, ok_ref, 0, None, tag::MIXED, &prog)?;
    let bytes = buffer(c, r)?.clone();
    let mut rd = In::new(&bytes);
    let graph = match rd.u32() {
        Ok(pack_tag::OBJECT) => auberean_script::graph::Graph::read(&mut rd, &prog.library).ok(),
        _ => None,
    };
    let id = match graph {
        Some(g) => {
            let used = rd.pos();
            buffer(c, r)?.drain(..used);
            c.vm.heap.import(&g, None)?
        }
        None => NULL,
    };
    c.vm.write_data(c.host, c.cx, ok_ref, 0, u32::from(id != NULL), &prog)?;
    ret(c, 1, id, tag::MIXED)?;
    Ok(Status::Success)
}

fn send_with(c: &mut Call<'_>, encrypted: bool) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let function = c.arg(0)?;
    let buf = c.arg(1)?;
    let bytes = buffer(c, buf)?.clone();
    c.host.send(function, &bytes, encrypted)?;
    ret(c, 2, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Send a script call to the other side: the function id and the buffer.
// D2-04669, D2-08055, D2-03980
fn send(c: &mut Call<'_>) -> Result<Status, Stop> {
    send_with(c, false)
}

/// Send a script call to the other side through the encrypted carrier.
// D2-04670
fn send_encrypted(c: &mut Call<'_>) -> Result<Status, Stop> {
    send_with(c, true)
}
