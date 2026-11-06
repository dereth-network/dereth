//! The world shutting down: the countdown every player in the world is shown, two minutes long, a
//! line every thirty seconds, then the server stops twenty seconds after the last; and, however it
//! stops, every character still in the world kept first. Fixture: two new players in the world.

use auberean_protocol::text::VariableValue;
use auberean_protocol::ServerMessage;
use yalain_store::Store as _;
use yalain_testkit::Harness;
use yalain_world::shutdown::{self, Shutdown};
use yalain_world::{examine, quals};

/// The countdown's lines a player was shown: when (seconds after the shutdown began), the string,
/// and the values of its variables.
fn lines(h: &Harness, b: usize, from: usize, at: &[(usize, f64)]) -> Vec<(f64, u32, Vec<u32>)> {
    let mut out = Vec::new();
    for (i, r) in h.bots[b].inbox.iter().enumerate().skip(from) {
        let Some(ServerMessage::Notice(n)) = r.message() else {
            continue;
        };
        assert_eq!(
            n.kind,
            shutdown::KIND,
            "shown as the server's text of its kind"
        );
        assert_eq!(n.text.table_id, shutdown::TABLE);
        let when = at
            .iter()
            .find(|(upto, _)| i < *upto)
            .map_or(f64::NAN, |(_, t)| *t);
        let values = n
            .text
            .variables
            .iter()
            .map(|v| match v.value {
                VariableValue::Word(w) => w,
                _ => u32::MAX,
            })
            .collect();
        out.push((when, n.text.string_id, values));
    }
    out
}

/// A shutdown begun with two players in the world: each is shown "2 minutes", "1 minute and 30
/// seconds", "1 minute", "30 seconds" and "NOW", thirty seconds apart, and the server is done
/// twenty seconds after the last line, not before.
/// Behaviour: server.operator.shutdown-counts-down-two-minutes
#[test]
fn a_shutdown_counts_down_two_minutes_to_every_player_then_stops() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Tamsin");
    h.run(5);
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    let begun = h.now.0;
    let clock = h.clock();
    shutdown::begin(&mut h.server, &mut h.net, clock, None);
    // Each second, where each inbox stood.
    let mut at: [Vec<(usize, f64)>; 2] = [Vec::new(), Vec::new()];
    let mut done_at = None;
    while h.now.0 - begun < 160.0 {
        h.step_by(1.0);
        let t = (h.now.0 - begun).round();
        at[0].push((h.bots[a].inbox.len(), t));
        at[1].push((h.bots[b].inbox.len(), t));
        if done_at.is_none() && h.server.shutdown == Shutdown::Done {
            done_at = Some(t);
        }
    }
    let want = [
        (shutdown::MINUTES, vec![2]),
        (shutdown::MINUTES_AND_SECONDS, vec![1, 30]),
        (shutdown::MINUTES, vec![1]),
        (shutdown::SECONDS, vec![30]),
        (shutdown::NOW, vec![]),
    ];
    for (i, bot) in [a, b].into_iter().enumerate() {
        let got = lines(&h, bot, from[i], &at[i]);
        let strings: Vec<(u32, Vec<u32>)> = got.iter().map(|(_, s, v)| (*s, v.clone())).collect();
        assert_eq!(strings, want, "the countdown's lines, in order");
        assert!(got[0].0 <= 2.0, "the first line at once ({} s)", got[0].0);
        for w in got.windows(2) {
            assert!(
                (w[1].0 - w[0].0 - 30.0).abs() <= 1.0,
                "a line {} s after the one before",
                w[1].0 - w[0].0
            );
        }
    }
    let done = done_at.expect("the server is done");
    assert!(
        (done - 140.0).abs() <= 1.0,
        "done twenty seconds after the last line (at {done} s)"
    );
}

/// The level a character's kept state holds, if any.
fn kept_level(h: &mut Harness, id: u64) -> Option<i32> {
    let st = h.server.store.world_state(id).expect("the store reads")?;
    quals::int(&st.qualities, examine::LEVEL)
}

/// The server stopping with two players in the world: each leaves the world, and what changed
/// since it was last kept (here its level) is in the store, as it is when its client goes away.
/// Behaviour: server.operator.stopping-keeps-everyone-in-the-world
#[test]
fn stopping_the_server_keeps_every_character_still_in_the_world() {
    let mut h = Harness::new();
    let (_, elara) = h.player_in_world("one", "Elara");
    let (_, tamsin) = h.player_in_world("two", "Tamsin");
    h.run(5);
    for p in h.server.players.values_mut() {
        quals::set_int(&mut p.qualities, examine::LEVEL, 7);
    }
    assert_ne!(
        kept_level(&mut h, elara),
        Some(7),
        "not kept before the stop"
    );
    assert_eq!(shutdown::save_everyone(&mut h.server, &mut h.net), 2);
    assert!(h.server.players.is_empty(), "nobody is left in the world");
    for id in [elara, tamsin] {
        assert_eq!(kept_level(&mut h, id), Some(7), "kept at the stop");
    }
}

/// A stop takes every client from the world to its disconnected screen, told it was logged off,
/// one at its character list too, and every connection ends rather than timing out.
/// Behaviour: server.operator.a-stop-lets-every-client-go
#[test]
fn a_stop_tells_every_client_it_was_logged_off_and_ends_its_connection() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let b = h.at_character_list("two");
    h.run(5);
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    assert!(h.net.connection_count() >= 3);
    let now = h.now;
    assert_eq!(shutdown::stopping(&mut h.server, &mut h.net, now), 1);
    assert_eq!(h.net.connection_count(), 0, "every connection is ended");
    h.run(4);
    for (bot, from) in [(a, from_a), (b, from_b)] {
        assert!(
            h.bots[bot].inbox[from..].iter().any(|r| matches!(
                r.message(),
                Some(auberean_protocol::ServerMessage::CharacterRefused(x))
                    if x.reason == auberean_protocol::login::CharacterRefused::LOGGED_OFF
            )),
            "bot {bot}"
        );
    }
}

/// A stop after a panic has nothing more to tell: everyone was let go at the panic.
/// Behaviour: server.operator.a-stop-lets-every-client-go
#[test]
fn a_stop_after_a_panic_tells_no_one_it_was_logged_off() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    h.run(5);
    let clock = h.clock();
    yalain_world::guard::stopped(&mut h.server, &mut h.net, clock, "a test");
    let from = h.bots[a].inbox.len();
    let now = h.now;
    assert_eq!(shutdown::stopping(&mut h.server, &mut h.net, now), 0);
    h.run(4);
    assert!(!h.bots[a].inbox[from..].iter().any(|r| matches!(
        r.message(),
        Some(auberean_protocol::ServerMessage::CharacterRefused(x))
            if x.reason == auberean_protocol::login::CharacterRefused::LOGGED_OFF
    )));
}

/// What a player was shown as notices since `from`: for each, when it was sent (seconds after the
/// countdown began, from the inbox's length each second: what the world sends at a step reaches
/// the client at the next), and its text: the operator's literal, or the countdown's string and
/// its variables' values.
fn shown(h: &Harness, b: usize, from: usize, at: &[(usize, f64)]) -> Vec<(f64, Shown)> {
    let mut out = Vec::new();
    for (i, r) in h.bots[b].inbox.iter().enumerate().skip(from) {
        let Some(ServerMessage::Notice(n)) = r.message() else {
            continue;
        };
        let when = at
            .iter()
            .find(|(upto, _)| i < *upto)
            .map_or(f64::NAN, |(_, t)| *t - 1.0);
        let text = match &n.text.literal {
            Some(l) => Shown::Literal(n.kind, l.text()),
            None => {
                assert_eq!(n.kind, shutdown::KIND, "the countdown's kind");
                assert_eq!(n.text.table_id, shutdown::TABLE);
                Shown::Line(
                    n.text.string_id,
                    n.text
                        .variables
                        .iter()
                        .map(|v| match v.value {
                            VariableValue::Word(w) => w,
                            _ => u32::MAX,
                        })
                        .collect(),
                )
            }
        };
        out.push((when, text));
    }
    out
}

/// A notice shown: a literal (its kind and text), or one of the countdown's lines.
#[derive(Debug, Clone, PartialEq)]
enum Shown {
    Literal(u32, String),
    Line(u32, Vec<u32>),
}

/// Step a second at a time for `seconds`, noting where the inbox of each bot stood each second,
/// and when (seconds after `begun`) the server was first done.
fn watch(
    h: &mut Harness,
    bots: &[usize],
    begun: f64,
    seconds: f64,
) -> (Vec<Vec<(usize, f64)>>, Option<f64>) {
    let mut at = vec![Vec::new(); bots.len()];
    let mut done_at = None;
    while h.now.0 - begun < seconds {
        let tick = (h.now.0 - begun).round();
        h.step_by(1.0);
        for (i, b) in bots.iter().enumerate() {
            at[i].push((h.bots[*b].inbox.len(), tick));
        }
        if done_at.is_none() && h.server.shutdown == Shutdown::Done {
            done_at = Some(tick);
        }
    }
    (at, done_at)
}

/// A shutdown the operator gave ten minutes and a message: each player is shown the message first,
/// then "10 minutes" at once, "5 minutes" at Empyrean's mark, and the 2017 lines from two minutes
/// on, thirty seconds apart; the server is done twenty seconds after the last.
/// Behaviour: server.operator.shutdown-message-and-interval
#[test]
fn a_shutdown_shows_the_operators_message_and_counts_down_the_operators_interval() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Tamsin");
    h.run(5);
    h.server.cfg.shutdown_seconds = 600;
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    let begun = h.now.0;
    let clock = h.clock();
    assert!(shutdown::begin(
        &mut h.server,
        &mut h.net,
        clock,
        Some("  Back in ten.  ")
    ));
    assert!(
        !shutdown::begin(&mut h.server, &mut h.net, clock, None),
        "one countdown at a time"
    );
    let (at, done) = watch(&mut h, &[a, b], begun, 640.0);
    let want = vec![
        (0.0, Shown::Literal(256, "Back in ten.\n".into())),
        (0.0, Shown::Line(shutdown::MINUTES, vec![10])),
        (300.0, Shown::Line(shutdown::MINUTES, vec![5])),
        (480.0, Shown::Line(shutdown::MINUTES, vec![2])),
        (
            510.0,
            Shown::Line(shutdown::MINUTES_AND_SECONDS, vec![1, 30]),
        ),
        (540.0, Shown::Line(shutdown::MINUTES, vec![1])),
        (570.0, Shown::Line(shutdown::SECONDS, vec![30])),
        (600.0, Shown::Line(shutdown::NOW, vec![])),
    ];
    for (i, bot) in [a, b].into_iter().enumerate() {
        assert_eq!(
            shown(&h, bot, from[i], &at[i]),
            want,
            "what player {i} was shown, and when"
        );
    }
    assert_eq!(done, Some(620.0), "done twenty seconds after the last line");
}

/// The countdown an update warns players with: every player in the world is shown its own length
/// at once, the lines follow as a shutdown's do, and the server is done twenty seconds after the
/// last; a second one, or one during an operator's shutdown, begins nothing.
/// Behaviour: server.update.players-warned-by-the-shutdown-countdown
#[test]
fn an_updates_warning_counts_down_its_own_seconds_to_every_player() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Tamsin");
    h.run(5);
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    let begun = h.now.0;
    let clock = h.clock();
    assert!(shutdown::begin_warning(
        &mut h.server,
        &mut h.net,
        clock,
        300
    ));
    assert_eq!(h.server.cfg.shutdown_seconds, 300);
    assert!(
        !shutdown::begin_warning(&mut h.server, &mut h.net, clock, 60),
        "one countdown at a time"
    );
    assert_eq!(
        h.server.cfg.shutdown_seconds, 300,
        "and it keeps its length"
    );
    let (at, done) = watch(&mut h, &[a, b], begun, 340.0);
    let want = vec![
        (0.0, Shown::Line(shutdown::MINUTES, vec![5])),
        (180.0, Shown::Line(shutdown::MINUTES, vec![2])),
        (
            210.0,
            Shown::Line(shutdown::MINUTES_AND_SECONDS, vec![1, 30]),
        ),
        (240.0, Shown::Line(shutdown::MINUTES, vec![1])),
        (270.0, Shown::Line(shutdown::SECONDS, vec![30])),
        (300.0, Shown::Line(shutdown::NOW, vec![])),
    ];
    for (i, bot) in [a, b].into_iter().enumerate() {
        assert_eq!(
            shown(&h, bot, from[i], &at[i]),
            want,
            "what player {i} was shown, and when"
        );
    }
    assert_eq!(done, Some(320.0), "done twenty seconds after the last line");
}

/// A countdown cancelled after two lines: every player in the world is told the world is no
/// longer shutting down, no more lines come, the world goes on, and a later shutdown counts down
/// afresh. Cancelling with no countdown running does nothing.
/// Behaviour: server.operator.shutdown-cancelled
#[test]
fn a_cancelled_shutdown_tells_every_player_and_the_world_goes_on() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Tamsin");
    h.run(5);
    assert!(
        !shutdown::cancel(&mut h.server, &mut h.net),
        "nothing to cancel"
    );
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    let begun = h.now.0;
    let clock = h.clock();
    shutdown::begin(&mut h.server, &mut h.net, clock, None);
    let (mut at, _) = watch(&mut h, &[a, b], begun, 40.0);
    assert!(shutdown::cancel(&mut h.server, &mut h.net));
    assert_eq!(h.server.shutdown, Shutdown::No);
    let (later, done) = watch(&mut h, &[a, b], begun, 300.0);
    assert_eq!(done, None, "the server goes on");
    let cancelled = Shown::Literal(256, format!("{}\n", shutdown::CANCELLED));
    for (i, bot) in [a, b].into_iter().enumerate() {
        at[i].extend(later[i].iter().copied());
        let got: Vec<Shown> = shown(&h, bot, from[i], &at[i])
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert_eq!(
            got,
            vec![
                Shown::Line(shutdown::MINUTES, vec![2]),
                Shown::Line(shutdown::MINUTES_AND_SECONDS, vec![1, 30]),
                cancelled.clone(),
            ],
            "two lines, then the cancel, then nothing"
        );
    }
    let again = h.bots[a].inbox.len();
    let clock = h.clock();
    assert!(shutdown::begin(&mut h.server, &mut h.net, clock, None));
    h.run(3);
    let first: Vec<Shown> = shown(&h, a, again, &[])
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    assert_eq!(
        first,
        vec![Shown::Line(shutdown::MINUTES, vec![2])],
        "a later shutdown counts down afresh"
    );
}

/// Stopping now, in the middle of a countdown: every player is shown the operator's message and
/// the last line, and the server may stop at once, not twenty seconds on; no line of the
/// countdown comes after.
/// Behaviour: server.operator.stop-now
#[test]
fn stopping_now_shows_the_last_line_and_lets_the_server_stop_at_once() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    h.run(5);
    let clock = h.clock();
    shutdown::begin(&mut h.server, &mut h.net, clock, None);
    h.run(3);
    let from = h.bots[a].inbox.len();
    let clock = h.clock();
    assert!(shutdown::stop_now(
        &mut h.server,
        &mut h.net,
        clock,
        Some("Restarting.")
    ));
    assert_eq!(h.server.shutdown, Shutdown::Done, "the server may stop now");
    assert!(
        !shutdown::stop_now(&mut h.server, &mut h.net, clock, None),
        "once"
    );
    let begun = h.now.0;
    let (at, _) = watch(&mut h, &[a], begun, 60.0);
    assert_eq!(
        shown(&h, a, from, &at[0])
            .into_iter()
            .map(|(_, s)| s)
            .collect::<Vec<_>>(),
        vec![
            Shown::Literal(256, "Restarting.\n".into()),
            Shown::Line(shutdown::NOW, vec![]),
        ]
    );
    assert_eq!(h.server.shutdown, Shutdown::Done);
}

/// A world that runs no pass for the limit's looks is stopped once, from where it was.
/// Behaviour: server.operator.a-world-that-runs-no-pass-is-stopped
#[test]
fn a_world_that_runs_no_pass_for_the_limit_is_stopped_once() {
    use yalain_world::guard::{watch, Part, Passes};
    let passes = Passes::default();
    passes.pass();
    passes.at(Part::World);
    let mut looks = 0;
    let mut stopped = Vec::new();
    assert!(watch(&passes, 5, || looks += 1, |part| stopped.push(part)));
    assert_eq!(stopped, [Part::World], "stopped once, from where it was");
    assert_eq!(looks, 5, "after the limit, its first look seeing the count");
}

/// A world that runs a pass between any two looks is never stopped, however many looks go by, and
/// its passes being over ends the watch.
/// Behaviour: server.operator.a-world-that-runs-no-pass-is-stopped
#[test]
fn a_world_still_running_passes_is_not_stopped_and_its_end_ends_the_watch() {
    use yalain_world::guard::{watch, Passes};
    let passes = Passes::default();
    let mut looks = 0;
    let mut stopped = false;
    let ended = watch(
        &passes,
        2,
        || {
            looks += 1;
            passes.pass();
            if looks == 1_000 {
                passes.over();
            }
        },
        |_| stopped = true,
    );
    assert!(!ended && !stopped, "never stopped");
    assert_eq!(looks, 1_000);
}

/// A limit of nothing never stops the world, and a pass after looks with none starts the count again.
/// Behaviour: server.operator.a-world-that-runs-no-pass-is-stopped
#[test]
fn a_limit_of_nothing_never_trips_and_a_pass_starts_the_count_again() {
    use yalain_world::guard::PassWatch;
    let mut never = PassWatch::new(0);
    assert!((0..1_000).all(|_| !never.look(7)));
    let mut w = PassWatch::new(3);
    assert!(!w.look(1) && !w.look(1) && !w.look(1), "two looks still");
    assert!(!w.look(2), "a pass");
    assert!(!w.look(2) && !w.look(2), "two looks still");
    assert!(w.look(2), "the third");
}
