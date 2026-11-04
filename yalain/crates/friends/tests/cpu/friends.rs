//! Friends and the ignore list within the world, and the names they are written in.

use super::*;
use yalain_friends::{ALREADY_IN_LIST, FRIENDS_FULL, FRIEND_REFUSED, MAX_LISTED, NOT_IN_LIST};

/// A friend of this world must be a character of it: the friend's state goes first as an event,
/// then the answer. A name with no world, with the world's name in any case, or with its slug, is
/// this world's, and is written back with the world's name exactly. A name already listed, one
/// not listed, and one that is no address are answered with the codes whose lines say so.
/// Behaviour: server.friends.friend-added-and-removed
#[test]
fn a_friend_of_this_world_is_added_with_its_state_first_and_removed_by_any_of_its_names() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.player("Cara");
    w.character("Dan");
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Cara@DAWNSONG".into(),
        },
    );
    let got = w.take(ann);
    assert_eq!(
        got.iter()
            .filter(|e| matches!(e, Element::Event(_) | Element::Answer { .. }))
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            Element::Event(stat("Cara@Dawnsong", Mode::ONLINE)),
            Element::Answer {
                cx,
                hr: Hr::OK,
                answer: Answer::FriendAdd
            },
        ],
        "the state, then the answer"
    );
    for (user, hr) in [
        ("cara", ALREADY_IN_LIST),
        ("Cara@dawnsong", ALREADY_IN_LIST),
        ("Nobody@Dawnsong", Hr::NO_SUCH_USER),
        ("Cara@Dawn@song", Hr::NO_SUCH_USER),
        ("Ann@Dawnsong", Hr::NO_SUCH_USER),
    ] {
        let cx = w.request(ann, Request::FriendAdd { user: user.into() });
        assert_eq!(answer(&w.take(ann), cx).0, hr, "{user}");
    }
    let cx = w.request(ann, Request::FriendAdd { user: "Dan".into() });
    let got = w.take(ann);
    assert_eq!(answer(&got, cx).0, Hr::OK);
    assert_eq!(events(&got), vec![stat("Dan@Dawnsong", Mode::OFFLINE)]);
    let cx = w.request(ann, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(ann), cx),
        (
            Hr::OK,
            Answer::GetFriendsList {
                friends: vec![
                    status("Cara@Dawnsong", Mode::ONLINE),
                    status("Dan@Dawnsong", Mode::OFFLINE)
                ]
            }
        )
    );
    let cx = w.request(
        ann,
        Request::FriendDelete {
            user: "dan@dawnsong".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    let cx = w.request(ann, Request::FriendDelete { user: "Dan".into() });
    assert_eq!(answer(&w.take(ann), cx).0, NOT_IN_LIST);
    let (_, id) = w.ids("Ann");
    assert_eq!(
        w.store
            .network_lists()
            .expect("reads")
            .into_iter()
            .filter(|l| l.character == id)
            .map(|l| l.entry)
            .collect::<Vec<_>>(),
        vec!["Cara".to_owned()],
        "kept in the store as the character's name"
    );
}

/// A character keeps at most a hundred friends: past that it is told its list is full, with a
/// code the client shows as its list's own error.
/// Behaviour: server.friends.lists-capped
#[test]
fn a_friends_list_holds_a_hundred_and_the_next_is_refused_with_a_line() {
    let mut w = World::new();
    let ann = w.player("Ann");
    for i in 0..=MAX_LISTED {
        let name = format!(
            "Friend {}",
            char::from(b'a' + u8::try_from(i % 26).expect("small"))
        ) + &"x".repeat(i / 26 + 1);
        w.character(&name);
        let cx = w.request(ann, Request::FriendAdd { user: name.clone() });
        // Below the stream's own bound on requests.
        w.now += std::time::Duration::from_millis(250);
        let hr = answer(&w.take(ann), cx).0;
        if i < MAX_LISTED {
            assert_eq!(hr, Hr::OK, "{i}");
        } else {
            assert_eq!(hr, FRIEND_REFUSED);
        }
    }
    assert_eq!(w.lines_of("Ann"), vec![FRIENDS_FULL.to_owned()]);
}

/// The ignore list keeps characters of this world (which must exist) and other worlds'
/// addresses; it is listed back with this world's names written as the client knows them, and
/// what is listed already or not listed is answered so.
/// Behaviour: server.friends.ignore-list-kept
#[test]
fn the_ignore_list_keeps_names_of_this_world_and_addresses_of_others() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.character("Cara");
    for (user, hr) in [
        ("Cara", Hr::OK),
        ("Bob@coldeve", Hr::OK),
        ("cara@dawnsong", ALREADY_IN_LIST),
        ("Nobody", Hr::NO_SUCH_USER),
    ] {
        let cx = w.request(ann, Request::IgnoredAdd { user: user.into() });
        assert_eq!(answer(&w.take(ann), cx).0, hr, "{user}");
    }
    let cx = w.request(ann, Request::GetIgnoredList);
    assert_eq!(
        answer(&w.take(ann), cx),
        (
            Hr::OK,
            Answer::GetIgnoredList {
                ignored: vec!["Cara@Dawnsong".into(), "Bob@coldeve".into()]
            }
        )
    );
    let (_, ann_id) = w.ids("Ann");
    let ignores = w.service.local_ignores().expect("changed");
    assert_eq!(
        ignores
            .get(&ann_id)
            .map(|s| s.iter().cloned().collect::<Vec<_>>()),
        Some(vec!["cara".to_owned()]),
        "this world's names, for the game's own tell"
    );
    assert!(w.service.local_ignores().is_none(), "unchanged since");
    let cx = w.request(
        ann,
        Request::IgnoredDelete {
            user: "BOB@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    let cx = w.request(
        ann,
        Request::IgnoredDelete {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, NOT_IN_LIST);
}

/// A world started again reads every character's lists from the store: its friends come with
/// its next entry.
/// Behaviour: server.friends.lists-kept
#[test]
fn the_lists_outlive_a_restart_of_the_service() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.character("Cara");
    w.request(
        ann,
        Request::FriendAdd {
            user: "Cara".into(),
        },
    );
    let mut again = World::new();
    again.store = w.store.clone();
    again.service = Service::load(Config::new(NAME, ""), &mut again.store).expect("loads");
    let s = again.stream("Ann");
    again.enter("Ann");
    assert_eq!(
        events(&again.take(s)),
        vec![Event::FStart {
            friends: vec![status("Cara@Dawnsong", Mode::OFFLINE)]
        }]
    );
}

/// Friend and ignore entries follow a rename, for a character in the world or not, and for good: the
/// lists it is asked for, what it can take off, the world's own tell's names, and the lists as a
/// later start reads them. An entry for another world's character of that name stays. One who lists
/// both names lists it once.
/// Behaviour: server.friends.lists-follow-a-rename
#[test]
fn other_characters_lists_hold_a_renamed_character_by_its_new_name() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    let (_, ann_id) = w.ids("Ann");
    w.player("Bob");
    w.character("Robertson");
    for user in ["Bob", "Robertson", "Bob@coldeve"] {
        w.request(ann, Request::FriendAdd { user: user.into() });
    }
    for user in ["Bob", "Robertson"] {
        w.request(ann, Request::IgnoredAdd { user: user.into() });
    }
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: false,
    });
    w.frames();
    w.take(ann);
    assert!(w.service.local_ignores().is_some());

    let (_, freed) = w.ids("Robertson");
    w.store.purge_character(freed).expect("its name free");
    w.rename("Bob", "Robertson");

    let cx = w.request(ann, Request::GetFriendsList);
    let Answer::GetFriendsList { friends } = answer(&w.take(ann), cx).1 else {
        panic!("its list");
    };
    let names: Vec<&str> = friends.iter().map(|f| f.user.as_str()).collect();
    assert_eq!(names, ["Robertson@Dawnsong", "Bob@coldeve"]);
    let cx = w.request(ann, Request::GetIgnoredList);
    assert_eq!(
        answer(&w.take(ann), cx).1,
        Answer::GetIgnoredList {
            ignored: vec!["Robertson@Dawnsong".to_owned()]
        }
    );
    let ignoring = w.service.local_ignores().expect("changed");
    assert_eq!(
        ignoring[&ann_id],
        std::collections::HashSet::from(["robertson".to_owned()])
    );

    let mut again = Service::load(Config::new(NAME, SLUG), &mut w.store).expect("reads");
    assert_eq!(
        again.local_ignores().expect("all of it"),
        ignoring,
        "a restart"
    );

    let cx = w.request(ann, Request::FriendDelete { user: "Bob".into() });
    assert_eq!(
        answer(&w.take(ann), cx).0,
        NOT_IN_LIST,
        "the old name is off it"
    );
    let cx = w.request(
        ann,
        Request::FriendDelete {
            user: "Robertson".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK, "the new one comes off");
    let lists = w.store.network_lists().expect("reads");
    assert!(
        lists.iter().all(|l| l.entry != "Bob" && l.entry != "bob"),
        "{lists:?}"
    );
    assert!(
        lists
            .iter()
            .all(|l| l.entry != "Robertson" || l.list == yalain_store::NetworkList::Ignored),
        "{lists:?}"
    );
}

/// The client's add button sends the selected object's name, its suffix with it: the friend and the
/// ignored are kept by their own names, and come off by either.
/// Behaviour: server.friends.suffixed-name
#[test]
fn a_friend_or_a_squelch_by_a_name_with_its_suffix_keeps_the_character() {
    let mut w = World::new();
    let ann = w.player("Ann");
    w.player("Cara");
    w.service
        .set_suffixed([("Cara Bold".to_owned(), "Cara".to_owned())]);
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Cara Bold@Dawnsong".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    let cx = w.request(ann, Request::GetFriendsList);
    let got = w.take(ann);
    assert!(
        format!("{:?}", answer(&got, cx).1).contains("Cara@Dawnsong"),
        "{got:?}"
    );
    let cx = w.request(
        ann,
        Request::FriendDelete {
            user: "Cara Bold@Dawnsong".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    let cx = w.request(
        ann,
        Request::IgnoredAdd {
            user: "Cara Bold@Dawnsong".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    let cx = w.request(ann, Request::GetIgnoredList);
    let got = w.take(ann);
    let list = format!("{:?}", answer(&got, cx).1);
    assert!(
        list.contains("Cara@Dawnsong") && !list.contains("Bold"),
        "{list}"
    );
    let cx = w.request(
        ann,
        Request::IgnoredDelete {
            user: "Cara Bold".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    // Oneself, by one's own shown name, is still refused.
    w.service
        .set_suffixed([("Ann Bold".to_owned(), "Ann".to_owned())]);
    let cx = w.request(
        ann,
        Request::FriendAdd {
            user: "Ann Bold@Dawnsong".into(),
        },
    );
    assert_eq!(answer(&w.take(ann), cx).0, Hr::NO_SUCH_USER);
}
