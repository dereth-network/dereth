//! Script-call arguments as they travel between client and server: a buffer of tagged values.
//!
//! ```text
//! { u32 tag, value }*   to the end of the buffer
//!   1 an integer        u32            pushed tagged as an integer
//!   2 a long integer    u64            two slots, tagged as a long integer
//!   3 a float           f32            tagged as a float
//!   4 a long float      f64            two slots, tagged as a long float (time values too)
//!   5 an object         a packed object graph, pushed as a reference to its root
//! ```
//!
//! Any other tag, or a buffer not consumed exactly, refuses the call. The receiver pushes the
//! values in buffer order between the return slot and the receiver, so the buffer holds the
//! called function's parameters **last first**, and every sender packs them in that order.

use crate::graph::Graph;
use crate::library::Library;
use crate::wire::{In, Out, Result};

/// The value tags of an argument buffer.
pub mod pack_tag {
    pub const INT: u32 = 1;
    pub const LONG_INT: u32 = 2;
    pub const FLOAT: u32 = 3;
    pub const LONG_FLOAT: u32 = 4;
    pub const OBJECT: u32 = 5;
}

/// One argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Int(u32),
    LongInt(u64),
    /// A float, as its bits.
    Float(u32),
    /// A long float, as its bits.
    LongFloat(u64),
    Object(Graph),
}

impl Arg {
    /// The tag this argument travels with.
    #[must_use]
    pub fn tag(&self) -> u32 {
        match self {
            Self::Int(_) => pack_tag::INT,
            Self::LongInt(_) => pack_tag::LONG_INT,
            Self::Float(_) => pack_tag::FLOAT,
            Self::LongFloat(_) => pack_tag::LONG_FLOAT,
            Self::Object(_) => pack_tag::OBJECT,
        }
    }
}

/// Read an argument buffer: every value, every byte.
// D2-16752, D2-16747, D2-16748, D2-16749, D2-16750, D2-16751
pub fn decode(bytes: &[u8], lib: &Library) -> Result<Vec<Arg>> {
    let mut r = In::new(bytes);
    let mut out = Vec::new();
    while !r.at_end() {
        let t = r.u32()?;
        out.push(match t {
            pack_tag::INT => Arg::Int(r.u32()?),
            pack_tag::LONG_INT => Arg::LongInt(r.u64()?),
            pack_tag::FLOAT => Arg::Float(r.u32()?),
            pack_tag::LONG_FLOAT => Arg::LongFloat(r.u64()?),
            pack_tag::OBJECT => Arg::Object(Graph::read(&mut r, lib)?),
            _ => return r.fail(format!("an argument of tag {t}")),
        });
    }
    Ok(out)
}

/// Write arguments in the order given.
pub fn encode(args: &[Arg]) -> core::result::Result<Vec<u8>, String> {
    let mut w = Out::new();
    for a in args {
        w.u32(a.tag());
        match a {
            Arg::Int(v) | Arg::Float(v) => w.u32(*v),
            Arg::LongInt(v) | Arg::LongFloat(v) => w.u64(*v),
            Arg::Object(g) => g.write(&mut w)?,
        }
    }
    Ok(w.buf)
}

/// The tag a parameter of a declared type travels as: integers and enumerations as integers,
/// floats as floats, a 64-bit one of either as its long form, time values as long floats,
/// strings and packages as objects. `kind` and `flags` are a frame member's type and flags.
#[must_use]
pub fn tag_for(kind: u32, flags: u32) -> Option<u32> {
    let long = flags & 2 != 0;
    match kind {
        2 | 8 => Some(if long {
            pack_tag::LONG_INT
        } else {
            pack_tag::INT
        }),
        3 => Some(if long {
            pack_tag::LONG_FLOAT
        } else {
            pack_tag::FLOAT
        }),
        5 => Some(pack_tag::LONG_FLOAT),
        7 | 9 => Some(pack_tag::OBJECT),
        _ => None,
    }
}

/// The tags a function's parameters travel as, in buffer order (last parameter first), from the
/// library's frame of the function. `skip_first` leaves out the first parameter (a sending stub's
/// interface, which is not packed). `None` when the library has no frame for the function or a
/// parameter has a type that cannot travel.
#[must_use]
pub fn expected_tags(lib: &Library, function: &str, skip_first: bool) -> Option<Vec<u32>> {
    let frame = lib.frame(function)?;
    let mut params: Vec<_> = frame.members.iter().filter(|m| m.slot < -1).collect();
    params.sort_by_key(|m| core::cmp::Reverse(m.slot));
    if skip_first {
        if params.is_empty() {
            return None;
        }
        params.remove(0);
    }
    // The buffer holds the last parameter (the deepest slot) first.
    params.sort_by_key(|m| m.slot);
    params.iter().map(|m| tag_for(m.kind, m.flags)).collect()
}
