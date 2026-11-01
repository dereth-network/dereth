//! Strings on the wire.

use auberean_protocol::text::NarrowString;
use auberean_protocol::wire::{Field, Reader, Writer};

/// The client version every January 2017 login carried is `1802`, sent as four scrambled bytes
/// after its length.
/// Behaviour: protocol.string.scrambled-on-the-wire
#[test]
fn the_client_version_travels_scrambled() {
    let mut w = Writer::default();
    NarrowString::from("1802").write(&mut w);
    assert_eq!(w.finish(), vec![4, 0, 0x2B, 0x11, 0x87, 0xC6, 0, 0]);
}

/// Behaviour: protocol.string.scrambled-on-the-wire
#[test]
fn a_string_pads_to_four_from_the_message_start_not_its_own() {
    // After a 2-byte prefix the 4-character string's padding brings the message to 12 bytes.
    let mut w = Writer::default();
    w.u16(7);
    NarrowString::from("abcd").write(&mut w);
    let bytes = w.finish();
    assert_eq!(
        bytes.len(),
        8,
        "no padding: the string ends on a four-byte boundary"
    );
    let mut r = Reader::new(&bytes);
    assert_eq!(r.u16().expect("prefix"), 7);
    assert_eq!(NarrowString::read(&mut r).expect("string").text(), "abcd");
    r.finish().expect("every byte used");
}
