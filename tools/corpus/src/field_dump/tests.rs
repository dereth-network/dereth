//! Behaviour: none (field-dump decoding and reporting contracts).

use super::*;

#[test]
fn wildcard_and_directional_bodies_keep_their_field_rendering() {
    for direction in [Direction::ClientToServer, Direction::ServerToClient] {
        let Decoded::Ok(text) = decode(direction, 0x0004, &[2, 0, b'h', b'i']) else {
            panic!("a popup decodes in either direction")
        };
        assert_eq!(text, "CommunicationPopUpString {\n    message: \"hi\",\n}");
    }
    let Decoded::Ok(request) = decode(Direction::ClientToServer, 0x00A3, &[1, 0, 0, 0]) else {
        panic!("quit request decodes")
    };
    let Decoded::Ok(notice) = decode(Direction::ServerToClient, 0x00A3, &[1, 0, 0, 0]) else {
        panic!("quit notice decodes")
    };
    assert!(
        request.starts_with("FellowshipQuitRequest {\n"),
        "{request}"
    );
    assert!(notice.starts_with("FellowshipQuitNotice {\n"), "{notice}");
}

#[test]
fn trailing_failed_unknown_and_zero_bodies_remain_separate_outcomes() {
    let direction = Direction::ServerToClient;
    assert!(matches!(decode(direction, 0, &[]), Decoded::AceZeroBlob));
    assert!(matches!(
        decode(direction, 0, &[0, 0]),
        Decoded::AceZeroBlob
    ));
    assert!(matches!(decode(direction, 0, &[1]), Decoded::NoDecoder));
    assert!(matches!(decode(direction, 0xDEAD, &[]), Decoded::NoDecoder));
    assert!(matches!(
        decode(direction, 0x0004, &[2]),
        Decoded::Failed(_)
    ));
    assert!(matches!(
        decode(direction, 0x0004, &[2, 0, b'h', b'i', 0, 0, 0]),
        Decoded::Ok(_)
    ));
    let Decoded::Trailing(text, count) = decode(direction, 0x0004, &[2, 0, b'h', b'i', 9, 8, 7, 6])
    else {
        panic!("a decoded prefix retains the extra byte count")
    };
    assert_eq!(count, 4);
    assert_eq!(text, "CommunicationPopUpString {\n    message: \"hi\",\n}");
}
