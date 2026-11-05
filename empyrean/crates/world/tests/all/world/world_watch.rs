//! Divergence: V508
//! A world that runs no pass for the limit's looks is stopped once, from the stage it reached. The
//! watch reads the counts the loop keeps, and counts its own looks, so no test here waits on a clock.
//! Fixture: none; the real loop's counting is in `world_loop`.

use empyrean_world::managers::world_watch::{watch, PassWatch, Passes};

/// A world that runs no pass for the limit's looks is stopped once, from where it was.
#[test]
fn a_world_that_runs_no_pass_for_the_limit_is_stopped_once() {
    let passes = Passes::default();
    passes.pass();
    passes.at("WorldManager.UpdateGameWorld");
    let mut looks = 0;
    let mut stopped = Vec::new();
    assert!(watch(
        &passes,
        5,
        || looks += 1,
        |stage| stopped.push(stage)
    ));
    assert_eq!(
        stopped,
        ["WorldManager.UpdateGameWorld"],
        "stopped once, from where it was"
    );
    assert_eq!(looks, 5, "after the limit, its first look seeing the count");
}

/// A world running a pass between any two looks is never stopped, and its passes being over ends the
/// watch.
#[test]
fn a_world_still_running_passes_is_not_stopped_and_its_end_ends_the_watch() {
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

/// A limit of nothing never trips, a pass after looks with none starts the count again, and the time
/// before the first pass doesn't count, however long the world takes to load.
#[test]
fn only_looks_in_a_row_at_a_running_world_count() {
    let mut never = PassWatch::new(0);
    assert!((0..1_000).all(|_| !never.look(7)));

    let mut starting = PassWatch::new(3);
    assert!((0..1_000).all(|_| !starting.look(0)), "no pass has begun");

    let mut w = PassWatch::new(3);
    assert!(!w.look(1));
    assert!(!w.look(1));
    assert!(!w.look(1));
    assert!(!w.look(2), "a pass starts the count again");
    assert!(!w.look(2));
    assert!(!w.look(2));
    assert!(w.look(2), "three looks seeing no new pass");
}
