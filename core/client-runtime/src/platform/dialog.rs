//! The modal error box seam: the one OS dialog the client shows before it has any screen.
//!
//! `ErrorDialogHost::show_modal` puts up [`crate::connect_failure::ErrorPopup`] and returns
//! once the player has closed it. The real host is the operating system's own message box and
//! lives with the other platform code in `dereth_client`; `NoDialog` is the headless one, which
//! shows nothing and returns at once, so a test never leaves a box on the desktop.

use crate::connect_failure::ErrorPopup;

/// Something that can show a modal error box and wait for it to be closed.
pub trait ErrorDialogHost {
    /// Show `popup` and return when the player has dismissed it.
    fn show_modal(&mut self, popup: &ErrorPopup);
}

/// The headless host: nothing is shown and the call returns at once.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoDialog;

impl ErrorDialogHost for NoDialog {
    fn show_modal(&mut self, _popup: &ErrorPopup) {}
}
