//! Divergence: V447
//! Tells between worlds: a tell to `Name@world` reaches the other world's character as a tell from
//! the sender's address and stand-in id, the sender sees the echo a local tell gives, replies go
//! back by name and by id, and the errors are the local ones. With the network off a name holding
//! `@` is simply not found, as in ACE.
//! Fixture: two (or three) virtual-time worlds through the hub's routing (`super::Net`).

use dereth_network_protocol::standin::{is_stand_in, FIRST};
use dereth_network_protocol::Federation;
use dereth_protocol::comms::{
    CommunicationHearDirectSpeech, CommunicationWeenieError, CommunicationWeenieErrorWithString,
};
use empyrean_entity::enums::{ChatMessageType, WeenieError, WeenieErrorWithString};

use super::{seed, two_worlds, Net, TELL, TEXTBOX, WEENIE_ERROR, WEENIE_ERROR_STR};

fn tells(net: &mut Net, slug: &str, who: &str, mark: usize) -> Vec<CommunicationHearDirectSpeech> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode::<CommunicationHearDirectSpeech>())
        .collect()
}

/// Divergence: V447
#[test]
fn a_tell_to_an_address_reaches_the_other_world_and_the_sender_sees_the_echo() {
    let mut net = two_worlds(false);
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    let wire = net.wire.0.len();
    net.tell("aa", "Ann", "Bob@bb", "long time! still playing?");

    let got = tells(&mut net, "bb", "Bob", mb);
    assert_eq!(got.len(), 1, "Bob gets one tell");
    assert_eq!(got[0].sender_name, "Ann@aa");
    assert_eq!(got[0].message, "long time! still playing?");
    assert_eq!(got[0].target_id.0, 0x5000_0001, "to Bob");
    assert!(
        is_stand_in(got[0].sender_id.0),
        "the sender is shown under a stand-in id, which the client makes clickable"
    );
    assert_eq!(got[0].text_type, ChatMessageType::Tell.0);

    let ann = net.since("aa", "Ann", ma);
    assert_eq!(
        Net::lines(&ann),
        ["You tell Bob@bb, \"long time! still playing?\""],
        "Ann sees the echo once Bob's world has taken the tell"
    );
    let echo = ann
        .iter()
        .find(|g| g.kind == TEXTBOX)
        .expect("the echo")
        .plain::<dereth_protocol::comms::CommunicationTextboxString>();
    assert_eq!(echo.text_type, ChatMessageType::OutgoingTell.0);
    assert_eq!(
        net.wire_since(wire),
        [
            "aa -> hub: tell",
            "hub -> bb: tell",
            "bb -> hub: tell.result",
            "hub -> aa: tell.result"
        ]
    );
}

/// Divergence: V447
#[test]
fn a_reply_goes_back_by_name_and_by_the_stand_in_id() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb", "still playing?");
    let stand_in = tells(&mut net, "bb", "Bob", mb)[0].sender_id.0;

    // `@r ` expands in the client to `@tell Ann@aa, ...`: by name. Ann's settings are the
    // cautious ones, but she told Bob, so his answer reaches her.
    let ma = net.mark("aa", "Ann");
    net.tell("bb", "Bob", "Ann@aa", "yes! come say hi Sat");
    let got = tells(&mut net, "aa", "Ann", ma);
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].sender_name.as_str(), got[0].message.as_str()),
        ("Bob@bb", "yes! come say hi Sat")
    );
    assert!(is_stand_in(got[0].sender_id.0));

    // `@reply` with no expansion goes by the id the client stored.
    let ma = net.mark("aa", "Ann");
    let mb = net.mark("bb", "Bob");
    net.tell_id("bb", "Bob", stand_in, "see you then");
    let got = tells(&mut net, "aa", "Ann", ma);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].message, "see you then");
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", mb)),
        ["You tell Ann@aa, \"see you then\""]
    );
}

/// Divergence: V447
#[test]
fn a_stand_in_id_is_the_same_for_every_tell_from_one_identity() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb", "one");
    net.tell("aa", "Ann", "BOB@BB", "two");
    let got = tells(&mut net, "bb", "Bob", mb);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].sender_id, got[1].sender_id);
    assert_eq!(
        got[0].sender_id.0, FIRST,
        "the first identity this world gave an id to"
    );
    let saved = net.world("bb").ts.world.federation.store.stand_ins.clone();
    let json = serde_json::to_string(&saved).expect("saved");
    assert!(
        json.contains("ann@aa"),
        "the id is kept with the world's network record"
    );
}

/// Divergence: V447
#[test]
fn a_tell_nobody_can_take_answers_character_not_available() {
    let mut net = two_worlds(false);
    net.say("bb", "Bob", "@dn off");
    for to in ["Nobody@bb", "Bob@bb"] {
        // Nobody does not exist; Bob is off the network.
        let ma = net.mark("aa", "Ann");
        net.tell("aa", "Ann", to, "hello?");
        let ann = net.since("aa", "Ann", ma);
        assert_eq!(ann.len(), 1, "{to}: one answer");
        assert_eq!(ann[0].kind, WEENIE_ERROR);
        assert_eq!(
            ann[0].decode::<CommunicationWeenieError>().error_type,
            WeenieError::CharacterNotAvailable.0.cast_unsigned(),
            "{to}"
        );
    }
}

/// Divergence: V447
#[test]
fn a_character_off_the_network_is_told_how_to_join_and_nothing_leaves_the_world() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob])]);
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    let ma = net.mark("aa", "Ann");
    let wire = net.wire.0.len();
    net.tell("aa", "Ann", "Bob@bb", "hi");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["You are not on the Dereth Network. Type @dn on to join it."]
    );
    assert!(
        net.wire_since(wire).iter().all(|l| !l.contains("tell")),
        "nothing was sent"
    );
}

/// Divergence: V447
#[test]
fn a_remote_line_reaches_the_client_only_after_the_text_rules() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    net.tell(
        "aa",
        "Ann",
        "Bob@bb",
        "click <Tell:IIDString:1342177281:Admin>Admin<\\Tell> now",
    );
    let got = tells(&mut net, "bb", "Bob", mb);
    assert_eq!(
        got[0].message, "click Admin now",
        "the forged name link is gone"
    );
}

/// Divergence: V447
#[test]
fn an_away_recipient_still_gets_the_tell_and_the_sender_gets_the_away_message() {
    let mut net = two_worlds(false);
    {
        let w = net.world("bb");
        let bob = w.clients["Bob"];
        w.ts.send_game_action(
            bob,
            &dereth_protocol::comms::CommunicationSetAfkMessage {
                message: "back after dinner".into(),
            },
        );
        w.ts.send_game_action(
            bob,
            &dereth_protocol::comms::CommunicationSetAfkMode { afk: 1 },
        );
        net.advance(0.3);
    }
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    net.tell("aa", "Ann", "Bob@bb", "around?");
    assert_eq!(tells(&mut net, "bb", "Bob", mb).len(), 1);
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(Net::lines(&ann), ["You tell Bob@bb, \"around?\""]);
    let away = ann
        .iter()
        .find(|g| g.kind == WEENIE_ERROR_STR)
        .expect("the away message");
    let m = away.decode::<CommunicationWeenieErrorWithString>();
    assert_eq!(m.error_type, WeenieErrorWithString::AFK.0.cast_unsigned());
    assert_eq!(m.text, "Bob@bb is away: back after dinner");
}

/// Divergence: V447
#[test]
fn this_worlds_own_address_for_a_character_is_a_local_tell() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let cat = seed("cat", 0x5000_0002, "Cat");
    let mut net = Net::new(&[("aa", &[ann, cat])]);
    net.link("aa");
    net.enter("aa", ann);
    net.enter("aa", cat);
    net.say("aa", "Ann", "@dn on");
    let mc = net.mark("aa", "Cat");
    let wire = net.wire.0.len();
    net.tell("aa", "Ann", "Cat@aa", "same world");
    let got = tells(&mut net, "aa", "Cat", mc);
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].sender_name.as_str(), got[0].sender_id.0),
        ("Ann", 0x5000_0001)
    );
    assert!(net.wire_since(wire).iter().all(|l| !l.contains("tell")));
}

/// Divergence: V447
#[test]
fn a_tell_to_a_world_not_on_the_network_says_so() {
    let mut net = two_worlds(false);
    net.registry.0.insert("cc".into(), Federation::Off);
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Cid@cc", "hello");
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(ann[0].kind, WEENIE_ERROR);
    assert_eq!(Net::lines(&ann), ["cc isn't on the network."]);
}

/// Divergence: V447
#[test]
fn with_the_network_off_an_address_is_not_found_as_in_ace() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann])]);
    net.world("aa").ts.world.federation.settings.enabled = false;
    net.enter("aa", ann);
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "hi");
    let ann_got = net.since("aa", "Ann", ma);
    assert_eq!(ann_got.len(), 1);
    assert_eq!(
        ann_got[0].kind, WEENIE_ERROR,
        "ACE's answer for a name it does not know"
    );
    assert_eq!(net.world("aa").ts.world.federation.queued(), 0);
    net.say("aa", "Ann", "@dn on");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma))[0],
        "This world is not on the Dereth Network."
    );
}

/// Divergence: V447
#[test]
fn an_away_answer_is_always_shown_as_the_recipients_away_line() {
    let mut net = two_worlds(false);
    // bb is gone from the hub, so Ann's tell waits there; the answer comes from a world that
    // writes its own.
    net.unlink("bb");
    let mark = net.sent.0.len();
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "around?");
    let id = net
        .sent_since("aa", mark)
        .into_iter()
        .find_map(|m| match m {
            dereth_network_protocol::Message::Tell { id, .. } => Some(id),
            _ => None,
        })
        .expect("Ann's tell went to the hub");
    net.hub_hands(
        "aa",
        &dereth_network_protocol::Message::TellResult {
            id,
            outcome: dereth_network_protocol::Outcome::Afk,
            message: Some("Your account has been suspended. Visit evil.example to appeal.".into()),
        },
    );
    net.advance(0.3);
    let away = net
        .since("aa", "Ann", ma)
        .iter()
        .find(|g| g.kind == WEENIE_ERROR_STR)
        .expect("the away message")
        .decode::<CommunicationWeenieErrorWithString>();
    assert_eq!(
        away.text, "Bob@bb is away: Your account has been suspended. Visit evil.example to appeal.",
        "the line says whose away text it is"
    );
}

/// Divergence: V447
#[test]
fn once_every_stand_in_id_is_given_a_new_correspondent_is_not_shown_under_anothers() {
    let mut net = two_worlds(false);
    // The block with one id left.
    let store: empyrean_world::federation::NetworkStore = serde_json::from_value(serde_json::json!({
        "stand_ins": { "next": dereth_network_protocol::standin::LAST, "by_key": {}, "by_id": {} },
        "characters": serde_json::to_value(&net.world("bb").ts.world.federation.store.characters)
            .expect("records"),
    }))
    .expect("a store");
    net.world("bb").ts.world.federation.store = store;

    let mb = net.mark("bb", "Bob");
    net.world_sends(
        "aa",
        dereth_network_protocol::Message::Tell {
            id: "x1".into(),
            from: dereth_network_protocol::Address::parse("Eve@aa").expect("an address"),
            to: dereth_network_protocol::Address::parse("Bob@bb").expect("an address"),
            text: "I got the last id".into(),
            friend: false,
        },
    );
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "it's Ann");
    let got = tells(&mut net, "bb", "Bob", mb);
    assert_eq!(got.len(), 1, "Eve's tell only: {got:?}");
    assert_eq!(got[0].sender_name, "Eve@aa");
    assert_eq!(got[0].sender_id.0, dereth_network_protocol::standin::LAST);
    assert!(
        net.since("aa", "Ann", ma)
            .iter()
            .any(|g| g.kind == WEENIE_ERROR),
        "Ann is told Bob is not available"
    );
}

/// Divergence: V447
#[test]
fn a_world_is_held_to_its_pace_of_tells_to_one_character_even_by_a_hub_that_does_not() {
    let mut net = two_worlds(false);
    let (mb, mark) = (net.mark("bb", "Bob"), net.sent.0.len());
    for i in 0..25u32 {
        net.hub_hands(
            "bb",
            &dereth_network_protocol::Message::Tell {
                id: format!("h{i}"),
                from: dereth_network_protocol::Address::parse(&format!(
                    "{}@aa",
                    super::friends::minted(i)
                ))
                .expect("an address"),
                to: dereth_network_protocol::Address::parse("Bob@bb").expect("an address"),
                text: "hi".into(),
                friend: false,
            },
        );
    }
    net.advance(0.3);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let burst = dereth_network_protocol::limits::Limits::DEFAULT
        .tells_per_recipient
        .burst as usize;
    assert_eq!(tells(&mut net, "bb", "Bob", mb).len(), burst);
    let limited = net
        .sent_since("bb", mark)
        .into_iter()
        .filter(|m| {
            matches!(
                m,
                dereth_network_protocol::Message::TellResult {
                    outcome: dereth_network_protocol::Outcome::RateLimited,
                    ..
                }
            )
        })
        .count();
    assert_eq!(limited, 25 - burst, "the rest are answered rate_limited");
}

/// A tell to `to` from `from`, as the hub (or a world writing its own frames) carries it.
fn tell_msg(id: &str, from: &str, to: &str) -> dereth_network_protocol::Message {
    dereth_network_protocol::Message::Tell {
        id: id.to_owned(),
        from: dereth_network_protocol::Address::parse(from).expect("an address"),
        to: dereth_network_protocol::Address::parse(to).expect("an address"),
        text: "hi".into(),
        friend: false,
    }
}

/// The outcomes of the tell results `slug` sent the hub since `mark`.
fn outcomes(net: &Net, slug: &str, mark: usize) -> Vec<dereth_network_protocol::Outcome> {
    net.sent_since(slug, mark)
        .into_iter()
        .filter_map(|m| match m {
            dereth_network_protocol::Message::TellResult { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

/// Divergence: V447
#[test]
fn a_world_flooding_another_with_tells_for_ten_minutes_holds_up_neither_its_players_nor_its_answers(
) {
    let mut net = two_worlds(false);
    let mut n = 0u32;
    let mut at = 0.0;
    for until in [120.0, 300.0, 600.0] {
        // 48 tells a second, from invented names to invented names on bb, through the hub.
        while at < until {
            for _ in 0..12 {
                n += 1;
                let name = super::friends::minted(n);
                net.world_sends(
                    "aa",
                    tell_msg(
                        &format!("f{n}"),
                        &format!("{name}@aa"),
                        &format!("{name}@bb"),
                    ),
                );
            }
            net.advance(0.25);
            at += 0.25;
        }
        let queued = net.world("bb").ts.world.federation.queued();
        assert!(queued < 100, "bb's queue after {until} s: {queued}");
        let ma = net.mark("aa", "Ann");
        net.tell("bb", "Bob", "Ann@aa", "still there?");
        assert_eq!(
            tells(&mut net, "aa", "Ann", ma).len(),
            1,
            "Bob's own tell goes at once after {until} s"
        );
        let ma = net.mark("aa", "Ann");
        net.tell("aa", "Ann", "Bob@bb", "yes");
        net.advance(1.0);
        let lines = Net::lines(&net.since("aa", "Ann", ma));
        assert!(
            lines.iter().any(|l| l == "You tell Bob@bb, \"yes\""),
            "bb answers Ann's tell in time after {until} s: {lines:?}"
        );
    }
}

/// Divergence: V447
#[test]
fn a_hub_that_does_not_pace_a_flooding_world_holds_up_neither_this_worlds_tells_nor_others_answers()
{
    let ann = seed("ann", 0x5000_0001, "Ann");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let cid = seed("cid", 0x5000_0001, "Cid");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob]), ("cc", &[cid])]);
    for (slug, s, who) in [("aa", ann, "Ann"), ("bb", bob, "Bob"), ("cc", cid, "Cid")] {
        net.link(slug);
        net.enter(slug, s);
        net.say(slug, who, "@dn on");
    }
    // 52 tells a second for two minutes, handed to bb straight, past any pace of the hub's.
    let mut n = 0u32;
    for _ in 0..480 {
        for _ in 0..13 {
            n += 1;
            let name = super::friends::minted(n);
            net.hub_hands(
                "bb",
                &tell_msg(
                    &format!("h{n}"),
                    &format!("{name}@aa"),
                    &format!("{name}@bb"),
                ),
            );
        }
        net.advance(0.25);
    }
    assert!(
        net.world("bb").ts.world.federation.answers_owed()
            <= empyrean_world::federation::MAX_ANSWERS_PER_WORLD,
        "bb owes aa at most its share"
    );
    let ma = net.mark("aa", "Ann");
    net.tell("bb", "Bob", "Ann@aa", "still there?");
    assert_eq!(
        tells(&mut net, "aa", "Ann", ma).len(),
        1,
        "bb's own tell goes ahead of what it owes aa"
    );
    let mc = net.mark("cc", "Cid");
    net.tell("cc", "Cid", "Bob@bb", "hello from cc");
    net.advance(0.5);
    assert!(
        Net::lines(&net.since("cc", "Cid", mc))
            .iter()
            .any(|l| l == "You tell Bob@bb, \"hello from cc\""),
        "cc's answer does not wait behind aa's"
    );
}

/// Divergence: V447
#[test]
fn a_pace_refusal_tells_another_world_nothing_a_delivered_tell_would_not() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let cid = seed("cid", 0x5000_0002, "Cid");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob, cid])]);
    net.link("aa");
    net.link("bb");
    net.enter("bb", bob);
    net.enter("bb", cid);
    // Bob was on the network and left it; Cid never joined it.
    net.say("bb", "Bob", "@dn on");
    net.say("bb", "Bob", "@dn off");
    for target in ["Cid@bb", "Bob@bb", "Nobody@bb"] {
        let mark = net.sent.0.len();
        for i in 0..25u32 {
            net.hub_hands(
                "bb",
                &tell_msg(
                    &format!("{target}{i}"),
                    &format!("{}@aa", super::friends::minted(i)),
                    target,
                ),
            );
        }
        net.advance(0.3);
        let got = outcomes(&net, "bb", mark);
        assert_eq!(got.len(), 25);
        assert!(
            got.iter()
                .all(|o| *o == dereth_network_protocol::Outcome::Offline),
            "{target}: {got:?}"
        );
    }
}

/// Divergence: V447
#[test]
fn a_tell_from_a_world_out_of_new_names_is_answered_rate_limited_not_offline() {
    let mut net = two_worlds(false);
    // A hub whose per-recipient pace lets every tell through, so the new names alone hold them.
    let limits = dereth_network_protocol::limits::Limits {
        tells_per_recipient: dereth_network_protocol::limits::Rate {
            burst: 1e6,
            per_sec: 1e6,
        },
        ..dereth_network_protocol::limits::Limits::DEFAULT
    };
    net.hub_hands(
        "bb",
        &dereth_network_protocol::Message::Welcome {
            proto: dereth_network_protocol::wire::PROTOCOL,
            world: "bb".into(),
            hub: "a hub".into(),
            worlds: Vec::new(),
            rooms: vec!["global".into()],
            limits,
        },
    );
    let (mb, mark) = (net.mark("bb", "Bob"), net.sent.0.len());
    for i in 0..210u32 {
        net.hub_hands(
            "bb",
            &tell_msg(
                &format!("x{i}"),
                &format!("{}@aa", super::friends::minted(i)),
                "Bob@bb",
            ),
        );
    }
    net.advance(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let budget = empyrean_world::federation::NEW_STAND_INS_PER_WORLD.burst as usize;
    assert_eq!(tells(&mut net, "bb", "Bob", mb).len(), budget);
    let got = outcomes(&net, "bb", mark);
    let limited = got
        .iter()
        .filter(|o| **o == dereth_network_protocol::Outcome::RateLimited)
        .count();
    assert_eq!(
        (got.len(), limited),
        (210, 210 - budget),
        "Bob is online and reachable: the rest are held back, not offline"
    );
}

/// Divergence: V447
#[test]
fn a_tell_the_network_holds_back_tells_the_sender_it_is_the_worlds_share() {
    let mut net = two_worlds(false);
    net.unlink("bb");
    let mark = net.sent.0.len();
    net.tell("aa", "Ann", "Bob@bb", "hi");
    let id = net
        .sent_since("aa", mark)
        .into_iter()
        .find_map(|m| match m {
            dereth_network_protocol::Message::Tell { id, .. } => Some(id),
            _ => None,
        })
        .expect("the tell went to the hub");
    let ma = net.mark("aa", "Ann");
    net.hub_hands(
        "aa",
        &dereth_network_protocol::Message::TellResult {
            id,
            outcome: dereth_network_protocol::Outcome::RateLimited,
            message: None,
        },
    );
    net.advance(0.3);
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["The Dereth Network is holding back tells from this world to Bob@bb for now. Wait a moment and try again."]
    );
}

/// Divergence: V447
#[test]
fn an_away_text_longer_than_the_wire_takes_is_cut_to_it_and_the_sender_still_sees_the_away_line() {
    let mut net = two_worlds(false);
    {
        let w = net.world("bb");
        let bob = w.clients["Bob"];
        w.ts.send_game_action(
            bob,
            &dereth_protocol::comms::CommunicationSetAfkMessage {
                message: "z".repeat(3_000),
            },
        );
        w.ts.send_game_action(
            bob,
            &dereth_protocol::comms::CommunicationSetAfkMode { afk: 1 },
        );
        net.advance(0.3);
    }
    let mark = net.sent.0.len();
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "around?");
    let answer = net
        .sent_since("bb", mark)
        .into_iter()
        .find_map(|m| match m {
            dereth_network_protocol::Message::TellResult { message, .. } => message,
            _ => None,
        })
        .expect("bb answers with Bob's away text");
    assert_eq!(
        answer.chars().count(),
        dereth_network_protocol::text::MAX_TEXT_CHARS,
        "cut to the most a link message carries"
    );
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(Net::lines(&ann), ["You tell Bob@bb, \"around?\""]);
    let away = ann
        .iter()
        .find(|g| g.kind == WEENIE_ERROR_STR)
        .expect("the away message")
        .decode::<CommunicationWeenieErrorWithString>();
    assert!(
        away.text.starts_with("Bob@bb is away: zzz"),
        "{}",
        away.text
    );
}

/// Divergence: V447
#[test]
fn a_tell_a_hub_hands_on_from_this_worlds_own_address_is_refused_and_shown_to_no_one() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let dee = seed("dee", 0x5000_0002, "Dee");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let mut net = Net::new(&[("aa", &[ann, dee]), ("bb", &[bob])]);
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    net.enter("aa", dee);
    for who in ["Ann", "Dee"] {
        net.say("aa", who, "@dn on");
    }
    let md = net.mark("aa", "Dee");
    let mark = net.sent.0.len();
    net.hub_hands("aa", &tell_msg("h900", "Ann@aa", "Dee@aa"));
    net.advance(0.5);
    assert!(
        tells(&mut net, "aa", "Dee", md).is_empty(),
        "Dee is shown nothing"
    );
    assert_eq!(
        outcomes(&net, "aa", mark),
        [dereth_network_protocol::Outcome::Offline],
        "the world refuses it, as any tell it will not show"
    );
}
