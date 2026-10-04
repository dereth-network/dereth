#![recursion_limit = "1024"]

//! DAT-tier scenarios for `dereth-testkit`: each one opens the retail dats under `$DERETH_TEST_DAT_DIR`
//! and builds a whole headless client over them.
//!
//! **This binary must run serially** (`-- --test-threads=1`). Two headless clients in one process
//! share `dereth_ui_screens`' request globals, which is the same constraint `dereth-headless`'s
//! own `dat` binary records.
//!
//! One module per registry subject, as in the `cpu` binary; `census` concatenates each file's own
//! `ALL` and checks it against the registry's `dat`-tier rows. `goldens` diffs two runs' frame
//! logs against the committed golden files.

mod census;
mod chat;
mod combat;
mod frame;
mod goldens;
mod inventory;
mod login;
mod magic;
mod net;
mod objects;
mod panels;
mod shell;
mod social;
mod ui;
mod world;
