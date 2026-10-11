//! Divergence: V448
//! Persons: a character linked to its player's Discord account with a code its world asks the hub
//! for, the alts setting, and friends: a linked character's friend is its person's, and the
//! characters it names are listed as the character's friends while it is online, come online as
//! their person, and are mutual friends through a person friend on each side; linking carries a
//! character's friends to its person and unlinking hands them back; nothing of a hidden person's
//! other characters reaches a world.
//! Fixture: three virtual-time worlds through the hub's routing (`super::Net`), with the hub's own
//! registry for persons; a player's sign-in on the site is the registry's, called directly.

use dereth_network_hub::operators::Operator;
use dereth_network_protocol::{Address, Message, PersonOp};
use dereth_primitives::ObjectId;
use dereth_protocol::social::{SocialFriendsUpdate, SocialRemoveFriend};
use empyrean_world::federation::{self, NetworkStore};
use empyrean_world::managers::player_manager;

use super::{add_friend, seed, Net, Seed, FRIENDS_UPDATE};

const ANN: u32 = 0x5000_0001;

fn ann() -> Seed {
    seed("ann", ANN, "Ann")
}

fn bob() -> Seed {
    seed("bob", 0x5000_0001, "Bob")
}

/// Ann on aa and Bob on bb, both on the network; Bob has a character on cc too, not yet in the
/// world.
fn worlds() -> Net {
    let mut net = Net::new(&[("aa", &[ann()]), ("bb", &[bob()]), ("cc", &[bob()])]);
    for slug in ["aa", "bb", "cc"] {
        net.link(slug);
    }
    net.enter("aa", ann());
    net.enter("bb", bob());
    net.say("aa", "Ann", "@dn on");
    net.say("bb", "Bob", "@dn on");
    net
}

/// The person of the Discord user `discord`, signed in on the site.
fn person(net: &Net, discord: &str) -> Operator {
    net.registry
        .1
        .signed_in(discord, &format!("Player {discord}"))
        .expect("signed in")
}

/// The code `who`'s world showed it after `/dn link`.
fn ask_code(net: &mut Net, slug: &str, who: &str) -> String {
    let m = net.mark(slug, who);
    net.say(slug, who, "@dn link");
    Net::lines(&net.since(slug, who, m))
        .iter()
        .find_map(|l| {
            l.strip_prefix("Your link code is ")
                .map(|r| r[..9].to_owned())
        })
        .expect("a code is shown")
}

/// `who` logs off `slug`, and is out of the world.
fn log_off(net: &mut Net, slug: &str, who: &str) {
    let id = net.world(slug).clients[who];
    net.world(slug).ts.client_mut(id).log_off();
    net.advance(12.0);
}

/// `op`'s person changed on the site: the hub sends again the views of its characters, and of
/// those naming one, that changed.
fn site_changed(net: &mut Net, op: &Operator) {
    let characters = net.registry.1.characters_of(op).expect("its characters");
    net.hub.persons_changed(&characters);
    net.advance(1.5);
}

/// Links `who` on `slug` to `discord`'s person as a player does: `/dn link` in game, then the code
/// on the site.
fn link(net: &mut Net, slug: &str, who: &str, discord: &str) {
    let code = ask_code(net, slug, who);
    let op = person(net, discord);
    net.registry
        .1
        .redeem_link_code(&op, &code)
        .expect("linked on the site");
    site_changed(net, &op);
}

/// `discord`'s person shows its alts, or hides them, from the site.
fn alts(net: &mut Net, discord: &str, shown: bool) {
    let op = person(net, discord);
    net.registry
        .1
        .set_alts_for(&op, shown)
        .expect("the setting");
    site_changed(net, &op);
}

fn updates(net: &mut Net, slug: &str, who: &str, mark: usize) -> Vec<SocialFriendsUpdate> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == FRIENDS_UPDATE)
        .map(|g| g.decode::<SocialFriendsUpdate>())
        .collect()
}

/// The rows of `kind` among `ups`, by name.
fn rows(ups: &[SocialFriendsUpdate], kind: u32) -> Vec<String> {
    ups.iter()
        .filter(|u| u.update_type == kind)
        .flat_map(|u| u.friends.iter().map(|f| f.name.clone()))
        .collect()
}

/// Ann's record's friends, and those a person friend listed, by identity key.
fn anns_friends(net: &mut Net) -> (Vec<String>, Vec<String>) {
    let r = net
        .world("aa")
        .ts
        .world
        .federation
        .record(ANN)
        .cloned()
        .unwrap_or_default();
    (
        r.friends.keys().cloned().collect(),
        r.from_person.keys().cloned().collect(),
    )
}

/// Bob, on bb and cc, shows his alts; Ann, linked, lists his person.
fn ann_lists_bob(net: &mut Net) {
    link(net, "aa", "Ann", "100");
    link(net, "bb", "Bob", "200");
    let code = {
        // Bob's character on cc, linked while he plays it, then logged off.
        net.enter("cc", bob());
        net.say("cc", "Bob", "@dn on");
        let code = ask_code(net, "cc", "Bob");
        log_off(net, "cc", "Bob");
        code
    };
    let op = person(net, "200");
    net.registry.1.redeem_link_code(&op, &code).expect("linked");
    alts(net, "200", true);
    net.say("aa", "Ann", "@dn friends add Bob@bb");
    net.advance(1.5);
}

/// Divergence: V451
#[test]
fn dn_link_asks_the_hub_for_a_code_for_the_character_and_shows_it_with_the_page() {
    let mut net = worlds();
    let mark = net.sent.0.len();
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn link");
    let asked: Vec<Message> = net.sent_since("aa", mark);
    assert!(
        asked.iter().any(|x| matches!(x, Message::Person { op: PersonOp::Code, who, .. } if who.to_string() == "Ann@aa")),
        "{asked:?}"
    );
    let lines = Net::lines(&net.since("aa", "Ann", m));
    let line = lines
        .iter()
        .find(|l| l.starts_with("Your link code is "))
        .expect("the code's line");
    assert!(
        line.contains("Within 10 minutes, sign in with Discord at https://site.test/characters/ and enter it there."),
        "{line}"
    );
    // Off the network, nothing is asked.
    net.say("aa", "Ann", "@dn off");
    let mark = net.sent.0.len();
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn link");
    assert!(!net
        .sent_since("aa", mark)
        .iter()
        .any(|x| matches!(x, Message::Person { .. })));
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        ["You are not on the Dereth Network. Type /dn on to join it."]
    );
}

/// Divergence: V451
#[test]
fn a_character_linked_on_the_site_while_online_is_told_and_its_status_says_so() {
    let mut net = worlds();
    let m = net.mark("aa", "Ann");
    link(&mut net, "aa", "Ann", "100");
    let lines = Net::lines(&net.since("aa", "Ann", m));
    assert!(
        lines.contains(&"This character is now linked to a Discord account.".to_owned()),
        "{lines:?}"
    );
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn");
    assert!(Net::lines(&net.since("aa", "Ann", m))[0]
        .ends_with("This character is linked to a Discord account."));
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn unlink");
    net.advance(1.5);
    let m2 = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn unlink");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m))[0],
        "This character is no longer linked to a Discord account."
    );
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m2)),
        ["This character is not linked to a Discord account."]
    );
}

/// Divergence: V448
#[test]
fn a_person_friends_characters_are_listed_watched_and_gone_at_logout() {
    let mut net = worlds();
    let m = net.mark("aa", "Ann");
    let mark = net.sent.0.len();
    ann_lists_bob(&mut net);
    assert!(Net::lines(&net.since("aa", "Ann", m))
        .contains(&"Bob@bb has been added to your friends list.".to_owned()));
    let added = rows(&updates(&mut net, "aa", "Ann", m), 1);
    assert_eq!(
        added,
        ["Bob@bb", "Bob@cc"],
        "rows for both, the handle first"
    );
    let watched: Vec<String> = net
        .sent_since("aa", mark)
        .iter()
        .filter_map(|x| match x {
            Message::Watch {
                target, on: true, ..
            } => Some(target.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        watched.contains(&"Bob@bb".to_owned()) && watched.contains(&"Bob@cc".to_owned()),
        "{watched:?}"
    );
    assert_eq!(
        anns_friends(&mut net),
        (
            vec!["bob@bb".to_owned(), "bob@cc".to_owned()],
            vec!["bob@bb".to_owned(), "bob@cc".to_owned()]
        )
    );
    log_off(&mut net, "aa", "Ann");
    assert_eq!(anns_friends(&mut net), (vec![], vec![]), "gone at logout");
    // Back in the world, the hub's view lists them again.
    net.enter("aa", ann());
    net.advance(1.5);
    assert_eq!(anns_friends(&mut net).1, ["bob@bb", "bob@cc"]);
}

/// Divergence: V448
#[test]
fn a_character_listed_through_person_friends_comes_online_as_its_person_and_is_mutual() {
    let mut net = worlds();
    ann_lists_bob(&mut net);
    // Bob lists Ann's person from bb; his alts are shown, so it holds on cc too.
    net.say("bb", "Bob", "@dn friends add Ann@aa");
    net.advance(1.5);
    let m = net.mark("aa", "Ann");
    net.enter("cc", bob());
    net.advance(6.0);
    let lines = Net::lines(&net.since("aa", "Ann", m));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Bob@bb is online as <Tell:IIDString:")
                && l.ends_with(":Bob@cc>Bob@cc<\\Tell>.")),
        "{lines:?}"
    );
    let ups = updates(&mut net, "aa", "Ann", m);
    assert!(
        ups.iter().any(|u| u.update_type == 4
            && u.friends[0].name == "Bob@cc"
            && u.friends[0].online == 1),
        "{ups:?}"
    );
}

/// Divergence: V448
#[test]
fn a_character_the_view_no_longer_names_leaves_the_friends_panel() {
    let mut net = worlds();
    ann_lists_bob(&mut net);
    let mark = net.sent.0.len();
    let m = net.mark("aa", "Ann");
    alts(&mut net, "200", false);
    assert_eq!(rows(&updates(&mut net, "aa", "Ann", m), 2), ["Bob@cc"]);
    assert!(net.sent_since("aa", mark).iter().any(|x| matches!(
        x,
        Message::Watch { target, on: false, .. } if target.to_string() == "Bob@cc"
    )));
    assert_eq!(
        anns_friends(&mut net),
        (vec!["bob@bb".to_owned()], vec!["bob@bb".to_owned()])
    );
}

/// Divergence: V448
#[test]
fn removing_a_row_a_person_friend_listed_removes_the_person_friend() {
    let mut net = worlds();
    ann_lists_bob(&mut net);
    let id = net
        .world("aa")
        .ts
        .world
        .federation
        .store
        .stand_ins
        .get(&Address::parse("Bob@cc").expect("an address"))
        .expect("Bob@cc's id");
    let m = net.mark("aa", "Ann");
    let mark = net.sent.0.len();
    let client = net.world("aa").clients["Ann"];
    net.world("aa").ts.send_game_action(
        client,
        &SocialRemoveFriend {
            friend_id: ObjectId(id),
        },
    );
    net.advance(2.0);
    assert!(net.sent_since("aa", mark).iter().any(|x| matches!(
        x,
        Message::Person { op: PersonOp::Unfriend, target: Some(t), .. } if t.to_string() == "Bob@bb"
    )));
    assert_eq!(
        rows(&updates(&mut net, "aa", "Ann", m), 2),
        ["Bob@cc", "Bob@bb"]
    );
    assert_eq!(anns_friends(&mut net), (vec![], vec![]));
}

/// Divergence: V448
#[test]
fn friends_listed_through_persons_are_never_kept_past_a_restart() {
    let mut net = worlds();
    ann_lists_bob(&mut net);
    let saved: NetworkStore = net.world("aa").ts.world.federation.store.clone();
    assert!(!saved.characters[&ANN].from_person.is_empty());
    let w = net.world("aa");
    let settings = w.ts.world.federation.settings.clone();
    federation::configure(&mut w.ts.world, settings, saved);
    assert_eq!(anns_friends(&mut net), (vec![], vec![]));
}

/// Divergence: V448
#[test]
fn dn_off_takes_the_rows_a_person_friend_listed_out_of_the_panel() {
    let mut net = worlds();
    ann_lists_bob(&mut net);
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn off");
    assert_eq!(
        rows(&updates(&mut net, "aa", "Ann", m), 2),
        ["Bob@bb", "Bob@cc"]
    );
    assert_eq!(anns_friends(&mut net), (vec![], vec![]));
}

/// Divergence: V448
#[test]
fn a_linked_characters_friend_is_its_persons_and_an_unlinked_ones_its_own() {
    let mut net = worlds();
    let mark = net.sent.0.len();
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    assert!(
        !net.sent_since("aa", mark)
            .iter()
            .any(|x| matches!(x, Message::Person { .. })),
        "unlinked: nothing is asked of the hub"
    );
    assert_eq!(
        anns_friends(&mut net),
        (vec!["bob@bb".to_owned()], vec![]),
        "kept in its record"
    );
    let mut net = worlds();
    link(&mut net, "aa", "Ann", "100");
    let mark = net.sent.0.len();
    let m = net.mark("aa", "Ann");
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    net.advance(1.5);
    assert!(net.sent_since("aa", mark).iter().any(|x| matches!(
        x,
        Message::Person { op: PersonOp::Befriend, target: Some(t), .. } if t.to_string() == "Bob@bb"
    )));
    assert_eq!(rows(&updates(&mut net, "aa", "Ann", m), 1), ["Bob@bb"]);
    assert_eq!(
        anns_friends(&mut net),
        (vec!["bob@bb".to_owned()], vec!["bob@bb".to_owned()]),
        "the person's, listed through the view"
    );
    let anns = person(&net, "100");
    assert_eq!(
        net.registry.1.person_json(&anns).expect("the person")["friends"][0]["handle"],
        "Bob@bb"
    );
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends add Bob@bb");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        ["That character is already in your friends list"]
    );
}

/// Divergence: V448
#[test]
fn a_friend_whose_person_hides_its_alts_is_only_the_character_added() {
    let mut net = worlds();
    link(&mut net, "aa", "Ann", "100");
    link(&mut net, "bb", "Bob", "200");
    net.enter("cc", bob());
    net.say("cc", "Bob", "@dn on");
    link(&mut net, "cc", "Bob", "200");
    log_off(&mut net, "cc", "Bob");
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends add Bob@bb");
    net.advance(1.5);
    assert_eq!(rows(&updates(&mut net, "aa", "Ann", m), 1), ["Bob@bb"]);
    assert_eq!(
        anns_friends(&mut net),
        (vec!["bob@bb".to_owned()], vec!["bob@bb".to_owned()])
    );
    // Bob plays his other character: Ann sees nothing of it, as for a character friend.
    let m = net.mark("aa", "Ann");
    log_off(&mut net, "bb", "Bob");
    net.enter("cc", bob());
    net.advance(6.0);
    let lines = Net::lines(&net.since("aa", "Ann", m));
    assert!(
        !lines.iter().any(|l| l.contains("Bob@cc")),
        "nothing of Bob@cc: {lines:?}"
    );
    assert!(
        rows(&updates(&mut net, "aa", "Ann", m), 1).is_empty(),
        "no row for Bob@cc"
    );
}

/// Divergence: V448
#[test]
fn linking_carries_a_characters_friends_to_its_person_and_lists_each_once() {
    let mut net = worlds();
    add_friend(&mut net, "aa", "Ann", "Bob@bb");
    add_friend(&mut net, "aa", "Ann", "Zed@cc");
    let mark = net.sent.0.len();
    let m = net.mark("aa", "Ann");
    link(&mut net, "aa", "Ann", "100");
    net.advance(1.5);
    let carried: Vec<String> = net
        .sent_since("aa", mark)
        .iter()
        .filter_map(|x| match x {
            Message::Person {
                op: PersonOp::Carry,
                targets,
                ..
            } => Some(targets.iter().map(ToString::to_string).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(carried, ["Bob@bb", "Zed@cc"]);
    let anns = person(&net, "100");
    let friends = net.registry.1.person_json(&anns).expect("the person")["friends"].clone();
    assert_eq!(
        friends
            .as_array()
            .map(|f| f.iter().map(|x| x["handle"].clone()).collect::<Vec<_>>()),
        Some(vec![
            serde_json::json!("Bob@bb"),
            serde_json::json!("Zed@cc")
        ])
    );
    let ups = updates(&mut net, "aa", "Ann", m);
    assert!(
        rows(&ups, 1).is_empty() && rows(&ups, 2).is_empty(),
        "the rows stay as they were: {ups:?}"
    );
    assert_eq!(
        anns_friends(&mut net),
        (
            vec!["bob@bb".to_owned(), "zed@cc".to_owned()],
            vec!["bob@bb".to_owned(), "zed@cc".to_owned()]
        ),
        "the person's now, each listed once"
    );
    let saved: NetworkStore = net.world("aa").ts.world.federation.store.clone();
    let w = net.world("aa");
    let settings = w.ts.world.federation.settings.clone();
    federation::configure(&mut w.ts.world, settings, saved);
    assert_eq!(
        anns_friends(&mut net),
        (vec![], vec![]),
        "the world keeps none of them past a restart: the hub has them"
    );
}

/// Divergence: V448
#[test]
fn unlinking_hands_a_characters_friends_back_to_it() {
    let mut net = worlds();
    link(&mut net, "aa", "Ann", "100");
    net.say("aa", "Ann", "@dn friends add Bob@bb");
    net.advance(1.5);
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn unlink");
    net.advance(3.0);
    assert_eq!(
        anns_friends(&mut net),
        (vec!["bob@bb".to_owned()], vec![]),
        "its own again, kept in its record"
    );
    let ups = updates(&mut net, "aa", "Ann", m);
    assert!(rows(&ups, 2).is_empty(), "no row went: {ups:?}");
    // Unlinked on the site while offline: handed back when it next comes online.
    link(&mut net, "aa", "Ann", "100");
    net.advance(1.5);
    assert_eq!(anns_friends(&mut net).1, ["bob@bb"]);
    log_off(&mut net, "aa", "Ann");
    let anns = person(&net, "100");
    net.registry
        .1
        .unlink_for(&anns, "Ann@aa")
        .expect("unlinked on the site");
    site_changed(&mut net, &anns);
    net.enter("aa", ann());
    net.advance(1.5);
    assert_eq!(anns_friends(&mut net), (vec!["bob@bb".to_owned()], vec![]));
    log_off(&mut net, "aa", "Ann");
    net.enter("aa", ann());
    net.advance(1.5);
    assert_eq!(
        anns_friends(&mut net),
        (vec!["bob@bb".to_owned()], vec![]),
        "kept by its world from then on"
    );
}

/// Divergence: V451
#[test]
fn dn_alts_hides_the_persons_characters_and_shows_a_shown_persons_characters() {
    let mut net = worlds();
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn alts shown");
    net.advance(1.0);
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        ["Link this character first: type /dn link."]
    );
    link(&mut net, "aa", "Ann", "100");
    link(&mut net, "bb", "Bob", "100");
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn alts");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        ["Your characters are hidden: nobody learns that they are one person."]
    );
    // Only the person shows them, on the site: a world that asks is refused and given the page.
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn alts shown");
    net.advance(1.5);
    net.say("aa", "Ann", "@dn alts");
    net.advance(1.0);
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        [
            "Only you can show your characters as one person: sign in with Discord at https://site.test/characters/ and choose it there.",
            "Your characters are hidden: nobody learns that they are one person."
        ]
    );
    alts(&mut net, "100", true);
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn alts");
    net.advance(1.0);
    net.say("aa", "Ann", "@dn alts hidden");
    net.advance(1.5);
    net.say("aa", "Ann", "@dn alts");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", m)),
        [
            "Your characters, shown as one person: Ann@aa, Bob@bb.",
            "Your characters are hidden now: nobody learns that they are one person.",
            "Your characters are hidden: nobody learns that they are one person."
        ]
    );
}

/// Divergence: V448
#[test]
fn a_character_made_under_a_linked_characters_name_is_not_its_persons() {
    let ann_again = seed("ann2", 0x5000_0002, "Ann");
    let mut net = Net::new(&[("aa", &[ann(), ann_again]), ("bb", &[bob()])]);
    for slug in ["aa", "bb"] {
        net.link(slug);
    }
    net.enter("aa", ann());
    net.enter("bb", bob());
    net.say("aa", "Ann", "@dn on");
    net.say("bb", "Bob", "@dn on");
    link(&mut net, "aa", "Ann", "100");
    link(&mut net, "bb", "Bob", "200");
    let anns = person(&net, "100");
    net.say("aa", "Ann", "@dn friends add Bob@bb");
    net.advance(1.5);
    log_off(&mut net, "aa", "Ann");
    // Ann is deleted, and a new character takes her name: it never asked for a code.
    player_manager::process_deleted_player(&mut net.world("aa").ts.world, ANN);
    net.enter("aa", ann_again);
    let m = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn on");
    net.advance(1.5);
    net.say("aa", "Ann", "@dn alts");
    let lines = Net::lines(&net.since("aa", "Ann", m));
    assert!(
        lines.contains(
            &"This character is not linked to a Discord account. Type /dn link to link it."
                .to_owned()
        ),
        "{lines:?}"
    );
    assert!(
        rows(&updates(&mut net, "aa", "Ann", m), 1).is_empty(),
        "none of the earlier Ann's person friends is listed"
    );
    let person_now = net.registry.1.person_json(&anns).expect("the person");
    assert_eq!(
        person_now["characters"],
        serde_json::json!([]),
        "the hub no longer links the name to the earlier Ann's person"
    );
}

/// Divergence: V448
#[test]
fn a_world_never_hears_of_a_hidden_persons_other_characters() {
    let mut net = worlds();
    link(&mut net, "aa", "Ann", "100");
    link(&mut net, "bb", "Bob", "200");
    net.enter("cc", bob());
    net.say("cc", "Bob", "@dn on");
    // Ann's person links Bob@cc too: two characters, one person, alts hidden.
    link(&mut net, "cc", "Bob", "100");
    log_off(&mut net, "cc", "Bob");
    let handed = net.handed.len();
    let m = net.mark("bb", "Bob");
    net.say("bb", "Bob", "@dn alts Ann@aa");
    net.advance(1.0);
    net.say("bb", "Bob", "@dn friends add Ann@aa");
    net.advance(1.5);
    let lines = Net::lines(&net.since("bb", "Bob", m));
    assert_eq!(lines[0], "Ann@aa shows no other characters.");
    for (slug, msg) in &net.handed[handed..] {
        assert!(
            slug != "bb" || !msg.to_json().to_lowercase().contains("bob@cc"),
            "bb heard of Ann's other character: {msg:?}"
        );
    }
    let watched: Vec<String> = net
        .sent
        .0
        .iter()
        .filter_map(|(s, x)| match x {
            Message::Watch { target, .. } if *s == "bb" => Some(target.key()),
            _ => None,
        })
        .collect();
    assert!(!watched.contains(&"bob@cc".to_owned()), "{watched:?}");
    alts(&mut net, "100", true);
    let m = net.mark("bb", "Bob");
    net.say("bb", "Bob", "@dn alts Ann@aa");
    net.advance(1.0);
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", m)),
        ["Ann@aa is one person with Bob@cc."]
    );
}

/// Divergence: V448
#[test]
fn a_view_is_taken_only_for_this_worlds_own_characters_on_the_network() {
    let mut net = worlds();
    // Ann asked to be linked, so a view of her own linked is hers.
    ask_code(&mut net, "aa", "Ann");
    let view = |to: &str| Message::PersonFriends {
        to: Address::parse(to).expect("an address"),
        linked: true,
        alts: dereth_network_protocol::Alts::Shown,
        persons: vec![dereth_network_protocol::PersonFriend {
            handle: Address::parse("Eve@bb").expect("an address"),
            characters: vec![Address::parse("Eve@bb").expect("an address")],
        }],
    };
    let m = net.mark("aa", "Ann");
    net.hub_hands("aa", &view("Ann@bb"));
    net.advance(0.5);
    assert!(
        rows(&updates(&mut net, "aa", "Ann", m), 1).is_empty(),
        "another world's Ann"
    );
    net.say("aa", "Ann", "@dn off");
    let m = net.mark("aa", "Ann");
    net.hub_hands("aa", &view("Ann@aa"));
    net.advance(0.5);
    assert!(
        rows(&updates(&mut net, "aa", "Ann", m), 1).is_empty(),
        "Ann off the network"
    );
    assert_eq!(anns_friends(&mut net), (vec![], vec![]));
    net.say("aa", "Ann", "@dn on");
    let m = net.mark("aa", "Ann");
    net.hub_hands("aa", &view("Ann@aa"));
    net.advance(0.5);
    assert_eq!(rows(&updates(&mut net, "aa", "Ann", m), 1), ["Eve@bb"]);
}

/// Divergence: V448
#[test]
fn clearing_the_friends_list_removes_every_person_friend_however_many() {
    let mut net = worlds();
    link(&mut net, "aa", "Ann", "100");
    let names = [
        "Ann", "Bea", "Cy", "Dee", "Eve", "Fay", "Gus", "Hal", "Ivy", "Jo", "Kit", "Lu",
    ];
    for name in names {
        net.say("aa", "Ann", &format!("@dn friends add {name}@bb"));
        // Within the character's pace of requests.
        net.advance(2.5);
    }
    assert_eq!(anns_friends(&mut net).1.len(), names.len());
    let mark = net.sent.0.len();
    let ann = net.world("aa").clients["Ann"];
    net.world("aa")
        .ts
        .send_game_action(ann, &dereth_protocol::social::SocialClearFriends);
    net.advance(2.0);
    let unfriended = net
        .sent_since("aa", mark)
        .iter()
        .filter(|x| {
            matches!(
                x,
                Message::Person {
                    op: PersonOp::Unfriend,
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        unfriended,
        names.len(),
        "every person friend goes, past the pace"
    );
    assert_eq!(
        anns_friends(&mut net),
        (vec![], vec![]),
        "and none comes back"
    );
}

/// Divergence: V448
#[test]
fn a_person_view_brings_only_its_worlds_share_of_new_stand_in_ids() {
    let mut net = worlds();
    // Ann asked to be linked, so a view of her own linked is hers.
    ask_code(&mut net, "aa", "Ann");
    // Five person friends of 20 characters each: a full view of 100 characters, all new on bb.
    let view = |from: u32| Message::PersonFriends {
        to: Address::parse("Ann@aa").expect("an address"),
        linked: true,
        alts: dereth_network_protocol::Alts::Shown,
        persons: (0..5)
            .map(|p| {
                let characters: Vec<Address> = (0..20)
                    .map(|c| {
                        Address::parse(&format!("{}@bb", super::friends::minted(from + p * 20 + c)))
                            .expect("an address")
                    })
                    .collect();
                dereth_network_protocol::PersonFriend {
                    handle: characters[0].clone(),
                    characters,
                }
            })
            .collect(),
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let budget = empyrean_world::federation::NEW_STAND_INS_PER_WORLD.burst as usize;
    assert_eq!(budget, 200);
    let listed = |net: &mut Net, from: u32| {
        let m = net.mark("aa", "Ann");
        net.hub_hands("aa", &view(from));
        net.advance(0.5);
        rows(&updates(net, "aa", "Ann", m), 1).len()
    };
    assert_eq!(listed(&mut net, 0), 100);
    assert_eq!(listed(&mut net, 100), 100, "bb's share, spent");
    assert_eq!(
        listed(&mut net, 200),
        0,
        "no new id past bb's share: none of these is listed"
    );
    assert_eq!(anns_friends(&mut net), (vec![], vec![]));
    assert_eq!(
        listed(&mut net, 0),
        100,
        "characters that already have an id are listed still"
    );
}
