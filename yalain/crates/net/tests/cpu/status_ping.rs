//! The status ping on the login port, as the specification and Empyrean have it: a hello answered
//! with a token, an ask with that token answered with the world's status, and nothing for anything
//! else. Fixture: made-up facts, a fixed sequence of secrets, and the clock in seconds given by the
//! test.

use std::net::SocketAddr;

use dereth_transport::status_ping::{self, StatusReply, Token, TOKEN_REPLY_LEN};
use yalain_net::status_ping::{
    Facts, Limits, StatusPing, WorldState, SECRET_WINDOWS, WINDOW_SECONDS,
};

fn ping(on: bool, limits: Limits) -> StatusPing {
    let mut n = 0u8;
    StatusPing::new(
        on,
        limits,
        Facts {
            state: WorldState::Open,
            players: 3,
            software: "Yalain".into(),
            version: "0.1.0".into(),
            world_name: "Builder's World".into(),
        },
        Box::new(move || {
            n = n.wrapping_add(1);
            [n; 32]
        }),
    )
}

fn at(text: &str) -> SocketAddr {
    text.parse().expect("an address")
}

/// The token a hello draws, at `seconds`.
fn token(p: &mut StatusPing, from: SocketAddr, seconds: u64) -> Option<Token> {
    p.answer(from, &status_ping::hello(), seconds)
        .and_then(|r| status_ping::parse_token_reply(&r))
}

/// A hello is answered with a token alone, smaller than the hello; an ask carrying it, from the
/// same address and port in its window or the next, with the world's state, players, software,
/// version and name, no era and no systems. The same token from another port, two windows on, or
/// with its code changed draws nothing, and so does a request cut short or a ping turned off.
/// Behaviour: server.operator.status-ping-on-the-login-port
#[test]
fn a_hello_draws_a_token_and_an_ask_with_it_draws_the_worlds_status() {
    let me = at("203.0.113.7:50123");
    let mut p = ping(true, Limits::default());
    let hello = status_ping::hello();
    let reply = p.answer(me, &hello, 100).expect("a token");
    assert_eq!(reply.len(), TOKEN_REPLY_LEN);
    assert!(reply.len() < hello.len(), "never more than it was sent");
    let t = status_ping::parse_token_reply(&reply).expect("a token reply");

    let status = p.answer(me, &status_ping::ask(t), 100).expect("the status");
    let s = StatusReply::parse(&status).expect("a status reply");
    assert_eq!(s.state, WorldState::Open);
    assert_eq!(s.players, 3);
    assert_eq!(
        (
            s.software.as_str(),
            s.software_version.as_str(),
            s.world_name.as_str()
        ),
        ("Yalain", "0.1.0", "Builder's World")
    );
    assert_eq!((s.era.as_str(), s.era_table_version), ("", 0));
    assert!(s.era_features.is_empty());
    p.facts.state = WorldState::ShuttingDown;
    p.facts.players = 1;
    let later = p
        .answer(me, &status_ping::ask(t), 100 + WINDOW_SECONDS)
        .expect("good in the next window");
    let s = StatusReply::parse(&later).expect("a status reply");
    assert_eq!((s.state, s.players), (WorldState::ShuttingDown, 1));

    assert_eq!(
        p.answer(at("203.0.113.7:50124"), &status_ping::ask(t), 101),
        None
    );
    assert_eq!(
        p.answer(at("198.51.100.1:50123"), &status_ping::ask(t), 101),
        None
    );
    assert_eq!(
        p.answer(me, &status_ping::ask(t), 100 + 2 * WINDOW_SECONDS),
        None
    );
    let mut forged = t;
    forged.mac[0] ^= 1;
    assert_eq!(p.answer(me, &status_ping::ask(forged), 101), None);
    assert_eq!(p.answer(me, &hello[..hello.len() - 1], 101), None);
    assert_eq!(
        p.answer(me, &reply, 101),
        None,
        "a token reply is no request"
    );

    let mut off = ping(false, Limits::default());
    assert_eq!(off.answer(me, &hello, 100), None);
}

/// A handshake that spans a new secret completes: a token given in the last window under one
/// secret is good in the first window under the next, and tokens under the new secret are good.
/// Behaviour: server.operator.status-ping-on-the-login-port
#[test]
fn a_token_given_before_the_secret_changes_is_good_in_the_next_window() {
    let me = at("192.0.2.9:4000");
    let mut p = ping(true, Limits::default());
    let last = SECRET_WINDOWS * WINDOW_SECONDS - 1;
    let t = token(&mut p, me, last).expect("a token");
    assert!(
        p.answer(me, &status_ping::ask(t), last + 1).is_some(),
        "across the change"
    );
    let t = token(&mut p, me, last + 60).expect("a token under the new secret");
    assert!(p.answer(me, &status_ping::ask(t), last + 61).is_some());
}

/// Each address has four hellos and four asks a minute, and the server sends at most its replies
/// a second in all; past them nothing is answered, until the next minute or second.
/// Behaviour: server.operator.status-ping-on-the-login-port
#[test]
fn hellos_and_asks_past_the_limits_draw_nothing() {
    let me = at("203.0.113.7:50123");
    let mut p = ping(true, Limits::default());
    for _ in 0..4 {
        assert!(token(&mut p, me, 600).is_some());
    }
    assert_eq!(token(&mut p, me, 600), None, "the fifth hello this minute");
    assert!(
        token(&mut p, at("203.0.113.8:1"), 600).is_some(),
        "another address"
    );
    assert!(token(&mut p, me, 660).is_some(), "the next minute");

    let mut p = ping(
        true,
        Limits {
            replies_per_second: 2,
            ..Limits::default()
        },
    );
    assert!(token(&mut p, at("192.0.2.1:1"), 700).is_some());
    assert!(token(&mut p, at("192.0.2.2:1"), 700).is_some());
    assert_eq!(
        token(&mut p, at("192.0.2.3:1"), 700),
        None,
        "the third this second"
    );
    assert!(
        token(&mut p, at("192.0.2.3:1"), 701).is_some(),
        "the next second"
    );
}
