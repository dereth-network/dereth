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
    shutdown::begin(&mut h.server, &mut h.net, clock);
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
