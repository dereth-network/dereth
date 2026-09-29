//! Text written to the real Windows clipboard reads back whole: ASCII, non-ASCII and astral text,
//! an empty string and a long one; a write moves the sequence number and leaves the clipboard
//! unlocked.
//!
//! Every test is ignored by default because the clipboard is desktop-global: running them replaces
//! whatever the person at the keyboard had copied. Run them on purpose, one at a time, since the
//! clipboard is one process-wide lock:
//!
//! ```text
//! cargo test -p dereth-clipboard --test cpu roundtrip:: -- --ignored --test-threads=1
//! ```
//!
//! Each test writes a string it generated and reads back the same string. Nothing reads, prints or
//! preserves what was on the clipboard before, so the previous contents are lost.
//!
//! Behaviour: none (the operating system's clipboard round trip, ignored by default)

use dereth_clipboard::{get_text, sequence_number, set_text};

/// A string this test made up, so that reading it back proves the round trip and not a coincidence.
fn unique(tag: &str) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("dereth-clipboard-{tag}-{n}")
}

/// **Text handed to `set_text` is on the real clipboard.**
///
/// This is the assertion no headless test can make. If it passes, `Copy` in the chat window puts
/// text where Notepad and every browser can see it.
#[test]
#[ignore = "writes the real system clipboard; run with --ignored --test-threads=1"]
fn text_set_can_be_read_back_from_the_real_clipboard() {
    let s = unique("ascii");
    set_text(&s).expect("SetClipboardData");
    assert_eq!(get_text().expect("GetClipboardData"), Some(s));
}

/// **`CF_UNICODETEXT` and not `CF_TEXT`.** The format choice is the one thing in
/// the clipboard write that is easy to get wrong and silent when wrong.
///
/// With `CF_TEXT` and a UTF-16 buffer behind it, this string would come back as its first
/// character or as mojibake; with a correct `CF_TEXT` implementation it would come back with every
/// one of these characters replaced by `?`. Only the Unicode leg returns them intact.
#[test]
#[ignore = "writes the real system clipboard; run with --ignored --test-threads=1"]
fn non_ascii_survives_the_real_round_trip() {
    let s = format!("{}-\u{e9}\u{7532}\u{1f600}", unique("wide"));
    set_text(&s).expect("SetClipboardData");
    assert_eq!(get_text().expect("GetClipboardData"), Some(s));
}

/// **An empty copy is still a valid clipboard write** — `GlobalAlloc(0x42, 2)`, one NUL.
#[test]
#[ignore = "writes the real system clipboard; run with --ignored --test-threads=1"]
fn an_empty_string_round_trips() {
    set_text("").expect("SetClipboardData");
    assert_eq!(get_text().expect("GetClipboardData"), Some(String::new()));
}

/// **A long string does not truncate**, which is what a `len` vs `len * 2` mistake in the
/// allocation would produce — and it would only show past the first few characters.
#[test]
#[ignore = "writes the real system clipboard; run with --ignored --test-threads=1"]
fn a_long_string_round_trips_whole() {
    let s = unique("long").repeat(400);
    assert!(
        s.len() > 10_000,
        "big enough to cross any plausible buffer boundary"
    );
    set_text(&s).expect("SetClipboardData");
    let back = get_text().expect("GetClipboardData").expect("some text");
    assert_eq!(back.len(), s.len(), "no truncation");
    assert_eq!(back, s);
}

/// **`SetClipboardData` bumps the sequence number**, which is what the client's per-frame refresh
/// gate depends on. If this were constant the mirror would never notice an external copy.
#[test]
#[ignore = "writes the real system clipboard; run with --ignored --test-threads=1"]
fn a_write_moves_the_sequence_number() {
    let before = sequence_number().expect("windows");
    set_text(&unique("seq")).expect("SetClipboardData");
    let after = sequence_number().expect("windows");
    assert_ne!(before, after, "GetClipboardSequenceNumber moved");
}

/// **The clipboard is left closed.** If `CloseClipboard` were missed, this second write would fail
/// and every other application on the desktop would be locked out until the process exited.
#[test]
#[ignore = "writes the real system clipboard; run with --ignored --test-threads=1"]
fn the_clipboard_lock_is_released_between_calls() {
    for i in 0..5 {
        let s = unique(&format!("lock{i}"));
        set_text(&s)
            .unwrap_or_else(|e| panic!("write {i} failed, lock probably still held: {e:?}"));
        assert_eq!(get_text().expect("read"), Some(s));
    }
}
