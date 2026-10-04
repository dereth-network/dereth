//! The host's text conversion and the chat log's file handle.
//!
//! `dereth-client-model` touches no platform: its chat text conversion (a `kernel32` NLS wrapper)
//! and the `std::fs::File` that `@log` writes to are reached through
//! [`dereth_primitives::HostEncoding`] and [`dereth_primitives::TextSink`], and this module is the
//! client's side of both.
//!
//! `HostAcp` wraps the `dereth_primitives::text` calls, and `ChatLog` performs the
//! `write_all`s for `Scroll::copy_final_to_log`.
//!
//! The Windows arm is installed from here, not made the shared crate's default, because only the
//! application switches on `dereth-primitives`' `host-nls` feature: a library below it sees the
//! pure table. `dereth_primitives::text` keeps its audited `extern "system"` block and its one
//! `allow(unsafe_code)` to itself; this crate, which `forbid`s unsafe, only names its safe
//! functions.

use std::sync::Arc;

use dereth_primitives::{HostEncoding, TextSink};

/// `WideCharToMultiByte(CP_ACP, 0, …)`, `MultiByteToWideChar(CP_ACP, 0, …)` and the English `%ws`
/// field, exactly as `dereth_primitives::text` audits them, behind the seam trait.
///
/// `CP_ACP` is `0` and `dereth_primitives::text::ChatConversion::default()` is that selector:
/// the live OS ANSI code page, read but never set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostAcp {
    /// The selector as given. `0` is `CP_ACP`; [`HostEncoding::acp`] resolves it.
    selector: u32,
    conversion: dereth_primitives::text::ChatConversion,
}

impl HostAcp {
    /// An explicit code page, for a test that wants a known NLS environment without changing any
    /// global or OS locale. `0` is the host's own ACP and is [`Default`].
    #[must_use]
    pub const fn for_ansi_code_page(code_page: u32) -> Self {
        Self {
            selector: code_page,
            conversion: dereth_primitives::text::ChatConversion::for_ansi_code_page(code_page),
        }
    }
}

impl HostEncoding for HostAcp {
    fn narrow(&self, utf16: &[u16]) -> Option<Vec<u8>> {
        // `Narrow::escaped` is the flag saying the whole-string `<%04x>` arm was taken. No caller
        // in the workspace reads it -- the bytes already say so -- so the seam carries the bytes.
        self.conversion
            .to_spstring(utf16)
            .ok()
            .map(|narrow| narrow.bytes)
    }

    fn widen(&self, bytes: &[u8]) -> Option<Vec<u16>> {
        self.conversion.to_wpstring(bytes).ok()
    }

    fn acp(&self) -> u32 {
        if self.selector == 0 {
            // `GetACP()`. Off Windows the table arm reports its own 1252.
            dereth_primitives::text::acp().unwrap_or(0)
        } else {
            self.selector
        }
    }

    fn english_ws_field(&self, utf16: &[u16]) -> Option<Vec<u8>> {
        // Fixed to the English locale's 1252, never the ACP: this is MSVCR70's `wctomb` inside
        // `char sprintf("%ws")` after client initialization calls `setlocale("English")`.
        dereth_primitives::text::english_ws_field(utf16).ok()
    }
}

/// Put the host's conversion on a freshly built `World`.
///
/// `dereth-client-model`'s own default is the workspace's 1252 table, because a crate with no platform
/// cannot read an ANSI code page. This call is what keeps the shipped client on the live `CP_ACP`
/// path, and it runs wherever the client builds a `World`:
/// `ObjectStream::new` and `ObjectStream::reset`.
pub fn install(world: &mut dereth_client_model::World) {
    let host = dereth_client_model::HostText::new(Arc::new(HostAcp::default()));
    world.chat.text_conversion = host.clone();
    world.scroll.encoding = host;
}

/// The client-system log `FILE*` opens here.
#[derive(Debug)]
pub struct ChatLog(std::fs::File);

impl TextSink for ChatLog {
    /// The log-file `fprintf("%ls%ls\n", …)` tail of adding a line to the chat scroll, for a line
    /// already composed, filtered and stamped by its producer.
    ///
    /// This repository's supported legacy text path is Windows-1252. Native uses the current ACP
    /// and its default replacement character; other system ACPs remain a stated tail. The explicit
    /// CRLF is what MSVCR70 text mode produces on Windows; Rust append mode otherwise writes a
    /// bare LF.
    fn write_line(&mut self, line: &str) {
        use std::io::Write as _;
        let legacy = dereth_protocol::cp1252::encode(line).unwrap_or_else(|| {
            line.chars()
                .flat_map(|c| {
                    dereth_protocol::cp1252::encode(&c.to_string()).unwrap_or_else(|| vec![b'?'])
                })
                .collect()
        });
        // A failed write is dropped: retail does not check `fprintf`'s return either, and a
        // full disk must not take the chat window down.
        let _ = self.0.write_all(&legacy);
        let _ = self.0.write_all(b"\r\n");
        let _ = self.0.flush();
    }
}

/// The chat-log copy's `fopen` in append/update mode. `None` is its failure, on
/// which the caller deliberately leaves logging stopped.
#[must_use]
pub fn open_chat_log(path: &std::path::Path) -> Option<Box<dyn TextSink>> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)
        .ok()?;
    Some(Box::new(ChatLog(file)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CP_ACP` is not a code page; it is the request for one. A seam that reported `0` would tell
    /// a consumer the bytes were in code page zero, which does not exist.
    #[test]
    fn the_default_selector_resolves_to_a_real_code_page() {
        assert_ne!(HostAcp::default().acp(), 0);
        assert_eq!(HostAcp::for_ansi_code_page(1252).acp(), 1252);
    }

    /// ASCII converts identically on every ANSI code page, so this holds whatever this machine's
    /// ACP is -- and it is the property `dereth_client_model::taboo` and `dereth_client_model::turbine` rest on.
    #[test]
    fn the_host_arm_round_trips_ascii() {
        let host = HostAcp::default();
        let units: Vec<u16> = (0x20u16..0x7F).collect();
        let bytes: Vec<u8> = (0x20u8..0x7F).collect();
        assert_eq!(host.narrow(&units).as_deref(), Some(&bytes[..]));
        assert_eq!(host.widen(&bytes).as_deref(), Some(&units[..]));
        assert_eq!(host.english_ws_field(&units).as_deref(), Some(&bytes[..]));
    }

    /// The sink writes what `Scroll::copy_final_to_log` used to write: 1252 bytes and CRLF.
    #[test]
    fn the_chat_log_writes_windows_1252_and_crlf() {
        let dir = std::env::temp_dir().join(format!("dereth-text-sink-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
        }
        std::fs::create_dir_all(&dir).expect("create disposable output directory");
        let path = dir.join("chat.txt");
        {
            let mut sink = open_chat_log(&path).expect("the disposable log opens");
            sink.write_line("9:05:03 caf\u{e9} \u{4e00}");
        }
        assert_eq!(
            std::fs::read(&path).expect("read the disposable log"),
            b"9:05:03 caf\xe9 ?\r\n",
            "a character with no 1252 byte is the per-character `?`, and the line ends CRLF"
        );
        std::fs::remove_dir_all(&dir).expect("remove disposable output directory");
    }
}
