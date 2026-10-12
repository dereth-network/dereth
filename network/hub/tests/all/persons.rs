//! Persons on a running hub: a world asks for a link code over its link, the player enters it on
//! the site, signed in with Discord, and the world's view of its character follows; what the site,
//! the public routes, a world's audit log and other worlds may and may not learn.
//! Fixture: the operator tests' hub (loopback, an in-memory registry, a stand-in for Discord), with
//! worlds registered on its command line and linked through the conformance suite's links.

use std::time::Duration;

use dereth_network_conformance::hub::Link;
use dereth_network_hub::registry::Registration;
use dereth_network_protocol::{Address, Alts, Federation, Message, PersonOp, PersonOutcome};
use serde_json::{json, Value};

use super::operators::{call, game_verify, hub, op, register, sign_in, Fixture, ADMIN, SITE};

fn a(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

/// Registers `slug` on the network, and links it: its link, greeted.
fn world(f: &Fixture, slug: &str) -> Link {
    let reg = Registration::from_json(&json!({
        "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
        "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
        "emulator": { "family": "ACE" },
    }))
    .expect("a registration");
    let key = {
        let r = f.running.registry();
        let (_, key) = r.register(&reg).expect("registered");
        r.set_federation(slug, Federation::On).expect("on");
        key
    };
    let mut link = Link::open(&format!("ws://{}", f.addr()), slug, &key).expect("links");
    link.greet().expect("welcomed");
    link
}

/// The next view of `who` the link receives.
fn next_view(link: &mut Link, who: &str) -> (bool, Alts, Vec<(String, Vec<String>)>) {
    let who = a(who);
    link.expect(Duration::from_secs(5), "a person view", |m| match m {
        Message::PersonFriends {
            to,
            linked,
            alts,
            persons,
        } if to.same(&who) => Some((
            *linked,
            *alts,
            persons
                .iter()
                .map(|p| {
                    (
                        p.handle.to_string(),
                        p.characters.iter().map(ToString::to_string).collect(),
                    )
                })
                .collect(),
        )),
        _ => None,
    })
    .expect("a view")
}

fn ask(
    link: &mut Link,
    id: &str,
    op: PersonOp,
    who: &str,
    target: Option<&str>,
    shown: Option<bool>,
) -> Message {
    link.send(&Message::Person {
        id: id.into(),
        op,
        who: a(who),
        target: target.map(a),
        targets: Vec::new(),
        shown,
    })
    .expect("sent");
    let id = id.to_owned();
    link.expect(Duration::from_secs(5), "a person result", |m| {
        matches!(m, Message::PersonResult { id: i, .. } if *i == id).then(|| m.clone())
    })
    .expect("an answer")
}

/// The code a world is given for `who`.
fn code_for(link: &mut Link, id: &str, who: &str) -> String {
    match ask(link, id, PersonOp::Code, who, None, None) {
        Message::PersonResult {
            outcome: PersonOutcome::Ok,
            code: Some(code),
            ..
        } => code,
        other => panic!("no code: {other:?}"),
    }
}

fn watch(link: &mut Link, who: &str) {
    link.send(&Message::PersonWatch {
        who: a(who),
        on: true,
    })
    .expect("sent");
}

fn link_with(f: &Fixture, session: &str, code: &str) -> Value {
    let r = op(
        f,
        session,
        "POST",
        "/v1/person/link",
        &json!({ "code": code }),
    );
    assert_eq!(r.status, 200, "{r:?}");
    r.json()
}

/// Spec: hub: a world asks for a code for its own character over its link, the player enters it
/// on the site, signed in, and the world's view of the character follows; a code works once.
#[test]
fn a_character_is_linked_with_the_code_its_world_asked_for_and_its_view_follows() {
    let f = hub();
    let mut aa = world(&f, "aa");
    watch(&mut aa, "Ann@aa");
    assert_eq!(next_view(&mut aa, "Ann@aa"), (false, Alts::Hidden, vec![]));
    let answer = ask(&mut aa, "p1", PersonOp::Code, "Ann@aa", None, None);
    let Message::PersonResult {
        outcome: PersonOutcome::Ok,
        code: Some(code),
        expires_secs: Some(600),
        url: Some(url),
        ..
    } = answer
    else {
        panic!("a code: {answer:?}")
    };
    assert_eq!(url, format!("{SITE}/characters/"));
    let session = sign_in(&f, "100");
    let linked = link_with(&f, &session, &code);
    assert_eq!(linked["character"]["address"], "Ann@aa");
    assert_eq!(linked["character"]["world"]["name"], "World aa");
    assert_eq!(next_view(&mut aa, "Ann@aa"), (true, Alts::Hidden, vec![]));
    let me = op(&f, &session, "GET", "/v1/person", &Value::Null).json();
    assert_eq!(me["person"]["characters"][0]["address"], "Ann@aa");
    assert_eq!(me["person"]["alts"], "hidden");
    assert!(me["person"].get("discordId").is_none(), "{me}");
    let again = op(
        &f,
        &session,
        "POST",
        "/v1/person/link",
        &json!({ "code": code }),
    );
    assert_eq!(again.status, 422);
    assert_eq!(again.json()["error"]["code"], "LINK_FAILED");
    assert_eq!(again.reason(), "no_such_code");
    let gone = op(
        &f,
        &session,
        "DELETE",
        "/v1/person/characters/Ann%40aa",
        &Value::Null,
    );
    assert_eq!(gone.status, 204, "{gone:?}");
    assert_eq!(next_view(&mut aa, "Ann@aa"), (false, Alts::Hidden, vec![]));
    f.running.shutdown();
}

/// Spec: hub: a change on the site reaches the views of the characters it changes, on every
/// world; a world holding one character of a person that hides its alts hears nothing of the
/// person's others.
#[test]
fn the_site_changes_what_worlds_hear_and_a_hidden_person_stays_hidden() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let mut bb = world(&f, "bb");
    let ann = sign_in(&f, "100");
    let bob = sign_in(&f, "200");
    let code = code_for(&mut aa, "a1", "Ann@aa");
    link_with(&f, &ann, &code);
    let code = code_for(&mut bb, "b1", "Ann@bb");
    link_with(&f, &ann, &code);
    let code = code_for(&mut bb, "b2", "Bob@bb");
    link_with(&f, &bob, &code);
    let code = code_for(&mut aa, "a2", "Bob@aa");
    link_with(&f, &bob, &code);
    watch(&mut aa, "Ann@aa");
    assert_eq!(next_view(&mut aa, "Ann@aa"), (true, Alts::Hidden, vec![]));
    watch(&mut bb, "Ann@bb");
    assert_eq!(next_view(&mut bb, "Ann@bb"), (true, Alts::Hidden, vec![]));
    let ok = ask(
        &mut aa,
        "a3",
        PersonOp::Befriend,
        "Ann@aa",
        Some("Bob@bb"),
        None,
    );
    assert!(matches!(
        ok,
        Message::PersonResult {
            outcome: PersonOutcome::Ok,
            ..
        }
    ));
    assert_eq!(
        next_view(&mut aa, "Ann@aa").2,
        vec![("Bob@bb".to_owned(), vec!["Bob@bb".to_owned()])],
        "Bob hides his alts: the handle alone"
    );
    let shown = op(
        &f,
        &bob,
        "PUT",
        "/v1/person/alts",
        &json!({ "shown": true }),
    );
    assert_eq!(shown.status, 200, "{shown:?}");
    assert_eq!(shown.json()["person"]["alts"], "shown");
    assert_eq!(
        next_view(&mut aa, "Ann@aa").2,
        vec![(
            "Bob@bb".to_owned(),
            vec!["Bob@bb".to_owned(), "Bob@aa".to_owned()]
        )]
    );
    // Bob's other character, asked about by anyone.
    let asked = ask(
        &mut bb,
        "b3",
        PersonOp::Characters,
        "Ann@bb",
        Some("Bob@aa"),
        None,
    );
    assert!(matches!(&asked, Message::PersonResult { characters, .. } if characters.len() == 2));
    // Ann hides hers: nothing bb receives ever names Ann@aa, and asking gives nothing.
    let asked = ask(
        &mut bb,
        "b4",
        PersonOp::Characters,
        "Ann@bb",
        Some("Ann@aa"),
        None,
    );
    assert!(
        matches!(&asked, Message::PersonResult { outcome: PersonOutcome::Ok, characters, .. } if characters.is_empty()),
        "{asked:?}"
    );
    let unlinked = op(
        &f,
        &bob,
        "DELETE",
        "/v1/person/characters/Bob@aa",
        &Value::Null,
    );
    assert_eq!(unlinked.status, 204);
    assert_eq!(
        next_view(&mut aa, "Ann@aa").2,
        vec![("Bob@bb".to_owned(), vec!["Bob@bb".to_owned()])]
    );
    let mut heard = Vec::new();
    while let Ok(Some(m)) = bb.recv(Duration::from_millis(300)) {
        heard.push(m);
    }
    for m in &heard {
        assert!(
            !m.to_json().to_lowercase().contains("ann@aa"),
            "bb heard of Ann@aa: {m:?}"
        );
    }
    f.running.shutdown();
}

/// Spec: hub: a world asking to show its character's person's characters is refused and given the
/// site's page for it; the person stays hidden until it shows them there.
#[test]
fn a_world_cannot_show_a_persons_characters_and_is_given_the_page_to_do_it_on() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let mut bb = world(&f, "bb");
    let bob = sign_in(&f, "200");
    let code = code_for(&mut aa, "a1", "Bob@aa");
    link_with(&f, &bob, &code);
    let code = code_for(&mut bb, "b1", "Bob@bb");
    link_with(&f, &bob, &code);
    let answer = ask(&mut aa, "a2", PersonOp::Alts, "Bob@aa", None, Some(true));
    let Message::PersonResult {
        outcome: PersonOutcome::Refused,
        url: Some(url),
        ..
    } = answer
    else {
        panic!("refused: {answer:?}")
    };
    assert_eq!(url, format!("{SITE}/characters/"));
    let me = op(&f, &bob, "GET", "/v1/person", &Value::Null).json();
    assert_eq!(me["person"]["alts"], "hidden");
    let asked = ask(
        &mut aa,
        "a3",
        PersonOp::Characters,
        "Bob@aa",
        Some("Bob@bb"),
        None,
    );
    assert!(
        matches!(&asked, Message::PersonResult { characters, .. } if characters.is_empty()),
        "{asked:?}"
    );
    f.running.shutdown();
}

/// The public routes' answers, as anyone may read them.
fn public(f: &Fixture) -> Vec<(String, String)> {
    [
        "/v1/servers",
        "/v1/servers/aa",
        "/v1/servers/aa/status",
        "/v1/network",
        "/v1/status",
        "/v1/config",
        "/compat/thwarg/Servers.xml",
        "/compat/treestats/player_counts-latest.json",
    ]
    .iter()
    .map(|p| {
        (
            (*p).to_owned(),
            without_generated_at(&call(f.addr(), "GET", p, &[], "").body),
        )
    })
    .collect()
}

/// `body` with the time its snapshot was made blanked: the lists are answered from a snapshot at
/// most a second old, so two reads seconds apart differ there and nowhere else.
fn without_generated_at(body: &str) -> String {
    const FIELD: &str = "\"generatedAt\":\"";
    match body.find(FIELD) {
        Some(at) => {
            let start = at + FIELD.len();
            let end = body[start..].find('"').map_or(body.len(), |n| start + n);
            format!("{}{}", &body[..start], &body[end..])
        }
        None => body.to_owned(),
    }
}

/// Spec: hub: no public route and no world's audit log says anything of persons.
#[test]
fn the_public_routes_and_a_worlds_audit_log_say_nothing_of_persons() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let mut bb = world(&f, "bb");
    let before = public(&f);
    let audit = f.running.registry().audit("aa").expect("the audit");
    let ann = sign_in(&f, "100");
    for (link, id, who) in [(&mut aa, "a1", "Ann@aa"), (&mut bb, "b1", "Ann@bb")] {
        let code = code_for(link, id, who);
        link_with(&f, &ann, &code);
    }
    let shown = op(
        &f,
        &ann,
        "PUT",
        "/v1/person/alts",
        &json!({ "shown": true }),
    );
    assert_eq!(shown.status, 200);
    ask(
        &mut aa,
        "a2",
        PersonOp::Befriend,
        "Ann@aa",
        Some("Bob@bb"),
        None,
    );
    let after = public(&f);
    assert_eq!(after, before);
    for (path, body) in &after {
        for word in ["Ann@", "Player 100", "Operator 100", "\"100\""] {
            assert!(!body.contains(word), "{path} names {word}");
        }
    }
    assert_eq!(f.running.registry().audit("aa").expect("the audit"), audit);
    f.running.shutdown();
}

/// Spec: hub: a person's routes need a session, and a change needs the site's origin.
#[test]
fn a_persons_routes_need_a_session_and_the_sites_origin() {
    let f = hub();
    let no_session = call(f.addr(), "GET", "/v1/person", &[("Origin", SITE)], "");
    assert_eq!(no_session.status, 401);
    let session = sign_in(&f, "100");
    let cookie = format!("__Host-dn_session={session}");
    for origin in ["https://evil.example", "https://other.site.test"] {
        let r = call(
            f.addr(),
            "POST",
            "/v1/person/link",
            &[
                ("Cookie", &cookie),
                ("Origin", origin),
                ("Content-Type", "application/json"),
            ],
            r#"{"code":"0000-0000"}"#,
        );
        assert_eq!(r.status, 403, "{origin}");
    }
    let pre = call(
        f.addr(),
        "OPTIONS",
        "/v1/person/link",
        &[("Origin", SITE)],
        "",
    );
    assert_eq!(pre.status, 204);
    assert_eq!(pre.header("access-control-allow-origin"), Some(SITE));
    let me = op(&f, &session, "GET", "/v1/person", &Value::Null);
    assert_eq!(me.header("access-control-allow-credentials"), Some("true"));
    assert_eq!(me.json()["person"]["characters"], json!([]));
    f.running.shutdown();
}

/// Spec: hub: forgetting a person that holds nothing else removes its account, and its session
/// with it.
#[test]
fn forgetting_a_person_ends_its_session() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let ann = sign_in(&f, "100");
    let code = code_for(&mut aa, "a1", "Ann@aa");
    link_with(&f, &ann, &code);
    watch(&mut aa, "Ann@aa");
    assert!(next_view(&mut aa, "Ann@aa").0);
    let gone = op(&f, &ann, "DELETE", "/v1/person", &Value::Null);
    assert_eq!(gone.status, 204, "{gone:?}");
    assert!(gone
        .cookie("__Host-dn_session")
        .is_some_and(|c| c.contains("Max-Age=0")));
    assert_eq!(op(&f, &ann, "GET", "/v1/person", &Value::Null).status, 401);
    assert_eq!(next_view(&mut aa, "Ann@aa"), (false, Alts::Hidden, vec![]));
    f.running.shutdown();
}

/// The operator id of the account signed in with `session`.
fn operator_id(f: &Fixture, session: &str) -> String {
    op(f, session, "GET", "/v1/operator", &Value::Null).json()["operator"]["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

/// Forgets the person signed in with `session`: whether its account went, its cookie cleared.
fn forget(f: &Fixture, session: &str) -> bool {
    let r = op(f, session, "DELETE", "/v1/person", &Value::Null);
    assert_eq!(r.status, 204, "{r:?}");
    r.cookie("__Host-dn_session")
        .is_some_and(|c| c.contains("Max-Age=0"))
}

/// The reports an admin reads, open and closed.
fn all_reports(f: &Fixture) -> Vec<Value> {
    let admin = sign_in(f, ADMIN);
    op(
        f,
        &admin,
        "GET",
        "/v1/admin/reports?state=all",
        &Value::Null,
    )
    .json()["reports"]
        .as_array()
        .expect("the reports")
        .clone()
}

/// The kinds of `slug`'s events, as the hub's command line reads them.
fn kinds(f: &Fixture, slug: &str) -> Vec<String> {
    f.running
        .registry()
        .audit(slug)
        .expect("the audit")
        .as_array()
        .expect("events")
        .iter()
        .map(|e| e["kind"].as_str().expect("a kind").to_owned())
        .collect()
}

/// Imports `slug` from the community list, as its `n`th entry: an unclaimed listing.
fn listing(f: &Fixture, slug: &str, n: u8) {
    let entry = dereth_network_hub::registry::CommunityEntry {
        external_id: format!("00000000-0000-4000-8000-0000000001{n:02}"),
        registration: Registration::from_json(&json!({
            "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
            "endpoint": {
                "mode": "manual", "address": "127.0.0.1", "port": super::operators::game_port(),
            },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration"),
        source_hash: "h".into(),
        legacy_type: "PvE".into(),
    };
    f.running
        .registry()
        .import_community(&[entry], true)
        .expect("imported");
}

/// Spec: hub: forgetting a person that is not an operator removes its account and every report it
/// filed, open or closed; a reported world's log still says it was reported, and why, and a world
/// revoked on a report stays revoked.
#[test]
fn forgetting_a_player_removes_its_reports_and_its_account() {
    let f = hub();
    let _aa = world(&f, "aa");
    let _bb = world(&f, "bb");
    let bob = sign_in(&f, "700");
    let bob_id = operator_id(&f, &bob);
    let report = |slug: &str| {
        let r = op(
            &f,
            &bob,
            "POST",
            "/v1/reports",
            &json!({ "slug": slug, "reason": "spam", "details": "Spam in the chat." }),
        );
        assert_eq!(r.status, 201, "{r:?}");
        r.json()["report"]["id"].as_str().expect("an id").to_owned()
    };
    report("aa");
    let closed = report("bb");
    let admin = sign_in(&f, ADMIN);
    let revoked = op(
        &f,
        &admin,
        "POST",
        &format!("/v1/admin/reports/{closed}"),
        &json!({ "action": "revoke", "note": "Spam." }),
    );
    assert_eq!(revoked.status, 200, "{revoked:?}");
    assert_eq!(all_reports(&f).len(), 2);
    assert!(forget(&f, &bob), "the account went");
    assert_eq!(
        op(&f, &bob, "GET", "/v1/operator", &Value::Null).status,
        401
    );
    assert_eq!(
        all_reports(&f),
        Vec::<Value>::new(),
        "open and closed alike"
    );
    assert!(kinds(&f, "aa").contains(&"reported".to_owned()));
    let bb = kinds(&f, "bb");
    for kind in ["reported", "report-resolved", "key-revoked", "locked"] {
        assert!(bb.contains(&kind.to_owned()), "{kind}: {bb:?}");
    }
    assert_eq!(
        call(f.addr(), "GET", "/v1/servers/bb", &[], "").status,
        404,
        "still locked"
    );
    for slug in ["aa", "bb"] {
        let log = f.running.registry().audit(slug).expect("the audit");
        assert!(
            log.as_array().is_some_and(|l| l
                .iter()
                .any(|e| e["kind"] == "reported" && e["details"]["reason"] == "spam")),
            "{log}"
        );
        let log = log.to_string();
        assert!(!log.contains(&bob_id), "{slug}: {log}");
    }
    // Signing in again starts a new account, with no reports.
    let again = sign_in(&f, "700");
    assert_ne!(operator_id(&f, &again), bob_id);
    assert_eq!(
        op(&f, &again, "GET", "/v1/operator/reports", &Value::Null).json()["reports"],
        json!([])
    );
    f.running.shutdown();
}

/// Spec: hub: forgetting a person whose claims were withdrawn or overtaken removes its account and
/// its claims; the world's owner still reads its events as `earlier-claimant`, and the hub's
/// command line no longer names it.
#[test]
fn forgetting_a_player_whose_claims_closed_removes_its_account() {
    let f = hub();
    listing(&f, "contested", 1);
    let ann = sign_in(&f, "711");
    let bob = sign_in(&f, "712");
    let cid = sign_in(&f, "713");
    let (bob_id, cid_id) = (operator_id(&f, &bob), operator_id(&f, &cid));
    let claim = |session: &str| {
        let r = op(
            &f,
            session,
            "POST",
            "/v1/operator/worlds/contested/claim",
            &Value::Null,
        );
        assert_eq!(r.status, 201, "{r:?}");
        r.json()
    };
    let kb = claim(&bob)["key"].as_str().expect("a key").to_owned();
    assert_eq!(
        game_verify(&f, "contested", &kb, "not the token", &[]).reason(),
        "wrong_token",
        "Bob's attempt"
    );
    claim(&cid);
    assert_eq!(
        op(
            &f,
            &cid,
            "DELETE",
            "/v1/operator/worlds/contested",
            &Value::Null
        )
        .status,
        204,
        "Cid withdraws"
    );
    let ca = claim(&ann);
    let (ka, ta) = (
        ca["key"].as_str().expect("a key").to_owned(),
        ca["verification"]["token"]
            .as_str()
            .expect("a token")
            .to_owned(),
    );
    assert_eq!(game_verify(&f, "contested", &ka, &ta, &[]).status, 200);
    // Bob's claim was overtaken by Ann's, and Cid's withdrawn: neither is an operator.
    for (who, session) in [("Bob", &bob), ("Cid", &cid)] {
        assert!(forget(&f, session), "{who}'s account went");
        assert_eq!(
            op(&f, session, "GET", "/v1/operator", &Value::Null).status,
            401,
            "{who}"
        );
    }
    let audit = op(
        &f,
        &ann,
        "GET",
        "/v1/operator/worlds/contested",
        &Value::Null,
    )
    .json()["audit"]
        .clone();
    let ann_id = operator_id(&f, &ann);
    let by = |kind: &str| -> Vec<String> {
        audit
            .as_array()
            .expect("the audit log")
            .iter()
            .filter(|e| e["kind"] == kind)
            .map(|e| e["actor"].as_str().expect("an actor").to_owned())
            .collect()
    };
    assert_eq!(
        by("claim-opened"),
        [ann_id.as_str(), "earlier-claimant", "earlier-claimant"]
    );
    assert_eq!(by("claim-withdrawn"), ["earlier-claimant"]);
    assert_eq!(
        by("verification-attempted"),
        [ann_id.as_str(), "earlier-claimant"]
    );
    let cli = f
        .running
        .registry()
        .audit("contested")
        .expect("the audit")
        .to_string();
    assert!(
        !cli.contains(&bob_id) && !cli.contains(&cid_id),
        "the command line names neither: {cli}"
    );
    f.running.shutdown();
}

/// Spec: hub: forgetting an operator, one that holds a world or has a claim open, removes its
/// links, its person friends and its setting, and keeps its account and its reports.
#[test]
fn forgetting_an_operator_keeps_its_account_and_its_reports() {
    let f = hub();
    let mut aa = world(&f, "aa");
    listing(&f, "listed", 2);
    let ann = sign_in(&f, "721");
    let (key, token) = register(&f, &ann, "annsworld", "127.0.0.1");
    assert_eq!(game_verify(&f, "annsworld", &key, &token, &[]).status, 200);
    let code = code_for(&mut aa, "a1", "Ann@aa");
    link_with(&f, &ann, &code);
    let dan = sign_in(&f, "722");
    assert_eq!(
        op(
            &f,
            &dan,
            "POST",
            "/v1/operator/worlds/listed/claim",
            &Value::Null
        )
        .status,
        201
    );
    for session in [&ann, &dan] {
        let r = op(
            &f,
            session,
            "POST",
            "/v1/reports",
            &json!({ "slug": "aa", "reason": "other", "details": "x" }),
        );
        assert_eq!(r.status, 201, "{r:?}");
    }
    for (who, session) in [("Ann, a world's", &ann), ("Dan, a claim's", &dan)] {
        assert!(!forget(&f, session), "{who} account stays");
        assert_eq!(
            op(&f, session, "GET", "/v1/operator", &Value::Null).status,
            200,
            "{who} session"
        );
        assert_eq!(
            op(&f, session, "GET", "/v1/operator/reports", &Value::Null).json()["reports"]
                .as_array()
                .map(Vec::len),
            Some(1),
            "{who} report"
        );
    }
    assert_eq!(all_reports(&f).len(), 2);
    let me = op(&f, &ann, "GET", "/v1/person", &Value::Null).json();
    assert_eq!(me["person"]["characters"], json!([]), "{me}");
    watch(&mut aa, "Ann@aa");
    assert_eq!(next_view(&mut aa, "Ann@aa"), (false, Alts::Hidden, vec![]));
    f.running.shutdown();
}
