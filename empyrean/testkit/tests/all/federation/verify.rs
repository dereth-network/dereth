//! Divergence: V451
//! `/dn verify` in game: the hub challenges the world's game port while the verification waits,
//! and the world posts the challenge's nonce back from whatever address its calls leave by; a
//! challenge with no verification waiting is ignored.
//! Fixture: virtual-time worlds (`super::Net`); for the end to end, a real hub on loopback and a
//! socket standing in for the world's game port, whose datagrams go to the world's transport as
//! the UDP driver hands them over.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

use dereth_network_hub::registry::{Registration, Registry};
use dereth_network_hub::server::{start_with, HubConfig};
use dereth_network_protocol::challenge::{nonce_hex, Challenge, MAX_ANSWERS, NONCE_LEN};
use empyrean_net::PortKind;
use serde_json::{json, Value};

use super::{seed, Net};

/// A POST to the hub, as a world's call reaching it through its trusted proxy from `from`: the
/// status and the body.
fn post(hub: SocketAddr, path: &str, key: &str, from: &str, body: &Value) -> (u16, Value) {
    let body = body.to_string();
    let mut s = TcpStream::connect(hub).expect("the hub");
    write!(
        s,
        "POST {path} HTTP/1.1\r\nHost: hub.test\r\nAuthorization: Bearer {key}\r\n\
         Content-Type: application/json\r\nX-Forwarded-For: {from}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("sent");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("an answer");
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// A challenge datagram for `slug` carrying `nonce`.
fn challenge(slug: &str, nonce: u8) -> Vec<u8> {
    Challenge {
        slug: slug.to_owned(),
        nonce: [nonce; NONCE_LEN],
    }
    .encode()
    .expect("a slug it carries")
}

/// The datagram reaches world `slug`'s game port, as the UDP driver hands it over.
fn heard(net: &mut Net, slug: &str, datagram: &[u8]) {
    let from: SocketAddr = "198.51.100.7:40000".parse().expect("an address");
    let w = &mut net.world(slug).ts.world;
    let now = w.now;
    w.net.on_datagram(PortKind::C2S, from, datagram, now);
}

fn answers(net: &mut Net, slug: &str) -> Vec<String> {
    empyrean_world::federation::take_challenge_answers(&mut net.world(slug).ts.world)
        .into_iter()
        .map(|a| a.nonce)
        .collect()
}

/// Divergence: V451
#[test]
fn a_world_verifies_in_game_by_answering_a_real_hubs_challenge_from_another_address() {
    let game_port = UdpSocket::bind("127.0.0.1:0").expect("a game port");
    game_port.set_nonblocking(true).expect("nonblocking");
    let port = game_port.local_addr().expect("an address").port();
    // A hub behind a proxy it trusts, so each call says which address it came from.
    let registry = Registry::open(
        ":memory:",
        "dereth.network",
        Box::new(std::time::SystemTime::now),
    )
    .expect("a registry");
    let running = start_with(
        HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: ":memory:".into(),
            loopback_allowed: true,
            trusted_proxies: vec!["127.0.0.1".parse().expect("a proxy")],
            challenge_every: Duration::from_millis(100),
            challenge_wait: Duration::from_secs(5),
            ..HubConfig::default()
        },
        registry,
    )
    .expect("the hub starts");
    let hub = running.local_addr();
    let op = running
        .registry()
        .signed_in("700", "Operator")
        .expect("an operator");
    let world = Registration::from_json(&json!({
        "slug": "aa", "name": "World aa", "ruleset": "PvE",
        "endpoint": { "mode": "manual", "address": "127.0.0.1", "port": port },
        "emulator": { "family": "Empyrean" },
    }))
    .expect("a registration");
    let (_, key, issued) = running
        .registry()
        .register_for(&op, &world)
        .expect("registered");

    let mut boss = seed("boss", 0x5000_0002, "Boss");
    boss.access = empyrean_entity::enums::AccessLevel::Admin;
    let mut net = Net::new(&[("aa", &[boss])]);
    net.enter("aa", boss);
    net.say("aa", "Boss", &format!("@dn verify {}", issued.token));
    let sent = empyrean_world::federation::take_verify_requests(&mut net.world("aa").ts.world);
    assert_eq!(sent.len(), 1);

    // The host's call leaves by one address, and the world's game port answers at another.
    let (status, body) = std::thread::scope(|s| {
        let call = s.spawn(|| {
            post(
                hub,
                "/v1/servers/aa/verify",
                &key,
                "198.51.100.20",
                &json!({ "token": sent[0].token, "by": sent[0].by }),
            )
        });
        let until = Instant::now() + Duration::from_secs(5);
        let mut buf = [0u8; 2048];
        let mut posted = Vec::new();
        while !call.is_finished() && Instant::now() < until {
            while let Ok((n, _)) = game_port.recv_from(&mut buf) {
                heard(&mut net, "aa", &buf[..n]);
            }
            net.advance(0.05);
            for nonce in answers(&mut net, "aa") {
                let (status, _) = post(
                    hub,
                    "/v1/servers/aa/verify/answer",
                    &key,
                    "203.0.113.9",
                    &json!({ "nonce": nonce }),
                );
                posted.push(status);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(posted, [200], "the nonce went back once");
        call.join().expect("the call")
    });
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        (body["verified"].clone(), body["approval"].clone()),
        (json!(true), json!("approved"))
    );
    let audit = running.registry().audit("aa").expect("the log");
    let verified = audit
        .as_array()
        .expect("events")
        .iter()
        .find(|e| e["kind"] == "verified")
        .cloned()
        .expect("verified");
    assert_eq!(
        (
            verified["details"]["method"].clone(),
            verified["details"]["address"].clone()
        ),
        (json!("game"), json!("127.0.0.1"))
    );
    running.shutdown();
}

/// Divergence: V451
#[test]
fn a_hub_challenge_is_answered_only_while_dn_verify_waits_once_for_each_nonce() {
    let mut boss = seed("boss", 0x5000_0002, "Boss");
    boss.access = empyrean_entity::enums::AccessLevel::Admin;
    let mut net = Net::new(&[("aa", &[boss])]);
    net.enter("aa", boss);

    heard(&mut net, "aa", &challenge("aa", 1));
    net.advance(0.1);
    assert!(answers(&mut net, "aa").is_empty(), "no verification waits");

    net.say("aa", "Boss", &format!("@dn verify dnv_{}", "Q".repeat(32)));
    heard(&mut net, "aa", &challenge("bb", 2));
    heard(&mut net, "aa", &challenge("aa", 3));
    heard(&mut net, "aa", &challenge("aa", 3));
    net.advance(0.1);
    assert_eq!(
        answers(&mut net, "aa"),
        [nonce_hex(&[3; NONCE_LEN])],
        "its own slug's, once"
    );
    for n in 10..30 {
        heard(&mut net, "aa", &challenge("aa", n));
        net.advance(0.05);
    }
    assert_eq!(
        answers(&mut net, "aa").len(),
        MAX_ANSWERS - 1,
        "no more nonces than its cap"
    );

    // The hub answered: what comes after is ignored.
    empyrean_world::federation::on_verify_answer(
        &mut net.world("aa").ts.world,
        None,
        "The Dereth Network verified this world.",
    );
    heard(&mut net, "aa", &challenge("aa", 40));
    net.advance(0.1);
    assert!(
        answers(&mut net, "aa").is_empty(),
        "the verification is over"
    );

    // So is one heard a minute after `/dn verify` was typed, however long the hub takes.
    net.say("aa", "Boss", &format!("@dn verify dnv_{}", "Q".repeat(32)));
    net.advance(61.0);
    heard(&mut net, "aa", &challenge("aa", 41));
    net.advance(0.1);
    assert!(answers(&mut net, "aa").is_empty(), "past the window");
}

/// Divergence: V451
#[test]
fn a_world_with_the_network_off_drops_the_hubs_challenge() {
    let mut ts = empyrean_testkit::TestServer::new();
    assert!(ts.world.net.hub_challenges.is_none());
    let now = ts.world.now;
    let from: SocketAddr = "198.51.100.7:40000".parse().expect("an address");
    ts.world
        .net
        .on_datagram(PortKind::C2S, from, &challenge("aa", 1), now);
    assert!(ts.world.net.hub_challenges.is_none());
    ts.advance(0.1);
    assert!(empyrean_world::federation::take_challenge_answers(&mut ts.world).is_empty());
}
