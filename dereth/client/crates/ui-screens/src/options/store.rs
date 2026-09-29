//! `UserPreferences` — the value store an option control reads and writes.
//!
//! Defined in [`dereth_client_contract::options::store`]: `dereth_client::config`,
//! `dereth_client::render_prefs` and `dereth_client::platform::window` all read it, and none of
//! them may name a presentation crate. Nothing in it is presentation — it is a registry of named
//! values and the choice tables that label them — and this module re-exports it.
pub use dereth_client_contract::options::store::*;
