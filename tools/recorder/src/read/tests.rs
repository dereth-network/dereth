//! The reader over sessions made in the test and played through the recorder between a stand-in
//! client and server: every message named by its game, and the ones that name one object.
//! Behaviour: none (a tool's contract, not game behavior)

use auberean_protocol::login::Empty;
use auberean_protocol::physics::{ObjectEvent, ObjectOnly, Target, Word};
use auberean_protocol::types::{BehaviorParams, ObjectStamp};
use auberean_protocol::{ClientMessage, ServerMessage};
use dereth_pcap::transport::build::{connect_request, fragments, login_request};
use dereth_primitives::ObjectId;
use dereth_protocol::login::LoginWorldInfo;
use dereth_protocol::objects::{ItemAppraise, ItemServerSaysRemove};

use super::*;
use crate::testing::{play, Scratch, Way};

/// A session: the login and its answer, then each message in its own datagram, numbered in its
/// direction from 2.
fn session(messages: &[(Dir, Vec<u8>)]) -> Vec<(Way, Vec<u8>)> {
    let mut out = vec![
        (Way::ToServer(0), login_request("1802")),
        (Way::ToClient(0), connect_request(0x1111, 0x2222)),
    ];
    let mut seq = [2u32, 2];
    for (n, (dir, blob)) in messages.iter().enumerate() {
        let d = dir.index();
        let datagram = fragments(seq[d], 0xABCD, &[(n as u64 + 1, 0, 1, blob)]);
        seq[d] += 1;
        out.push((
            match dir {
                Dir::C2s => Way::ToServer(0),
                Dir::S2c => Way::ToClient(1),
            },
            datagram,
        ));
    }
    out
}

fn recorded(messages: &[(Dir, Vec<u8>)]) -> Recording {
    let scratch = Scratch::new();
    play(&scratch, &session(messages));
    read_file(&scratch.file(), None).expect("the recording reads")
}

fn stamp(id: u64) -> ObjectStamp {
    ObjectStamp {
        id,
        instance_stamp: 1,
        other_stamp: 1,
    }
}

const WASP: u64 = 0x0000_0001_8000_0A5B;
const OTHER: u64 = 0x0000_0001_8000_0077;

#[test]
fn the_reader_lists_every_message_that_names_one_object_in_order() {
    let r = recorded(&[
        (
            Dir::C2s,
            ClientMessage::SceneRenderingComplete(Empty).encode(),
        ),
        (
            Dir::S2c,
            ServerMessage::PlayBehavior(ObjectEvent {
                sender: stamp(WASP),
                action: BehaviorParams::default(),
            })
            .encode(),
        ),
        (
            Dir::S2c,
            ServerMessage::StopBehavior(ObjectEvent {
                sender: stamp(OTHER),
                action: Word(3),
            })
            .encode(),
        ),
        (
            Dir::S2c,
            ServerMessage::LookAt(ObjectEvent {
                sender: stamp(OTHER),
                action: Target(WASP),
            })
            .encode(),
        ),
        (
            Dir::S2c,
            ServerMessage::ForgetObject(ObjectOnly(stamp(WASP))).encode(),
        ),
    ]);
    assert_eq!(r.sessions.len(), 1);
    let all: Vec<_> = r.messages.iter().map(|m| (m.name, m.read)).collect();
    assert_eq!(
        all,
        [
            ("SceneRenderingComplete", true),
            ("PlayBehavior", true),
            ("StopBehavior", true),
            ("LookAt", true),
            ("ForgetObject", true)
        ]
    );
    assert!(r.messages.iter().all(|m| m.game == Game::Auberean));

    let wasp: Vec<_> = r.naming(WASP).map(|(m, how)| (m.name, how)).collect();
    assert_eq!(
        wasp,
        [
            ("PlayBehavior", "its subject".to_owned()),
            ("LookAt", "its id at byte 16".to_owned()),
            ("ForgetObject", "its subject".to_owned()),
        ]
    );
    let times: Vec<f64> = r.naming(WASP).map(|(m, _)| m.t).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
    let first = r.naming(WASP).next().expect("one");
    let shown = line(first.0, Some(&first.1));
    assert!(
        shown.contains("s2c  0x1007F  PlayBehavior  its subject"),
        "{shown}"
    );
}

#[test]
fn a_first_game_recording_names_its_objects_by_their_fields() {
    let player = ObjectId(0x5000_0001);
    let item = 0x8000_1234u32;
    let r = recorded(&[
        (
            Dir::S2c,
            dereth_protocol::write_blob(&LoginWorldInfo {
                connections: 1,
                max_connections: 100,
                world_name: "Test".into(),
            })
            .expect("encodes"),
        ),
        (
            Dir::S2c,
            dereth_protocol::events::pack_event(
                player,
                1,
                &ItemServerSaysRemove {
                    object: ObjectId(item),
                },
            )
            .expect("encodes"),
        ),
        (
            Dir::C2s,
            dereth_protocol::actions::pack_action(
                1,
                &ItemAppraise {
                    target: ObjectId(item),
                },
            )
            .expect("encodes"),
        ),
        (
            Dir::S2c,
            dereth_protocol::events::pack_event(
                player,
                2,
                &ItemServerSaysRemove {
                    object: ObjectId(0x8000_9999),
                },
            )
            .expect("encodes"),
        ),
    ]);
    assert!(r.messages.iter().all(|m| m.game == Game::Dereth && m.read));
    let named: Vec<_> = r
        .naming(u64::from(item))
        .map(|(m, how)| (m.dir, m.mtype, m.name, how))
        .collect();
    assert_eq!(
        named,
        [
            (
                Dir::S2c,
                0x0024,
                type_name(0x0024).expect("named"),
                "$.object".to_owned()
            ),
            (
                Dir::C2s,
                0x00C8,
                type_name(0x00C8).expect("named"),
                "$.target".to_owned()
            ),
        ]
    );
    // The recipient of a game event is named by every event sent to it.
    assert_eq!(r.naming(u64::from(player.0)).count(), 2);
}

#[test]
fn object_ids_are_read_in_hexadecimal_or_decimal() {
    assert_eq!(parse_id("0x80000A5B"), Ok(0x8000_0A5B));
    assert_eq!(parse_id("0x0000_0001_8000_0A5B"), Ok(WASP));
    assert_eq!(parse_id("2147486299"), Ok(0x8000_0A5B));
    assert_eq!(parse_id("80000A5B"), Ok(0x8000_0A5B));
    assert!(parse_id("a wasp").is_err());
}
