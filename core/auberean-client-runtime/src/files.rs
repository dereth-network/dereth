//! The files scripts write: a chat log, an exported allegiance. The client writes them in its own
//! folder, through the host's [`LocalSystem`]; it never touches a disk itself. A path a script
//! builds on the client's folder (the output path the interface gives) names the file after it; a
//! path that is not then a bare name ([`auberean_script::local::bare_name`]) does not open.
//!
//! Files are text in 16-bit units, little-endian, after a byte-order mark (`FF FE`):
//!
//! - **opening** a file to write starts it anew, the mark alone; opening it to add to it keeps
//!   what it holds, unless it holds something that does not start with the mark (then it does not
//!   open, answering 0); an empty or new one gets the mark; opening one to read needs it to be
//!   there. A file the host cannot write or read does not open. An open file is open under its path
//!   until it is closed; opening answers 1;
//! - **writing** a line to a file that is not open opens it to add to it first (answering 0 when
//!   that fails); each line feed in the text is written as a carriage return and a line feed, and
//!   a text that does not end in a line feed gets a carriage return and a line feed after it;
//!   writing answers whether the host wrote it, and a file open to read takes nothing (answering
//!   1);
//! - **writing with a timestamp** writes the local time first, `<Mar 05 2013, 14:07:09>  ` (when
//!   the host has a clock), then the line as above;
//! - **closing** a file answers whether it was open.

use std::collections::BTreeMap;

use auberean_script::local::{bare_name, LocalSystem, LocalTime};
use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{native_type, Item, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};

use crate::host::ClientHost;

/// The file natives' package.
const PACKAGE: u32 = 0x26;

/// The native package of the original's mail, and the file this client keeps the reports in.
const MAIL: u32 = 0x21;
const REPORTS: &str = "bug-reports.txt";

/// The byte-order mark a file starts with.
const MARK: [u8; 2] = [0xFF, 0xFE];

/// How a file is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Open {
    Read,
    Write,
}

/// The files scripts have open, by path (as the scripts spelled it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
// D2-08568
pub struct Files {
    pub open: BTreeMap<Vec<u16>, Open>,
}

/// The file a script's path names in the client's folder (`folder`, the output path scripts build
/// on): the path after the folder and a backslash, else the path itself. None when it is not text.
#[must_use]
pub fn file_name(folder: &[u16], path: &[u16]) -> Option<String> {
    let rest = match path.strip_prefix(folder) {
        Some([0x5C, rest @ ..]) => rest,
        _ => path,
    };
    String::from_utf16(rest).ok()
}

/// A line as a file holds it: each line feed after a carriage return, and a carriage return and a
/// line feed after a line that does not end in one, in 16-bit units, little-endian.
fn line_bytes(text: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2 + 4);
    for &u in text {
        if u == u16::from(b'\n') {
            out.extend_from_slice(&u16::from(b'\r').to_le_bytes());
        }
        out.extend_from_slice(&u.to_le_bytes());
    }
    if text.last() != Some(&u16::from(b'\n')) {
        out.extend_from_slice(&[b'\r', 0, b'\n', 0]);
    }
    out
}

/// The month names a timestamp uses.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A log line's timestamp, `<%b %d %Y, %H:%M:%S>  ` in the C locale.
#[must_use]
pub fn timestamp(t: &LocalTime) -> String {
    let month = MONTHS
        .get(usize::from(t.month.max(1)) - 1)
        .copied()
        .unwrap_or("");
    format!(
        "<{month} {:02} {:04}, {:02}:{:02}:{:02}>  ",
        t.day, t.year, t.hour, t.minute, t.second
    )
}

impl Files {
    /// Open a file (`path` as the script spelled it, `name` the file it names in the client's
    /// folder): to read, to write it anew, or to add to it. Whether it opened.
    // D2-08575
    pub fn open(
        &mut self,
        disk: &mut dyn LocalSystem,
        path: &[u16],
        name: &str,
        read: bool,
        append: bool,
    ) -> bool {
        let mode = if read {
            if disk.read_file(name).is_none() {
                return false;
            }
            Open::Read
        } else if append {
            let ready = match disk.read_file(name) {
                None => disk.write_file(name, &MARK),
                Some(held) if held.is_empty() => disk.append_line(name, &MARK),
                Some(held) => held.starts_with(&MARK),
            };
            if !ready {
                return false;
            }
            Open::Write
        } else {
            if !disk.write_file(name, &MARK) {
                return false;
            }
            Open::Write
        };
        self.open.insert(path.to_vec(), mode);
        true
    }

    /// Write a line, opening the file to add to it when it is not open. Whether it was written
    /// (a file open to read answers yes and takes nothing).
    // D2-08581, D2-08579
    pub fn write(
        &mut self,
        disk: &mut dyn LocalSystem,
        path: &[u16],
        name: &str,
        text: &[u16],
    ) -> bool {
        self.write_stamped(disk, path, name, None, text)
    }

    /// Write a line after a timestamp, when given one, opening the file to add to it when it is not
    /// open. Whether it was written.
    // D2-08580
    pub fn write_stamped(
        &mut self,
        disk: &mut dyn LocalSystem,
        path: &[u16],
        name: &str,
        stamp: Option<&LocalTime>,
        text: &[u16],
    ) -> bool {
        if !self.open.contains_key(path) && !self.open(disk, path, name, false, true) {
            return false;
        }
        if self.open.get(path) != Some(&Open::Write) {
            return true;
        }
        let mut bytes = Vec::new();
        if let Some(t) = stamp {
            bytes.extend(timestamp(t).encode_utf16().flat_map(u16::to_le_bytes));
        }
        bytes.extend(line_bytes(text));
        disk.append_line(name, &bytes)
    }

    /// Close a file: whether it was open.
    // D2-08570
    pub fn close(&mut self, path: &[u16]) -> bool {
        self.open.remove(path).is_some()
    }
}

const NATIVES: &[(u32, NativeFn)] = &[
    (0x01, write),
    (0x02, read_lines),
    (0x03, open),
    (0x04, close),
    (0x05, write_stamped),
];

/// The file natives, by id.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(PACKAGE, *n), *f))
        .chain(std::iter::once((
            NativeId::new(MAIL, 0x01),
            send_mail as NativeFn,
        )))
}

fn host<'a>(c: &'a mut Call<'_>) -> Result<&'a mut ClientHost, Stop> {
    c.host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))
}

/// A string argument (`fp[k]`).
fn text(c: &Call<'_>, k: i64) -> Result<Vec<u16>, Stop> {
    match c.vm.heap.native(c.cx.at(k)?)? {
        NativeObject::Text(t) => Ok(t.clone()),
        _ => Err(c.fail("an argument is not a string")),
    }
}

/// The file a script's path names, and the host's files through the bare-name check, for one
/// call.
fn with_disk<T>(
    c: &mut Call<'_>,
    path: &[u16],
    f: impl FnOnce(&mut Files, &mut dyn LocalSystem, &str, Option<LocalTime>) -> T,
    refused: T,
) -> Result<T, Stop> {
    let h = host(c)?;
    let Some(name) = file_name(&h.output_path, path) else {
        return Ok(refused);
    };
    let now = h.local.as_ref().and_then(|l| l.local_time());
    let mut disk = crate::host::Checked(&mut h.local);
    Ok(f(&mut h.files, &mut disk, &name, now))
}

/// Open the file at a path (`fp[-2]`), to read (`fp[-3]`) or else to add to it (`fp[-4]`) or to
/// write it anew; answers whether it opened (`fp[-5]`).
// D2-08577
fn open(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let read = c.cx.at(-3)? != 0;
    let append = c.cx.at(-4)? != 0;
    let ok = with_disk(
        c,
        &path,
        |files, disk, name, _| files.open(disk, &path, name, read, append),
        false,
    )?;
    c.write(-5, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// A file's lines (its path at `fp[-2]`) into the list the reference at `fp[-3]` reaches; whether the
/// file could be read (`fp[-4]`). A file is its 16-bit text after the mark, or plain bytes without
/// one.
// D2-08584
fn read_lines(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let bytes = {
        let h = host(c)?;
        let name = file_name(&h.output_path, &path).filter(|n| bare_name(n));
        name.and_then(|n| crate::host::Checked(&mut h.local).read_file(&n))
    };
    let out = c.cx.at(-3)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (list, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    let found = bytes.is_some();
    if let (Some(bytes), true) = (bytes, list != NULL) {
        let text = if bytes.starts_with(&MARK) {
            let (pairs, _) = bytes[MARK.len()..].as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().map(|b| u16::from_le_bytes(*b)).collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let mut items = Vec::new();
        for line in text.lines() {
            let t = c.vm.heap.add_native(
                native_type::WP_STRING,
                NativeObject::Text(line.encode_utf16().collect()),
            );
            items.push(Item::Ref(t));
        }
        if let NativeObject::Sequence { items: held, .. } = c.vm.heap.native_mut(list)? {
            *held = items;
        }
    }
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

/// A report for the original's mail (four strings: the address, the subject, the body and where to
/// answer why not, `fp[-2]` to `fp[-5]`): this client mails nothing and keeps it in a file of the
/// client's folder. It answers whether it was kept (`fp[-6]`).
// D2-08506
fn send_mail(c: &mut Call<'_>) -> Result<Status, Stop> {
    let lines: [Vec<u16>; 3] = [text(c, -2)?, text(c, -3)?, text(c, -4)?];
    let done = {
        let h = host(c)?;
        let mut bytes = Vec::new();
        for l in &lines {
            bytes.extend(line_bytes(l));
        }
        crate::host::Checked(&mut h.local).append_line(REPORTS, &bytes)
    };
    c.write(-6, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Write a line (`fp[-3]`) to the file at a path (`fp[-2]`); answers whether it was written
/// (`fp[-4]`).
// D2-08582
fn write(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let line = text(c, -3)?;
    let ok = with_disk(
        c,
        &path,
        |files, disk, name, _| files.write(disk, &path, name, &line),
        false,
    )?;
    c.write(-4, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Write a line (`fp[-3]`) after the local time to the file at a path (`fp[-2]`); answers whether
/// it was written (`fp[-4]`).
// D2-08583
fn write_stamped(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let line = text(c, -3)?;
    let ok = with_disk(
        c,
        &path,
        |files, disk, name, now| files.write_stamped(disk, &path, name, now.as_ref(), &line),
        false,
    )?;
    c.write(-4, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Close the file at a path (`fp[-2]`); answers whether it was open (`fp[-3]`).
// D2-08571
fn close(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let ok = host(c)?.files.close(&path);
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}
