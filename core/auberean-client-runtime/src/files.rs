//! The files scripts write: a chat log, an allegiance's export. The client keeps them here, by
//! path, for a front end to save or show; it never touches a disk itself.
//!
//! Files are text in 16-bit units, little-endian, after a byte-order mark (`FF FE`):
//!
//! - **opening** a file to write starts it anew, the mark alone; opening it to add to it keeps
//!   what it holds, unless it holds something that does not start with the mark (then it does not
//!   open, answering 0); an empty or new one gets the mark; opening one to read needs it to be
//!   there. An open file is open under its path until it is closed; opening answers 1;
//! - **writing** a line to a file that is not open opens it to add to it first (answering 0 when
//!   that fails); each line feed in the text is written as a carriage return and a line feed, and
//!   a text that does not end in a line feed gets a carriage return and a line feed after it;
//!   writing answers 1, and a file open to read takes nothing;
//! - **closing** a file answers whether it was open.

use std::collections::BTreeMap;

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::NativeObject;
use auberean_script::value::{tag, NativeId};

use crate::host::ClientHost;

/// The file natives' package.
const PACKAGE: u32 = 0x26;

/// The byte-order mark a file starts with.
const MARK: [u8; 2] = [0xFF, 0xFE];

/// How a file is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Open {
    Read,
    Write,
}

/// The files scripts wrote, by path (as the scripts spelled it), and which are open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Files {
    pub contents: BTreeMap<Vec<u16>, Vec<u8>>,
    pub open: BTreeMap<Vec<u16>, Open>,
}

impl Files {
    /// Open a file: to read, to write it anew, or to add to it. Whether it opened.
    // D2-08575
    pub fn open(&mut self, path: &[u16], read: bool, append: bool) -> bool {
        let mode = if read {
            if !self.contents.contains_key(path) {
                return false;
            }
            Open::Read
        } else if append {
            let held = self.contents.entry(path.to_vec()).or_default();
            if held.is_empty() {
                held.extend_from_slice(&MARK);
            } else if !held.starts_with(&MARK) {
                return false;
            }
            Open::Write
        } else {
            self.contents.insert(path.to_vec(), MARK.to_vec());
            Open::Write
        };
        self.open.insert(path.to_vec(), mode);
        true
    }

    /// Write a line, opening the file to add to it when it is not open. Whether it was written
    /// (a file open to read answers yes and takes nothing).
    // D2-08581, D2-08579
    pub fn write(&mut self, path: &[u16], text: &[u16]) -> bool {
        if !self.open.contains_key(path) && !self.open(path, false, true) {
            return false;
        }
        if self.open.get(path) != Some(&Open::Write) {
            return true;
        }
        let file = self.contents.entry(path.to_vec()).or_default();
        for &u in text {
            if u == u16::from(b'\n') {
                file.extend_from_slice(&u16::from(b'\r').to_le_bytes());
            }
            file.extend_from_slice(&u.to_le_bytes());
        }
        if text.last() != Some(&u16::from(b'\n')) {
            file.extend_from_slice(&[b'\r', 0, b'\n', 0]);
        }
        true
    }

    /// Close a file: whether it was open.
    // D2-08570
    pub fn close(&mut self, path: &[u16]) -> bool {
        self.open.remove(path).is_some()
    }
}

const NATIVES: &[(u32, NativeFn)] = &[(0x01, write), (0x03, open), (0x04, close)];

/// The file natives, by id.
pub fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(PACKAGE, *n), *f))
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

/// Open the file at a path (`fp[-2]`), to read (`fp[-3]`) or else to add to it (`fp[-4]`) or to
/// write it anew; answers whether it opened (`fp[-5]`).
// D2-08577
fn open(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let read = c.cx.at(-3)? != 0;
    let append = c.cx.at(-4)? != 0;
    let ok = host(c)?.files.open(&path, read, append);
    c.write(-5, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Write a line (`fp[-3]`) to the file at a path (`fp[-2]`); answers whether it was written
/// (`fp[-4]`).
// D2-08582
fn write(c: &mut Call<'_>) -> Result<Status, Stop> {
    let path = text(c, -2)?;
    let line = text(c, -3)?;
    let ok = host(c)?.files.write(&path, &line);
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
