//! Other worlds' characters: friends and tells through the world's link.

use super::*;
use dereth_network_protocol::Outcome;
use yalain_federation::{Friends, NOT_ON, WORLD_OFF};
use yalain_friends::{tell_code, FRIEND_REFUSED};

/// Another world's friend needs the world on the network and the character on it: refused with
/// the world's line otherwise. Kept, the world watches it, the add is answered with the state its
/// world reports, and each later state reaches the character; when its world goes, offline again.
/// Behaviour: server.friends.remote-friend-watched
#[test]
fn another_worlds_friend_is_watched_and_its_state_shown_once_the_character_is_on_the_network() {
    let mut off = World::new();
    let ann = off.player("Ann");
    let cx = off.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&off.take(ann), cx).0, FRIEND_REFUSED);
    assert_eq!(off.lines_of("Ann"), vec![WORLD_OFF.to_owned()]);

    let mut w = World::linked();
    let ann = w.player("Ann");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, FRIEND_REFUSED);
    assert_eq!(w.lines_of("Ann"), vec![NOT_ON.to_owned()]);
    w.on("Ann");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@Coldeve".into(),
        },
    );
    assert!(w.take(ann).is_empty(), "answered when its world answers");
    let mut sent = w.frames();
    sent.retain(|m| !matches!(m, Message::PersonWatch { .. }));
    assert_eq!(
        sent,
        vec![Message::Watch {
            owner: addr("Ann@dawnsong"),
            target: addr("Bob@coldeve"),
            on: true,
        }]
    );
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    let got = w.take(ann);
    assert_eq!(answer(&got, cx).0, Hr::OK);
    assert_eq!(events(&got), vec![stat("Bob@coldeve", Mode::ONLINE)]);
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(ann), cx).1,
        Answer::GetFriendsList {
            friends: vec![status("Bob@coldeve", Mode::ONLINE)]
        }
    );
    w.hub(&Message::World(WorldInfo {
        slug: "coldeve".into(),
        name: "Coldeve".into(),
        online: false,
    }));
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Bob@coldeve", Mode::OFFLINE)]
    );
    let cx = w.request(
        ann,
        Request::FriendDelete {
            user: "bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
}

/// Going off the network leaves nothing of another world watched for the character: each friend
/// there shows offline at once, whatever its world last said.
/// Behaviour: server.friends.remote-friend-watched
#[test]
fn going_off_the_network_shows_each_friend_on_another_world_offline() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    let cx = w.request(
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
    let got = w.take(ann);
    assert_eq!(answer(&got, cx).0, Hr::OK);
    assert_eq!(events(&got), vec![stat("Bob@coldeve", Mode::ONLINE)]);
    w.said("Ann", "off");
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Bob@coldeve", Mode::OFFLINE)]
    );
}

/// A present character on the network is online to another world's watch, and offline once it
/// has no stream; within the world, presence needs no network at all.
/// Behaviour: server.friends.remote-friend-watched
#[test]
fn another_worlds_watch_sees_a_character_online_only_while_it_has_a_stream() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.frames();
    w.hub(&Message::Watch {
        owner: addr("Bob@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    assert!(w.frames().contains(&Message::Presence {
        to: addr("Bob@coldeve"),
        of: addr("Ann@dawnsong"),
        online: true,
    }));
    w.drop_stream(ann);
    assert!(w.frames().contains(&Message::Presence {
        to: addr("Bob@coldeve"),
        of: addr("Ann@dawnsong"),
        online: false,
    }));
}

/// A tell to another world's character goes over the link under the teller's address and is
/// answered once, when it ends: done for delivered, away (with the world's away line) and
/// squelched; not logged in for offline; could not be sent, with the world's line, for the
/// rest. A tell from another world is shown with the teller's address.
/// Behaviour: server.friends.remote-tell
#[test]
fn a_tell_to_another_world_is_answered_once_by_how_it_ended() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: "Bob@coldeve".into(),
            text: "hi".into(),
            extra: None,
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::DENIED, "off the network");
    assert_eq!(w.lines_of("Ann"), vec![NOT_ON.to_owned()]);
    w.on("Ann");
    w.frames();
    for (outcome, message, hr, line) in [
        (Outcome::Delivered, None, Hr::OK, None),
        (
            Outcome::Afk,
            Some("brb"),
            Hr::OK,
            Some("Bob@coldeve is away: brb"),
        ),
        (Outcome::Squelched, None, Hr::OK, None),
        (Outcome::Offline, None, Hr::NOT_ONLINE, None),
        (
            Outcome::Unavailable,
            None,
            Hr::TIMED_OUT,
            Some("The Dereth Network can't reach coldeve right now."),
        ),
        (
            Outcome::NotOnNetwork,
            None,
            Hr::TIMED_OUT,
            Some("coldeve isn't on the network."),
        ),
        (
            Outcome::RateLimited,
            None,
            Hr::DENIED,
            Some(
                "The Dereth Network is holding back tells from this world to Bob@coldeve for \
                 now. Wait a moment and try again.",
            ),
        ),
    ] {
        let cx = w.request(
            ann,
            Request::SendToUserEx {
                user: "Bob@coldeve".into(),
                text: "hi".into(),
                extra: Some(auberean_friends::Extra([1; 16])),
            },
        );
        assert!(w.take(ann).is_empty(), "waits for the hub");
        let sent = w.frames();
        let Some(Message::Tell {
            id, from, to, text, ..
        }) = sent.into_iter().next()
        else {
            panic!("a tell");
        };
        assert_eq!(
            (from, to, text.as_str()),
            (addr("Ann@dawnsong"), addr("Bob@coldeve"), "hi")
        );
        w.hub(&Message::TellResult {
            id: id.clone(),
            outcome,
            message: message.map(str::to_owned),
        });
        let got = w.take(ann);
        assert_eq!(answer(&got, cx).0, hr, "{outcome:?}");
        assert_eq!(
            w.lines_of("Ann"),
            line.map(str::to_owned).into_iter().collect::<Vec<_>>(),
            "{outcome:?}"
        );
        // The same answer again changes nothing: one answer for each request.
        w.hub(&Message::TellResult {
            id,
            outcome,
            message: None,
        });
        assert!(w.take(ann).is_empty());
        w.advance(2);
    }
    w.told("x1", "Bob@coldeve", "Ann@dawnsong", "hey");
    assert_eq!(
        events(&w.take(ann)),
        vec![Event::UTellEv {
            user: "Bob@coldeve".into(),
            text: "hey".into(),
            extra: None,
        }]
    );
}

/// A tell the hub never answers ends after its deadline with the could-not-be-sent code, and
/// the line naming the world that could not be reached.
/// Behaviour: server.friends.remote-tell
#[test]
fn a_tell_the_hub_never_answers_ends_with_the_timed_out_code() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.frames();
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: "Bob@coldeve".into(),
            text: "hi".into(),
            extra: None,
        },
    );
    w.frames();
    for i in 0..99 {
        if i % 10 == 0 {
            // The client keeps its stream alive.
            w.send(ann, &Element::Ping { seq: i });
        }
        w.advance(1);
    }
    let got = w.take(ann);
    assert!(
        got.iter().all(|e| !matches!(e, Element::Answer { .. })),
        "not yet: {got:?}"
    );
    w.advance(1);
    assert_eq!(answer(&w.take(ann), cx).0, Hr::TIMED_OUT);
    assert_eq!(
        w.lines_of("Ann"),
        vec!["The Dereth Network can't reach coldeve right now.".to_owned()]
    );
}

/// Each way a tell to another world ends has one code, chosen from the client's own lines.
/// Behaviour: server.friends.remote-tell
#[test]
fn each_end_of_a_tell_has_the_code_whose_line_fits_it() {
    use yalain_federation::Ended;
    for (e, hr) in [
        (Ended::Delivered, Hr::OK),
        (Ended::Away, Hr::OK),
        (Ended::Squelched, Hr::OK),
        (Ended::Offline, Hr::NOT_ONLINE),
        (Ended::Unavailable, Hr::TIMED_OUT),
        (Ended::NotOnNetwork, Hr::TIMED_OUT),
        (Ended::NetworkUnavailable, Hr::TIMED_OUT),
        (Ended::RateLimited, Hr::DENIED),
        (Ended::TooQuickly, Hr::DENIED),
        (Ended::NotOn, Hr::DENIED),
        (Ended::WorldOff, Hr::DENIED),
    ] {
        assert_eq!(tell_code(e), hr, "{e:?}");
    }
}

impl World {
    /// A tell from another world.
    pub fn told(&mut self, id: &str, from: &str, to: &str, text: &str) {
        self.hub(&Message::Tell {
            id: id.into(),
            from: addr(from),
            to: addr(to),
            text: text.into(),
            friend: false,
        });
    }
}

/// An add of a friend on another world, as the client takes it: its state as one event, then the
/// add's answer, and nothing else.
fn added(got: &[Element], cx: Cx, user: &str, mode: Mode) {
    let stat_at = got
        .iter()
        .position(|e| matches!(e, Element::Event(Event::FriendStat { .. })));
    let answer_at = got
        .iter()
        .position(|e| matches!(e, Element::Answer { cx: c, .. } if *c == cx));
    assert_eq!(events(got), vec![stat(user, mode)], "{got:?}");
    assert_eq!(answer(got, cx), (Hr::OK, Answer::FriendAdd));
    assert!(stat_at < answer_at, "the state before the answer: {got:?}");
}

/// A friend added on another world is shown once, with the state its world reports for it,
/// online or not, followed by the add's answer; never offline first and then its real state,
/// which the client would read as the friend leaving the world.
/// Behaviour: server.friends.remote-add-shown-once
#[test]
fn a_friend_added_on_another_world_is_shown_once_with_the_state_its_world_reports() {
    for online in [true, false] {
        let mut w = World::linked();
        let ann = w.player("Ann");
        w.on("Ann");
        w.frames();
        let cx = w.request(
            ann,
            Request::FriendAdd {
                user: "Cara@coldeve".into(),
            },
        );
        assert!(w.take(ann).is_empty(), "nothing before its world answers");
        w.hub(&Message::Presence {
            to: addr("Ann@dawnsong"),
            of: addr("Cara@coldeve"),
            online,
        });
        let mode = if online { Mode::ONLINE } else { Mode::OFFLINE };
        added(&w.take(ann), cx, "Cara@coldeve", mode);
        w.advance(10);
        assert!(w.take(ann).is_empty(), "and nothing after");
    }
}

/// With nobody to ask, a friend added on another world is shown offline at once: when the link is
/// down, or the friend's world is not linked.
/// Behaviour: server.friends.remote-add-shown-once
#[test]
fn a_friend_added_on_a_world_that_cannot_be_asked_is_shown_offline_at_once() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Cara@nowhere".into(),
        },
    );
    added(&w.take(ann), cx, "Cara@nowhere", Mode::OFFLINE);

    let t = w.now.as_secs_f64();
    let f = w.fed.as_mut().expect("the link's half");
    f.on_link_down(&mut w.service, t);
    w.flush();
    w.take(ann);
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Cara@coldeve".into(),
        },
    );
    added(&w.take(ann), cx, "Cara@coldeve", Mode::OFFLINE);
}

/// A friend added on another world whose world does not answer within three seconds is shown
/// offline then; an answer after that is shown only if it says online.
/// Behaviour: server.friends.remote-add-shown-once
#[test]
fn a_friend_added_on_another_world_that_does_not_answer_is_shown_offline_after_three_seconds() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Cara@coldeve".into(),
        },
    );
    w.advance(2);
    assert!(w.take(ann).is_empty(), "still waiting at two seconds");
    w.advance(1);
    added(&w.take(ann), cx, "Cara@coldeve", Mode::OFFLINE);
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Cara@coldeve"),
        online: false,
    });
    assert!(events(&w.take(ann)).is_empty(), "offline again is no news");
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Cara@coldeve"),
        online: true,
    });
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cara@coldeve", Mode::ONLINE)]
    );
}

/// A rename reaches the network at once, without a restart: other worlds see the old address go, and
/// the character goes on as the new one. It stays on the network, watches its friends under its new
/// name, and answers to it, and a friend's world answering again shows its client nothing new.
/// Behaviour: server.network.rename-seen-at-once
#[test]
fn a_renamed_character_goes_on_on_the_network_under_its_new_name() {
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

    w.rename("Ann", "Anna");
    let sent = w.frames();
    let of_old = |to: &str| Message::Presence {
        to: addr(to),
        of: addr("Ann@dawnsong"),
        online: false,
    };
    assert!(sent.contains(&of_old("Cid@coldeve")), "{sent:?}");
    assert!(sent.contains(&of_old("Bob@coldeve")), "{sent:?}");
    for (owner, on) in [("Ann@dawnsong", false), ("Anna@dawnsong", true)] {
        let watch = Message::Watch {
            owner: addr(owner),
            target: addr("Bob@coldeve"),
            on,
        };
        assert!(sent.contains(&watch), "{owner}: {sent:?}");
    }
    assert!(w.fed.as_ref().expect("linked").is_on("Anna"));
    assert!(!w.fed.as_ref().expect("linked").is_on("Ann"));
    assert_eq!(
        w.fed.as_ref().expect("linked").watcher_count("Ann"),
        0,
        "nobody goes on watching the old address"
    );
    assert!(
        Friends::remote_friends(&w.service, "Ann").is_empty(),
        "the old name holds nothing"
    );

    // Its friend's world answers the new watch: what its client shows already stands.
    w.hub(&Message::Presence {
        to: addr("Anna@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    assert!(events(&w.take(ann)).is_empty());
    w.hub(&Message::Presence {
        to: addr("Anna@dawnsong"),
        of: addr("Bob@coldeve"),
        online: false,
    });
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Bob@coldeve", Mode::OFFLINE)]
    );

    // A watch of each name is answered by what each now is.
    for (target, online) in [("Ann@dawnsong", false), ("Anna@dawnsong", true)] {
        w.hub(&Message::Watch {
            owner: addr("Dee@coldeve"),
            target: addr(target),
            on: true,
        });
        let answered = Message::Presence {
            to: addr("Dee@coldeve"),
            of: addr(target),
            online,
        };
        let sent = w.frames();
        assert!(sent.contains(&answered), "{target}: {sent:?}");
    }

    // A tell from another world reaches it under the new name.
    w.hub(&Message::Tell {
        id: "t1".into(),
        from: addr("Bob@coldeve"),
        to: addr("Anna@dawnsong"),
        text: "Hi".into(),
        friend: true,
    });
    assert_eq!(
        events(&w.take(ann)),
        vec![Event::UTellEv {
            user: "Bob@coldeve".into(),
            text: "Hi".into(),
            extra: None,
        }]
    );
}

/// A character off the network, renamed, sends the network nothing.
/// Behaviour: server.network.rename-seen-at-once
#[test]
fn a_rename_off_the_network_tells_the_network_nothing() {
    let mut w = World::linked();
    w.player("Ann");
    w.frames();
    w.rename("Ann", "Anna");
    assert_eq!(w.frames(), Vec::new());
    assert!(!w.fed.as_ref().expect("linked").is_on("Anna"));
}

/// A friend on another world being added as its adder is renamed is answered all the same, by the
/// answer its world gives the new name's watch.
/// Behaviour: server.network.rename-seen-at-once
#[test]
fn an_add_waiting_for_its_friends_world_is_answered_after_a_rename() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert!(w.take(ann).is_empty(), "waiting");
    w.rename("Ann", "Anna");
    w.hub(&Message::Presence {
        to: addr("Anna@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    let got = w.take(ann);
    assert_eq!(answer(&got, cx).0, Hr::OK);
    assert_eq!(events(&got), vec![stat("Bob@coldeve", Mode::ONLINE)]);
}

/// An add shown offline for want of an answer stays so after a rename: its world's late offline
/// shows nothing more.
/// Behaviour: server.network.rename-seen-at-once
#[test]
fn an_add_assumed_offline_stays_so_after_a_rename() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    w.advance(yalain_friends::ADD_WAIT.as_secs());
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    w.rename("Ann", "Anna");
    w.hub(&Message::Presence {
        to: addr("Anna@dawnsong"),
        of: addr("Bob@coldeve"),
        online: false,
    });
    assert!(events(&w.take(ann)).is_empty());
}

/// A friend's state waiting for its pace as its lister is renamed is shown when its time comes.
/// Behaviour: server.network.rename-seen-at-once
#[test]
fn a_friends_state_waiting_for_its_pace_is_shown_after_a_rename() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    let mut online = true;
    for _ in 0..7 {
        w.hub(&Message::Presence {
            to: addr("Ann@dawnsong"),
            of: addr("Bob@coldeve"),
            online,
        });
        online = !online;
    }
    let last_said = !online;
    w.take(ann);
    w.rename("Ann", "Anna");
    assert!(events(&w.take(ann)).is_empty(), "held to its pace");
    w.advance(10);
    let mode = if last_said {
        Mode::ONLINE
    } else {
        Mode::OFFLINE
    };
    assert_eq!(events(&w.take(ann)), vec![stat("Bob@coldeve", mode)]);
}

/// A character hidden on the network and renamed says nothing to the network either way, and watches
/// nobody until it comes back.
/// Behaviour: server.network.rename-seen-at-once
#[test]
fn a_hidden_character_renamed_says_nothing_to_the_network() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    let (_, id) = w.ids("Ann");
    let hide = |w: &mut World, ids: &[u64]| {
        w.service.set_hidden(
            w.now,
            ids.iter().copied().collect(),
            &mut w.store,
            w.fed.as_mut(),
        );
        w.flush();
    };
    hide(&mut w, &[id]);
    w.frames();
    w.rename("Ann", "Anna");
    assert_eq!(w.frames(), Vec::new());
    hide(&mut w, &[]);
    let sent = w.frames();
    assert!(
        sent.contains(&Message::Watch {
            owner: addr("Anna@dawnsong"),
            target: addr("Bob@coldeve"),
            on: true,
        }),
        "{sent:?}"
    );
}
