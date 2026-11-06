//! The Dereth Network hub's challenge to the world's game port, taken before the packet reader.

use std::net::{SocketAddr, SocketAddrV4};

use dereth_network_protocol::challenge::{Challenge, NONCE_LEN};
use yalain_net::server::{NetConfig, Port, ServerNet, HUB_CHALLENGES_KEPT};
use yalain_net::{fixed_stream, LocalTime};

fn challenge(nonce: u8) -> Challenge {
    Challenge {
        slug: "dawnsong".into(),
        nonce: [nonce; NONCE_LEN],
    }
}

fn bytes(c: &Challenge) -> Vec<u8> {
    c.encode().expect("a slug it carries")
}

fn net() -> ServerNet {
    let world: SocketAddrV4 = "127.0.0.1:9002".parse().expect("an address");
    ServerNet::new(
        NetConfig::retail_ids(world),
        fixed_stream(7),
        0.0,
        LocalTime(1.0),
    )
}

/// A datagram from the hub to `port`; what the server sent back and the events it made.
fn heard(net: &mut ServerNet, port: Port, bytes: &[u8]) -> (usize, usize) {
    let hub: SocketAddr = "203.0.113.9:40000".parse().expect("an address");
    let now = LocalTime(2.0);
    net.on_datagram(port, hub, bytes, now);
    net.tick(now);
    let mut events = 0;
    while net.poll_event().is_some() {
        events += 1;
    }
    (net.take_datagrams().len(), events)
}

/// While the world hears them, a hub's challenge on the login service's even port (the game port)
/// is kept for it, read; on any other port, past what is kept between two passes, or with the
/// world hearing none (no verification waiting, or the network off), it is dropped. It is never
/// answered over UDP, and makes no connection or event.
/// Behaviour: server.network.verify-game-port-keeps-challenges
#[test]
fn a_hub_challenge_on_the_game_port_is_kept_only_while_the_world_hears_them_and_never_answered() {
    let mut net = net();
    assert_eq!(
        heard(&mut net, Port::LOGIN_EVEN, &bytes(&challenge(1))),
        (0, 0)
    );
    assert!(net.take_hub_challenges().is_empty(), "the world hears none");

    net.hear_hub_challenges(true);
    assert_eq!(
        heard(&mut net, Port::LOGIN_EVEN, &bytes(&challenge(2))),
        (0, 0)
    );
    for port in [Port::LOGIN_ODD, Port::WORLD_EVEN, Port::WORLD_ODD] {
        assert_eq!(
            heard(&mut net, port, &bytes(&challenge(3))),
            (0, 0),
            "{port:?}"
        );
    }
    let mut cut = bytes(&challenge(4));
    cut.truncate(30);
    heard(&mut net, Port::LOGIN_EVEN, &cut);
    assert_eq!(
        net.take_hub_challenges(),
        [challenge(2)],
        "the game port's, read"
    );
    assert!(net.take_hub_challenges().is_empty(), "taken");
    assert_eq!(net.connection_count(), 0, "no connection");

    for n in 0..40 {
        heard(&mut net, Port::LOGIN_EVEN, &bytes(&challenge(n)));
    }
    assert_eq!(
        net.take_hub_challenges().len(),
        HUB_CHALLENGES_KEPT,
        "no more kept than a pass takes"
    );
    heard(&mut net, Port::LOGIN_EVEN, &bytes(&challenge(5)));
    net.hear_hub_challenges(false);
    assert!(net.take_hub_challenges().is_empty(), "dropped untaken");
    heard(&mut net, Port::LOGIN_EVEN, &bytes(&challenge(6)));
    net.hear_hub_challenges(true);
    assert!(
        net.take_hub_challenges().is_empty(),
        "dropped while not heard"
    );
    assert_eq!(net.connection_count(), 0);
}
