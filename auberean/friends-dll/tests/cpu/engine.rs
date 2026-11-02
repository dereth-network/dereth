//! The library's state against a world in memory, on a clock the tests turn: the connect events
//! the client waits for, one result for each request with the client's own context, the ticket,
//! and the extra data of a tell.

use auberean_friends_dll::engine::{self, Engine, Io, Message, Reply, FIRST_WAIT};
use stream::{Answer, Event, Extra, Hr, Request};

use crate::common::{friends, ignored, secs, ticket, Reach, Rig, WORLD_NAME};

fn result(context: u32, hr: Hr, answer: Answer) -> Message {
    Message::Result {
        context,
        hr,
        reply: Reply::Answer(answer),
    }
}

/// What a ticket the world admits brings: its result, then the friends, since the account's
/// character is in the world.
fn admitted(context: u32) -> [Message; 2] {
    [
        result(context, Hr::OK, Answer::PresentTicket { status: 0 }),
        Message::Event(Event::FStart { friends: friends() }),
    ]
}

/// Behaviour: friends.library.one-result-per-context
#[test]
fn each_request_is_answered_once_at_a_pump_with_the_client_s_own_context() {
    let mut rig = Rig::new();
    rig.connect();
    assert_eq!(rig.take(), [Message::Connected]);

    rig.present(0x0C00_0010);
    assert_eq!(
        rig.take(),
        admitted(0x0C00_0010),
        "the ticket's result carries the client's context"
    );

    rig.ask(0x0C00_0020, Request::GetFriendsList);
    rig.ask(0x0C00_0030, Request::GetIgnoredList);
    rig.ask(
        0x0C00_0040,
        Request::FriendAdd {
            user: format!("Dan@{WORLD_NAME}"),
        },
    );
    rig.ask(
        0x0C00_0050,
        Request::SendToUserEx {
            user: format!("Echo@{WORLD_NAME}"),
            text: "hello".into(),
            extra: None,
        },
    );
    rig.ask(
        0x0C00_0060,
        Request::SendToUserEx {
            user: format!("Bob@{WORLD_NAME}"),
            text: "hello".into(),
            extra: None,
        },
    );
    let seen = rig.take();
    assert_eq!(
        seen,
        [
            result(
                0x0C00_0020,
                Hr::OK,
                Answer::GetFriendsList { friends: friends() }
            ),
            result(
                0x0C00_0030,
                Hr::OK,
                Answer::GetIgnoredList { ignored: ignored() }
            ),
            Message::Event(Event::FriendStat {
                user: format!("Dan@{WORLD_NAME}"),
                mode: stream::Mode::ONLINE,
            }),
            result(0x0C00_0040, Hr::OK, Answer::FriendAdd),
            result(0x0C00_0050, Hr::OK, Answer::SendToUserEx),
            Message::Event(Event::UTellEv {
                user: format!("Echo@{WORLD_NAME}"),
                text: "hello".into(),
                extra: None,
            }),
            result(0x0C00_0060, Hr::NOT_ONLINE, Answer::SendToUserEx),
        ]
    );
    assert_eq!(rig.engine.waiting(), 0, "no context is left waiting");

    // Nothing comes twice, however long the clock runs.
    rig.wait(secs(5));
    assert!(rig.take().is_empty());
}

/// Behaviour: friends.library.down-requests-answered-later
#[test]
fn a_request_made_while_down_is_answered_at_the_next_pump_with_the_not_connected_code() {
    let mut engine = Engine::new("test");
    let now = secs(0);
    engine.request(now, 7, Request::GetFriendsList);
    engine.request(
        now,
        8,
        Request::SendToUserEx {
            user: "Echo@Dawnsong".into(),
            text: "hi".into(),
            extra: None,
        },
    );
    let mut seen = Vec::new();
    while let Some(m) = engine.next_message() {
        seen.push(m);
    }
    assert_eq!(
        seen,
        [
            result(
                7,
                Hr::NOT_CONNECTED,
                Answer::GetFriendsList {
                    friends: Vec::new()
                }
            ),
            result(8, Hr::NOT_CONNECTED, Answer::SendToUserEx),
        ]
    );

    // A request in flight when the connection goes is answered the same way, once.
    let mut rig = Rig::new();
    rig.connect();
    rig.take();
    rig.reach = Reach::Silent;
    rig.ask(9, Request::GetIgnoredList);
    assert!(rig.take().is_empty(), "the world has not answered yet");
    rig.drop_world();
    assert_eq!(
        rig.take(),
        [
            result(
                9,
                Hr::NOT_CONNECTED,
                Answer::GetIgnoredList {
                    ignored: Vec::new()
                }
            ),
            Message::Lost,
        ]
    );
}

/// Behaviour: friends.library.first-connection-fails-once
#[test]
fn a_first_connection_that_fails_is_one_lost_event_and_the_library_stays_down() {
    let mut rig = Rig::new();
    rig.reach = Reach::Unreachable;
    rig.connect();
    assert_eq!(rig.take(), [Message::Lost]);
    assert_eq!(rig.dials.len(), 1);
    rig.wait(secs(120));
    assert!(rig.take().is_empty(), "one event, and only one");
    assert_eq!(
        rig.dials.len(),
        1,
        "the library stays down for the rest of the login"
    );

    // A world that answers the dial but never welcomes the client is given up the same way.
    let mut rig = Rig::new();
    rig.reach = Reach::Silent;
    rig.connect();
    assert!(rig.take().is_empty());
    rig.wait(secs(9));
    assert!(rig.take().is_empty());
    rig.wait(secs(1));
    assert_eq!(rig.take(), [Message::Lost]);
    rig.wait(secs(120));
    assert!(rig.take().is_empty());
    assert_eq!(rig.dials.len(), 1);

    // The next login's connect tries again.
    rig.reach = Reach::Serves;
    rig.connect();
    assert_eq!(rig.take(), [Message::Connected]);
}

/// Behaviour: friends.library.first-connection-limit
#[test]
fn a_first_connection_with_no_answer_at_all_is_given_up_after_fifteen_seconds() {
    let mut rig = Rig::new();
    rig.reach = Reach::Hangs;
    rig.connect();
    rig.wait(FIRST_WAIT - secs(1));
    assert!(rig.take().is_empty());
    rig.wait(secs(1));
    assert_eq!(rig.take(), [Message::Lost]);
    assert_eq!(rig.closes, rig.dials, "the hanging dial is closed");
    rig.wait(secs(120));
    assert!(rig.take().is_empty());
    assert_eq!(rig.dials.len(), 1);
}

/// Behaviour: friends.library.lost-connection-redials
#[test]
fn a_lost_connection_is_one_event_and_the_library_dials_again_on_its_own_waits() {
    let mut rig = Rig::new();
    rig.connect();
    rig.present(1);
    rig.take();

    rig.reach = Reach::Unreachable;
    rig.drop_world();
    assert_eq!(rig.take(), [Message::Lost]);

    // The client calls connect again at once: no event of its own, and no dial before the wait.
    let now = rig.now;
    rig.engine.connect(now, "127.0.0.1", 9000);
    rig.settle();
    assert!(rig.take().is_empty());
    assert_eq!(rig.dials.len(), 1);

    // The waits between dials: 1, 2, 5, 10, then 30 seconds; failed dials say nothing.
    let mut at = Vec::new();
    for _ in 0..(60 * 2) {
        let before = rig.dials.len();
        rig.wait(secs(1));
        if rig.dials.len() > before {
            at.push(rig.now.as_secs());
        }
    }
    assert_eq!(at, [1, 3, 8, 18, 48, 78, 108]);
    assert!(rig.take().is_empty());

    // Back: connected once more, and the ticket presented again is admitted.
    rig.reach = Reach::Serves;
    rig.wait(secs(30));
    assert_eq!(rig.take(), [Message::Connected]);
    rig.present(2);
    assert_eq!(rig.take(), admitted(2));
}

/// Behaviour: friends.library.world-goodbye-disconnects
#[test]
fn a_world_that_says_goodbye_is_a_disconnected_event_and_the_library_dials_again() {
    let mut rig = Rig::new();
    rig.connect();
    rig.take();
    rig.world_says_goodbye();
    assert_eq!(rig.take(), [Message::Disconnected]);
    rig.wait(secs(1));
    assert_eq!(rig.take(), [Message::Connected]);
    assert_eq!(rig.dials.len(), 2);
}

/// Behaviour: friends.library.second-login-redials
#[test]
fn a_connect_while_connected_closes_the_stream_and_dials_again_with_one_event() {
    let mut rig = Rig::new();
    rig.connect();
    rig.take();
    rig.reach = Reach::Silent;
    rig.ask(5, Request::GetFriendsList);
    rig.reach = Reach::Serves;
    rig.connect();
    assert_eq!(
        rig.take(),
        [
            result(
                5,
                Hr::NOT_CONNECTED,
                Answer::GetFriendsList {
                    friends: Vec::new()
                }
            ),
            Message::Connected,
        ],
        "what the old stream owed, then one event for the new attempt"
    );
    assert_eq!(rig.dials, [1, 2]);
    assert_eq!(rig.closes, [1]);
}

/// Behaviour: friends.library.ticket-kept
#[test]
fn the_ticket_is_kept_without_its_nul_and_what_the_client_presents_is_sent() {
    let mut engine = Engine::new("test");
    assert_eq!(engine.ticket(), None, "started without a ticket");
    let t = ticket("ann");
    let mut given = t.clone().into_bytes();
    given.push(0);
    engine.set_ticket(&given);
    assert_eq!(engine.ticket(), Some(t.as_bytes()));

    // What the client presents (the ticket it was handed back) reaches the world as it was.
    let mut rig = Rig::new();
    rig.engine.set_ticket(&given);
    rig.connect();
    rig.take();
    let now = rig.now;
    let back = rig.engine.ticket().expect("the ticket").to_vec();
    rig.engine.present_ticket(now, 3, &back);
    rig.settle();
    assert_eq!(rig.take(), admitted(3));

    // A ticket the world refuses comes back as its code.
    let refused = ticket(crate::common::REFUSED);
    let mut rig = Rig::new();
    rig.connect();
    rig.take();
    let now = rig.now;
    rig.engine.present_ticket(now, 4, refused.as_bytes());
    rig.settle();
    assert_eq!(
        rig.take(),
        [result(
            4,
            Hr::TICKET_REFUSED,
            Answer::PresentTicket { status: 0 }
        )]
    );
}

/// Behaviour: friends.library.extra-carried
#[test]
fn a_tell_s_extra_data_is_carried_in_sixteen_bytes_or_left_off() {
    assert_eq!(engine::extra(&[]), None);
    let twelve = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut padded = [0u8; 16];
    padded[0] = 1;
    assert_eq!(engine::extra(&twelve), Some(Extra(padded)));
    let sixteen: [u8; 16] = core::array::from_fn(|i| u8::try_from(i).unwrap_or(0));
    assert_eq!(engine::extra(&sixteen), Some(Extra(sixteen)));
    assert_eq!(engine::extra(&[0u8; 17]), None);

    // Carried to a teller on the same world, and back.
    let mut rig = Rig::new();
    rig.connect();
    rig.present(1);
    rig.take();
    rig.ask(
        2,
        Request::SendToUserEx {
            user: format!("Echo@{WORLD_NAME}"),
            text: "hi".into(),
            extra: engine::extra(&twelve),
        },
    );
    let seen = rig.take();
    assert!(seen.contains(&Message::Event(Event::UTellEv {
        user: format!("Echo@{WORLD_NAME}"),
        text: "hi".into(),
        extra: Some(Extra(padded)),
    })));
}

/// Behaviour: friends.library.one-result-per-context
#[test]
fn the_release_of_the_ticket_is_answered_by_the_library_and_the_greeting_names_the_locale() {
    let mut engine = Engine::new("test");
    engine.release_ticket(0x55);
    assert_eq!(
        engine.next_message(),
        Some(Message::Result {
            context: 0x55,
            hr: Hr::OK,
            reply: Reply::TicketReleased,
        })
    );
    assert_eq!(engine.next_message(), None);

    // The locale the client sets up with goes in the greeting.
    let mut engine = Engine::new("test");
    engine.init(1033);
    engine.connect(secs(0), "127.0.0.1", 9000);
    let conn = match engine.take_io().as_slice() {
        [Io::Dial { conn, .. }] => *conn,
        other => panic!("one dial, not {other:?}"),
    };
    engine.opened(secs(0), conn);
    let greeting = engine
        .take_io()
        .into_iter()
        .find_map(|io| match io {
            Io::Write { bytes, .. } => Some(bytes),
            _ => None,
        })
        .expect("the greeting");
    let text = String::from_utf8_lossy(&greeting);
    assert!(text.contains("Locale=\"1033\""), "{text}");
}
