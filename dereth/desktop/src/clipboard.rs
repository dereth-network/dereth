//! The clipboard: the client shell's bridge ([`dereth_client_shell::clipboard`], re-exported
//! here) over the system clipboard, which [`dereth_clipboard`] reaches -- the crate this workspace
//! confines the clipboard's unsafe to. **This file contains no unsafe code.**

pub use dereth_client_shell::clipboard::*;

/// The real one. Every method is a straight forward to [`dereth_clipboard`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClipboard;

fn host_error(e: dereth_clipboard::Error) -> ClipboardError {
    ClipboardError(format!("{e:?}"))
}

impl HostClipboard for SystemClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        dereth_clipboard::set_text(text).map_err(host_error)
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        dereth_clipboard::get_text().map_err(host_error)
    }
    fn sequence_number(&mut self) -> Option<u32> {
        dereth_clipboard::sequence_number()
    }
}
