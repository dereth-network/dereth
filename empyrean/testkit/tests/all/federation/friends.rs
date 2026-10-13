//! Divergence: V448
//! Friends on other worlds: a remote friend is a friends-panel row (`Social_FriendsUpdate`,
//! `0x0021`) under its stand-in id, its status is what its home world shows to mutual friends,
//! and each change is a status row and the online or offline line, with the name a link.
//! Fixture: two virtual-time worlds through the hub's routing (`super::Net`).

use dereth_network_protocol::standin::is_stand_in;
use dereth_network_protocol::{Address, Message};
use dereth_primitives::ObjectId;
use dereth_protocol::social::{SocialFriendsUpdate, SocialRemoveFriend};
use empyrean_entity::ObjectGuid;
use empyrean_world::federation;

use super::{add_friend, seed, two_worlds, Net, FRIENDS_UPDATE};

fn updates(net: &mut Net, slug: &str, who: &str, mark: usize) -> Vec<SocialFriendsUpdate> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == FRIENDS_UPDATE)
        .map(|g| g.decode::<SocialFriendsUpdate>())
        .collect()
}

/// Divergence: V448
#[test]
fn a_remote_friend_is_a_row_under_its_stand_in_id_shown_online_as_its_world_says() {
    let mut net = two_worlds(false);
    let ma = net.mark("aa", "Ann");
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert_eq!(ups[0].update_type, 1, "FriendAdded");
    let row = &ups[0].friends[0];
    assert_eq!(row.name, "Bob@bb");
    assert!(is_stand_in(row.id.0));
    assert_eq!(row.online, 0, "offline until its world says otherwise");
    let status = ups
        .iter()
        .find(|u| u.update_type == 4)
        .expect("a status row for Bob");
    assert_eq!(
        (status.friends[0].name.as_str(), status.friends[0].online),
        ("Bob@bb", 1),
        "Bob lists nobody, and shows online to anyone who lists him"
    );
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["Bob@bb has been added to your friends list."],
        "the first report is a state, not news"
    );

    // Bob goes off the network and comes back: each change is a line, the name a link.
    net.say("bb", "Bob", "@dn off");
    let ma = net.mark("aa", "Ann");
    net.say("bb", "Bob", "@dn on");
    let line = &Net::lines(&net.since("aa", "Ann", ma))[0];
    assert!(
        line.ends_with("Bob@bb<\\Tell> has come online."),
        "the online line, with the name a link: {line}"
    );
    assert!(line.starts_with("<Tell:IIDString:"), "{line}");
}

/// Divergence: V448
#[test]
fn dn_friends_add_and_remove_do_what_the_friends_panel_does() {
    let mut net = two_worlds(false);
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends add Bob@bb");
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert_eq!(ups[0].update_type, 1, "FriendAdded");
    let row = ups[0].friends[0].clone();
    assert_eq!(row.name, "Bob@bb");
    assert!(is_stand_in(row.id.0));
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma))[0],
        "Bob@bb has been added to your friends list."
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends remove bob@BB");
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert_eq!(ups[0].update_type, 2, "FriendRemoved");
    assert_eq!(
        (ups[0].friends[0].id, ups[0].friends[0].name.as_str()),
        (row.id, "Bob@bb"),
        "the id and the name the client holds"
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends remove Bob@bb");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["That character is not in your friends list!"]
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends add Bob");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["@dn friends add <Name@world> - add a friend on another world"]
    );
}

/// Divergence: V448
#[test]
fn a_friend_logging_off_and_on_again_is_seen_by_the_other_world() {
    let mut net = two_worlds(true);
    let ma = net.mark("aa", "Ann");
    let bob = net.world("bb").clients["Bob"];
    net.world("bb").ts.client_mut(bob).log_off();
    net.advance(12.0);
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert!(
        ups.iter()
            .any(|u| u.update_type == 4 && u.friends[0].online == 0),
        "Bob shows offline: {ups:?}"
    );
    assert!(Net::lines(&net.since("aa", "Ann", ma))
        .iter()
        .any(|l| l.ends_with("Bob@bb<\\Tell> has gone offline.")));
}

/// Divergence: V448
#[test]
fn a_character_on_the_network_shows_online_to_anyone_who_lists_it() {
    let mut net = two_worlds(false);
    let ma = net.mark("aa", "Ann");
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    net.advance(0.5);
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert!(
        ups.iter()
            .any(|u| u.update_type == 4 && u.friends[0].online == 1),
        "Bob does not list Ann, and shows online to her: {ups:?}"
    );
    let ma = net.mark("aa", "Ann");
    net.say("bb", "Bob", "@dn off");
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert!(
        ups.iter()
            .any(|u| u.update_type == 4 && u.friends[0].online == 0),
        "off the network, offline to everyone: {ups:?}"
    );
}

/// Divergence: V448
#[test]
fn removing_a_remote_friend_removes_its_row_by_its_stand_in_id() {
    let mut net = two_worlds(true);
    let id = net
        .world("aa")
        .ts
        .world
        .federation
        .store
        .stand_ins
        .get(&dereth_network_protocol::Address::parse("Bob@bb").expect("an address"))
        .expect("Bob has an id");
    let ma = net.mark("aa", "Ann");
    let ann = net.world("aa").clients["Ann"];
    net.world("aa").ts.send_game_action(
        ann,
        &SocialRemoveFriend {
            friend_id: ObjectId(id),
        },
    );
    net.advance(0.3);
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert_eq!(ups[0].update_type, 2, "FriendRemoved");
    assert_eq!(
        (ups[0].friends[0].id.0, ups[0].friends[0].name.as_str()),
        (id, "Bob@bb")
    );
    assert!(net
        .world("aa")
        .ts
        .world
        .federation
        .record(0x5000_0001)
        .expect("a record")
        .friends
        .is_empty());
}

/// Divergence: V448
#[test]
fn the_full_friends_list_at_login_carries_the_remote_friends() {
    let mut net = two_worlds(true);
    let ann = net.world("aa").clients["Ann"];
    net.world("aa").ts.client_mut(ann).log_off();
    net.advance(12.0);
    let again = seed("ann", 0x5000_0001, "Ann");
    let id = net.enter("aa", again);
    let full: Vec<SocialFriendsUpdate> = net
        .since("aa", "Ann", 0)
        .iter()
        .filter(|g| g.kind == FRIENDS_UPDATE)
        .map(|g| g.decode::<SocialFriendsUpdate>())
        .filter(|u| u.update_type == 0)
        .collect();
    let _ = id;
    assert_eq!(full.len(), 1, "one full list at login");
    assert_eq!(full[0].friends.len(), 1);
    assert_eq!(full[0].friends[0].name, "Bob@bb");
}

/// Divergence: V448
#[test]
fn a_friend_who_is_logged_off_or_has_never_logged_in_is_watched_and_the_world_runs_on() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let cid = seed("cid", 0x5000_0002, "Cid");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob, cid])]);
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    net.enter("bb", bob);
    net.say("aa", "Ann", "@dn on");
    net.say("bb", "Bob", "@dn on");
    add_friend(&mut net, "bb", "Bob", "Ann@aa");
    let b = net.world("bb").clients["Bob"];
    net.world("bb").ts.client_mut(b).log_off();
    net.advance(12.0);

    // A watch for Bob, logged off, and for Cid, who has not been in the world since it started;
    // then both again when Ann opts in anew.
    let mark = net.sent.0.len();
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    add_friend(&mut net, "aa", "Ann", "Cid@bb");
    net.say("aa", "Ann", "@dn off");
    net.say("aa", "Ann", "@dn on");
    let answers: Vec<(String, bool)> = net
        .sent_since("bb", mark)
        .into_iter()
        .filter_map(|m| match m {
            Message::Presence { of, online, .. } => Some((of.to_string(), online)),
            _ => None,
        })
        .collect();
    assert!(
        answers.iter().all(|(_, online)| !online)
            && answers.iter().any(|(of, _)| of == "Bob@bb")
            && answers.iter().any(|(of, _)| of == "Cid@bb"),
        "Bob, logged off, and Cid, never on the network, are answered alike: offline: \
         {answers:?}"
    );

    // bb is still running: Bob comes back and Ann sees him online.
    let ma = net.mark("aa", "Ann");
    net.enter("bb", bob);
    net.advance(0.5);
    let ups = updates(&mut net, "aa", "Ann", ma);
    assert!(
        ups.iter().any(|u| u.update_type == 4
            && u.friends[0].name == "Bob@bb"
            && u.friends[0].online == 1),
        "{ups:?}"
    );
}

/// Divergence: V448
#[test]
fn each_watcher_sees_only_what_the_home_world_shows_it_and_its_answer_to_one_is_not_anothers() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let cat = seed("cat", 0x5000_0002, "Cat");
    let bob = seed("bob", 0x5000_0001, "Bob");
    let mut net = Net::new(&[("aa", &[ann, cat]), ("bb", &[bob])]);
    net.link("aa");
    net.link("bb");
    net.enter("aa", ann);
    net.enter("aa", cat);
    net.enter("bb", bob);
    for (w, who) in [("aa", "Ann"), ("aa", "Cat"), ("bb", "Bob")] {
        net.say(w, who, "@dn on");
    }
    // Bob's world blocks Cat alone: Ann sees him online, Cat never does.
    let _ = federation::commands::admin(
        &mut net.world("bb").ts.world,
        &["block".to_owned(), "Cat@aa".to_owned()],
    );
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    let mc = net.mark("aa", "Cat");
    add_friend(&mut net, "aa", "Cat", "Bob@bb");
    net.advance(1.0);
    let ups = updates(&mut net, "aa", "Cat", mc);
    assert!(
        ups.iter().all(|u| u.friends.iter().all(|f| f.online == 0)),
        "Cat never sees Bob online: {ups:?}"
    );
    assert!(
        Net::lines(&net.since("aa", "Cat", mc))
            .iter()
            .all(|l| !l.contains("has gone offline") && !l.contains("has come online")),
        "nor a line about him"
    );
    let rows = federation::friend_rows(&mut net.world("aa").ts.world, ObjectGuid::new(0x5000_0001));
    assert_eq!(
        rows.iter().map(|r| (r.1.as_str(), r.2)).collect::<Vec<_>>(),
        [("Bob@bb", true)],
        "Bob's answer to Cat leaves Ann's alone"
    );
    let rows = federation::friend_rows(&mut net.world("aa").ts.world, ObjectGuid::new(0x5000_0002));
    assert_eq!(
        rows.iter().map(|r| (r.1.as_str(), r.2)).collect::<Vec<_>>(),
        [("Bob@bb", false)],
        "and Bob's answer to Ann leaves Cat's alone"
    );
}

/// Divergence: V448
#[test]
fn a_character_lists_at_most_the_clients_own_number_of_remote_friends() {
    let mut net = two_worlds(false);
    for i in 0..federation::MAX_REMOTE_FRIENDS {
        let i = u32::try_from(i).expect("a count");
        add_friend(&mut net, "aa", "Ann", &format!("{}@bb", minted(i)));
    }
    let ma = net.mark("aa", "Ann");
    add_friend(&mut net, "aa", "Ann", "OneTooMany@bb");
    let got = net.since("aa", "Ann", ma);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(
        got[0]
            .decode::<dereth_protocol::comms::CommunicationWeenieError>()
            .error_type,
        empyrean_entity::enums::WeenieError::MaxFriendsExceeded
            .0
            .cast_unsigned(),
        "the error the client gives for a full list"
    );
    assert_eq!(
        net.world("aa")
            .ts
            .world
            .federation
            .record(0x5000_0001)
            .expect("a record")
            .friends
            .len(),
        federation::MAX_REMOTE_FRIENDS
    );
}

/// Divergence: V448
#[test]
fn one_character_with_a_crowd_of_watchers_cannot_push_out_anothers_presence() {
    let bob = seed("bob", 0x5000_0001, "Bob");
    let bot = seed("bot", 0x5000_0002, "Bot");
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob, bot])]);
    net.link("aa");
    net.link("bb");
    net.enter("bb", bob);
    net.enter("bb", bot);
    for who in ["Bob", "Bot"] {
        net.say("bb", who, "@dn on");
    }
    let mark = net.sent.0.len();
    // Ann's watch of Bob is answered first; then a crowd watches Bot, more answers than the lane
    // holds, all before bb sends anything.
    let w = &mut net.world("bb").ts.world;
    let watch = |owner: &str, target: &str| {
        Message::Watch {
            owner: address(owner),
            target: address(target),
            on: true,
        }
        .to_json()
    };
    federation::on_link_frame(w, &watch("Ann@aa", "Bob@bb"));
    for i in 0..u32::try_from(federation::MAX_CHATTER + 500).expect("a count") {
        federation::on_link_frame(w, &watch(&format!("{}@aa", minted(i)), "Bot@bb"));
    }
    net.advance(2.0);
    let to_ann = net
        .sent_since("bb", mark)
        .into_iter()
        .filter(|m| matches!(m, Message::Presence { to, of, .. } if to.to_string() == "Ann@aa" && of.to_string() == "Bob@bb"))
        .count();
    assert_eq!(
        to_ann, 1,
        "Bob's answer to Ann is not pushed out by Bot's crowd"
    );
}

/// Divergence: V448
#[test]
fn appearing_offline_shows_a_character_offline_to_its_friends_on_other_worlds() {
    let mut net = two_worlds(true);
    let mb = net.mark("bb", "Bob");
    let ann = net.world("aa").clients["Ann"];
    let appear_offline = |value: u32| dereth_protocol::login::CharacterPlayerOptionChangedEvent {
        option: empyrean_entity::enums::CharacterOption::AppearOffline
            .0
            .cast_unsigned(),
        value,
    };
    net.world("aa").ts.send_game_action(ann, &appear_offline(1));
    net.advance(0.5);
    let ups = updates(&mut net, "bb", "Bob", mb);
    assert!(
        ups.iter().any(|u| u.update_type == 4
            && u.friends[0].name == "Ann@aa"
            && u.friends[0].online == 0),
        "Ann shows offline to Bob: {ups:?}"
    );
    let mb = net.mark("bb", "Bob");
    net.world("aa").ts.send_game_action(ann, &appear_offline(0));
    net.advance(0.5);
    let ups = updates(&mut net, "bb", "Bob", mb);
    assert!(
        ups.iter().any(|u| u.update_type == 4
            && u.friends[0].name == "Ann@aa"
            && u.friends[0].online == 1),
        "and online again: {ups:?}"
    );
}

/// Divergence: V448
#[test]
fn a_watch_is_answered_alike_for_a_character_off_the_network_a_blocked_watcher_and_a_name_the_world_lacks(
) {
    let bob = seed("bob", 0x5000_0001, "Bob");
    let cid = seed("cid", 0x5000_0002, "Cid");
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann]), ("bb", &[bob, cid])]);
    net.link("aa");
    net.link("bb");
    net.enter("bb", bob);
    net.enter("bb", cid);
    net.say("bb", "Bob", "@dn on");
    let _ = federation::commands::admin(
        &mut net.world("bb").ts.world,
        &["block".to_owned(), "Mal@aa".to_owned()],
    );
    let mark = net.sent.0.len();
    // Cid is in the world but never joined the network; Nobody does not exist; Mal is blocked.
    // Spelled as a watcher may spell them. Bob, on the network, shows online to Eve, whom he does
    // not list.
    let watches = [
        ("Eve@aa", "cID@bb"),
        ("Eve@aa", "Nobody@bb"),
        ("Eve@aa", "bOB@bb"),
        ("Mal@aa", "bOB@bb"),
    ];
    for (owner, target) in watches {
        net.world_sends(
            "aa",
            Message::Watch {
                owner: address(owner),
                target: address(target),
                on: true,
            },
        );
    }
    let answered: Vec<(String, String, bool)> = net
        .sent_since("bb", mark)
        .into_iter()
        .filter_map(|m| match m {
            Message::Presence { to, of, online } => Some((to.to_string(), of.to_string(), online)),
            _ => None,
        })
        .collect();
    assert_eq!(
        answered,
        watches
            .iter()
            .map(|(o, t)| (
                (*o).to_owned(),
                (*t).to_owned(),
                *o == "Eve@aa" && *t == "bOB@bb"
            ))
            .collect::<Vec<_>>(),
        "the three are answered offline alike, named as the watch named them"
    );
}

/// Divergence: V448
#[test]
fn clearing_the_friends_list_stops_watching_and_changes_nothing_a_former_friend_sees() {
    let mut net = two_worlds(true);
    let mb = net.mark("bb", "Bob");
    let mark = net.sent.0.len();
    let ann = net.world("aa").clients["Ann"];
    net.world("aa")
        .ts
        .send_game_action(ann, &dereth_protocol::social::SocialClearFriends);
    net.advance(0.3);
    assert!(
        net.sent_since("aa", mark).iter().any(|m| matches!(m,
            Message::Watch { owner, target, on: false }
                if owner.to_string() == "Ann@aa" && target.to_string() == "Bob@bb")),
        "Ann stops watching Bob"
    );
    let ups = updates(&mut net, "bb", "Bob", mb);
    assert!(
        !ups.iter().any(|u| u.update_type == 4
            && u.friends[0].name == "Ann@aa"
            && u.friends[0].online == 0),
        "Ann shows online to anyone who lists her, Bob still among them: {ups:?}"
    );
}

/// Divergence: V448
#[test]
fn another_world_cannot_make_this_one_keep_watchers_or_presence_without_end() {
    let mut net = two_worlds(false);
    for i in 0..500u32 {
        // Within aa's share of bb's pace for watches.
        if i % 10 == 9 {
            net.advance(1.0);
        }
        net.hub_hands(
            "bb",
            &Message::Watch {
                owner: address(&format!("{}@aa", minted(i))),
                target: address("Bob@bb"),
                on: true,
            },
        );
        net.hub_hands(
            "aa",
            &Message::Presence {
                to: address("Ann@aa"),
                of: address(&format!("{}@bb", minted(i))),
                online: true,
            },
        );
    }
    assert_eq!(
        net.world("bb")
            .ts
            .world
            .federation
            .watcher_count(0x5000_0001),
        federation::MAX_WATCHERS_PER_WORLD,
        "Bob keeps at most one world's share of strangers' watches"
    );
    assert_eq!(
        net.world("aa").ts.world.federation.presence_count(),
        0,
        "presence for characters nobody on aa lists is not kept"
    );
}

/// Divergence: V448
#[test]
fn a_flood_of_presence_answers_does_not_hold_up_a_tell() {
    let mut net = two_worlds(false);
    // Every watch is answered, kept or not: 2,000 answers for bb to send.
    for i in 0..2_000u32 {
        let w = net.world("bb");
        federation::on_link_frame(
            &mut w.ts.world,
            &Message::Watch {
                owner: address(&format!("{}@aa", minted(i))),
                target: address("Bob@bb"),
                on: true,
            }
            .to_json(),
        );
    }
    let ma = net.mark("aa", "Ann");
    net.tell("bb", "Bob", "Ann@aa", "first in line");
    net.advance(1.0);
    let got = net
        .since("aa", "Ann", ma)
        .iter()
        .filter(|g| g.kind == super::TELL)
        .count();
    assert_eq!(got, 1, "the tell goes ahead of the presence answers");
}

fn address(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

/// A name of letters only, different for each `i`.
pub(crate) fn minted(i: u32) -> String {
    let mut n = String::from("Mint");
    let mut i = i;
    loop {
        n.push(char::from(b'a' + u8::try_from(i % 26).expect("a letter")));
        i /= 26;
        if i == 0 {
            break n;
        }
    }
}

/// Divergence: V448
#[test]
fn a_friends_world_flipping_its_answer_is_shown_at_a_pace_and_then_its_latest_state() {
    let mut net = two_worlds(true);
    let ma = net.mark("aa", "Ann");
    // A hub that hands on whatever bb writes: Bob's status flips 400 times in eight seconds,
    // ending online.
    for i in 0..400u32 {
        net.hub_hands(
            "aa",
            &Message::Presence {
                to: address("Ann@aa"),
                of: address("Bob@bb"),
                online: i % 2 == 1,
            },
        );
        if i % 50 == 49 {
            net.advance(1.0);
        }
    }
    net.advance(10.0);
    let got = net.since("aa", "Ann", ma);
    let rows: Vec<SocialFriendsUpdate> = got
        .iter()
        .filter(|g| g.kind == FRIENDS_UPDATE)
        .map(|g| g.decode::<SocialFriendsUpdate>())
        .collect();
    let lines = Net::lines(&got).len();
    // The pace's burst, then one every five seconds, over some eighteen seconds.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let most =
        (federation::PRESENCE_PACE.burst + 19.0 * federation::PRESENCE_PACE.per_sec) as usize;
    assert!(
        !rows.is_empty() && rows.len() <= most,
        "{} status rows (at most {most})",
        rows.len()
    );
    assert!(
        lines <= most,
        "{lines} online and offline lines (at most {most})"
    );
    assert_eq!(
        rows.last().map(|u| u.friends[0].online),
        Some(1),
        "the last row is the latest state"
    );
    assert_eq!(
        net.world("aa")
            .ts
            .world
            .federation
            .shown_status(0x5000_0001, "bob@bb"),
        Some(true)
    );
}

/// Divergence: V448
#[test]
fn a_world_sending_watches_past_its_share_is_answered_only_its_share() {
    let mut net = two_worlds(false);
    let mark = net.sent.0.len();
    // A hub that hands on whatever aa writes: 300 watches at once.
    for i in 0..300u32 {
        net.hub_hands(
            "bb",
            &Message::Watch {
                owner: address(&format!("{}@aa", minted(i))),
                target: address("Bob@bb"),
                on: true,
            },
        );
    }
    // Long enough for every answer taken to have gone, at aa's share of bb's pace.
    net.advance(10.0);
    let answered = net
        .sent_since("bb", mark)
        .iter()
        .filter(|m| matches!(m, Message::Presence { .. }))
        .count();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let share = dereth_network_protocol::limits::Limits::DEFAULT
        .watches_per_world_pair()
        .burst as usize;
    assert_eq!(answered, share, "aa's share of bb's pace, no more");
}

/// Divergence: V448
#[test]
fn watch_floods_from_many_worlds_do_not_hold_up_a_real_friends_presence() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let cid = seed("cid", 0x5000_0001, "Cid");
    let flooders = ["ba", "bb", "bc", "bd", "be"];
    let (on_aa, on_cc) = ([ann], [cid]);
    let mut worlds: Vec<(&'static str, &[super::Seed])> = vec![("aa", &on_aa), ("cc", &on_cc)];
    for f in flooders {
        worlds.push((f, &[]));
    }
    let mut net = Net::new(&worlds);
    for slug in ["aa", "cc"].iter().chain(flooders.iter()) {
        net.link(slug);
    }
    net.enter("aa", ann);
    net.enter("cc", cid);
    net.say("aa", "Ann", "@dn on");
    net.say("cc", "Cid", "@dn on");
    add_friend(&mut net, "aa", "Ann", "Cid@cc");
    add_friend(&mut net, "cc", "Cid", "Ann@aa");
    let c = net.world("cc").clients["Cid"];
    net.world("cc").ts.client_mut(c).log_off();
    net.advance(12.0);
    // Five worlds, each within its share of aa's pace for watches, together past what aa can
    // answer: a minute of that, then Cid logs in.
    let mut n = 0u32;
    let mut flood = |net: &mut Net| {
        for f in flooders {
            for _ in 0..3 {
                n += 1;
                net.world_sends(
                    f,
                    Message::Watch {
                        owner: address(&format!("{}@{f}", minted(n))),
                        target: address("Ann@aa"),
                        on: true,
                    },
                );
            }
        }
        net.advance(0.25);
    };
    for _ in 0..240 {
        flood(&mut net);
    }
    assert!(
        net.world("aa").ts.world.federation.queued() > 100,
        "aa has more to answer than it can send"
    );
    let cid = seed("cid", 0x5000_0001, "Cid");
    net.enter("cc", cid);
    let mc = net.mark("cc", "Cid");
    let mut seen_after = None;
    for step in 0..40u32 {
        flood(&mut net);
        let online = updates(&mut net, "cc", "Cid", mc)
            .iter()
            .any(|u| u.update_type == 4 && u.friends[0].online == 1);
        if online {
            seen_after = Some(f64::from(step) * 0.25);
            break;
        }
    }
    assert!(
        seen_after.is_some_and(|s| s <= 2.0),
        "Cid sees Ann online within two seconds: {seen_after:?}"
    );
}

/// Divergence: V448
#[test]
fn invented_friends_added_and_removed_leave_no_stand_in_id_once_unused_for_a_day() {
    let mut net = two_worlds(true);
    let kept = net.world("aa").ts.world.federation.store.stand_ins.len();
    let ann = net.world("aa").clients["Ann"];
    for i in 0..300u32 {
        let name = format!("{}@zz", minted(i));
        let w = net.world("aa");
        w.ts.send_game_action(
            ann,
            &dereth_protocol::social::SocialAddFriend { name: name.clone() },
        );
        w.ts.advance(0.05);
        let id =
            w.ts.world
                .federation
                .store
                .stand_ins
                .get(&address(&name))
                .expect("an id while listed");
        w.ts.send_game_action(
            ann,
            &SocialRemoveFriend {
                friend_id: ObjectId(id),
            },
        );
        w.ts.advance(0.05);
    }
    net.advance(61.0);
    assert_eq!(
        net.world("aa").ts.world.federation.store.stand_ins.len(),
        kept + 300,
        "kept while lately used"
    );
    // A day on, nothing names them and nobody has used them.
    net.world("aa")
        .ts
        .clock
        .advance(std::time::Duration::from_secs_f64(
            federation::STAND_IN_IDLE_SECS,
        ));
    net.advance(61.0);
    let f = &net.world("aa").ts.world.federation;
    assert_eq!(f.store.stand_ins.len(), kept, "forgotten");
    assert!(
        f.store.stand_ins.get(&address("Bob@bb")).is_some(),
        "a friend's id is kept however long unused"
    );
}

/// Divergence: V448
#[test]
fn the_stand_in_table_keeps_at_most_its_cap_of_ids_nothing_names() {
    let mut net = two_worlds(true);
    {
        let table =
            std::sync::Arc::make_mut(&mut net.world("aa").ts.world.federation.store.stand_ins);
        for i in 0..(u32::try_from(federation::MAX_IDLE_STAND_INS).expect("small") + 500) {
            table
                .id_for(&address(&format!("{}@zz", minted(i))))
                .expect("an id");
        }
    }
    net.advance(61.0);
    let f = &net.world("aa").ts.world.federation;
    assert_eq!(
        f.store.stand_ins.len(),
        federation::MAX_IDLE_STAND_INS + 1,
        "the cap, and Bob, whom Ann lists"
    );
    assert!(f.store.stand_ins.get(&address("Bob@bb")).is_some());
}
