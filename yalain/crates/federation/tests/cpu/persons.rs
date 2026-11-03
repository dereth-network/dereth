//! `@dn link`, the person view, and the hub's question about a character away.

use dereth_network_protocol::{Message, PersonOp, PersonOutcome, ReachMessage};
use yalain_federation::{
    Federation, Settings, ASKED_RECORD, NOT_ON, NOW_LINKED, TOO_QUICKLY, UNAVAILABLE,
};
use yalain_store::Store;

use crate::{addr, World, SLUG};

/// A world on the network with `Ann` on it, present, the link up; her id.
fn with_ann() -> (World, u64) {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[("coldeve", true)]);
    w.frames();
    let id = w.ids["Ann"];
    (w, id)
}

/// `Ann` asks for a link; the request the hub is sent.
fn link_asked(w: &mut World, id: u64) -> Message {
    let t = w.t;
    assert_eq!(w.fed.link_person(&mut w.store, t, id, "Ann"), None);
    w.frames().pop().expect("the request goes")
}

fn result(id: String, outcome: PersonOutcome, code: Option<&str>, url: Option<&str>) -> Message {
    Message::PersonResult {
        id,
        outcome,
        code: code.map(str::to_owned),
        expires_secs: code.map(|_| 600),
        url: url.map(str::to_owned),
        characters: Vec::new(),
    }
}

fn view(to: &str, linked: bool) -> Message {
    Message::PersonFriends {
        to: addr(to),
        linked,
        alts: Default::default(),
        persons: Vec::new(),
    }
}

fn id_of(m: &Message) -> String {
    match m {
        Message::Person { id, .. } => id.clone(),
        other => panic!("not a person request: {other:?}"),
    }
}

/// `@dn link` asks the hub for a code under the character's address, and the answer is shown to it:
/// the link, or the code when the hub's page does not carry it; too many, a hub that links nothing,
/// or anything else each its own line. `@dn unlink` the same. Off the network, or past its pace,
/// nothing goes and it is told why; a request the hub never answers ends in the network's line.
/// Behaviour: server.network.person-link-and-unlink
#[test]
fn link_and_unlink_ask_the_hub_and_show_its_answer() {
    let (mut w, ann) = with_ann();
    let m = link_asked(&mut w, ann);
    assert!(
        matches!(&m, Message::Person { op: PersonOp::Code, who, target: None, .. } if *who == addr("Ann@dawnsong")),
        "{m:?}"
    );
    let cases = [
        (
            PersonOutcome::Ok,
            Some("K7QD-2MXP"),
            Some("https://dereth.network/link/#K7QD-2MXP"),
            "Open https://dereth.network/link/#K7QD-2MXP within 10 minutes to link this character \
             to your Discord account. Keep the link to yourself.",
        ),
        (
            PersonOutcome::Ok,
            Some("K7QD-2MXP"),
            Some("https://hub.example/link"),
            "Your link code is K7QD-2MXP. Enter it at https://hub.example/link within 10 minutes \
             to link this character to your Discord account. Keep the code to yourself.",
        ),
        (
            PersonOutcome::RateLimited,
            None,
            None,
            "You have asked for too many links. Wait a while and try again.",
        ),
        (
            PersonOutcome::Unavailable,
            None,
            None,
            "The Dereth Network's hub does not link characters.",
        ),
        (
            PersonOutcome::Refused,
            None,
            None,
            "The Dereth Network gave no link.",
        ),
    ];
    let mut request = m;
    for (outcome, code, url, line) in cases {
        w.hub(&result(id_of(&request), outcome, code, url));
        assert_eq!(w.service.lines.pop(), Some((ann, line.to_owned())));
        request = link_asked(&mut w, ann);
    }
    w.hub(&result(id_of(&request), PersonOutcome::Refused, None, None));
    w.service.lines.clear();

    let t = w.t;
    assert_eq!(w.fed.unlink_person(t, ann, "Ann"), None);
    let m = w.frames().pop().expect("the request goes");
    assert!(
        matches!(
            m,
            Message::Person {
                op: PersonOp::Unlink,
                ..
            }
        ),
        "{m:?}"
    );
    w.hub(&result(id_of(&m), PersonOutcome::Ok, None, None));
    assert_eq!(
        w.service.lines.pop().map(|l| l.1).as_deref(),
        Some("This character is no longer linked to a Discord account.")
    );
    assert_eq!(w.fed.unlink_person(t, ann, "Ann"), None);
    let m = w.frames().pop().expect("the request goes");
    w.hub(&result(id_of(&m), PersonOutcome::NotLinked, None, None));
    assert_eq!(
        w.service.lines.pop().map(|l| l.1).as_deref(),
        Some("This character is not linked to a Discord account.")
    );

    // Off the network, nothing goes.
    w.set_on("Ann", false);
    w.frames();
    assert_eq!(
        w.fed.link_person(&mut w.store, t, ann, "Ann").as_deref(),
        Some(NOT_ON)
    );
    assert_eq!(w.fed.unlink_person(t, ann, "Ann").as_deref(), Some(NOT_ON));
    assert_eq!(w.frames(), []);

    // Past its pace, on no clock but this one.
    let (mut w, ann) = with_ann();
    let t = w.t;
    let mut sent = 0;
    while w.fed.unlink_person(t, ann, "Ann").is_none() {
        sent += 1;
        assert!(sent < 1_000, "a pace holds it back at last");
    }
    assert!(sent >= 1);
    assert_eq!(
        w.fed.unlink_person(t, ann, "Ann").as_deref(),
        Some(TOO_QUICKLY)
    );

    // Never answered: given up on, once the hub has had time to answer.
    let (mut w, ann) = with_ann();
    let m = link_asked(&mut w, ann);
    drop(m);
    w.advance(10_000.0);
    assert_eq!(w.service.lines, [(ann, UNAVAILABLE.to_owned())]);
}

/// A request handed to a link that goes before the hub answers waits for the next one, and goes
/// again first.
/// Behaviour: server.network.person-request-outlives-a-link
#[test]
fn a_person_request_on_a_link_that_goes_goes_again_on_the_next() {
    let (mut w, ann) = with_ann();
    let first = link_asked(&mut w, ann);
    let t = w.t;
    w.fed.on_link_down(&mut w.service, t);
    assert_eq!(w.frames(), []);
    let after = w.link(&[("coldeve", true)]);
    assert!(
        after
            .iter()
            .any(|m| matches!(m, Message::Person { id, .. } if *id == id_of(&first))),
        "{after:?}"
    );
    assert_eq!(w.service.lines, []);
}

/// A link holds only for the character that asked for it, and that it asked is kept across a
/// restart. A view saying one that never asked is linked speaks of an earlier character of its
/// name: it is taken as unlinked and the hub told so, quietly. One that asked is told when it
/// becomes linked while here, and not when it arrives linked.
/// Behaviour: server.network.person-link-holds-for-its-asker
#[test]
fn a_link_holds_only_for_the_character_that_asked_and_is_told_when_it_comes() {
    let (mut w, ann) = with_ann();
    w.hub(&view("Ann@dawnsong", true));
    assert!(!w.fed.is_linked("Ann"), "she never asked");
    assert_eq!(w.service.lines, []);
    let quiet = w.frames().pop().expect("the hub is told");
    assert!(
        matches!(
            quiet,
            Message::Person {
                op: PersonOp::Unlink,
                ..
            }
        ),
        "{quiet:?}"
    );
    w.hub(&result(id_of(&quiet), PersonOutcome::Ok, None, None));
    assert_eq!(w.service.lines, [], "its answer shows nothing");

    let m = link_asked(&mut w, ann);
    w.hub(&result(
        id_of(&m),
        PersonOutcome::Ok,
        Some("AAAA"),
        Some("https://h/#AAAA"),
    ));
    w.service.lines.clear();
    w.hub(&view("Ann@dawnsong", false));
    assert_eq!(w.service.lines, []);
    w.hub(&view("Ann@dawnsong", true));
    assert_eq!(w.service.lines, [(ann, NOW_LINKED.to_owned())]);
    assert!(w.fed.is_linked("Ann"));
    assert_eq!(w.frames(), [], "no quiet unlink");
    w.service.lines.clear();

    // Kept: a restart still knows she asked.
    assert!(w.store.world_record(ASKED_RECORD).expect("reads").is_some());
    let settings = Settings {
        enabled: true,
        slug: SLUG.into(),
        software: "yalain test".into(),
    };
    w.fed = Federation::load(settings, 1_790_000_100, &mut w.store).expect("loads");
    assert!(w.fed.asked_link(ann));
    w.link(&[("coldeve", true)]);
    w.frames();
    w.hub(&view("Ann@dawnsong", true));
    assert!(w.fed.is_linked("Ann"));
    assert_eq!(w.service.lines, [], "arriving linked says nothing");
    assert_eq!(w.frames(), [], "no quiet unlink");

    // Another world's character, or one not here, changes nothing.
    w.hub(&view("Ann@coldeve", false));
    assert!(w.fed.is_linked("Ann"));
    w.leave("Ann");
    assert!(!w.fed.is_linked("Ann"), "not known while she is away");
}

/// A present character on the network asks for its person view, again at each welcome, and stops
/// when it leaves or goes off the network; a character off the network asks for none.
/// Behaviour: server.network.person-view-watched
#[test]
fn the_person_view_is_watched_while_a_character_is_present_on_the_network() {
    let watches = |frames: &[Message]| -> Vec<(String, bool)> {
        frames
            .iter()
            .filter_map(|m| match m {
                Message::PersonWatch { who, on } => Some((who.to_string(), *on)),
                _ => None,
            })
            .collect()
    };
    let mut w = World::new();
    w.character("Ann");
    w.character("Bob");
    w.enter("Bob", yalain_federation::State::Available);
    w.set_on("Ann", true);
    assert_eq!(
        watches(&w.link(&[])),
        [],
        "not present, or not on the network"
    );
    w.enter("Ann", yalain_federation::State::Available);
    assert_eq!(watches(&w.frames()), [("Ann@dawnsong".to_owned(), true)]);
    assert_eq!(
        watches(&w.link(&[])),
        [("Ann@dawnsong".to_owned(), true)],
        "at each welcome"
    );
    w.leave("Ann");
    assert_eq!(watches(&w.frames()), [("Ann@dawnsong".to_owned(), false)]);
    w.enter("Ann", yalain_federation::State::Available);
    w.frames();
    w.set_on("Ann", false);
    assert_eq!(watches(&w.frames()), [("Ann@dawnsong".to_owned(), false)]);
}

/// The hub's question about a tell to a character away is answered true only for one of this
/// world's, away, on the network, that asked for its link, ignoring neither the teller nor its
/// world, the operator blocking neither; a true takes one from the pace of that world's tells to it,
/// and one past it is false. Anything else is false, alike: in the world, off the network, never
/// asked, unknown, or a teller of this world.
/// Behaviour: server.network.reach-answered
#[test]
fn the_hubs_question_about_a_character_away_is_true_only_as_a_shown_tell_would_be() {
    let mut w = World::new();
    w.character("Ann");
    w.character("Cara");
    w.character("Dee");
    w.character("Eve");
    w.link(&[("coldeve", true)]);
    w.frames();
    for name in ["Ann", "Cara", "Dee"] {
        w.set_on(name, true);
    }
    let t = w.t;
    for name in ["Ann", "Dee"] {
        let id = w.ids[name];
        assert_eq!(w.fed.link_person(&mut w.store, t, id, name), None);
    }
    w.frames();
    let ask = |w: &mut World, from: &str, to: &str| -> bool {
        w.frame(
            &ReachMessage::Ask {
                id: "h1".into(),
                from: addr(from),
                to: addr(to),
            }
            .to_json(),
        );
        let frames = w.fed.take_frames(w.t);
        match ReachMessage::from_json(frames.first().expect("an answer")).expect("reads") {
            ReachMessage::Result { id, reachable } => {
                assert_eq!(id, "h1");
                reachable
            }
            other => panic!("{other:?}"),
        }
    };
    assert!(ask(&mut w, "Bob@coldeve", "Ann@dawnsong"));
    assert!(!ask(&mut w, "Bob@coldeve", "Cara@dawnsong"), "never asked");
    assert!(
        !ask(&mut w, "Bob@coldeve", "Eve@dawnsong"),
        "off the network"
    );
    assert!(!ask(&mut w, "Bob@coldeve", "Nobody@dawnsong"), "no such");
    assert!(
        !ask(&mut w, "Bob@coldeve", "Ann@frost"),
        "not of this world"
    );
    assert!(
        !ask(&mut w, "Cara@dawnsong", "Ann@dawnsong"),
        "of this world"
    );

    w.service
        .ignored
        .insert("dee".into(), vec!["bob@coldeve".into()]);
    assert!(
        !ask(&mut w, "Bob@coldeve", "Dee@dawnsong"),
        "she ignores him"
    );
    assert!(ask(&mut w, "Bob@coldeve", "Ann@dawnsong"));
    w.service
        .ignored
        .insert("dee".into(), vec!["coldeve".into()]);
    assert!(
        !ask(&mut w, "Bob@coldeve", "Dee@dawnsong"),
        "she ignores his world"
    );
    w.service.ignored.clear();

    w.operator(&["block", "coldeve"]);
    assert!(!ask(&mut w, "Bob@coldeve", "Ann@dawnsong"), "blocked");
    w.operator(&["unblock", "coldeve"]);

    w.service.in_world.push("ann".into());
    assert!(!ask(&mut w, "Bob@coldeve", "Ann@dawnsong"), "in the world");
    w.service.in_world.clear();

    let mut allowed = 0;
    while ask(&mut w, "Bob@coldeve", "Ann@dawnsong") {
        allowed += 1;
        assert!(allowed < 1_000, "the pace holds at last");
    }
    assert!(allowed >= 1);
}
