//! Focus while away: a hero out of the world regains the points a world's away regeneration row
//! gives for each whole span, as it enters again, its description carrying them; time in the world
//! brings none back, a world with no row gives none, and one who is no hero has none to regain.
//! Fixture: a player in the test hall, made a hero or not, the row 1 point each 600 s.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::vitals::{AwayRegen, Vital};
use yalain_testkit::Harness;
use yalain_world::{focus, quals};

const FOCUS: u32 = 266;
const EVERY: u32 = 600;

/// A player in the hall (made a hero, or not), its focus set, and the world's row for focus (or
/// none): its client and character.
fn hall(hero: bool, row: bool, f: i32) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    if row {
        h.server.content.away_regen.by_vital.insert(
            Vital::Focus,
            AwayRegen {
                every: EVERY,
                points: 1,
            },
        );
    }
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    if hero {
        focus::set_hero(&mut h.server, &mut h.net, "Kestrel", true).expect("made a hero");
        h.run(5);
        set_focus(&mut h, me, f);
    }
    (h, b, me)
}

fn set_focus(h: &mut Harness, me: u64, f: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    quals::set_int(&mut p.qualities, FOCUS, f);
}

fn focus_of(h: &Harness, me: u64) -> Option<i32> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| quals::int(&p.qualities, FOCUS))
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: id,
        }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// Away `seconds`, then in the world again: the focus its description carried (`None`: none), and
/// the focus updates it was sent as it entered.
fn back_after(h: &mut Harness, b: usize, me: u64, seconds: f64) -> (Option<i32>, Vec<i32>) {
    log_off(h, b, me);
    h.advance(seconds);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    h.run(10);
    let told = h.world_messages(b, from);
    let described = told.iter().find_map(|m| match m {
        ServerMessage::PlayerQualities(q) => Some(q.0.ints.get(FOCUS).copied()),
        _ => None,
    });
    let sent = told
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(x) if x.quality == FOCUS => Some(x.value),
            _ => None,
        })
        .collect();
    (described.expect("a description"), sent)
}

/// A hero out of the world regains a point of focus for each whole ten minutes it was away, given
/// back as it enters: its description carries the new focus, and no focus is sent besides; under
/// ten minutes brings nothing back.
/// Behaviour: server.focus.regained-while-away
#[test]
fn a_hero_regains_a_point_of_focus_for_each_ten_minutes_away_in_its_description() {
    let (mut h, b, me) = hall(true, true, 10);
    let (described, sent) = back_after(&mut h, b, me, 2.0 * f64::from(EVERY) + 50.0);
    assert_eq!(described, Some(12), "two whole spans");
    assert!(sent.is_empty(), "nothing sent besides: {sent:?}");
    assert_eq!(focus_of(&h, me), Some(12));
    let (described, _) = back_after(&mut h, b, me, f64::from(EVERY) - 30.0);
    assert_eq!(described, Some(12), "under a span, nothing");
    let (described, _) = back_after(&mut h, b, me, 7.0 * f64::from(EVERY) + 10.0);
    assert_eq!(described, Some(19));
}

/// Time a hero spends in the world brings no focus back, at entry or after; what it regains away
/// stops at its greatest; a world with no row gives nothing back.
/// Behaviour: server.focus.regained-only-while-away
#[test]
fn focus_comes_back_only_while_away_and_stops_at_its_greatest() {
    let (mut h, b, me) = hall(true, true, 10);
    for _ in 0..40 {
        h.step_by(100.0);
    }
    assert_eq!(
        focus_of(&h, me),
        Some(10),
        "4,000 s in the world brings none"
    );
    let (described, _) = back_after(&mut h, b, me, 5.0);
    assert_eq!(described, Some(10), "nor does it count at the next entry");
    let (described, _) = back_after(&mut h, b, me, 200.0 * f64::from(EVERY));
    assert_eq!(described, Some(100), "up to its greatest");

    let (mut h, b, me) = hall(true, false, 10);
    let (described, _) = back_after(&mut h, b, me, 200.0 * f64::from(EVERY));
    assert_eq!(described, Some(10), "no row, nothing back");
}

/// A player who is no hero regains no focus while away: one never a hero has none, its
/// description carrying none; one no longer a hero keeps what it held, no more.
/// Behaviour: server.focus.regained-while-away-heroes-only
#[test]
fn a_player_who_is_no_hero_regains_no_focus_while_away() {
    let (mut h, b, me) = hall(false, true, 0);
    let (described, sent) = back_after(&mut h, b, me, 20.0 * f64::from(EVERY));
    assert_eq!((described, sent), (None, Vec::new()));
    assert_eq!(focus_of(&h, me), None);

    let (mut h, b, me) = hall(true, true, 10);
    focus::set_hero(&mut h.server, &mut h.net, "Kestrel", false).expect("no longer a hero");
    h.run(5);
    let (described, _) = back_after(&mut h, b, me, 20.0 * f64::from(EVERY));
    assert_eq!(
        described,
        Some(10),
        "no longer a hero: what it held, no more"
    );
}
