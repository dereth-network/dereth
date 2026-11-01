//! The native interface: how script calls engine code, and how a run ends.
//!
//! A native is engine code the bytecode calls by id. The machine opens a frame for it exactly as
//! for a script function (`fp[-1]` is the receiver, `fp[-2]`, `fp[-3]`... the arguments, the
//! caller's result slot below them), calls it, and closes the frame unless the native left the
//! context suspended. A native writes its result into the result slot with a tag, and returns a
//! [`Status`].
//!
//! Natives are found in a [`NativeTable`] by id when the library is loaded. An id the table does
//! not know fails the load; an id the table knows but whose native is not written yet
//! ([`Binding::NotPorted`]) stops a run cleanly when it is called, naming the native, so what is
//! missing is counted rather than guessed at.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::context::Context;
use crate::graph::SavedState;
use crate::heap::HeapError;
use crate::value::{split, tag, NativeId};
use crate::vm::Vm;

/// How a handler or a native left the context, as the client numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Status {
    /// The context's call is over.
    Terminated = 1,
    /// Go on with the next instruction.
    Success = 2,
    /// A native left the context suspended, to be resumed later.
    Interrupted = 3,
    /// The script failed: the context is discarded.
    Crash = 4,
    /// A package initializer had no function to run.
    NoFunction = 5,
}

/// Why this machine stopped a run that the client would have gone on with, or would have
/// failed in undefined ways.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// A native that is not written yet: its id and the retail function it stands for.
    NotPorted {
        native: u32,
        port: &'static str,
    },
    /// The run used its step budget.
    Budget,
    StackOverflow,
    StackUnderflow,
    CallStackOverflow,
    CallStackUnderflow,
    /// The code at a position is not an instruction the machine has (or the position is not in
    /// the code).
    BadCode {
        at: usize,
        word: u32,
    },
    /// A call whose dispatch tables have no entry for it.
    BadDispatch(String),
    /// An integer division by zero (the client faults).
    DivideByZero,
    /// The script used the null reference where an object was needed (the client reads
    /// outside its table there and fails).
    NullReference,
    /// A reference the table cannot follow.
    Heap(HeapError),
    /// A saved state that would not read.
    BadState {
        state: u32,
        what: String,
    },
    /// A value that could not be packed or read.
    Codec(String),
    /// A native given what it cannot work with: the native and what.
    Native {
        native: u32,
        what: String,
    },
    /// The host refused something.
    Host(String),
}

/// What stopped a run, as the machine's record of stops ([`crate::vm::Vm::stops`]) keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stopped {
    /// A native not written yet, by id.
    NotPorted(u32),
    /// The null reference used as an object.
    NullReference,
    /// Another stop, by its description.
    Other(String),
}

impl Stop {
    /// What stopped the run, for the record of stops.
    #[must_use]
    pub fn stopped(&self) -> Stopped {
        match self {
            Self::NotPorted { native, .. } => Stopped::NotPorted(*native),
            Self::NullReference => Stopped::NullReference,
            e => Stopped::Other(e.to_string()),
        }
    }
}

impl From<HeapError> for Stop {
    fn from(e: HeapError) -> Self {
        match e {
            HeapError::BadReference(crate::value::NULL) => Self::NullReference,
            e => Self::Heap(e),
        }
    }
}

impl core::fmt::Display for Stop {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotPorted { native, port } => {
                write!(f, "native {} is not ported ({port})", NativeId(*native))
            }
            Self::Budget => f.write_str("the step budget ran out"),
            Self::StackOverflow => f.write_str("the stack overflowed"),
            Self::StackUnderflow => f.write_str("the stack underflowed"),
            Self::CallStackOverflow => f.write_str("the call stack overflowed"),
            Self::CallStackUnderflow => f.write_str("the call stack underflowed"),
            Self::BadCode { at, word } => write!(f, "no instruction {word:#010x} at word {at}"),
            Self::BadDispatch(s) => write!(f, "dispatch: {s}"),
            Self::DivideByZero => f.write_str("an integer division by zero"),
            Self::NullReference => f.write_str("the null reference was used as an object"),
            Self::Heap(e) => write!(f, "{e}"),
            Self::BadState { state, what } => write!(f, "saved state {state:#x}: {what}"),
            Self::Codec(s) => write!(f, "packing: {s}"),
            Self::Native { native, what } => write!(f, "native {}: {what}", NativeId(*native)),
            Self::Host(s) => write!(f, "host: {s}"),
        }
    }
}

impl std::error::Error for Stop {}

/// What a native is given: the machine, the calling context (its frame is the native's), and the
/// host.
pub struct Call<'a> {
    pub vm: &'a mut Vm,
    pub cx: &'a mut Context,
    pub host: &'a mut dyn Host,
    /// The native's id.
    pub native: u32,
}

impl core::fmt::Debug for Call<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Call")
            .field("native", &NativeId(self.native))
            .finish_non_exhaustive()
    }
}

impl Call<'_> {
    /// The receiver, `fp[-1]`.
    pub fn receiver(&self) -> Result<u32, Stop> {
        self.cx.at(-1)
    }

    /// Argument `n` (0 is the first), `fp[-2 - n]`.
    pub fn arg(&self, n: i64) -> Result<u32, Stop> {
        self.cx.at(-2 - n)
    }

    /// A 64-bit argument whose low word is at slot `fp[k]` (`k` negative).
    pub fn long_at(&self, k: i64) -> Result<u64, Stop> {
        Ok(crate::value::join(self.cx.at(k)?, self.cx.at(k + 1)?))
    }

    /// Write slot `fp[k]` with a tag (a result slot, `k` negative).
    pub fn write(&mut self, k: i64, v: u32, t: u8) -> Result<(), Stop> {
        self.cx.set(k, v, t)
    }

    /// Write a 64-bit value at `fp[k]` and `fp[k + 1]`, both with a tag.
    pub fn write_long(&mut self, k: i64, v: u64, t: u8) -> Result<(), Stop> {
        let (lo, hi) = split(v);
        self.cx.set(k, lo, t)?;
        self.cx.set(k + 1, hi, t)
    }

    /// Write slot `fp[k]` tagged as written by a native.
    pub fn write_mixed(&mut self, k: i64, v: u32) -> Result<(), Stop> {
        self.cx.set(k, v, tag::MIXED)
    }

    /// A failure of this native.
    #[must_use]
    pub fn fail(&self, what: impl Into<String>) -> Stop {
        Stop::Native {
            native: self.native,
            what: what.into(),
        }
    }
}

/// A native written in Rust.
pub type NativeFn = fn(&mut Call<'_>) -> Result<Status, Stop>;

/// What a native id is bound to.
#[derive(Clone, Copy)]
pub enum Binding {
    Ported(NativeFn),
    /// Not written yet: the retail function it stands for.
    NotPorted(&'static str),
}

impl core::fmt::Debug for Binding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Ported(_) => f.write_str("Ported"),
            Self::NotPorted(p) => write!(f, "NotPorted({p})"),
        }
    }
}

/// The natives a machine can call, by id.
#[derive(Clone, Debug, Default)]
pub struct NativeTable {
    map: BTreeMap<u32, Binding>,
}

impl NativeTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind (or rebind) an id.
    pub fn bind(&mut self, id: NativeId, b: Binding) {
        self.map.insert(id.0, b);
    }

    /// What an id is bound to.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<Binding> {
        self.map.get(&id).copied()
    }

    /// Every binding, by id.
    pub fn iter(&self) -> impl Iterator<Item = (u32, Binding)> + '_ {
        self.map.iter().map(|(k, v)| (*k, *v))
    }

    /// How many ids are bound.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// What the machine needs from outside: the saved states, and the engine side of the natives
/// that reach beyond the script world. Everything has a default that refuses, so a host only
/// implements what its natives use.
pub trait Host {
    /// A saved script state by id, if the data has one.
    fn saved_state(&mut self, id: u32) -> Option<Arc<SavedState>>;

    /// A script call for the other side of the connection: the function, its packed arguments,
    /// and whether it goes through the encrypted carrier.
    fn send(&mut self, function: u32, args: &[u8], encrypted: bool) -> Result<(), Stop> {
        let _ = (function, args, encrypted);
        Err(Stop::Host("this host sends nothing".into()))
    }

    /// The game's clock as scripts read it, in seconds.
    fn time(&mut self) -> f64 {
        0.0
    }

    /// Whether a string table (by record id) holds a string (by token): false when there is no
    /// such table.
    fn string_exists(&mut self, table: u32, string: u32) -> Result<bool, Stop> {
        let _ = (table, string);
        Err(Stop::Host("this host has no string tables".into()))
    }

    /// A line the scripts print for the developer (the client opens a console window for it); a
    /// host without a console lets it go.
    fn print(&mut self, line: &str) {
        let _ = line;
    }

    /// The next number of the process's random number generator, in `(0, 1)`. One generator
    /// serves the whole process, scripts and everything else, so the host keeps it.
    fn random(&mut self) -> Result<f64, Stop> {
        Err(Stop::Host("this host draws no random numbers".into()))
    }

    /// The record id an enumeration value maps to in an enumeration mapping (0 for none).
    fn data_id_by_enum(&mut self, value: u32, mapping: u32) -> u32 {
        let _ = (value, mapping);
        0
    }

    /// The record id of a named record of a type (0 for none).
    fn data_id_by_name(&mut self, name: &str, record_type: u32) -> u32 {
        let _ = (name, record_type);
        0
    }

    /// Where string references find their tables and the language (none: a reference that names
    /// a table writes as one whose table is missing).
    fn text(&mut self) -> Option<&mut dyn auberean_text::Source> {
        None
    }

    /// The records of the client's data files, decoded once and kept (none: a native that reads a
    /// record finds none).
    fn database(&mut self) -> Option<&auberean_assets::Database> {
        None
    }

    /// A script wrote a string reference (as it was before the writing) to this text.
    fn wrote_text(&mut self, info: &crate::native_object::TextInfo, text: &[u16]) {
        let _ = (info, text);
    }

    /// The host itself, for natives that know the concrete host they run under.
    fn as_any(&mut self) -> &mut dyn Any;
}
