//! The world's half of one stream alone, against a client written by hand, on a clock the tests
//! turn: the preamble, the greeting, the ticket's place, the binding to a character, the
//! deadlines, the bound on requests, and the answers the stream gives on its own.

use core::time::Duration;

use auberean_friends::{
    Answer, ByeReason, Cx, Element, Ended, Event, FriendStatus, Hello, Hr, Mode, NotSent, Request,
    ServerEvent, ServerOut, ServerStream, Welcome, PREAMBLE,
};

use crate::common::{frame, secs, split_world, world_config};

fn hello(proto: u32) -> Element {
    Element::Hello(Hello {
        proto,
        software: "tests/1".into(),
        locale: None,
    })
}

fn opening(proto: u32) -> Vec<u8> {
    let mut b = PREAMBLE.to_vec();
    b.extend(frame(&hello(proto)));
    b
}

fn request(cx: u64, request: Request) -> Vec<u8> {
    frame(&Element::Request {
        cx: Cx(cx),
        request,
    })
}

fn ticket(cx: u64) -> Vec<u8> {
    request(
        cx,
        Request::PresentTicket {
            email: String::new(),
            ticket: "a1.YW5u.c2VjcmV0".into(),
        },
    )
}

fn answer(cx: u64, hr: Hr, answer: Answer) -> Element {
    Element::Answer {
        cx: Cx(cx),
        hr,
        answer,
    }
}

fn ended(reason: ByeReason) -> [ServerOut; 2] {
    [
        ServerOut::Close,
        ServerOut::Event(ServerEvent::Ended(Ended::OwnBye(reason))),
    ]
}

/// A stream greeted at `t`, its ticket admitted, whatever it said taken.
fn admitted_at(t: Duration) -> ServerStream {
    let mut s = ServerStream::new(t, world_config());
    s.on_bytes(t, &opening(1));
    s.on_bytes(t, &ticket(1));
    let _ = s.poll(t);
    assert!(s.admit(t, Cx(1)));
    let _ = s.poll(t);
    s
}

/// Behaviour: friends.server.greeting
#[test]
fn the_world_welcomes_a_greeting_and_refuses_another_version_or_another_first_element() {
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(secs(1), &opening(1));
    let (written, rest) = split_world(s.poll(secs(1)));
    assert_eq!(
        written,
        [Element::Welcome(Welcome {
            proto: 1,
            world: "dawnsong".into(),
            name: "Dawnsong".into(),
            ping: 20,
        })]
    );
    assert_eq!(
        rest,
        [ServerOut::Event(ServerEvent::Hello(Hello {
            proto: 1,
            software: "tests/1".into(),
            locale: None,
        }))]
    );
    for first in [opening(2), {
        let mut b = PREAMBLE.to_vec();
        b.extend(frame(&Element::Ping { seq: 1 }));
        b
    }] {
        let mut s = ServerStream::new(Duration::ZERO, world_config());
        s.on_bytes(secs(1), &first);
        let (written, rest) = split_world(s.poll(secs(1)));
        assert_eq!(
            written,
            [Element::Bye {
                reason: ByeReason::Protocol
            }]
        );
        assert_eq!(rest, ended(ByeReason::Protocol));
        assert!(s.is_ended());
    }
}

/// Behaviour: friends.server.preamble
#[test]
fn bytes_that_do_not_begin_with_the_preamble_are_closed_at_once() {
    let not_a_stream = |websocket| {
        [
            ServerOut::Close,
            ServerOut::Event(ServerEvent::Ended(Ended::NotAStream { websocket })),
        ]
    };
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(Duration::ZERO, b"X");
    assert_eq!(
        s.poll(Duration::ZERO),
        not_a_stream(false),
        "at the first wrong byte"
    );
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(Duration::ZERO, b"GET /friends HTTP/1.1\r\n");
    assert_eq!(s.poll(Duration::ZERO), not_a_stream(true));
    // The preamble may arrive a byte at a time.
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    for b in opening(1) {
        s.on_bytes(Duration::ZERO, &[b]);
    }
    let (written, _) = split_world(s.poll(Duration::ZERO));
    assert!(matches!(written[..], [Element::Welcome(_)]));
    // Nothing at all within ten seconds.
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    assert!(s.poll(secs(9)).is_empty());
    assert_eq!(s.poll(secs(10)), not_a_stream(false));
}

/// Behaviour: friends.server.ticket-order
#[test]
fn requests_before_the_ticket_and_a_second_ticket_are_answered_by_the_stream() {
    let t = Duration::ZERO;
    let mut s = ServerStream::new(t, world_config());
    s.on_bytes(t, &opening(1));
    let _welcome = s.poll(t);
    s.on_bytes(t, &request(1, Request::GetFriendsList));
    s.on_bytes(t, &ticket(2));
    s.on_bytes(t, &ticket(3));
    let (written, rest) = split_world(s.poll(t));
    assert_eq!(
        written,
        [
            answer(
                1,
                Hr::NOT_TICKETED,
                Answer::GetFriendsList { friends: vec![] }
            ),
            answer(3, Hr::ALREADY_TICKETED, Answer::PresentTicket { status: 0 }),
        ]
    );
    assert_eq!(
        rest,
        [ServerOut::Event(ServerEvent::Ticket {
            cx: Cx(2),
            ticket: "a1.YW5u.c2VjcmV0".into(),
        })]
    );
    assert!(!s.admit(t, Cx(3)), "only the ticket waiting");
    assert!(s.refuse(t, Cx(2), Hr::TICKET_REFUSED));
    s.on_bytes(t, &ticket(4));
    assert!(s.admit(t, Cx(4)));
    s.on_bytes(t, &ticket(5));
    let (written, rest) = split_world(s.poll(t));
    assert_eq!(
        written,
        [
            answer(2, Hr::TICKET_REFUSED, Answer::PresentTicket { status: 0 }),
            answer(4, Hr::OK, Answer::PresentTicket { status: 0 }),
            answer(5, Hr::ALREADY_TICKETED, Answer::PresentTicket { status: 0 }),
        ]
    );
    assert_eq!(
        rest,
        [ServerOut::Event(ServerEvent::Ticket {
            cx: Cx(4),
            ticket: "a1.YW5u.c2VjcmV0".into(),
        })]
    );
    assert!(s.is_admitted());
}

/// Behaviour: friends.server.bound-to-the-character
#[test]
fn until_the_character_enters_every_request_is_answered_not_connected() {
    let t = secs(1);
    let mut s = admitted_at(t);
    let add = |cx| {
        request(
            cx,
            Request::FriendAdd {
                user: "Bob@coldeve".into(),
            },
        )
    };
    s.on_bytes(t, &add(6));
    let stat = Event::FriendStat {
        user: "Bob@coldeve".into(),
        mode: Mode::ONLINE,
    };
    assert_eq!(s.event(t, stat.clone()), Err(NotSent::NotInWorld));
    let friends = vec![FriendStatus::new("Bob@coldeve", Mode::ONLINE)];
    assert_eq!(s.enter(t, friends.clone()), Ok(()));
    assert!(s.is_in_world());
    s.on_bytes(t, &add(7));
    let (written, rest) = split_world(s.poll(t));
    assert_eq!(
        written,
        [
            answer(6, Hr::NOT_CONNECTED, Answer::FriendAdd),
            Element::Event(Event::FStart { friends }),
        ]
    );
    assert_eq!(
        rest,
        [ServerOut::Event(ServerEvent::Request {
            cx: Cx(7),
            request: Request::FriendAdd {
                user: "Bob@coldeve".into()
            },
        })]
    );
    assert_eq!(s.event(t, stat.clone()), Ok(()));
    s.leave();
    assert_eq!(
        s.answer(t, Cx(7), Hr::OK, Answer::FriendAdd),
        Ok(()),
        "a request handed over is still answered after the character leaves"
    );
    assert_eq!(s.event(t, stat.clone()), Err(NotSent::NotInWorld));
    s.on_bytes(t, &add(8));
    let (written, rest) = split_world(s.poll(t));
    assert_eq!(
        written,
        [
            Element::Event(stat),
            answer(7, Hr::OK, Answer::FriendAdd),
            answer(8, Hr::NOT_CONNECTED, Answer::FriendAdd),
        ]
    );
    assert!(rest.is_empty());
}

/// Behaviour: friends.server.deadlines
#[test]
fn the_world_closes_a_stream_without_a_greeting_in_ten_seconds_or_a_ticket_in_thirty() {
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(secs(1), &PREAMBLE);
    assert!(s.poll(Duration::from_millis(9_999)).is_empty());
    assert_eq!(s.next_wake(), Some(secs(10)));
    let (written, rest) = split_world(s.poll(secs(10)));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Idle
        }]
    );
    assert_eq!(rest, ended(ByeReason::Idle));

    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(secs(2), &opening(1));
    let _welcome = s.poll(secs(2));
    s.on_bytes(secs(31), &frame(&Element::Ping { seq: 1 }));
    let (written, rest) = split_world(s.poll(secs(31)));
    assert_eq!(written[0], Element::Pong { seq: 1 });
    assert!(rest.is_empty(), "29 seconds after the welcome");
    let (written, rest) = split_world(s.poll(secs(32)));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Idle
        }]
    );
    assert_eq!(rest, ended(ByeReason::Idle));
}

/// Behaviour: friends.server.flood
#[test]
fn past_twenty_at_once_then_five_a_second_the_stream_is_closed_for_flood() {
    let pings = |n: u32| -> Vec<u8> {
        (0..n)
            .flat_map(|seq| frame(&Element::Ping { seq }))
            .collect()
    };
    let flooded = |outs: Vec<ServerOut>| {
        let (written, rest) = split_world(outs);
        let pongs = written
            .iter()
            .filter(|e| matches!(e, Element::Pong { .. }))
            .count();
        let bye = written.last()
            == Some(&Element::Bye {
                reason: ByeReason::Flood,
            });
        (pongs, bye && rest == ended(ByeReason::Flood))
    };
    // The greeting is the first of twenty at once.
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(Duration::ZERO, &opening(1));
    s.on_bytes(Duration::ZERO, &pings(19));
    assert_eq!(flooded(s.poll(Duration::ZERO)), (19, false));
    s.on_bytes(Duration::ZERO, &pings(1));
    assert_eq!(flooded(s.poll(Duration::ZERO)), (0, true));
    // Then five a second.
    let mut s = ServerStream::new(Duration::ZERO, world_config());
    s.on_bytes(Duration::ZERO, &opening(1));
    s.on_bytes(Duration::ZERO, &pings(19));
    let _ = s.poll(Duration::ZERO);
    s.on_bytes(secs(1), &pings(6));
    assert_eq!(flooded(s.poll(secs(1))), (5, true));
    // A quiet stream earns back no more than twenty.
    let mut s = admitted_at(Duration::ZERO);
    s.on_bytes(secs(60), &pings(21));
    assert_eq!(flooded(s.poll(secs(60))), (20, true));
}

/// Behaviour: friends.server.own-answers
#[test]
fn flags_are_echoed_and_calls_and_rooms_are_not_implemented() {
    let t = secs(1);
    let mut s = admitted_at(t);
    s.enter(t, vec![]).expect("admitted");
    let _ = s.poll(t);
    s.on_bytes(
        t,
        &request(
            1,
            Request::Flags {
                op: 1,
                word: 3,
                flags: 1,
            },
        ),
    );
    s.on_bytes(t, &request(2, Request::CallCreate));
    s.on_bytes(t, &request(3, Request::SendToRoomEx));
    let (written, rest) = split_world(s.poll(t));
    assert_eq!(
        written,
        [
            answer(
                1,
                Hr::OK,
                Answer::Flags {
                    which_flag: 3,
                    flags: 1
                }
            ),
            answer(2, Hr::NOT_IMPLEMENTED, Answer::CallCreate),
            answer(3, Hr::NOT_IMPLEMENTED, Answer::SendToRoomEx),
        ]
    );
    assert!(rest.is_empty(), "nothing for the host");
}

/// Behaviour: friends.server.replaced
#[test]
fn a_replaced_stream_is_told_so_and_closed() {
    let t = secs(1);
    let mut s = admitted_at(t);
    s.replace(t);
    let (written, rest) = split_world(s.poll(t));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Replaced
        }]
    );
    assert_eq!(rest, ended(ByeReason::Replaced));
    s.on_bytes(t, &request(9, Request::GetFriendsList));
    assert!(s.poll(t).is_empty(), "a closed stream hears nothing more");
    assert_eq!(
        s.answer(t, Cx(9), Hr::OK, Answer::GetFriendsList { friends: vec![] }),
        Err(NotSent::NotAdmitted)
    );
    assert_eq!(s.next_wake(), None);
}

/// Behaviour: friends.stream.keep-alive
#[test]
fn the_world_pings_a_quiet_stream_answers_pings_and_closes_a_silent_one() {
    let mut s = admitted_at(Duration::ZERO);
    assert!(s.poll(secs(19)).is_empty());
    let (written, _) = split_world(s.poll(secs(20)));
    assert_eq!(written, [Element::Ping { seq: 1 }]);
    s.on_bytes(secs(30), &frame(&Element::Pong { seq: 1 }));
    s.on_bytes(secs(30), &frame(&Element::Ping { seq: 4 }));
    let (written, _) = split_world(s.poll(secs(30)));
    assert_eq!(written, [Element::Pong { seq: 4 }]);
    assert_eq!(s.next_wake(), Some(secs(50)));
    let (written, _) = split_world(s.poll(secs(99)));
    assert_eq!(
        written,
        [Element::Ping { seq: 2 }],
        "heard at 30: not silent yet"
    );
    let (written, rest) = split_world(s.poll(secs(100)));
    assert_eq!(
        written,
        [Element::Bye {
            reason: ByeReason::Idle
        }]
    );
    assert_eq!(rest, ended(ByeReason::Idle));
}

/// Behaviour: friends.stream.protocol-errors-close
#[test]
fn a_client_that_breaks_the_protocol_is_told_so_and_closed() {
    let t = secs(1);
    for bad in [
        vec![0, 0, 0, 0],
        vec![1, 0, 1, 0],
        vec![2, 0, 0, 0, 0xC3, 0x28],
        {
            let mut b = Vec::new();
            auberean_friends::frame::write_frame("<FriendAdd Cx=\"0x1\"/>", &mut b).expect("fits");
            b
        },
    ] {
        let mut s = admitted_at(t);
        s.on_bytes(t, &bad);
        let (written, rest) = split_world(s.poll(t));
        assert_eq!(
            written,
            [Element::Bye {
                reason: ByeReason::Protocol
            }]
        );
        assert_eq!(rest, ended(ByeReason::Protocol));
    }
    // What it does not know it skips, and a farewell ends the stream without an answer.
    let mut s = admitted_at(t);
    s.on_bytes(
        t,
        &frame(&Element::Unknown {
            name: "Shout".into(),
        }),
    );
    assert!(s.poll(t).is_empty());
    s.on_bytes(
        t,
        &frame(&Element::Bye {
            reason: ByeReason::Shutdown,
        }),
    );
    assert_eq!(
        s.poll(t),
        [
            ServerOut::Close,
            ServerOut::Event(ServerEvent::Ended(Ended::PeerBye(ByeReason::Shutdown)))
        ]
    );
}
