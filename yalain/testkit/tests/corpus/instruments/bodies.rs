//! Behaviour: none (prints what following every 2017 report with a player's body costs)
//!
//! The replay of `invariants`' players' bodies, twice: following every report, and putting each body at every report
//! with the walls read just the same. Each prints its work; the cost of a follow is the difference
//! of the two runs' times, as the test harness reports them, over the follows. Run each alone with
//! `--ignored --nocapture --exact`. `YALAIN_BODIES_CAPTURE` names one capture to replay alone,
//! printing each report its body missed.

/// One capture alone, each missed report printed: `YALAIN_BODIES_CAPTURE=<capture id>`.
fn trace() -> Option<i64> {
    std::env::var("YALAIN_BODIES_CAPTURE")
        .ok()
        .and_then(|v| v.parse().ok())
}

#[test]
#[ignore = "instrument: prints the corrections players' bodies send over the 2017 reports, and the follows' work"]
fn the_2017_reports_followed_by_players_bodies() {
    let t = crate::invariants::replay(true, trace());
    crate::invariants::print(&t);
    #[allow(clippy::cast_precision_loss)]
    let moves = t.moves as f64 / t.follows.max(1) as f64;
    println!(
        "follows {}, moves through the transition {} ({moves:.2} a follow)",
        t.follows, t.moves
    );
}

#[test]
#[ignore = "instrument: the same replay with every body put at each report, the walls read alike"]
fn the_2017_reports_with_players_bodies_put_at_each() {
    let t = crate::invariants::replay(false, trace());
    println!("reports {:?}", t.counts.get("reports"));
}
