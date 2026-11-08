//! What a hub's word can make the world hold or write has a bound, and a hub within them never meets
//! one.

use dereth_network_protocol::limits::{Limits, Rate};
use dereth_network_protocol::wire::PROTOCOL;
use dereth_network_protocol::{DirectoryMessage, OffBy};
use dereth_network_protocol::{Message, PersonFriend, WorldInfo};
use yalain_federation::{
    MAX_RECEIVED, MAX_WORLDS, MAX_WORLDS_OWED, VIEW_PACE, VIEW_ROWS_PACE, WORLD_CHANGE_PACE,
};

use crate::{addr, World, SLUG};

/// A rate's burst as a count.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(burst: f64) -> usize {
    burst as usize
}

/// A slug no other `n` gives.
fn slug_of(n: usize) -> String {
    let mut s = String::from("w");
    let mut k = n;
    for _ in 0..8 {
        s.push(char::from(b'a' + u8::try_from(k % 26).expect("small")));
        k /= 26;
    }
    s
}

/// A name no other `n` gives.
fn name_of(n: usize) -> String {
    format!(
        "Fr{}{}",
        char::from(b'a' + u8::try_from(n / 26).expect("small")),
        char::from(b'a' + u8::try_from(n % 26).expect("small"))
    )
}

fn world(slug: &str, online: bool) -> Message {
    Message::World(WorldInfo {
        slug: slug.into(),
        name: slug.into(),
        online,
    })
}

fn view(to: &str, linked: bool, handles: &[&str]) -> Message {
    Message::PersonFriends {
        to: addr(to),
        linked,
        alts: Default::default(),
        persons: handles
            .iter()
            .map(|h| PersonFriend {
                handle: addr(h),
                characters: vec![addr(h)],
            })
            .collect(),
    }
}

/// A link is welcomed once: a second welcome neither moves the limits nor walks anyone again.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn a_second_welcome_on_a_link_changes_nothing() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Bob@coldeve");
    w.link(&[("coldeve", true)]);
    w.frames();
    w.hub(&Message::Welcome {
        proto: PROTOCOL,
        world: SLUG.into(),
        hub: "another".into(),
        worlds: vec![WorldInfo {
            slug: "frost".into(),
            name: "Frost".into(),
            online: true,
        }],
        rooms: Vec::new(),
        limits: Limits::DEFAULT,
    });
    assert!(w.fed.world_online("coldeve"), "as it was");
    assert!(!w.fed.world_online("frost"));
    assert_eq!(w.frames(), []);
}

/// A name no slug can be is not kept, and past the most worlds a new one is not taken: one known goes
/// on being told.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn the_world_keeps_at_most_so_many_other_worlds_and_only_by_slugs() {
    let mut w = World::new();
    w.link(&[("coldeve", true)]);
    w.frames();
    w.hub(&world("not a slug", true));
    assert_eq!(w.fed.worlds_online(), 1);
    for n in 0..MAX_WORLDS + 10 {
        w.hub(&world(&slug_of(n), true));
    }
    assert_eq!(w.fed.worlds_online(), MAX_WORLDS);
    assert!(!w.fed.world_online(&slug_of(MAX_WORLDS + 5)));
    w.hub(&world("coldeve", false));
    assert!(!w.fed.world_online("coldeve"));
    assert_eq!(w.fed.worlds_online(), MAX_WORLDS - 1);
}

/// However a world's state flips, what it means for the characters goes at the pace, and in the end
/// is what the hub last said.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn a_world_changing_past_the_pace_comes_to_the_characters_at_the_pace_and_ends_as_last_said() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.lists("Ann", "Bob@coldeve");
    w.link_with(
        &[("coldeve", false)],
        Limits {
            messages_per_world: Rate {
                burst: 100_000.0,
                per_sec: 1_000.0,
            },
            ..Limits::DEFAULT
        },
    );
    w.frames();
    let watches = |frames: &[Message]| {
        frames
            .iter()
            .filter(|m| matches!(m, Message::Watch { on: true, .. }))
            .count()
    };
    // A hub within the pace: each comes at once.
    w.hub(&world("coldeve", true));
    assert_eq!(watches(&w.frames()), 1);
    let flips = 4 * whole(WORLD_CHANGE_PACE.burst);
    let mut said = 0;
    for n in 0..flips {
        w.hub(&world("coldeve", n % 2 == 1));
        said += watches(&w.frames());
    }
    assert!(
        said < flips / 2,
        "{said} watches of {flips} changes, half of them up"
    );
    // It settles as it was last said, up.
    w.hub(&world("coldeve", false));
    w.hub(&world("coldeve", true));
    w.advance(10.0);
    assert!(w.fed.world_online("coldeve"));
    assert!(watches(&w.frames()) >= 1, "it is watched again");
}

/// A character's person view past the pace waits, its newest: what it lists comes in the end, and
/// what views can write in the meantime has a bound.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn person_views_past_their_pace_wait_and_the_newest_holds_in_the_end() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[("coldeve", true)]);
    w.frames();
    let id = w.ids["Ann"];
    let t = w.t;
    assert_eq!(w.fed.link_person(&mut w.store, t, id, "Ann"), None);
    w.frames();
    let first = view("Ann@dawnsong", true, &["Bob@coldeve"]);
    w.hub(&first);
    assert_eq!(w.service.views.len(), 1, "a first view comes at once");
    let frames = 4 * whole(VIEW_PACE.burst);
    let before = w.service.views.len();
    for n in 0..frames {
        let handle = format!("{}@coldeve", name_of(n));
        w.hub(&view("Ann@dawnsong", true, &[&handle]));
    }
    let came = w.service.views.len() - before;
    assert!(came <= whole(VIEW_PACE.burst), "{came} views at once");
    w.advance(5.0);
    let last = format!("{}@coldeve", name_of(frames - 1));
    assert!(
        w.fed
            .person_friends("Ann")
            .iter()
            .any(|f| f.same(&addr(&last))),
        "the newest holds: {:?}",
        w.fed.person_friends("Ann")
    );
    assert_eq!(w.fed.person_friends("Ann").len(), 1);
}

/// What person views change on the kept lists, which the store keeps, goes at the world's pace.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn what_person_views_change_on_kept_lists_goes_at_the_worlds_pace() {
    let names: Vec<String> = (0..100)
        .map(|n| format!("{}@coldeve", name_of(n)))
        .collect();
    let handles: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut w = World::new();
    let mut who = Vec::new();
    for n in 0..20 {
        let name = format!("Ann{}", char::from(b'a' + n));
        w.on_and_present(&name);
        who.push(name);
    }
    w.link(&[("coldeve", true)]);
    w.frames();
    for name in &who {
        let (t, id) = (w.t, w.ids[name.as_str()]);
        w.fed.link_person(&mut w.store, t, id, name);
        for h in &names {
            w.lists(name, h);
        }
    }
    w.frames();
    let kept = |w: &World| w.service.lists.values().map(Vec::len).sum::<usize>();
    let before = kept(&w);
    // Linked by views naming them as its own: each takes them from its list, while the pace goes.
    for name in &who {
        w.hub(&view(&format!("{name}@{SLUG}"), true, &handles));
    }
    let changed = before - kept(&w);
    assert!(
        changed < 3 * whole(VIEW_ROWS_PACE.burst) / 2,
        "{changed} changed at once"
    );
    for _ in 0..30 {
        w.advance(1.0);
    }
    assert_eq!(kept(&w), 0, "and in the end, all of them");
}

/// A hub's limits may tighten or ease a pace, within reason, and a wait has a longest.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn a_hubs_wait_past_an_hour_is_an_hour() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link_with(
        &[("coldeve", true)],
        Limits {
            result_timeout_secs: 1.0e12,
            ..Limits::DEFAULT
        },
    );
    w.tell(1, "Ann", "Bob@coldeve", "hi");
    w.frames();
    w.advance(3_599.0);
    assert!(w.service.ended.is_empty());
    w.advance(2.0);
    assert_eq!(w.service.ended.len(), 1);
}

/// Answers are owed to so many worlds at once: a tell from one more is not taken.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn answers_are_owed_to_so_many_worlds_at_once() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link_with(
        &[("coldeve", true)],
        Limits {
            messages_per_world: Rate {
                burst: 1.0,
                per_sec: 0.0,
            },
            ..Limits::DEFAULT
        },
    );
    w.frames();
    for n in 0..MAX_WORLDS_OWED + 5 {
        w.told(
            &format!("t{n}"),
            &format!("Bob@{}", slug_of(n)),
            "Ann@dawnsong",
            "hi",
        );
    }
    assert_eq!(w.fed.answers_owed(), MAX_WORLDS_OWED);
    assert_eq!(w.service.tells.len(), MAX_WORLDS_OWED);
}

/// The paces of what other worlds bring are kept for so many at once: past it what would start one
/// more is not shown, until others have refilled.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn the_paces_of_what_other_worlds_bring_are_kept_for_so_many_at_once() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link_with(
        &[("coldeve", true)],
        Limits {
            messages_per_world: Rate {
                burst: 200_000.0,
                per_sec: 0.0,
            },
            ..Limits::DEFAULT
        },
    );
    w.frames();
    for n in 0..MAX_RECEIVED + 5 {
        w.told(
            &format!("t{n}"),
            &format!("Bob@{}", slug_of(n)),
            "Ann@dawnsong",
            "hi",
        );
        w.frames();
    }
    assert_eq!(w.service.tells.len(), MAX_RECEIVED);
    w.advance(120.0);
    w.told(
        "again",
        &format!("Bob@{}", slug_of(0)),
        "Ann@dawnsong",
        "hi",
    );
    assert_eq!(
        w.service.tells.len(),
        MAX_RECEIVED + 1,
        "refilled, forgotten"
    );
}

/// The hub taking down a listing of a name this world does not know leaves nothing to keep, and one
/// it knows is taken as ever.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn a_listing_taken_off_for_a_name_this_world_does_not_know_keeps_nothing() {
    use yalain_store::Store;
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[("coldeve", true)]);
    w.frames();
    w.frame(
        &DirectoryMessage::ListingOff {
            who: addr("Nobody@dawnsong"),
            by: OffBy::Operator,
        }
        .to_json(),
    );
    w.fed.keep(&mut w.store).expect("keeps");
    let kept = w
        .store
        .world_record(yalain_federation::directory::RECORD)
        .expect("reads");
    assert!(kept.as_deref().is_none_or(<[u8]>::is_empty), "{kept:?}");
}

/// The hub's reports of an error reach the log a few at once, then seldom.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn the_log_takes_the_hubs_reports_of_an_error_at_a_pace() {
    let mut w = World::new();
    w.link(&[("coldeve", true)]);
    w.frames();
    for _ in 0..100 {
        w.hub(&Message::Error {
            code: "bad".into(),
            message: "x".repeat(5_000),
        });
    }
    assert!(w.fed.hub_errors_logged() <= 8);
    assert!(w.fed.hub_errors_logged() >= 1);
}

/// A welcome's list of worlds is taken as the world's own word is: by slugs.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn a_welcome_names_worlds_only_by_slugs() {
    let mut w = World::new();
    w.link(&[("not a slug", true), ("coldeve", true)]);
    assert_eq!(w.fed.worlds_online(), 1);
    assert!(w.fed.world_online("coldeve"));
    assert!(!w.fed.world_online("not a slug"));
}

/// A hub may ease a pace, within reason: past it a character's requests still run out.
/// Behaviour: server.network.hub-word-bounded
#[test]
fn a_hubs_pace_past_reason_still_runs_out() {
    use dereth_network_protocol::ChannelOp;
    let mut w = World::new();
    w.on_and_present("Ann");
    let eased = Limits::DEFAULT.tells_per_character;
    w.link_with(
        &[("coldeve", true)],
        Limits {
            tells_per_character: Rate {
                burst: 1.0e12,
                per_sec: 0.0,
            },
            ..Limits::DEFAULT
        },
    );
    let id = w.ids["Ann"];
    let t = w.t;
    let most = 1_000 * whole(eased.burst);
    let refused = (0..=most)
        .map(|_| {
            w.fed
                .ask_channel(t, id, "Ann", ChannelOp::List, "list", None)
        })
        .filter(Result::is_err)
        .count();
    assert_eq!(refused, 1);
}
