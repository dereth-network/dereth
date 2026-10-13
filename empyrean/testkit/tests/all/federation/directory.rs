//! Divergence: V451
//! The network's directory, end to end through the hub's own registry: a character on the network
//! has its profile published with its page, and `/dn off` takes it out at once; an allegiance
//! under a key of the world's own with its name only when its monarch named it, a patron named once
//! it is on the network, a character taken out when it is deleted, renamed or taken down by the
//! hub's operator, what the hub has not answered kept across a restart, and the age, which grows as
//! the character plays, sent only with another change or at logout.
//! Fixture: virtual-time worlds through the hub's routing and its registry (`super::Net`).

use empyrean_entity::enums::{AccessLevel, CharacterOption, PropertyInt, PropertyString};
use empyrean_entity::ObjectGuid;
use empyrean_world::federation::{self, directory, FederationSettings, NetworkStore, RoomSlot};
use empyrean_world::managers::player_manager;
use empyrean_world::world_objects::{player_allegiance, player_character};
use serde_json::Value;

use super::{seed, Net, Seed};

const ANN: u32 = 0x5000_0001;
const CAT: u32 = 0x5000_0003;

fn ann() -> Seed {
    seed("ann", ANN, "Ann")
}

fn cat() -> Seed {
    seed("cat", CAT, "Cat")
}

/// Gives `guid` on `slug` what character creation gives a player: a level, a heritage, a gender.
fn made(net: &mut Net, slug: &str, guid: u32, level: i32) {
    let o = net
        .world(slug)
        .ts
        .world
        .objects
        .get_mut(ObjectGuid::new(guid))
        .expect("in the world");
    o.set_property(PropertyInt::Level, level);
    o.set_property(PropertyInt::HeritageGroup, 1);
    o.set_property(PropertyInt::Gender, 2);
}

/// Ann and Cat on aa, linked and in the world, neither on the network yet.
fn world() -> Net {
    let mut net = Net::new(&[("aa", &[ann(), cat()])]);
    net.link("aa");
    net.enter("aa", ann());
    net.enter("aa", cat());
    made(&mut net, "aa", ANN, 12);
    made(&mut net, "aa", CAT, 50);
    net
}

/// The directory's public answer for `address`, if it lists it.
fn page(net: &Net, address: &str) -> Option<Value> {
    net.registry
        .1
        .directory_character(address)
        .expect("the registry answers")
        .map(|v| v["character"].clone())
}

/// Ann swears allegiance to Cat.
fn swear(net: &mut Net) {
    player_allegiance::swear_allegiance(
        &mut net.world("aa").ts.world,
        ObjectGuid::new(ANN),
        CAT,
        true,
        true,
    );
    net.advance(0.5);
}

/// Divergence: V451
#[test]
fn a_character_on_the_network_is_published_at_once_with_its_page_and_dn_off_takes_it_out() {
    let mut net = world();
    net.advance(0.3);
    assert!(
        page(&net, "Ann@aa").is_none(),
        "off the network, not listed"
    );
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    let lines = Net::lines(&net.since("aa", "Ann", m));
    let url = "https://dereth.network/directory/character/?a=Ann%40aa";
    assert!(
        lines.contains(&format!("Your directory page: {url}")),
        "the page is shown: {lines:?}"
    );
    let p = page(&net, "Ann@aa").expect("published at once");
    assert_eq!(
        (&p["level"], &p["heritage"], &p["gender"]),
        (&Value::from(12), &Value::from(1), &Value::from(2))
    );
    assert_eq!(p["attributes"]["strength"]["innate"], 100);
    assert!(p["vitals"]["health"]["base"].as_u64().is_some());

    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m))[1],
        format!("Your directory page: {url}")
    );

    net.say("aa", "Ann", "@dn off");
    net.advance(0.3);
    assert!(page(&net, "Ann@aa").is_none(), "out at once");
}

/// Divergence: V451
#[test]
fn an_allegiance_goes_under_a_key_of_the_worlds_own_with_its_name_only_once_its_monarch_names_it() {
    let mut net = world();
    swear(&mut net);
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    let p = page(&net, "Ann@aa").expect("listed");
    let key = p["allegiance"]["key"].as_str().expect("a key").to_owned();
    assert_eq!(key.len(), 32, "{key}");
    assert!(
        !key.contains(&CAT.to_string()) && !key.contains(&format!("{CAT:08x}")),
        "the key is no game id: {key}"
    );
    assert_eq!(p["allegiance"]["members"], 2);
    assert!(
        p["allegiance"].get("name").is_none(),
        "an unnamed allegiance has no name, not its monarch's: {p}"
    );

    // The monarch names it: the name goes with the next listing, under the same key.
    let w = &mut net.world("aa").ts.world;
    let ag = *w
        .allegiance_manager
        .players
        .get(&ObjectGuid::new(ANN))
        .expect("Ann's allegiance is loaded");
    w.objects.get_mut(ag).expect("the allegiance").set_property(
        PropertyString::AllegianceName,
        "Order of the Dawn".to_owned(),
    );
    net.advance(400.0);
    let p = page(&net, "Ann@aa").expect("listed");
    assert_eq!(p["allegiance"]["name"], "Order of the Dawn");
    assert_eq!(p["allegiance"]["key"], key.as_str());
}

/// Divergence: V451
#[test]
fn a_patron_is_named_on_its_vassals_page_once_it_is_on_the_network() {
    let mut net = world();
    swear(&mut net);
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    let p = page(&net, "Ann@aa").expect("listed");
    assert!(p["allegiance"].get("patron").is_none(), "{p}");

    net.say("aa", "Cat", "@dn on");
    net.advance(400.0);
    let p = page(&net, "Ann@aa").expect("listed");
    assert_eq!(p["allegiance"]["patron"], "Cat@aa");
    assert_eq!(p["allegiance"]["monarch"], "Cat@aa");

    net.say("aa", "Cat", "@dn off");
    net.advance(0.3);
    let p = page(&net, "Ann@aa").expect("listed");
    assert!(
        p["allegiance"].get("patron").is_none(),
        "a patron off the network is never named: {p}"
    );
}

/// Divergence: V451
#[test]
fn a_listed_character_renamed_is_listed_under_its_new_name_and_one_deleted_is_taken_out() {
    let mut boss = seed("boss", 0x5000_0002, "Boss");
    boss.access = AccessLevel::Admin;
    let mut net = Net::new(&[("aa", &[ann(), boss, cat()])]);
    net.link("aa");
    for s in [ann(), boss, cat()] {
        net.enter("aa", s);
    }
    made(&mut net, "aa", ANN, 12);
    made(&mut net, "aa", CAT, 50);
    for who in ["Ann", "Cat"] {
        net.say("aa", who, "@dn on");
    }
    net.advance(0.3);
    assert!(page(&net, "Ann@aa").is_some() && page(&net, "Cat@aa").is_some());

    net.say("aa", "Boss", "@rename Ann, Annie");
    net.advance(12.0);
    assert!(page(&net, "Ann@aa").is_none(), "out under its old name");
    net.advance(40.0);
    assert!(
        page(&net, "Annie@aa").is_some(),
        "listed under its new name, still on the network"
    );

    let id = net.world("aa").clients["Cat"];
    net.world("aa").ts.client_mut(id).log_off();
    net.advance(12.0);
    player_manager::handle_player_delete(&mut net.world("aa").ts.world, CAT);
    net.advance(0.3);
    assert!(page(&net, "Cat@aa").is_none(), "out once deleted");
}

/// Divergence: V451
#[test]
fn a_character_the_hubs_operator_takes_down_is_told_once_and_sent_no_more_until_its_next_login() {
    let mut net = world();
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    assert!(net
        .registry
        .1
        .take_down("Ann@aa")
        .expect("the registry answers"));
    let m = net.mark("aa", "Ann");
    net.advance(6.0);
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        [directory::TAKEN_DOWN],
        "told at once"
    );
    let confirmed = net.listed.iter().any(|(s, m)| {
        s == "aa"
            && matches!(m, dereth_network_protocol::DirectoryMessage::Listing { who, on: false, .. }
                if who.to_string() == "Ann@aa")
    });
    assert!(confirmed, "the world confirms it has heard");
    let sent = listings_of(&net, "aa", "Ann@aa");
    made(&mut net, "aa", ANN, 13);
    net.advance(400.0);
    assert_eq!(
        listings_of(&net, "aa", "Ann@aa"),
        sent,
        "nothing more is sent while the character plays"
    );

    // Back on the network, its listing is refused while the hub holds it out, and it is told.
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn off");
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    let lines = Net::lines(&net.since("aa", "Ann", m));
    assert!(
        lines.contains(&directory::TAKEN_DOWN.to_owned()),
        "{lines:?}"
    );
    assert!(page(&net, "Ann@aa").is_none());
}

/// Divergence: V451
#[test]
fn a_listing_the_hub_has_not_answered_survives_a_restart_and_goes_after_the_welcome() {
    let mut net = world();
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    assert!(page(&net, "Ann@aa").is_some());
    net.unlink("aa");
    net.say("aa", "Ann", "@dn off");
    net.advance(0.3);
    assert!(page(&net, "Ann@aa").is_some(), "the hub has not heard yet");

    // The world stops and starts again from what it saved.
    let w = &mut net.world("aa").ts.world;
    let saved = serde_json::to_string(&w.federation.store).expect("the store saves");
    assert!(saved.contains("\"unlistings\""), "{saved}");
    let store: NetworkStore = serde_json::from_str(&saved).expect("the store loads");
    w.federation = federation::FederationState::default();
    federation::configure(
        w,
        FederationSettings {
            enabled: true,
            slug: "aa".into(),
            global_room: Some(RoomSlot::Roleplay),
            global_room_named: false,
            min_character_age_secs: 0,
        },
        store,
    );
    net.link("aa");
    net.advance(0.5);
    assert!(
        page(&net, "Ann@aa").is_none(),
        "taken out after the welcome"
    );
    assert!(
        net.world("aa")
            .ts
            .world
            .federation
            .store
            .unlistings
            .is_empty(),
        "kept only until answered"
    );
}

/// How many listings of `who` that list it `slug` has sent the hub.
fn listings_of(net: &Net, slug: &str, who: &str) -> usize {
    net.listed
        .iter()
        .filter(|(s, m)| {
            s == slug
                && matches!(m, dereth_network_protocol::DirectoryMessage::Listing { who: w, on: true, .. }
                    if w.to_string() == who)
        })
        .count()
}

/// Divergence: V451
#[test]
fn the_age_alone_goes_only_with_another_change_or_at_logout_and_its_option_turned_on_at_once() {
    let mut net = world();
    net.say("aa", "Ann", "@dn on");
    net.advance(0.3);
    assert!(
        page(&net, "Ann@aa").is_some_and(|p| p.get("age").is_none()),
        "the age's option is off"
    );
    let w = &mut net.world("aa").ts.world;
    let ann = ObjectGuid::new(ANN);
    w.objects
        .get_mut(ann)
        .expect("in the world")
        .set_property(PropertyInt::Age, 3_600_000);
    player_character::set_character_option(w, ann, CharacterOption::AllowOthersToSeeYourAge, true);
    net.advance(400.0);
    let shown = page(&net, "Ann@aa").and_then(|p| p["age"].as_u64());
    assert!(
        shown.is_some_and(|a| a >= 3_600_000),
        "turning the option on is a change: {shown:?}"
    );
    let sent = listings_of(&net, "aa", "Ann@aa");
    net.advance(1_200.0);
    assert_eq!(
        listings_of(&net, "aa", "Ann@aa"),
        sent,
        "the age growing alone sends nothing while the character plays"
    );
    let id = net.world("aa").clients["Ann"];
    net.world("aa").ts.client_mut(id).log_off();
    net.advance(12.0);
    let later = page(&net, "Ann@aa").and_then(|p| p["age"].as_u64());
    assert!(
        later > shown,
        "at logout the latest age goes: {later:?} after {shown:?}"
    );
}
