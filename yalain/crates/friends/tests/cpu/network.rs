//! Other worlds' characters: friends and tells through the world's link.

use super::*;
use dereth_network_protocol::{Outcome, PersonOp, PersonOutcome};
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

/// The client was told a friend on another world is offline when it was told its friends, and prints
/// a line for every state: the first report of offline shows nothing, a first report of online does,
/// and a later change either way does.
/// Behaviour: server.friends.first-state-offline-quiet
#[test]
fn a_friend_first_reported_offline_is_not_announced_as_having_left() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    for friend in ["Bob@coldeve", "Cal@coldeve"] {
        let cx = w.request(
            ann,
            Request::FriendAdd {
                user: friend.into(),
            },
        );
        w.hub(&Message::Presence {
            to: addr("Ann@dawnsong"),
            of: addr(friend),
            online: false,
        });
        assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    }
    w.leave("Ann");
    w.enter("Ann");
    w.take(ann);
    w.frames();
    let say = |w: &mut World, of: &str, online: bool| {
        w.hub(&Message::Presence {
            to: addr("Ann@dawnsong"),
            of: addr(of),
            online,
        });
    };
    say(&mut w, "Bob@coldeve", false);
    say(&mut w, "Cal@coldeve", true);
    assert_eq!(
        events(&w.take(ann)),
        vec![stat("Cal@coldeve", Mode::ONLINE)]
    );
    say(&mut w, "Bob@coldeve", true);
    say(&mut w, "Cal@coldeve", false);
    assert_eq!(
        events(&w.take(ann)),
        vec![
            stat("Bob@coldeve", Mode::ONLINE),
            stat("Cal@coldeve", Mode::OFFLINE)
        ]
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

/// A world with `Ann` present on the network, and `Dee` away, on the network and linked by herself.
fn with_dee_away() -> (World, StreamId) {
    let mut w = World::linked();
    let dee = w.player("Dee");
    w.on("Dee");
    assert_eq!(w.said("Dee", "link"), Vec::<String>::new());
    w.leave("Dee");
    w.drop_stream(dee);
    let ann = w.player("Ann");
    w.on("Ann");
    w.frames();
    w.take(ann);
    (w, ann)
}

/// A tell by address to an away, linked character of this world goes through the link and is
/// answered as the hub says, not logged in, with no line of the world's; the link down, it stays the
/// world's own, answered at once.
/// Behaviour: server.network.tell-away-linked
#[test]
fn a_tell_by_address_to_an_away_linked_character_goes_through_the_link() {
    let (mut w, ann) = with_dee_away();
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: here("Dee"),
            text: "you there?".into(),
            extra: None,
        },
    );
    assert!(w.take(ann).is_empty(), "answered when it ends");
    let sent = w.frames();
    let [Message::Tell {
        id, from, to, text, ..
    }] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    assert_eq!(
        (
            from.to_string().as_str(),
            to.to_string().as_str(),
            text.as_str()
        ),
        ("Ann@dawnsong", "Dee@dawnsong", "you there?")
    );
    w.hub(&Message::TellResult {
        id: id.clone(),
        outcome: Outcome::Offline,
        message: None,
    });
    assert_eq!(answer(&w.take(ann), cx).0, Hr::NOT_ONLINE);
    assert!(w.lines_of("Ann").is_empty());

    let t = w.now.as_secs_f64();
    w.fed
        .as_mut()
        .expect("the link's half")
        .on_link_down(&mut w.service, t);
    w.flush();
    w.take(ann);
    let cx = w.request(
        ann,
        Request::SendToUserEx {
            user: here("Dee"),
            text: "you there?".into(),
            extra: None,
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::NOT_ONLINE);
    assert!(w.frames().is_empty());
}

/// The game's own tell, which reached no player in the world and has been answered there, goes on to
/// the hub when its name is an away, linked character's, and nobody waits on its end but for its
/// line. Any other name goes nowhere.
/// Behaviour: server.network.tell-away-linked
#[test]
fn the_games_own_tell_to_an_away_linked_character_goes_on_to_the_hub() {
    let (mut w, ann) = with_dee_away();
    let (_, id) = w.ids("Ann");
    let now = w.now;
    w.service
        .unheard_tell(now, id, "Ann", "Nobody", "hi", w.fed.as_mut());
    assert!(w.frames().is_empty());
    w.service
        .unheard_tell(now, id, "Ann", "dee", "you there?", w.fed.as_mut());
    w.flush();
    let sent = w.frames();
    let [Message::Tell {
        id: tell, to, text, ..
    }] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    assert_eq!(
        (to.to_string().as_str(), text.as_str()),
        ("Dee@dawnsong", "you there?")
    );
    w.hub(&Message::TellResult {
        id: tell.clone(),
        outcome: Outcome::Offline,
        message: None,
    });
    assert!(w.take(ann).is_empty(), "no request waits on it");

    // Spent until the teller's own pace holds one back: the line is still shown.
    let mut lines = Vec::new();
    for _ in 0..1_000 {
        w.service
            .unheard_tell(now, id, "Ann", "Dee", "and again", w.fed.as_mut());
        w.flush();
        lines = w.lines_of("Ann");
        if !lines.is_empty() {
            break;
        }
    }
    assert_eq!(lines, [yalain_federation::TOO_QUICKLY.to_owned()]);
}

/// An away, linked character's reply from elsewhere reaches a present character as a tell written as
/// the world writes its own, so the client's reply to it goes within the world.
/// Behaviour: server.network.tell-away-linked
#[test]
fn a_reply_from_an_away_linked_character_is_written_as_one_of_the_worlds_own() {
    let (mut w, ann) = with_dee_away();
    w.hub(&Message::Tell {
        id: "h1".into(),
        from: addr("Dee@dawnsong"),
        to: addr("Ann@dawnsong"),
        text: "hello back".into(),
        friend: false,
    });
    assert_eq!(
        events(&w.take(ann)),
        vec![Event::UTellEv {
            user: here("Dee"),
            text: "hello back".into(),
            extra: None,
        }]
    );
}

/// The hub's question about a teller of this world is answered by the list the world keeps for the
/// character away.
/// Behaviour: server.network.reach-answered
#[test]
fn the_list_kept_for_a_character_away_decides_the_hubs_question_about_a_teller_of_this_world() {
    let (mut w, ann) = with_dee_away();
    let ask = |w: &mut World| -> bool {
        let t = w.now.as_secs_f64();
        let frame = dereth_network_protocol::ReachMessage::Ask {
            id: "h1".into(),
            from: addr("Ann@dawnsong"),
            to: addr("Dee@dawnsong"),
        }
        .to_json();
        let f = w.fed.as_mut().expect("the link's half");
        f.on_frame(&mut w.service, t, &frame);
        let frames = f.take_frames(t);
        match dereth_network_protocol::ReachMessage::from_json(&frames[0]).expect("reads") {
            dereth_network_protocol::ReachMessage::Result { reachable, .. } => reachable,
            other => panic!("{other:?}"),
        }
    };
    assert!(ask(&mut w));
    let dee = w.stream("Dee");
    w.enter("Dee");
    let cx = w.request(dee, Request::IgnoredAdd { user: here("Ann") });
    assert_eq!(answer(&w.take(dee), cx).0, Hr::OK);
    w.leave("Dee");
    w.drop_stream(dee);
    w.take(ann);
    w.frames();
    assert!(!ask(&mut w));
}

/// A view saying a character is linked, its person friends as (handle, characters).
fn person_view(to: &str, persons: &[(&str, &[&str])]) -> Message {
    Message::PersonFriends {
        to: addr(to),
        linked: true,
        alts: Default::default(),
        persons: persons
            .iter()
            .map(|(h, cs)| dereth_network_protocol::PersonFriend {
                handle: addr(h),
                characters: cs.iter().map(|c| addr(c)).collect(),
            })
            .collect(),
    }
}

/// `Ann` present, on the network, her link her own and her first view come with these person friends.
fn linked_ann(persons: &[(&str, &[&str])]) -> (World, StreamId) {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    assert_eq!(w.said("Ann", "link"), Vec::<String>::new());
    w.hub(&person_view("Ann@dawnsong", persons));
    w.frames();
    w.take(ann);
    (w, ann)
}

fn listed(got: &[Element], cx: Cx) -> Vec<(String, Mode)> {
    match answer(got, cx) {
        (Hr::OK, Answer::GetFriendsList { friends }) => {
            friends.into_iter().map(|f| (f.user, f.mode)).collect()
        }
        other => panic!("{other:?}"),
    }
}

/// The client's page holds what its list said, and it asks once as the character enters: that ask
/// waits for the character's first person view, so the page has its person's friends from the start.
/// A view that doesn't come in three seconds is not waited for. A later ask waits for nothing.
/// Behaviour: server.friends.first-list-waits-for-the-view
#[test]
fn the_clients_first_ask_for_its_friends_waits_for_the_first_person_view() {
    let (mut w, ann) = linked_ann(&[]);
    w.leave("Ann");
    w.enter("Ann");
    w.take(ann);
    w.frames();
    let cx = w.request(ann, Request::GetFriendsList);
    assert!(w.take(ann).is_empty(), "waiting for the view");
    w.hub(&person_view(
        "Ann@dawnsong",
        &[("Bob@coldeve", &["Bob@coldeve", "Rob@frost"])],
    ));
    assert_eq!(
        listed(&w.take(ann), cx),
        [
            ("Bob@coldeve".to_owned(), Mode::OFFLINE),
            ("Rob@frost".to_owned(), Mode::OFFLINE),
        ]
    );
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(
        listed(&w.take(ann), cx).len(),
        2,
        "a later ask waits for nothing"
    );

    w.leave("Ann");
    w.enter("Ann");
    w.take(ann);
    let cx = w.request(ann, Request::GetFriendsList);
    assert!(w.take(ann).is_empty());
    w.advance(2);
    assert!(w.take(ann).is_empty());
    w.advance(1);
    assert_eq!(listed(&w.take(ann), cx), [], "no view came");

    // A character that never asked for its link waits for nothing, on the network or off it.
    let mut never = World::linked();
    let cara = never.player("Cara");
    let cx = never.request(cara, Request::GetFriendsList);
    assert_eq!(listed(&never.take(cara), cx), []);
    never.on("Cara");
    let cx = never.request(cara, Request::GetFriendsList);
    assert_eq!(listed(&never.take(cara), cx), []);
}

/// A stream that ends while its ask waits is forgotten.
/// Behaviour: server.friends.first-list-waits-for-the-view
#[test]
fn an_ask_for_friends_on_a_stream_that_ended_is_forgotten() {
    let (mut w, ann) = linked_ann(&[]);
    w.leave("Ann");
    w.enter("Ann");
    w.request(ann, Request::GetFriendsList);
    assert_eq!(w.service.next_wake(), Some(w.now + Duration::from_secs(3)));
    w.drop_stream(ann);
    assert_eq!(w.service.next_wake(), None);
}

/// A linked character's add on another world goes to the hub, which keeps it: nothing is kept here.
/// The hub's answer holds nothing back: once the view names it and its world answers, its state then
/// done, as any add. The client then asks for the list, which holds it.
/// Behaviour: server.network.person-friend-added
#[test]
fn a_linked_characters_add_is_the_hubs_and_answered_when_the_view_names_it() {
    let (mut w, ann) = linked_ann(&[]);
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert!(w.take(ann).is_empty());
    let sent = w.frames();
    let [Message::Person {
        op: PersonOp::Befriend,
        id,
        target: Some(target),
        ..
    }] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    assert!(target.same(&addr("Bob@coldeve")));
    assert_eq!(w.store.network_lists().expect("reads"), []);
    w.hub(&Message::PersonResult {
        id: id.clone(),
        outcome: PersonOutcome::Ok,
        code: None,
        expires_secs: None,
        url: None,
        characters: Vec::new(),
    });
    assert!(w.take(ann).is_empty(), "the row isn't there yet");
    assert!(
        w.lines_of("Ann").is_empty(),
        "the client writes its own line"
    );
    w.hub(&person_view(
        "Ann@dawnsong",
        &[("Bob@coldeve", &["Bob@coldeve"])],
    ));
    assert!(w.take(ann).is_empty(), "its world is asked");
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    let got = w.take(ann);
    assert_eq!(events(&got), vec![stat("Bob@coldeve", Mode::ONLINE)]);
    assert_eq!(answer(&got, cx).0, Hr::OK);
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(
        listed(&w.take(ann), cx),
        [("Bob@coldeve".to_owned(), Mode::ONLINE)]
    );
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, yalain_friends::ALREADY_IN_LIST);
}

/// A linked character's add the hub refuses is answered so, beside the world's line; one the view
/// doesn't name in three seconds is shown offline and answered done; a view naming a friend on a world
/// that can't be asked answers it at once, offline.
/// Behaviour: server.network.person-friend-added
#[test]
fn a_linked_characters_add_ends_however_the_hub_answers() {
    let (mut w, ann) = linked_ann(&[]);
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    let sent = w.frames();
    let [Message::Person { id, .. }] = sent.as_slice() else {
        panic!("{sent:?}");
    };
    w.hub(&Message::PersonResult {
        id: id.clone(),
        outcome: PersonOutcome::Refused,
        code: None,
        expires_secs: None,
        url: None,
        characters: Vec::new(),
    });
    assert_eq!(answer(&w.take(ann), cx).0, FRIEND_REFUSED);
    assert_eq!(
        w.lines_of("Ann"),
        vec![dereth_network_protocol::answers::FRIENDS_FULL.to_owned()]
    );

    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Cal@coldeve".into(),
        },
    );
    w.frames();
    w.advance(3);
    let got = w.take(ann);
    assert_eq!(events(&got), vec![stat("Cal@coldeve", Mode::OFFLINE)]);
    assert_eq!(answer(&got, cx).0, Hr::OK);

    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Dee@unlinked".into(),
        },
    );
    w.frames();
    w.hub(&person_view(
        "Ann@dawnsong",
        &[("Dee@unlinked", &["Dee@unlinked"])],
    ));
    let got = w.take(ann);
    assert_eq!(events(&got), vec![stat("Dee@unlinked", Mode::OFFLINE)]);
    assert_eq!(answer(&got, cx).0, Hr::OK);
}

/// With the link down a linked character keeps the friend itself: the hub can't be asked.
/// Behaviour: server.network.person-friends-carried
#[test]
fn a_linked_characters_add_with_the_link_down_is_kept() {
    let (mut w, ann) = linked_ann(&[]);
    let t = w.now.as_secs_f64();
    w.fed
        .as_mut()
        .expect("the link's half")
        .on_link_down(&mut w.service, t);
    w.flush();
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    assert_eq!(w.store.network_lists().expect("reads").len(), 1);
}

/// `@dn friends add` from a linked character goes to the hub too, its line coming with the answer;
/// `@dn friends` lists what the view does beside what it keeps, and `@dn` counts them.
/// Behaviour: server.network.person-friend-added
#[test]
fn the_dn_friends_commands_of_a_linked_character_are_the_hubs() {
    let (mut w, _) = linked_ann(&[("Bob@coldeve", &["Bob@coldeve", "Rob@frost"])]);
    assert_eq!(
        w.said("Ann", "friends"),
        [
            "Your friends on other worlds:".to_owned(),
            "Bob@coldeve: offline".to_owned(),
            "Rob@frost: offline".to_owned(),
        ]
    );
    assert!(w
        .said("Ann", "")
        .join(" ")
        .contains("You have 2 friends on other worlds."));
    assert_eq!(
        w.said("Ann", "friends add Rob@frost"),
        ["That character is already in your friends list".to_owned()]
    );
    assert_eq!(w.said("Ann", "friends add Cal@frost"), Vec::<String>::new());
    let sent = w.frames();
    let [Message::Person {
        op: PersonOp::Befriend,
        id,
        ..
    }] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    w.hub(&Message::PersonResult {
        id: id.clone(),
        outcome: PersonOutcome::Ok,
        code: None,
        expires_secs: None,
        url: None,
        characters: Vec::new(),
    });
    assert_eq!(
        w.lines_of("Ann"),
        vec!["Cal@frost has been added to your friends list.".to_owned()]
    );
    assert_eq!(
        w.said("Ann", "friends remove Rob@frost"),
        ["Rob@frost has been removed from your friends list.".to_owned()]
    );
    assert!(w
        .frames()
        .iter()
        .any(|m| matches!(m, Message::Person { op: PersonOp::Unfriend, target: Some(h), .. } if h.same(&addr("Bob@coldeve")))));
}

/// Removing a row a person friend listed is answered done and removes that person friend at the hub.
/// A row that went in play is still on the client's page: its removal is answered done too, which
/// takes it off, until the client's next list. A name never listed is not in the list.
/// Behaviour: server.network.person-friend-removed
#[test]
fn removing_a_row_is_answered_done_while_the_client_may_show_it() {
    let (mut w, ann) = linked_ann(&[
        ("Bob@coldeve", &["Bob@coldeve"]),
        ("Cal@frost", &["Cal@frost"]),
    ]);
    let remove = |w: &mut World, user: &str| {
        let cx = w.request(ann, Request::FriendDelete { user: user.into() });
        answer(&w.take(ann), cx).0
    };
    assert_eq!(remove(&mut w, "Bob@coldeve"), Hr::OK);
    let sent = w.frames();
    assert!(sent.iter().any(|m| matches!(
        m,
        Message::Person { op: PersonOp::Unfriend, target: Some(h), .. } if h.same(&addr("Bob@coldeve"))
    )));
    assert!(sent.iter().any(|m| matches!(
        m,
        Message::Watch { target, on: false, .. } if target.same(&addr("Bob@coldeve"))
    )));
    w.hub(&person_view("Ann@dawnsong", &[]));
    w.frames();
    assert_eq!(
        remove(&mut w, "Cal@frost"),
        Hr::OK,
        "the view took it; the page has it"
    );
    assert_eq!(
        remove(&mut w, "Cal@frost"),
        yalain_friends::NOT_IN_LIST,
        "once"
    );
    assert_eq!(remove(&mut w, "Zed@frost"), yalain_friends::NOT_IN_LIST);

    w.hub(&person_view(
        "Ann@dawnsong",
        &[("Eli@frost", &["Eli@frost"])],
    ));
    w.hub(&person_view("Ann@dawnsong", &[]));
    w.frames();
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(listed(&w.take(ann), cx), []);
    assert_eq!(
        remove(&mut w, "Eli@frost"),
        yalain_friends::NOT_IN_LIST,
        "the page was given its list"
    );
}

/// A friend a linked character kept that the view names as a handle is taken off its kept list, in
/// the store too, and listed once; the friends handed back after an unlink are kept in the store from
/// then on.
/// Behaviour: server.network.person-friends-carried
#[test]
fn what_the_view_takes_and_hands_back_reaches_the_store() {
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
        online: false,
    });
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    assert_eq!(w.store.network_lists().expect("reads").len(), 1);
    assert_eq!(w.said("Ann", "link"), Vec::<String>::new());
    w.hub(&person_view(
        "Ann@dawnsong",
        &[("Bob@coldeve", &["Bob@coldeve"])],
    ));
    assert_eq!(
        w.store.network_lists().expect("reads"),
        [],
        "the person's now"
    );
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(listed(&w.take(ann), cx).len(), 1, "listed once");
    w.hub(&Message::PersonFriends {
        to: addr("Ann@dawnsong"),
        linked: false,
        alts: Default::default(),
        persons: vec![dereth_network_protocol::PersonFriend {
            handle: addr("Bob@coldeve"),
            characters: vec![addr("Bob@coldeve")],
        }],
    });
    let kept = w.store.network_lists().expect("reads");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].entry, "Bob@coldeve");
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(listed(&w.take(ann), cx).len(), 1, "still listed once");
}

/// What a character keeps and what its person friends list hold at most 100 names together: past
/// that an add is refused with the world's line, by the hub's way or kept.
/// Behaviour: server.network.person-friend-added
#[test]
fn an_add_past_what_a_character_may_list_is_refused_with_the_rows_counted() {
    let names: Vec<String> = (0..100u8)
        .map(|n| {
            format!(
                "A{}{}@coldeve",
                (b'a' + n / 26) as char,
                (b'a' + n % 26) as char
            )
        })
        .collect();
    let chars: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut w = World::linked();
    w.character("Cara");
    let ann = w.player("Ann");
    w.on("Ann");
    assert_eq!(w.said("Ann", "link"), Vec::<String>::new());
    let persons: Vec<(&str, &[&str])> = chars
        .iter()
        .map(|c| (*c, std::slice::from_ref(c)))
        .collect();
    w.hub(&person_view("Ann@dawnsong", &persons));
    w.frames();
    w.take(ann);
    assert_eq!(w.said("Ann", "friends").len(), 101);
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, FRIEND_REFUSED);
    assert!(w.frames().is_empty(), "the hub isn't asked");
    assert_eq!(
        w.lines_of("Ann"),
        vec![yalain_friends::FRIENDS_FULL.to_owned()]
    );
    let cx = w.request(ann, Request::FriendAdd { user: here("Cara") });
    assert_eq!(
        answer(&w.take(ann), cx).0,
        FRIEND_REFUSED,
        "one of this world's"
    );
    assert_eq!(
        w.lines_of("Ann"),
        vec![yalain_friends::FRIENDS_FULL.to_owned()]
    );
}
