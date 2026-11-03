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
//! - **reading** a file that is not open opens it to read first (answering 0 when that fails), and
//!   gives its lines. It reads as narrow text in the Windows Latin code page, not as the 16-bit
//!   units files are written in: a carriage return before a line feed is dropped, a Ctrl+Z ends the
//!   text, a line longer than 4095 characters goes on as a next one, and a line stops at a zero, so
//!   a file a script wrote reads as its mark and first letter. Reading uses the file up: its name
//!   stays open, but it gives nothing more until it is closed, and a file open to write gives
//!   nothing;
//! - **closing** a file answers whether it was open.

use std::collections::BTreeMap;

use auberean_script::local::{LocalSystem, LocalTime};
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
    /// Read or asked to be read: nothing more comes of it.
    UsedUp,
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
/// What the Windows Latin code page holds at 0x80 to 0x9F; the rest of its bytes are themselves.
const LATIN_80: [u16; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
];

/// The most characters one read of a line gives.
const LINE_READ: usize = 4095;

/// A file's bytes as lines: a Ctrl+Z ends them, a carriage return before a line feed goes, a read
/// stops after a line feed or at its limit, and each line stops at its first zero or line feed.
#[must_use]
pub fn text_lines(bytes: &[u8]) -> Vec<Vec<u16>> {
    let mut chars = Vec::with_capacity(bytes.len());
    let mut at = bytes.iter().copied().peekable();
    while let Some(b) = at.next() {
        match b {
            0x1A => break,
            b'\r' if at.peek() == Some(&b'\n') => {}
            0x80..=0x9F => chars.push(LATIN_80[usize::from(b - 0x80)]),
            _ => chars.push(u16::from(b)),
        }
    }
    let mut lines = Vec::new();
    let mut rest = chars.as_slice();
    while !rest.is_empty() {
        let read = rest
            .iter()
            .take(LINE_READ)
            .position(|&u| u == u16::from(b'\n'))
            .map_or(rest.len().min(LINE_READ), |k| k + 1);
        let (one, more) = rest.split_at(read);
        let end = one
            .iter()
            .position(|&u| u == 0 || u == u16::from(b'\n'))
            .unwrap_or(one.len());
        lines.push(one[..end].to_vec());
        rest = more;
    }
    lines
}

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

    /// Read a file's lines, opening it to read when it is not open: none when that fails.
    // D2-08576
    pub fn read(
        &mut self,
        disk: &mut dyn LocalSystem,
        path: &[u16],
        name: &str,
    ) -> Option<Vec<Vec<u16>>> {
        if !self.open.contains_key(path) && !self.open(disk, path, name, true, false) {
            return None;
        }
        let was = self.open.insert(path.to_vec(), Open::UsedUp);
        if was != Some(Open::Read) {
            return Some(Vec::new());
        }
        Some(text_lines(&disk.read_file(name).unwrap_or_default()))
    }

    /// Close a file: whether it was open.
    // D2-08570
    pub fn close(&mut self, path: &[u16]) -> bool {
        self.open.remove(path).is_some()
    }
}

const NATIVES: &[(u32, NativeFn)] = &[
    (0x01, write),
    (0x02, read),
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

/// Read the file at a path (`fp[-2]`) into the array an out reference (`fp[-3]`) holds, whatever it
/// held gone, a new string a line; answers whether it read (`fp[-4]`).
// D2-08584, D2-08572, D2-08578
fn read(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let out = c.cx.at(-3)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (array, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if array == NULL {
        return Ok(Status::Crash);
    }
    let lines = with_disk(
        c,
        &path,
        |files, disk, name, _| files.read(disk, &path, name),
        None,
    )?;
    let ok = lines.is_some();
    let items: Vec<Item> = lines
        .unwrap_or_default()
        .into_iter()
        .map(|line| {
            Item::Ref(
                c.vm.heap
                    .add_native(native_type::WP_STRING, NativeObject::Text(line)),
            )
        })
        .collect();
    match c.vm.heap.native_mut(array)? {
        NativeObject::Sequence { items: held, .. } => *held = items,
        _ => return Err(c.fail("the out reference is not to an array")),
    }
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
