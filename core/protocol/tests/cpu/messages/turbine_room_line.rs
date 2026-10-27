//! A request to send a line to a chat room: the line, and the blob its sender attaches about the
//! speaker, which the room service passes on without reading. Every blob length reads, and the
//! request is written back byte for byte.
//! Fixture: synthetic request packets built byte by byte.

use dereth_protocol::turbine::{ExtraBlob, SendToRoomById};
use dereth_protocol::{read_body, write_body};

fn word(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&u32::try_from(value).expect("fits a word").to_le_bytes());
}

/// A request body as a sending client writes it: the packet's length, the packet's seven header
/// words and its payload's length, then the context, the two method words, the room, the counted
/// wide text and the counted blob.
fn request(context: u32, room: u32, text: &str, extra: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    for w in [context, 2, 2, room] {
        payload.extend_from_slice(&w.to_le_bytes());
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    // A count below 128 is a single byte.
    payload.push(u8::try_from(units.len()).expect("a short line"));
    for u in units {
        payload.extend_from_slice(&u.to_le_bytes());
    }
    word(&mut payload, extra.len());
    payload.extend_from_slice(extra);
    let mut packet = Vec::new();
    for w in [3_u32, 2, 1, 0, 0, 0, 0] {
        packet.extend_from_slice(&w.to_le_bytes());
    }
    word(&mut packet, payload.len());
    packet.extend(payload);
    let mut body = Vec::new();
    word(&mut body, packet.len());
    body.extend(packet);
    body
}

/// Behaviour: chat.turbine.a-room-line-is-read-whatever-note-its-sender-attaches
#[test]
fn a_room_line_is_read_and_written_back_whatever_blob_its_sender_attached() {
    // This client's own twelve bytes: the speaker, a zero word, the chat type.
    let own = [0x17, 0, 0, 0x50, 0, 0, 0, 0, 2, 0, 0, 0];
    let bytes = request(4, 7, "hello", &own);
    let m: SendToRoomById = read_body(&bytes).expect("this client's own form reads");
    assert_eq!(
        m,
        SendToRoomById {
            context: 4,
            room: 7,
            text: "hello".to_owned(),
            extra: ExtraBlob::Speaker {
                sender: 0x5000_0017,
                chat_type: 2,
            },
        }
    );
    assert_eq!(write_body(&m).expect("encodes"), bytes);

    // A longer blob, as another client of the same service attaches: twelve bytes shaped like
    // this client's own, then four more. It is kept whole, not read as a speaker with a tail.
    let longer = [
        1, 0, 0, 0, 0, 0, 0, 0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
    ];
    // And blobs of other lengths, including a twelve-byte one whose middle word is not zero.
    let middle_set = [1, 0, 0, 0, 9, 0, 0, 0, 2, 0, 0, 0];
    let others: [&[u8]; 5] = [&longer, &middle_set, &[], &[0xAA; 4], &[0x55; 20]];
    for extra in others {
        let bytes = request(9, 1, "Anyone here?", extra);
        let m: SendToRoomById =
            read_body(&bytes).unwrap_or_else(|e| panic!("a {}-byte blob reads: {e}", extra.len()));
        assert_eq!(m.extra, ExtraBlob::Other(extra.to_vec()));
        assert_eq!((m.context, m.room, m.text.as_str()), (9, 1, "Anyone here?"));
        assert_eq!(
            write_body(&m).expect("encodes"),
            bytes,
            "a {}-byte blob is written back as it came",
            extra.len()
        );
    }

    // A blob that promises more bytes than the request holds is refused, not half-read.
    let bytes = request(9, 1, "cut", &longer);
    let payload_at = 4 + 7 * 4 + 4;
    let blob_size_at = payload_at + 16 + 1 + 3 * 2;
    let mut overlong = bytes.clone();
    overlong[blob_size_at] += 4;
    assert!(read_body::<SendToRoomById>(&overlong).is_err());
}
