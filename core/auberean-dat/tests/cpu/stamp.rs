//! Data version stamps: their text and their packing.

use auberean_dat::{DatIdStamp, Reader, Writer};

/// A stamp is its id (a version-4 unique id when made new) and its minor version as text, read
/// back from text as the C library scans it (the minor kept modulo 2^32, read before the id is
/// checked), and packed as that text.
///
/// Behaviour: data.version-stamp.text-and-pack
#[test]
fn a_stamp_is_its_id_and_minor_as_text_and_packs_as_that_text() {
    let mut s = DatIdStamp::default();
    s.generate_new_major_version([0xAB; 16]);
    s.generate_new_minor_version();
    s.generate_new_minor_version();
    let text = s.to_text();
    assert_eq!(text, "abababab-abab-4bab-abab-abababababab : 2");
    let mut back = DatIdStamp::default();
    assert!(back.from_text("  ABABABAB-ABAB-4BAB-ABAB-ABABABABABAB : 2"));
    assert_eq!(back, s);
    assert!(
        !back.from_text("ABABABAB-ABAB-4BAB-ABAB-ABABABABABAB:2"),
        "the colon is part of the word"
    );
    assert!(back.from_text("abababab-abab-4bab-abab-abababababab : -1"));
    assert_eq!(back.minor, u32::MAX);
    assert!(back.from_text("abababab-abab-4bab-abab-abababababab : 99999999999"));
    assert_eq!(back.minor, 0x4876_E7FF, "kept modulo 2^32");
    assert!(!back.from_text("not-an-id : 3"), "a malformed id");
    assert_eq!(back.minor, 3, "the minor version is read before the id is");
    assert!(!back.from_text("abababab-abab-4bab-abab-abababababab 3"));
    let mut w = Writer::new();
    s.write(&mut w);
    let bytes = w.finish();
    assert_eq!(bytes.len(), s.pack_size());
    let mut r = Reader::new(&bytes);
    let mut read = DatIdStamp::default();
    assert!(read.read(&mut r).expect("reads"));
    assert_eq!(read, s);
    read.clear();
    assert_eq!(read, DatIdStamp::default());
}
