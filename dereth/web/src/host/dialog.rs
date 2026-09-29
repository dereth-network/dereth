//! The modal error box. A page has no modal box a worker can raise, so the popup is a log line
//! the page shows.

pub use dereth_client_runtime::platform::dialog::{ErrorDialogHost, NoDialog};

use dereth_client_runtime::connect_failure::ErrorPopup;

/// The popup's caption and text, as an error in the log.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDialog;

impl ErrorDialogHost for SystemDialog {
    fn show_modal(&mut self, popup: &ErrorPopup) {
        tracing::error!("{}: {}", popup.caption, popup.text);
    }
}
