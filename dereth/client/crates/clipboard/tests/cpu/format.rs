//! The clipboard format a copy asks for and the bytes it puts behind it. The selector is branchless
//! arithmetic on the stored OS platform id, `((os_version != 2 ? -1 : 0) & 0xFFFFFFF4) + 0x0D`, so
//! NT asks for `CF_UNICODETEXT` (13) and everything else for `CF_TEXT` (1). Getting it wrong is
//! silent: `CF_TEXT` with a UTF-16 buffer behind it pastes the first character and nothing more, so
//! both arms are asserted against the constants rather than against each other. Pure functions
//! only; no clipboard is touched.

use dereth_clipboard::{clipboard_format, decode_units, unicode_payload, CF_TEXT, CF_UNICODETEXT};

/// Behaviour: ui.clipboard.copies-as-unicode-text-with-one-terminator
///
/// **NT asks for `CF_UNICODETEXT`; everything else asks for `CF_TEXT`.**
///
/// The stored OS version is a `GetVersionEx` platform id, so the values that reach this are 0
/// (Win32s), 1 (9x) and 2 (NT). Only 2 takes the Unicode leg.
#[test]
fn the_branchless_selector_yields_13_on_nt_and_1_everywhere_else() {
    assert_eq!(clipboard_format(2), CF_UNICODETEXT, "0 + 0xd");
    assert_eq!(
        clipboard_format(2),
        13,
        "and 13 is CF_UNICODETEXT, not some other 13"
    );

    assert_eq!(clipboard_format(1), CF_TEXT, "0xfffffff4 + 0xd, wrapping");
    assert_eq!(clipboard_format(0), CF_TEXT);
    assert_eq!(
        clipboard_format(3),
        CF_TEXT,
        "the test is `!= 2`, not `< 2`"
    );
    assert_eq!(clipboard_format(1), 1);
}

/// Behaviour: ui.clipboard.copies-as-unicode-text-with-one-terminator
///
/// **The payload is the code units verbatim, plus the terminator the allocation flag supplied.**
///
/// `GlobalAlloc(0x42, len * 2 + 2)` zero-fills; the copy loop writes exactly `len`
/// code units and never writes a terminator. So the block is always one unit longer than the text.
#[test]
fn the_payload_is_utf16_with_one_trailing_nul() {
    assert_eq!(unicode_payload("abc"), vec![0x61, 0x62, 0x63, 0]);
    assert_eq!(
        unicode_payload(""),
        vec![0],
        "an empty copy is still a terminated block"
    );
}

/// **Non-ASCII is handed over verbatim** — the whole reason NT takes the Unicode leg.
///
/// The ANSI leg (one byte per character) would truncate each of these to one byte and
/// paste mojibake. A surrogate pair must survive as two units, not be pre-composed or replaced.
#[test]
fn non_ascii_and_astral_text_survive_as_utf16() {
    // U+00E9 LATIN SMALL LETTER E WITH ACUTE — one unit; the ANSI leg would make it 0xE9 as a byte.
    assert_eq!(unicode_payload("\u{e9}"), vec![0x00E9, 0]);
    // U+7532 — one unit whose low byte alone (0x32, '2') is what CF_TEXT would have pasted.
    assert_eq!(unicode_payload("\u{7532}"), vec![0x7532, 0]);
    // U+1F600 — a surrogate pair, two units.
    assert_eq!(unicode_payload("\u{1f600}"), vec![0xD83D, 0xDE00, 0]);
}

/// **Reading back trims at the first NUL and not at the end of the block.**
///
/// `GlobalSize` counts the whole allocation, which for retail's own writes includes the terminator
/// and may include slack the allocator rounded up to. Decoding the raw count would append NULs to
/// every pasted string.
#[test]
fn decoding_stops_at_the_terminator() {
    assert_eq!(decode_units(&[0x61, 0x62, 0]).unwrap(), "ab");
    assert_eq!(
        decode_units(&[0x61, 0x62, 0, 0, 0]).unwrap(),
        "ab",
        "slack past the NUL is not text"
    );
    assert_eq!(decode_units(&[0]).unwrap(), "");
    assert_eq!(decode_units(&[]).unwrap(), "");
    assert_eq!(decode_units(&[0xD83D, 0xDE00, 0]).unwrap(), "\u{1f600}");
}

/// A round trip through the two pure halves, which is everything except the Win32 call itself.
#[test]
fn payload_and_decode_are_inverse() {
    for s in [
        "",
        "a",
        "hello world",
        "\u{e9}\u{7532}",
        "\u{1f600}ok",
        "tab\there",
    ] {
        assert_eq!(
            decode_units(&unicode_payload(s)).unwrap(),
            s,
            "round trip of {s:?}"
        );
    }
}

/// **An unpaired surrogate is rejected rather than silently replaced.**
#[test]
fn a_lone_surrogate_is_an_error() {
    assert!(
        decode_units(&[0xD83D, 0]).is_err(),
        "a high surrogate with no low half"
    );
}
