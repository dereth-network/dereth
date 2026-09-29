//! CPU-tier tests for `dereth-clipboard`: no retail dats, no GPU device.
//!
//! `format` covers the clipboard format and payload a write asks for; `roundtrip` writes the real
//! system clipboard and is ignored by default (`cargo test -p dereth-clipboard --test cpu
//! roundtrip:: -- --ignored --test-threads=1`).

mod format;
mod roundtrip;
