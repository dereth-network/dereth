//! Verification in game: the challenge the hub sends a world's game port, the answer the world
//! posts back from any address, what the hub says when none comes, the bounds on what one attempt
//! sends and where it may send it, and the hub's operator vouching for a world instead.
//! Fixture: the operator tests' hub (loopback, an in-memory registry, a stand-in for Discord), a
//! stand-in for the system's resolver where a test names hosts, and the socket on loopback that
//! stands in for the worlds' game port.

use std::sync::Arc;
use std::time::Duration;

use dereth_network_hub::challenge::{COPIES, MAX_ADDRESSES};
use dereth_network_hub::registry::Registration;
use dereth_network_hub::resolver::Lookup;
use dereth_network_protocol::challenge::nonce_hex;
use serde_json::{json, Value};

use super::operators::{
    answer_challenge, call, federation, game_port, game_verify, hub, hub_with, op, register,
    sign_in, verify_answering, world, Answering, Fixture,
};

/// A stand-in for the system's resolver that knows `names` and nothing else.
fn names(names: &'static [(&'static str, &'static str)]) -> Lookup {
    Lookup(Arc::new(move |name| {
        Ok(names
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, a)| a.parse().expect("an address"))
            .collect())
    }))
}

fn silent() -> Answering {
    Answering {
        silent: true,
        ..Answering::default()
    }
}

/// The world's events, newest first, as its owner of `session` reads them.
fn audit(f: &Fixture, session: &str, slug: &str) -> Vec<Value> {
    op(
        f,
        session,
        "GET",
        &format!("/v1/operator/worlds/{slug}"),
        &Value::Null,
    )
    .json()["audit"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A world the hub's operator registered at `address` and handed to the operator of Discord id
/// `id`, with a token that operator asked for: its key and token.
fn handed(f: &Fixture, session: &str, id: &str, slug: &str, address: &str) -> (String, String) {
    let (_, key) = f
        .running
        .registry()
        .register(&Registration::from_json(&world(slug, address)).expect("valid"))
        .expect("registered");
    f.running.registry().assign(slug, id).expect("assigned");
    let v = op(
        f,
        session,
        "POST",
        &format!("/v1/operator/worlds/{slug}/verification"),
        &Value::Null,
    );
    let token = v.json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    (key, token)
}

/// Spec: hub: the hub challenges the world at its endpoint's game port, a datagram carrying the
/// world's slug and a nonce; the world posting that nonce back with its key verifies it.
#[test]
fn the_challenge_reaches_the_endpoints_game_port_and_its_nonce_verifies_the_world() {
    let f = hub();
    let ann = sign_in(&f, "3001");
    let (key, token) = register(&f, &ann, "reach", "127.0.0.1");
    let (a, heard) = verify_answering(&f, "reach", &key, &token, &[], &Answering::default());
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(a.json()["approval"], "approved");
    let (from, c) = heard.challenges.first().expect("a challenge");
    assert_eq!(c.slug, "reach");
    assert!(from.ip().is_loopback(), "from the hub: {from}");
    assert_eq!(heard.answers, [200], "{heard:?}");
    assert_eq!(federation(&f, "reach"), "on");
    f.running.shutdown();
}

/// Spec: hub: a challenge nobody answers fails the verification `no_answer`, naming the host, the
/// addresses it was sent to and the port.
#[test]
fn a_challenge_nobody_answers_is_no_answer_naming_the_host_its_addresses_and_the_port() {
    const NAMES: &[(&str, &str)] = &[("game.example", "127.0.0.1")];
    let f = hub_with(|c| c.lookup = names(NAMES));
    let ann = sign_in(&f, "3011");
    let port = game_port();
    let (key, token) = register(&f, &ann, "quiet", "game.example");
    let (a, heard) = verify_answering(&f, "quiet", &key, &token, &[], &silent());
    assert_eq!(
        (a.status, a.reason()),
        (422, "no_answer".to_owned()),
        "{a:?}"
    );
    assert_eq!(
        a.json()["error"]["message"],
        format!(
            "The hub's challenge to game.example:{port} (127.0.0.1) got no answer: is the game port reachable from the internet?"
        )
    );
    assert_eq!(heard.challenges.len(), COPIES, "each copy: {heard:?}");
    assert!(
        heard
            .challenges
            .iter()
            .all(|(_, c)| c.nonce == heard.challenges[0].1.nonce),
        "one address, one nonce"
    );
    let failed = audit(&f, &ann, "quiet");
    assert!(
        failed
            .iter()
            .any(|e| e["kind"] == "verification-failed" && e["details"]["reason"] == "no_answer"),
        "{failed:?}"
    );

    // An endpoint given as an address is named as one.
    let (key, token) = register(&f, &ann, "quieter", "127.0.0.1");
    let (a, _) = verify_answering(&f, "quieter", &key, &token, &[], &silent());
    assert_eq!(
        a.json()["error"]["message"],
        format!(
            "The hub's challenge to 127.0.0.1:{port} got no answer: is the game port reachable from the internet?"
        )
    );
    f.running.shutdown();
}

/// Spec: hub: only the nonce of the challenge waiting for the world, posted with the key its call
/// came with, answers it: a wrong nonce, an old one and another world's key verify nothing.
#[test]
fn a_wrong_or_old_nonce_is_refused_and_verifies_nothing() {
    let f = hub();
    let ann = sign_in(&f, "3021");
    let (key, token) = register(&f, &ann, "nonce", "127.0.0.1");
    let (other_key, _) = register(&f, &ann, "other", "127.0.0.1");
    let wrong = Answering {
        nonce: Some("00".repeat(16)),
        ..Answering::default()
    };
    let (a, heard) = verify_answering(&f, "nonce", &key, &token, &[], &wrong);
    assert_eq!(heard.answers, [422], "a nonce it never sent: {heard:?}");
    assert_eq!(a.reason(), "no_answer");
    let old = nonce_hex(&heard.challenges[0].1.nonce);
    let late = answer_challenge(f.addr(), "nonce", &key, &old, &[]);
    assert_eq!(
        (late.status, late.reason()),
        (422, "wrong_nonce".to_owned()),
        "an old nonce, once its call is over: {late:?}"
    );
    let not_hex = answer_challenge(f.addr(), "nonce", &key, "not a nonce", &[]);
    assert_eq!(not_hex.reason(), "wrong_nonce");
    let theirs = answer_challenge(f.addr(), "nonce", &other_key, &old, &[]);
    assert_eq!(theirs.status, 401, "another world's key");
    let none = call(
        f.addr(),
        "POST",
        "/v1/servers/nonce/verify/answer",
        &[("Content-Type", "application/json")],
        &json!({ "nonce": old }).to_string(),
    );
    assert_eq!(none.status, 401, "no key");
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/nonce", &Value::Null).json();
    assert_eq!(w["world"]["ownership"]["state"], "pending");
    // The right nonce, on the next attempt, verifies it.
    assert_eq!(game_verify(&f, "nonce", &key, &token, &[]).status, 200);
    f.running.shutdown();
}

/// Spec: hub: one attempt sends each address its copies and no more, to at most four addresses.
#[test]
fn one_attempt_sends_at_most_its_copies_to_at_most_four_addresses() {
    const NAMES: &[(&str, &str)] = &[
        ("many.example", "127.0.0.1"),
        ("many.example", "127.0.0.2"),
        ("many.example", "127.0.0.3"),
        ("many.example", "127.0.0.4"),
        ("many.example", "127.0.0.5"),
        ("many.example", "127.0.0.6"),
    ];
    let f = hub_with(|c| c.lookup = names(NAMES));
    let ann = sign_in(&f, "3031");
    let (key, token) = register(&f, &ann, "many", "many.example");
    let (a, heard) = verify_answering(&f, "many", &key, &token, &[], &silent());
    assert_eq!(a.reason(), "no_answer");
    assert_eq!(MAX_ADDRESSES, 4);
    assert_eq!(
        a.json()["error"]["message"],
        format!(
            "The hub's challenge to many.example:{} (127.0.0.1, 127.0.0.2, 127.0.0.3, 127.0.0.4) got no answer: is the game port reachable from the internet?",
            game_port()
        ),
        "the first four"
    );
    assert_eq!(
        heard.challenges.len(),
        COPIES,
        "the copies to the one address listening, and no more: {heard:?}"
    );
    f.running.shutdown();
}

/// Spec: hub: verification attempts in game are held to their limit, and an attempt past it sends
/// nothing.
#[test]
fn an_attempt_past_the_limit_sends_no_challenge() {
    let f = hub_with(|c| c.challenge_wait = Duration::from_millis(250));
    let ann = sign_in(&f, "3041");
    let (key, token) = register(&f, &ann, "limited", "127.0.0.1");
    for i in 0..10 {
        let (a, heard) = verify_answering(&f, "limited", &key, &token, &[], &silent());
        assert_eq!(a.reason(), "no_answer", "attempt {i}");
        assert_eq!(heard.challenges.len(), COPIES, "attempt {i}");
    }
    let (a, heard) = verify_answering(&f, "limited", &key, &token, &[], &Answering::default());
    assert_eq!(a.status, 429, "the eleventh this hour: {a:?}");
    assert!(heard.challenges.is_empty(), "{heard:?}");
    f.running.shutdown();
}

/// Spec: hub: a challenge is sent only to an address on the public internet, outside every range
/// the hub's operator denies; with none, the verification fails `address_not_public` naming them.
#[test]
fn an_address_off_the_public_internet_is_never_challenged() {
    const NAMES: &[(&str, &str)] = &[("lan.example", "10.0.0.5"), ("lan.example", "127.0.0.1")];
    let f = hub_with(|c| {
        c.loopback_allowed = false;
        c.lookup = names(NAMES);
    });
    let ann = sign_in(&f, "3051");
    for (slug, address, message) in [
        (
            "lanname",
            "lan.example",
            "lan.example resolves to no address the hub may reach on the public internet (10.0.0.5, 127.0.0.1)",
        ),
        (
            "lanaddr",
            "127.0.0.1",
            "127.0.0.1 is not an address the hub may reach on the public internet",
        ),
    ] {
        let (key, token) = handed(&f, &ann, "3051", slug, address);
        let (a, heard) = verify_answering(&f, slug, &key, &token, &[], &silent());
        assert_eq!(
            (a.status, a.reason()),
            (422, "address_not_public".to_owned()),
            "{a:?}"
        );
        assert_eq!(a.json()["error"]["message"], message);
        assert!(heard.challenges.is_empty(), "{slug}: {heard:?}");
    }
    f.running.shutdown();

    // A range the hub's operator denies is never challenged, whatever else allows it.
    let f = hub_with(|c| c.deny_fetch = vec!["127.0.0.0/8".parse().expect("a range")]);
    let ann = sign_in(&f, "3052");
    let (key, token) = register(&f, &ann, "denied", "127.0.0.1");
    let (a, heard) = verify_answering(&f, "denied", &key, &token, &[], &silent());
    assert_eq!(a.reason(), "address_not_public", "{a:?}");
    assert!(heard.challenges.is_empty(), "{heard:?}");
    f.running.shutdown();
}

/// Spec: hub: while a world's challenge waits for its answer, a second verification of it is
/// refused.
#[test]
fn a_second_verification_while_one_waits_is_refused() {
    let f = hub_with(|c| c.challenge_wait = Duration::from_millis(1_500));
    let ann = sign_in(&f, "3061");
    let (key, token) = register(&f, &ann, "twice", "127.0.0.1");
    std::thread::scope(|s| {
        let first = s.spawn(|| verify_answering(&f, "twice", &key, &token, &[], &silent()).0);
        std::thread::sleep(Duration::from_millis(400));
        let second = call(
            f.addr(),
            "POST",
            "/v1/servers/twice/verify",
            &[
                ("Authorization", &format!("Bearer {key}")),
                ("Content-Type", "application/json"),
            ],
            &json!({ "token": token }).to_string(),
        );
        assert_eq!(
            (second.status, second.reason()),
            (409, "in_progress".to_owned()),
            "{second:?}"
        );
        assert_eq!(first.join().expect("the first").reason(), "no_answer");
    });
    f.running.shutdown();
}

/// Spec: hub: the hub's operator vouches for a registered world on the command line: it is
/// verified by the method `admin`, its owner's token is spent, approval is asked as a proof asks
/// it, and its log keeps the evidence under the hub's operator, naming no operator.
#[test]
fn the_hubs_operator_vouches_for_a_world_as_a_proof_would_verify_it() {
    let f = hub();
    let ann = sign_in(&f, "3071");
    let (key, token) = register(&f, &ann, "vouched", "127.0.0.1");
    let evidence = "Saw the world's console with its operator on a call.";
    let approval = f
        .running
        .registry()
        .vouch("vouched", evidence)
        .expect("vouched");
    assert_eq!(approval.map(|a| a.word()), Some("approved"));
    assert_eq!(federation(&f, "vouched"), "on");
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/vouched", &Value::Null).json();
    assert_eq!(w["world"]["ownership"]["state"], "managed");
    let verification = &w["world"]["operator"]["verification"];
    assert_eq!(
        (
            verification["method"].clone(),
            verification["endpointVerified"].clone(),
            verification["open"].clone()
        ),
        (json!("admin"), json!(true), json!(false)),
        "{w}"
    );
    let verified = w["audit"]
        .as_array()
        .expect("the log")
        .iter()
        .find(|e| e["kind"] == "verified")
        .cloned()
        .expect("a verified event");
    assert_eq!(verified["actor"], "hub-operator");
    assert_eq!(
        verified["details"],
        json!({ "method": "admin", "evidence": evidence })
    );
    let spent = game_verify(&f, "vouched", &key, &token, &[]);
    assert_eq!(spent.reason(), "no_verification", "the token is spent");
    f.running.shutdown();
}

/// Spec: hub: the hub's operator's vouching refuses what a claim refuses: evidence too short or
/// holding hidden characters, a world that is not there, a locked one, and a community listing,
/// which is claimed instead.
#[test]
fn vouching_refuses_what_a_claim_refuses() {
    let f = hub();
    let ann = sign_in(&f, "3081");
    register(&f, &ann, "short", "127.0.0.1");
    let fine = "Reviewed on a call with its operator.";
    let refused = |slug: &str, evidence: &str| {
        f.running
            .registry()
            .vouch(slug, evidence)
            .expect_err("refused")
            .status
    };
    assert_eq!(refused("short", "too short"), 400);
    assert_eq!(refused("short", "Reviewed\u{202e} on a call."), 400);
    assert_eq!(refused("nowhere", fine), 404);
    f.running.registry().remove("short").expect("removed");
    assert_eq!(refused("short", fine), 404, "a removed world");

    let entry = dereth_network_hub::registry::CommunityEntry {
        external_id: "00000000-0000-4000-8000-000000003081".into(),
        registration: Registration::from_json(&world("listed", "127.0.0.1")).expect("valid"),
        source_hash: "h".into(),
        legacy_type: "PvE".into(),
    };
    f.running
        .registry()
        .import_community(&[entry], true)
        .expect("imported");
    assert_eq!(refused("listed", fine), 409, "a listing is claimed");

    let (key, token) = register(&f, &ann, "rogue", "127.0.0.1");
    assert_eq!(game_verify(&f, "rogue", &key, &token, &[]).status, 200);
    let bob = sign_in(&f, "3082");
    let report = op(
        &f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": "rogue", "reason": "impersonation", "details": "Not theirs." }),
    );
    let id = report.json()["report"]["id"]
        .as_str()
        .expect("a report")
        .to_owned();
    f.running
        .registry()
        .resolve_report(&id, "revoke", "Impersonation.", None)
        .expect("revoked");
    assert_eq!(refused("rogue", fine), 409, "a locked world");
    f.running.shutdown();
}
