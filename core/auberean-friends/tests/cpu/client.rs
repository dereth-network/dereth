//! The client's half of the stream alone, against a world written by hand, on a clock the tests
//! turn: its greeting, keep-alive, the waits between dials, its answers to every request, its
//! farewells.

use core::time::Duration;

use auberean_friends::{
    Answer, ByeReason, ClientOut, ClientSession, Cx, Element, Ended, Event, Hello, Hr, Request,
    Welcome, PREAMBLE,
};

use crate::common::{client, elements, frame, secs, split_client};

fn welcome() -> Welcome {
    Welcome {
        proto: 1,
        world: "dawnsong".into(),
        name: "Dawnsong".into(),
        ping: 20,
    }
}

/// A client whose stream opened at `t` and was welcomed at once.
fn up_at(t: Duration) -> ClientSession {
    let mut c = client();
    c.connect(t);
    assert_eq!(c.poll(t), [ClientOut::Dial]);
    c.opened(t);
    let _hello = c.poll(t);
    c.on_bytes(t, &frame(&Element::Welcome(welcome())));
    assert_eq!(c.poll(t), [ClientOut::Up(welcome())]);
    c
}

/// The waits a client was given each time its stream ended, in order.
fn waits(outs: &[ClientOut]) -> Vec<Option<Duration>> {
    outs.iter()
        .filter_map(|o| match o {
            ClientOut::Down { retry_in, .. } => Some(*retry_in),
            _ => None,
        })
        .collect()
}

/// Behaviour: friends.client.greeting
#[test]
fn the_client_opens_with_the_preamble_and_its_greeting_and_is_up_at_welcome() {
    let mut c = client();
    let t = secs(5);
    assert!(c.poll(t).is_empty(), "nothing until asked");
    c.connect(t);
    assert_eq!(c.poll(t), [ClientOut::Dial]);
    c.opened(t);
    let outs = c.poll(t);
    let [ClientOut::Write(bytes)] = outs.as_slice() else {
        panic!("one write: {outs:?}");
    };
    assert_eq!(&bytes[..4], &PREAMBLE);
    assert_eq!(
        elements(bytes),
        [Element::Hello(Hello {
            proto: 1,
            software: "auberean-friends-tests/1".into(),
            locale: Some(1033),
        })]
    );
    assert!(!c.is_up());
    c.request(t, Cx(1), Request::GetFriendsList);
    assert_eq!(
        c.poll(t),
        [ClientOut::Answer {
            cx: Cx(1),
            hr: Hr::NOT_CONNECTED,
            answer: Answer::GetFriendsList { friends: vec![] },
        }],
        "a request before the greeting is answered by the session"
    );
    c.on_bytes(t, &frame(&Element::Welcome(welcome())));
    assert_eq!(c.poll(t), [ClientOut::Up(welcome())]);
    assert!(c.is_up());
    assert_eq!(c.welcome(), Some(&welcome()));
    c.request(t, Cx(2), Request::GetFriendsList);
    let (written, rest) = split_client(c.poll(t));
    assert_eq!(
        written,
        [Element::Request {
            cx: Cx(2),
            request: Request::GetFriendsList
        }]
    );
    assert!(rest.is_empty());
}

/// Behaviour: friends.client.greeting
#[test]
fn a_world_that_does_not_welcome_the_client_within_ten_seconds_is_left() {
    let mut c = client();
    c.connect(Duration::ZERO);
    let _dial = c.poll(Duration::ZERO);
    c.opened(secs(1));
    let _hello = c.poll(secs(1));
    assert_eq!(c.next_wake(), Some(secs(11)));
    assert!(c.poll(Duration::from_millis(10_999)).is_empty());
    let (written, rest) = split_client(c.poll(secs(11)));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Idle
        }]
    );
    assert_eq!(
        rest,
        [
            ClientOut::Close,
            ClientOut::Down {
                ended: Ended::OwnBye(ByeReason::Idle),
                retry_in: Some(secs(1)),
            }
        ]
    );
}

/// Behaviour: friends.stream.keep-alive
#[test]
fn the_client_pings_after_the_worlds_interval_of_quiet_and_answers_each_ping() {
    let mut c = up_at(Duration::ZERO);
    assert_eq!(c.next_wake(), Some(secs(20)));
    assert!(c.poll(secs(19)).is_empty());
    let (written, _) = split_client(c.poll(secs(20)));
    assert_eq!(written, [Element::Ping { seq: 1 }]);
    assert_eq!(c.next_wake(), Some(secs(40)));
    c.on_bytes(secs(25), &frame(&Element::Ping { seq: 9 }));
    let (written, _) = split_client(c.poll(secs(25)));
    assert_eq!(written, [Element::Pong { seq: 9 }]);
    assert!(c.poll(secs(44)).is_empty(), "answering counted as sending");
    let (written, _) = split_client(c.poll(secs(45)));
    assert_eq!(written, [Element::Ping { seq: 2 }]);
    c.on_bytes(secs(46), &frame(&Element::Pong { seq: 2 }));
    assert!(c.poll(secs(46)).is_empty());
}

/// Behaviour: friends.stream.silence
#[test]
fn the_client_gives_up_a_stream_it_has_heard_nothing_on_for_seventy_seconds() {
    let mut c = up_at(Duration::ZERO);
    let (written, rest) = split_client(c.poll(secs(69)));
    assert_eq!(
        written,
        [Element::Ping { seq: 1 }],
        "still open, and pinging"
    );
    assert!(rest.is_empty());
    assert_eq!(c.next_wake(), Some(secs(70)));
    let (written, rest) = split_client(c.poll(secs(70)));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Idle
        }]
    );
    assert_eq!(
        rest,
        [
            ClientOut::Close,
            ClientOut::Down {
                ended: Ended::OwnBye(ByeReason::Idle),
                retry_in: Some(secs(1)),
            }
        ]
    );
}

/// Behaviour: friends.client.reconnect-waits
#[test]
fn the_client_waits_one_two_five_ten_then_thirty_seconds_between_dials() {
    let mut c = client();
    let mut t = Duration::ZERO;
    c.connect(t);
    let mut seen = Vec::new();
    for _ in 0..7 {
        assert_eq!(c.poll(t), [ClientOut::Dial], "at {t:?}");
        c.failed(t);
        let outs = c.poll(t);
        let wait = waits(&outs)[0].expect("it dials again");
        seen.push(wait.as_secs());
        assert_eq!(c.next_wake(), Some(t + wait));
        assert!(
            c.poll(t + wait - Duration::from_millis(1)).is_empty(),
            "not before the wait"
        );
        c.connect(t + wait - Duration::from_millis(1));
        assert!(
            c.poll(t).is_empty(),
            "asking again does not cut the wait short"
        );
        t += wait;
    }
    assert_eq!(seen, [1, 2, 5, 10, 30, 30, 30]);
}

/// Behaviour: friends.client.reconnect-waits-start-again
#[test]
fn a_stream_that_stood_a_minute_starts_the_waits_again_from_one_second() {
    let mut c = client();
    c.connect(secs(0));
    let _dial = c.poll(secs(0));
    c.failed(secs(0)); // wait 1
    assert_eq!(c.poll(secs(1)).last(), Some(&ClientOut::Dial));
    c.failed(secs(1)); // wait 2
    let _down = c.poll(secs(1));
    assert_eq!(c.poll(secs(3)), [ClientOut::Dial]);
    c.opened(secs(3));
    c.closed(secs(62)); // stood 59 seconds: the waits go on
    assert_eq!(waits(&c.poll(secs(62))), [Some(secs(5))]);
    assert_eq!(c.poll(secs(67)), [ClientOut::Dial]);
    c.opened(secs(67));
    c.closed(secs(127)); // stood 60 seconds: they start again
    assert_eq!(waits(&c.poll(secs(127))), [Some(secs(1))]);
    assert_eq!(c.poll(secs(128)), [ClientOut::Dial]);
    c.failed(secs(128));
    assert_eq!(waits(&c.poll(secs(128))), [Some(secs(2))]);
}

/// Behaviour: friends.client.replaced-waits-longest
#[test]
fn after_being_replaced_the_client_waits_the_longest_step() {
    let mut c = up_at(Duration::ZERO);
    c.on_bytes(
        secs(5),
        &frame(&Element::Bye {
            reason: ByeReason::Replaced,
        }),
    );
    let outs = c.poll(secs(5));
    assert_eq!(
        outs,
        [
            ClientOut::Close,
            ClientOut::Down {
                ended: Ended::PeerBye(ByeReason::Replaced),
                retry_in: Some(secs(30)),
            }
        ]
    );
    assert!(c.poll(secs(34)).is_empty());
    assert_eq!(c.poll(secs(35)), [ClientOut::Dial]);
    c.opened(secs(35));
    c.on_bytes(
        secs(36),
        &frame(&Element::Bye {
            reason: ByeReason::Replaced,
        }),
    );
    assert_eq!(
        waits(&c.poll(secs(36))),
        [Some(secs(30))],
        "each time it is replaced"
    );
}

/// Behaviour: friends.client.every-request-answered
#[test]
fn every_request_gets_exactly_one_answer_even_across_a_lost_stream() {
    let mut c = client();
    let mut answered: Vec<(Cx, Hr)> = Vec::new();
    let mut note = |outs: Vec<ClientOut>| {
        for o in outs {
            if let ClientOut::Answer { cx, hr, .. } = o {
                answered.push((cx, hr));
            }
        }
    };
    c.request(secs(0), Cx(1), Request::GetIgnoredList);
    note(c.poll(secs(0)));
    let mut c = up_at(secs(1));
    c.request(
        secs(2),
        Cx(2),
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    c.request(secs(2), Cx(3), Request::GetFriendsList);
    c.request(
        secs(2),
        Cx(4),
        Request::SendToUserEx {
            user: "Bob@coldeve".into(),
            text: "x".repeat(1_025),
            extra: None,
        },
    );
    let (written, rest) = split_client(c.poll(secs(2)));
    assert_eq!(written.len(), 2, "the tell out of bounds is never sent");
    note(rest);
    c.on_bytes(
        secs(3),
        &frame(&Element::Answer {
            cx: Cx(2),
            hr: Hr::OK,
            answer: Answer::FriendAdd,
        }),
    );
    note(c.poll(secs(3)));
    // A second answer for the same mark, one of another kind, and one for a mark never sent: none
    // reaches the caller.
    for (cx, answer) in [
        (2, Answer::FriendAdd),
        (3, Answer::FriendAdd),
        (99, Answer::GetFriendsList { friends: vec![] }),
    ] {
        c.on_bytes(
            secs(3),
            &frame(&Element::Answer {
                cx: Cx(cx),
                hr: Hr::OK,
                answer,
            }),
        );
    }
    assert!(c.poll(secs(3)).is_empty());
    c.closed(secs(4));
    let outs = c.poll(secs(4));
    assert!(outs.contains(&ClientOut::Answer {
        cx: Cx(3),
        hr: Hr::NOT_CONNECTED,
        answer: Answer::GetFriendsList { friends: vec![] },
    }));
    note(outs);
    answered.sort_by_key(|(cx, _)| *cx);
    assert_eq!(
        answered,
        [
            (Cx(1), Hr::NOT_CONNECTED),
            (Cx(2), Hr::OK),
            (Cx(3), Hr::NOT_CONNECTED),
            (Cx(4), Hr::INVALID_ARGUMENT),
        ]
    );
}

/// Behaviour: friends.stream.protocol-errors-close
#[test]
fn a_world_that_breaks_the_protocol_is_told_so_and_left() {
    let bye = |outs: Vec<ClientOut>| {
        let (written, rest) = split_client(outs);
        assert_eq!(
            written,
            [Element::Bye {
                reason: ByeReason::Protocol
            }]
        );
        assert_eq!(rest[0], ClientOut::Close);
        assert!(matches!(
            rest[1],
            ClientOut::Down {
                ended: Ended::OwnBye(ByeReason::Protocol),
                ..
            }
        ));
    };
    // A frame of no bytes.
    let mut c = up_at(Duration::ZERO);
    c.on_bytes(secs(1), &[0, 0, 0, 0]);
    bye(c.poll(secs(1)));
    // A malformed element.
    let mut c = up_at(Duration::ZERO);
    let mut bytes = Vec::new();
    auberean_friends::frame::write_frame("<FriendStat User=\"Bob@coldeve\"/>", &mut bytes)
        .expect("fits");
    c.on_bytes(secs(1), &bytes);
    bye(c.poll(secs(1)));
    // A version the client does not speak.
    let mut c = client();
    c.connect(Duration::ZERO);
    c.opened(Duration::ZERO);
    let _ = c.poll(Duration::ZERO);
    c.on_bytes(
        Duration::ZERO,
        &frame(&Element::Welcome(Welcome {
            proto: 2,
            ..welcome()
        })),
    );
    bye(c.poll(Duration::ZERO));
    // An element it does not know is skipped, and the stream goes on.
    let mut c = up_at(Duration::ZERO);
    c.on_bytes(
        secs(1),
        &frame(&Element::Unknown {
            name: "Shout".into(),
        }),
    );
    c.on_bytes(
        secs(1),
        &frame(&Element::Event(Event::FriendStat {
            user: "Bob@coldeve".into(),
            mode: auberean_friends::Mode::ONLINE,
        })),
    );
    assert_eq!(
        c.poll(secs(1)),
        [ClientOut::Event(Event::FriendStat {
            user: "Bob@coldeve".into(),
            mode: auberean_friends::Mode::ONLINE,
        })]
    );
    assert!(c.is_up());
}

/// Behaviour: friends.client.disconnect-says-goodbye
#[test]
fn a_client_that_disconnects_says_goodbye_answers_what_it_owed_and_dials_no_more() {
    let mut c = up_at(Duration::ZERO);
    c.request(secs(1), Cx(7), Request::GetFriendsList);
    let _sent = c.poll(secs(1));
    c.disconnect(secs(2));
    let (written, rest) = split_client(c.poll(secs(2)));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Shutdown
        }]
    );
    assert_eq!(
        rest,
        [
            ClientOut::Close,
            ClientOut::Answer {
                cx: Cx(7),
                hr: Hr::NOT_CONNECTED,
                answer: Answer::GetFriendsList { friends: vec![] },
            },
            ClientOut::Down {
                ended: Ended::OwnBye(ByeReason::Shutdown),
                retry_in: None,
            }
        ]
    );
    assert_eq!(c.next_wake(), None);
    assert!(c.poll(secs(1_000)).is_empty());
    c.connect(secs(1_000));
    assert_eq!(c.poll(secs(1_000)), [ClientOut::Dial]);
}
