//! The one `dereth-ui-screens::ctime` item the contract names.
//!
//! `GameView::utc_offset_secs` returns it, so it has to be reachable from a crate that may not
//! depend on `dereth-ui-screens`. The two formatters (`strftime_c`, `asctime`) stay where they are:
//! they are host-side text, not contract. `dereth_ui_screens::ctime::UtcOffsetSecs` is a `pub use`
//! of this alias, so every existing path still resolves.

/// The shift from UTC that `localtime` would apply, in seconds, for a **particular instant**.
///
/// Named so that the many call sites read the same way, and so that a grep for the concept finds
/// one type rather than a bare `i32`.
pub type UtcOffsetSecs = i32;
