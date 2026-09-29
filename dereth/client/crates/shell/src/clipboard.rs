//! The client half of copying text to the host clipboard and reading it back.
//!
//! `dereth-ui` implements selection, `Copy`, `Cut` and `Paste` but has nowhere to send the text:
//! it has no window, and the workspace forbids `unsafe_code`. Without this module `Copy` only
//! fills a `String` inside the process, and nothing outside the process can ever see it.
//!
//! This module is the wire between [`dereth_ui::UiSystem`] and the host's clipboard, which the
//! host supplies ([`crate::platform::host::Host::clipboard`]): on the desktop the crate the workspace
//! confines the clipboard's unsafe to, in a browser the page's. **This file contains no unsafe
//! code.**
//!
//! # Why a trait
//!
//! [`HostClipboard`] exists so that the routing — *does a `Copy` reach `set_text`, does a bare
//! Ctrl+C leave the clipboard alone, is the same text sent twice* — is asserted by an ordinary
//! headless test against [`FakeClipboard`], while the desktop's implementation is the thin shell
//! that only forwards. The Win32 call itself is not exercised by any test that runs in a normal
//! sweep; see `dereth/client/crates/clipboard/tests/cpu/roundtrip.rs`.
//!
//! # The refresh direction
//!
//! The original client reads the clipboard *on the keystroke*, inside `Paste`. This build cannot:
//! paste handling runs inside the text element's action callback, deep in an arena borrow, and cannot reach the
//! host. So [`UiSystem::clipboard`] is kept mirrored instead, refreshed here once a frame — but
//! only when `GetClipboardSequenceNumber` says something changed, because opening the real
//! clipboard sixty times a second would take a desktop-wide lock away from every other application
//! on the machine.

use dereth_ui::UiSystem;

/// Why the host's clipboard refused, in the host's own words. Its `Debug` is those words alone, so
/// the warning a failure logs reads as the host's error would.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardError(pub String);

impl std::fmt::Debug for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The host's clipboard, behind a seam so the routing can be tested without a window station.
pub trait HostClipboard {
    /// Send text as `CF_UNICODETEXT`, preserving its UTF-16 representation.
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError>;
    /// `Ok(None)` when no `CF_UNICODETEXT` is on offer.
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError>;
    /// `GetClipboardSequenceNumber`, or `None` where there is no clipboard to poll.
    fn sequence_number(&mut self) -> Option<u32>;
}

/// No clipboard at all: nothing is sent anywhere and the sequence number is `None`, so the mirror
/// is never refreshed. What a host with no clipboard supplies.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoClipboard;

impl HostClipboard for NoClipboard {
    fn set_text(&mut self, _text: &str) -> Result<(), ClipboardError> {
        Ok(())
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(None)
    }
    fn sequence_number(&mut self) -> Option<u32> {
        None
    }
}

/// What the client remembers between frames so that it neither re-sends nor re-reads needlessly.
#[derive(Debug, Default)]
pub struct ClipboardBridge {
    /// The sequence number as of the last refresh. `None` means "never looked", which is what makes
    /// the first frame pick up whatever the player copied before the client started — the same
    /// answer retail gives, since it reads the live clipboard on every `Paste`.
    last_seq: Option<u32>,
    /// Diagnostics: how many `SetClipboardData` calls this session made, and how many failed.
    pub sends: u32,
    pub send_failures: u32,
    /// How many times the mirror was refreshed from the host.
    pub refreshes: u32,
}

impl ClipboardBridge {
    /// One frame of both directions.
    ///
    /// **Send first, then refresh.** A `Copy` this frame bumps the sequence number, so doing it in
    /// this order means the refresh below sees our own write and agrees with it, rather than the
    /// mirror and the clipboard disagreeing for one frame.
    pub fn sync(&mut self, ui: &mut UiSystem, host: &mut impl HostClipboard) {
        // ---- out: `Copy`/`Cut` -> the host clipboard ----------------
        //
        // A **take**, so the same text is not re-sent every frame. `None` leaves the host
        // clipboard untouched.
        if let Some(text) = ui.take_pending_clipboard() {
            self.sends += 1;
            if let Err(e) = host.set_text(&text) {
                // Never log the text: it is the player's, and on the read path it is whatever
                // they last copied anywhere on the machine.
                self.send_failures += 1;
                tracing::warn!("clipboard write failed: {e:?}");
            }
        }

        // ---- in: the host -> the mirror `Paste` reads ---------------------------
        let seq = host.sequence_number();
        if seq != self.last_seq {
            self.last_seq = seq;
            self.refreshes += 1;
            match host.get_text() {
                // `Ok(None)` is `IsClipboardFormatAvailable` saying no `CF_UNICODETEXT`. The mirror
                // is left alone rather than cleared: an image on the clipboard is not a reason to
                // throw away text this client copied itself.
                Ok(None) => {}
                Ok(Some(text)) => ui.clipboard = text,
                Err(e) => tracing::warn!("clipboard read failed: {e:?}"),
            }
        }
    }
}

/// An in-memory stand-in, so the routing above can be asserted without a window station.
///
/// Deliberately in the library and not in a test file: the clipboard tests and any later
/// caller need the same one, and it also documents what the trait's contract is.
#[derive(Debug, Default)]
pub struct FakeClipboard {
    /// What `set_text` was handed, in order. The assertion target.
    pub sent: Vec<String>,
    /// What `get_text` will answer.
    pub contents: Option<String>,
    /// What `sequence_number` will answer.
    pub seq: Option<u32>,
    /// How many times the host was actually read.
    pub reads: u32,
}

impl HostClipboard for FakeClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.sent.push(text.to_string());
        self.contents = Some(text.to_string());
        // A real `SetClipboardData` bumps the sequence number; the fake must too, or a test would
        // never see the write-then-refresh ordering the real one has.
        self.seq = Some(self.seq.unwrap_or(0) + 1);
        Ok(())
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        self.reads += 1;
        Ok(self.contents.clone())
    }
    fn sequence_number(&mut self) -> Option<u32> {
        self.seq
    }
}
