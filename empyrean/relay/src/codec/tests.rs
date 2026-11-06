//! Behaviour: none (synthetic ac1999 wire conformance and bounded malformed-input checks).

use super::*;
use dereth_protocol::{read_body, read_checked, write_blob};

fn roster_vector() -> Vec<u8> {
    // Two slots; an empty name leaves the second slot at an unaligned stream offset.
    let mut bytes = vec![0x58, 0xf6, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];
    bytes.extend([0, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
    bytes.extend(7u32.to_le_bytes());
    bytes.extend([1, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0xc9, 0, 0, 0]);
    bytes.extend(u32::MAX.to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(b"demo");
    bytes.extend([0; 20]);
    bytes
}

#[test]
fn the_ac1999_roster_preserves_slots_deletion_time_and_account_with_absolute_string_alignment() {
    let bytes = roster_vector();
    let roster = read_checked::<Roster>(&bytes).unwrap();
    assert_eq!(roster.account, "demo");
    assert_eq!(
        roster.characters,
        vec![
            CharacterIdentity {
                gid: ObjectId(1),
                name: String::new(),
                seconds_greyed_out: 7
            },
            CharacterIdentity {
                gid: ObjectId(2),
                name: "É".into(),
                seconds_greyed_out: u32::MAX
            },
        ]
    );
    assert_eq!(write_blob(&roster).unwrap(), bytes);
    assert!(read_checked::<dereth_protocol::login::LoginCharacterSet>(&bytes).is_err());
}

#[test]
fn the_ac1999_roster_rejects_every_truncation_and_invalid_counts_slots_ids_padding_and_tails() {
    let bytes = roster_vector();
    for end in 0..bytes.len() {
        assert!(read_checked::<Roster>(&bytes[..end]).is_err(), "{end}");
    }
    for (offset, value) in [(4, 65), (8, 1), (12, 1), (16, 0), (26, 0), (30, 1), (38, 1)] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert!(read_checked::<Roster>(&bad).is_err(), "offset {offset}");
    }
    // The account field's marker, reserved word and bytes after its NUL are not checked.
    let mut loose = bytes.clone();
    for offset in [44, 53, bytes.len() - 1] {
        loose[offset] = 0x5a;
    }
    assert_eq!(read_checked::<Roster>(&loose).unwrap().account, "demo");
    let mut tail = bytes;
    tail.push(0);
    assert!(read_checked::<Roster>(&tail).is_err());
    let too_many = Roster {
        characters: vec![CharacterIdentity::default(); 65],
        account: String::new(),
    };
    assert!(write_blob(&too_many).is_err());
}

#[test]
fn the_ac1999_news_empty_leading_string_does_not_align_and_second_string_is_cp1252() {
    let bytes = [0x5a, 0xf6, 0, 0, 0, 0, 3, 0, 0x80, b'!', 0, 0];
    let news = read_checked::<News>(&bytes).unwrap();
    assert_eq!(news.leading, "");
    assert_eq!(news.text, "€!");
    assert_eq!(write_blob(&news).unwrap(), bytes);
    for end in 0..bytes.len() {
        assert!(read_checked::<News>(&bytes[..end]).is_err());
    }
    for index in [10, 11] {
        let mut bad = bytes;
        bad[index] = 1;
        assert!(read_checked::<News>(&bad).is_err());
    }
    assert!(write_blob(&News {
        leading: "🌍".into(),
        text: String::new()
    })
    .is_err());
    assert!(write_blob(&News {
        leading: String::new(),
        text: "x".repeat(MAX_NEWS + 1)
    })
    .is_err());
    assert!(read_body::<News>(&[0, 0, 0xff, 0xff]).is_err());
}

#[test]
fn the_ac1999_entry_request_is_f7c8_and_the_selection_carries_a_marked_account_field() {
    assert_eq!(write_blob(&EntryRequest).unwrap(), [0xc8, 0xf7, 0, 0]);
    assert_eq!(write_blob(&EntryReady).unwrap(), [0xc7, 0xf7, 0, 0]);
    assert!(read_body::<EntryReady>(&[0]).is_err());
    assert!(read_checked::<EntryReady>(&[0xdf, 0xf7, 0, 0]).is_err());
    let mut bytes = vec![
        0x57, 0xf6, 0, 0, 0x78, 0x56, 0x34, 0x12, 0xef, 0xbe, 0xef, 0xbe,
    ];
    bytes.extend(b"demo");
    bytes.extend([0; 20]);
    let selected = read_checked::<SelectCharacter>(&bytes).unwrap();
    assert_eq!(selected.character, ObjectId(0x1234_5678));
    assert_eq!(selected.account, "demo");
    assert_eq!(write_blob(&selected).unwrap(), bytes);
    for end in 0..bytes.len() {
        assert!(read_checked::<SelectCharacter>(&bytes[..end]).is_err());
    }
    assert!(write_blob(&SelectCharacter {
        character: ObjectId(1),
        account: "a".repeat(21)
    })
    .is_err());
    assert!(write_blob(&SelectCharacter {
        character: ObjectId(0),
        account: "demo".into()
    })
    .is_err());
}

fn creation() -> CharacterGen {
    CharacterGen {
        heritage: 0,
        gender: 1,
        face: [1, 2, 3, 4, 5, 6, 7],
        headgear_color: 8,
        clothing: [(9, 10), (11, 12), (13, 14)],
        shades: [0.5; 6],
        template: 15,
        attributes: [55; 6],
        player: 0,
        access: 1,
        skills: vec![1, 2, 3],
        name: "Abc".into(),
        heraldry: -1,
        heraldry_colors: 2,
        start_area: 3,
    }
}

#[test]
fn the_ac1999_creation_is_packed_in_the_clients_order_and_closed_by_its_sum() {
    let value = CreateCharacter {
        account: "probe".into(),
        character: creation(),
    };
    let bytes = write_blob(&value).unwrap();
    let words = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(words(0), 0xf656);
    assert_eq!(words(4), 0xbeef_beef);
    assert_eq!(&bytes[8..13], b"probe");
    // Version, heritage, gender, then the seven face choices and the headgear color.
    assert_eq!(
        (32..72).step_by(4).map(words).collect::<Vec<_>>(),
        [1, 0, 1, 1, 2, 3, 4, 5, 6, 7]
    );
    // Styles and colors interleaved, then the six shades.
    assert_eq!(
        (72..100).step_by(4).map(words).collect::<Vec<_>>(),
        [8, 9, 10, 11, 12, 13, 14]
    );
    assert_eq!(&bytes[100..108], &0.5f64.to_le_bytes());
    // The sum of heritage, gender, face, styles, template and attributes.
    let sum = 1 + (1..=7).sum::<u32>() + 9 + 11 + 13 + 15 + 6 * 55;
    assert_eq!(words(bytes.len() - 4), sum);
    assert_eq!(read_checked::<CreateCharacter>(&bytes).unwrap(), value);
    let mut wrong = bytes.clone();
    let last = wrong.len() - 1;
    wrong[last] ^= 1;
    assert!(read_checked::<CreateCharacter>(&wrong).is_err());
    for end in 0..bytes.len() {
        assert!(read_checked::<CreateCharacter>(&bytes[..end]).is_err());
    }
}

#[test]
fn the_ac1999_creation_answer_is_a_code_and_on_success_the_new_entry() {
    let done = CreateResponse {
        code: 1,
        character: Some(CharacterIdentity {
            gid: ObjectId(0x5000_0001),
            name: "Abc".into(),
            seconds_greyed_out: 0,
        }),
    };
    let bytes = write_blob(&done).unwrap();
    assert_eq!(read_checked::<CreateResponse>(&bytes).unwrap(), done);
    let refused = [0x43, 0xf6, 0, 0, 3, 0, 0, 0];
    assert_eq!(
        read_checked::<CreateResponse>(&refused).unwrap(),
        CreateResponse {
            code: 3,
            character: None
        }
    );
}

#[test]
fn an_appearance_event_reads_the_early_appearance_and_both_stamps() {
    use dereth_protocol::objects::ItemObjDescEvent;
    use dereth_protocol::types::{physicsdesc::PhysicsEventStamp, AnimPartChange, ObjDesc};
    let event = objects::ObjDescEvent(ItemObjDescEvent {
        id: ObjectId(0x8000_0001),
        objdesc: ObjDesc {
            anim_part_changes: vec![AnimPartChange {
                part_index: 3,
                part_id: 0x0100_0042,
            }],
            ..Default::default()
        },
        timestamps: PhysicsEventStamp {
            instance: 2,
            event: 9,
        },
    });
    let blob = dereth_protocol::write_blob(&event).unwrap();
    // Opcode, id, version and three counts, one model change of three bytes, padding, stamps.
    assert_eq!(blob.len(), 4 + 4 + 4 + 4 + 4);
    assert_eq!(&blob[8..12], &[0x11, 0, 0, 1]);
    assert_eq!(
        dereth_protocol::read_body::<objects::ObjDescEvent>(&blob[4..]).unwrap(),
        event
    );
}

#[test]
fn an_ordered_message_carries_the_object_its_stamp_and_a_whole_message() {
    let envelope = Envelope {
        object: ObjectId(0x5000_0001),
        stamp: 7,
        message: vec![0x96, 0x01, 0, 0, 1, 0, 0, 0],
    };
    let blob = dereth_protocol::write_blob(&envelope).unwrap();
    let back = dereth_protocol::read_body::<Envelope>(&blob[4..]).unwrap();
    assert_eq!((back.opcode(), back), (0x0196, envelope));
    let mut w = dereth_protocol::Writer::body();
    w.u32(1);
    w.u32(2);
    assert!(dereth_protocol::read_body::<Envelope>(w.as_slice()).is_err());
}
