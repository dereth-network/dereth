//! A stream, an account, a character: the ticket, the binding, the bounds.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::*;

/// A stream is bound to the account its ticket names, as the host's check admits it. Until the
/// account's character is in the world every request is answered not connected; when it enters,
/// the stream is sent its friends; when it leaves, nothing is sent, and the next to enter gets its
/// own.
/// Behaviour: server.friends.stream-bound-to-account-and-character
#[test]
fn a_ticketed_stream_acts_for_its_accounts_character_while_it_is_in_the_world() {
    let mut w = World::new();
    w.character("Ann");
    w.character("Cara");
    let s = w.connect();
    let cx = w.present(s, "acct-Ann");
    assert!(
        w.take(s).iter().all(|e| matches!(e, Element::Welcome(_))),
        "the host checks the ticket first"
    );
    w.check_tickets();
    assert_eq!(answer(&w.take(s), cx).0, Hr::OK);
    let cx = w.request(s, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(s), cx).0,
        Hr::NOT_CONNECTED,
        "no character yet"
    );
    w.enter("Ann");
    assert_eq!(
        events(&w.take(s)),
        vec![Event::FStart {
            friends: Vec::new()
        }]
    );
    assert!(w.service.is_present("ann"));
    let cx = w.request(s, Request::FriendAdd { user: here("Cara") });
    assert_eq!(answer(&w.take(s), cx).0, Hr::OK);
    w.leave("Ann");
    assert!(w.take(s).is_empty(), "leaving sends nothing");
    assert!(!w.service.is_present("Ann"));
    let cx = w.request(s, Request::GetFriendsList);
    assert_eq!(answer(&w.take(s), cx).0, Hr::NOT_CONNECTED);
    w.enter("Ann");
    assert_eq!(
        events(&w.take(s)),
        vec![Event::FStart {
            friends: vec![status(&here("Cara"), Mode::OFFLINE)]
        }]
    );
}

/// A stream that comes back while its character is in the world is sent the friends list again
/// at its ticket; a character in the world without a stream is not present.
/// Behaviour: server.friends.stream-bound-to-account-and-character
#[test]
fn a_stream_that_comes_back_while_its_character_is_in_the_world_is_sent_its_friends() {
    let mut w = World::new();
    w.character("Ann");
    w.enter("Ann");
    assert!(
        !w.service.is_present("Ann"),
        "in the world without a stream"
    );
    let s = w.connect();
    w.present(s, "acct-Ann");
    w.check_tickets();
    let got = w.take(s);
    assert!(
        events(&got).contains(&Event::FStart {
            friends: Vec::new()
        }),
        "{got:?}"
    );
    assert!(w.service.is_present("Ann"));
}

/// A second stream for the same account replaces the first: the first is told so and closed,
/// and the character stays present, its friends told nothing, its new stream sent its friends.
/// Behaviour: server.friends.second-stream-replaces-first
#[test]
fn a_second_stream_for_an_account_replaces_the_first_and_the_character_stays_present() {
    let mut w = World::new();
    let cara = w.player("Cara");
    let ann = w.player("Ann");
    w.request(cara, Request::FriendAdd { user: here("Ann") });
    w.take(cara);
    let again = w.connect();
    w.present(again, "acct-Ann");
    w.check_tickets();
    let first = w.take(ann);
    assert!(bye(&first, ByeReason::Replaced), "{first:?}");
    assert!(w.closed.contains(&ann));
    assert!(events(&w.take(again)).contains(&Event::FStart {
        friends: Vec::new()
    }));
    assert!(w.service.is_present("Ann"));
    assert!(
        events(&w.take(cara)).is_empty(),
        "Ann's friends see no change"
    );
    // The first stream's end changes nothing either.
    w.drop_stream(ann);
    assert!(w.service.is_present("Ann"));
    assert!(events(&w.take(cara)).is_empty());
}

/// A ticket the login's rule refuses is answered with the refused code, and the stream may try
/// again; one that is not of the ticket's form is refused without asking the host. Five refusals
/// from one address in a minute and its next minute's tickets are refused unchecked.
/// Behaviour: server.friends.ticket-refused-and-guessing-held-back
#[test]
fn refused_tickets_are_answered_so_and_five_in_a_minute_lock_the_address_for_a_minute() {
    let mut w = World::new();
    w.character("Ann");
    let s = w.connect();
    let cx = w.present(s, "acct-Nobody");
    w.check_tickets();
    assert_eq!(answer(&w.take(s), cx).0, Hr::TICKET_REFUSED);
    let cx = w.request(
        s,
        Request::PresentTicket {
            email: String::new(),
            ticket: "not a ticket".into(),
        },
    );
    assert!(w.service.take_tickets().is_empty(), "never handed over");
    assert_eq!(answer(&w.take(s), cx).0, Hr::TICKET_REFUSED);
    for _ in 0..3 {
        w.present(s, "acct-Nobody");
        w.check_tickets();
    }
    w.take(s);
    // Five refused: the address's good ticket is refused unchecked for a minute.
    let cx = w.present(s, "acct-Ann");
    assert!(w.service.take_tickets().is_empty(), "refused unchecked");
    assert_eq!(answer(&w.take(s), cx).0, Hr::TICKET_REFUSED);
    let other = w.connect_from(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)));
    let cx = w.present(other, "acct-Ann");
    w.check_tickets();
    assert_eq!(
        answer(&w.take(other), cx).0,
        Hr::OK,
        "another address is checked"
    );
    w.now += LOCKOUT_SECS;
    // Keep the stream alive past the minute: it is heard from.
    let s = w.connect();
    let cx = w.present(s, "acct-Ann");
    w.check_tickets();
    assert_eq!(answer(&w.take(s), cx).0, Hr::OK, "a minute later");
}

const LOCKOUT_SECS: std::time::Duration = yalain_friends::LOCKOUT;

/// At most eight streams from one address, an IPv6 address counted by its /64; the ninth is told
/// the world is full and closed.
/// Behaviour: server.friends.streams-bounded
#[test]
fn an_address_holds_at_most_eight_streams_an_ipv6_one_by_its_prefix() {
    let mut w = World::new();
    let v6 = |last: u16| IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 2, 0, 0, 0, last));
    for i in 0..8 {
        let s = w.connect_from(v6(i));
        assert!(!w.closed.contains(&s));
    }
    let ninth = w.connect_from(v6(99));
    assert!(bye(&w.take(ninth), ByeReason::Full));
    assert!(w.closed.contains(&ninth));
    let elsewhere = w.connect_from(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 3, 0, 0, 0, 1)));
    assert!(!w.closed.contains(&elsewhere), "another /64 is not counted");
}

/// A character whose friends are Cara (present) and Bob (not), in a world where Cara lists it
/// too: its stream, Cara's, and the friends list it should be sent.
fn ann_with_friends(w: &mut World) -> (StreamId, StreamId, Vec<FriendStatus>) {
    let cara = w.player("Cara");
    w.character("Bob");
    let ann = w.player("Ann");
    for name in ["Cara", "Bob"] {
        let cx = w.request(ann, Request::FriendAdd { user: here(name) });
        assert_eq!(answer(&w.take(ann), cx).0, Hr::OK);
    }
    let cx = w.request(cara, Request::FriendAdd { user: here("Ann") });
    assert_eq!(answer(&w.take(cara), cx).0, Hr::OK);
    let full = vec![
        status(&here("Cara"), Mode::ONLINE),
        status(&here("Bob"), Mode::OFFLINE),
    ];
    (ann, cara, full)
}

/// A stream that drops and comes back while its character is in the world is sent the whole
/// friends list again at its ticket, each friend with its state, and answers the friends list
/// asked for after; while it was down the character was not present.
/// Behaviour: server.friends.stream-back-sent-its-friends
#[test]
fn a_stream_that_drops_and_comes_back_is_sent_the_whole_friends_list_again() {
    let mut w = World::new();
    let (ann, cara, full) = ann_with_friends(&mut w);
    w.drop_stream(ann);
    assert!(!w.service.is_present("Ann"), "no stream, not present");
    assert_eq!(
        events(&w.take(cara)),
        vec![stat(&here("Ann"), Mode::OFFLINE)]
    );
    let again = w.connect();
    w.present(again, "acct-Ann");
    w.check_tickets();
    assert_eq!(
        events(&w.take(again)),
        vec![Event::FStart {
            friends: full.clone()
        }]
    );
    assert!(w.service.is_present("Ann"));
    assert_eq!(
        events(&w.take(cara)),
        vec![stat(&here("Ann"), Mode::ONLINE)]
    );
    let cx = w.request(again, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(again), cx),
        (Hr::OK, Answer::GetFriendsList { friends: full })
    );
}

/// A stream that takes the place of one the world has not yet seen end is sent the whole
/// friends list at its ticket and answers the friends list asked for after; the character never
/// stops being present, so neither its friends in the world nor another world's watch see it go
/// and come back, even when the old connection ends after.
/// Behaviour: server.friends.stream-back-sent-its-friends
#[test]
fn a_stream_in_place_of_one_still_open_is_sent_the_whole_list_and_nobody_sees_a_flicker() {
    let mut w = World::linked();
    let (ann, cara, full) = ann_with_friends(&mut w);
    w.on("Ann");
    w.hub(&Message::Watch {
        owner: addr("Bob@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    w.frames();
    let again = w.connect();
    w.present(again, "acct-Ann");
    w.check_tickets();
    assert!(bye(&w.take(ann), ByeReason::Replaced));
    assert_eq!(
        events(&w.take(again)),
        vec![Event::FStart {
            friends: full.clone()
        }]
    );
    let cx = w.request(again, Request::GetFriendsList);
    assert_eq!(
        answer(&w.take(again), cx),
        (Hr::OK, Answer::GetFriendsList { friends: full })
    );
    w.drop_stream(ann);
    assert!(w.service.is_present("Ann"));
    assert!(events(&w.take(cara)).is_empty(), "Cara sees no change");
    let frames = w.frames();
    assert!(
        !frames.iter().any(|m| matches!(m, Message::Presence { .. })),
        "another world sees no change: {frames:?}"
    );
}

/// The service tells its host which accounts have an admitted stream whenever that changes, so
/// the world can say at entry that the friends list is out of reach only to those without one.
/// Behaviour: server.friends.entry-line-only-without-a-stream
#[test]
fn the_host_is_told_the_accounts_with_streams_as_they_come_and_go() {
    let mut w = World::new();
    assert_eq!(w.service.accounts_with_streams(), Some(Default::default()));
    let (ann, _) = w.character("Ann");
    let s = w.connect();
    w.present(s, "acct-Ann");
    assert_eq!(
        w.service.accounts_with_streams(),
        None,
        "not until the ticket is admitted"
    );
    w.check_tickets();
    assert_eq!(
        w.service.accounts_with_streams(),
        Some([ann].into_iter().collect())
    );
    assert_eq!(w.service.accounts_with_streams(), None, "no change since");
    let again = w.stream("Ann");
    assert_eq!(
        w.service.accounts_with_streams(),
        Some([ann].into_iter().collect()),
        "a replacing stream keeps the account"
    );
    w.drop_stream(s);
    assert_eq!(
        w.service.accounts_with_streams(),
        None,
        "the replaced one ends"
    );
    w.drop_stream(again);
    assert_eq!(w.service.accounts_with_streams(), Some(Default::default()));
}
