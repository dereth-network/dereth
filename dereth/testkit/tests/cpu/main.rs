//! CPU-tier scenarios for `dereth-testkit`: no retail dats, no graphics device, no window.
//!
//! One test binary per crate per tier. The recorded captures under `fixtures/packet-captures` are
//! committed to the repository and are not a retail data file, so some scenarios here replay one;
//! nothing here opens `$DERETH_TEST_DAT_DIR`.
//!
//! This binary may run its tests in parallel. The `dat` binary may not.
//!
//! One module per registry subject: a scenario lives in the file of the subject its behaviour row
//! is in, so a reader looking for what the client claims about the chat window finds it in one
//! place. `census` checks that every `cpu`-tier row is declared by exactly one scenario here.

mod census;
mod chat;
mod combat;
mod frame;
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
