//! Divergence: V449
//! The Global room and channels: a line in the TurbineChat room a world carries the Global room in
//! reaches every other world carrying it, as a room event under the speaker's address and stand-in
//! id; a channel is a tell address, joined by invitation, whose lines reach every other member.
//! Fixture: virtual-time worlds through the hub's routing (`super::Net`).

use dereth_network_protocol::standin::is_stand_in;
use dereth_primitives::NetQueue;
use dereth_protocol::comms::CommunicationHearDirectSpeech;
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};

use super::{seed, two_worlds, Net, TELL, TURBINE_CHAT};

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

fn post(net: &mut Net, slug: &str, who: &str, guid: u32, text: &str) {
    let w = net.world(slug);
    let id = w.clients[who];
    w.ts.send_message(
        id,
        NetQueue::Logon,
        &SendToRoomById {
            context: 7,
            room: 5,
            text: text.to_owned(),
            extra: ExtraBlob::Speaker {
                sender: guid,
                chat_type: 5,
            },
        },
    );
    net.advance(0.3);
}

/// Divergence: V449
#[test]
fn a_global_room_line_reaches_the_other_worlds_listeners_under_the_speakers_address() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    let wire = net.wire.0.len();
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        "Ballroom dance at the Arwic meeting hall, 8 pm!",
    );
    let got = room_events(&mut net, "bb", "Bob", mb);
    assert_eq!(
        got.len(),
        1,
        "Bob listens to Roleplay, which carries the Global room"
    );
    let (room, name, text, sender) = &got[0];
    assert_eq!(*room, 5);
    assert_eq!(name, "Ann@aa");
    assert_eq!(text, "Ballroom dance at the Arwic meeting hall, 8 pm!");
    assert!(
        is_stand_in(*sender),
        "the client checks this id against its squelch list"
    );
    assert_eq!(
        net.wire_since(wire),
        [
            "aa -> hub: room.say",
            "hub -> bb: room.line",
            "hub -> aa: room.result"
        ]
    );
}

/// Divergence: V449
#[test]
fn a_speaker_off_the_network_is_heard_only_on_its_own_world() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob])]);
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    net.enter("bb", bob);
    let mb = net.mark("bb", "Bob");
    post(&mut net, "aa", "Ann", 0x5000_0001, "local only");
    assert!(room_events(&mut net, "bb", "Bob", mb).is_empty());
}

/// Divergence: V449
#[test]
fn a_channel_is_joined_by_invitation_and_a_tell_to_it_reaches_the_other_members() {
    let mut net = two_worlds(false);
    // An invitation is shown as a first tell from the inviter would be.
    net.say("aa", "Ann", "@dn channel create oldguard");
    let mb = net.mark("bb", "Bob");
    net.say("aa", "Ann", "@dn channel invite oldguard Bob@bb");
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", mb)),
        ["Ann@aa invites you to the channel oldguard@channel. Type @dn channel join oldguard to join it."]
    );
    net.say("bb", "Bob", "@dn channel join oldguard");

    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    net.tell("bb", "Bob", "oldguard@channel", "anyone around tonight?");
    let got: Vec<CommunicationHearDirectSpeech> = net
        .since("aa", "Ann", ma)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode())
        .collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].sender_name, "oldguard@channel");
    assert_eq!(got[0].message, "Bob@bb: anyone around tonight?");
    assert!(
        is_stand_in(got[0].sender_id.0),
        "a reply goes back to the channel"
    );
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", mb)),
        ["You tell oldguard@channel, \"anyone around tonight?\""]
    );

    // A reply by id to the channel's stand-in goes to the channel.
    let mb = net.mark("bb", "Bob");
    net.tell_id("aa", "Ann", got[0].sender_id.0, "I am!");
    let back: Vec<CommunicationHearDirectSpeech> = net
        .since("bb", "Bob", mb)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode())
        .collect();
    assert_eq!(back[0].message, "Ann@aa: I am!");
}

/// Divergence: V449
#[test]
fn a_tell_to_a_channel_from_a_non_member_is_refused() {
    let mut net = two_worlds(false);
    net.say("aa", "Ann", "@dn channel create oldguard");
    let mb = net.mark("bb", "Bob");
    net.tell("bb", "Bob", "oldguard@channel", "let me in");
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", mb)),
        ["You are not a member of oldguard@channel."]
    );
}

fn line_from(name: &str) -> dereth_network_protocol::Message {
    dereth_network_protocol::Message::RoomLine {
        room: "global".into(),
        from: dereth_network_protocol::Address::parse(name).expect("an address"),
        text: "free gold at the bank".into(),
        via: None,
    }
}

/// Divergence: V449
#[test]
fn a_room_line_nobody_here_hears_gives_its_speaker_no_stand_in_id() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut bob = seed("bob", 0x5000_0001, "Bob");
    bob.listens = &[];
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob])]);
    net.link("aa");
    net.link("bb");
    net.enter("bb", bob);
    let before = net.world("bb").ts.world.federation.store.stand_ins.len();
    net.hub_hands("bb", &line_from("Eve@aa"));
    assert_eq!(
        net.world("bb").ts.world.federation.store.stand_ins.len(),
        before,
        "Bob does not listen to the room"
    );
}

/// Divergence: V449
#[test]
fn a_world_inventing_names_brings_only_so_many_new_stand_in_ids() {
    let mut net = two_worlds(false);
    // A hub whose room pace lets every line through, so the stand-in ids alone hold them.
    roomy_welcome(&mut net, "bb");
    let mb = net.mark("bb", "Bob");
    for i in 0..300u32 {
        net.hub_hands(
            "bb",
            &line_from(&format!("{}@aa", super::friends::minted(i))),
        );
    }
    net.advance(0.3);
    let shown = room_events(&mut net, "bb", "Bob", mb).len();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = empyrean_world::federation::NEW_STAND_INS_PER_WORLD.burst as usize;
    assert_eq!(shown, burst, "the rest are not shown");
    assert_eq!(
        net.world("bb").ts.world.federation.store.stand_ins.len(),
        burst
    );
    // A speaker who already has an id is still heard.
    let mb = net.mark("bb", "Bob");
    net.hub_hands(
        "bb",
        &line_from(&format!("{}@aa", super::friends::minted(0))),
    );
    net.advance(0.3);
    assert_eq!(room_events(&mut net, "bb", "Bob", mb).len(), 1);
}

/// Divergence: V449
#[test]
fn a_room_the_operator_has_turned_off_carries_no_line_from_another_world() {
    let mut net = two_worlds(false);
    let w = &net.world("bb").ts.world;
    assert!(empyrean_world::managers::property_manager::modify_bool(
        w,
        "chat_disable_roleplay",
        true
    ));
    let mb = net.mark("bb", "Bob");
    net.hub_hands("bb", &line_from("Eve@aa"));
    net.advance(0.3);
    assert!(room_events(&mut net, "bb", "Bob", mb).is_empty());
}

/// Divergence: V449
#[test]
fn a_character_too_new_for_the_operator_sends_no_room_line_and_no_channel_request() {
    let mut net = two_worlds(false);
    net.world("aa")
        .ts
        .world
        .federation
        .settings
        .min_character_age_secs = 3_600;
    let wire = net.wire.0.len();
    let ma = net.mark("aa", "Ann");
    post(&mut net, "aa", "Ann", 0x5000_0001, "hello, other worlds");
    net.say("aa", "Ann", "@dn channel create newbies");
    assert!(
        net.wire_since(wire)
            .iter()
            .all(|l| !l.starts_with("aa -> hub: room.say") && !l.starts_with("aa -> hub: channel")),
        "{:?}",
        net.wire_since(wire)
    );
    let lines = Net::lines(&net.since("aa", "Ann", ma));
    assert!(
        lines.iter().any(|l| l.contains("too new")),
        "Ann is told why: {lines:?}"
    );
}

/// Divergence: V449
#[test]
fn a_line_the_hub_holds_back_tells_its_speaker_it_is_the_worlds_share_of_the_room() {
    let mut net = two_worlds(false);
    // Other speakers of aa's spend the world's share of the room first.
    for i in 0..20u32 {
        net.world_sends(
            "aa",
            dereth_network_protocol::Message::RoomSay {
                id: format!("s{i}"),
                room: "global".into(),
                from: dereth_network_protocol::Address::parse(&format!(
                    "{}@aa",
                    super::friends::minted(i)
                ))
                .expect("an address"),
                text: "an event tonight".into(),
                via: None,
            },
        );
    }
    let ma = net.mark("aa", "Ann");
    post(&mut net, "aa", "Ann", 0x5000_0001, "mine too");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["Your line did not reach the other worlds: the Dereth Network is holding back this world's lines for now."]
    );
}

/// `slug` welcomed again by a hub whose per-world room pace lets any number of lines through.
fn roomy_welcome(net: &mut Net, slug: &str) {
    let limits = dereth_network_protocol::limits::Limits {
        room_lines_per_world: dereth_network_protocol::limits::Rate {
            burst: 1e6,
            per_sec: 1e6,
        },
        ..dereth_network_protocol::limits::Limits::DEFAULT
    };
    net.hub_hands(
        slug,
        &dereth_network_protocol::Message::Welcome {
            proto: dereth_network_protocol::wire::PROTOCOL,
            world: slug.to_owned(),
            hub: "a hub".into(),
            worlds: Vec::new(),
            rooms: vec!["global".into()],
            limits,
        },
    );
}

/// Divergence: V449
#[test]
fn a_world_is_held_to_its_pace_of_room_lines_even_by_a_hub_that_does_not() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    for _ in 0..15 {
        net.hub_hands("bb", &line_from("Eve@aa"));
    }
    net.advance(0.3);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = dereth_network_protocol::limits::Limits::DEFAULT
        .room_lines_per_world
        .burst as usize;
    assert_eq!(room_events(&mut net, "bb", "Bob", mb).len(), burst);
}

/// Divergence: V449
#[test]
fn channel_lines_and_invitations_from_a_hub_that_does_not_pace_them_are_held_to_a_tells_pace() {
    let mut net = two_worlds(false);
    let addr = |s: &str| dereth_network_protocol::Address::parse(s).expect("an address");
    let burst = dereth_network_protocol::limits::Limits::DEFAULT
        .tells_per_recipient
        .burst;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = burst as usize;
    let ma = net.mark("aa", "Ann");
    for i in 0..60u32 {
        net.hub_hands(
            "aa",
            &dereth_network_protocol::Message::ChannelLine {
                channel: "floodchannel".into(),
                from: addr(&format!("{}@bb", super::friends::minted(i))),
                to: addr("Ann@aa"),
                text: "buy gold".into(),
            },
        );
    }
    net.advance(0.5);
    let lines = net
        .since("aa", "Ann", ma)
        .iter()
        .filter(|g| g.kind == TELL)
        .count();
    assert_eq!(lines, burst, "bb's per-recipient burst, as for its tells");
    // A minute on, the pace has refilled; invitations take from the same.
    net.advance(60.0);
    let ma = net.mark("aa", "Ann");
    for i in 0..60u32 {
        net.hub_hands(
            "aa",
            &dereth_network_protocol::Message::ChannelInvite {
                channel: format!("spam{i}"),
                by: addr(&format!("{}@bb", super::friends::minted(i))),
                to: addr("Ann@aa"),
            },
        );
    }
    net.advance(0.5);
    let invitations = Net::lines(&net.since("aa", "Ann", ma))
        .iter()
        .filter(|l| l.contains("invites you"))
        .count();
    assert_eq!(invitations, burst);
}

/// Divergence: V449
#[test]
fn dng_and_dn_global_reach_the_other_worlds_and_show_here_in_the_carrying_room() {
    let mut net = two_worlds(false);
    for (line, text) in [
        ("@dng dance at 8, Arwic hall", "dance at 8, Arwic hall"),
        ("@dn global bring snacks", "bring snacks"),
    ] {
        let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
        let wire = net.wire.0.len();
        net.say("aa", "Ann", line);
        let there = room_events(&mut net, "bb", "Bob", mb);
        assert_eq!(there.len(), 1, "{line}: Bob listens to Roleplay");
        let (room, name, got, sender) = &there[0];
        assert_eq!((*room, name.as_str(), got.as_str()), (5, "Ann@aa", text));
        assert!(is_stand_in(*sender), "{line}");
        assert_eq!(
            room_events(&mut net, "aa", "Ann", ma),
            [(5, "Ann".to_owned(), text.to_owned(), 0x5000_0001)],
            "{line}: shown here as Roleplay's own line from Ann"
        );
        assert_eq!(
            net.wire_since(wire),
            [
                "aa -> hub: room.say",
                "hub -> bb: room.line",
                "hub -> aa: room.result"
            ],
            "{line}"
        );
    }
}

/// Divergence: V449
#[test]
fn dng_on_a_world_that_carries_no_global_room_says_so_and_sends_nothing() {
    let mut net = two_worlds(false);
    // The world's operator carries the Global room nowhere.
    let asked = empyrean_world::federation::commands::admin(
        &mut net.world("aa").ts.world,
        &["global".into(), "off".into()],
    );
    assert!(asked.starts_with("Asking"), "{asked}");
    net.pump();
    for line in ["@dng anyone?", "@dn global anyone?"] {
        let ma = net.mark("aa", "Ann");
        let wire = net.wire.0.len();
        net.say("aa", "Ann", line);
        let got = net.since("aa", "Ann", ma);
        assert_eq!(
            Net::lines(&got),
            ["This world does not carry the Global room."],
            "{line}"
        );
        assert!(got.iter().all(|g| g.kind != TURBINE_CHAT), "{line}");
        assert!(net.wire_since(wire).is_empty(), "{line}");
    }
}

/// Divergence: V449
#[test]
fn dng_is_held_to_the_rooms_rules() {
    // Off the network: refused, and not said at all.
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann])]);
    net.link("aa");
    net.enter("aa", ann);
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dng hello");
    let got = net.since("aa", "Ann", ma);
    assert_eq!(
        Net::lines(&got),
        ["You are not on the Dereth Network. Type @dn on to join it."]
    );
    assert!(got.iter().all(|g| g.kind != TURBINE_CHAT));

    // Too new for the operator: said here, not sent.
    let mut net = two_worlds(false);
    net.world("aa")
        .ts
        .world
        .federation
        .settings
        .min_character_age_secs = 3_600;
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    net.say("aa", "Ann", "@dng hello, other worlds");
    assert!(room_events(&mut net, "bb", "Bob", mb).is_empty());
    let lines = Net::lines(&net.since("aa", "Ann", ma));
    assert!(lines.iter().any(|l| l.contains("too new")), "{lines:?}");

    // The room turned off here: nothing goes anywhere.
    let mut net = two_worlds(false);
    assert!(empyrean_world::managers::property_manager::modify_bool(
        &net.world("aa").ts.world,
        "chat_disable_roleplay",
        true
    ));
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    let wire = net.wire.0.len();
    net.say("aa", "Ann", "@dng hello");
    assert!(room_events(&mut net, "bb", "Bob", mb).is_empty());
    assert!(room_events(&mut net, "aa", "Ann", ma).is_empty());
    assert!(net.wire_since(wire).is_empty());

    // A listener who squelches the speaker's world is not shown it.
    let mut net = two_worlds(false);
    net.say("bb", "Bob", "@dn squelch aa");
    let mb = net.mark("bb", "Bob");
    net.say("aa", "Ann", "@dng hello");
    assert!(room_events(&mut net, "bb", "Bob", mb).is_empty());

    // The world's share of the room spent: the speaker is told.
    let mut net = two_worlds(false);
    for i in 0..20u32 {
        net.world_sends(
            "aa",
            dereth_network_protocol::Message::RoomSay {
                id: format!("s{i}"),
                room: "global".into(),
                from: dereth_network_protocol::Address::parse(&format!(
                    "{}@aa",
                    super::friends::minted(i)
                ))
                .expect("an address"),
                text: "an event tonight".into(),
                via: None,
            },
        );
    }
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dng mine too");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["Your line did not reach the other worlds: the Dereth Network is holding back this world's lines for now."]
    );
}
