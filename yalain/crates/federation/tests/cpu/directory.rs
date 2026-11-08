//! This world's characters in the network's directory: when each profile goes, what takes a
//! character out, and what the hub's answers do.

use dereth_network_protocol::directory::{
    AubereanVitals, ListingOutcome, OffBy, Refusal, Vital, LISTINGS_PER_WORLD,
};
use dereth_network_protocol::{AubereanProfile, DirectoryMessage, Game, ListingProfile, Message};
use yalain_federation::directory::{page_line, FORGOTTEN, FULL, RECORD, TAKEN_DOWN};
use yalain_federation::{Federation, Settings, State};
use yalain_store::Store;

use crate::{addr, World, SLUG};

const PAGE: &str = "https://dereth.network/directory/character/?a=Ann%40dawnsong";

fn profile(level: u32) -> AubereanProfile {
    AubereanProfile {
        level,
        race: 4,
        sex: 0x2000,
        title: None,
        vitals: Some(AubereanVitals {
            health: Vital { base: 100 + level },
            vigor: Vital { base: 50 + level },
        }),
        allegiance: None,
    }
}

/// One listing the world sent: its id, whom it names, whether it lists, and the level its profile
/// gives.
#[derive(Debug, Clone, PartialEq)]
struct Sent {
    id: String,
    who: String,
    on: bool,
    level: Option<u32>,
}

/// The listings the world sends now.
fn sent(w: &mut World) -> Vec<Sent> {
    w.frames();
    std::mem::take(&mut w.listings)
        .into_iter()
        .filter_map(|m| match m {
            DirectoryMessage::Listing {
                id,
                who,
                on,
                profile,
            } => Some(Sent {
                id,
                who: who.to_string(),
                on,
                level: profile.map(|p| match *p {
                    ListingProfile::Auberean(p) => p.level,
                    ListingProfile::Dereth(_) => panic!("an Asheron's Call profile"),
                }),
            }),
            _ => None,
        })
        .collect()
}

/// The (whom, on, level) of each listing sent now.
fn what(w: &mut World) -> Vec<(String, bool, Option<u32>)> {
    sent(w)
        .into_iter()
        .map(|s| (s.who, s.on, s.level))
        .collect()
}

fn l(who: &str, on: bool, level: Option<u32>) -> (String, bool, Option<u32>) {
    (who.to_owned(), on, level)
}

/// The hub answers the listing `id`.
fn answer(w: &mut World, id: &str, outcome: ListingOutcome, reason: Option<Refusal>) {
    let url = (outcome == ListingOutcome::Ok).then(|| PAGE.to_owned());
    let m = DirectoryMessage::ListingResult {
        id: id.into(),
        outcome,
        reason,
        url,
    };
    w.frame(&m.to_json());
}

/// The one listing sent now, answered `outcome`.
fn answered(w: &mut World, outcome: ListingOutcome, reason: Option<Refusal>) -> Sent {
    let s = sent(w);
    assert_eq!(s.len(), 1, "{s:?}");
    answer(w, &s[0].id, outcome, reason);
    s[0].clone()
}

fn on(w: &mut World, name: &str, level: u32) {
    let t = w.t;
    w.fed.listing_on(t, name, profile(level));
}

fn changed(w: &mut World, name: &str, level: u32) {
    let t = w.t;
    w.fed.listing_changed(t, name, profile(level));
}

fn left(w: &mut World, name: &str, level: u32) {
    let t = w.t;
    w.fed.listing_left(t, name, profile(level));
}

fn notices(w: &mut World) -> Vec<(String, String)> {
    w.fed.take_notices(|_| true)
}

fn line(name: &str, line: &str) -> (String, String) {
    (name.to_owned(), line.to_owned())
}

/// A world linked, with Ann on the network and listed, the hub's answer taken.
fn listed() -> World {
    let mut w = World::new();
    w.character("Ann");
    w.set_on("Ann", true);
    w.link(&[]);
    on(&mut w, "Ann", 10);
    answered(&mut w, ListingOutcome::Ok, None);
    notices(&mut w);
    w
}

/// A character on the network is listed with its Asheron's Call 2 profile at once at each login,
/// and at `@dn on`, after which the world is asked for its profile and its page is shown to it; a
/// character off the network is never listed.
/// Behaviour: server.network.directory-listed-on
#[test]
fn a_character_on_the_network_is_listed_at_login_and_at_dn_on() {
    let mut w = World::new();
    w.character("Ann");
    w.character("Bob");
    w.link(&[]);
    assert!(w.fed.profiles_wanted().is_empty());
    on(&mut w, "Bob", 5);
    assert!(what(&mut w).is_empty(), "off the network");
    w.set_on("Ann", true);
    assert_eq!(w.fed.profiles_wanted(), [(w.ids["Ann"], "Ann".to_owned())]);
    on(&mut w, "Ann", 10);
    assert!(w.fed.profiles_wanted().is_empty());
    let s = answered(&mut w, ListingOutcome::Ok, None);
    assert_eq!(
        (s.who.as_str(), s.on, s.level),
        ("Ann@dawnsong", true, Some(10))
    );
    assert_eq!(notices(&mut w), [line("Ann", &page_line(PAGE))]);
    assert_eq!(w.fed.listing_page("ann"), Some(PAGE));
    // Logged out unchanged, it sends nothing; logged in again, it is listed at once, quietly.
    left(&mut w, "Ann", 10);
    assert!(what(&mut w).is_empty());
    w.advance(30.0);
    on(&mut w, "Ann", 10);
    answered(&mut w, ListingOutcome::Ok, None);
    assert!(notices(&mut w).is_empty(), "a login shows no page");
}

/// A change while the character plays goes once its last listing is five minutes old, the latest
/// profile then and that alone; a profile handed in unchanged sends nothing, nor one out of the
/// directory's bounds; at logout a changed profile goes at once.
/// Behaviour: server.network.directory-changes-paced
#[test]
fn a_change_waits_its_five_minutes_and_a_logout_sends_it_at_once() {
    let mut w = listed();
    w.advance(60.0);
    changed(&mut w, "Ann", 11);
    assert!(what(&mut w).is_empty(), "within five minutes");
    w.advance(200.0);
    changed(&mut w, "Ann", 12);
    assert!(what(&mut w).is_empty());
    w.advance(39.0);
    assert!(what(&mut w).is_empty(), "299 s after the last");
    w.advance(1.0);
    let s = answered(&mut w, ListingOutcome::Ok, None);
    assert_eq!(s.level, Some(12), "the latest alone");
    w.advance(600.0);
    changed(&mut w, "Ann", 12);
    assert!(what(&mut w).is_empty(), "unchanged");
    let t = w.t;
    w.fed.listing_changed(
        t,
        "Ann",
        AubereanProfile {
            race: 3,
            ..profile(14)
        },
    );
    assert!(what(&mut w).is_empty(), "out of the directory's bounds");
    w.advance(10.0);
    left(&mut w, "Ann", 13);
    assert_eq!(what(&mut w), [l("Ann@dawnsong", true, Some(13))]);
    left(&mut w, "Ann", 13);
    assert!(what(&mut w).is_empty());
}

/// Going off the network takes the character out of the directory at once; what takes it out is
/// owed until the hub answers it, again after a lost link and after a restart (it is kept in the
/// store), and goes before any profile; a profile the hub has not answered goes again after a lost
/// link.
/// Behaviour: server.network.directory-off-kept
#[test]
fn going_off_takes_the_character_out_at_once_and_is_owed_until_answered() {
    let mut w = listed();
    w.character("Bob");
    w.set_on("Bob", true);
    w.set_on("Ann", false);
    assert_eq!(what(&mut w), [l("Ann@dawnsong", false, None)]);
    w.fed.keep(&mut w.store).expect("kept");
    let t = w.t;
    w.fed.on_link_down(&mut w.service, t);
    on(&mut w, "Bob", 3);
    w.link(&[]);
    assert_eq!(
        what(&mut w),
        [
            l("Ann@dawnsong", false, None),
            l("Bob@dawnsong", true, Some(3))
        ],
        "owed again on the next link, first"
    );
    // Neither answered when the link goes: both go again on the next.
    let t = w.t;
    w.fed.on_link_down(&mut w.service, t);
    w.link(&[]);
    assert_eq!(
        what(&mut w),
        [
            l("Ann@dawnsong", false, None),
            l("Bob@dawnsong", true, Some(3))
        ],
        "unanswered: owed again"
    );
    // A restart: the store's record owes it still.
    w.fed = Federation::load(
        Settings {
            enabled: true,
            slug: SLUG.into(),
            software: "yalain test".into(),
            ..Settings::default()
        },
        1_790_000_001,
        &mut w.store,
    )
    .expect("loaded");
    w.link(&[]);
    let s = answered(&mut w, ListingOutcome::Ok, None);
    assert_eq!((s.who.as_str(), s.on), ("Ann@dawnsong", false));
    w.fed.keep(&mut w.store).expect("kept");
    assert_eq!(
        w.store.world_record(RECORD).expect("read").as_deref(),
        Some(&b""[..])
    );
    let t = w.t;
    w.fed.on_link_down(&mut w.service, t);
    w.link(&[]);
    assert!(what(&mut w).is_empty(), "answered: owed no more");
}

/// The hub's operator taking a character out: the world says it heard, at once; the character is
/// told (at its next login if it is away) and held out, sent nothing until its next login.
/// Behaviour: server.network.directory-taken-out-by-operator
#[test]
fn the_hubs_operator_holds_a_character_out_until_its_next_login() {
    let mut w = listed();
    w.frame(
        &DirectoryMessage::ListingOff {
            who: addr("ann@dawnsong"),
            by: OffBy::Operator,
        }
        .to_json(),
    );
    let heard = answered(&mut w, ListingOutcome::Ok, None);
    assert_eq!((heard.who.as_str(), heard.on), ("ann@dawnsong", false));
    assert!(w.fed.take_notices(|_| false).is_empty(), "away: it waits");
    assert_eq!(notices(&mut w), [line("Ann", TAKEN_DOWN)]);
    assert!(!w.fed.listed("Ann") && w.fed.is_on("Ann"));
    w.advance(600.0);
    changed(&mut w, "Ann", 30);
    left(&mut w, "Ann", 30);
    assert!(what(&mut w).is_empty(), "held out");
    on(&mut w, "Ann", 30);
    assert_eq!(what(&mut w), [l("Ann@dawnsong", true, Some(30))]);
    assert!(w.fed.listed("Ann"), "listed again from its login");
}

/// A character whose person asked the network to forget it: the world says it heard, takes the
/// character off the network (kept in the store), and tells it.
/// Behaviour: server.network.directory-taken-out-by-person
#[test]
fn a_character_its_person_forgot_goes_off_the_network() {
    let mut w = listed();
    w.frame(
        &DirectoryMessage::ListingOff {
            who: addr("Ann@dawnsong"),
            by: OffBy::Person,
        }
        .to_json(),
    );
    assert_eq!(what(&mut w), [l("Ann@dawnsong", false, None)]);
    assert!(!w.fed.is_on("Ann"));
    w.fed.keep(&mut w.store).expect("kept");
    assert!(w.store.network_characters().expect("read").is_empty());
    assert_eq!(notices(&mut w), [line("Ann", FORGOTTEN)]);
    on(&mut w, "Ann", 10);
    assert!(what(&mut w).is_empty(), "off the network");
}

/// A refusal after `@dn on` is shown, and holds the character out until its next listing asked
/// for; a listing the hub held back to its pace goes again a minute later, the latest profile
/// then.
/// Behaviour: server.network.directory-answers
#[test]
fn a_refusal_holds_the_character_out_and_one_held_back_goes_a_minute_later() {
    let mut w = World::new();
    for name in ["Ann", "Bob", "Cat"] {
        w.character(name);
    }
    w.link(&[]);
    w.set_on("Ann", true);
    on(&mut w, "Ann", 10);
    answered(&mut w, ListingOutcome::Refused, Some(Refusal::TakenDown));
    assert_eq!(notices(&mut w), [line("Ann", TAKEN_DOWN)]);
    assert!(!w.fed.listed("Ann") && w.fed.is_on("Ann"), "held out");
    w.set_on("Cat", true);
    on(&mut w, "Cat", 10);
    answered(&mut w, ListingOutcome::Refused, Some(Refusal::Full));
    assert_eq!(notices(&mut w), [line("Cat", FULL)]);
    w.advance(600.0);
    changed(&mut w, "Ann", 11);
    assert!(what(&mut w).is_empty(), "held out");
    w.set_on("Bob", true);
    on(&mut w, "Bob", 10);
    answered(&mut w, ListingOutcome::RateLimited, None);
    changed(&mut w, "Bob", 20);
    w.advance(59.0);
    assert!(what(&mut w).is_empty());
    w.advance(1.0);
    let s = answered(&mut w, ListingOutcome::Ok, None);
    assert_eq!((s.who.as_str(), s.level), ("Bob@dawnsong", Some(20)));
    assert_eq!(notices(&mut w), [line("Bob", &page_line(PAGE))]);
}

/// Listings go after everything else the link carries, the world's within the hub's pace for
/// them, and none is lost to the pace: each goes once it allows.
/// Behaviour: server.network.directory-paced
#[test]
fn listings_go_after_the_rest_within_the_hubs_pace() {
    let mut w = World::new();
    // Names hold letters alone.
    let names: Vec<String> = (0u8..60)
        .map(|i| {
            format!(
                "Char{}{}",
                char::from(b'a' + i / 26),
                char::from(b'a' + i % 26)
            )
        })
        .collect();
    for n in &names {
        w.character(n);
        w.set_on(n, true);
    }
    w.enter("Charaa", State::Available);
    w.link(&[("coldeve", true)]);
    for n in &names {
        on(&mut w, n, 5);
    }
    w.tell(1, "Charaa", "Bob@coldeve", "hi");
    let t = w.t;
    let frames = w.fed.take_frames(t);
    assert!(
        matches!(Message::from_json(&frames[0]), Ok(Message::Tell { .. })),
        "the tell first: {}",
        frames[0]
    );
    let mut seen = frames
        .iter()
        .filter(|f| DirectoryMessage::from_json_for(Game::Auberean, f).is_ok())
        .count();
    assert!(
        seen > 0 && seen as f64 <= LISTINGS_PER_WORLD.burst,
        "{seen} at once"
    );
    for _ in 0..20 {
        w.advance(1.0);
        seen += sent(&mut w).len();
    }
    assert_eq!(seen, names.len(), "each once");
}

/// A character takes a new name.
fn rename(w: &mut World, old: &str, new: &str) {
    w.fed.renaming(&w.service, old);
    if let Some(mut p) = w.service.present.remove(&old.to_lowercase()) {
        new.clone_into(&mut p.name);
        w.service.present.insert(new.to_lowercase(), p);
    }
    w.fed.renamed(&w.service, old, new);
    let id = w.ids[old];
    w.ids.insert(new.to_owned(), id);
}

/// A renamed character on the network is taken out of the directory under the name it had, at once,
/// and listed under the new one with the world's next pass, its page not shown; on the network still.
/// Behaviour: server.network.directory-rename
#[test]
fn a_renamed_character_leaves_the_directory_under_its_old_name_and_is_listed_under_its_new() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[]);
    w.fed.listing_on(w.t, "Ann", profile(5));
    let id = sent(&mut w)[0].id.clone();
    answer(&mut w, &id, ListingOutcome::Ok, None);
    w.fed.take_notices(|_| true);

    rename(&mut w, "Ann", "Anna");
    assert_eq!(what(&mut w), [l("Ann@dawnsong", false, None)]);
    let anna = w.ids["Anna"];
    assert_eq!(w.fed.profiles_wanted(), [(anna, "Anna".to_owned())]);
    w.fed.listing_on(w.t, "Anna", profile(5));
    let listed = sent(&mut w);
    assert_eq!(
        listed
            .iter()
            .map(|s| (s.who.as_str(), s.on))
            .collect::<Vec<_>>(),
        [("Anna@dawnsong", true)]
    );
    answer(&mut w, &listed[0].id.clone(), ListingOutcome::Ok, None);
    assert!(
        w.fed.take_notices(|_| true).is_empty(),
        "no page line for a character that did not ask"
    );
    assert!(w.fed.listed("Anna"));
    assert!(!w.fed.listed("Ann"));
}

/// A character the hub holds out stays out under its new name, until its next login or `@dn on`, as
/// under the old; the old name is still taken out.
/// Behaviour: server.network.directory-rename
#[test]
fn a_renamed_character_the_hub_holds_out_stays_out() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[]);
    w.fed.listing_on(w.t, "Ann", profile(5));
    sent(&mut w);
    w.frame(
        &DirectoryMessage::ListingOff {
            who: addr("Ann@dawnsong"),
            by: OffBy::Operator,
        }
        .to_json(),
    );
    what(&mut w);
    assert!(!w.fed.listed("Ann"));
    rename(&mut w, "Ann", "Anna");
    assert!(w.fed.profiles_wanted().is_empty());
    assert!(!w.fed.listed("Anna"));
    assert!(w.fed.is_on("Anna"));
}

/// A line the directory holds for a character out of reach reaches it under its new name.
/// Behaviour: server.network.directory-rename
#[test]
fn a_directory_line_waiting_for_a_renamed_character_reaches_its_new_name() {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[]);
    w.frame(
        &DirectoryMessage::ListingOff {
            who: addr("Ann@dawnsong"),
            by: OffBy::Operator,
        }
        .to_json(),
    );
    rename(&mut w, "Ann", "Anna");
    assert_eq!(
        w.fed.take_notices(|n| n == "Anna"),
        [("Anna".to_owned(), TAKEN_DOWN.to_owned())]
    );
}

/// What the hub doesn't answer in time is owed again, and what the link holds of it doesn't grow: a
/// listing that takes a character out goes again, and so does a profile.
/// Behaviour: server.network.directory-unanswered-goes-again
#[test]
fn a_listing_the_hub_does_not_answer_in_time_goes_again() {
    let timeout = dereth_network_protocol::limits::Limits::DEFAULT.result_timeout_secs;
    let mut w = listed();
    w.set_on("Ann", false);
    assert_eq!(what(&mut w), [l("Ann@dawnsong", false, None)]);
    assert_eq!(w.fed.listings_unanswered(), 1);
    w.advance(timeout - 1.0);
    assert!(what(&mut w).is_empty(), "not before its time");
    w.advance(2.0);
    assert_eq!(
        what(&mut w),
        [l("Ann@dawnsong", false, None)],
        "unanswered in time: again"
    );
    assert_eq!(w.fed.listings_unanswered(), 1, "the first is forgotten");
    w.character("Bob");
    w.set_on("Bob", true);
    on(&mut w, "Bob", 3);
    let first = what(&mut w);
    assert!(
        first.contains(&l("Bob@dawnsong", true, Some(3))),
        "{first:?}"
    );
    w.advance(timeout + 1.0);
    let again = what(&mut w);
    assert!(
        again.contains(&l("Bob@dawnsong", true, Some(3))),
        "{again:?}"
    );
    assert_eq!(w.fed.listings_unanswered(), 2, "one for each");
}
