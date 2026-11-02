//! Rooms through the chat service, in memory: when the chat service's client starts, the lines
//! it sends and refuses, the answers handed back with their lines, and the rooms' lines heard.

use auberean_client_runtime::friends::weenie_result;
use auberean_client_runtime::rooms::{Heard, RoomChat};

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// The service's packet: its header (the kind, a 1, the login's word), the payload's length, the
/// payload.
fn packet(kind: [u32; 2], body: &[u8]) -> Vec<u8> {
    let word = 0x00F1_0001;
    let mut p: Vec<u8> = [kind[0], kind[1], 1, word, 1, word, 0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    p.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    p.extend_from_slice(body);
    p
}

/// The service's answer to the line with mark `context`, with its code.
fn answer(context: u32, code: u32) -> Vec<u8> {
    let body: Vec<u8> = [context, 2, 2, code]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    packet([5, 2], &body)
}

/// A room's line: the room, the speaker's name, the words, the note its client attached.
fn line(room: u32, name: &str, text: &str, note: &[u8]) -> Vec<u8> {
    let mut b = room.to_le_bytes().to_vec();
    for t in [name, text] {
        let u = units(t);
        b.push(u8::try_from(u.len()).unwrap());
        for x in u {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    b.extend_from_slice(&u32::try_from(note.len()).unwrap().to_le_bytes());
    b.extend_from_slice(note);
    packet([1, 1], &b)
}

/// The mark, room and text of a packet the client sends.
fn sent(p: &[u8]) -> (u32, u32, Vec<u16>) {
    let w = |at: usize| u32::from_le_bytes(p[at..at + 4].try_into().unwrap());
    assert_eq!(
        (w(0), w(4), w(8), w(12), w(16), w(20), w(24)),
        (3, 2, 1, 0, 0, 0, 0)
    );
    let (context, room) = (w(32), w(44));
    assert_eq!((w(36), w(40)), (2, 2), "a line to a room by its id");
    let n = usize::from(p[48]);
    let text = p[49..49 + 2 * n]
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    (context, room, text)
}

/// A world whose character list names a chat service starts the chat service's client once: the
/// lines go to it, their marks counted from 1 and running on through a second character list.
/// Without a list naming one nothing starts: a line is refused (0) with nothing sent, whether
/// rooms would go to the friends service or to a chat service never started.
///
/// Behaviour: friends.rooms.the-character-list-starts-the-chat-service-once-and-its-marks-run-on
#[test]
fn the_character_list_starts_the_chat_service_once_and_its_marks_run_on() {
    let mut r = RoomChat::default();
    let me = units("Vacuos [M]");
    r.character_set(0);
    assert!(!r.started() && !r.uses_chat_service());
    assert_eq!(r.send_to_room(1, &units("hi"), 0, &me, true), 0);
    assert_eq!(
        r.send_to_room(1, &units("hi"), 7, &me, false),
        7,
        "without a friends service the flags come back"
    );
    r.use_chat_service();
    assert_eq!(
        r.send_to_room(1, &units("hi"), 0, &me, false),
        0,
        "its client never started"
    );
    assert!(r.take_packets().is_empty());

    r.character_set(1);
    assert!(r.started());
    assert_eq!(r.send_to_room(1, &units("one"), 0, &me, false), 1);
    r.character_set(1);
    assert_eq!(r.send_to_room(8, &units("two"), 0, &me, false), 1);
    let marks: Vec<(u32, u32, Vec<u16>)> = r.take_packets().iter().map(|p| sent(p)).collect();
    assert_eq!(
        marks,
        [(1, 1, units("one")), (2, 8, units("two"))],
        "counted from 1, on through the second list"
    );
    assert_eq!(r.waiting(), [1, 2]);
}

/// An empty line, or one to room 0, is refused at once (0): nothing goes out and nothing waits.
///
/// Behaviour: friends.rooms.an-empty-line-or-room-0-is-refused-at-once
#[test]
fn an_empty_line_or_room_0_is_refused_at_once() {
    let mut r = RoomChat::default();
    r.character_set(1);
    let me = units("Vacuos [M]");
    assert_eq!(r.send_to_room(1, &[], 0, &me, false), 0);
    assert_eq!(r.send_to_room(0, &units("hi"), 0, &me, false), 0);
    assert!(r.take_packets().is_empty());
    assert!(r.waiting().is_empty());
}

/// The service's answer comes back with the room and text of the line it answers, its code
/// turned into the client's own result (0 for done; the not-logged-in code the scripts' own
/// line); a second answer to the same mark, or one to a mark never sent, is dropped.
///
/// Behaviour: friends.rooms.an-answer-comes-back-with-its-lines-room-and-text
#[test]
fn an_answer_comes_back_with_its_lines_room_and_text() {
    let mut r = RoomChat::default();
    r.character_set(1);
    let me = units("Vacuos [M]");
    r.send_to_room(1, &units("one"), 0, &me, false);
    r.send_to_room(44, &units("two"), 0, &me, false);
    assert_eq!(
        r.receive(&answer(2, 0xC112_0011)),
        Some(Heard::Result {
            result: weenie_result(0xC112_0011),
            room: 44,
            text: units("two"),
        })
    );
    assert_eq!(weenie_result(0xC112_0011), 0xC73B);
    assert_eq!(r.receive(&answer(2, 0)), None, "answered once");
    assert_eq!(r.receive(&answer(9, 0)), None, "never sent");
    assert_eq!(
        r.receive(&answer(1, 0)),
        Some(Heard::Result {
            result: 0,
            room: 1,
            text: units("one"),
        })
    );
    assert!(r.waiting().is_empty());
}

/// A room's line heard names its speaker with the meta letters the speaker's client attached in
/// brackets after the name, and carries that client's chat flags; a note that does not read
/// leaves the bare name and no flags. Before the chat service's client started nothing is heard.
///
/// Behaviour: friends.rooms.a-rooms-line-heard-carries-the-speakers-meta-letters-and-flags
#[test]
fn a_rooms_line_heard_carries_the_speakers_meta_letters_and_flags() {
    // The two notes the 2017 sessions carried: `M` (12 bytes) and `Mv` (16), version 1.
    let m = [1, 0, 0, 0, 5, 0, 0, 0, 1, 0, 0xD4, 0];
    let mv = [1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0xEC, 0x92, 0xE3, 0xE5, 0, 0];
    let mut r = RoomChat::default();
    assert_eq!(r.receive(&line(1, "Vacuos", "hi", &m)), None, "not started");
    r.character_set(1);
    assert_eq!(
        r.receive(&line(1, "Vacuos", "hi", &m)),
        Some(Heard::Line {
            speaker: units("Vacuos[M]"),
            text: units("hi"),
            room: 1,
            flags: 5,
        })
    );
    assert_eq!(
        r.receive(&line(6, "Aurum", "yo", &mv)),
        Some(Heard::Line {
            speaker: units("Aurum[Mv]"),
            text: units("yo"),
            room: 6,
            flags: 0,
        })
    );
    assert_eq!(
        r.receive(&line(1, "Nobody", "x", &[1, 0, 0])),
        Some(Heard::Line {
            speaker: units("Nobody"),
            text: units("x"),
            room: 1,
            flags: 0,
        })
    );
}
