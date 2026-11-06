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

/// A character the world has gagged tells nobody through the service: a tell to a character of
/// this world or of another is shown to nobody and sends nothing to the hub, and the teller's
/// client is answered that it could not be sent, with the world's line that it is gagged. Its gag
/// lifted, it tells again.
/// Behaviour: server.friends.gagged-tells-go-nowhere
#[test]
fn a_gagged_characters_tells_through_the_service_go_nowhere() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.on("Ann");
    w.frames();
    let (_, ann_id) = w.ids("Ann");
    w.service.set_gagged(std::iter::once(ann_id).collect());
    for user in ["Cara", "Bob@coldeve"] {
        let cx = w.request(
            ann,
            Request::SendToUserEx {
                user: user.into(),
                text: "hi".into(),
                extra: None,
            },
        );
        assert_eq!(answer(&w.take(ann), cx).0, Hr::DENIED, "{user}");
        assert_eq!(w.lines_of("Ann"), vec![GAGGED.to_owned()], "{user}");
    }
    assert!(events(&w.take(cara)).is_empty(), "Cara is shown nothing");
    assert!(w.frames().is_empty(), "nothing goes to the hub");
    w.service.set_gagged(std::collections::HashSet::new());
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: "Cara".into(),
            text: "hi".into(),
            extra: None,
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK, "ungagged");
    assert_eq!(events(&w.take(cara)).len(), 1);
}

/// A tell by a name as a client shows it, with its suffix, reaches the character, in any case, with
/// or without the world; a suffix no one has now is no one; and a character's own name wins.
/// Behaviour: server.friends.suffixed-name
#[test]
fn a_tell_by_a_name_with_its_suffix_reaches_the_character() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.character("Dan");
    let tell = |w: &mut World, user: &str| {
        let cx = w.request(
            ann,
            Request::SendToUserEx {
                user: user.into(),
                text: "hi".into(),
                extra: None,
            },
        );
        answer(&w.take(ann), cx).0
    };
    assert_eq!(tell(&mut w, "Cara St. 3@Dawnsong"), Hr::NO_SUCH_USER);
    w.service
        .set_suffixed([("Cara St. 3".to_owned(), "Cara".to_owned())]);
    for user in ["Cara St. 3@Dawnsong", "cara st. 3", "Cara St. 3@dawnsong"] {
        assert_eq!(tell(&mut w, user), Hr::OK, "{user}");
        assert_eq!(events(&w.take(cara)).len(), 1, "{user}");
    }
    assert_eq!(tell(&mut w, "Cara St. 4@Dawnsong"), Hr::NO_SUCH_USER);
    // A shown name that is someone's own: the one with that name is told.
    let bold = w.player("Ann Bold");
    w.service
        .set_suffixed([("Ann Bold".to_owned(), "Ann".to_owned())]);
    assert_eq!(tell(&mut w, "Ann Bold@Dawnsong"), Hr::OK);
    assert_eq!(events(&w.take(bold)).len(), 1);
    assert!(events(&w.take(ann)).is_empty());
    // No longer suffixed, the name is no one again.
    w.service.set_suffixed([]);
    assert_eq!(tell(&mut w, "Cara St. 3@Dawnsong"), Hr::NO_SUCH_USER);
}
