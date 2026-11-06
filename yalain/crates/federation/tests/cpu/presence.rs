//! This world's characters watching their friends on other worlds.

use dereth_network_protocol::limits::{Limits, Rate};
use dereth_network_protocol::{Message, WorldInfo};
use yalain_federation::State;

use crate::{addr, World};

/// The watches a world sends now: (owner, target, on).
fn watches(frames: &[Message]) -> Vec<(String, String, bool)> {
    let mut v: Vec<_> = frames
        .iter()
        .filter_map(|m| match m {
            Message::Watch { owner, target, on } => {
                Some((owner.to_string(), target.to_string(), *on))
            }
            _ => None,
        })
        .collect();
    v.sort();
    v
}

fn wt(owner: &str, target: &str, on: bool) -> (String, String, bool) {
    (owner.to_owned(), target.to_owned(), on)
}

fn presence(w: &mut World, to: &str, of: &str, online: bool) {
    w.hub(&Message::Presence {
        to: addr(to),
        of: addr(of),
        online,
    });
}

fn world(w: &mut World, slug: &str, online: bool) {
    w.hub(&Message::World(WorldInfo {
        slug: slug.into(),
        name: slug.into(),
        online,
    }));
}

/// What the friends service was asked to show: (to, friend, online, news).
fn states(w: &mut World) -> Vec<(String, String, bool, bool)> {
    std::mem::take(&mut w.service.states)
        .into_iter()
        .map(|(to, f, online, news)| (to, f.to_string(), online, news))
        .collect()
}

fn st(to: &str, f: &str, online: bool, news: bool) -> (String, String, bool, bool) {
    (to.to_owned(), f.to_owned(), online, news)
}

/// Once welcomed, the world watches each friend on a linked world of each present character on
/// the network: not a friend of its own world, not one on a world whose link is down (watched when
/// the hub says it is up), not a character off the network's, not an absent one's. A character
/// arriving watches its friends; one leaving stops; a friend added or removed is watched or no
/// longer.
/// Behaviour: server.network.friends-watched
#[test]
fn present_characters_on_the_network_watch_their_friends_on_linked_worlds() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Bob@coldeve");
    w.lists("Ann", "Eve@frost");
    w.lists("Ann", "Cara@dawnsong");
    w.character("Cara");
    w.enter("Cara", State::Available);
    w.lists("Cara", "Bob@coldeve");
    w.character("Dee");
    w.set_on("Dee", true);
    w.lists("Dee", "Bob@coldeve");
    let after = w.link(&[("coldeve", true), ("frost", false)]);
    assert_eq!(
        watches(&after),
        vec![wt("Ann@dawnsong", "Bob@coldeve", true)]
    );
    world(&mut w, "frost", true);
    assert_eq!(
        watches(&w.frames()),
        vec![wt("Ann@dawnsong", "Eve@frost", true)]
    );
    world(&mut w, "frost", true);
    assert!(
        w.frames().is_empty(),
        "a world already up is not watched again"
    );
    w.enter("Dee", State::Available);
    assert_eq!(
        watches(&w.frames()),
        vec![wt("Dee@dawnsong", "Bob@coldeve", true)]
    );
    w.lists("Ann", "Gil@coldeve");
    w.fed.friend_added(&w.service, "Ann", &addr("Gil@coldeve"));
    assert_eq!(
        watches(&w.frames()),
        vec![wt("Ann@dawnsong", "Gil@coldeve", true)]
    );
    w.service.lists.get_mut("ann").expect("listed").pop();
    w.fed.friend_removed("Ann", &addr("Gil@coldeve"));
    assert_eq!(
        watches(&w.frames()),
        vec![wt("Ann@dawnsong", "Gil@coldeve", false)]
    );
    w.leave("Ann");
    assert_eq!(
        watches(&w.frames()),
        vec![
            wt("Ann@dawnsong", "Bob@coldeve", false),
            wt("Ann@dawnsong", "Eve@frost", false),
        ]
    );
}

/// A friend's state reaches the character that lists it as its world reports it: the first
/// report after the friend was added or its world came back is a state, later changes are news.
/// A report for a character that does not list the friend, or is not present, is not shown. When
/// the friend's world goes, or the hub, friends shown online show offline, a change from what was
/// shown.
/// Behaviour: server.network.friends-state-shown
#[test]
fn a_friends_state_is_shown_as_its_world_reports_it_and_offline_when_its_world_goes() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "bob@coldeve");
    w.on_and_present("Eli");
    w.link(&[("coldeve", true)]);
    w.frames();
    presence(&mut w, "Ann@dawnsong", "Bob@coldeve", false);
    presence(&mut w, "Ann@dawnsong", "Bob@coldeve", true);
    presence(&mut w, "Eli@dawnsong", "Bob@coldeve", true);
    presence(&mut w, "Zoe@dawnsong", "Bob@coldeve", true);
    assert_eq!(
        states(&mut w),
        vec![
            st("Ann", "bob@coldeve", false, false),
            st("Ann", "bob@coldeve", true, true),
        ],
        "named as listed; the first a state, the next news"
    );
    presence(&mut w, "Ann@dawnsong", "Bob@coldeve", true);
    assert!(states(&mut w).is_empty(), "no change, nothing shown");
    world(&mut w, "coldeve", false);
    assert_eq!(states(&mut w), vec![st("Ann", "bob@coldeve", false, true)]);
    world(&mut w, "coldeve", true);
    assert_eq!(
        watches(&w.frames()),
        vec![wt("Ann@dawnsong", "bob@coldeve", true)],
        "watched again"
    );
    presence(&mut w, "Ann@dawnsong", "Bob@coldeve", true);
    assert_eq!(
        states(&mut w),
        vec![st("Ann", "bob@coldeve", true, false)],
        "the first report after its world came back is a state"
    );
    w.fed.on_link_down(&mut w.service, w.t);
    assert_eq!(states(&mut w), vec![st("Ann", "bob@coldeve", false, true)]);
}

/// A character is shown at most four changes of one friend at once, then one every five seconds:
/// a friend's world that changes its answer faster is shown its latest state as the pace allows.
/// Behaviour: server.network.friends-state-paced
#[test]
fn a_friends_changes_are_shown_four_at_once_then_one_every_five_seconds() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Bob@coldeve");
    w.link(&[("coldeve", true)]);
    w.frames();
    for n in 0..9 {
        presence(&mut w, "Ann@dawnsong", "Bob@coldeve", n % 2 == 0);
    }
    let shown = states(&mut w);
    assert_eq!(shown.len(), 4, "{shown:?}");
    assert_eq!(shown.last().map(|s| s.2), Some(false));
    w.advance(4.9);
    assert!(states(&mut w).is_empty());
    w.advance(0.1);
    assert_eq!(
        states(&mut w),
        vec![st("Ann", "Bob@coldeve", true, true)],
        "the latest state"
    );
    presence(&mut w, "Ann@dawnsong", "Bob@coldeve", false);
    presence(&mut w, "Ann@dawnsong", "Bob@coldeve", true);
    w.advance(5.0);
    assert!(
        states(&mut w).is_empty(),
        "back to what was shown: nothing waits"
    );
}

/// Watches and presence answers wait behind this world's own tells and the answers it owes, and
/// go a world at a time, each world's at most at its share of the hub's pace; a newer watch for
/// the same pair takes the place of one waiting. They are not queued while the link is down: a
/// reconnect sends them again.
/// Behaviour: server.network.frames-paced-in-order
#[test]
fn watches_wait_behind_tells_and_answers_and_go_a_world_at_a_time() {
    let mut w = World::new();
    w.on_and_present("Ann");
    for n in 0..20 {
        w.lists("Ann", &format!("Bob{}@coldeve", char::from(b'a' + n)));
        w.lists("Ann", &format!("Eve{}@frost", char::from(b'a' + n)));
    }
    let limits = Limits {
        messages_per_world: Rate {
            burst: 80.0,
            per_sec: 4.0,
        },
        ..Limits::DEFAULT
    };
    w.fed.on_link_up();
    w.frames();
    w.tell(1, "Ann", "Bob@coldeve", "first");
    w.hub(&Message::Welcome {
        proto: dereth_network_protocol::wire::PROTOCOL,
        world: crate::SLUG.into(),
        hub: "test hub".into(),
        worlds: vec![
            WorldInfo {
                slug: "coldeve".into(),
                name: "Coldeve".into(),
                online: true,
            },
            WorldInfo {
                slug: "frost".into(),
                name: "Frost".into(),
                online: true,
            },
        ],
        rooms: Vec::new(),
        limits,
    });
    w.told("h1", "Hal@ember", "Ann@dawnsong", "hi");
    w.fed.friend_removed("Ann", &addr("Bobb@coldeve"));
    w.fed.friend_added(&w.service, "Ann", &addr("Bobb@coldeve"));
    let first = w.frames();
    assert!(matches!(first[0], Message::Tell { .. }), "{first:?}");
    assert!(matches!(first[1], Message::TellResult { .. }), "{first:?}");
    assert!(matches!(first[2], Message::PersonWatch { .. }), "{first:?}");
    let rest = &first[3..];
    // A burst of 72 (a tenth under 80): the tell, the answer, then 70 watches; each world's share
    // is a burst of 18 (a tenth under 20).
    assert_eq!(rest.len(), 36, "two worlds' shares: {}", rest.len());
    let worlds: Vec<&str> = rest
        .iter()
        .map(|m| match m {
            Message::Watch { target, .. } => target.world(),
            m => panic!("{m:?}"),
        })
        .collect();
    assert!(
        worlds.windows(2).all(|p| p[0] != p[1]),
        "a world at a time: {worlds:?}"
    );
    let bobb: Vec<bool> = rest
        .iter()
        .filter_map(|m| match m {
            Message::Watch { target, on, .. } if target.name() == "Bobb" => Some(*on),
            _ => None,
        })
        .collect();
    assert_eq!(
        bobb,
        vec![true],
        "the newer watch took the waiting one's place"
    );
    w.fed.on_link_down(&mut w.service, w.t);
    assert_eq!(
        w.fed.queued(),
        1,
        "the tell alone waits for the next link: watches are not queued"
    );
}
