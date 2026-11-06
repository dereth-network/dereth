//! Behaviour: none (the 1999 messages as the later ones, over made-up bodies from the relay's codecs).

use dereth_protocol::objects::ObjectCreatePayload;
use dereth_protocol::types::qualities::SecondaryAttribute;
use dereth_protocol::{read_checked, Reader};

use super::*;

const PLAYER: u32 = 0x5000_0001;
const OTHER: u32 = 0x8000_0002;

fn entered() -> Down {
    let mut d = Down::default();
    d.message(
        &write_blob(&LoginCreatePlayer {
            player_id: ObjectId(PLAYER),
        })
        .unwrap(),
    );
    d
}

fn later(out: Vec<Out>) -> Vec<OutboundMessage> {
    out.into_iter()
        .map(|o| match o {
            Out::Later(m) => m,
            other => panic!("not a message for Dereth: {other:?}"),
        })
        .collect()
}

fn one(d: &mut Down, bytes: &[u8]) -> OutboundMessage {
    let mut all = later(d.message(bytes));
    assert_eq!(all.len(), 1, "{bytes:02X?}");
    all.remove(0)
}

fn words(op: u32, words: &[u32]) -> Vec<u8> {
    let mut b = op.to_le_bytes().to_vec();
    for w in words {
        b.extend(w.to_le_bytes());
    }
    b
}

/// A wrapped event's header and what it wraps.
fn unwrapped(m: &OutboundMessage) -> (OrderedEventHeader, Vec<u8>) {
    assert_eq!(m.data[..4], 0xF7B0u32.to_le_bytes(), "wrapped");
    let header = OrderedEventHeader {
        iid: ObjectId(u32::from_le_bytes(m.data[4..8].try_into().unwrap())),
        stamp: u32::from_le_bytes(m.data[8..12].try_into().unwrap()),
    };
    (header, m.data[12..].to_vec())
}

fn create(id: u32) -> Vec<u8> {
    write_blob(&CreateObject {
        object: ObjectCreatePayload {
            id: ObjectId(id),
            wdesc: dereth_protocol::types::PublicWeenieDesc {
                name: "Thing".into(),
                wcid: 1,
                icon_id: 0x0600_1036,
                ..Default::default()
            },
            ..Default::default()
        },
        vendor_class: None,
    })
    .unwrap()
}

#[test]
fn a_second_create_of_a_known_object_is_the_later_update_until_it_is_deleted() {
    let mut d = entered();
    assert_eq!(one(&mut d, &create(OTHER)).opcode(), 0xF745);
    assert_eq!(one(&mut d, &create(OTHER)).opcode(), 0xF7DB);
    assert_eq!(one(&mut d, &create(OTHER)).opcode(), 0xF7DB);
    let delete = write_blob(&ItemDeleteObject {
        id: ObjectId(OTHER),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(one(&mut d, &delete).opcode(), 0xF747);
    assert_eq!(one(&mut d, &create(OTHER)).opcode(), 0xF745);
}

#[test]
fn each_objects_events_take_stamps_from_one_with_no_gap() {
    let mut d = Down::default();
    // Before the player is known, events are ordered against no object.
    let m = one(&mut d, &words(0x000E, &[OTHER, 25, 1]));
    assert_eq!(
        unwrapped(&m).0,
        OrderedEventHeader {
            iid: ObjectId(0),
            stamp: 1
        }
    );
    d.message(
        &write_blob(&LoginCreatePlayer {
            player_id: ObjectId(PLAYER),
        })
        .unwrap(),
    );
    for stamp in 1..=3 {
        let m = one(&mut d, &words(0x000E, &[PLAYER, 25, 1]));
        assert_eq!(
            unwrapped(&m).0,
            OrderedEventHeader {
                iid: ObjectId(PLAYER),
                stamp
            },
            "a gap here would hold every later event back"
        );
    }
}

#[test]
fn an_update_of_the_player_is_private_and_of_another_public_each_with_its_own_sequence() {
    let mut d = entered();
    for sequence in 1..=2u8 {
        let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[PLAYER, 25, 7])));
        assert_eq!(body[..4], 0x02CDu32.to_le_bytes());
        assert_eq!(body[4], sequence);
        assert_eq!(body[5..9], 25u32.to_le_bytes());
        assert_eq!(body[9..13], 7u32.to_le_bytes());
    }
    let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[OTHER, 25, 7])));
    assert_eq!(body[..4], 0x02CEu32.to_le_bytes());
    assert_eq!(body[4], 1, "another object's property counts on its own");
    assert_eq!(body[5..9], OTHER.to_le_bytes());
    // Another property of the player counts on its own too.
    let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[PLAYER, 26, 7])));
    assert_eq!(body[4], 1);
}

#[test]
fn a_sequence_byte_wraps() {
    let mut d = entered();
    for _ in 0..255 {
        d.message(&words(0x000E, &[PLAYER, 25, 7]));
    }
    let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[PLAYER, 25, 7])));
    assert_eq!(body[4], 0);
}

#[test]
fn experience_on_its_int_keys_becomes_the_later_int64() {
    let mut d = entered();
    let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[PLAYER, 0x15, 500])));
    assert_eq!(body[..4], 0x02CFu32.to_le_bytes());
    assert_eq!(body[5..9], 1u32.to_le_bytes(), "total");
    assert_eq!(body[9..17], 500i64.to_le_bytes());
    let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[PLAYER, 0x16, 40])));
    assert_eq!(body[5..9], 2u32.to_le_bytes(), "available");
    assert_eq!(body[9..17], 40i64.to_le_bytes());
}

#[test]
fn a_combat_mode_becomes_the_later_bits() {
    let mut d = entered();
    for (old, bits) in [(1u32, 1u32), (2, 2), (3, 4), (4, 8)] {
        let (_, body) = unwrapped(&one(&mut d, &words(0x000E, &[PLAYER, 0x28, old])));
        assert_eq!(body[9..13], bits.to_le_bytes());
    }
}

#[test]
fn a_maximum_vital_sends_the_whole_attribute_and_a_current_one_only_its_level() {
    let mut d = entered();
    let vital = [10u32, 20, 30, 44];
    let mut body = vec![PLAYER, 1];
    body.extend(vital);
    let (_, sent) = unwrapped(&one(&mut d, &words(0x0029, &body)));
    assert_eq!(sent[..4], 0x02E7u32.to_le_bytes());
    let mut r = Reader::body(&sent[9..]);
    let read = SecondaryAttribute::read(&mut r).unwrap();
    assert_eq!(read.current_level, 44);
    let mut body = vec![PLAYER, 2];
    body.extend(vital);
    let (_, sent) = unwrapped(&one(&mut d, &words(0x0029, &body)));
    assert_eq!(sent[..4], 0x02E9u32.to_le_bytes());
    assert_eq!(sent[5..9], 2u32.to_le_bytes());
    assert_eq!(sent[9..], 44u32.to_le_bytes());
}

#[test]
fn a_position_with_no_w_is_stated_as_zero() {
    let mut p = PositionPack {
        flags: position_flags::ORIENTATION_HAS_NO_W,
        ..PositionPack::default()
    };
    p.orientation.x = 1.0;
    let m = write_blob(&MovementPositionEvent {
        id: ObjectId(OTHER),
        position: p,
    })
    .unwrap();
    let mut d = entered();
    let sent = one(&mut d, &m);
    let back = read_checked::<MovementPositionEvent>(&sent.data).unwrap();
    assert_eq!(back.position.orientation.w, 0.0, "the later default is one");
    assert_eq!(
        back.position.flags & position_flags::ORIENTATION_HAS_NO_W,
        0
    );
    assert_eq!(back.position.orientation.x, 1.0);
}

#[test]
fn messages_the_two_wires_share_pass_byte_for_byte() {
    let mut d = entered();
    let mut movement = 0xF74Cu32.to_le_bytes().to_vec();
    movement.extend([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    let sent = one(&mut d, &movement);
    assert_eq!(
        sent.data, movement,
        "motion commands are the world's own numbering"
    );
    assert_eq!(sent.group, GameMessageGroup::SmartboxQueue);
}

#[test]
fn a_stale_wrapped_event_is_dropped_and_a_newer_one_taken() {
    let wrap = |stamp: u32, inner: &[u8]| {
        write_blob(&Envelope {
            object: ObjectId(PLAYER),
            stamp,
            message: inner.to_vec(),
        })
        .unwrap()
    };
    let inner = words(0x0196, &[OTHER]);
    let mut d = entered();
    assert_eq!(later(d.message(&wrap(5, &inner))).len(), 1);
    assert!(d.message(&wrap(5, &inner)).is_empty());
    assert!(d.message(&wrap(4, &inner)).is_empty());
    assert_eq!(later(d.message(&wrap(6, &inner))).len(), 1);
}

#[test]
fn an_unknown_opcode_is_counted_and_dropped_and_a_later_only_one_dropped_by_policy() {
    let mut d = entered();
    assert!(d.message(&words(0x7777, &[1])).is_empty());
    assert_eq!(d.tally.unknown[&0x7777].messages, 1);
    assert!(d.message(&words(0x002B, &[1])).is_empty(), "a title");
    assert_eq!(d.tally.dropped[&0x002B].messages, 1);
}

#[test]
fn a_body_too_short_to_read_is_counted_and_dropped() {
    let mut d = entered();
    assert!(d.message(&words(0x000E, &[1])).is_empty());
    assert_eq!(d.tally.malformed[&0x000E].messages, 1);
    assert!(d.message(&[1]).is_empty());
}

#[test]
fn a_server_line_becomes_the_later_text_with_its_type() {
    let mut w = Writer::new();
    w.u32(0xF62C);
    codec::write_string(&mut w, "Hello there", codec::MAX_NEWS).unwrap();
    w.u32(3);
    let mut d = entered();
    let sent = one(&mut d, &w.into_inner());
    let body = if sent.data[..4] == 0xF7B0u32.to_le_bytes() {
        unwrapped(&sent).1
    } else {
        sent.data.clone()
    };
    let line = read_checked::<CommunicationTextboxString>(&body).unwrap();
    assert_eq!((line.text.as_str(), line.text_type), ("Hello there", 3));
}

#[test]
fn the_list_offers_room_when_the_1999_wire_names_none() {
    let roster = Roster {
        characters: Vec::new(),
        account: "probe".into(),
    };
    let set = character_set(&roster);
    assert_eq!(set.account, "probe");
    assert!(
        set.num_allowed_characters > 0,
        "or nothing could be created"
    );
}

#[test]
fn the_overlay_manifest_travels_with_the_download() {
    assert_eq!(group_of(0xF7EC), GameMessageGroup::DatabaseQueue);
    assert_eq!(group_of(0xF7E2), GameMessageGroup::DatabaseQueue);
    assert_eq!(group_of(0xF745), GameMessageGroup::SmartboxQueue);
    assert_eq!(group_of(0x0013), GameMessageGroup::UIQueue);
}

#[test]
fn a_spell_update_becomes_the_later_message_with_the_spell_alone() {
    let mut d = entered();
    let (header, body) = unwrapped(&one(&mut d, &words(0x004C, &[1234])));
    assert_eq!(header.iid, ObjectId(PLAYER));
    let m = read_checked::<later::MagicUpdateSpell>(&body).unwrap();
    assert_eq!(m.layered_spell_id, 1234);
    assert_eq!(body.len(), 8, "the wire says only which spell");
}

#[test]
fn a_wrapped_spell_update_arrives_in_the_relays_own_sequence() {
    let inner = words(0x004C, &[1234]);
    let wrapped_by_server = write_blob(&Envelope {
        object: ObjectId(PLAYER),
        stamp: 77,
        message: inner,
    })
    .unwrap();
    let mut d = entered();
    let (header, body) = unwrapped(&one(&mut d, &wrapped_by_server));
    assert_eq!(header.stamp, 1, "not the server's stamp");
    assert_eq!(body[..4], 0x02C1u32.to_le_bytes());
}

#[test]
fn a_spell_removal_becomes_the_later_message() {
    let mut d = entered();
    let (_, body) = unwrapped(&one(&mut d, &words(0x004D, &[1234])));
    let m = read_checked::<later::MagicRemoveSpell>(&body).unwrap();
    assert_eq!(m.layered_spell_id, 1234);
}

#[test]
fn a_spell_update_with_no_spell_is_counted_and_dropped() {
    let mut d = entered();
    assert!(d.message(&words(0x004C, &[])).is_empty());
    assert_eq!(d.tally.malformed[&0x004C].messages, 1);
}

#[test]
fn each_portal_storm_stage_becomes_its_later_message() {
    let mut d = entered();
    // Subsided is bare; the warnings carry a word the 1999 client discards.
    let (_, subsided) = unwrapped(&one(&mut d, &words(0x009C, &[])));
    assert_eq!(subsided, 0x02CCu32.to_le_bytes());
    for (old, new) in [(0x009Du32, 0x02C9u32), (0x009E, 0x02CA)] {
        let (_, warning) = unwrapped(&one(&mut d, &words(old, &[0])));
        assert_eq!(warning[..4], new.to_le_bytes());
        assert_eq!(
            warning[4..],
            STORM_WARNING_LEVEL.to_le_bytes(),
            "the client's lamp lights only above zero"
        );
    }
}

#[test]
fn the_storm_itself_carries_no_text_because_the_later_client_writes_the_line() {
    let mut w = Writer::new();
    w.u32(0x009F);
    codec::write_string(
        &mut w,
        "The Portal Storm has teleported you away from the crowded area!",
        codec::MAX_NEWS,
    )
    .unwrap();
    let mut d = entered();
    let (_, storm) = unwrapped(&one(&mut d, &w.into_inner()));
    assert_eq!(storm, 0x02CBu32.to_le_bytes());
}

#[test]
fn scripts_and_sounds_keep_the_numbers_the_server_sent() {
    let mut d = entered();
    // The portal storm's script in the 1999 files' own numbering, which the later table has at 0x73.
    let script = words(0xF755, &[PLAYER, 0x72, 0x3F80_0000]);
    let by_id = words(0xF754, &[PLAYER, 0x3300_0072]);
    let sound = words(0xF750, &[PLAYER, 0x7C, 0x3F80_0000]);
    for message in [&script, &by_id, &sound] {
        let sent = one(&mut d, message);
        assert_eq!(&sent.data, message, "byte for byte");
        assert_eq!(sent.group, GameMessageGroup::SmartboxQueue);
    }
}

#[test]
fn a_magic_flavor_line_keeps_its_text_and_its_magic_type() {
    let mut w = Writer::new();
    w.u32(0xF62C);
    codec::write_string(&mut w, "The magic blazes through you!", codec::MAX_NEWS).unwrap();
    // The server's magic line type, and the later client's.
    w.u32(7);
    let mut d = entered();
    let sent = one(&mut d, &w.into_inner());
    let body = if sent.data[..4] == 0xF7B0u32.to_le_bytes() {
        unwrapped(&sent).1
    } else {
        sent.data.clone()
    };
    let line = read_checked::<CommunicationTextboxString>(&body).unwrap();
    assert_eq!(line.text, "The magic blazes through you!");
    assert_eq!(line.text_type, 7);
}

/// Two strings and whatever words follow, as the server's chat builders write them.
fn strings(op: u32, first: &str, second: &str, tail: &[u32]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(op);
    codec::write_string(&mut w, first, codec::MAX_NEWS).unwrap();
    codec::write_string(&mut w, second, codec::MAX_NEWS).unwrap();
    for v in tail {
        w.u32(*v);
    }
    w.into_inner()
}

#[test]
fn speech_becomes_the_later_speech_with_its_line_type() {
    let mut d = entered();
    let sent = one(&mut d, &strings(0x0037, "Hello", "Bob", &[OTHER]));
    let m = read_checked::<CommunicationHearSpeech>(&sent.data).unwrap();
    assert_eq!(
        (m.message.as_str(), m.sender_name.as_str()),
        ("Hello", "Bob")
    );
    assert_eq!((m.sender_id, m.text_type), (ObjectId(OTHER), LINE_SPEECH));
}

#[test]
fn a_tell_becomes_the_later_direct_speech() {
    let mut d = entered();
    let sent = one(&mut d, &strings(0x0038, "Psst", "Bob", &[OTHER, PLAYER]));
    let m = read_checked::<CommunicationHearDirectSpeech>(&sent.data).unwrap();
    assert_eq!(m.message, "Psst");
    assert_eq!(
        (m.sender_id, m.target_id),
        (ObjectId(OTHER), ObjectId(PLAYER))
    );
    assert_eq!(m.text_type, LINE_TELL);
}

#[test]
fn an_emote_keeps_who_and_what() {
    let mut d = entered();
    let mut w = Writer::new();
    w.u32(0x01E0);
    w.u32(OTHER);
    codec::write_string(&mut w, "Bob", codec::MAX_NEWS).unwrap();
    codec::write_string(&mut w, "waves", codec::MAX_NEWS).unwrap();
    let sent = one(&mut d, &w.into_inner());
    let m = read_checked::<CommunicationHearEmote>(&sent.data).unwrap();
    assert_eq!((m.sender, m.text.as_str()), (ObjectId(OTHER), "waves"));
}

#[test]
fn a_channel_line_keeps_its_channel_bit() {
    let mut d = entered();
    let mut w = Writer::new();
    w.u32(0x014A);
    w.u32(0x800);
    codec::write_string(&mut w, "Bob", codec::MAX_NEWS).unwrap();
    codec::write_string(&mut w, "hi all", codec::MAX_NEWS).unwrap();
    let sent = one(&mut d, &w.into_inner());
    let (_, body) = unwrapped(&sent);
    let m = read_checked::<CommunicationChannelBroadcastRecv>(&body).unwrap();
    assert_eq!((m.channel, m.message.as_str()), (0x800, "hi all"));
}

#[test]
fn the_squelch_deltas_become_the_whole_later_list() {
    let mut d = entered();
    let add = |id: u32, name: &str| {
        let mut w = Writer::new();
        w.u32(0x005A);
        codec::write_string(&mut w, name, codec::MAX_NEWS).unwrap();
        w.u32(id);
        w.into_inner()
    };
    d.message(&add(1, "Ann"));
    let (_, body) = unwrapped(&one(&mut d, &add(2, "Bob")));
    let m = read_checked::<CommunicationSetSquelchDb>(&body).unwrap();
    assert_eq!(m.0.character_hash.entries.len(), 2);
    let mut w = Writer::new();
    w.u32(0x005B);
    codec::write_string(&mut w, "Ann", codec::MAX_NEWS).unwrap();
    w.u32(1);
    let (_, body) = unwrapped(&one(&mut d, &w.into_inner()));
    let m = read_checked::<CommunicationSetSquelchDb>(&body).unwrap();
    assert_eq!(m.0.character_hash.entries[0].1.name, "Bob");
}

#[test]
fn a_hit_keeps_percent_damage_and_all_the_conditions() {
    let mut w = Writer::new();
    w.u32(0x01B1);
    codec::write_string(&mut w, "Drudge", codec::MAX_NEWS).unwrap();
    w.u32(4);
    w.f64(0.25);
    w.u32(9);
    w.u32(1);
    w.u32(0x10);
    w.u32(0);
    let mut d = entered();
    let (_, body) = unwrapped(&one(&mut d, &w.into_inner()));
    let m = read_checked::<AttackerNotification>(&body).unwrap();
    assert_eq!(m.defender_name, "Drudge");
    assert_eq!((m.percent, m.damage, m.critical), (0.25, 9, 1));
    assert_eq!(m.attack_conditions, 0x10);
}

#[test]
fn a_stack_size_gets_a_sequence_byte() {
    let mut d = entered();
    for sequence in 1..=2u8 {
        let (_, body) = unwrapped(&one(&mut d, &words(0x0197, &[OTHER, 5, 50])));
        let m = read_checked::<ItemUpdateStackSize>(&body).unwrap();
        assert_eq!(m.sequence, sequence);
        assert_eq!((m.amount, m.new_value), (5, 50));
    }
}

#[test]
fn a_status_text_and_an_attempt_failed_become_their_later_events() {
    let mut d = entered();
    let (_, body) = unwrapped(&one(&mut d, &words(0x001C, &[0x1D])));
    assert_eq!(body, words(0x028A, &[0x1D]));
}

/// `0x0016`: the status text ac1999 sends flat, as its missile refusal does.
fn status_text(line: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(0x0016);
    w.pstring(line).unwrap();
    w.into_inner()
}

#[test]
fn a_status_text_is_the_transient_line_and_a_line_in_the_chat() {
    let mut d = entered();
    let sent = later(d.message(&status_text("Out of Range!")));
    assert_eq!(sent.len(), 2);

    let (_, strip) = unwrapped(&sent[0]);
    assert_eq!(strip[..4], 0x02EBu32.to_le_bytes());
    let transient =
        read_checked::<dereth_protocol::comms::CommunicationTransientString>(&strip).unwrap();
    assert_eq!(transient.text, "Out of Range!");

    assert_eq!(sent[1].data[..4], 0xF7E0u32.to_le_bytes());
    let line = read_checked::<CommunicationTextboxString>(&sent[1].data).unwrap();
    assert_eq!(line.text, "Out of Range!");
    // Type 0x1A is the one type the main chat window's default filter leaves out.
    assert_eq!(line.text_type, SYSTEM_TEXT);
    assert_ne!(line.text_type, 0x1A);
}

#[test]
fn a_missile_refused_for_range_ends_the_attack_with_its_code_and_says_why() {
    let mut d = entered();
    let said = later(d.message(&status_text("Out of Range!")));
    assert_eq!(said.len(), 2);
    let attack_done = words(0x01A7, &[0x0550]);
    assert_eq!(unwrapped(&one(&mut d, &attack_done)).1, attack_done);
}

#[test]
fn every_enchantment_message_names_the_layer_the_description_gave_it() {
    let e = |id: u32| {
        let mut w = Writer::new();
        Enchantment {
            id,
            ..Enchantment::default()
        }
        .write(&mut w)
        .unwrap();
        w.into_inner()
    };
    let mut add = 0x004Eu32.to_le_bytes().to_vec();
    add.extend(e(55));
    let mut d = entered();
    let (_, body) = unwrapped(&one(&mut d, &add));
    assert_eq!(body[..4], 0x02C2u32.to_le_bytes());
    let m = read_checked::<later::MagicUpdateEnchantment>(&body).unwrap();
    assert_eq!((m.0.spell_id(), m.0.layer()), (55, 1));
    let (_, body) = unwrapped(&one(&mut d, &words(0x004F, &[55])));
    let m = read_checked::<later::MagicRemoveEnchantment>(&body).unwrap();
    assert_eq!(m.layered_spell_id, player::layered(55));
    let (_, body) = unwrapped(&one(&mut d, &words(0x01A4, &[55])));
    assert_eq!(body[..4], 0x02C7u32.to_le_bytes());
    let (_, body) = unwrapped(&one(&mut d, &words(0x01A6, &[1, 55])));
    assert_eq!(
        read_checked::<later::MagicRemoveMultipleEnchantments>(&body)
            .unwrap()
            .0,
        [player::layered(55)]
    );
    let (_, body) = unwrapped(&one(&mut d, &words(0x01AE, &[1, 55])));
    assert_eq!(body[..4], 0x02C8u32.to_le_bytes());
}

#[test]
fn a_cleared_stat_and_an_error_with_a_name_pass_as_they_are() {
    for opcode in [0x02B8u32, 0x02B9, 0x028B] {
        let mut d = Down::default();
        let mut bytes = opcode.to_le_bytes().to_vec();
        bytes.extend([1, 2, 3, 4]);
        let out = d.message(&bytes);
        assert_eq!(out.len(), 1, "{opcode:#06X}");
        assert!(d.tally.taken.contains_key(&opcode), "{opcode:#06X}");
    }
}

fn create_component(id: u32, container: Option<ObjectId>) -> Vec<u8> {
    write_blob(&CreateObject {
        object: ObjectCreatePayload {
            id: ObjectId(id),
            wdesc: dereth_protocol::types::PublicWeenieDesc {
                header: 0x0000_1008
                    | if container.is_some() {
                        CONTAINER_HEADER
                    } else {
                        0
                    },
                name: "Lead Scarab".into(),
                wcid: 691,
                icon_id: 0x0600_1036,
                obj_type: SPELL_COMPONENT_TYPE,
                stack_size: Some(20),
                value: Some(100),
                container_id: container,
                ..Default::default()
            },
            ..Default::default()
        },
        vendor_class: None,
    })
    .unwrap()
}

/// The public description's bit for a container.
const CONTAINER_HEADER: u32 = dereth_protocol::types::weeniedesc::header::CONTAINER_ID;

#[test]
fn a_component_created_in_a_pack_says_its_stack_and_nothing_else_does() {
    let mut d = entered();
    let out = later(d.message(&create_component(OTHER, Some(ObjectId(PLAYER)))));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].opcode(), 0xF745, "the create comes first");
    let (_, body) = unwrapped(&out[1]);
    let m = read_checked::<ItemUpdateStackSize>(&body).unwrap();
    assert_eq!((m.item, m.amount, m.new_value), (ObjectId(OTHER), 20, 100));

    let mut d = entered();
    assert_eq!(later(d.message(&create_component(OTHER, None))).len(), 1);
    assert_eq!(later(d.message(&create(OTHER + 1))).len(), 1);

    let mut before_the_player = Down::default();
    let out = before_the_player.message(&create_component(OTHER, Some(ObjectId(PLAYER))));
    assert_eq!(later(out).len(), 1, "the description at login counts these");
}

#[test]
fn a_stack_size_and_an_update_of_that_int_share_one_sequence() {
    let mut d = entered();
    let (_, body) = unwrapped(&one(&mut d, &words(0x0197, &[OTHER, 5, 50])));
    assert_eq!(
        read_checked::<ItemUpdateStackSize>(&body).unwrap().sequence,
        1
    );
    let (_, update) = unwrapped(&one(&mut d, &words(0x000E, &[OTHER, STACK_SIZE_INT, 6])));
    assert_eq!(update[4], 2, "the next sequence byte of the same property");
}
