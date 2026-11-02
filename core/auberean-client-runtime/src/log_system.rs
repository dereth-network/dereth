//! The scripts' log system: two masks of the kinds of log line the client keeps, one for its log
//! file and one for its console, which the scripts read and change.
//!
//! The file's mask starts at `0x1A4` and the console's at 0. A kind is logged when every bit of it
//! is in the mask; kind 4 always goes to the file. Setting a mask replaces it, adding sets bits,
//! removing clears them. The masks belong to the client, not to a character: they stay through
//! entering and leaving the world.

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::value::{tag, NativeId};

use crate::host::host;

/// The kinds of log line the client keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogMasks {
    pub file: u32,
    pub print: u32,
}

impl Default for LogMasks {
    fn default() -> Self {
        Self {
            file: 0x1A4,
            print: 0,
        }
    }
}

impl LogMasks {
    /// Whether lines of a kind go to the file: kind 4 always, else when the mask holds all of it.
    // D2-08110
    #[must_use]
    pub fn file_logging(&self, kind: u32) -> bool {
        kind == 4 || self.file & kind == kind
    }

    /// Whether lines of a kind go to the console: when the mask holds all of it.
    // D2-08111
    #[must_use]
    pub fn print_logging(&self, kind: u32) -> bool {
        self.print & kind == kind
    }
}

/// Which mask a native changes.
#[derive(Clone, Copy)]
enum Mask {
    File,
    Print,
}

fn mask(m: &mut LogMasks, which: Mask) -> &mut u32 {
    match which {
        Mask::File => &mut m.file,
        Mask::Print => &mut m.print,
    }
}

/// Change a mask by the word at `fp[-2]`.
fn change(c: &mut Call<'_>, which: Mask, f: fn(u32, u32) -> u32) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    let m = mask(&mut host(c)?.logs, which);
    *m = f(*m, v);
    Ok(Status::Success)
}

// D2-09039, D2-08112
fn set_file_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, Mask::File, |_, v| v)
}

// D2-09040, D2-08113
fn set_print_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, Mask::Print, |_, v| v)
}

// D2-09035, D2-08114
fn add_file_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, Mask::File, |m, v| m | v)
}

// D2-09036, D2-08115
fn add_print_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, Mask::Print, |m, v| m | v)
}

// D2-09037, D2-08116
fn remove_file_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, Mask::File, |m, v| m & !v)
}

// D2-09038, D2-08117
fn remove_print_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    change(c, Mask::Print, |m, v| m & !v)
}

/// A mask into `fp[-2]`.
fn get(c: &mut Call<'_>, which: Mask) -> Result<Status, Stop> {
    let v = *mask(&mut host(c)?.logs, which);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-09043, D2-08118
fn get_file_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, Mask::File)
}

// D2-09044, D2-08119
fn get_print_log(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, Mask::Print)
}

/// Whether a kind (`fp[-2]`) is logged, into `fp[-3]`.
fn ask(c: &mut Call<'_>, f: fn(&LogMasks, u32) -> bool) -> Result<Status, Stop> {
    let kind = c.cx.at(-2)?;
    let v = f(&host(c)?.logs, kind);
    c.write(-3, u32::from(v), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-09045
fn is_file_logging(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask(c, LogMasks::file_logging)
}

// D2-09046
fn is_print_logging(c: &mut Call<'_>) -> Result<Status, Stop> {
    ask(c, LogMasks::print_logging)
}

/// The log system's natives.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    const LOG_SYSTEM: u32 = 0x75;
    let n: [(u32, NativeFn); 10] = [
        (0x01, is_file_logging),
        (0x02, is_print_logging),
        (0x03, add_file_log),
        (0x04, add_print_log),
        (0x05, set_file_log),
        (0x06, set_print_log),
        (0x07, remove_file_log),
        (0x08, remove_print_log),
        (0x09, get_file_log),
        (0x0A, get_print_log),
    ];
    n.into_iter()
        .map(|(number, f)| (NativeId::new(LOG_SYSTEM, number), f))
}
