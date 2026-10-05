//! Shared preference values, registration defaults, choice tables and interface metadata.
//!
//! Runtime configuration and host adapters read this store directly. The modern and Classic
//! option pages bind to the same values; their widgets, drafts and rendering stay in their
//! respective interface crates.

pub mod classic;
pub mod config;
pub mod interface;
pub mod landscape;
pub mod names;
pub mod performance;
pub mod preferences;
pub mod sheet;
pub mod store;

/// The title of the error box shown when opening a support URL fails (the shell's result is 32
/// or less). The Options page's support buttons raise the request; the host makes the call.
pub const SHELL_EXECUTE_ERROR_TITLE: &str = "Asheron's Call Error";
