//! The admin model: the flag on an account, and characters made from the admin template. Fixture:
//! new players in the world, their accounts logged in with the harness's ticket.

use auberean_protocol::login::{CharacterCreated, CreateCharacter};
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_store::Store as _;
use yalain_testkit::Harness;
use yalain_world::admin::{self, Issuer};
use yalain_world::player::{
    ADMIN_ENTITY, ADMIN_PACKAGE, ADMIN_QUALITIES, PLAYER_ENTITY, PLAYER_PACKAGE, PLAYER_QUALITIES,
};

/// Run a console command: its answer.
fn console(h: &mut Harness, line: &str) -> Vec<String> {
    let c = admin::parse(line)
        .expect("a moderation command")
        .expect("it reads");
    let clock = h.clock();
    admin::run(&mut h.server, &mut h.net, clock, &Issuer::Console, &c)
}

/// A character asked for from `entity`, on the account of the bot at the character list: the answer.
fn ask(h: &mut Harness, b: usize, account: &str, name: &str, entity: u32) -> CharacterCreated {
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::CreateCharacter(CreateCharacter {
        account: NarrowString::from(account),
        name: WideString::from(name),
        entity_did: entity,
        start_area: 0x4000_0001,
        race: 1,
        sex: 0x1000,
        class: 2,
        implement: 1,
        physique: Table::from_inserted(SMALL, [(1, 1.0), (2, 1.0)]),
    });
    h.bots[b].send(false, queue::LOGON, &m);
    assert!(h.run_until(200, |h| h.bots[b].inbox.len() > before));
    match h.bots[b].inbox[before].message() {
        Some(ServerMessage::CharacterCreated(c)) => c,
        other => panic!("not a creation answer: {other:?}"),
    }
}

/// What a client was told of an object since `from`: its entity and package, the last told.
fn told_of(h: &Harness, b: usize, from: usize, id: u64) -> Option<(u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .rev()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == id => {
                Some((o.weenie.entity_did, o.weenie.package_id))
            }
            _ => None,
        })
}

/// The qualities record a client's description of itself named since `from`.
fn described(h: &Harness, b: usize, from: usize) -> Option<u32> {
    h.world_messages(b, from)
        .iter()
        .rev()
        .find_map(|m| match m {
            ServerMessage::PlayerQualities(q) => Some(q.0.did),
            _ => None,
        })
}

/// Make an account's first login hold a secret, then flag it.
fn raise(h: &mut Harness, account: &str) {
    console(h, &format!("set-accountpassword {account} ticket"));
    let done = console(h, &format!("set-accountadmin {account}"));
    assert!(done[0].ends_with("as an Admin."), "{done:?}");
}

/// A creation asking for the admin template, from a player's account, is answered with the client's
/// own line that the account may not, and makes nothing; the same from an admin's account makes a
/// character of that template. A player's account asking for the player's template is as ever.
/// Behaviour: server.admin.admin-characters-asked-for-only-by-admins
#[test]
fn a_creation_asking_for_the_admin_template_needs_an_admins_account() {
    let mut h = Harness::new();
    let b = h.at_character_list("one");
    let refused = ask(&mut h, b, "one", "Elara", ADMIN_ENTITY);
    assert_eq!(refused.response, CharacterCreated::ADMIN_REFUSED);
    assert_eq!(
        h.server.store.character_named("Elara").expect("reads"),
        None
    );

    raise(&mut h, "one");
    let made = ask(&mut h, b, "one", "Elara", ADMIN_ENTITY);
    assert_eq!(made.response, CharacterCreated::OK);
    let kept = h.server.store.character(made.identity.id).expect("reads");
    assert_eq!(kept.expect("kept").entity_did, ADMIN_ENTITY);
}

/// A character made an admin character at the console enters from the admin template (its object's
/// entity and package, its description's record), for its own client and for every other that sees
/// it; a character of a player's account cannot be made one; `off` makes it a player again at its
/// next entry, and its admin rights stop at once.
/// Behaviour: server.admin.admin-characters-enter-from-the-admin-template
#[test]
fn an_admin_character_enters_from_the_admin_template() {
    let mut h = Harness::new();
    let (other, _) = h.player_in_world("two", "Babyfare");
    let b = h.at_character_list("one");
    let id = h.create_character(b, "one", "Elara");
    assert_eq!(
        console(&mut h, "set-characteradmin Elara"),
        ["Elara's account is not an admin's: set-accountadmin first."]
    );
    raise(&mut h, "one");
    assert_eq!(
        console(&mut h, "set-characteradmin Elara"),
        ["Elara is now an admin character."]
    );
    let from = [h.bots[b].inbox.len(), h.bots[other].inbox.len()];
    h.enter_world(b, "one", id);
    h.run(10);
    assert_eq!(
        told_of(&h, b, from[0], id),
        Some((ADMIN_ENTITY, ADMIN_PACKAGE))
    );
    assert_eq!(described(&h, b, from[0]), Some(ADMIN_QUALITIES));
    assert_eq!(
        told_of(&h, other, from[1], id),
        Some((ADMIN_ENTITY, ADMIN_PACKAGE)),
        "another client sees the same object"
    );
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character == id && p.admin_character));
    assert_eq!(
        console(&mut h, "set-characteradmin Elara off"),
        ["Elara is no longer an admin character. It takes effect the next time it enters the world."]
    );
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character == id && !p.admin_character));
}

/// An admin character whose account is no longer an admin's enters as a player: the template follows
/// the account's flag at every entry, whatever is stored on the character.
/// Behaviour: server.admin.template-follows-the-accounts-flag
#[test]
fn an_admin_character_of_a_players_account_enters_as_a_player() {
    let mut h = Harness::new();
    let b = h.at_character_list("one");
    raise(&mut h, "one");
    let made = ask(&mut h, b, "one", "Elara", ADMIN_ENTITY);
    console(&mut h, "set-accountadmin one off");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", made.identity.id);
    h.run(5);
    assert_eq!(
        told_of(&h, b, from, made.identity.id),
        Some((PLAYER_ENTITY, PLAYER_PACKAGE))
    );
    assert_eq!(described(&h, b, from), Some(PLAYER_QUALITIES));
    assert!(h.server.players.values().all(|p| !p.admin_character));
}

/// An account with no secret cannot be raised: an open world gives such an account to the first
/// ticket it sees, so it would give away an admin's rights.
/// Behaviour: server.admin.no-admin-without-a-secret
#[test]
fn an_account_with_no_secret_cannot_be_made_an_admins() {
    let mut h = Harness::new();
    h.server
        .store
        .create_account("open", None, 1)
        .expect("made");
    let said = console(&mut h, "set-accountadmin open");
    assert!(said[0].contains("has no secret"), "{said:?}");
    let kept = h.server.store.account_by_name("open").expect("reads");
    assert!(!kept.expect("kept").admin);
}

/// An account with no secret and the admin flag, however it came to have them, is not claimed by the
/// first ticket on an open world, at the game's login or the friends service's; a player's account
/// with no secret takes the first ticket, as ever.
/// Behaviour: server.admin.no-claiming-an-admins-account
#[test]
fn an_admins_account_with_no_secret_is_not_claimed_by_the_first_ticket() {
    let mut h = Harness::new();
    let raised = h
        .server
        .store
        .create_account("raised", None, 1)
        .expect("made");
    h.server
        .store
        .set_account_admin(raised.id, true)
        .expect("set");
    h.server
        .store
        .create_account("open", None, 1)
        .expect("made");
    let unix = h.unix;
    assert_eq!(
        admin::ticket(&mut h.server, "raised", b"a stranger", "127.0.0.1", unix),
        admin::Ticket::Refused
    );
    let kept = h.server.store.account_by_name("raised").expect("reads");
    assert_eq!(kept.expect("kept").secret, None, "nothing kept for it");
    assert!(matches!(
        admin::ticket(&mut h.server, "open", b"the first", "127.0.0.1", unix),
        admin::Ticket::Admitted(_)
    ));
}
