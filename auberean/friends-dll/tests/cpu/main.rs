//! The friends library with no world but its tests' own: its state against a world in memory on a
//! clock the tests turn, each result and event as the client reads it, the identifiers, the library
//! on real sockets against a world on loopback, and, on 32-bit Windows, the built library loaded and
//! called as the retail client loads and calls it.

mod common;
mod engine;
mod host;
mod ids;
mod layout;
#[cfg(all(windows, target_arch = "x86"))]
mod library;
