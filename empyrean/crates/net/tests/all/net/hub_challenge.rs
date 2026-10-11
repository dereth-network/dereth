//! Divergence: V451, the Dereth Network hub's challenge on the game port.
//! A challenge on `P` is kept for the world when the network is on, and dropped when it is off,
//! on `P + 1` or past what is kept; it is never answered over UDP, and makes no session or event.
//! Fixture: locally constructed datagrams on a virtual clock.

use std::collections::VecDeque;

use dereth_network_protocol::challenge::{Challenge, NONCE_LEN};
use empyrean_net::managers::network_manager::HUB_CHALLENGES_KEPT;
use empyrean_net::{ClockSnapshot, ClockSnapshotExt, NetConfig, Outgoing, PortKind, ServerNet};

use crate::common::{client_addr, server};

fn challenge(nonce: u8) -> Challenge {
    Challenge {
        slug: "aa".into(),
        nonce: [nonce; NONCE_LEN],
    }
}

fn send(net: &mut ServerNet, kind: PortKind, bytes: &[u8]) -> Vec<Outgoing> {
    let now = ClockSnapshot::at_seconds(100.0);
    net.on_datagram(kind, client_addr(5, 40_000), bytes, now);
    net.poll(now).collect()
}

#[test]
fn a_hub_challenge_on_the_game_port_is_kept_for_the_world_and_never_answered() {
    let mut net = server(NetConfig::default());
    net.hub_challenges = Some(VecDeque::new());
    let c = challenge(1);
    let out = send(
        &mut net,
        PortKind::C2S,
        &c.encode().expect("a slug it carries"),
    );
    assert!(out.is_empty(), "nothing goes back");
    assert_eq!(
        net.hub_challenges
            .as_ref()
            .map(|k| k.iter().cloned().collect::<Vec<_>>()),
        Some(vec![c])
    );
    assert_eq!(net.get_session_count(), 0, "no session");
    assert_eq!(net.events().count(), 0, "no event");

    let out = send(
        &mut net,
        PortKind::S2C,
        &challenge(2).encode().expect("a slug it carries"),
    );
    assert!(out.is_empty());
    assert_eq!(
        net.hub_challenges.as_ref().map(VecDeque::len),
        Some(1),
        "not on P + 1"
    );
    for n in 0..40 {
        send(
            &mut net,
            PortKind::C2S,
            &challenge(n).encode().expect("a slug it carries"),
        );
    }
    assert_eq!(
        net.hub_challenges.as_ref().map(VecDeque::len),
        Some(HUB_CHALLENGES_KEPT),
        "no more kept than the world takes in a turn"
    );
}

#[test]
fn with_the_network_off_a_hub_challenge_is_dropped_as_aces_packet_reader_drops_it() {
    let mut net = server(NetConfig::default());
    let out = send(
        &mut net,
        PortKind::C2S,
        &challenge(1).encode().expect("a slug it carries"),
    );
    assert!(out.is_empty());
    assert!(net.hub_challenges.is_none());
    assert_eq!(net.get_session_count(), 0, "no session");
    assert_eq!(net.events().count(), 0, "no event");
}
