//! The script machine: the compiled script library, its load pass, the packed object graph codec,
//! the interpreter, the reference table and the native interface.
//!
//! **Depends on** `auberean-assets` (the script library record, read into its sections),
//! `auberean-dat` (the record reader), `auberean-primitives` (the string scramble),
//! `auberean-text` (the text source a host offers its natives) and `auberean-port` (the marker of
//! what is not ported yet). **Used by** the native functions
//! (`auberean-glue`), the client runtime (`auberean-client-runtime`) and the server
//! (`yalain-world`).
//!
//! Most of the game's logic is script: packages (classes) of functions compiled to 32-bit words.
//! The client and the server each run the same small stack machine over the same library and talk
//! to each other mostly by calling script functions on the other side.
//!
//! - [`library`]: the compiled library as data: its code, literals, packages and dispatch tables.
//! - [`opcode`]: the instruction set and how long each instruction is.
//! - [`link`]: the one load pass: every native call bound to a native by id, every call that
//!   names its function outright turned into a direct call. A native the table does not know
//!   fails the load.
//! - [`wire`], [`native_object`], [`graph`], [`args`]: the packed object graph, one codec shared
//!   by saved states, script-call arguments on the network and object copying, with the packed
//!   form of every native object it meets; [`multi_hash`], the engine's multi-hash table, and
//!   [`multi_map`], its ordered multi-map;
//!   [`options`], the player's options and shortcuts as the engine holds them,
//!   [`options_file`], their text form for exporting and importing them, and [`ui_locations`],
//!   the windows' saved places among them.
//! - [`heap`]: the reference table: script objects, native objects, references into a slot,
//!   singletons filled from saved states on first use, mark-and-sweep collection.
//! - [`context`], [`interp`]: a context's slot stack and call stack, and the handler table the
//!   instruction words run through.
//! - [`vm`]: the machine: processes, contexts, calls into script from outside.
//! - [`native`]: the native interface: what a native sees, the statuses and stops, the table of
//!   natives by id.
//! - [`bridge`]: script calls that arrive from, or leave for, the other side of the connection.
//! - [`local`]: the wall clock and the files in the client's own folder, as a host gives them.
//!
//! **Must never** do I/O, read a clock or draw a random number: saved states arrive through
//! [`native::Host`], time and randomness through the natives the host implements, and every run
//! has a step budget, so a runaway script stops instead of hanging.

pub mod args;
pub mod bridge;
pub mod context;
pub mod graph;
pub mod heap;
pub mod interp;
pub mod library;
pub mod link;
pub mod local;
pub mod multi_hash;
pub mod multi_map;
pub mod native;
pub mod native_object;
pub mod opcode;
pub mod options;
pub mod options_file;
pub mod states;
pub mod story;
pub mod ui_locations;
pub mod value;
pub mod vm;
pub mod wire;

pub use library::Library;
pub use link::Program;
pub use native::{Binding, Call, Host, NativeFn, NativeTable, Status, Stop};
pub use vm::Vm;
