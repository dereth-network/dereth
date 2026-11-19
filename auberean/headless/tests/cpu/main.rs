//! Tests that need no private data: the client through a hand-written game login played by a
//! server double, on a virtual clock and an in-memory network; its scripts and their runs; and its
//! plugin lines.

mod builtin;
mod double;
mod plugins;
#[path = "../replay/mod.rs"]
#[allow(dead_code)]
mod replay;
mod run;
mod script;
mod world_entry;
