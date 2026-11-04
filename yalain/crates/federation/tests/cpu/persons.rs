//! `@dn link`, the person view, and the hub's question about a character away.

use dereth_network_protocol::{Message, PersonFriend, PersonOp, PersonOutcome, ReachMessage};
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
        ..Settings::default()
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
/// asked, or unknown.
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

/// A teller of this world reaches an away, linked character too, unless the character ignores it on
/// the list the world keeps for it; never a character telling itself, or one off the network.
/// Behaviour: server.network.reach-answered
#[test]
fn the_hubs_question_about_a_teller_of_this_world_is_answered_by_the_list_kept_for_the_character() {
    let mut w = World::new();
    w.character("Ann");
    w.character("Cara");
    w.character("Dee");
    w.link(&[("coldeve", true)]);
    w.frames();
    for name in ["Ann", "Cara"] {
        w.set_on(name, true);
    }
    let t = w.t;
    let id = w.ids["Ann"];
    assert_eq!(w.fed.link_person(&mut w.store, t, id, "Ann"), None);
    w.frames();
    let ask = |w: &mut World, from: &str| -> bool {
        w.frame(
            &ReachMessage::Ask {
                id: "h1".into(),
                from: addr(from),
                to: addr("Ann@dawnsong"),
            }
            .to_json(),
        );
        let frames = w.fed.take_frames(w.t);
        match ReachMessage::from_json(frames.first().expect("an answer")).expect("reads") {
            ReachMessage::Result { reachable, .. } => reachable,
            other => panic!("{other:?}"),
        }
    };
    assert!(ask(&mut w, "Cara@dawnsong"));
    assert!(!ask(&mut w, "Ann@dawnsong"), "itself");
    assert!(!ask(&mut w, "Dee@dawnsong"), "off the network");
    assert!(!ask(&mut w, "Nobody@dawnsong"), "no such");
    w.service.ignored.insert("ann".into(), vec!["cara".into()]);
    assert!(!ask(&mut w, "Cara@dawnsong"), "ignored");
}

/// `Ann` present, on the network, her link her own, the first view not yet come; `coldeve` and
/// `frost` linked.
fn linked_ann() -> (World, u64) {
    let (mut w, id) = with_ann();
    w.link(&[("coldeve", true), ("frost", true)]);
    w.frames();
    let t = w.t;
    assert_eq!(w.fed.link_person(&mut w.store, t, id, "Ann"), None);
    w.frames();
    (w, id)
}

/// A view saying `Ann` is linked, her person friends as (handle, characters).
fn person_view(persons: &[(&str, &[&str])]) -> Message {
    Message::PersonFriends {
        to: addr("Ann@dawnsong"),
        linked: true,
        alts: Default::default(),
        persons: persons
            .iter()
            .map(|(h, cs)| PersonFriend {
                handle: addr(h),
                characters: cs.iter().map(|c| addr(c)).collect(),
            })
            .collect(),
    }
}

fn watches(frames: &[Message]) -> Vec<(String, bool)> {
    frames
        .iter()
        .filter_map(|m| match m {
            Message::Watch { target, on, .. } => Some((target.to_string(), *on)),
            _ => None,
        })
        .collect()
}

fn names(list: &[dereth_network_protocol::Address]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

/// A view lists what its person friends name as `Ann`'s friends on other worlds, watched, in the
/// view's order, each once; not her own world's. One no longer named goes, and its watch stops, shown
/// offline if it was shown online. Leaving, or going off the network, all go, and none was kept.
/// Behaviour: server.network.person-friends-listed
#[test]
fn a_view_lists_what_its_person_friends_name_while_it_names_them() {
    let (mut w, _) = linked_ann();
    w.hub(&person_view(&[
        ("Bob@coldeve", &["Bob@coldeve", "Rob@frost", "Dee@dawnsong"]),
        ("Cal@frost", &["Cal@frost", "Rob@frost"]),
    ]));
    assert_eq!(
        names(&w.fed.person_friends("Ann")),
        ["Bob@coldeve", "Rob@frost", "Cal@frost"]
    );
    assert_eq!(w.service.views, ["Ann"], "the view is told");
    assert!(
        w.service.lists.get("ann").is_none_or(Vec::is_empty),
        "not kept"
    );
    assert_eq!(
        watches(&w.frames()),
        [
            ("Bob@coldeve".to_owned(), true),
            ("Rob@frost".to_owned(), true),
            ("Cal@frost".to_owned(), true),
        ]
    );
    // The same view again changes nothing; a state for one of them reaches her.
    w.hub(&person_view(&[
        ("Bob@coldeve", &["Bob@coldeve", "Rob@frost", "Dee@dawnsong"]),
        ("Cal@frost", &["Cal@frost", "Rob@frost"]),
    ]));
    assert!(watches(&w.frames()).is_empty());
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Cal@frost"),
        online: true,
    });
    assert_eq!(w.service.states.len(), 1);
    w.service.states.clear();

    w.hub(&person_view(&[(
        "Bob@coldeve",
        &["Bob@coldeve", "Rob@frost"],
    )]));
    assert_eq!(
        names(&w.fed.person_friends("Ann")),
        ["Bob@coldeve", "Rob@frost"]
    );
    assert_eq!(watches(&w.frames()), [("Cal@frost".to_owned(), false)]);
    assert_eq!(
        w.service.states,
        [("Ann".to_owned(), addr("Cal@frost"), false, true)],
        "shown online, so shown offline"
    );
    assert!(
        w.fed.take_gone_row("Ann", &addr("Cal@frost")),
        "its row may still show"
    );
    assert!(!w.fed.take_gone_row("Ann", &addr("Cal@frost")), "once");
    assert!(
        !w.fed.take_gone_row("Ann", &addr("Zed@frost")),
        "never listed"
    );

    w.leave("Ann");
    assert!(w.fed.person_friends("Ann").is_empty());
    assert_eq!(
        watches(&w.frames()),
        [
            ("Bob@coldeve".to_owned(), false),
            ("Rob@frost".to_owned(), false)
        ]
    );
    assert!(
        !w.fed.take_gone_row("Ann", &addr("Bob@coldeve")),
        "a new client asks anew"
    );
    w.enter("Ann", yalain_federation::State::Available);
    w.frames();
    w.hub(&person_view(&[("Bob@coldeve", &["Bob@coldeve"])]));
    assert_eq!(names(&w.fed.person_friends("Ann")), ["Bob@coldeve"]);
    w.set_on("Ann", false);
    assert!(w.fed.person_friends("Ann").is_empty(), "off the network");
    assert!(
        w.fed.take_gone_row("Ann", &addr("Bob@coldeve")),
        "she stays, and so does its row"
    );
}

/// The first view saying a character is linked carries the friends it keeps on other worlds to its
/// person, quietly, once. One the view then names as a handle is the person's and no longer kept, its
/// watch going on; one named only as another's character stays kept, and is listed once. None is
/// carried at a later view.
/// Behaviour: server.network.person-friends-carried
#[test]
fn the_first_view_carries_kept_friends_and_a_handle_is_no_longer_kept() {
    let (mut w, _) = linked_ann();
    w.lists("Ann", "Bob@coldeve");
    w.lists("Ann", "Rob@frost");
    w.hub(&person_view(&[]));
    let sent = w.frames();
    let carried: Vec<_> = sent
        .iter()
        .filter_map(|m| match m {
            Message::Person {
                op: PersonOp::Carry,
                targets,
                ..
            } => Some(names(targets)),
            _ => None,
        })
        .collect();
    assert_eq!(carried, [["Bob@coldeve", "Rob@frost"]]);
    w.hub(&person_view(&[(
        "Bob@coldeve",
        &["Bob@coldeve", "Rob@frost"],
    )]));
    assert!(
        !w.frames().iter().any(|m| matches!(
            m,
            Message::Person {
                op: PersonOp::Carry,
                ..
            }
        )),
        "once"
    );
    assert_eq!(names(&w.fed.person_friends("Ann")), ["Bob@coldeve"]);
    assert_eq!(
        w.service.lists["ann"],
        [addr("Rob@frost")],
        "the other stays kept"
    );
    assert_eq!(
        names(&w.fed.remote_of(&w.service, "Ann")),
        ["Rob@frost", "Bob@coldeve"],
        "each once"
    );
    // Arriving linked again, what it still keeps goes again.
    w.leave("Ann");
    w.enter("Ann", yalain_federation::State::Available);
    w.frames();
    w.hub(&person_view(&[("Bob@coldeve", &["Bob@coldeve"])]));
    let carried: Vec<_> = w
        .frames()
        .iter()
        .filter_map(|m| match m {
            Message::Person {
                op: PersonOp::Carry,
                targets,
                ..
            } => Some(names(targets)),
            _ => None,
        })
        .collect();
    assert_eq!(carried, [["Rob@frost"]]);
}

/// A view listing no more than a character may: what it keeps and what its person friends list hold
/// at most 100 together, and one past that is listed when a later view has room.
/// Behaviour: server.network.person-friends-listed
#[test]
fn a_view_lists_no_more_than_a_character_may() {
    let (mut w, _) = linked_ann();
    for n in 0..99 {
        w.service
            .lists
            .entry("ann".into())
            .or_default()
            .push(addr(&format!(
                "Kept{}@frost",
                (b'a' + (n % 26) as u8) as char
            )));
    }
    w.hub(&person_view(&[(
        "Bob@coldeve",
        &["Bob@coldeve", "Rob@coldeve"],
    )]));
    assert_eq!(names(&w.fed.person_friends("Ann")), ["Bob@coldeve"]);
}

/// The view of a character nobody links hands back the friends it had: they are kept from then on, a
/// row already listed stays with no new watch, and a new one is watched. A character that never asked
/// for its link takes none.
/// Behaviour: server.network.person-friends-carried
#[test]
fn a_view_after_an_unlink_hands_the_friends_back_to_be_kept() {
    let (mut w, _) = linked_ann();
    w.hub(&person_view(&[("Bob@coldeve", &["Bob@coldeve"])]));
    w.frames();
    let back = |handles: &[&str]| Message::PersonFriends {
        to: addr("Ann@dawnsong"),
        linked: false,
        alts: Default::default(),
        persons: handles
            .iter()
            .map(|h| PersonFriend {
                handle: addr(h),
                characters: vec![addr(h)],
            })
            .collect(),
    };
    w.hub(&back(&["Bob@coldeve", "Cal@frost"]));
    assert!(w.fed.person_friends("Ann").is_empty());
    assert_eq!(
        w.service.lists["ann"],
        [addr("Bob@coldeve"), addr("Cal@frost")]
    );
    assert_eq!(watches(&w.frames()), [("Cal@frost".to_owned(), true)]);

    let mut never = World::new();
    never.on_and_present("Ann");
    never.link(&[("coldeve", true)]);
    never.frames();
    never.hub(&back(&["Bob@coldeve"]));
    assert!(never.service.lists.get("ann").is_none_or(Vec::is_empty));
    assert!(never.fed.person_friends("Ann").is_empty());
}

/// A character an old view's link says is linked, which never asked, lists none of its persons.
/// Behaviour: server.network.person-friends-listed
#[test]
fn a_view_of_a_character_that_never_asked_lists_none_of_its_persons() {
    let (mut w, _) = with_ann();
    w.hub(&person_view(&[("Bob@coldeve", &["Bob@coldeve"])]));
    assert!(w.fed.person_friends("Ann").is_empty());
    assert!(w.frames().iter().any(|m| matches!(
        m,
        Message::Person {
            op: PersonOp::Unlink,
            ..
        }
    )));
}

/// A linked character's add asks the hub for `befriend`, within its pace, and the hub's answer is
/// shown unless a client's own add waits on it, which writes its own; a refusal is always shown. A
/// request the hub never answers ends a waiting add too.
/// Behaviour: server.network.person-friend-added
#[test]
fn a_linked_characters_add_goes_to_the_hub_and_its_answer_is_shown() {
    let (mut w, id) = linked_ann();
    w.hub(&person_view(&[]));
    assert!(w.fed.friends_are_persons("Ann"));
    w.frames();
    let t = w.t;
    let bob = addr("Bob@coldeve");
    w.fed.befriend(t, id, "Ann", &bob).expect("within its pace");
    let sent = w.frames();
    let [Message::Person {
        op: PersonOp::Befriend,
        target: Some(target),
        id: asked,
        ..
    }] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    assert!(target.same(&bob));
    w.hub(&result(asked.clone(), PersonOutcome::Ok, None, None));
    assert_eq!(
        w.service.lines.last().map(|(_, l)| l.as_str()),
        Some("Bob@coldeve has been added to your friends list.")
    );
    assert_eq!(w.service.adds_answered, [(bob.clone(), false)]);

    w.service.lines.clear();
    w.service.adds_answered.clear();
    w.service.add_waits = true;
    w.fed.befriend(t, id, "Ann", &bob).expect("within its pace");
    let asked = id_of(&w.frames()[0]);
    w.hub(&result(asked, PersonOutcome::Ok, None, None));
    assert!(w.service.lines.is_empty(), "the client's own line");
    w.fed.befriend(t, id, "Ann", &bob).expect("within its pace");
    let asked = id_of(&w.frames()[0]);
    w.hub(&result(asked, PersonOutcome::Refused, None, None));
    assert_eq!(
        w.service.lines.last().map(|(_, l)| l.as_str()),
        Some(dereth_network_protocol::answers::FRIENDS_FULL)
    );
    assert_eq!(w.service.adds_answered.last(), Some(&(bob.clone(), true)));
    w.fed.befriend(t, id, "Ann", &bob).expect("within its pace");
    let asked = id_of(&w.frames()[0]);
    w.hub(&result(asked, PersonOutcome::NotLinked, None, None));
    assert_eq!(
        w.service.lines.last().map(|(_, l)| l.as_str()),
        Some("Link this character first: say @dn link.")
    );

    w.service.adds_answered.clear();
    w.fed.befriend(t, id, "Ann", &bob).expect("within its pace");
    w.frames();
    w.advance(10_000.0);
    let t = w.t;
    assert_eq!(w.service.adds_answered, [(bob.clone(), true)]);
    assert_eq!(
        w.service.lines.last().map(|(_, l)| l.as_str()),
        Some(UNAVAILABLE)
    );

    let mut n = 0;
    while w.fed.befriend(t, id, "Ann", &bob).is_ok() {
        n += 1;
        assert!(n < 1_000, "held to a pace");
    }
    assert_eq!(w.fed.befriend(t, id, "Ann", &bob), Err(TOO_QUICKLY));
}

/// Removing a row a person friend listed removes that person friend at the hub, quietly and whatever
/// the pace, by its handle; its watch stops, and its row goes at once. A later view takes the rest.
/// Behaviour: server.network.person-friend-removed
#[test]
fn removing_a_row_a_person_friend_listed_removes_that_person_friend() {
    let (mut w, id) = linked_ann();
    w.hub(&person_view(&[(
        "Bob@coldeve",
        &["Bob@coldeve", "Rob@frost"],
    )]));
    w.frames();
    let t = w.t;
    while w.fed.befriend(t, id, "Ann", &addr("Zed@frost")).is_ok() {}
    w.frames();
    w.fed.unfriend_person(t, id, "Ann", &addr("Rob@frost"));
    let sent = w.frames();
    assert!(
        sent.iter().any(|m| matches!(
            m,
            Message::Person { op: PersonOp::Unfriend, target: Some(h), .. } if h.same(&addr("Bob@coldeve"))
        )),
        "{sent:?}"
    );
    assert_eq!(watches(&sent), [("Rob@frost".to_owned(), false)]);
    assert_eq!(names(&w.fed.person_friends("Ann")), ["Bob@coldeve"]);
    let asked = sent
        .iter()
        .find(|m| {
            matches!(
                m,
                Message::Person {
                    op: PersonOp::Unfriend,
                    ..
                }
            )
        })
        .map(id_of)
        .expect("asked");
    w.service.lines.clear();
    w.hub(&result(asked, PersonOutcome::Ok, None, None));
    assert!(w.service.lines.is_empty(), "quietly");
}

/// A character listed through a person friend, other than its handle, coming online is shown as its
/// person; its first report is not news, and a handle has its own client's line alone.
/// Behaviour: server.network.person-friends-listed
#[test]
fn a_character_of_a_person_friend_coming_online_is_shown_as_that_person() {
    let (mut w, id) = linked_ann();
    w.hub(&person_view(&[(
        "Bob@coldeve",
        &["Bob@coldeve", "Rob@frost", "Cal@frost"],
    )]));
    let say = |w: &mut World, of: &str, online: bool| {
        w.hub(&Message::Presence {
            to: addr("Ann@dawnsong"),
            of: addr(of),
            online,
        });
    };
    w.service.lines.clear();
    say(&mut w, "Cal@frost", true);
    say(&mut w, "Rob@frost", false);
    say(&mut w, "Bob@coldeve", false);
    assert!(w.service.lines.is_empty(), "a first report is not news");
    say(&mut w, "Rob@frost", true);
    assert_eq!(
        w.service.lines,
        [(id, "Bob@coldeve is online as Rob@frost.".to_owned())]
    );
    w.service.lines.clear();
    say(&mut w, "Bob@coldeve", true);
    say(&mut w, "Rob@frost", false);
    assert!(w.service.lines.is_empty());
}

/// A character that asked for its link is owed its first view while the link is up, until it comes;
/// one that never asked has nothing to wait for.
/// Behaviour: server.network.person-friends-listed
#[test]
fn a_character_that_asked_for_its_link_is_owed_a_view_until_it_comes() {
    let (mut w, id) = with_ann();
    assert!(!w.fed.view_due("Ann"), "never asked");
    let t = w.t;
    assert_eq!(w.fed.link_person(&mut w.store, t, id, "Ann"), None);
    assert!(w.fed.view_due("Ann"));
    assert!(!w.fed.view_due("Nobody"));
    w.hub(&view("Ann@dawnsong", false));
    assert!(!w.fed.view_due("Ann"));
}
