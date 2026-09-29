//! Live diagnostics, on the `dereth::trace::*` log targets (`--log info,dereth::trace=debug`).
//!
//! **Diagnostic instruments, not client features.** The filter checks themselves -- [`raise`],
//! [`notice`] and [`hex`] -- live in `dereth_client_runtime::trace` and are re-exported here so
//! that `dereth_client::trace::notice()` and its siblings resolve from this crate too. The `+10`
//! watch line reads the live UI tree and the HUD's panels; it runs inside the gameplay screen's
//! drive, so it lives beside it and is re-exported here.

pub use dereth_client_runtime::trace::{hex, notice, raise};

pub use dereth_ui_screens::screens::gameplay_host::plus_ten_lines;
