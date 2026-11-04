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

/// Hide or show a character, as the host says.
fn hide(w: &mut World, names: &[&str]) {
    let ids = names.iter().map(|n| w.ids(n).1).collect();
    w.service
        .set_hidden(w.now, ids, &mut w.store, w.fed.as_mut());
    w.flush();
}

/// A hidden character goes offline to its friends once and is told of no more: its state, its
/// leaving and its coming back say nothing, a friend added is shown offline, and a tell to it is
/// answered as one to a character offline is, and shows nothing. No longer hidden, it is back with
/// the state it chose.
/// Behaviour: server.friends.hidden-is-offline-to-friends
#[test]
fn a_hidden_character_is_offline_to_its_friends_and_tells() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let cara = w.player("Cara");
    w.character("Dan");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    w.take(ann);
    let cx = w.request(
        cara,
        Request::SetLoc {
            flags: 2,
            location: String::new(),
        },
    );
    assert_eq!(answer(&w.take(cara), cx).0, Hr::OK);
    w.take(ann);

    hide(&mut w, &["Cara"]);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::OFFLINE)]
    );
    hide(&mut w, &["Cara"]);
    assert_eq!(events(&w.take(ann)), [], "said once");

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
    assert_eq!(tell(&mut w, "Cara"), tell(&mut w, "Dan"), "as one offline");
    assert_eq!(tell(&mut w, "Cara"), Hr::NOT_ONLINE);
    assert_eq!(events(&w.take(cara)), [], "it hears nothing");
    assert_eq!(w.lines_of("Ann"), Vec::<String>::new(), "nor its away line");

    w.request(
        cara,
        Request::SetLoc {
            flags: 4,
            location: String::new(),
        },
    );
    assert_eq!(events(&w.take(ann)), [], "its state is not told");

    let added = w.player("Eli");
    w.request(
        added,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    assert_eq!(
        events(&w.take(added)),
        vec![stat("Cara@Dawnsong", Mode::OFFLINE)],
        "added, it shows offline"
    );

    hide(&mut w, &[]);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode(5))],
        "back as it chose to be"
    );

    hide(&mut w, &["Cara"]);
    w.take(ann);
    w.leave("Cara");
    assert_eq!(events(&w.take(ann)), [], "its leaving is not told");
    w.enter("Cara");
    assert_eq!(events(&w.take(ann)), [], "nor its entering");
    assert!(w.service.is_present("Cara"), "its client keeps its stream");
    w.take(cara);
    assert_eq!(
        {
            let cx = w.request(
                cara,
                Request::SendToUserEx {
                    user: "Ann".into(),
                    text: "hello".into(),
                    extra: None,
                },
            );
            answer(&w.take(cara), cx).0
        },
        Hr::OK,
        "it may still tell whom it chooses"
    );
}

/// A hidden character that left the world but stays present after a hand-over does not show online
/// on its way out, when the world no longer names it as hidden.
/// Behaviour: server.friends.hidden-is-offline-to-friends
#[test]
fn a_hidden_character_held_after_a_hand_over_stays_hidden_until_it_goes() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.character("Cara");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    w.enter("Cara");
    let late = w.stream("Cara");
    w.take(late);
    w.take(ann);
    hide(&mut w, &["Cara"]);
    w.take(ann);
    w.leave("Cara");
    assert!(w.service.is_present("Cara"), "held");
    hide(&mut w, &[]);
    assert_eq!(events(&w.take(ann)), [], "not shown online");
    w.now += yalain_friends::HAND_OVER;
    w.flush();
    assert!(!w.service.is_present("Cara"), "gone");
    assert_eq!(events(&w.take(ann)), [], "nor offline: it was");
    hide(&mut w, &[]);
    w.enter("Cara");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@Dawnsong", Mode::ONLINE)],
        "the next time it is seen"
    );
}

/// To other worlds a hidden character is not there: its watchers see it offline, a watch is answered
/// offline, a tell is answered as one to a character offline is and shows nothing. It stops watching
/// its own friends, which would say it is here, and what its client showed of them goes offline.
/// Hidden as it enters it says nothing at all; no longer hidden it is online and watches again.
/// Behaviour: server.friends.hidden-is-not-on-the-network
#[test]
fn a_hidden_character_is_not_there_to_other_worlds() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    w.hub(&Message::Watch {
        owner: addr("Cid@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    w.frames();
    w.take(ann);
    let offline = |to: &str| Message::Presence {
        to: addr(to),
        of: addr("Ann@dawnsong"),
        online: false,
    };
    let online = |to: &str| Message::Presence {
        to: addr(to),
        of: addr("Ann@dawnsong"),
        online: true,
    };
    let stops = Message::Watch {
        owner: addr("Ann@dawnsong"),
        target: addr("Bob@coldeve"),
        on: false,
    };
    let starts = Message::Watch {
        owner: addr("Ann@dawnsong"),
        target: addr("Bob@coldeve"),
        on: true,
    };

    hide(&mut w, &["Ann"]);
    let sent = w.frames();
    assert!(sent.contains(&offline("Cid@coldeve")), "{sent:?}");
    assert!(sent.contains(&offline("Bob@coldeve")), "{sent:?}");
    assert!(sent.contains(&stops), "{sent:?}");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Bob@coldeve", Mode::OFFLINE)]
    );

    w.hub(&Message::Watch {
        owner: addr("Dot@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    assert_eq!(w.frames(), vec![offline("Dot@coldeve")]);
    w.told("t1", "Cid@coldeve", "Ann@dawnsong", "are you there");
    assert_eq!(
        w.frames(),
        vec![Message::TellResult {
            id: "t1".into(),
            outcome: dereth_network_protocol::Outcome::Offline,
            message: None,
        }],
        "as any character offline"
    );
    assert_eq!(events(&w.take(ann)), [], "it hears nothing");
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    assert_eq!(events(&w.take(ann)), [], "nor of its friends");
    for online in [false, true] {
        w.hub(&Message::World(WorldInfo {
            slug: "coldeve".into(),
            name: "Coldeve".into(),
            online,
        }));
    }
    assert_eq!(
        w.frames(),
        [],
        "its friend's world coming back gets no watch"
    );

    w.leave("Ann");
    assert_eq!(w.frames(), [], "its leaving says nothing");
    w.enter("Ann");
    assert_eq!(w.frames(), [], "nor its entering");
    w.take(ann);

    hide(&mut w, &[]);
    let sent = w.frames();
    assert!(sent.contains(&online("Bob@coldeve")), "{sent:?}");
    assert!(sent.contains(&starts), "{sent:?}");
}

/// A renamed character stays in the world with its stream and its state, by the new name alone: a
/// friend present sees the old name go and the new come, once each, in the state it has; a tell
/// finds the new name and not the old; and leaving goes by the new name. A friend's list, and one it
/// asks for or enters to, holds the new name.
/// Behaviour: server.friends.rename-seen-at-once
#[test]
fn a_renamed_character_stays_present_under_its_new_name_alone() {
    let mut w = World::new();
    let ann = w.player("Ann");
    let bob = w.player("Bob");
    w.character("Cara");
    let cara = w.stream("Cara");
    let dan = w.player("Dan");
    w.request(ann, Request::FriendAdd { user: "Bob".into() });
    w.request(ann, Request::FriendAdd { user: "Dan".into() });
    w.enter("Cara");
    w.take(cara);
    w.request(cara, Request::FriendAdd { user: "Bob".into() });
    w.leave("Cara");
    w.request(
        bob,
        Request::SetLoc {
            flags: 2,
            location: String::new(),
        },
    );
    for s in [ann, bob, cara, dan] {
        w.take(s);
    }

    w.rename("Bob", "Robert");
    let away = Mode::ONLINE.with(Mode::AWAY);
    assert_eq!(
        events(&w.take(ann)),
        vec![
            stat(&here("Bob"), Mode::OFFLINE),
            stat(&here("Robert"), away)
        ],
        "the old name goes, the new comes, in its state"
    );
    assert!(events(&w.take(dan)).is_empty(), "one who lists it not");
    assert!(
        events(&w.take(bob)).is_empty(),
        "its own client: its stream goes on"
    );
    assert!(w.service.is_present("robert"));
    assert!(!w.service.is_present("Bob"));

    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(ann), cx).1,
        Answer::GetFriendsList {
            friends: vec![
                status(&here("Robert"), away),
                status(&here("Dan"), Mode::ONLINE)
            ]
        }
    );
    // Its own requests go on, as the new name.
    let cx = w.request(
        bob,
        Request::SetLoc {
            flags: 0,
            location: String::new(),
        },
    );
    assert_eq!(answer(&w.take(bob), cx).0, Hr::OK);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat(&here("Robert"), Mode::ONLINE)]
    );

    let tell = |w: &mut World, user: &str| {
        let cx = w.request(
            ann,
            Request::SendToUserEx {
                user: user.into(),
                text: "Psst".into(),
                extra: None,
            },
        );
        answer(&w.take(ann), cx).0
    };
    assert_eq!(tell(&mut w, "Robert"), Hr::OK);
    assert_eq!(events(&w.take(bob)).len(), 1, "it hears");
    assert_eq!(
        tell(&mut w, "Bob"),
        Hr::NO_SUCH_USER,
        "the old name is nobody"
    );

    // Who was out of the world when it happened finds the new name on entering.
    w.enter("Cara");
    let got = w.take(cara);
    assert_eq!(
        events(&got),
        vec![Event::FStart {
            friends: vec![status(&here("Robert"), Mode::ONLINE)]
        }]
    );

    w.leave("Robert");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat(&here("Robert"), Mode::OFFLINE)]
    );
    assert!(!w.service.is_present("Robert"), "gone by the new name");
}

/// A character renamed while hidden tells nobody either name, and comes back under the new one.
/// Behaviour: server.friends.rename-seen-at-once
#[test]
fn a_hidden_character_renamed_is_seen_only_as_it_comes_back() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.player("Bob");
    w.request(ann, Request::FriendAdd { user: "Bob".into() });
    hide(&mut w, &["Bob"]);
    w.take(ann);
    w.rename("Bob", "Robert");
    assert!(events(&w.take(ann)).is_empty());
    hide(&mut w, &[]);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat(&here("Robert"), Mode::ONLINE)]
    );
}

/// A character handed over to a new client and renamed before its old body left is held under the
/// new name, and goes under it when the time is up.
/// Behaviour: server.friends.rename-seen-at-once
#[test]
fn a_character_renamed_after_a_hand_over_goes_under_its_new_name() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.player("Bob");
    w.request(ann, Request::FriendAdd { user: "Bob".into() });
    let again = w.stream("Bob");
    w.take(again);
    w.take(ann);
    w.rename("Bob", "Robert");
    w.take(ann);
    w.leave("Robert");
    assert!(w.service.is_present("Robert"), "held");
    assert!(events(&w.take(ann)).is_empty());
    w.advance(yalain_friends::HAND_OVER.as_secs());
    assert!(!w.service.is_present("Robert"), "gone at last");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat(&here("Robert"), Mode::OFFLINE)]
    );
}
