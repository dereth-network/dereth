//! Presence and the away state within the world.

use super::*;

/// A character with a stream entering the world is shown online to every present character that
/// lists it, and offline when it leaves or its stream ends; its state (away, not to be
/// disturbed) is shown as the modes 3 and 5, always with the online bit.
/// Behaviour: server.friends.presence-within-the-world
#[test]
fn friends_in_the_world_see_a_character_come_go_and_change_its_state() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.character("Cara");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    w.take(ann);
    let cara = w.stream("Cara");
    assert!(
        events(&w.take(ann)).is_empty(),
        "a stream alone is not presence"
    );
    w.enter("Cara");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::ONLINE)]
    );
    for (flags, mode) in [(2, 3), (4, 5), (1, 1)] {
        let cx = w.request(
            cara,
            Request::SetLoc {
                flags,
                location: String::new(),
            },
        );
        assert_eq!(answer(&w.take(cara), cx).0, Hr::OK);
        assert_eq!(
            events(&w.take(ann)),
            vec![stat("Cara@Dawnsong", Mode(mode))],
            "flags {flags}"
        );
    }
    w.leave("Cara");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::OFFLINE)]
    );
    w.enter("Cara");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::ONLINE)]
    );
    w.drop_stream(cara);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::OFFLINE)],
        "a stream that ends takes the character's presence"
    );
    assert!(!w.service.is_present("Cara"));
}

/// The state a character chose lasts its session: entering again, it is available.
/// Behaviour: server.friends.presence-within-the-world
#[test]
fn a_characters_state_lasts_until_it_leaves() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    w.request(
        cara,
        Request::SetLoc {
            flags: 2,
            location: String::new(),
        },
    );
    w.take(ann);
    w.leave("Cara");
    w.enter("Cara");
    w.take(cara);
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(ann), cx).1,
        Answer::GetFriendsList {
            friends: vec![status("Cara@Dawnsong", Mode::ONLINE)]
        }
    );
}

/// The client asks for its state again without changing it (the final client sends away twice
/// for one `/afk`): every ask is answered, and friends are told only of a change.
/// Behaviour: server.friends.state-told-only-when-it-changes
#[test]
fn the_same_state_asked_for_again_is_answered_but_not_told_to_friends_again() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    w.take(ann);
    let set = |w: &mut World, flags: u32| {
        let cx = w.request(
            cara,
            Request::SetLoc {
                flags,
                location: String::new(),
            },
        );
        assert_eq!(answer(&w.take(cara), cx).0, Hr::OK, "flags {flags}");
    };
    set(&mut w, 2);
    assert_eq!(events(&w.take(ann)), vec![stat("Cara@Dawnsong", Mode(3))]);
    set(&mut w, 2);
    assert!(events(&w.take(ann)).is_empty(), "away again is no news");
    set(&mut w, 4);
    set(&mut w, 4);
    assert_eq!(events(&w.take(ann)), vec![stat("Cara@Dawnsong", Mode(5))]);
    set(&mut w, 1);
    set(&mut w, 0);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::ONLINE)],
        "available asked for twice, told once"
    );
}

/// Ann in the world on the network, listed by Cara and watched by Bob on another world; her
/// client is killed and a new one's stream comes while her character still lingers, then the
/// character leaves the world `linger` seconds after. `old_stream_open`: the world has not yet
/// seen the old stream end, so the new one takes its place. Ann's new stream and Cara's.
fn hand_over(w: &mut World, old_stream_open: bool, linger: u64) -> (StreamId, StreamId) {
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.request(cara, Request::FriendAdd { user: here("Ann") });
    w.take(cara);
    w.request(ann, Request::FriendAdd { user: here("Cara") });
    w.take(ann);
    w.on("Ann");
    w.hub(&Message::Watch {
        owner: addr("Bob@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    w.frames();
    if !old_stream_open {
        // The killed client's stream ends: Ann has gone.
        w.drop_stream(ann);
        assert_eq!(
            events(&w.take(cara)),
            vec![stat(&here("Ann"), Mode::OFFLINE)]
        );
        w.frames();
    }
    let again = w.stream("Ann");
    if old_stream_open {
        assert!(events(&w.take(cara)).is_empty());
    } else {
        assert_eq!(
            events(&w.take(cara)),
            vec![stat(&here("Ann"), Mode::ONLINE)]
        );
    }
    w.frames();
    w.advance(linger);
    assert!(events(&w.take(cara)).is_empty());
    w.leave("Ann");
    assert!(w.service.is_present("Ann"), "held after the hand-over");
    (again, cara)
}

/// No `Presence` among what the world sent the hub.
fn no_presence(frames: &[Message]) -> bool {
    !frames.iter().any(|m| matches!(m, Message::Presence { .. }))
}

/// A character handed over to a new client stays present after it leaves the world, so when the
/// new client enters it soon after, neither its friends in the world nor another world's watch
/// see it go and come back; the new client is sent its friends as it enters. Its next leaving,
/// with a stream it entered with, is seen at once.
/// Behaviour: server.friends.hand-over-keeps-presence
#[test]
fn a_character_handed_over_and_entered_again_in_time_is_never_seen_offline() {
    for old_stream_open in [false, true] {
        let mut w = World::linked();
        let (again, cara) = hand_over(&mut w, old_stream_open, 0);
        w.advance(18);
        w.enter("Ann");
        assert!(w.service.is_present("Ann"));
        assert!(
            events(&w.take(cara)).is_empty(),
            "Cara sees no change ({old_stream_open})"
        );
        let frames = w.frames();
        assert!(no_presence(&frames), "{frames:?}");
        assert!(
            events(&w.take(again)).contains(&Event::FStart {
                friends: vec![status(&here("Cara"), Mode::ONLINE)]
            }),
            "the client is sent its friends as it enters"
        );
        // Past when the hand-over would have ended.
        w.advance(5);
        assert!(events(&w.take(cara)).is_empty(), "nothing later either");
        assert!(no_presence(&w.frames()));
        w.leave("Ann");
        assert_eq!(
            events(&w.take(cara)),
            vec![stat(&here("Ann"), Mode::OFFLINE)],
            "an ordinary leaving is seen at once"
        );
        assert!(w.frames().contains(&Message::Presence {
            to: addr("Bob@coldeve"),
            of: addr("Ann@dawnsong"),
            online: false,
        }));
    }
}

/// A character handed over to a new client that does not enter it again is seen to leave once
/// the hand-over's time is up, by its friends in the world and another world's watch alike.
/// Behaviour: server.friends.hand-over-keeps-presence
#[test]
fn a_character_handed_over_and_not_entered_again_is_seen_to_leave_when_the_time_is_up() {
    for old_stream_open in [false, true] {
        let mut w = World::linked();
        let (again, cara) = hand_over(&mut w, old_stream_open, 0);
        w.advance(18);
        assert!(events(&w.take(cara)).is_empty());
        assert!(no_presence(&w.frames()));
        assert_eq!(
            w.service.next_wake(),
            Some(w.now + Duration::from_secs(1)),
            "the hand-over's end wakes the service"
        );
        w.advance(1);
        assert!(!w.service.is_present("Ann"));
        assert_eq!(
            events(&w.take(cara)),
            vec![stat(&here("Ann"), Mode::OFFLINE)],
            "seen once ({old_stream_open})"
        );
        assert!(w.frames().contains(&Message::Presence {
            to: addr("Bob@coldeve"),
            of: addr("Ann@dawnsong"),
            online: false,
        }));
        let cx = w.request(again, Request::GetFriendsList);
        assert_eq!(
            answer(&w.take(again), cx).0,
            Hr::NOT_CONNECTED,
            "no character for the stream now"
        );
        w.enter("Ann");
        assert_eq!(
            events(&w.take(cara)),
            vec![stat(&here("Ann"), Mode::ONLINE)]
        );
    }
}

/// Another character of the account entering ends the hand-over at once: the held one is seen to
/// leave, then the new one to come.
/// Behaviour: server.friends.hand-over-keeps-presence
#[test]
fn another_character_of_the_account_entering_ends_the_hand_over_at_once() {
    let mut w = World::linked();
    let (again, cara) = hand_over(&mut w, false, 0);
    let (account, _) = w.ids("Ann");
    let dee = 0x5000_0000 + w.store.next_character_number().expect("a number");
    let mut c = w.store.character_named("Ann").expect("reads").expect("Ann");
    c.id = dee;
    c.name = "Dee".into();
    w.store.insert_character(&c).expect("kept");
    w.request(cara, Request::FriendAdd { user: here("Dee") });
    w.take(cara);
    w.advance(5);
    w.service
        .entered(w.now, account, dee, "Dee", &mut w.store, w.fed.as_mut());
    w.flush();
    assert!(!w.service.is_present("Ann"));
    assert_eq!(
        events(&w.take(cara)),
        vec![
            stat(&here("Ann"), Mode::OFFLINE),
            stat(&here("Dee"), Mode::ONLINE)
        ]
    );
    assert!(events(&w.take(again)).contains(&Event::FStart {
        friends: Vec::new()
    }));
}

/// The new client's stream ending ends the hand-over at once: the character is seen to leave,
/// and seen to come when it enters again with a stream.
/// Behaviour: server.friends.hand-over-keeps-presence
#[test]
fn the_new_clients_stream_ending_ends_the_hand_over_at_once() {
    let mut w = World::linked();
    let (again, cara) = hand_over(&mut w, false, 0);
    w.drop_stream(again);
    assert!(!w.service.is_present("Ann"));
    assert_eq!(
        events(&w.take(cara)),
        vec![stat(&here("Ann"), Mode::OFFLINE)]
    );
    let third = w.stream("Ann");
    w.enter("Ann");
    assert_eq!(
        events(&w.take(cara)),
        vec![stat(&here("Ann"), Mode::ONLINE)]
    );
    assert!(events(&w.take(third)).contains(&Event::FStart {
        friends: vec![status(&here("Cara"), Mode::ONLINE)]
    }));
}

/// The hand-over's time counts from the old body's leaving, not from the new stream's coming: a
/// new client whose stream came while the old body lingered (15 s, as a client gone lingers),
/// entering 12 s after the body left, tells nobody anything; one that never enters is seen to
/// leave 19 s after the body left.
/// Behaviour: server.friends.hand-over-keeps-presence
#[test]
fn the_hand_over_counts_from_the_old_bodys_leaving() {
    for old_stream_open in [false, true] {
        let mut w = World::linked();
        let (again, cara) = hand_over(&mut w, old_stream_open, 15);
        w.advance(12);
        w.enter("Ann");
        assert!(
            events(&w.take(cara)).is_empty(),
            "a slow entry tells nobody ({old_stream_open})"
        );
        assert!(no_presence(&w.frames()));
        assert!(events(&w.take(again))
            .iter()
            .any(|e| matches!(e, Event::FStart { .. })));

        let mut w = World::linked();
        let (_, cara) = hand_over(&mut w, old_stream_open, 15);
        w.advance(18);
        assert!(
            events(&w.take(cara)).is_empty(),
            "held 18 s after the body left"
        );
        assert!(no_presence(&w.frames()));
        w.advance(1);
        assert_eq!(
            events(&w.take(cara)),
            vec![stat(&here("Ann"), Mode::OFFLINE)],
            "seen to leave 19 s after the body left ({old_stream_open})"
        );
        assert!(w.frames().contains(&Message::Presence {
            to: addr("Bob@coldeve"),
            of: addr("Ann@dawnsong"),
            online: false,
        }));
    }
}
