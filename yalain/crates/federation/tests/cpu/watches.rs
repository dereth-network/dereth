//! Other worlds watching this world's characters.

use dereth_network_protocol::limits::{Limits, Rate};
use dereth_network_protocol::Message;
use yalain_federation::State;

use crate::{addr, World};

/// The presence answers a world sends now: (to, of, online).
fn presences(frames: &[Message]) -> Vec<(String, String, bool)> {
    frames
        .iter()
        .filter_map(|m| match m {
            Message::Presence { to, of, online } => Some((to.to_string(), of.to_string(), *online)),
            _ => None,
        })
        .collect()
}

fn watch(w: &mut World, owner: &str, target: &str, on: bool) {
    w.hub(&Message::Watch {
        owner: addr(owner),
        target: addr(target),
        on,
    });
}

fn p(to: &str, of: &str, online: bool) -> (String, String, bool) {
    (to.to_owned(), of.to_owned(), online)
}

/// A watch from another world is answered online only for a character on the network and present
/// (in the world with a friends stream), and `offline` alike for a character not present, one off
/// the network, a name this world does not have, and a watcher the operator blocks, each named as
/// the watch named it.
/// Behaviour: server.network.watch-answered
#[test]
fn a_watch_is_answered_online_only_for_a_present_character_on_the_network() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.character("Cara");
    w.enter("Cara", State::Available);
    w.character("Dee");
    w.set_on("Dee", true);
    w.link(&[("coldeve", true)]);
    w.frames();
    w.operator(&["block", "Mal@coldeve"]);
    for (owner, target) in [
        ("Bob@coldeve", "ann@dawnsong"),
        ("Bob@coldeve", "Cara@dawnsong"),
        ("Bob@coldeve", "Dee@dawnsong"),
        ("Bob@coldeve", "Nobody@dawnsong"),
        ("Mal@coldeve", "Ann@dawnsong"),
    ] {
        watch(&mut w, owner, target, true);
    }
    assert_eq!(
        presences(&w.frames()),
        vec![
            p("Bob@coldeve", "ann@dawnsong", true),
            p("Bob@coldeve", "Cara@dawnsong", false),
            p("Bob@coldeve", "Dee@dawnsong", false),
            p("Bob@coldeve", "Nobody@dawnsong", false),
            p("Mal@coldeve", "Ann@dawnsong", false),
        ]
    );
}

/// A kept watcher is told each change of what it may see: a character on the network leaving is
/// offline to it, coming back online, going off the network offline. Every friend on another
/// world the character lists is told too, watch or no watch. A watch stopped is told nothing more,
/// and a watcher the operator blocks is told nothing while the block lasts.
/// Behaviour: server.network.watchers-told-each-change
#[test]
fn a_kept_watcher_and_every_listed_friend_is_told_each_change() {
    let mut w = World::new();
    w.character("Dee");
    w.set_on("Dee", true);
    w.lists("Dee", "Fay@frost");
    w.link(&[("coldeve", true), ("frost", true)]);
    w.frames();
    watch(&mut w, "Bob@coldeve", "Dee@dawnsong", true);
    watch(&mut w, "Hal@coldeve", "Dee@dawnsong", true);
    watch(&mut w, "Mal@coldeve", "Dee@dawnsong", true);
    w.frames();
    assert_eq!(w.fed.watcher_count("dee"), 3);
    watch(&mut w, "Hal@coldeve", "Dee@dawnsong", false);
    w.operator(&["block", "Mal@coldeve"]);
    w.frames();
    w.enter("Dee", State::Available);
    let mut seen = presences(&w.frames());
    seen.sort();
    assert_eq!(
        seen,
        vec![
            p("Bob@coldeve", "Dee@dawnsong", true),
            p("Fay@frost", "Dee@dawnsong", true),
        ]
    );
    w.leave("Dee");
    let mut seen = presences(&w.frames());
    seen.sort();
    assert_eq!(
        seen,
        vec![
            p("Bob@coldeve", "Dee@dawnsong", false),
            p("Fay@frost", "Dee@dawnsong", false),
        ]
    );
    w.enter("Dee", State::Away(None));
    w.frames();
    w.set_on("Dee", false);
    let mut seen = presences(&w.frames());
    seen.sort();
    assert_eq!(
        seen,
        vec![
            p("Bob@coldeve", "Dee@dawnsong", false),
            p("Fay@frost", "Dee@dawnsong", false),
        ],
        "off the network, at once"
    );
    // Lifting the block gives back what it took: the watcher kept through it sees what it may.
    w.set_on("Dee", true);
    w.frames();
    w.operator(&["unblock", "Mal@coldeve"]);
    assert_eq!(
        presences(&w.frames()),
        vec![p("Mal@coldeve", "Dee@dawnsong", true)]
    );
}

/// A character keeps at most a hundred watchers from any one world and a thousand in all, but
/// always one it lists; a watch past the caps is still answered. Another world's watches are held
/// to its share of this world's pace (a quarter of the hub's messages per world): one past it is
/// neither answered nor kept. A world whose link goes takes its watchers with it.
/// Behaviour: server.network.watchers-capped
#[test]
fn watchers_are_capped_by_world_and_in_all_and_watches_paced_by_world() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Zed@coldeve");
    let limits = Limits {
        messages_per_world: Rate {
            burst: 40_000.0,
            per_sec: 0.0,
        },
        ..Limits::DEFAULT
    };
    let worlds: Vec<String> = (0..12).map(|n| format!("w{n:02}")).collect();
    let mut linked: Vec<(&str, bool)> = worlds.iter().map(|s| (s.as_str(), true)).collect();
    linked.push(("coldeve", true));
    w.link_with(&linked, limits);
    w.frames();
    for n in 0..150 {
        watch(
            &mut w,
            &format!("Bob{}@coldeve", letters(n)),
            "Ann@dawnsong",
            true,
        );
    }
    assert_eq!(w.fed.watcher_count("Ann"), 100);
    watch(&mut w, "Zed@coldeve", "Ann@dawnsong", true);
    assert_eq!(
        w.fed.watcher_count("Ann"),
        101,
        "a listed watcher is always kept"
    );
    let answered = presences(&w.frames()).len();
    assert_eq!(answered, 151, "every watch within the pace is answered");
    for world in &worlds {
        for n in 0..100 {
            watch(
                &mut w,
                &format!("Eve{}@{world}", letters(n)),
                "Ann@dawnsong",
                true,
            );
        }
    }
    assert_eq!(w.fed.watcher_count("Ann"), 1_000, "a thousand in all");
    w.hub(&Message::World(dereth_network_protocol::WorldInfo {
        slug: "coldeve".into(),
        name: "Coldeve".into(),
        online: false,
    }));
    assert_eq!(w.fed.watcher_count("Ann"), 899, "coldeve's went with it");
    w.frames();
    // The pace: a quarter of the hub's default messages per world is a burst of fifty.
    let mut paced = World::new();
    paced.on_and_present("Ann");
    paced.link(&[("coldeve", true)]);
    paced.frames();
    for n in 0..60 {
        watch(
            &mut paced,
            &format!("Bob{}@coldeve", letters(n)),
            "Ann@dawnsong",
            true,
        );
    }
    assert_eq!(paced.fed.watcher_count("Ann"), 50);
    paced.advance(1.0);
    watch(&mut paced, "Late@coldeve", "Ann@dawnsong", true);
    assert_eq!(paced.fed.watcher_count("Ann"), 51, "then 12.5 a second");
}

/// A name made of letters alone for each number, since a character's name holds no digits.
fn letters(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.push(char::from(b'a' + u8::try_from(n % 26).expect("a letter")));
        n /= 26;
        if n == 0 {
            return s;
        }
    }
}
