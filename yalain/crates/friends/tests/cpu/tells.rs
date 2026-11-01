//! Tells within the world, through the service.

use super::*;
use auberean_friends::Extra;

/// A tell to a present character of this world is shown to it, from the teller written as the
/// client knows its own world, with the extra data carried; the teller is answered done. A
/// character not present is answered not logged in, and a name no character has, doesn't exist.
/// Behaviour: server.friends.tell-within-the-world
#[test]
fn a_tell_within_the_world_is_shown_to_a_present_character_and_answered_by_its_outcome() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.character("Dan");
    let extra = Extra([7; 16]);
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: "Cara@Dawnsong".into(),
            text: "hi".into(),
            extra: Some(extra),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    assert_eq!(
        events(&w.take(cara)),
        vec![Event::UTellEv {
            user: "Ann@Dawnsong".into(),
            text: "hi".into(),
            extra: Some(extra),
        }]
    );
    for (user, hr) in [("Dan", Hr::NOT_ONLINE), ("Nobody", Hr::NO_SUCH_USER)] {
        let cx = w.request(
            ann,
            Request::SendToUserEx {
                user: user.into(),
                text: "hi".into(),
                extra: None,
            },
        );
        assert_eq!(answer(&w.take(ann), cx).0, hr, "{user}");
    }
    assert!(w.lines_of("Ann").is_empty());
}

/// A tell to a character that is away is shown and answered done, as the client shows only its
/// echo for that; the teller is shown the world's own away line.
/// Behaviour: server.friends.tell-within-the-world
#[test]
fn a_tell_to_an_away_character_is_shown_and_the_teller_given_the_away_line() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.request(
        cara,
        Request::SetLoc {
            flags: 4,
            location: String::new(),
        },
    );
    w.take(cara);
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: "Cara".into(),
            text: "hi".into(),
            extra: None,
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    assert_eq!(events(&w.take(cara)).len(), 1);
    assert_eq!(
        w.lines_of("Ann"),
        vec!["Cara is away: Not to be disturbed.".to_owned()]
    );
}

/// A character that ignores another is never shown its tells through the service; the teller is
/// answered as if the tell were delivered.
/// Behaviour: server.friends.ignored-tells-not-shown
#[test]
fn an_ignored_tellers_tell_is_answered_done_and_shown_to_nobody() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.request(
        ann,
        Request::IgnoredAdd {
            user: "Cara".into(),
        },
    );
    w.take(ann);
    let cx = w.request(
        cara,
        Request::SendToUserEx {
            user: "Ann".into(),
            text: "hey".into(),
            extra: None,
        },
    );
    assert_eq!(answer(&w.take(cara), cx).0, Hr::OK);
    assert!(events(&w.take(ann)).is_empty());
}

/// A character's tells within the world are held to the network's pace for one character: past
/// it, refused with the could-not-be-sent code and the world's line.
/// Behaviour: server.friends.tell-within-the-world
#[test]
fn tells_within_the_world_are_held_to_the_characters_pace() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.player("Cara");
    let mut refused = 0;
    let mut sent = 0;
    for _ in 0..40 {
        let cx = w.request(
            ann,
            Request::SendToUserEx {
                user: "Cara".into(),
                text: "spam".into(),
                extra: None,
            },
        );
        match answer(&w.take(ann), cx).0 {
            Hr::OK => sent += 1,
            Hr::DENIED => refused += 1,
            hr => panic!("{hr}"),
        }
        // Below the stream's own bound on requests.
        w.now += std::time::Duration::from_millis(250);
    }
    assert!(sent >= 5, "{sent}");
    assert!(refused >= 1, "{refused}");
    assert!(w
        .lines_of("Ann")
        .iter()
        .all(|l| l == yalain_federation::TOO_QUICKLY));
}
