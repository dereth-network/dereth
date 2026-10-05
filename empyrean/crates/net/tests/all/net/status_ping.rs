//! Divergence: V442, the status ping on the game port.
//! A hello is answered with a token from `P`, an ask with that token with the status, and
//! anything else (no token, a stale one, the ping off, the wrong port) with nothing; none of it
//! makes a session or an event.
//! Fixture: locally constructed datagrams on a virtual clock.

use dereth_primitives::era::{EraFeatureBits, EraFeatures};
use dereth_transport::status_ping::{self, StatusReply, Token, WorldState};
use empyrean_net::status_ping::{StatusFacts, StatusPing, StatusPingConfig};
use empyrean_net::{ClockSnapshot, ClockSnapshotExt, NetConfig, Outgoing, PortKind, ServerNet};

use crate::common::{client_addr, server};

fn with_ping(config: StatusPingConfig) -> ServerNet {
    let mut net = server(NetConfig::default());
    net.status_ping = Some(StatusPing::new(
        config,
        StatusFacts {
            state: WorldState::Open,
            players: 7,
            era: "eor".into(),
            features: EraFeatures::END_OF_RETAIL,
            software: "Empyrean".into(),
            software_version: "0.2.0".into(),
            world_name: "Loopback".into(),
        },
        Box::new(|| [9; 32]),
    ));
    net
}

fn send(net: &mut ServerNet, kind: PortKind, bytes: &[u8], at: f64) -> Vec<Outgoing> {
    let now = ClockSnapshot::at_seconds(at);
    net.on_datagram(kind, client_addr(5, 40_000), bytes, now);
    net.poll(now).collect()
}

/// Whatever the ping is sent, the session table and the event queue stay empty.
fn assert_untouched(net: &mut ServerNet) {
    assert_eq!(net.get_session_count(), 0, "no session");
    assert_eq!(net.events().count(), 0, "no event");
}

#[test]
fn a_hello_gets_a_token_and_the_ask_with_it_gets_the_status_without_a_session() {
    let mut net = with_ping(StatusPingConfig::default());
    let out = send(&mut net, PortKind::C2S, &status_ping::hello(), 100.0);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].to, client_addr(5, 40_000));
    assert_eq!(out[0].via_port_kind, PortKind::C2S);
    assert_eq!(out[0].session, None);
    assert!(out[0].bytes.len() <= status_ping::REQUEST_LEN);
    let token = status_ping::parse_token_reply(&out[0].bytes).expect("a token");

    let out = send(&mut net, PortKind::C2S, &status_ping::ask(token), 101.0);
    assert_eq!(out.len(), 1);
    let s = StatusReply::parse(&out[0].bytes).expect("the status");
    assert!(out[0].bytes.len() <= status_ping::MAX_REPLY_LEN);
    assert_eq!(
        (s.state, s.players, s.era.as_str(), s.world_name.as_str()),
        (WorldState::Open, 7, "eor", "Loopback")
    );
    let bits = EraFeatureBits {
        table_version: s.era_table_version,
        bytes: s.era_features,
    };
    assert_eq!(
        bits.overrides().apply(EraFeatures::NONE),
        EraFeatures::END_OF_RETAIL
    );
    assert_untouched(&mut net);
}

#[test]
fn an_ask_without_a_token_or_with_a_stale_one_gets_nothing() {
    let mut net = with_ping(StatusPingConfig::default());
    assert!(send(
        &mut net,
        PortKind::C2S,
        &status_ping::ask(Token::default()),
        100.0
    )
    .is_empty());
    let out = send(&mut net, PortKind::C2S, &status_ping::hello(), 100.0);
    let token = status_ping::parse_token_reply(&out[0].bytes).expect("a token");
    // Two windows later.
    assert!(send(&mut net, PortKind::C2S, &status_ping::ask(token), 160.0).is_empty());
    assert_untouched(&mut net);
}

#[test]
fn the_excess_over_the_rate_limit_gets_nothing() {
    let mut net = with_ping(StatusPingConfig {
        hellos_per_minute: 2,
        ..StatusPingConfig::default()
    });
    let answered: usize = (0..5)
        .map(|i| {
            send(
                &mut net,
                PortKind::C2S,
                &status_ping::hello(),
                120.0 + f64::from(i),
            )
            .len()
        })
        .sum();
    assert_eq!(answered, 2);
    assert_untouched(&mut net);
}

#[test]
fn with_the_ping_off_or_not_set_up_or_on_the_other_port_nothing_is_said() {
    for mut net in [
        with_ping(StatusPingConfig {
            enabled: false,
            ..StatusPingConfig::default()
        }),
        server(NetConfig::default()),
    ] {
        assert!(send(&mut net, PortKind::C2S, &status_ping::hello(), 100.0).is_empty());
        assert_untouched(&mut net);
    }
    let mut net = with_ping(StatusPingConfig::default());
    assert!(send(&mut net, PortKind::S2C, &status_ping::hello(), 100.0).is_empty());
    assert_untouched(&mut net);
}
