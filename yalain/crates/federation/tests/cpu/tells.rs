//! Tells to and from other worlds.

use dereth_network_protocol::limits::Limits;
use dereth_network_protocol::{Message, Outcome};
use yalain_federation::{Ended, State, TellEnd, UNAVAILABLE};

use crate::{addr, World};

/// The tell a world sent: its id, sender, recipient, text and friend flag.
fn sent_tell(frames: &[Message]) -> (String, String, String, String, bool) {
    let tells: Vec<_> = frames
        .iter()
        .filter_map(|m| match m {
            Message::Tell {
                id,
                from,
                to,
                text,
                friend,
            } => Some((
                id.clone(),
                from.to_string(),
                to.to_string(),
                text.clone(),
                *friend,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(tells.len(), 1, "one tell: {frames:?}");
    tells[0].clone()
}

fn ended(w: &World) -> Vec<(u64, Ended, Option<String>)> {
    w.service
        .ended
        .iter()
        .map(|(id, TellEnd { ended, line })| (*id, *ended, line.clone()))
        .collect()
}

/// A tell from a character on the network goes to the hub under its address, saying whether the
/// teller lists the recipient, and ends as the recipient's world answers: delivered, away (with
/// the line the teller's world writes around the away text), ignored (which the teller sees as
/// delivered), offline, the world unreachable, the world not on the network, or held back.
/// Behaviour: server.network.tell-sent-and-answered
#[test]
fn a_tell_to_another_world_goes_to_the_hub_and_ends_as_its_answer_says() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Bob@coldeve");
    w.link(&[("coldeve", true)]);
    w.frames();
    let cases: &[(Outcome, Option<&str>, Ended, Option<&str>)] = &[
        (Outcome::Delivered, None, Ended::Delivered, None),
        (
            Outcome::Afk,
            Some("gone fishing"),
            Ended::Away,
            Some("Bob@coldeve is away: gone fishing"),
        ),
        (
            Outcome::Afk,
            Some("Bob@coldeve is away: <tell:x>brb"),
            Ended::Away,
            Some("Bob@coldeve is away: brb"),
        ),
        (Outcome::Squelched, None, Ended::Squelched, None),
        (Outcome::Offline, None, Ended::Offline, None),
        (
            Outcome::Unavailable,
            None,
            Ended::Unavailable,
            Some("The Dereth Network can't reach coldeve right now."),
        ),
        (
            Outcome::NotOnNetwork,
            None,
            Ended::NotOnNetwork,
            Some("coldeve isn't on the network."),
        ),
        (
            Outcome::RateLimited,
            None,
            Ended::RateLimited,
            Some(
                "The Dereth Network is holding back tells from this world to Bob@coldeve for now. \
                 Wait a moment and try again.",
            ),
        ),
    ];
    for (n, (outcome, message, want, line)) in cases.iter().enumerate() {
        let tell = n as u64 + 1;
        w.tell(tell, "ann", "Bob@coldeve", "still playing?");
        let (id, from, to, text, friend) = sent_tell(&w.frames());
        assert_eq!(
            (from.as_str(), to.as_str(), text.as_str(), friend),
            ("Ann@dawnsong", "Bob@coldeve", "still playing?", true),
            "the teller's own address, as the world writes its name"
        );
        assert!(id.starts_with("t1790000000-"), "{id}");
        assert!(w.service.ended.is_empty(), "nothing ends before the answer");
        w.hub(&Message::TellResult {
            id,
            outcome: *outcome,
            message: message.map(str::to_owned),
        });
        assert_eq!(
            ended(&w),
            vec![(tell, *want, line.map(str::to_owned))],
            "{outcome:?}"
        );
        assert_eq!(
            want.reached(),
            matches!(
                outcome,
                Outcome::Delivered | Outcome::Afk | Outcome::Squelched
            )
        );
        assert_eq!(want.busy(), *outcome == Outcome::RateLimited);
        w.service.ended.clear();
        w.advance(10.0);
    }
    // A friend flag is false for a recipient the teller does not list.
    w.tell(99, "Ann", "Cara@coldeve", "hi");
    assert!(!sent_tell(&w.frames()).4);
}

/// A tell is refused before it is sent, with the line its teller is shown: from a world off the
/// network; from a character off the network; to a world or character the operator blocks (as to a
/// character not available, with no line); and past the teller's own pace of a burst of ten, then
/// one every two seconds.
/// Behaviour: server.network.tell-refused-before-sending
#[test]
fn a_tell_is_refused_here_for_the_world_the_teller_the_blocks_and_the_tellers_pace() {
    let mut off = World::with(false);
    off.on_and_present("Ann");
    off.tell(1, "Ann", "Bob@coldeve", "hi");
    assert_eq!(
        ended(&off),
        vec![(
            1,
            Ended::WorldOff,
            Some("This world is not on the Dereth Network.".into())
        )]
    );

    let mut w = World::new();
    w.character("Cara");
    w.enter("Cara", State::Available);
    w.on_and_present("Ann");
    w.link(&[("coldeve", true), ("frost", true)]);
    w.frames();
    w.tell(1, "Cara", "Bob@coldeve", "hi");
    assert_eq!(
        ended(&w),
        vec![(
            1,
            Ended::NotOn,
            Some("You are not on the Dereth Network. Say @dn on to join it.".into())
        )]
    );
    w.service.ended.clear();
    w.operator(&["block", "frost"]);
    w.operator(&["block", "Mal@coldeve"]);
    w.tell(2, "Ann", "Eve@frost", "hi");
    w.tell(3, "Ann", "mal@COLDEVE", "hi");
    assert_eq!(
        ended(&w),
        vec![(2, Ended::Offline, None), (3, Ended::Offline, None)]
    );
    assert!(w.frames().is_empty(), "nothing refused goes to the hub");
    w.service.ended.clear();
    for n in 0..10 {
        w.tell(10 + n, "Ann", "Bob@coldeve", "hi");
    }
    assert!(w.service.ended.is_empty(), "a burst of ten goes");
    w.tell(20, "Ann", "Bob@coldeve", "hi");
    assert_eq!(
        ended(&w),
        vec![(
            20,
            Ended::TooQuickly,
            Some(
                "You are sending to the Dereth Network too quickly. Wait a moment and try again."
                    .into()
            )
        )]
    );
    w.advance(2.0);
    w.tell(21, "Ann", "Bob@coldeve", "hi");
    assert_eq!(w.service.ended.len(), 1, "one more after two seconds");
    assert_eq!(
        w.frames()
            .iter()
            .filter(|m| matches!(m, Message::Tell { .. }))
            .count(),
        11
    );
}

/// A tell sent while the link is down waits for it, and goes once the hub has welcomed the world;
/// one that waits past the hub's hold (60 seconds) ends with the network unavailable. A tell the
/// link took and lost before its answer is sent again first on the next link, and one the hub has
/// not answered within 100 seconds ends with the recipient's world out of reach.
/// Behaviour: server.network.tells-wait-for-the-link
#[test]
fn tells_wait_for_the_link_for_a_minute_and_for_an_answer_for_a_hundred_seconds() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.tell(1, "Ann", "Bob@coldeve", "first");
    w.advance(30.0);
    w.tell(2, "Ann", "Bob@coldeve", "second");
    w.advance(29.0);
    assert!(w.service.ended.is_empty());
    w.advance(1.0);
    assert_eq!(
        ended(&w),
        vec![(1, Ended::NetworkUnavailable, Some(UNAVAILABLE.into()))],
        "the first waited sixty seconds"
    );
    let after = w.link(&[("coldeve", true)]);
    let (id, _, _, text, _) = sent_tell(&after);
    assert_eq!(text, "second", "the second goes once welcomed");
    // The link takes it and goes before an answer: the next link sends it again, first.
    w.fed.on_link_down(&mut w.service, w.t);
    w.advance(5.0);
    let again = w.link(&[("coldeve", true)]);
    assert_eq!(sent_tell(&again).0, id, "the same tell, under the same id");
    assert!(
        matches!(again.first(), Some(Message::Tell { .. })),
        "{again:?}"
    );
    w.advance(99.0);
    assert_eq!(w.service.ended.len(), 1, "not yet");
    w.advance(1.0);
    assert_eq!(
        ended(&w)[1],
        (
            2,
            Ended::NetworkUnavailable,
            Some("The Dereth Network can't reach coldeve right now.".into())
        ),
        "an unanswered tell says the recipient's world cannot be reached"
    );
    // A late answer changes nothing.
    w.hub(&Message::TellResult {
        id,
        outcome: Outcome::Delivered,
        message: None,
    });
    assert_eq!(w.service.ended.len(), 2);
    // The hub's own give-up, when it gives a longer one, is the world's.
    let mut slow = World::new();
    slow.on_and_present("Ann");
    let limits = Limits {
        result_timeout_secs: 150.0,
        ..Limits::DEFAULT
    };
    slow.link_with(&[("coldeve", true)], limits);
    slow.tell(1, "Ann", "Bob@coldeve", "hi");
    slow.frames();
    slow.advance(120.0);
    assert!(slow.service.ended.is_empty());
    slow.advance(30.0);
    assert_eq!(slow.service.ended.len(), 1);
}

/// A tell from another world is shown to a present character on the network after the text
/// rules (markup out, line breaks as spaces, cut at 255 characters), and answered delivered; to
/// one away or not to be disturbed it is shown too, and answered with its away text, or the
/// default for its state.
/// Behaviour: server.network.tell-received
#[test]
fn a_tell_from_another_world_is_shown_after_the_text_rules_and_answered_at_once() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.character("Dee");
    w.set_on("Dee", true);
    w.enter("Dee", State::Away(None));
    w.character("Fen");
    w.set_on("Fen", true);
    w.enter("Fen", State::DoNotDisturb(Some("raiding".into())));
    w.character("Gus");
    w.set_on("Gus", true);
    w.enter("Gus", State::DoNotDisturb(None));
    w.link(&[("coldeve", true)]);
    w.frames();
    let long = "x".repeat(400);
    w.told(
        "h1",
        "Bob@coldeve",
        "ann@dawnsong",
        "hi <Tell:IIDString:1:Admin>Admin<\\Tell>\nthere",
    );
    w.told("h2", "Bob@coldeve", "Ann@dawnsong", &long);
    w.told("h3", "Bob@coldeve", "Dee@dawnsong", "you there?");
    w.told("h4", "Bob@coldeve", "Fen@dawnsong", "you there?");
    w.told("h5", "Bob@coldeve", "Gus@dawnsong", "you there?");
    let shown: Vec<_> = w
        .service
        .tells
        .iter()
        .map(|(to, from, text)| (to.as_str(), from.to_string(), text.len()))
        .collect();
    assert_eq!(
        shown,
        vec![
            ("Ann", "Bob@coldeve".to_owned(), 14),
            ("Ann", "Bob@coldeve".to_owned(), 255),
            ("Dee", "Bob@coldeve".to_owned(), 10),
            ("Fen", "Bob@coldeve".to_owned(), 10),
            ("Gus", "Bob@coldeve".to_owned(), 10),
        ]
    );
    assert_eq!(w.service.tells[0].2, "hi Admin there");
    assert_eq!(
        w.answers(),
        vec![
            ("h1".into(), Outcome::Delivered, None),
            ("h2".into(), Outcome::Delivered, None),
            (
                "h3".into(),
                Outcome::Afk,
                Some("Away from keyboard.".into())
            ),
            ("h4".into(), Outcome::Afk, Some("raiding".into())),
            (
                "h5".into(),
                Outcome::Afk,
                Some("Not to be disturbed.".into())
            ),
        ]
    );
}

/// A tell from another world is not shown, and answered alike (`offline`), to a character not
/// present, one off the network, a name this world does not have, from a world or a character the
/// operator blocks, and from this world's own address; to a character that ignores the teller or
/// its world it is answered `squelched`. One world's tells to one character are held to a burst of
/// twenty, then one a second (`rate_limited`), counting only those that would be shown.
/// Behaviour: server.network.tell-received-refused
#[test]
fn a_tell_from_another_world_is_refused_alike_and_held_to_its_pace() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.character("Cara");
    w.enter("Cara", State::Available);
    w.character("Dee");
    w.set_on("Dee", true);
    w.on_and_present("Eli");
    w.service
        .ignored
        .insert("eli".into(), vec!["bob@coldeve".into(), "frost".into()]);
    w.link(&[("coldeve", true), ("frost", true), ("ember", true)]);
    w.frames();
    w.operator(&["block", "ember"]);
    w.operator(&["block", "Mal@coldeve"]);
    w.frames();
    for (id, from, to) in [
        ("a", "Bob@coldeve", "Cara@dawnsong"),
        ("b", "Bob@coldeve", "Dee@dawnsong"),
        ("c", "Bob@coldeve", "Nobody@dawnsong"),
        ("d", "Hal@ember", "Ann@dawnsong"),
        ("e", "Mal@coldeve", "Ann@dawnsong"),
        ("f", "Cara@dawnsong", "Ann@dawnsong"),
        ("g", "Bob@coldeve", "Eli@dawnsong"),
        ("h", "Eve@frost", "Eli@dawnsong"),
    ] {
        w.told(id, from, to, "hi");
    }
    assert!(w.service.tells.is_empty(), "{:?}", w.service.tells);
    let answers: Vec<_> = w.answers().into_iter().map(|(i, o, _)| (i, o)).collect();
    let want: Vec<(String, Outcome)> = ["a", "b", "c", "d", "e", "f"]
        .into_iter()
        .map(|i| (i.to_owned(), Outcome::Offline))
        .chain(["g", "h"].map(|i| (i.to_owned(), Outcome::Squelched)))
        .collect();
    let mut sorted = answers.clone();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sorted, want);
    let mut shown = 0;
    let mut first_refused = None;
    for n in 0..25 {
        w.told(&format!("p{n}"), "Bob@coldeve", "Ann@dawnsong", "hi");
        for (_, outcome, _) in w.answers() {
            match outcome {
                Outcome::Delivered => shown += 1,
                Outcome::RateLimited => {
                    first_refused.get_or_insert(n);
                }
                o => panic!("{o:?}"),
            }
        }
    }
    assert_eq!((shown, first_refused), (20, Some(20)));
    w.advance(1.0);
    w.told("q", "Bob@coldeve", "Ann@dawnsong", "hi");
    assert_eq!(
        w.answers()[0].1,
        Outcome::Delivered,
        "one more a second later"
    );
    // Another world's tells to the same character are held apart.
    w.told("r", "Eve@frost", "Ann@dawnsong", "hi");
    assert_eq!(w.answers()[0].1, Outcome::Delivered);
}

/// A world owed a hundred answers already gets no answer to its next tell, which is not shown:
/// the hub answers it for this world. Answers the hub stopped waiting for (after 30 seconds) are
/// dropped, and the world is answered again.
/// Behaviour: server.network.answers-owed-capped
#[test]
fn a_world_owed_a_hundred_answers_is_answered_no_more_until_it_is_owed_fewer() {
    let mut w = World::new();
    w.on_and_present("Ann");
    // A hub whose pace lets nothing through, so every answer stays owed.
    let limits = Limits {
        messages_per_world: dereth_network_protocol::limits::Rate {
            burst: 1.0,
            per_sec: 0.0,
        },
        tells_per_recipient: dereth_network_protocol::limits::Rate {
            burst: 1_000.0,
            per_sec: 0.0,
        },
        ..Limits::DEFAULT
    };
    w.link_with(&[("coldeve", true)], limits);
    w.frames();
    for n in 0..101 {
        w.told(&format!("t{n}"), "Bob@coldeve", "Ann@dawnsong", "hi");
    }
    assert_eq!(w.fed.answers_owed(), 100);
    assert_eq!(
        w.service.tells.len(),
        100,
        "the hundred-and-first is not shown"
    );
    w.advance(30.0);
    assert_eq!(w.fed.answers_owed(), 0, "the hub stopped waiting for them");
    w.told("u", "Bob@coldeve", "Ann@dawnsong", "hi");
    assert_eq!(w.fed.answers_owed(), 1);
}

/// A character younger than the operator asks sends no tell to another world and is told why; it
/// sends when it is no longer.
/// Behaviour: server.network.too-new-sends-nothing
#[test]
fn a_character_too_new_sends_no_tell_to_another_world() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[("coldeve", true)]);
    w.frames();
    w.fed
        .set_too_new(std::collections::HashSet::from(["ann".to_owned()]));
    w.tell(1, "Ann", "Bob@coldeve", "hi");
    assert_eq!(
        ended(&w),
        vec![(
            1,
            Ended::TooNew,
            Some("This character is too new to send to the Dereth Network yet.".into())
        )]
    );
    assert!(w.frames().is_empty(), "nothing goes to the hub");
    w.fed.set_too_new(std::collections::HashSet::new());
    w.tell(2, "Ann", "Bob@coldeve", "hi");
    assert_eq!(w.frames().len(), 1, "now it goes");
}

/// A world with `Ann` present on the network, `Dee` away on it and linked by herself, `Cara` away
/// on it and never linked, `Eve` away off it, `Gil` in the world and linked, the link up.
fn with_away_linked() -> World {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.character("Cara");
    w.set_on("Cara", true);
    w.character("Dee");
    w.set_on("Dee", true);
    w.character("Eve");
    w.on_and_present("Gil");
    w.link(&[("coldeve", true)]);
    let t = w.t;
    for name in ["Dee", "Gil"] {
        let id = w.ids[name];
        assert_eq!(w.fed.link_person(&mut w.store, t, id, name), None);
    }
    w.frames();
    w
}

/// A tell within the world to a character away, on the network and linked by itself goes to the hub
/// under the character's own address, so it may reach its player's Discord, and ends as the hub
/// answers it; from a teller on the network, while the link is up. Every other tell of this world's
/// own stays the world's: to one never linked, off the network, in the world, unknown, or itself.
/// Behaviour: server.network.tell-away-linked
#[test]
fn a_tell_within_the_world_to_an_away_linked_character_goes_to_the_hub() {
    let mut w = with_away_linked();
    assert_eq!(
        w.fed.away_linked(&w.service, "ann", "DEE"),
        Some(addr("Dee@dawnsong"))
    );
    w.tell(1, "Ann", "Dee@dawnsong", "you there?");
    let (id, from, to, text, friend) = sent_tell(&w.frames());
    assert_eq!(
        (from.as_str(), to.as_str(), text.as_str(), friend),
        ("Ann@dawnsong", "Dee@dawnsong", "you there?", false)
    );
    assert!(w.service.ended.is_empty(), "nothing ends before the answer");
    w.hub(&Message::TellResult {
        id,
        outcome: Outcome::Offline,
        message: None,
    });
    assert_eq!(ended(&w), vec![(1, Ended::Offline, None)]);

    for (n, to) in ["Cara", "Eve", "Gil", "Nobody", "Ann"].iter().enumerate() {
        assert_eq!(w.fed.away_linked(&w.service, "Ann", to), None, "{to}");
        w.service.ended.clear();
        w.tell(n as u64 + 2, "Ann", &format!("{to}@dawnsong"), "hi");
        assert_eq!(
            ended(&w),
            vec![(n as u64 + 2, Ended::Offline, None)],
            "{to}"
        );
        assert!(w.frames().is_empty(), "{to}");
    }

    let t = w.t;
    let eve = w.ids["Eve"];
    w.fed
        .set_on(&mut w.store, &w.service, eve, "Eve", true)
        .expect("kept");
    w.fed
        .set_on(&mut w.store, &w.service, eve, "Eve", false)
        .expect("kept");
    assert_eq!(
        w.fed.away_linked(&w.service, "Eve", "Dee"),
        None,
        "a teller off the network"
    );
    w.frames();
    w.fed.on_link_down(&mut w.service, t);
    assert_eq!(
        w.fed.away_linked(&w.service, "Ann", "Dee"),
        None,
        "the link down"
    );
}

/// An away, linked character's reply from elsewhere reaches its world as a tell from its own
/// address, and is shown and answered as any tell, unless the recipient ignores it. A tell from this
/// world's own address that no reply can be, from a character in the world, never linked, off the
/// network or unknown, is answered `offline`.
/// Behaviour: server.network.tell-away-linked
#[test]
fn a_reply_from_an_away_linked_character_is_shown_as_any_tell() {
    let mut w = with_away_linked();
    w.told("a", "Dee@dawnsong", "Ann@dawnsong", "hello back");
    assert_eq!(
        w.service.tells,
        vec![(
            "Ann".to_owned(),
            addr("Dee@dawnsong"),
            "hello back".to_owned()
        )]
    );
    assert_eq!(w.answers()[0].1, Outcome::Delivered);
    w.service.tells.clear();
    w.service.ignored.insert("ann".into(), vec!["dee".into()]);
    w.told("b", "Dee@dawnsong", "Ann@dawnsong", "hello");
    assert_eq!(w.answers()[0].1, Outcome::Squelched);
    w.service.ignored.clear();
    for from in ["Gil", "Cara", "Eve", "Nobody"] {
        w.told("c", &format!("{from}@dawnsong"), "Ann@dawnsong", "hello");
        assert_eq!(w.answers()[0].1, Outcome::Offline, "{from}");
    }
    assert!(w.service.tells.is_empty(), "{:?}", w.service.tells);
}
