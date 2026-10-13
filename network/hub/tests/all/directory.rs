//! The character directory on a running hub: a world lists its characters over its link, the
//! public routes answer at once, and a person, the hub's operator or a world leaving takes
//! characters out; what every kind of absence answers, and what the directory never says.
//! Fixture: the operator tests' hub (loopback, an in-memory registry, a stand-in for Discord), with
//! worlds registered on its command line and linked through the conformance suite's links.

use std::time::Duration;

use dereth_network_conformance::hub::Link;
use dereth_network_hub::registry::Registration;
use dereth_network_protocol::directory::{Allegiance, Refusal};
use dereth_network_protocol::{
    Address, DirectoryMessage, Federation, ListingOutcome, Message, OffBy, PersonOp, PersonOutcome,
    Profile,
};
use serde_json::{json, Value};

use super::operators::{call, hub, op, sign_in, Answer, Fixture, SITE};

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

fn profile(level: u32) -> Profile {
    Profile {
        level,
        heritage: 1,
        gender: 1,
        title: Some("the Bloodletter".into()),
        allegiance: Some(Allegiance {
            key: "dawn-key-1".into(),
            name: Some("Order of the Dawn".into()),
            members: 41,
            rank: 3,
            patron: None,
            monarch: None,
        }),
        ..Profile::default()
    }
}

/// `who`'s world lists it (`profile`) or takes it out (`None`): the hub's answer, and the page it
/// names.
fn list(
    link: &mut Link,
    id: &str,
    who: &str,
    profile: Option<Profile>,
) -> (ListingOutcome, Option<Refusal>, Option<String>) {
    link.send_raw(
        &DirectoryMessage::Listing {
            id: id.into(),
            who: a(who),
            on: profile.is_some(),
            profile: profile.map(Box::new),
        }
        .to_json(),
    )
    .expect("sent");
    let id = id.to_owned();
    link.expect_directory(
        Duration::from_secs(5),
        "the listing's answer",
        |m| match m {
            DirectoryMessage::ListingResult {
                id: i,
                outcome,
                reason,
                url,
            } if *i == id => Some((*outcome, *reason, url.clone())),
            _ => None,
        },
    )
    .expect("an answer")
}

/// The `listing.off` the hub tells `link` next, within `within`.
fn next_off(link: &mut Link, within: Duration) -> (String, OffBy) {
    link.expect_directory(within, "an off", |m| match m {
        DirectoryMessage::ListingOff { who, by } => Some((who.to_string(), *by)),
        _ => None,
    })
    .expect("an off")
}

fn get(f: &Fixture, path: &str) -> Answer {
    call(f.addr(), "GET", path, &[], "")
}

/// Links `who` to the person signed in with `session`, with a code its world asks for while it has
/// it on the network.
fn link_to(f: &Fixture, link: &mut Link, session: &str, who: &str) {
    super::persons::go_on(link, who);
    link.send(&Message::Person {
        id: format!("code-{who}"),
        op: PersonOp::Code,
        who: a(who),
        target: None,
        targets: Vec::new(),
        shown: None,
    })
    .expect("sent");
    let code = link
        .expect(Duration::from_secs(5), "a code", |m| match m {
            Message::PersonResult {
                outcome: PersonOutcome::Ok,
                code: Some(code),
                ..
            } => Some(code.clone()),
            _ => None,
        })
        .expect("a code");
    let r = op(
        f,
        session,
        "POST",
        "/v1/person/link",
        &json!({ "code": code }),
    );
    assert_eq!(r.status, 200, "{r:?}");
}

/// Where each of the person's characters stands in the directory, as its own route says.
fn states(f: &Fixture, session: &str) -> Vec<(String, String)> {
    op(f, session, "GET", "/v1/person", &Value::Null).json()["person"]["characters"]
        .as_array()
        .expect("characters")
        .iter()
        .map(|c| {
            (
                c["address"].as_str().expect("an address").to_owned(),
                c["listing"].as_str().expect("a state").to_owned(),
            )
        })
        .collect()
}

/// Spec: hub: a world's listing is published on the public routes at once, cacheable by anyone
/// for a minute, and taking it out takes it off them at once.
#[test]
fn a_listing_over_a_link_is_published_at_once_on_the_public_routes() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let (outcome, _, url) = list(&mut aa, "l1", "Sir Bob@aa", Some(profile(126)));
    assert_eq!(outcome, ListingOutcome::Ok);
    assert_eq!(
        url.as_deref(),
        Some(format!("{SITE}/directory/character/?a=Sir%20Bob%40aa").as_str())
    );
    let page = get(&f, "/v1/directory/characters/Sir%20Bob%40aa");
    assert_eq!(page.status, 200, "{page:?}");
    assert_eq!(page.header("cache-control"), Some("public, max-age=60"));
    assert_eq!(page.header("access-control-allow-origin"), Some("*"));
    let c = &page.json()["character"];
    assert_eq!(c["address"], "Sir Bob@aa");
    assert_eq!(c["level"], 126);
    assert_eq!(c["world"]["name"], "World aa");
    let found = get(&f, "/v1/directory/characters?q=bob&sort=level");
    assert_eq!(found.status, 200, "{found:?}");
    assert_eq!(found.json()["data"][0]["address"], "Sir Bob@aa");
    assert_eq!(found.json()["total"], 1);
    let members = get(&f, "/v1/directory/allegiances/aa/dawn-key-1");
    assert_eq!(members.status, 200, "{members:?}");
    assert_eq!(members.json()["allegiance"]["name"], "Order of the Dawn");
    let bad = get(&f, "/v1/directory/characters?q=b");
    assert_eq!(bad.status, 400);
    assert_eq!(
        list(&mut aa, "l2", "sir bob@aa", None).0,
        ListingOutcome::Ok
    );
    assert_eq!(
        get(&f, "/v1/directory/characters/Sir%20Bob%40aa").status,
        404,
        "gone at once"
    );
    assert_eq!(get(&f, "/v1/directory/characters?q=bob").json()["total"], 0);
    f.running.shutdown();
}

/// Spec: hub: every kind of absence answers the same `404`, and none is kept by a cache.
#[test]
fn every_kind_of_absence_answers_the_same_404() {
    let f = hub();
    let mut aa = world(&f, "aa");
    list(&mut aa, "l1", "Bob@aa", Some(profile(10)));
    list(&mut aa, "l2", "Bob@aa", None);
    list(&mut aa, "l3", "Eve@aa", Some(profile(10)));
    f.running
        .registry()
        .take_down("Eve@aa")
        .expect("taken down");
    // What an absence answers: its status, its body and what a cache may do with it.
    let absent = |path: &str| {
        let r = get(&f, &format!("/v1/directory/characters/{path}"));
        let cache = r.header("cache-control").map(str::to_owned);
        (r.status, r.body, cache)
    };
    let first = absent("Bob%40aa");
    assert_eq!(first.0, 404);
    assert_eq!(first.2.as_deref(), Some("no-store"));
    let answers: Vec<_> = [
        "Eve%40aa",
        "Nobody%40aa",
        "Ghost%40zz",
        "oldguard%40channel",
        "not%20an%20address",
    ]
    .iter()
    .map(|p| (*p, absent(p)))
    .collect();
    for (path, answer) in &answers {
        assert_eq!(answer, &first, "{path}");
    }
    f.running.shutdown();
}

/// Spec: hub: a person sees where its characters stand in the directory, and may not take one out
/// on the site: a character is listed while it is on the network, and off it while its world takes
/// it out. Forgetting the person takes every one of them off the network, and out at once.
#[test]
fn a_person_sees_its_characters_listings_and_cannot_take_them_out() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let mut bb = world(&f, "bb");
    let ann = sign_in(&f, "100");
    link_to(&f, &mut aa, &ann, "Ann@aa");
    link_to(&f, &mut bb, &ann, "Ann@bb");
    assert_eq!(
        states(&f, &ann),
        [
            ("Ann@aa".to_owned(), "listed".to_owned()),
            ("Ann@bb".to_owned(), "listed".to_owned())
        ]
    );
    list(&mut bb, "l2", "Ann@bb", None);
    assert_eq!(states(&f, &ann)[1].1, "off", "off the network");
    list(&mut bb, "l3", "Ann@bb", Some(profile(30)));
    assert_eq!(states(&f, &ann)[1].1, "listed");
    let unlisted = op(
        &f,
        &ann,
        "DELETE",
        "/v1/person/characters/Ann%40aa/listing",
        &Value::Null,
    );
    assert_eq!(unlisted.status, 404, "no such route: {unlisted:?}");
    let forgot = op(&f, &ann, "DELETE", "/v1/person", &Value::Null);
    assert_eq!(forgot.status, 204, "{forgot:?}");
    assert_eq!(get(&f, "/v1/directory/characters/Ann%40aa").status, 404);
    assert_eq!(get(&f, "/v1/directory/characters/Ann%40bb").status, 404);
    f.running.shutdown();
}

/// Spec: hub: forgetting a person tells each of its characters' worlds to take the character off
/// the network (`listing.off`, by its person): a linked world within seconds, one away at its next
/// link, each once a link until the world says it has heard.
#[test]
fn forgetting_a_person_tells_each_world_to_take_its_character_off() {
    let f = hub();
    let mut aa = world(&f, "aa");
    let mut bb = world(&f, "bb");
    let ann = sign_in(&f, "100");
    link_to(&f, &mut aa, &ann, "Ann@aa");
    link_to(&f, &mut bb, &ann, "Ann@bb");
    // bb is away when the person forgets itself: its link goes with a key turned over.
    let bb_key = f.running.registry().rotate_key("bb").expect("a new key");
    assert!(bb.closes_within(Duration::from_secs(10)));
    let forgot = op(&f, &ann, "DELETE", "/v1/person", &Value::Null);
    assert_eq!(forgot.status, 204, "{forgot:?}");
    assert_eq!(
        next_off(&mut aa, Duration::from_secs(10)),
        ("Ann@aa".to_owned(), OffBy::Person)
    );
    let relink = || {
        let mut bb = Link::open(&format!("ws://{}", f.addr()), "bb", &bb_key).expect("links");
        bb.greet().expect("welcomed");
        bb
    };
    let mut bb = relink();
    assert_eq!(
        next_off(&mut bb, Duration::from_secs(10)),
        ("Ann@bb".to_owned(), OffBy::Person),
        "at its next link"
    );
    list(&mut bb, "l1", "Ann@bb", None);
    bb.close();
    let mut bb = relink();
    assert!(
        bb.expect_directory(Duration::from_secs(7), "an off", |m| match m {
            DirectoryMessage::ListingOff { .. } => Some(()),
            _ => None,
        })
        .is_err(),
        "heard: not told again"
    );
    f.running.shutdown();
}

/// Spec: hub: the hub's operator takes a character down: gone at once, its world told within
/// seconds, its listings refused for the hold's days, and its world's log says so.
#[test]
fn the_operators_takedown_tells_the_world_and_holds_the_character_out() {
    let f = hub();
    let mut aa = world(&f, "aa");
    list(&mut aa, "l1", "Eve@aa", Some(profile(20)));
    let was = f
        .running
        .registry()
        .take_down("Eve@aa")
        .expect("taken down");
    assert!(was);
    assert_eq!(get(&f, "/v1/directory/characters/Eve%40aa").status, 404);
    assert_eq!(
        next_off(&mut aa, Duration::from_secs(10)),
        ("Eve@aa".to_owned(), OffBy::Operator)
    );
    list(&mut aa, "l2", "Eve@aa", None);
    assert_eq!(
        list(&mut aa, "l3", "Eve@aa", Some(profile(20))),
        (ListingOutcome::Refused, Some(Refusal::TakenDown), None)
    );
    let log = f.running.registry().audit("aa").expect("the log");
    assert!(log.to_string().contains("listing-taken-down"), "{log}");
    f.running.shutdown();
}

/// Spec: hub: a world that leaves the network takes its characters' listings off the public
/// routes at once.
#[test]
fn a_world_leaving_the_network_takes_its_listings_at_once() {
    let f = hub();
    let mut aa = world(&f, "aa");
    list(&mut aa, "l1", "Bob@aa", Some(profile(20)));
    assert_eq!(get(&f, "/v1/directory/characters/Bob%40aa").status, 200);
    f.running
        .registry()
        .set_federation("aa", Federation::Off)
        .expect("off");
    assert_eq!(get(&f, "/v1/directory/characters/Bob%40aa").status, 404);
    assert_eq!(
        get(&f, "/v1/directory/allegiances/aa/dawn-key-1").status,
        404
    );
    f.running.shutdown();
}

/// Spec: hub: searches have an allowance of their own, past which they are answered `429` with
/// when to try again; a character's page is not held to it.
#[test]
fn a_search_has_an_allowance_of_its_own() {
    let f = hub();
    let mut aa = world(&f, "aa");
    list(&mut aa, "l1", "Bob@aa", Some(profile(20)));
    let allowance = 30;
    // Searched until the first refusal: a slow machine may be given a search back on the way.
    let mut answered = 0;
    let over = loop {
        let r = get(&f, "/v1/directory/characters?q=bob");
        if r.status != 200 {
            break r;
        }
        answered += 1;
        assert!(answered <= allowance * 2, "searches are never refused");
    };
    assert!(
        answered >= allowance,
        "{answered} searches answered before the first refusal"
    );
    assert_eq!(over.status, 429);
    assert!(over.header("retry-after").is_some(), "{over:?}");
    assert_eq!(get(&f, "/v1/directory/characters/Bob%40aa").status, 200);
    f.running.shutdown();
}
