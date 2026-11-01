//! The library on real sockets, against a world on loopback: the threads that dial, read and write,
//! and the host that runs the engine on them.

use auberean_friends::engine::{Message, Reply};
use auberean_friends::host::Host;
use stream::{Answer, Event, Hr, Request};

use crate::common::{friends, ticket, until, LoopbackWorld, WORLD_NAME};

/// Pump the host until a message comes.
fn next(host: &mut Host) -> Message {
    until("a message", || {
        host.intake();
        host.next_message()
    })
}

/// Behaviour: friends.library.real-sockets
#[test]
fn the_library_on_real_sockets_connects_trades_and_comes_back_after_a_cut() {
    let world = LoopbackWorld::start();
    let mut host = Host::new();
    host.init(1033);
    host.set_ticket(format!("{}\0", ticket("ann")).as_bytes());
    host.connect("127.0.0.1", world.port);
    assert_eq!(next(&mut host), Message::Connected);
    assert_eq!(
        world.greetings.recv().ok(),
        Some(1033),
        "the greeting names the locale"
    );

    let t = host.ticket().expect("the ticket").to_vec();
    host.present_ticket(10, &t);
    assert_eq!(
        next(&mut host),
        Message::Result {
            context: 10,
            hr: Hr::OK,
            reply: Reply::Answer(Answer::PresentTicket { status: 0 }),
        }
    );
    // The world sent the friends at the character's entry.
    assert_eq!(
        next(&mut host),
        Message::Event(Event::FStart { friends: friends() })
    );

    host.request(
        11,
        Request::SendToUserEx {
            user: format!("Echo@{WORLD_NAME}"),
            text: "over the wire".into(),
            extra: None,
        },
    );
    assert_eq!(
        next(&mut host),
        Message::Result {
            context: 11,
            hr: Hr::OK,
            reply: Reply::Answer(Answer::SendToUserEx),
        }
    );
    assert_eq!(
        next(&mut host),
        Message::Event(Event::UTellEv {
            user: format!("Echo@{WORLD_NAME}"),
            text: "over the wire".into(),
            extra: None,
        })
    );

    // Cut: one lost event; the client's connect at once changes nothing; the library dials again
    // after its first wait and is connected once more.
    world.cut();
    assert_eq!(next(&mut host), Message::Lost);
    host.connect("127.0.0.1", world.port);
    assert_eq!(next(&mut host), Message::Connected);
    host.present_ticket(12, &t);
    assert_eq!(
        next(&mut host),
        Message::Result {
            context: 12,
            hr: Hr::OK,
            reply: Reply::Answer(Answer::PresentTicket { status: 0 }),
        }
    );
}

/// Behaviour: friends.library.first-connection-fails-once
#[test]
fn a_world_that_is_not_there_is_one_lost_event() {
    // A port nothing listens on: bound, then let go.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        l.local_addr().expect("its address").port()
    };
    let mut host = Host::new();
    host.connect("127.0.0.1", port);
    assert_eq!(next(&mut host), Message::Lost);
}
