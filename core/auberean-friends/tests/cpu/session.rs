//! A client and a world joined by an in-memory connection on a clock the tests turn: the greeting,
//! the ticket, the friends list, a request answered by the world and one by the stream, a tell
//! each way, and a reconnect after which the world sends the friends list again.

use auberean_friends::{
    Answer, ClientOut, Cx, Ended, Event, FriendStatus, Hr, Mode, Request, ServerEvent, Ticket,
};

use crate::common::{secs, Pair};

fn present_ticket(pair: &mut Pair, cx: u64) {
    let ticket = Ticket::new("ann", "secret").encode().expect("a ticket");
    let now = pair.now;
    pair.client.request(
        now,
        Cx(cx),
        Request::PresentTicket {
            email: String::new(),
            ticket,
        },
    );
    pair.settle();
}

/// The host's check of a ticket the world was handed: here, the one account and its secret.
fn check_ticket(pair: &mut Pair) {
    let (_, world) = pair.take();
    let Some(ServerEvent::Ticket { cx, ticket }) = world.into_iter().last() else {
        panic!("the world was handed no ticket");
    };
    let t = Ticket::decode(&ticket).expect("an a1. ticket");
    assert_eq!((t.account.as_str(), t.secret.as_str()), ("ann", "secret"));
    let now = pair.now;
    assert!(pair.stream().admit(now, cx));
}

fn friends() -> Vec<FriendStatus> {
    vec![
        FriendStatus::new("Bob@coldeve", Mode::ONLINE),
        FriendStatus::new("Cara@Dawnsong", Mode::ONLINE.with(Mode::AWAY)),
    ]
}

/// Behaviour: friends.session.greet-admit-and-trade
#[test]
fn a_client_and_its_world_greet_admit_the_ticket_and_trade_friends_and_tells() {
    let mut pair = Pair::new();
    pair.client.connect(pair.now);
    pair.settle();
    let (client, world) = pair.take();
    assert!(
        matches!(client[..], [ClientOut::Up(ref w)] if w.world == "dawnsong" && w.name == "Dawnsong")
    );
    assert!(
        matches!(world[..], [ServerEvent::Hello(ref h)] if h.software == "auberean-friends-tests/1")
    );

    present_ticket(&mut pair, 0x10);
    check_ticket(&mut pair);
    let now = pair.now;
    pair.stream().enter(now, friends()).expect("admitted");
    pair.settle();
    let (client, _) = pair.take();
    assert_eq!(
        client,
        [
            ClientOut::Answer {
                cx: Cx(0x10),
                hr: Hr::OK,
                answer: Answer::PresentTicket { status: 0 },
            },
            ClientOut::Event(Event::FStart { friends: friends() }),
        ]
    );

    // A friend added: the world sends the friend's state before its answer.
    pair.at(secs(5));
    pair.client.request(
        secs(5),
        Cx(0x11),
        Request::FriendAdd {
            user: "Dex@coldeve".into(),
        },
    );
    pair.settle();
    let (_, world) = pair.take();
    assert_eq!(
        world,
        [ServerEvent::Request {
            cx: Cx(0x11),
            request: Request::FriendAdd {
                user: "Dex@coldeve".into()
            },
        }]
    );
    let stream = pair.stream();
    let stat = Event::FriendStat {
        user: "Dex@coldeve".into(),
        mode: Mode::OFFLINE,
    };
    stream.event(secs(5), stat.clone()).expect("in the world");
    stream
        .answer(secs(5), Cx(0x11), Hr::OK, Answer::FriendAdd)
        .expect("admitted");
    // The start-of-session flags, which the stream answers itself, and a tell each way.
    pair.client.request(
        secs(5),
        Cx(0x12),
        Request::Flags {
            op: 1,
            word: 1,
            flags: 1,
        },
    );
    pair.client.request(
        secs(5),
        Cx(0x13),
        Request::SendToUserEx {
            user: "Bob@coldeve".into(),
            text: "hi".into(),
            extra: None,
        },
    );
    pair.settle();
    let (client, world) = pair.take();
    assert_eq!(
        world,
        [ServerEvent::Request {
            cx: Cx(0x13),
            request: Request::SendToUserEx {
                user: "Bob@coldeve".into(),
                text: "hi".into(),
                extra: None,
            },
        }]
    );
    assert_eq!(
        client,
        [
            ClientOut::Event(stat),
            ClientOut::Answer {
                cx: Cx(0x11),
                hr: Hr::OK,
                answer: Answer::FriendAdd,
            },
            ClientOut::Answer {
                cx: Cx(0x12),
                hr: Hr::OK,
                answer: Answer::Flags {
                    which_flag: 1,
                    flags: 1
                },
            },
        ]
    );
    let tell = Event::UTellEv {
        user: "Bob@coldeve".into(),
        text: "hey".into(),
        extra: None,
    };
    let stream = pair.stream();
    stream
        .answer(secs(5), Cx(0x13), Hr::OK, Answer::SendToUserEx)
        .expect("admitted");
    stream.event(secs(5), tell.clone()).expect("in the world");
    pair.settle();
    let (client, _) = pair.take();
    assert_eq!(
        client,
        [
            ClientOut::Answer {
                cx: Cx(0x13),
                hr: Hr::OK,
                answer: Answer::SendToUserEx,
            },
            ClientOut::Event(tell),
        ]
    );

    // A quiet minute: each side pings the other, and the stream stands.
    for t in (10..=70).step_by(10) {
        pair.at(secs(t));
    }
    let (client, world) = pair.take();
    assert!(
        client.is_empty() && world.is_empty(),
        "{client:?} {world:?}"
    );
    assert!(pair.client.is_up() && pair.stream().is_in_world());
    assert_eq!(pair.dials, 1);
}

/// Behaviour: friends.session.fresh-friends-list-after-reconnect
#[test]
fn after_a_reconnect_the_client_presents_again_and_the_world_sends_the_friends_list_again() {
    let mut pair = Pair::new();
    pair.client.connect(pair.now);
    pair.settle();
    present_ticket(&mut pair, 0x20);
    check_ticket(&mut pair);
    let now = pair.now;
    pair.stream().enter(now, friends()).expect("admitted");
    pair.settle();
    pair.take();

    // The connection drops under both.
    pair.at(secs(10));
    pair.stream().closed(secs(10));
    pair.settle();
    let (client, world) = pair.take();
    assert_eq!(
        client,
        [ClientOut::Down {
            ended: Ended::Lost,
            retry_in: Some(secs(1)),
        }]
    );
    assert_eq!(world, [ServerEvent::Ended(Ended::Lost)]);

    // A second later the client is back, and presents its ticket again as soon as it is up.
    pair.at(secs(11));
    assert_eq!(pair.dials, 2);
    let (client, _) = pair.take();
    assert!(matches!(client[..], [ClientOut::Up(_)]));
    pair.at(secs(11));
    present_ticket(&mut pair, 0x21);
    check_ticket(&mut pair);
    // The account's character is still in the world, so the host sends its list at once.
    let now = pair.now;
    pair.stream().enter(now, friends()).expect("admitted");
    pair.settle();
    let (client, _) = pair.take();
    assert_eq!(
        client,
        [
            ClientOut::Answer {
                cx: Cx(0x21),
                hr: Hr::OK,
                answer: Answer::PresentTicket { status: 0 },
            },
            ClientOut::Event(Event::FStart { friends: friends() }),
        ]
    );
}
