//! The stream's bytes against the vectors: the preamble, frames of a little-endian length and one
//! element, and the frames that close a stream.

use auberean_friends::frame::{write_frame, MAX_FRAME};
use auberean_friends::{FrameError, FrameReader, PREAMBLE};

use crate::common::{list, text, unhex};

/// Behaviour: friends.stream.frames
#[test]
fn a_stream_is_the_preamble_then_frames_of_a_length_and_one_element() {
    for v in list("frames") {
        let bytes = unhex(text(v, "hex"));
        let mut body = &bytes[..];
        if v["preamble"].as_bool() == Some(true) {
            assert_eq!(&bytes[..4], &PREAMBLE, "{}", text(v, "about"));
            body = &bytes[4..];
        }
        let want: Vec<&str> = v["frames"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        // Read whole, and a byte at a time.
        let mut whole = FrameReader::new();
        whole.push(body);
        let mut slow = FrameReader::new();
        let mut got = Vec::new();
        let mut got_slowly = Vec::new();
        while let Some(f) = whole.next_frame().expect("good frames") {
            got.push(f);
        }
        for b in body {
            slow.push(std::slice::from_ref(b));
            while let Some(f) = slow.next_frame().expect("good frames") {
                got_slowly.push(f);
            }
        }
        assert_eq!(got, want, "{}", text(v, "about"));
        assert_eq!(got_slowly, want, "{}", text(v, "about"));
        let mut written = Vec::new();
        for f in &want {
            write_frame(f, &mut written).expect("fits");
        }
        assert_eq!(written, body, "{}", text(v, "about"));
    }
}

/// Behaviour: friends.stream.frames
#[test]
fn a_frame_of_no_bytes_past_the_bound_or_not_utf8_is_refused() {
    for v in list("bad_frames") {
        let mut r = FrameReader::new();
        r.push(&unhex(text(v, "hex")));
        assert!(r.next_frame().is_err(), "{}", text(v, "about"));
    }
    let mut out = Vec::new();
    assert_eq!(write_frame("", &mut out), Err(FrameError::Empty));
    let longest = "x".repeat(MAX_FRAME);
    assert_eq!(write_frame(&longest, &mut out), Ok(()));
    assert_eq!(out.len(), 4 + MAX_FRAME);
    let mut r = FrameReader::new();
    r.push(&out);
    assert_eq!(
        r.next_frame().expect("the longest frame").map(|f| f.len()),
        Some(MAX_FRAME)
    );
    let too_long = "x".repeat(MAX_FRAME + 1);
    let mut out = Vec::new();
    assert_eq!(
        write_frame(&too_long, &mut out),
        Err(FrameError::TooLong(MAX_FRAME + 1))
    );
    assert!(out.is_empty());
}
