//! The hub's operator API held against what an adversarial review found: a disabled operator's
//! worlds, claims one account cannot block, client addresses through proxies, allowances an
//! attacker cannot spend for everyone, what a verification may reach and say, text that cannot
//! break a listing, verification tied to the endpoint it checked, revoked and abandoned worlds,
//! slow requests, heads past the limits, and odd URLs.
//! Fixture: the operator tests' hub (loopback, an in-memory registry on a clock the test turns, a
//! stand-in for Discord), with its worlds linked through the conformance suite's links.

use std::time::Duration;

use dereth_network_conformance::hub::Link;
use dereth_network_hub::registry::{CommunityEntry, Registration};
use serde_json::{json, Value};

use super::operators::{
    call, federation, game_verify, hub, hub_with, op, register, sign_in, world, Answer, Fixture,
    SITE,
};

/// A community listing, imported: its endpoint `address`.
pub(super) fn listing(f: &Fixture, slug: &str, address: &str) {
    let entry = CommunityEntry {
        external_id: format!("00000000-0000-4000-8000-{:012}", slug.len() * 7919),
        registration: Registration::from_json(&world(slug, address)).expect("a registration"),
        source_hash: "h".into(),
        legacy_type: "PvE".into(),
    };
    f.running
        .registry()
        .import_community(&[entry], true)
        .expect("imported");
}

/// `session`'s claim of `slug`: its key and token.
fn claim(f: &Fixture, session: &str, slug: &str) -> (String, String) {
    let c = op(
        f,
        session,
        "POST",
        &format!("/v1/operator/worlds/{slug}/claim"),
        &Value::Null,
    );
    assert_eq!(c.status, 201, "{c:?}");
    let v = c.json();
    (
        v["key"].as_str().expect("a key").to_owned(),
        v["verification"]["token"]
            .as_str()
            .expect("a token")
            .to_owned(),
    )
}

fn status(a: &Answer) -> u16 {
    a.status
}

/// Spec: hub: disabling an operator closes what it opened, takes its worlds off the network and
/// stops their keys and links until it is enabled again.
#[test]
fn a_disabled_operators_worlds_leave_the_network_and_their_keys_stop() {
    let f = hub();
    let ann = sign_in(&f, "1101");
    let (key, token) = register(&f, &ann, "dis", "127.0.0.1");
    assert_eq!(game_verify(&f, "dis", &key, &token, &[]).status, 200);
    assert_eq!(federation(&f, "dis"), "on");
    let (pending_key, pending_token) = register(&f, &ann, "dis-two", "127.0.0.1");
    let mut link = Link::open(&format!("ws://{}", f.addr()), "dis", &key).expect("links");
    link.greet().expect("welcomed");
    let beat = |key: &str| {
        call(
            f.addr(),
            "POST",
            "/v1/servers/dis/heartbeat",
            &[
                ("Authorization", &format!("Bearer {key}")),
                ("Content-Type", "application/json"),
            ],
            r#"{"state":"online","playersOnline":1}"#,
        )
        .status
    };
    assert_eq!(beat(&key), 200);

    f.running
        .registry()
        .set_operator_disabled("1101", true)
        .expect("disabled");
    assert_eq!(federation(&f, "dis"), "off", "its world leaves the network");
    assert!(link.closes_within(Duration::from_secs(5)), "and its link");
    assert_eq!(beat(&key), 401, "its key stops");
    let late = game_verify(&f, "dis-two", &pending_key, &pending_token, &[]);
    assert_eq!(
        (late.status, late.reason()),
        (422, "no_verification".to_owned()),
        "what it opened is closed"
    );
    assert_eq!(federation(&f, "dis-two"), "unset");

    f.running
        .registry()
        .set_operator_disabled("1101", false)
        .expect("enabled");
    assert_eq!(beat(&key), 200, "the key works again");
    assert_eq!(
        federation(&f, "dis"),
        "off",
        "the network stays off until it is turned on"
    );
    let on = op(
        &f,
        &sign_in(&f, "1101"),
        "PUT",
        "/v1/operator/worlds/dis/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(on.status, 200, "{on:?}");
    f.running.shutdown();
}

/// Spec: hub: one claimant's verification attempts and new tokens are its own, so another account
/// cannot spend them; and an operator opens only so many claims a day.
#[test]
fn one_claimant_cannot_spend_anothers_attempts_or_tokens() {
    let f = hub();
    listing(&f, "listed", "127.0.0.1");
    let mallory = sign_in(&f, "1201");
    let real = sign_in(&f, "1202");
    let (mkey, _) = claim(&f, &mallory, "listed");
    for _ in 0..12 {
        let _ = game_verify(&f, "listed", &mkey, &format!("dnv_{}", "A".repeat(32)), &[]);
        let _ = op(
            &f,
            &mallory,
            "POST",
            "/v1/operator/worlds/listed/verification",
            &Value::Null,
        );
    }
    let (rkey, _) = claim(&f, &real, "listed");
    let fresh = op(
        &f,
        &real,
        "POST",
        "/v1/operator/worlds/listed/verification",
        &Value::Null,
    );
    assert_eq!(
        status(&fresh),
        200,
        "the real claimant still gets a token: {fresh:?}"
    );
    let token = fresh.json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let verified = game_verify(&f, "listed", &rkey, &token, &[]);
    assert_eq!(status(&verified), 200, "and verifies: {verified:?}");

    // Claiming and withdrawing again and again is held to a day's few.
    listing(&f, "another", "127.0.0.1");
    let mut refused = None;
    for i in 0..8 {
        let c = op(
            &f,
            &mallory,
            "POST",
            "/v1/operator/worlds/another/claim",
            &Value::Null,
        );
        if c.status == 429 {
            refused = Some(i);
            break;
        }
        assert_eq!(c.status, 201, "{c:?}");
        let w = op(
            &f,
            &mallory,
            "DELETE",
            "/v1/operator/worlds/another",
            &Value::Null,
        );
        assert_eq!(w.status, 204);
    }
    assert_eq!(
        refused,
        Some(4),
        "a fifth claim today, after one on the first listing, is refused"
    );
    f.running.shutdown();
}

/// Spec: hub: behind a trusted proxy, a forwarded header that does not read is refused, a request
/// and a link attempt alike, never taken to come from the proxy.
#[test]
fn an_unreadable_forwarded_header_is_refused_not_taken_for_the_proxy() {
    let f = hub_with(|c| {
        c.trusted_proxies = vec!["127.0.0.1".parse().expect("a range")];
        c.proxy_hops = 2;
    });
    for header in ["not-an-address", "203.0.113.5, ", ""] {
        let r = call(
            f.addr(),
            "GET",
            "/v1/servers",
            &[("X-Forwarded-For", header)],
            "",
        );
        assert_eq!(r.status, 400, "{header:?}: {r:?}");
    }
    let fine = call(
        f.addr(),
        "GET",
        "/v1/servers",
        &[("X-Forwarded-For", "198.51.100.1, 127.0.0.1")],
        "",
    );
    assert_eq!(fine.status, 200);
    let link = call(
        f.addr(),
        "GET",
        "/v1/servers/aa/link",
        &[
            ("Upgrade", "websocket"),
            ("Connection", "Upgrade"),
            ("X-Forwarded-For", "garbage"),
        ],
        "",
    );
    assert_eq!(link.status, 400, "{link:?}");
    f.running.shutdown();
}

/// Spec: hub: the address an observed world's heartbeats or verification come from becomes its
/// address only when it is a public one.
#[test]
fn an_observed_address_is_a_public_one_only() {
    let f = hub_with(|c| c.loopback_allowed = false);
    let observed = |slug: &str| {
        json!({
            "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
            "endpoint": { "mode": "observed", "port": 9000 },
            "emulator": { "family": "ACE" },
        })
    };
    let (_, key) = f
        .running
        .registry()
        .register(&Registration::from_json(&observed("seen")).expect("a registration"))
        .expect("registered");
    let beat = call(
        f.addr(),
        "POST",
        "/v1/servers/seen/heartbeat",
        &[
            ("Authorization", &format!("Bearer {key}")),
            ("Content-Type", "application/json"),
        ],
        r#"{"state":"online","playersOnline":1}"#,
    );
    assert_eq!(beat.status, 400, "a heartbeat from loopback: {beat:?}");
    let ann = sign_in(&f, "1301");
    let r = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds",
        &observed("seentoo"),
    );
    assert_eq!(r.status, 201, "{r:?}");
    let v = r.json();
    let verified = game_verify(
        &f,
        "seentoo",
        v["key"].as_str().expect("a key"),
        v["verification"]["token"].as_str().expect("a token"),
        &[],
    );
    assert_eq!(
        (verified.status, verified.reason()),
        (422, "address_not_public".to_owned()),
        "{verified:?}"
    );
    f.running.shutdown();
}

/// Spec: hub: a page that is not on the world's host is refused before anything is counted, so it
/// spends neither the operator's page checks nor the world's attempts.
#[test]
fn a_page_off_the_worlds_host_spends_no_allowance() {
    let f = hub();
    let ann = sign_in(&f, "1401");
    let (_, token) = register(&f, &ann, "paged", "127.0.0.1");
    let held = std::sync::Arc::new(std::sync::Mutex::new(token));
    let pages = super::operators::page_server(std::sync::Arc::clone(&held));
    for _ in 0..40 {
        let r = super::operators::verify_page(&f, &ann, "paged", "http://example.invalid/token");
        assert_eq!(
            (r.status, r.reason()),
            (422, "host_mismatch".to_owned()),
            "{r:?}"
        );
    }
    let ok = super::operators::verify_page(
        &f,
        &ann,
        "paged",
        &format!("http://127.0.0.1:{}/token", pages.addr.port()),
    );
    assert_eq!(ok.status, 200, "the real page still verifies: {ok:?}");
    f.running.shutdown();
}

/// Spec: hub: the hub's ceiling on approvals is above what operators' own allowances reach
/// together, so a handful of accounts cannot spend it for everyone.
#[test]
fn the_hubs_approval_ceiling_never_holds_back_an_operator_within_its_own() {
    let f = hub();
    for i in 0..7 {
        let session = sign_in(&f, &format!("15{i:02}"));
        for j in 0..3 {
            let slug = format!("ap{i}{j}");
            let (key, token) = register(&f, &session, &slug, "127.0.0.1");
            let v = game_verify(&f, &slug, &key, &token, &[]);
            assert_eq!(v.json()["approval"], "approved", "{slug}: {v:?}");
        }
    }
    f.running.shutdown();
}

/// A Discord account's id, made `days` ago.
pub(super) fn snowflake(days: u64) -> String {
    let made = std::time::SystemTime::now() - Duration::from_secs(days * 86_400);
    let ms = made
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis();
    let since_2015 = u64::try_from(ms).expect("a time") - 1_420_070_400_000;
    (since_2015 << 22).to_string()
}

/// Spec: hub: a Discord account younger than the minimum age (30 days unless the hub's setting
/// says otherwise) may not register or claim a world.
#[test]
fn a_new_discord_account_may_not_register_or_claim_a_world() {
    let f = hub();
    listing(&f, "listed", "127.0.0.1");
    let young = sign_in(&f, &snowflake(10));
    let r = op(
        &f,
        &young,
        "POST",
        "/v1/operator/worlds",
        &world("young", "127.0.0.1"),
    );
    assert_eq!(
        (r.status, r.reason()),
        (403, "account_too_new".to_owned()),
        "{r:?}"
    );
    let c = op(
        &f,
        &young,
        "POST",
        "/v1/operator/worlds/listed/claim",
        &Value::Null,
    );
    assert_eq!(
        (c.status, c.reason()),
        (403, "account_too_new".to_owned()),
        "{c:?}"
    );
    let old = sign_in(&f, &snowflake(40));
    let r = op(
        &f,
        &old,
        "POST",
        "/v1/operator/worlds",
        &world("older", "127.0.0.1"),
    );
    assert_eq!(r.status, 201, "{r:?}");
    f.running.shutdown();
}

/// Spec: hub: the per-address allowance counts an IPv6 client by its /64, as connections are
/// counted, so one host cannot spread its requests over the addresses it is given.
#[test]
fn the_request_allowance_counts_an_ipv6_client_by_its_64() {
    let f = hub_with(|c| {
        c.trusted_proxies = vec!["127.0.0.1".parse().expect("a range")];
        c.http_per_minute = 10;
    });
    let mut limited = 0;
    for i in 0..30 {
        let r = call(
            f.addr(),
            "GET",
            "/v1/status",
            &[("X-Forwarded-For", &format!("2001:db8:7:7::{:x}", i + 1))],
            "",
        );
        if r.status == 429 {
            limited += 1;
        }
    }
    assert!(
        limited >= 15,
        "one /64 shares one allowance: {limited} refused"
    );
    let other = call(
        f.addr(),
        "GET",
        "/v1/status",
        &[("X-Forwarded-For", "2001:db8:7:8::1")],
        "",
    );
    assert_eq!(other.status, 200, "another /64 has its own");
    f.running.shutdown();
}

/// Spec: hub: the public lists are served from a snapshot at most a second old, never waiting on
/// the registry the links need, and a cache in front of the hub may keep them ten seconds.
#[test]
fn the_public_lists_never_wait_on_the_registry() {
    let f = hub();
    let ann = sign_in(&f, "1601");
    let (key, token) = register(&f, &ann, "listy", "127.0.0.1");
    assert_eq!(game_verify(&f, "listy", &key, &token, &[]).status, 200);
    let first = call(f.addr(), "GET", "/v1/servers", &[], "");
    assert_eq!(first.status, 200);
    assert_eq!(first.header("cache-control"), Some("public, max-age=10"));
    assert_eq!(
        call(f.addr(), "GET", "/v1/operator", &[], "").header("cache-control"),
        Some("no-store"),
        "nothing else may be kept"
    );
    // Something holds the registry for longer than the snapshot lasts.
    let held = f.running.registry();
    std::thread::sleep(Duration::from_millis(1_200));
    let addr = f.addr();
    let reader = std::thread::spawn(move || {
        [
            "/v1/servers",
            "/v1/status",
            "/v1/network",
            "/compat/thwarg/Servers.xml",
            "/compat/treestats/player_counts-latest.json",
        ]
        .map(|p| {
            let r = call(addr, "GET", p, &[], "");
            (
                r.status,
                r.body.contains("listy") || p.ends_with("status") || p.contains("treestats"),
            )
        })
    });
    let started = std::time::Instant::now();
    while !reader.is_finished() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let answered = reader.is_finished();
    drop(held);
    assert!(
        answered,
        "the public lists answered while the registry was held"
    );
    for (status, names) in reader.join().expect("the reader") {
        assert_eq!(status, 200);
        assert!(names, "from the last snapshot");
    }
    f.running.shutdown();
}

/// Spec: hub: a verification page is never read from a range the hub's operator denies (the hub's
/// own network), whatever else allows it, and the check says only that the page did not show the
/// token.
#[test]
fn a_page_in_a_denied_range_is_never_read() {
    let token = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let pages = super::operators::page_server(std::sync::Arc::clone(&token));
    let f = hub_with(|c| c.deny_fetch = vec!["127.0.0.0/8".parse().expect("a range")]);
    let ann = sign_in(&f, "1701");
    let (_, t) = register(&f, &ann, "denied", "127.0.0.1");
    *token.lock().expect("the token") = t;
    let before = pages.hits();
    let a = super::operators::verify_page(
        &f,
        &ann,
        "denied",
        &format!("http://127.0.0.1:{}/token", pages.addr.port()),
    );
    assert_eq!(
        (a.status, a.reason()),
        (422, "token_not_found".to_owned()),
        "{a:?}"
    );
    assert_eq!(
        pages.hits(),
        before,
        "nothing connected to the denied range"
    );
    f.running.shutdown();
}

/// The characters no text the hub keeps may hold: a control character, a right-to-left override,
/// a zero-width space, a byte-order mark used as a joiner, and a noncharacter XML forbids.
const HIDDEN: [char; 5] = ['\u{1}', '\u{202e}', '\u{200b}', '\u{feff}', '\u{fffe}'];

/// Spec: hub: no text a world or an operator sends (a registration's every field, a heartbeat's,
/// a report's details or an admin's note) holds a control, bidirectional or zero-width character;
/// a description keeps line feeds.
#[test]
fn no_field_a_world_or_operator_sends_holds_a_hidden_character() {
    let registration = |field: &str, value: &str| {
        let mut w = world("plain", "127.0.0.1");
        match field {
            "emulator.version" => w["emulator"]["version"] = json!(value),
            "endpoint.address" => w["endpoint"]["address"] = json!(value),
            _ => w[field] = json!(value),
        }
        Registration::from_json(&w)
    };
    for (field, plain) in [
        ("name", "Plain World"),
        ("description", "A world\nof two lines."),
        ("region", "North America"),
        ("maturity", "Live"),
        ("website", "https://plain.example/"),
        ("discord", "https://discord.gg/plain"),
        ("emulator.version", "1.2.3"),
    ] {
        assert!(registration(field, plain).is_ok(), "{field}: {plain:?}");
        for c in HIDDEN {
            let at = plain.len() / 2;
            let bad = format!("{}{c}{}", &plain[..at], &plain[at..]);
            let e = registration(field, &bad).expect_err(&bad);
            assert_eq!(e.status, 400, "{field}");
            assert!(e.message.starts_with(field), "{field}: {}", e.message);
        }
    }
    assert!(
        registration("description", "one\r\ntwo").is_err(),
        "a carriage return is not a line feed"
    );
    assert!(registration("endpoint.address", "plain\u{200b}.example").is_err());
    let beat = |v: Value| {
        serde_json::from_value::<dereth_network_hub::registry::Heartbeat>(v)
            .expect("a heartbeat")
            .check()
    };
    assert!(beat(
        json!({ "state": "online", "message": "Back soon.", "emulatorVersion": "1.2",
        "adapter": { "kind": "ace-mod", "version": "0.4" } })
    )
    .is_ok());
    for c in HIDDEN {
        for v in [
            json!({ "state": "online", "message": format!("Back{c} soon.") }),
            json!({ "state": "online", "emulatorVersion": format!("1.{c}2") }),
            json!({ "state": "online", "adapter": { "kind": "ace-mod", "version": format!("0.{c}4") } }),
        ] {
            assert_eq!(beat(v.clone()).map_err(|e| e.status), Err(400), "{v}");
        }
    }

    let f = hub();
    let ann = sign_in(&f, "1801");
    let (key, token) = register(&f, &ann, "texty", "127.0.0.1");
    assert_eq!(game_verify(&f, "texty", &key, &token, &[]).status, 200);
    let bob = sign_in(&f, "1802");
    let report = |details: &str| {
        op(
            &f,
            &bob,
            "POST",
            "/v1/reports",
            &json!({ "slug": "texty", "reason": "other", "details": details }),
        )
    };
    for c in HIDDEN {
        let a = report(&format!("Not{c} right."));
        assert_eq!(a.status, 400, "{a:?}");
    }
    let filed = report("Not right.\nTwice over.");
    assert_eq!(filed.status, 201, "{filed:?}");
    let id = filed.json()["report"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let admin = sign_in(&f, super::operators::ADMIN);
    let resolve = |note: &str| {
        op(
            &f,
            &admin,
            "POST",
            &format!("/v1/admin/reports/{id}"),
            &json!({ "action": "dismiss", "note": note }),
        )
    };
    assert_eq!(resolve("Fine\u{202e}.").status, 400);
    assert_eq!(resolve("Fine.").status, 200);
    listing(&f, "imported", "192.0.2.40");
    for c in HIDDEN {
        let e = f
            .running
            .registry()
            .claim("imported", &format!("Seen on{c} their site."))
            .expect_err("refused");
        assert_eq!(e.status, 400, "{}", e.message);
    }
    assert!(f
        .running
        .registry()
        .claim("imported", "Seen on their site.\nAnd in game.")
        .is_ok());
    f.running.shutdown();
}

/// Spec: hub: who verified a world in game, as the world names it, is kept without hidden
/// characters.
#[test]
fn a_worlds_note_of_who_verified_it_loses_its_hidden_characters() {
    let f = hub();
    let ann = sign_in(&f, "1811");
    let (key, token) = register(&f, &ann, "noted", "127.0.0.1");
    let by = super::operators::Answering {
        by: Some("Ash\u{202e}er\u{200b}on\u{1}".into()),
        ..super::operators::Answering::default()
    };
    let (a, _) = super::operators::verify_answering(&f, "noted", &key, &token, &[], &by);
    assert_eq!(a.status, 200, "{a:?}");
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/noted", &Value::Null).json();
    let verified = w["audit"]
        .as_array()
        .expect("the audit log")
        .iter()
        .find(|e| e["kind"] == "verified")
        .cloned()
        .expect("a verified event");
    assert_eq!(verified["details"]["by"], "Asheron");
    f.running.shutdown();
}

/// A stand-in for the system's resolver that takes `wait` for each name, finds `found` for it,
/// and counts its lookups.
fn lookup(
    wait: Duration,
    found: &str,
) -> (
    dereth_network_hub::resolver::Lookup,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&calls);
    let found: std::net::IpAddr = found.parse().expect("an address");
    let lookup = dereth_network_hub::resolver::Lookup(std::sync::Arc::new(move |_name| {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(wait);
        Ok(vec![found])
    }));
    (lookup, calls)
}

/// Spec: hub: a world's verification call looks nothing up until its key is the world's or an
/// open claim's.
#[test]
fn a_verification_call_without_a_valid_key_looks_nothing_up() {
    let (stand_in, calls) = lookup(Duration::ZERO, "127.0.0.1");
    let f = hub_with(move |c| c.lookup = stand_in);
    let ann = sign_in(&f, "1901");
    // The hub's operator registers it (nothing is looked up) and hands it to Ann, who asks for a
    // token to verify it in game.
    let (_, key) = f
        .running
        .registry()
        .register(&Registration::from_json(&world("named", "named.example")).expect("valid"))
        .expect("registered");
    f.running
        .registry()
        .assign("named", "1901")
        .expect("assigned");
    let v = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/named/verification",
        &Value::Null,
    );
    let token = v.json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let looked_up = || calls.load(std::sync::atomic::Ordering::SeqCst);
    let before = looked_up();
    assert_eq!(before, 0);
    let wrong = format!("drth_{}", "A".repeat(43));
    for slug in ["named", "nowhere"] {
        assert_eq!(
            status(&game_verify(&f, slug, &wrong, &token, &[])),
            401,
            "{slug}"
        );
    }
    assert_eq!(looked_up(), before, "nothing looked up without a key");
    let a = game_verify(&f, "named", &key, &token, &[]);
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(looked_up(), before + 1);
    f.running.shutdown();
}

/// Spec: hub: a slow name server holds a verification call no longer than its lookup deadline,
/// and `/v1/resolve` never waits on one: it answers what the hub knows, or that it does not know
/// yet.
#[test]
fn a_slow_name_server_holds_no_call_past_the_deadline() {
    let (stand_in, _) = lookup(Duration::from_secs(10), "127.0.0.1");
    let f = hub_with(move |c| {
        c.lookup = stand_in;
        c.resolve_deadline = Duration::from_millis(100);
        c.challenge_lookup_deadline = Duration::from_millis(100);
    });
    let ann = sign_in(&f, "1911");
    let (key, token) = register(&f, &ann, "slowdns", "slow.example");
    let started = std::time::Instant::now();
    let a = game_verify(&f, "slowdns", &key, &token, &[]);
    assert_eq!(
        (a.status, a.reason()),
        (422, "not_resolved".to_owned()),
        "{a:?}"
    );
    assert!(
        a.json()["error"]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("slow.example did not resolve in time")),
        "{a:?}"
    );
    // Well short of the 10 s lookup, with room for a slow machine.
    assert!(started.elapsed() < Duration::from_secs(5));
    f.running.shutdown();

    // A lookup well inside its deadline is still never waited on by /v1/resolve.
    let (stand_in, _) = lookup(Duration::from_secs(3), "1.1.1.1");
    let f = hub_with(move |c| {
        c.lookup = stand_in;
        c.resolve_deadline = Duration::from_secs(10);
    });
    f.running
        .registry()
        .register(&Registration::from_json(&world("slowres", "slower.example")).expect("valid"))
        .expect("registered");
    let started = std::time::Instant::now();
    let r = call(f.addr(), "GET", "/v1/resolve/slowres", &[], "");
    // Short of the 3 s lookup: the answer did not wait for it.
    assert!(started.elapsed() < Duration::from_millis(2500));
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(r.json()["resolution"]["source"], "unknown");
    assert_eq!(r.json()["resolution"]["addresses"], json!([]));
    assert_eq!(r.json()["endpoint"]["address"], "slower.example");
    // Once the lookup has finished, the next answer has its addresses.
    let until = std::time::Instant::now() + Duration::from_secs(10);
    let r = loop {
        let r = call(f.addr(), "GET", "/v1/resolve/slowres", &[], "");
        if r.json()["resolution"]["source"] == "dns" || std::time::Instant::now() > until {
            break r;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    assert_eq!(r.json()["resolution"]["source"], "dns", "{r:?}");
    assert_eq!(r.json()["endpoint"]["address"], "1.1.1.1");
    f.running.shutdown();
}

/// Spec: hub: a verification counts only for the endpoint it checked: an endpoint changed while a
/// page was read, or while the world's challenge waited for its answer, is not verified by it.
#[test]
fn a_verification_counts_only_for_the_endpoint_it_checked() {
    let f = hub();
    let ann = sign_in(&f, "2001");
    let (key, token) = register(&f, &ann, "moving", "127.0.0.1");
    let reg = f.running.registry();
    let me = reg
        .signed_in("2001", "Operator 2001")
        .expect("the operator");
    let hosts = reg.page_attempt(&me, "moving").expect("an attempt");
    let moved = |address: &str| Registration::from_json(&world("moving", address)).expect("valid");
    reg.update_for(&me, "moving", &moved("127.0.0.2"))
        .expect("moved");
    let e = reg
        .page_found(&me, "moving", std::slice::from_ref(&token), &hosts)
        .expect_err("not the endpoint it checked");
    assert_eq!(e.reason, Some("endpoint_changed"), "{}", e.message);
    let check = reg
        .game_check("moving", &key, &token)
        .expect("the call's checks");
    reg.update_for(&me, "moving", &moved("127.0.0.3"))
        .expect("moved again");
    let ip: std::net::IpAddr = "127.0.0.2".parse().expect("an address");
    let e = reg
        .game_answered("moving", &check, ip, "")
        .expect_err("not the endpoint it challenged");
    assert_eq!(e.reason, Some("endpoint_changed"), "{}", e.message);
    drop(reg);
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/moving", &Value::Null).json();
    assert_eq!(w["world"]["ownership"]["state"], "pending", "{w}");
    f.running.shutdown();
}

/// Spec: hub: a verified world given a new endpoint by its operator leaves the network, and shows
/// its endpoint as unverified, until the new one is verified.
#[test]
fn a_new_endpoint_takes_a_world_off_the_network_until_it_is_verified() {
    let f = hub();
    let ann = sign_in(&f, "2011");
    let (key, token) = register(&f, &ann, "shift", "127.0.0.1");
    assert_eq!(game_verify(&f, "shift", &key, &token, &[]).status, 200);
    assert_eq!(federation(&f, "shift"), "on");
    let public = || call(f.addr(), "GET", "/v1/servers/shift", &[], "").json();
    assert_eq!(public()["ownership"]["endpointVerified"], true);
    let mut same = world("shift", "127.0.0.1");
    same["description"] = json!("Only the words changed.");
    let a = op(&f, &ann, "PUT", "/v1/operator/worlds/shift", &same);
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(
        federation(&f, "shift"),
        "on",
        "the same endpoint keeps it on"
    );
    let a = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/shift",
        &world("shift", "127.0.0.2"),
    );
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(
        a.json()["world"]["operator"]["verification"]["endpointVerified"],
        false
    );
    assert_eq!(federation(&f, "shift"), "off");
    assert_eq!(public()["ownership"]["endpointVerified"], false);
    let on = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/shift/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(
        (on.status, on.reason()),
        (409, "endpoint_not_verified".to_owned()),
        "{on:?}"
    );
    // Back where it was is still a change since it was verified: verified again, it rejoins.
    let a = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/shift",
        &world("shift", "127.0.0.1"),
    );
    assert_eq!(
        a.json()["world"]["operator"]["verification"]["endpointVerified"],
        false
    );
    let v = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/shift/verification",
        &Value::Null,
    );
    let token = v.json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    let a = game_verify(&f, "shift", &key, &token, &[]);
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(federation(&f, "shift"), "on");
    assert_eq!(public()["ownership"]["endpointVerified"], true);
    f.running.shutdown();
}

/// A stand-in resolver that knows `names` and nothing else.
fn names(names: &'static [(&'static str, &'static str)]) -> dereth_network_hub::resolver::Lookup {
    dereth_network_hub::resolver::Lookup(std::sync::Arc::new(move |name| {
        Ok(names
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, a)| a.parse().expect("an address"))
            .collect())
    }))
}

/// Spec: hub: a world's host name must resolve to public addresses only, to be registered or
/// given as its endpoint, and `/v1/resolve` answers only public addresses.
#[test]
fn a_host_name_that_resolves_to_a_private_address_is_refused() {
    const NAMES: &[(&str, &str)] = &[
        ("lan.example", "192.168.1.20"),
        ("mixed.example", "8.8.8.8"),
        ("mixed.example", "10.0.0.7"),
        ("open.example", "8.8.4.4"),
        ("cli.example", "192.168.1.30"),
    ];
    let f = hub_with(|c| c.lookup = names(NAMES));
    let ann = sign_in(&f, "2021");
    for host in ["lan.example", "mixed.example", "192-168-1-20.nip.example"] {
        let a = op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds",
            &world("lanny", host),
        );
        let refused = host != "192-168-1-20.nip.example";
        assert_eq!(a.status, if refused { 400 } else { 201 }, "{host}: {a:?}");
        if !refused {
            op(
                &f,
                &ann,
                "DELETE",
                "/v1/operator/worlds/lanny",
                &Value::Null,
            );
        }
    }
    register(&f, &ann, "openly", "open.example");
    let a = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/openly",
        &world("openly", "lan.example"),
    );
    assert_eq!(a.status, 400, "{a:?}");
    f.running
        .registry()
        .register(&Registration::from_json(&world("byhand", "cli.example")).expect("valid"))
        .expect("the hub's operator registers what it likes");
    let first = call(f.addr(), "GET", "/v1/resolve/byhand", &[], "");
    assert_eq!(first.json()["resolution"]["source"], "unknown", "{first:?}");
    std::thread::sleep(Duration::from_millis(300));
    let again = call(f.addr(), "GET", "/v1/resolve/byhand", &[], "");
    assert_eq!(again.status, 503, "{again:?}");
    assert!(!again.body.contains("192.168.1.30"), "{again:?}");
    f.running.shutdown();
}

/// The public answers' bodies, once the snapshot has caught up: every list, and the world's own.
fn public_answers(f: &Fixture, slug: &str) -> Vec<(String, u16, String)> {
    std::thread::sleep(Duration::from_millis(1100));
    [
        "/v1/servers".to_owned(),
        "/v1/network".to_owned(),
        "/v1/status".to_owned(),
        "/compat/thwarg/Servers.xml".to_owned(),
        "/compat/treestats/player_counts-latest.json".to_owned(),
        format!("/v1/servers/{slug}"),
        format!("/v1/servers/{slug}/status"),
        format!("/v1/resolve/{slug}"),
    ]
    .into_iter()
    .map(|path| {
        let a = call(f.addr(), "GET", &path, &[], "");
        (path, a.status, a.body)
    })
    .collect()
}

fn shown_in(answers: &[(String, u16, String)], slug: &str) -> Vec<String> {
    answers
        .iter()
        .filter(|(path, status, body)| {
            *status == 200
                && (body.contains(&format!("\"{slug}\""))
                    || body.contains(&format!("World {slug}")))
                && !path.ends_with("/v1/status")
        })
        .map(|(path, _, _)| path.clone())
        .collect()
}

/// Spec: hub: a world the hub's operator locked is in no public answer, and its operator cannot
/// edit it, until it is unlocked.
#[test]
fn a_locked_world_is_in_no_public_answer_and_cannot_be_edited() {
    let f = hub();
    let ann = sign_in(&f, "2101");
    let (key, token) = register(&f, &ann, "lockt", "127.0.0.1");
    assert_eq!(game_verify(&f, "lockt", &key, &token, &[]).status, 200);
    let before = shown_in(&public_answers(&f, "lockt"), "lockt");
    assert!(before.len() >= 5, "listed while it stands: {before:?}");
    let bob = sign_in(&f, "2102");
    let filed = op(
        &f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": "lockt", "reason": "impersonation", "details": "Not them." }),
    );
    let id = filed.json()["report"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let admin = sign_in(&f, super::operators::ADMIN);
    let done = op(
        &f,
        &admin,
        "POST",
        &format!("/v1/admin/reports/{id}"),
        &json!({ "action": "revoke", "note": "Impersonation." }),
    );
    assert_eq!(done.status, 200, "{done:?}");
    let answers = public_answers(&f, "lockt");
    assert_eq!(shown_in(&answers, "lockt"), Vec::<String>::new());
    for (path, status, _) in &answers {
        if path.contains("lockt") {
            assert_eq!(*status, 404, "{path}");
        }
    }
    let mut edited = world("lockt", "127.0.0.1");
    edited["name"] = json!("Lockt Reborn");
    let a = op(&f, &ann, "PUT", "/v1/operator/worlds/lockt", &edited);
    assert_eq!((a.status, a.reason()), (409, "locked".to_owned()), "{a:?}");
    let w = op(&f, &ann, "GET", "/v1/operator/worlds/lockt", &Value::Null);
    assert_eq!(w.status, 200, "its operator still sees it");
    f.running.registry().unlock("lockt").expect("unlocked");
    assert!(shown_in(&public_answers(&f, "lockt"), "lockt").len() >= 2);
    f.running.shutdown();
}

/// Spec: hub: the hub's operator removes a world for good, freeing its slug, or takes it out of
/// the public lists (federation off, and its operator cannot turn it on) and puts it back.
#[test]
fn the_hubs_operator_removes_or_unlists_a_world() {
    let f = hub();
    let (_, key) = f
        .running
        .registry()
        .register(&Registration::from_json(&world("gone", "127.0.0.1")).expect("valid"))
        .expect("registered");
    f.running.registry().remove("gone").expect("removed");
    assert_eq!(
        call(f.addr(), "GET", "/v1/servers/gone", &[], "").status,
        404
    );
    assert!(!f.running.registry().authenticate("gone", &key));
    assert_eq!(
        f.running.registry().remove("gone").map_err(|e| e.status),
        Err(404)
    );
    f.running
        .registry()
        .register(&Registration::from_json(&world("gone", "127.0.0.1")).expect("valid"))
        .expect("the slug is free");

    let ann = sign_in(&f, "2111");
    let (key, token) = register(&f, &ann, "hide", "127.0.0.1");
    assert_eq!(game_verify(&f, "hide", &key, &token, &[]).status, 200);
    assert_eq!(federation(&f, "hide"), "on");
    f.running
        .registry()
        .set_listed("hide", false)
        .expect("unlisted");
    assert_eq!(federation(&f, "hide"), "off");
    assert_eq!(
        shown_in(&public_answers(&f, "hide"), "hide"),
        Vec::<String>::new()
    );
    let on = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/hide/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(
        (on.status, on.reason()),
        (409, "unlisted".to_owned()),
        "{on:?}"
    );
    assert_eq!(
        f.running
            .registry()
            .set_federation("hide", dereth_network_protocol::Federation::On)
            .map_err(|e| e.status),
        Err(409),
        "nor the hub's operator, while it is unlisted"
    );
    assert_eq!(
        f.running
            .registry()
            .set_listed("hide", false)
            .map_err(|e| e.status),
        Err(409)
    );
    f.running
        .registry()
        .set_listed("hide", true)
        .expect("relisted");
    assert!(shown_in(&public_answers(&f, "hide"), "hide").len() >= 2);
    let on = op(
        &f,
        &ann,
        "PUT",
        "/v1/operator/worlds/hide/federation",
        &json!({ "flag": "on" }),
    );
    assert_eq!(on.status, 200, "{on:?}");

    f.running.registry().remove("hide").expect("removed");
    let mine = op(&f, &ann, "GET", "/v1/operator/worlds", &Value::Null).json();
    assert_eq!(mine["worlds"], json!([]), "gone from its operator's worlds");
    f.running.shutdown();
}

/// Spec: hub: a world left `pending` past its days (14 unless the hub's setting says otherwise)
/// is removed, and its slug is free for another operator.
#[test]
fn a_pending_world_expires_and_frees_its_slug() {
    let f = hub();
    let ann = sign_in(&f, "2121");
    let bob = sign_in(&f, "2122");
    let (key, token) = register(&f, &ann, "keeper", "127.0.0.1");
    assert_eq!(game_verify(&f, "keeper", &key, &token, &[]).status, 200);
    register(&f, &ann, "lapse", "127.0.0.1");
    let again = || {
        op(
            &f,
            &bob,
            "POST",
            "/v1/operator/worlds",
            &world("lapse", "127.0.0.1"),
        )
        .status
    };
    assert_eq!(again(), 409);
    f.advance(Duration::from_secs(14 * 86_400 - 60));
    assert_eq!(again(), 409, "not yet");
    f.advance(Duration::from_secs(120));
    assert_eq!(again(), 201, "expired, and the slug is free");
    let mine = op(&f, &ann, "GET", "/v1/operator/worlds", &Value::Null).json();
    let slugs: Vec<&str> = mine["worlds"]
        .as_array()
        .expect("worlds")
        .iter()
        .filter_map(|w| w["slug"].as_str())
        .collect();
    assert_eq!(slugs, ["keeper"], "a verified world stays");
    f.running.shutdown();

    // Two days, here: the first world lapses a day before the second.
    let f = hub_with(|c| c.pending_days = Some(2));
    let ann = sign_in(&f, "2123");
    register(&f, &ann, "brief", "127.0.0.1");
    f.advance(Duration::from_secs(86_400));
    register(&f, &ann, "brief-two", "127.0.0.1");
    f.advance(Duration::from_secs(86_400 + 1));
    f.running
        .registry()
        .register(&Registration::from_json(&world("brief", "127.0.0.1")).expect("valid"))
        .expect("the hub's operator registers the freed slug");
    f.advance(Duration::from_secs(86_400));
    let mine = op(&f, &ann, "GET", "/v1/operator/worlds", &Value::Null).json();
    assert_eq!(mine["worlds"], json!([]), "both lapsed");
    f.running.shutdown();
}

/// Spec: hub: an operator's lapsed worlds no longer count against the worlds it may hold.
#[test]
fn lapsed_worlds_do_not_count_against_what_an_operator_holds() {
    let f = hub_with(|c| c.pending_days = Some(5));
    let ann = sign_in(&f, "2131");
    let mut n = 0;
    for _day in 0..4 {
        for _ in 0..3 {
            if n < 10 {
                register(&f, &ann, &format!("held-{}", letters(n)), "127.0.0.1");
                n += 1;
            }
        }
        f.advance(Duration::from_secs(86_400 + 1));
    }
    let more = || {
        op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds",
            &world("one-more", "127.0.0.1"),
        )
    };
    assert_eq!(more().status, 409, "ten held");
    f.advance(Duration::from_secs(86_400));
    let a = more();
    assert_eq!(a.status, 201, "the first day's three lapsed: {a:?}");
    f.running.shutdown();
}

/// `n` as letters, for a slug.
fn letters(n: usize) -> String {
    let a = b'a' + u8::try_from(n % 26).expect("small");
    let b = b'a' + u8::try_from(n / 26 % 26).expect("small");
    String::from_utf8(vec![b, a]).expect("letters")
}

/// Writes `head` to a fresh connection to the hub, then `body` a byte at a time, `every` apart,
/// until the hub answers or closes: its answer's status, and how long it took.
fn trickle(f: &Fixture, head: &str, body: &[u8], every: Duration) -> (Option<u16>, Duration) {
    use std::io::{Read as _, Write as _};
    let mut s = std::net::TcpStream::connect(f.addr()).expect("connect");
    let started = std::time::Instant::now();
    s.write_all(head.as_bytes()).expect("the head");
    s.set_read_timeout(Some(every)).expect("a timeout");
    let mut answer = Vec::new();
    let mut sent = 0;
    loop {
        let mut buf = [0u8; 1024];
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                answer.extend_from_slice(&buf[..n]);
                if answer.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => {}
        }
        if started.elapsed() > Duration::from_secs(20) {
            break;
        }
        if sent < body.len() && s.write_all(&body[sent..=sent]).is_ok() {
            sent += 1;
        }
    }
    let status = std::str::from_utf8(&answer)
        .ok()
        .and_then(|a| a.split(' ').nth(1))
        .and_then(|c| c.parse().ok());
    (status, started.elapsed())
}

/// Spec: hub: a request's body arrives within its deadline in all, however it trickles in, or
/// the request is answered `408` and closed.
#[test]
fn a_trickled_body_is_cut_off_at_its_deadline() {
    let f = hub_with(|c| c.body_deadline = Duration::from_millis(800));
    let body = br#"{"slug":"x","reason":"other","details":"slowly, slowly, slowly"}"#;
    let head = format!(
        "POST /v1/reports HTTP/1.1\r\nHost: hub.test\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n",
        body.len()
    );
    // The whole body would take nearly ten seconds; the cut comes at the deadline, well inside five.
    let (status, took) = trickle(&f, &head, body, Duration::from_millis(150));
    assert_eq!(status, Some(408));
    assert!(took < Duration::from_secs(5), "{took:?}");
    // A body that never comes is cut off at the same deadline.
    let (status, took) = trickle(&f, &head, &[], Duration::from_millis(100));
    assert_eq!(status, Some(408));
    assert!(took < Duration::from_secs(5), "{took:?}");
    f.running.shutdown();
}

/// Spec: hub: a request is counted against its address's allowance before its body is read, so a
/// refused one is answered at once, its body never waited for.
#[test]
fn the_allowance_is_taken_before_the_body_is_read() {
    let f = hub_with(|c| c.http_per_minute = 2);
    for _ in 0..2 {
        assert_eq!(call(f.addr(), "GET", "/v1/servers", &[], "").status, 200);
    }
    let head = "POST /v1/reports HTTP/1.1\r\nHost: hub.test\r\nContent-Type: application/json\r\n\
                Content-Length: 4000\r\n\r\n";
    let (status, took) = trickle(&f, head, &[], Duration::from_millis(100));
    assert_eq!(status, Some(429));
    assert!(took < Duration::from_secs(2), "{took:?}");
    f.running.shutdown();
}

/// Writes `bytes` to a fresh connection to the hub and reads its whole answer.
fn send_raw(f: &Fixture, bytes: &[u8]) -> Answer {
    use std::io::{Read as _, Write as _};
    let mut s = std::net::TcpStream::connect(f.addr()).expect("connect");
    s.write_all(bytes).expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let mut lines = head.split("\r\n");
    Answer {
        status: lines
            .next()
            .and_then(|l| l.split(' ').nth(1))
            .and_then(|c| c.parse().ok())
            .unwrap_or(0),
        headers: lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect(),
        body: body.to_owned(),
    }
}

/// Spec: hub: a request's head may be 32 KiB and 128 header lines, what Cloudflare passes on; one
/// past either is answered `431`, and one that is not HTTP `400`, never dropped unanswered.
#[test]
fn a_head_within_cloudflares_limits_is_read_and_one_past_them_is_answered() {
    let f = hub();
    let get = |headers: &[(String, String)]| {
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        call(f.addr(), "GET", "/v1/status", &headers, "")
    };
    let cookie = |kib: usize| {
        vec![(
            "Cookie".to_owned(),
            format!("stale={}", "c".repeat(kib * 1024)),
        )]
    };
    assert_eq!(get(&cookie(20)).status, 200, "a head of 20 KiB");
    let big = get(&cookie(40));
    assert_eq!(big.status, 431, "a head of 40 KiB: {big:?}");
    assert_eq!(big.json()["error"]["code"], "HEADERS_TOO_LARGE");
    // `call` writes two lines of its own, Host and Content-Length.
    let lines = |n: usize| -> Vec<(String, String)> {
        (2..n)
            .map(|i| (format!("X-Line-{i}"), "1".into()))
            .collect()
    };
    assert_eq!(get(&lines(100)).status, 200, "100 header lines");
    let many = get(&lines(200));
    assert_eq!(many.status, 431, "200 header lines: {many:?}");
    for garbage in [
        &b"NOT HTTP AT ALL\r\n\r\n"[..],
        &b"\x16\x03\x01\x02\x00\x01\x00\x01\xfc\x03\x03\r\n\r\n"[..],
    ] {
        let a = send_raw(&f, garbage);
        assert_eq!(a.status, 400, "{a:?}");
        assert_eq!(a.json()["error"]["code"], "INVALID_REQUEST");
    }
    f.running.shutdown();
}

/// Spec: hub: an answer to a head the hub does not read carries the site's CORS when the head's
/// `Origin` was read and is a site origin, so the site can say why; otherwise it carries none.
#[test]
fn a_large_cookie_from_the_site_is_answered_431_where_the_site_reads_it() {
    let f = hub();
    let cookie = format!("__Host-dn_session={}", "c".repeat(40 * 1024));
    let from_site = call(
        f.addr(),
        "GET",
        "/v1/operator",
        &[("Origin", SITE), ("Cookie", &cookie)],
        "",
    );
    assert_eq!(from_site.status, 431, "{from_site:?}");
    assert_eq!(from_site.header("access-control-allow-origin"), Some(SITE));
    assert_eq!(
        from_site.header("access-control-allow-credentials"),
        Some("true")
    );
    for (why, headers) in [
        (
            "another origin",
            [
                ("Origin", "https://elsewhere.test"),
                ("Cookie", cookie.as_str()),
            ],
        ),
        (
            "an origin past what was read",
            [("Cookie", cookie.as_str()), ("Origin", SITE)],
        ),
    ] {
        let a = call(f.addr(), "GET", "/v1/operator", &headers, "");
        assert_eq!(a.status, 431, "{why}: {a:?}");
        assert_eq!(a.header("access-control-allow-origin"), None, "{why}");
    }
    f.running.shutdown();
}

/// Reports `slug` as Bob and has an admin revoke it: the world is locked.
fn lock(f: &Fixture, slug: &str, reporter: &str) {
    let bob = sign_in(f, reporter);
    let filed = op(
        f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": slug, "reason": "impersonation", "details": "Not them." }),
    );
    assert_eq!(filed.status, 201, "{filed:?}");
    let id = filed.json()["report"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let admin = sign_in(f, super::operators::ADMIN);
    let done = op(
        f,
        &admin,
        "POST",
        &format!("/v1/admin/reports/{id}"),
        &json!({ "action": "revoke", "note": "Impersonation." }),
    );
    assert_eq!(done.status, 200, "{done:?}");
}

/// Spec: hub: a locked listing cannot be claimed, by an operator or on the command line, nor a
/// locked world given a key, until the hub's operator unlocks it.
#[test]
fn a_locked_listing_cannot_be_claimed_or_given_a_key() {
    let f = hub();
    listing(&f, "shut", "127.0.0.1");
    lock(&f, "shut", "2201");
    let cl = sign_in(&f, "2202");
    let a = op(
        &f,
        &cl,
        "POST",
        "/v1/operator/worlds/shut/claim",
        &Value::Null,
    );
    assert_eq!(a.status, 404, "{a:?}");
    assert_eq!(
        f.running
            .registry()
            .claim("shut", "Seen on their site.")
            .map_err(|e| e.status),
        Err(409)
    );
    let ann = sign_in(&f, "2203");
    let (key, token) = register(&f, &ann, "shut-two", "127.0.0.1");
    assert_eq!(game_verify(&f, "shut-two", &key, &token, &[]).status, 200);
    lock(&f, "shut-two", "2204");
    assert_eq!(
        f.running
            .registry()
            .rotate_key("shut-two")
            .map_err(|e| e.status),
        Err(409)
    );
    f.running.registry().unlock("shut").expect("unlocked");
    let a = op(
        &f,
        &cl,
        "POST",
        "/v1/operator/worlds/shut/claim",
        &Value::Null,
    );
    assert_eq!(a.status, 201, "{a:?}");
    f.running.registry().unlock("shut-two").expect("unlocked");
    assert!(f.running.registry().rotate_key("shut-two").is_ok());
    f.running.shutdown();
}

/// Spec: hub: an observed world is challenged at the address its call came from, and verified
/// there keeps the address it answered at.
#[test]
fn an_observed_world_verified_in_game_keeps_the_address_it_answered_at() {
    let f = hub();
    let ann = sign_in(&f, "2301");
    let mut w = world("seen", "127.0.0.1");
    w["endpoint"] = json!({ "mode": "observed", "port": super::operators::game_port(), "addressFamily": "ipv4" });
    let a = op(&f, &ann, "POST", "/v1/operator/worlds", &w);
    assert_eq!(a.status, 201, "{a:?}");
    let key = a.json()["key"].as_str().expect("a key").to_owned();
    let token = a.json()["verification"]["token"]
        .as_str()
        .expect("a token")
        .to_owned();
    assert_eq!(game_verify(&f, "seen", &key, &token, &[]).status, 200);
    let shown = call(f.addr(), "GET", "/v1/servers/seen", &[], "").json();
    assert_eq!(shown["endpoint"]["address"], "127.0.0.1", "{shown}");
    assert_eq!(shown["endpoint"]["source"], "heartbeat-source");
    f.running.shutdown();
}

/// Spec: hub: a claimant sees only what it did in a listing's log, never another claimant's
/// verification; and another operator's `pending` world cannot be claimed, as it is not found.
#[test]
fn a_claimant_sees_only_its_own_claim() {
    let f = hub();
    listing(&f, "both", "127.0.0.1");
    let bob = sign_in(&f, "2401");
    let carol = sign_in(&f, "2402");
    claim(&f, &bob, "both");
    let (carol_key, _) = claim(&f, &carol, "both");
    let wrong = game_verify(
        &f,
        "both",
        &carol_key,
        &format!("dnv_{}", "A".repeat(43)),
        &[],
    );
    assert_eq!(wrong.status, 422, "{wrong:?}");
    let id_of = |session: &str| {
        op(&f, session, "GET", "/v1/operator", &Value::Null).json()["operator"]["id"]
            .as_str()
            .expect("an id")
            .to_owned()
    };
    let (bob_id, carol_id) = (id_of(&bob), id_of(&carol));
    let seen = op(&f, &bob, "GET", "/v1/operator/worlds/both", &Value::Null);
    assert_eq!(seen.status, 200, "{seen:?}");
    let audit = seen.json()["audit"].clone();
    let audit = audit.as_array().expect("the log");
    assert!(
        audit.iter().any(|e| e["kind"] == "claim-opened"),
        "its own claim: {audit:?}"
    );
    assert!(
        audit.iter().all(|e| e["actor"] == bob_id.as_str()),
        "{audit:?}"
    );
    assert!(!seen.body.contains(&carol_id));

    let ann = sign_in(&f, "2403");
    register(&f, &ann, "hers", "127.0.0.1");
    let a = op(
        &f,
        &bob,
        "POST",
        "/v1/operator/worlds/hers/claim",
        &Value::Null,
    );
    assert_eq!(a.status, 404, "{a:?}");
    let a = op(
        &f,
        &ann,
        "POST",
        "/v1/operator/worlds/hers/claim",
        &Value::Null,
    );
    assert_eq!(a.status, 409, "her own: {a:?}");
    f.running.shutdown();
}

/// Spec: hub: a verification page's URL that is not one, however odd, is refused with `400`.
#[test]
fn an_odd_page_url_is_refused_not_a_failure() {
    let f = hub();
    let ann = sign_in(&f, "2501");
    register(&f, &ann, "oddurl", "127.0.0.1");
    for url in [
        "https:/\u{e9}/x",
        "h\u{e9}ttps://x",
        "https://\u{e9}.example/",
        "https://x:+1/",
    ] {
        let a = op(
            &f,
            &ann,
            "POST",
            "/v1/operator/worlds/oddurl/verify",
            &json!({ "url": url }),
        );
        assert_eq!(a.status, 400, "{url}: {a:?}");
    }
    f.running.shutdown();
}

/// Spec: hub: every text field has a bound (a world's emulator version, a heartbeat's, an admin's
/// note included), refused past it rather than cut.
#[test]
fn every_text_field_is_bounded() {
    let mut w = world("bounded", "127.0.0.1");
    w["emulator"]["version"] = json!("1".repeat(60));
    assert!(Registration::from_json(&w).is_ok());
    w["emulator"]["version"] = json!("1".repeat(61));
    assert_eq!(
        Registration::from_json(&w).map_err(|e| e.status).err(),
        Some(400)
    );
    w["emulator"]["version"] = json!("");
    assert_eq!(
        Registration::from_json(&w).map_err(|e| e.status).err(),
        Some(400)
    );
    let beat = |version: &str| {
        serde_json::from_value::<dereth_network_hub::registry::Heartbeat>(
            json!({ "state": "online", "emulatorVersion": version }),
        )
        .expect("a heartbeat")
        .check()
        .map_err(|e| e.status)
    };
    assert_eq!(beat(&"9".repeat(60)), Ok(()));
    assert_eq!(beat(&"9".repeat(61)), Err(400));

    let f = hub();
    let ann = sign_in(&f, "2601");
    let (key, token) = register(&f, &ann, "noted", "127.0.0.1");
    assert_eq!(game_verify(&f, "noted", &key, &token, &[]).status, 200);
    let bob = sign_in(&f, "2602");
    let filed = op(
        &f,
        &bob,
        "POST",
        "/v1/reports",
        &json!({ "slug": "noted", "reason": "other", "details": "Odd." }),
    );
    let id = filed.json()["report"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let admin = sign_in(&f, super::operators::ADMIN);
    let resolve = |note: String| {
        op(
            &f,
            &admin,
            "POST",
            &format!("/v1/admin/reports/{id}"),
            &json!({ "action": "dismiss", "note": note }),
        )
        .status
    };
    assert_eq!(resolve("n".repeat(2001)), 400);
    assert_eq!(resolve("n".repeat(2000)), 200);
    f.running.shutdown();
}

/// Spec: hub: the operator API takes a body only as `application/json` exactly (parameters
/// allowed), and no request's body chunked: that is answered `411` and the connection closed.
#[test]
fn a_body_is_json_exactly_and_never_chunked() {
    let f = hub();
    let ann = sign_in(&f, "2611");
    let (key, token) = register(&f, &ann, "typed", "127.0.0.1");
    assert_eq!(game_verify(&f, "typed", &key, &token, &[]).status, 200);
    let bob = sign_in(&f, "2612");
    let cookie = format!("__Host-dn_session={bob}");
    let report = |kind: &str| {
        call(
            f.addr(),
            "POST",
            "/v1/reports",
            &[
                ("Cookie", &cookie),
                ("Origin", super::operators::SITE),
                ("Content-Type", kind),
            ],
            r#"{"slug":"typed","reason":"other","details":"Odd."}"#,
        )
        .status
    };
    for refused in [
        "application/jsonp",
        "application/json-seq",
        "text/plain; application/json",
        "text/plain",
    ] {
        assert_eq!(report(refused), 400, "{refused}");
    }
    assert_eq!(report("Application/JSON ; charset=utf-8"), 201);

    let chunked =
        "POST /v1/reports HTTP/1.1\r\nHost: hub.test\r\nContent-Type: application/json\r\n\
                   Transfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
    let (status, took) = trickle(&f, chunked, &[], Duration::from_millis(100));
    assert_eq!(status, Some(411));
    assert!(took < Duration::from_secs(2), "{took:?}");
    f.running.shutdown();
}

/// Spec: hub: a request refused before its body is read (an unreadable forwarded address, the
/// allowance spent, a chunked body) is still answered: the hub takes in what the client is sending
/// before it closes, so the answer is not lost to a reset.
#[test]
fn a_refused_requests_answer_survives_its_unread_body() {
    let f = hub_with(|c| {
        c.trusted_proxies = vec!["127.0.0.1".parse().expect("a range")];
        c.http_per_minute = 12;
    });
    let body = format!(r#"{{"state":"online"}}{}"#, " ".repeat(16 * 1024));
    let send = |extra: &[(&str, &str)]| {
        let mut headers = vec![("Content-Type", "application/json")];
        headers.extend_from_slice(extra);
        call(
            f.addr(),
            "POST",
            "/v1/servers/nowhere/heartbeat",
            &headers,
            &body,
        )
        .status
    };
    for _ in 0..10 {
        assert_eq!(send(&[("X-Forwarded-For", "unknown, 9.9.9.9")]), 400);
        assert_eq!(
            send(&[
                ("X-Forwarded-For", "9.9.9.11"),
                ("Transfer-Encoding", "chunked")
            ]),
            411
        );
    }
    let mut refused = 0;
    for _ in 0..30 {
        if send(&[("X-Forwarded-For", "9.9.9.10")]) == 429 {
            refused += 1;
        }
    }
    assert!(refused >= 10, "{refused} answered 429");
    // A client that sends everything at once and reads only a while later still reads the
    // answer: nothing it sent is left unread when the hub closes.
    for _ in 0..5 {
        use std::io::{Read as _, Write as _};
        let mut s = std::net::TcpStream::connect(f.addr()).expect("connect");
        let request = "POST /v1/servers/nowhere/heartbeat HTTP/1.1\r\nHost: hub.test\r\n\
                       X-Forwarded-For: unknown, 9.9.9.9\r\nContent-Type: application/json\r\n\
                       Content-Length: 18\r\n\r\n{\"state\":\"online\"}";
        s.write_all(request.as_bytes()).expect("the request");
        std::thread::sleep(Duration::from_millis(400));
        let mut answer = Vec::new();
        s.read_to_end(&mut answer).expect("the answer, not a reset");
        assert!(
            answer.starts_with(b"HTTP/1.1 400 "),
            "{:?}",
            String::from_utf8_lossy(&answer)
        );
    }
    f.running.shutdown();
}
