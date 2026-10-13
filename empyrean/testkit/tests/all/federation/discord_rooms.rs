//! Divergence: V449
//! A world's own rooms mirrored into Discord: a line said in a room the hub says it mirrors goes
//! to the hub under the room's own name and from there to Discord alone; nothing comes back into
//! it, so a room line the hub hands the world there is never shown; a room the hub does not mirror
//! for the world sends nothing; a room the hub stops mirroring stops at once; and a room a hub
//! answers it does not mirror keeps its lines here, telling nobody.
//! Fixture: virtual-time worlds through the hub's routing (`super::Net`), the hub mirroring the
//! room into Discord as its operator would have it do.

use std::collections::BTreeSet;

use dereth_network_hub::bridge::Bridged;
use dereth_network_protocol::{Address, Message, RoomsMessage};
use dereth_primitives::NetQueue;
use dereth_protocol::turbine::{decode_incoming, IncomingPayload, SendToRoomById};
use empyrean_entity::enums::CharacterOption;

use super::{seed, Net, TURBINE_CHAT};

/// The TurbineChat rooms the tests speak in.
pub(super) const GENERAL: u32 = 2;
const TRADE: u32 = 3;
const ROLEPLAY: u32 = 5;

const GENERAL_AND_ROLEPLAY: &[CharacterOption] = &[
    CharacterOption::ListenToGeneralChat,
    CharacterOption::ListenToRoleplayChat,
];

fn addr(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

/// aa's General room, as the hub mirrors it into Discord.
pub(super) fn general() -> Bridged {
    Bridged::World {
        slug: "aa".into(),
        room: "general".into(),
    }
}

/// Ann and Cy on aa, Bob on bb, each on the network. Ann and Bob listen to General and Roleplay,
/// Cy to Roleplay alone. The hub mirrors aa's General room into Discord when `carried`, as it does
/// once aa's operator mirrors it, and tells aa so.
pub(super) fn discord_worlds(carried: bool) -> Net {
    let mut ann = seed("ann", 0x5000_0001, "Ann");
    ann.listens = GENERAL_AND_ROLEPLAY;
    let cy = seed("cy", 0x5000_0002, "Cy");
    let mut bob = seed("bob", 0x5000_0001, "Bob");
    bob.listens = GENERAL_AND_ROLEPLAY;
    let mut net = Net::new(&[("aa", &[ann, cy]), ("bb", &[bob])]);
    if carried {
        net.hub.set_bridged([general()].into());
    }
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    net.enter("aa", cy);
    net.enter("bb", bob);
    for (slug, who) in [("aa", "Ann"), ("aa", "Cy"), ("bb", "Bob")] {
        net.say(slug, who, "@dn on");
    }
    let _ = net.hub.take_bridged();
    net
}

/// `who` says `text` in TurbineChat room `room`, as the client sends it.
pub(super) fn post(net: &mut Net, slug: &str, who: &str, guid: u32, room: u32, text: &str) {
    let w = net.world(slug);
    let id = w.clients[who];
    w.ts.send_message(
        id,
        NetQueue::Logon,
        &SendToRoomById {
            context: 7,
            room,
            text: text.to_owned(),
            sender: guid,
            chat_type: room,
        },
    );
    net.advance(0.3);
}

/// The room events `who` was shown since `mark`: the room, the speaker's name, the text and the
/// speaker's id.
fn room_events(
    net: &mut Net,
    slug: &str,
    who: &str,
    mark: usize,
) -> Vec<(u32, String, String, u32)> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == TURBINE_CHAT)
        .filter_map(|g| {
            match decode_incoming(&g.blob[4..])
                .expect("a TurbineChat blob")
                .payload
            {
                IncomingPayload::RoomEvent(e) => {
                    let sender = e
                        .extra
                        .get(0..4)
                        .map_or(0, |b| u32::from_le_bytes(b.try_into().expect("four bytes")));
                    Some((e.room, e.name, e.text, sender))
                }
                _ => None,
            }
        })
        .collect()
}

/// The room lines `slug` sent the hub since `mark`: the room, the speaker and the text.
fn said_since(net: &Net, slug: &str, mark: usize) -> Vec<(String, String, String)> {
    net.sent_since(slug, mark)
        .into_iter()
        .filter_map(|m| match m {
            Message::RoomSay {
                room, from, text, ..
            } => Some((room, from.to_string(), text)),
            _ => None,
        })
        .collect()
}

/// The lines the hub mirrored into Discord since it was last asked: the room, the speaker and the
/// text.
fn in_discord(net: &mut Net) -> Vec<(Bridged, String, String)> {
    net.hub
        .take_bridged()
        .into_iter()
        .map(|l| (l.to, l.from.to_string(), l.text))
        .collect()
}

/// Divergence: V449
#[test]
fn a_line_in_a_room_the_hub_mirrors_goes_to_discord_alone_under_the_speakers_address() {
    let mut net = discord_worlds(true);
    assert!(
        net.sent_since("aa", 0)
            .iter()
            .any(|m| matches!(m, Message::RoomJoin { room, on: true } if room == "general")),
        "aa joins its own room by its name once the hub says it mirrors it"
    );
    let (ma, mc, mb) = (
        net.mark("aa", "Ann"),
        net.mark("aa", "Cy"),
        net.mark("bb", "Bob"),
    );
    let (wire, sent) = (net.wire.0.len(), net.sent.0.len());
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        GENERAL,
        "selling scarabs by the fountain",
    );
    assert_eq!(
        said_since(&net, "aa", sent),
        [(
            "general".to_owned(),
            "Ann@aa".to_owned(),
            "selling scarabs by the fountain".to_owned()
        )]
    );
    assert_eq!(
        net.wire_since(wire),
        ["aa -> hub: room.say", "hub -> aa: room.result"],
        "no world is handed it"
    );
    assert_eq!(
        in_discord(&mut net),
        [(
            general(),
            "Ann@aa".to_owned(),
            "selling scarabs by the fountain".to_owned()
        )]
    );
    assert_eq!(
        room_events(&mut net, "aa", "Ann", ma),
        [(
            GENERAL,
            "Ann".to_owned(),
            "selling scarabs by the fountain".to_owned(),
            0x5000_0001
        )],
        "shown here as General's own line, once"
    );
    assert!(room_events(&mut net, "aa", "Cy", mc).is_empty());
    assert!(room_events(&mut net, "bb", "Bob", mb).is_empty());
    assert!(
        Net::lines(&net.since("aa", "Ann", ma)).is_empty(),
        "the speaker is told nothing"
    );
}

/// Divergence: V449
#[test]
fn a_room_the_hub_does_not_mirror_for_the_world_sends_nothing() {
    // The hub mirrors aa's General alone: neither aa's Trade nor bb's General.
    let mut net = discord_worlds(true);
    let (wire, sent) = (net.wire.0.len(), net.sent.0.len());
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        TRADE,
        "trade stays here",
    );
    post(
        &mut net,
        "bb",
        "Bob",
        0x5000_0001,
        GENERAL,
        "bb's General stays there",
    );
    assert!(
        net.wire_since(wire).is_empty(),
        "{:?}",
        net.wire_since(wire)
    );
    assert!(said_since(&net, "aa", sent).is_empty() && said_since(&net, "bb", sent).is_empty());
    assert!(in_discord(&mut net).is_empty());
    // The Global room's carrying room goes to the other worlds as before, and not to Discord.
    post(&mut net, "aa", "Ann", 0x5000_0001, ROLEPLAY, "dance at 8");
    assert_eq!(
        said_since(&net, "aa", sent),
        [(
            "global".to_owned(),
            "Ann@aa".to_owned(),
            "dance at 8".to_owned()
        )]
    );
    assert!(in_discord(&mut net).is_empty());
}

/// Divergence: V449
#[test]
fn the_worlds_own_room_shows_no_line_the_hub_hands_it() {
    let mut net = discord_worlds(true);
    let ma = net.mark("aa", "Ann");
    for from in ["Ann@aa", "Cy@aa", "Bob@bb", "Nelly@discord"] {
        net.hub_hands(
            "aa",
            &Message::RoomLine {
                room: "general".into(),
                from: addr(from),
                text: format!("a line from {from}"),
                via: None,
            },
        );
    }
    net.advance(0.3);
    assert!(room_events(&mut net, "aa", "Ann", ma).is_empty());
    // Nor does the hub hand back a line aa said there.
    let handed = net.handed.len();
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "said once");
    assert!(
        net.handed[handed..]
            .iter()
            .all(|(_, m)| !matches!(m, Message::RoomLine { .. })),
        "{:?}",
        &net.handed[handed..]
    );
}

/// Divergence: V449
#[test]
fn a_room_the_hub_stops_mirroring_stops_at_once_and_goes_again_once_mirrored() {
    let mut net = discord_worlds(true);
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "first");
    assert_eq!(in_discord(&mut net).len(), 1);
    let sent = net.sent.0.len();
    net.hub.set_bridged(BTreeSet::new());
    net.advance(1.0);
    assert!(
        net.sent_since("aa", sent)
            .iter()
            .any(|m| matches!(m, Message::RoomJoin { room, on: false } if room == "general")),
        "aa leaves the room on the link once the hub says it mirrors it no more"
    );
    let (ma, wire) = (net.mark("aa", "Ann"), net.wire.0.len());
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "here alone");
    assert!(
        net.wire_since(wire).is_empty(),
        "{:?}",
        net.wire_since(wire)
    );
    assert_eq!(room_events(&mut net, "aa", "Ann", ma).len(), 1, "said here");
    net.hub.set_bridged([general()].into());
    net.advance(1.0);
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        GENERAL,
        "mirrored again",
    );
    let texts: Vec<String> = in_discord(&mut net).into_iter().map(|l| l.2).collect();
    assert_eq!(texts, ["mirrored again"], "at once");
}

/// Divergence: V449
#[test]
fn a_room_a_hub_answers_it_does_not_mirror_keeps_its_lines_here_for_a_minute_telling_nobody() {
    let mut net = discord_worlds(false);
    // A hub that says it mirrors aa's General, and answers otherwise.
    net.hub_hands_rooms(
        "aa",
        &RoomsMessage::Setup {
            id: None,
            global: Some("roleplay".into()),
            mirrored: vec!["general".into()],
        },
    );
    let ma = net.mark("aa", "Ann");
    let wire = net.wire.0.len();
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "first");
    assert_eq!(
        net.wire_since(wire),
        ["aa -> hub: room.say", "hub -> aa: room.result"],
        "the hub answers that it does not carry aa's General"
    );
    // The room's lines stay here now, faster than any pace, and spend none of the speaker's.
    let wire = net.wire.0.len();
    for i in 0..6 {
        post(
            &mut net,
            "aa",
            "Ann",
            0x5000_0001,
            GENERAL,
            &format!("line {i}"),
        );
    }
    assert!(
        net.wire_since(wire).is_empty(),
        "{:?}",
        net.wire_since(wire)
    );
    assert!(
        Net::lines(&net.since("aa", "Ann", ma)).is_empty(),
        "nobody is told anything"
    );
    assert_eq!(
        room_events(&mut net, "aa", "Ann", ma).len(),
        7,
        "every line is said here"
    );

    // aa's operator mirrors the room at the hub: within the minute, it goes.
    net.hub.set_bridged([general()].into());
    net.advance(empyrean_world::federation::UNCARRIED_SECS);
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "carried now");
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "and this");
    let texts: Vec<String> = in_discord(&mut net).into_iter().map(|l| l.2).collect();
    assert_eq!(texts, ["carried now", "and this"]);

    // A new link forgets what the hub said before: the first line after it goes at once.
    net.hub.set_bridged(BTreeSet::new());
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "no longer");
    net.unlink("aa");
    net.hub.set_bridged([general()].into());
    net.link("aa");
    // Within the minute, past the speaker's own pace.
    net.advance(10.0);
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "back again");
    let texts: Vec<String> = in_discord(&mut net).into_iter().map(|l| l.2).collect();
    assert_eq!(texts, ["back again"]);
}

/// Divergence: V449
#[test]
fn a_line_for_discord_is_held_to_the_rooms_rules_and_its_speaker_told_of_discord() {
    // Off the network: said here, not sent.
    let mut net = discord_worlds(true);
    net.say("aa", "Ann", "@dn off");
    let (ma, wire) = (net.mark("aa", "Ann"), net.wire.0.len());
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "local only");
    assert!(net.wire_since(wire).is_empty());
    assert!(Net::lines(&net.since("aa", "Ann", ma)).is_empty());
    assert_eq!(room_events(&mut net, "aa", "Ann", ma).len(), 1);

    // Too new for the operator: said here, not sent, and told.
    let mut net = discord_worlds(true);
    net.world("aa")
        .ts
        .world
        .federation
        .settings
        .min_character_age_secs = 3_600;
    let (ma, wire) = (net.mark("aa", "Ann"), net.wire.0.len());
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "hello Discord");
    assert!(net.wire_since(wire).is_empty());
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["Your line did not reach Discord: this character is too new to send to the Dereth Network yet."]
    );

    // Faster than the speaker's pace of room lines: the rest stay here, and the speaker is told.
    let mut net = discord_worlds(true);
    let ma = net.mark("aa", "Ann");
    for i in 0..4 {
        post(
            &mut net,
            "aa",
            "Ann",
            0x5000_0001,
            GENERAL,
            &format!("line {i}"),
        );
    }
    assert_eq!(in_discord(&mut net).len(), 3, "the pace's burst");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["Your line did not reach Discord: you are sending too quickly."]
    );

    // The world's share of room lines spent at the hub: the speaker is told.
    let mut net = discord_worlds(true);
    for i in 0..20u32 {
        net.world_sends(
            "aa",
            Message::RoomSay {
                id: format!("s{i}"),
                room: "general".into(),
                from: addr(&format!("{}@aa", super::friends::minted(i))),
                text: "an auction at noon".into(),
                via: None,
            },
        );
    }
    let ma = net.mark("aa", "Ann");
    post(&mut net, "aa", "Ann", 0x5000_0001, GENERAL, "mine too");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["Your line did not reach Discord: the Dereth Network is holding back this world's lines for now."]
    );

    // The room turned off here: nothing goes out.
    let mut net = discord_worlds(true);
    assert!(empyrean_world::managers::property_manager::modify_bool(
        &net.world("aa").ts.world,
        "chat_disable_general",
        true
    ));
    let wire = net.wire.0.len();
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        GENERAL,
        "is anyone there?",
    );
    assert!(net.wire_since(wire).is_empty());
}
