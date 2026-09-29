//! The clipboard, as the client sees it: text it copies waits here for the page to put on the
//! system clipboard, and text the page pastes is put here with a new sequence number, which is
//! how the client's once-a-frame refresh notices it. A worker cannot reach the system clipboard
//! itself.

use std::cell::RefCell;

use dereth_client_shell::clipboard::{ClipboardError, HostClipboard};

/// The page's clipboard, as the shell's bridge reaches it.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageClipboard;

impl HostClipboard for PageClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        set_text(text);
        Ok(())
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(get_text())
    }
    fn sequence_number(&mut self) -> Option<u32> {
        sequence_number()
    }
}

#[derive(Default)]
struct Board {
    text: Option<String>,
    seq: u32,
    outgoing: Vec<String>,
}

thread_local! {
    static BOARD: RefCell<Board> = RefCell::new(Board::default());
}

/// Put `text` on the clipboard: the client reads it back, and the page is handed it.
pub fn set_text(text: &str) {
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        b.text = Some(text.to_string());
        b.seq += 1;
        b.outgoing.push(text.to_string());
    });
}

/// The clipboard's text, if it holds any.
#[must_use]
pub fn get_text() -> Option<String> {
    BOARD.with(|b| b.borrow().text.clone())
}

/// A number that changes whenever the clipboard does.
#[must_use]
pub fn sequence_number() -> Option<u32> {
    Some(BOARD.with(|b| b.borrow().seq))
}

/// Text the page pasted, which becomes the clipboard's.
pub fn paste_from_page(text: &str) {
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        b.text = Some(text.to_string());
        b.seq += 1;
    });
}

/// Text the client copied since the last call, for the page to put on the system clipboard.
#[must_use]
pub fn take_copied() -> Vec<String> {
    BOARD.with(|b| std::mem::take(&mut b.borrow_mut().outgoing))
}
