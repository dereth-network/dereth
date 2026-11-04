//! Logging in, the character list, creating, deleting, entering the world and leaving it.

use auberean_protocol::login::{
    CharacterCreated, CharacterList, CharacterRef, CharacterRefused, CreateCharacter, DataHeld,
    DeleteCharacter, PlayCharacter, RestoreCharacter,
};
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};

use yalain_net::{fixed_stream, NetConfig, ServerNet, Service};
use yalain_store::{MemoryStore, Store as _};
use yalain_testkit::{Harness, WORLD_PORT};
use yalain_world::login::{account_for_command, add_account, PLAYER_ID_BASE, SECRET_COST};
use yalain_world::names::NameRules;
use yalain_world::{AccountPolicy, ServerConfig, Stage};

pub(super) const STEPS: usize = 400;

fn types(msgs: &[ServerMessage]) -> Vec<u32> {
    msgs.iter().map(ServerMessage::message_type).collect()
}

/// Log a new bot in and answer the data query; returns it once the character list is in.
pub(super) fn at_character_list(h: &mut Harness, account: &str) -> usize {
    at_character_list_from(h, account, Ipv4Addr::LOCALHOST)
}

/// [`at_character_list`] for a bot at `ip`.
fn at_character_list_from(h: &mut Harness, account: &str, ip: Ipv4Addr) -> usize {
    let b = h.add_bot_from(ip);
    let now = h.now;
    h.bots[b].log_in(account, b"ticket", now);
    assert!(h.run_until(STEPS, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    h.bots[b].send(false, queue::DATABASE, &held);
    assert!(h.run_until(STEPS, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::CharacterList(_)))));
    b
}

fn last_list(h: &Harness, b: usize) -> CharacterList {
    h.bots[b]
        .messages(false)
        .into_iter()
        .rev()
        .find_map(|m| {
            if let ServerMessage::CharacterList(l) = m {
                Some(l)
            } else {
                None
            }
        })
        .expect("a character list")
}

fn create(h: &mut Harness, b: usize, name: &str) -> CharacterCreated {
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::CreateCharacter(CreateCharacter {
        account: NarrowString::from("tester"),
        name: WideString::from(name),
        entity_did: 0x4700_110F,
        start_area: 0x4000_0001,
        race: 1,
        sex: 0x1000,
        class: 2,
        implement: 1,
        physique: Table::from_inserted(SMALL, [(1, 1.0), (2, 1.0), (4, 0.5), (0x100, 0.25)]),
    });
    h.bots[b].send(false, queue::LOGON, &m);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
    match h.bots[b].inbox[before].message() {
        Some(ServerMessage::CharacterCreated(c)) => c,
        other => panic!("not a creation answer: {other:?}"),
    }
}

fn enter(id: u64) -> ClientMessage {
    ClientMessage::PlayCharacter(PlayCharacter {
        character: id,
        account: NarrowString::from("tester"),
    })
}

/// A login is answered with the world's name and the data query; the client's answer with the
/// end of the download and the account's characters by name, then with their looks, each on the
/// queue the January 2017 login servers used.
/// Behaviour: server.login.answers-in-the-recorded-order
#[test]
fn a_login_is_answered_in_the_order_and_on_the_queues_of_2017() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let mut sent = h.bots[b].inbox.clone();
    sent.sort_by_key(|r| r.blob_id & 0xFFFF_FFFF);
    let order: Vec<(u16, u32)> = sent
        .iter()
        .map(|r| (r.queue, r.message().expect("reads").message_type()))
        .collect();
    assert_eq!(
        order,
        vec![
            (queue::EVENT, 0x100CE),
            (queue::DATABASE, 0x10037),
            (queue::DATABASE, 0x1003B),
            (queue::EVENT, 0x100B8),
            (queue::EVENT, 0x10055)
        ]
    );
    let list = last_list(&h, b);
    assert_eq!(
        (list.slots, list.characters.len(), list.uses_chat_service),
        (6, 0, 1)
    );
}

/// A character is made with an object id in the players' range and the look its choices give,
/// and is in the lists from then on.
/// Behaviour: server.characters.created-and-listed
#[test]
fn a_created_character_is_answered_with_its_identity_and_listed_after() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let c = create(&mut h, b, "Babyfare");
    assert_eq!(c.response, CharacterCreated::OK);
    assert_eq!(c.identity.id & 0xFFFF_FFFF_0000_0000, PLAYER_ID_BASE);
    assert_eq!(c.identity.name.text(), "Babyfare");
    assert_eq!(
        c.identity.visual.bits, 0x122,
        "parent, scale and the avatar's appearance"
    );
    // Logging in again (the account name in another case) lists it, with the same look.
    let b2 = at_character_list(&mut h, "TESTER");
    let list = last_list(&h, b2);
    assert_eq!(list.characters.len(), 1);
    assert_eq!(list.characters[0].id, c.identity.id);
    assert_eq!(list.characters[0].visual, c.identity.visual);
}

/// A name in use (in any case), a name with no letters, and a seventh character are refused with
/// their reasons and an empty identity.
/// Behaviour: server.characters.creation-refusals
#[test]
fn a_taken_or_bad_name_or_a_full_account_is_refused_with_its_reason() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    assert_eq!(create(&mut h, b, "Lugscrew").response, CharacterCreated::OK);
    let taken = create(&mut h, b, "lugSCREW");
    assert_eq!(taken.response, CharacterCreated::NAME_IN_USE);
    assert_eq!(taken.identity.id, 0);
    assert_eq!(
        create(&mut h, b, "-- 42 --").response,
        CharacterCreated::NAME_REFUSED
    );
    for name in ["Bea", "Cee", "Dee", "Eve", "Fay"] {
        assert_eq!(create(&mut h, b, name).response, CharacterCreated::OK);
    }
    assert_eq!(
        create(&mut h, b, "Gus").response,
        CharacterCreated::NO_FREE_SLOT
    );
}

/// A name is kept in its written form: trimmed (or, with nothing to trim, each run of spaces made
/// one), and each letter a capital after anything that is not a letter and small after a letter.
/// The identity answered and the list say it so.
/// Behaviour: server.characters.name-written-form
#[test]
fn a_name_is_kept_in_its_written_form() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    for (typed, kept) in [
        (" Hand Of Doom", "Hand Of Doom"),
        ("berndine", "Berndine"),
        ("SLASHER", "Slasher"),
        ("teal'k", "Teal'K"),
        ("a-jax", "A-Jax"),
        ("two  spaces", "Two Spaces"),
    ] {
        let c = create(&mut h, b, typed);
        assert_eq!(c.response, CharacterCreated::OK, "{typed:?}");
        assert_eq!(c.identity.name.text(), kept, "{typed:?}");
    }
    let again = at_character_list(&mut h, "tester");
    let listed: Vec<String> = last_list(&h, again)
        .characters
        .iter()
        .map(|c| c.name.text())
        .collect();
    assert!(listed.iter().any(|n| n == "Hand Of Doom"), "{listed:?}");
}

/// A name of two to sixteen English letters, spaces, apostrophes and hyphens, beginning and ending
/// with a letter and never with two spaces together in its written form, is a name; anything else
/// is refused.
/// Behaviour: server.characters.name-what-it-may-hold
#[test]
fn only_a_name_the_2017_servers_held_is_made() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    for name in ["Ab", "Captainpackspace", "Hugh' Jass", "Aed O'Brien"] {
        assert_eq!(
            create(&mut h, b, name).response,
            CharacterCreated::OK,
            "{name:?}"
        );
    }
    for name in [
        "A",
        "Captainpackspaces",
        "Sledge 9",
        "-Ashron",
        "Ashron'",
        " Two  Spaces",
        "J\u{fc}rgen",
        "Tab\tName",
        "",
    ] {
        assert_eq!(
            create(&mut h, b, name).response,
            CharacterCreated::NAME_REFUSED,
            "{name:?}"
        );
    }
}

/// With the operator's banned words, a name whose letters, small, match one of their patterns is
/// refused; spaces and marks between the letters do not hide the word. Without, the same name is
/// made.
/// Behaviour: server.characters.banned-names-from-the-operator
#[test]
fn a_name_holding_an_operators_banned_word_is_refused() {
    let mut h = Harness::new();
    h.server.names = NameRules::from_text("# words this server refuses\n*grim*\n\nmud*\n");
    assert_eq!(h.server.names.banned, ["*grim*", "mud*"]);
    let b = at_character_list(&mut h, "tester");
    for name in ["Ungrimly", "G-R'I M", "Mudlark"] {
        assert_eq!(
            create(&mut h, b, name).response,
            CharacterCreated::NAME_REFUSED,
            "{name:?}"
        );
    }
    assert_eq!(create(&mut h, b, "Larkmud").response, CharacterCreated::OK);

    let mut open = Harness::new();
    let b = at_character_list(&mut open, "tester");
    assert_eq!(
        create(&mut open, b, "Ungrimly").response,
        CharacterCreated::OK
    );
}

/// A deleted character asked back is answered as a made one, its grey gone and its look kept, and
/// can be entered; one not deleted is refused and changes nothing.
/// Behaviour: server.characters.deleted-brought-back
#[test]
fn a_deleted_character_asked_back_loses_its_grey() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let made = create(&mut h, b, "Babyfare").identity;
    let id = made.id;
    let restore = ClientMessage::RestoreCharacter(RestoreCharacter {
        character: id,
        name: WideString::from("Babyfare"),
        account: NarrowString::from("tester"),
    });

    let before = h.bots[b].inbox.len();
    h.bots[b].send(false, queue::LOGON, &restore);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
    let Some(ServerMessage::CharacterCreated(c)) = h.bots[b].inbox[before].message() else {
        panic!("an answer");
    };
    assert_ne!(c.response, CharacterCreated::OK, "a living character");

    let delete = ClientMessage::DeleteCharacter(DeleteCharacter {
        account: NarrowString::from("tester"),
        character: id,
    });
    let before = h.bots[b].inbox.len();
    h.bots[b].send(false, queue::LOGON, &delete);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() >= before + 3));

    let before = h.bots[b].inbox.len();
    h.bots[b].send(false, queue::LOGON, &restore);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
    let Some(ServerMessage::CharacterCreated(c)) = h.bots[b].inbox[before].message() else {
        panic!("an answer");
    };
    assert_eq!(c.response, CharacterCreated::OK);
    assert_eq!(c.identity.id, id);
    assert_eq!(c.identity.seconds_greyed_out, 0);
    assert_eq!(c.identity.name, made.name);
    assert_eq!(c.identity.visual, made.visual);
    let stored = h.server.store.character(id).unwrap().unwrap();
    assert_eq!(stored.deleted_at, None);
}

/// A deleted character is answered with its id and the lists again: gone from the list by name,
/// greyed out for an hour in the list with looks, then gone.
/// Behaviour: server.characters.deleted-greyed-for-an-hour
#[test]
fn a_deleted_character_stays_greyed_out_for_an_hour_then_is_gone() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare").identity.id;
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::DeleteCharacter(DeleteCharacter {
        account: NarrowString::from("tester"),
        character: id,
    });
    h.bots[b].send(false, queue::LOGON, &m);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() >= before + 3));
    let after: Vec<ServerMessage> = h.bots[b].inbox[before..]
        .iter()
        .filter_map(|r| r.message())
        .collect();
    assert_eq!(types(&after), vec![0x10029, 0x100B8, 0x10055]);
    let ServerMessage::CharacterNames(names) = &after[1] else {
        panic!("names")
    };
    assert!(names.ids.is_empty());
    let ServerMessage::CharacterList(list) = &after[2] else {
        panic!("list")
    };
    assert_eq!(list.characters.len(), 1);
    assert_eq!(list.characters[0].seconds_greyed_out, 3600);
    h.advance(3601.0);
    let b2 = at_character_list(&mut h, "tester");
    assert!(last_list(&h, b2).characters.is_empty());
}

/// Entering the world sends a referral to the world service; the client's world connection then
/// belongs to that character.
/// Behaviour: server.world.entered-by-referral
#[test]
fn entering_the_world_refers_the_client_to_the_world_service() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare").identity.id;
    h.bots[b].send(false, queue::LOGON, &enter(id));
    assert!(h.run_until(STEPS, |h| h.bots[b].in_world()));
    let r = h.bots[b].referrals[0];
    assert_eq!((r.addr.port(), r.id_server), (WORLD_PORT, 0x0B));
    assert!(h.run_until(STEPS, |h| h.server.sessions.values().any(
        |s| matches!(s.stage, Stage::InWorld { character, .. } if character == id)
    )));
    let world = *h.server.worlds.keys().next().expect("a world connection");
    assert_eq!(h.net.service(world), Some(Service::World));
}

/// A world that names another address for players from elsewhere, listening on every address,
/// refers each client to the address it reached the world at: one on the machine itself to
/// loopback, where it enters; one on the machine's home network to the machine's address there;
/// one from elsewhere to the named address.
/// Behaviour: server.net.nearby-clients-referred-where-they-reached-the-world
#[test]
fn each_client_is_referred_to_the_world_at_the_address_it_reached_the_login_by() {
    let mut h = Harness::new();
    let named = Ipv4Addr::new(203, 0, 113, 10);
    let own_on_lan = Ipv4Addr::new(192, 168, 1, 20);
    h.net = ServerNet::new(
        NetConfig::retail_ids(SocketAddrV4::new(named, WORLD_PORT)),
        fixed_stream(0x5EED),
        1.25e8,
        h.now,
    );
    // The machine's routes: loopback to itself, its home network's address to everyone else.
    h.net
        .refer_nearby(IpAddr::V4(Ipv4Addr::UNSPECIFIED), move |client| {
            Some(if client.is_loopback() {
                Ipv4Addr::LOCALHOST
            } else {
                own_on_lan
            })
        });
    let here = at_character_list_from(&mut h, "here", Ipv4Addr::LOCALHOST);
    let lan = at_character_list_from(&mut h, "neighbor", Ipv4Addr::new(192, 168, 1, 30));
    let away = at_character_list_from(&mut h, "traveler", Ipv4Addr::new(198, 51, 100, 7));
    let mut ids = Vec::new();
    for (b, name) in [
        (here, "Hearthkeeper"),
        (lan, "Neighbor"),
        (away, "Wayfarer"),
    ] {
        let id = create(&mut h, b, name).identity.id;
        h.bots[b].send(false, queue::LOGON, &enter(id));
        ids.push(id);
    }
    assert!(h.run_until(STEPS, |h| h.bots[here].in_world()
        && [lan, away].iter().all(|&b| !h.bots[b].referrals.is_empty())));
    let to = |b: usize| {
        let r = h.bots[b].referrals[0];
        (*r.addr.ip(), r.addr.port())
    };
    assert_eq!(
        to(here),
        (Ipv4Addr::LOCALHOST, WORLD_PORT),
        "on the machine"
    );
    assert_eq!(to(lan), (own_on_lan, WORLD_PORT), "on its network");
    assert_eq!(to(away), (named, WORLD_PORT), "from elsewhere");
    assert!(h.run_until(STEPS, |h| h.server.sessions.values().any(
        |s| matches!(s.stage, Stage::InWorld { character, .. } if character == ids[0])
    )));
}

/// A client on the world's own network that reached the login through the router (its login comes
/// from the router's address there) and is referred to the machine's address on that network goes
/// to the world straight across the network, from its own address: its cookie lets it in. A world
/// login with that cookie from outside the network, or with a cookie never sent, is not let in.
/// Behaviour: server.net.nearby-world-login-by-its-cookie
#[test]
fn a_client_on_the_worlds_network_enters_by_its_cookie_from_another_address_there() {
    let mut h = Harness::new();
    let named = Ipv4Addr::new(203, 0, 113, 10);
    let own_on_lan = Ipv4Addr::new(10, 21, 23, 20);
    h.net = ServerNet::new(
        NetConfig::retail_ids(SocketAddrV4::new(named, WORLD_PORT)),
        fixed_stream(0x5EED),
        1.25e8,
        h.now,
    );
    h.net
        .refer_nearby(IpAddr::V4(Ipv4Addr::UNSPECIFIED), move |_| Some(own_on_lan));
    // The router hands the login on from its own address on the network.
    let b = at_character_list_from(&mut h, "neighbor", Ipv4Addr::new(10, 21, 23, 1));
    h.bots[b].follow_referrals = false;
    let id = create(&mut h, b, "Neighbor").identity.id;
    h.bots[b].send(false, queue::LOGON, &enter(id));
    assert!(h.run_until(STEPS, |h| !h.bots[b].referrals.is_empty()));
    let referred = h.bots[b].referrals[0];
    assert_eq!(
        *referred.addr.ip(),
        own_on_lan,
        "referred to the machine on the network"
    );
    // The test's wire reaches the machine at loopback: the same referral, there.
    let r = dereth_transport::conn::Referral {
        addr: SocketAddrV4::new(Ipv4Addr::LOCALHOST, WORLD_PORT),
        ..referred
    };
    // From outside the network, the cookie does not let it in.
    let away = h.add_bot_from(Ipv4Addr::new(198, 51, 100, 7));
    h.bots[away].follow(r);
    h.run(60);
    assert!(!h.bots[away].in_world(), "not from outside the network");
    // A cookie never sent does not either.
    let stray = h.add_bot_from(Ipv4Addr::new(10, 21, 23, 157));
    h.bots[stray].follow(dereth_transport::conn::Referral {
        cookie: r.cookie ^ 1,
        ..r
    });
    h.run(60);
    assert!(!h.bots[stray].in_world(), "not with a cookie never sent");
    // The client itself, straight across the network from its own address.
    h.bots[b].addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 21, 23, 156), 49_156));
    h.bots[b].follow(r);
    assert!(
        h.run_until(STEPS, |h| h.bots[b].in_world()),
        "in the world by its cookie"
    );
    assert!(h.run_until(STEPS, |h| h.server.sessions.values().any(
        |s| matches!(s.stage, Stage::InWorld { character, .. } if character == id)
    )));
}

/// A character already in the world is refused a second entry (the reason the 2017 servers gave,
/// 11), and one not on the account another (13).
/// Behaviour: server.world.entry-refusals
#[test]
fn a_character_in_the_world_or_not_on_the_account_cannot_enter() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare").identity.id;
    h.bots[b].send(false, queue::LOGON, &enter(id));
    assert!(h.run_until(STEPS, |h| h.bots[b].in_world()));
    let b2 = at_character_list(&mut h, "tester");
    let before = h.bots[b2].inbox.len();
    h.bots[b2].send(false, queue::LOGON, &enter(id));
    assert!(h.run_until(STEPS, |h| h.bots[b2].inbox.len() > before));
    assert_eq!(
        h.bots[b2].inbox[before].message(),
        Some(ServerMessage::CharacterRefused(CharacterRefused {
            reason: 11
        }))
    );
    h.bots[b2].send(false, queue::LOGON, &enter(PLAYER_ID_BASE | 999));
    assert!(h.run_until(STEPS, |h| h.bots[b2].inbox.len() > before + 1));
    assert_eq!(
        h.bots[b2].inbox[before + 1].message(),
        Some(ServerMessage::CharacterRefused(CharacterRefused {
            reason: 13
        }))
    );
}

/// Leaving the world is answered with the character's exit and the lists again, and its world
/// connection ends.
/// Behaviour: server.world.left-to-the-character-list
#[test]
fn leaving_the_world_answers_the_exit_then_the_lists() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare").identity.id;
    h.bots[b].send(false, queue::LOGON, &enter(id));
    assert!(h.run_until(STEPS, |h| !h.server.worlds.is_empty()));
    let before = h.bots[b].inbox.iter().filter(|r| !r.world).count();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    assert!(h.run_until(STEPS, |h| h.bots[b]
        .inbox
        .iter()
        .filter(|r| !r.world)
        .count()
        >= before + 3));
    let after: Vec<u32> = h.bots[b]
        .inbox
        .iter()
        .filter(|r| !r.world)
        .skip(before)
        .filter_map(yalain_testkit::Received::message)
        .map(|m| m.message_type())
        .collect();
    assert_eq!(after, vec![0x10027, 0x100B8, 0x10055]);
    assert!(h.server.worlds.is_empty());
    assert!(
        h.run_until(STEPS, |h| !h.bots[b].in_world()),
        "the world connection was closed"
    );
}

/// The first login's reply: whether the bot reached the character list (its data query came), or
/// the refusal it was sent alone.
fn first_reply(h: &mut Harness, account: &str, ticket: &[u8]) -> Option<ServerMessage> {
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in(account, ticket, now);
    assert!(h.run_until(STEPS, |h| !h.bots[b].inbox.is_empty()));
    h.bots[b].inbox[0].message()
}

/// On an open world an account is made at its name's first login, with that ticket, and from then
/// on a login with another ticket is refused before the world's name; an account made with no
/// ticket takes the first one it logs in with.
/// Behaviour: server.accounts.open-made-at-first-login-then-need-their-ticket
#[test]
fn an_open_account_is_made_at_its_first_login_and_needs_that_ticket_after() {
    let mut store = MemoryStore::default();
    store.create_account("older", None, 1).expect("made");
    let mut h = Harness::with(ServerConfig::default(), store, yalain_testkit::test_rules());
    let refused = Some(ServerMessage::CharacterRefused(CharacterRefused {
        reason: 7,
    }));
    for account in ["newcomer", "older"] {
        let first = first_reply(&mut h, account, b"mine");
        assert_ne!(first, refused, "{account}: the first login is admitted");
        let again = first_reply(&mut h, &account.to_uppercase(), b"mine");
        assert_ne!(
            again, refused,
            "{account}: its own ticket again, in any case of its name"
        );
        let other = first_reply(&mut h, account, b"theirs");
        assert_eq!(other, refused, "{account}: another ticket is refused");
    }
}

/// A character the operator makes for an account that does not exist: an open world makes the
/// account, with no ticket, and it then logs in with the first ticket it is given and that one
/// only; a world of listed accounts makes none and says to make it with its ticket, and the
/// account made so is found.
/// Behaviour: server.accounts.made-for-a-command-can-log-in
#[test]
fn an_account_made_for_an_operators_character_can_log_in() {
    let mut listed = MemoryStore::default();
    let refused = account_for_command(AccountPolicy::Listed, &mut listed, "tester", 1);
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("--add-account tester:")),
        "{refused:?}"
    );
    assert_eq!(
        listed.account_by_name("tester").expect("reads"),
        None,
        "none made"
    );
    let made = add_account(&mut listed, "tester", b"ticket", SECRET_COST, 1).expect("made");
    let found = account_for_command(AccountPolicy::Listed, &mut listed, "Tester", 1);
    assert_eq!(found.map(|a| a.id), Ok(made.id));

    let mut open = MemoryStore::default();
    let made = account_for_command(AccountPolicy::Open, &mut open, "tester", 1).expect("made");
    assert_eq!(made.secret, None, "made with no ticket");
    let mut h = Harness::with(ServerConfig::default(), open, yalain_testkit::test_rules());
    let refused = Some(ServerMessage::CharacterRefused(CharacterRefused {
        reason: 7,
    }));
    assert_ne!(
        first_reply(&mut h, "tester", b"mine"),
        refused,
        "its first ticket"
    );
    assert_eq!(
        first_reply(&mut h, "tester", b"theirs"),
        refused,
        "and that one only"
    );
}

/// With listed accounts only, a login needs an account made beforehand and its ticket; any other
/// is refused before the world's name.
/// Behaviour: server.accounts.listed-need-their-ticket
#[test]
fn a_listed_account_logs_in_with_its_ticket_only() {
    let cfg = ServerConfig {
        accounts: AccountPolicy::Listed,
        ..ServerConfig::default()
    };
    let mut store = MemoryStore::default();
    add_account(&mut store, "tester", b"ticket", SECRET_COST, 1).expect("made");
    let mut h = Harness::with(cfg, store, yalain_testkit::test_rules());
    let good = at_character_list(&mut h, "tester");
    assert!(h.bots[good].logged_in());
    for (account, ticket) in [("tester", &b"wrong"[..]), ("nobody", &b"ticket"[..])] {
        let b = h.add_bot();
        let now = h.now;
        h.bots[b].log_in(account, ticket, now);
        assert!(h.run_until(STEPS, |h| !h.bots[b].inbox.is_empty()));
        assert_eq!(
            h.bots[b].inbox[0].message(),
            Some(ServerMessage::CharacterRefused(CharacterRefused {
                reason: 7
            }))
        );
        h.run(20);
        assert_eq!(h.bots[b].inbox.len(), 1, "nothing after the refusal");
    }
}
