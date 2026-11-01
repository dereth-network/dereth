//! The files scripts write, kept by the client for a front end.

use auberean_client_runtime::files::{Files, Open};

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn bytes(s: &str) -> Vec<u8> {
    let mut b = vec![0xFF, 0xFE];
    b.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    b
}

/// A file starts with its byte-order mark; each line feed is written after a carriage return, and
/// a line that does not end in one gets both; writing to a file that is not open opens it to add
/// to it; opening to add keeps what a file holds unless it does not start with the mark; opening
/// to read needs the file; closing answers whether it was open.
///
/// Behaviour: script.natives.files-scripts-write
#[test]
fn a_file_is_marked_and_its_lines_end_in_a_carriage_return_and_a_line_feed() {
    let mut f = Files::default();
    let log = units("chat.txt");
    assert!(!f.open(&log, true, false), "nothing to read yet");
    assert!(f.write(&log, &units("hello")));
    assert!(f.write(&log, &units("a\nb\n")));
    assert_eq!(f.contents[&log], bytes("hello\r\na\r\nb\r\n"));
    assert!(f.close(&log));
    assert!(!f.close(&log), "closed already");
    // Adding to it keeps what it holds; writing it anew starts again.
    assert!(f.open(&log, false, true));
    assert!(f.write(&log, &units("")));
    assert_eq!(f.contents[&log], bytes("hello\r\na\r\nb\r\n\r\n"));
    assert!(f.open(&log, false, false));
    assert_eq!(f.contents[&log], bytes(""));
    // Open to read, it takes nothing written to it.
    assert!(f.open(&log, true, false));
    assert_eq!(f.open[&log], Open::Read);
    assert!(f.write(&log, &units("x")));
    assert_eq!(f.contents[&log], bytes(""));
    // A file that does not start with the mark is not added to.
    let other = units("other.txt");
    f.contents.insert(other.clone(), b"ab".to_vec());
    assert!(!f.open(&other, false, true));
    assert!(!f.write(&other, &units("x")));
    assert_eq!(f.contents[&other], b"ab");
}
