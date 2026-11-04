//! The operator's network commands at the console, and what is kept across a restart.

use dereth_network_protocol::Message;
use yalain_federation::{Federation, Settings, State, HELP, IN_GAME_HELP};
use yalain_store::Store;

use crate::{addr, World, SLUG};

fn lines(v: &[&str]) -> Vec<String> {
    v.iter().map(|&s| s.to_owned()).collect()
}

/// `dn` shows the world's link and what is blocked; `dn block` and `dn unblock` (in any case) with
/// a world or `Name@world` block or lift it for everyone on this world, and say so; a block of what is
/// blocked already, or a lift of what is not, says that; anything else is the help. With the
/// network off for the world, every command says so.
/// Behaviour: server.network.operator-console
#[test]
fn the_console_shows_the_link_and_the_blocks_and_blocks_and_lifts() {
    let mut off = World::with(false);
    assert_eq!(
        off.operator(&["block", "coldeve"]),
        lines(&["This world is not on the Dereth Network."])
    );
    let mut w = World::new();
    assert_eq!(
        w.operator(&[]),
        lines(&[
            "This world is on the Dereth Network as dawnsong. The hub is not connected. 0 worlds \
             are online. 0 messages are waiting for the hub. The Global room is carried in no \
             room. Rooms mirrored into Discord: none.",
            "Nothing is blocked.",
        ])
    );
    w.link(&[("coldeve", true), ("frost", false)]);
    assert_eq!(
        w.operator(&["block", "coldeve"]),
        lines(&["coldeve is blocked for everyone on this world."])
    );
    assert_eq!(
        w.operator(&["block", "Mal", "Dark@Frost"]),
        lines(&["mal dark@frost is blocked for everyone on this world."])
    );
    assert_eq!(
        w.operator(&["block", "COLDEVE"]),
        lines(&["coldeve was already blocked."])
    );
    assert_eq!(
        w.operator(&["unblock"]),
        lines(&[
            "This world is on the Dereth Network as dawnsong. The hub is connected. 1 world is \
             online. 0 messages are waiting for the hub. The Global room is carried in no room. \
             Rooms mirrored into Discord: none.",
            "Blocked: coldeve, mal dark@frost.",
        ])
    );
    assert_eq!(
        w.operator(&["UnBlock", "coldeve"]),
        lines(&["coldeve is no longer blocked."])
    );
    assert_eq!(
        w.operator(&["BLOCK", "coldeve"]),
        lines(&["coldeve is blocked for everyone on this world."])
    );
    assert_eq!(
        w.operator(&["unblock", "coldeve"]),
        lines(&["coldeve is no longer blocked."])
    );
    assert_eq!(
        w.operator(&["unblock", "coldeve"]),
        lines(&["coldeve was not blocked."])
    );
    assert_eq!(w.operator(&["block", "not a world!"]), lines(&[HELP[1]]));
    assert_eq!(w.operator(&["help"]), lines(HELP));
    assert_eq!(w.operator(&["frobnicate"]), lines(HELP));
}

/// A block takes hold at once: the world's (or character's) watchers and the friends there are
/// told this world's characters are offline, once; the friends there are no longer watched and
/// show offline here, as a state; and its watches are answered offline. Lifting it gives back
/// what it took: watched again, and told what each may see.
/// Behaviour: server.network.operator-block-at-once
#[test]
fn a_block_takes_hold_at_once_and_lifting_it_gives_back_what_it_took() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Bob@coldeve");
    w.lists("Ann", "Eve@frost");
    w.link(&[("coldeve", true), ("frost", true)]);
    w.frames();
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    w.hub(&Message::Watch {
        owner: addr("Hal@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    w.frames();
    w.service.states.clear();
    w.operator(&["block", "coldeve"]);
    let mut sent: Vec<String> = w.frames().iter().map(Message::to_json).collect();
    sent.sort();
    assert_eq!(
        sent,
        vec![
            r#"{"type":"presence","to":"Bob@coldeve","of":"Ann@dawnsong","online":false}"#,
            r#"{"type":"presence","to":"Hal@coldeve","of":"Ann@dawnsong","online":false}"#,
            r#"{"type":"watch","owner":"Ann@dawnsong","target":"Bob@coldeve","on":false}"#,
        ]
    );
    assert_eq!(w.service.states.len(), 1);
    assert!(
        !w.service.states[0].2 && w.service.states[0].3,
        "offline, a change from what it was shown"
    );
    // Its presence is no longer shown, and its watches are answered offline.
    w.service.states.clear();
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    assert!(w.service.states.is_empty());
    w.hub(&Message::Watch {
        owner: addr("Hal@coldeve"),
        target: addr("Ann@dawnsong"),
        on: true,
    });
    assert_eq!(
        w.frames().iter().map(Message::to_json).collect::<Vec<_>>(),
        vec![r#"{"type":"presence","to":"Hal@coldeve","of":"Ann@dawnsong","online":false}"#]
    );
    // A character arriving tells nothing to what is blocked.
    w.leave("Ann");
    w.frames();
    w.enter("Ann", State::Available);
    let arrived: Vec<String> = w.frames().iter().map(Message::to_json).collect();
    assert!(
        arrived.iter().all(|f| !f.contains("coldeve")),
        "{arrived:?}"
    );
    w.operator(&["unblock", "coldeve"]);
    let mut sent: Vec<String> = w.frames().iter().map(Message::to_json).collect();
    sent.sort();
    assert_eq!(
        sent,
        vec![
            r#"{"type":"presence","to":"Bob@coldeve","of":"Ann@dawnsong","online":true}"#,
            r#"{"type":"presence","to":"Hal@coldeve","of":"Ann@dawnsong","online":true}"#,
            r#"{"type":"watch","owner":"Ann@dawnsong","target":"Bob@coldeve","on":true}"#,
        ]
    );
}

/// The characters on the network and the operator's blocks are kept in the store, and a world
/// started again from it has them (a deleted character's setting left out).
/// Behaviour: server.network.setting-and-blocks-kept
#[test]
fn the_setting_and_the_blocks_are_kept_and_read_back_at_start() {
    let mut w = World::new();
    w.character("Ann");
    w.set_on("Ann", true);
    w.character("Cara");
    w.character("Dee");
    w.set_on("Dee", true);
    w.store.set_deleted(w.ids["Dee"], Some(5)).expect("deleted");
    w.operator(&["block", "coldeve"]);
    w.operator(&["block", "Mal@frost"]);
    let again = Federation::load(
        Settings {
            enabled: true,
            slug: SLUG.into(),
            software: "yalain test".into(),
            ..Settings::default()
        },
        1,
        &mut w.store,
    )
    .expect("read");
    assert!(again.is_on("ann") && !again.is_on("Cara") && !again.is_on("Dee"));
    assert_eq!(again.blocks(), vec!["coldeve", "mal@frost"]);
    w.set_on("Ann", false);
    assert_eq!(
        w.store.network_characters().expect("read"),
        vec![w.ids["Dee"]],
        "Ann is off; the deleted Dee is forgotten with its character"
    );
}

/// `dn admin block` is `dn block` as the game says it, at the console or by an admin in game; a name
/// may carry the mark a line said from Discord shows; `dn admin` alone is the help.
/// Behaviour: server.network.operator-admin-words
#[test]
fn the_operators_words_read_the_same_after_admin_and_in_game() {
    let mut off = World::with(false);
    assert_eq!(
        off.operator(&["admin", "block", "coldeve"]),
        lines(&["This world is not on the Dereth Network."])
    );
    let mut w = World::new();
    w.link(&[("coldeve", true)]);
    w.frames();
    assert_eq!(
        w.operator(&["ADMIN", "BLOCK", "coldeve"]),
        lines(&["coldeve is blocked for everyone on this world."])
    );
    assert_eq!(
        w.operator(&["admin", "unblock", "coldeve", "(Discord)"]),
        lines(&["coldeve is no longer blocked."])
    );
    assert_eq!(
        w.operator(&["admin", "block"]),
        w.operator(&[]),
        "with nothing after it, the state of the link and the blocks"
    );
    assert_eq!(w.operator(&["admin"]), lines(HELP));
    let said = w
        .fed
        .operator_said(&mut w.store, &mut w.service, 1, &["block", "Mal@Frost"]);
    assert_eq!(
        said,
        lines(&["mal@frost is blocked for everyone on this world."])
    );
    let help = w.fed.operator_said(&mut w.store, &mut w.service, 1, &[]);
    assert_eq!(help, lines(IN_GAME_HELP));
    assert!(help.iter().all(|l| l.starts_with("@dn ")), "{help:?}");
}

/// A world's `hello` names its WebSocket endpoint when it has one, and nothing when it has none.
/// Behaviour: server.network.websocket-url-told-to-the-hub
#[test]
fn the_hello_names_the_worlds_websocket_endpoint_when_it_has_one() {
    let hello_with = |websocket_url: Option<&str>| {
        let mut w = World::new();
        w.fed = Federation::new(
            Settings {
                enabled: true,
                slug: SLUG.into(),
                software: "yalain test".into(),
                websocket_url: websocket_url.map(str::to_owned),
                ..Settings::default()
            },
            1,
        );
        w.fed.on_link_up();
        w.frames()
    };
    let named = hello_with(Some("wss://play.example.org/ws"));
    assert!(
        matches!(named.as_slice(), [Message::Hello { websocket_url: Some(u), .. }] if u == "wss://play.example.org/ws"),
        "{named:?}"
    );
    let none = hello_with(None);
    assert!(
        matches!(
            none.as_slice(),
            [Message::Hello {
                websocket_url: None,
                ..
            }]
        ),
        "{none:?}"
    );
}
