//! The modal error box, shown by the operating system.
//!
//! The seam (`ErrorDialogHost`) and the headless host (`NoDialog`) are
//! `dereth_client_runtime::platform::dialog`'s and are re-exported here. `SystemDialog` is the
//! windowed build's host: the OS message box, through `dereth_console::error_box`, because this
//! crate forbids `unsafe` and the call needs it.

pub use dereth_client_runtime::platform::dialog::{ErrorDialogHost, NoDialog};

use dereth_client_runtime::connect_failure::ErrorPopup;

/// The operating system's own message box, with the popup's caption, text and style.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDialog;

impl ErrorDialogHost for SystemDialog {
    fn show_modal(&mut self, popup: &ErrorPopup) {
        dereth_console::error_box(&popup.caption, &popup.text, popup.style);
    }
}
