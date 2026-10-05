//! The option *data*: the preference value store, the choice tables and the two const tables the
//! store registers from.
//!
//! It lives here because the client's state modules need it: `dereth_client_runtime::config` reads
//! `store::display_choice`,
//! `dereth_client_shell::render_prefs` reads `store::enum_choices` and `store::set_value`, and
//! `dereth_desktop::platform::window` names `store::DisplayMode` — none of which is presentation.
//!
//! The split is data / panel. What is here is the value store and the tables it is built from;
//! what stays in `dereth_ui_screens::options` is the page — `PlayerOptionPage`, the rows, the
//! element wiring, the key-binding screen, and `init_ui_preferences`'s `UiPreferenceItem`
//! registry. Every item here is re-exported there, so
//! `dereth_ui_screens::options::store::display_choice` and its siblings resolve.

pub mod classic;
pub mod config;
pub mod interface;
pub mod landscape;
pub mod performance;
pub mod preferences;
pub mod sheet;
pub mod store;

/// The title of the error box shown when opening a support URL fails (the shell's result is 32
/// or less). The Options page's support buttons raise the request; the host makes the call.
pub const SHELL_EXECUTE_ERROR_TITLE: &str = "Asheron's Call Error";
