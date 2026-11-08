//! The world's step timer: each step's work since the step before, by stage, the figures drawn
//! from the last minute's steps, and the warning when a step's work passes the step. Fixture: the
//! timer on a clock each test turns itself, and the harness's world with a few creatures on a clock
//! that moves on by a fixed amount each time it is read, so no figure depends on the machine.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use yalain_testkit::Harness;
use yalain_world::step_timer::{Said, Stage, StepTimer, RECENT_STEPS};

/// A clock the test sets, and a timer reading it.
fn timer() -> (StepTimer, Arc<Mutex<f64>>) {
    let at = Arc::new(Mutex::new(0.0));
    let read = Arc::clone(&at);
    let mut t = StepTimer::default();
    t.set_clock(Box::new(move || {
        *read
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }));
    (t, at)
}

/// One step's work: each stage's seconds in turn, on the test's clock; then the step ends at
/// `world_now`. Returns what the timer had to say, if anything.
fn step(t: &mut StepTimer, at: &Mutex<f64>, work: &[(Stage, f64)], world_now: f64) -> Option<Said> {
    t.begin();
    for (stage, seconds) in work {
        *at.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += seconds;
        t.lap(*stage);
    }
    t.step_ends(world_now)
}

/// The world's first step, which fills its camps: two seconds of camps.
fn first_step(t: &mut StepTimer, at: &Mutex<f64>) -> Option<Said> {
    step(t, at, &[(Stage::Camps, 2.0)], -1.0)
}

/// A clock that moves on by `seconds` each time it is read.
fn ticking(seconds: f64) -> Box<dyn Fn() -> f64 + Send> {
    let reads = AtomicU64::new(0);
    #[allow(clippy::cast_precision_loss)]
    Box::new(move || reads.fetch_add(1, Ordering::Relaxed) as f64 * seconds)
}

/// The world's first step fills every camp at once: its time is told as news, kept as its own
/// figure, and it is no overrun, however long it took.
/// Behaviour: server.operator.slow-world-steps-warned
#[test]
fn the_first_step_which_fills_the_camps_is_told_as_news_and_is_no_overrun() {
    let (mut t, at) = timer();
    assert_eq!(
        first_step(&mut t, &at),
        Some(Said::First(
            "the world's first step, which fills its camps, took 2000.0 ms".into()
        ))
    );
    let f = t.figures();
    assert_eq!(
        (
            f.steps,
            f.overruns,
            f.recent_steps,
            f.first_us,
            f.worst_ever_us
        ),
        (1, 0, 0, 2_000_000, 0)
    );
    assert_eq!(t.warnings(), 0);
}

/// A step whose work passes the step's quarter second is warned of at once, naming its largest
/// stages; a step of exactly the budget is not an overrun. After a warning the next comes no sooner
/// than a minute later on the world's clock, and counts the overruns since the last.
/// Behaviour: server.operator.slow-world-steps-warned
#[test]
fn a_slow_step_is_warned_of_at_once_then_once_a_minute_counting_the_slow_steps_between() {
    let (mut t, at) = timer();
    first_step(&mut t, &at);
    assert_eq!(step(&mut t, &at, &[(Stage::Creatures, 0.25)], 0.0), None);
    let first = step(
        &mut t,
        &at,
        &[
            (Stage::Network, 0.05),
            (Stage::Creatures, 0.25),
            (Stage::Saves, 0.01),
        ],
        0.25,
    );
    assert_eq!(
        first,
        Some(Said::Slow(
            "a world step's work took 310.0 ms, over the step's 250.0 ms (creatures 250.0 ms, \
             network 50.0 ms, saves 10.0 ms)"
                .into()
        ))
    );
    // Slow steps for the next minute but a quarter second: none warned of.
    let mut now = 0.25;
    for _ in 0..239 {
        now += 0.25;
        assert_eq!(step(&mut t, &at, &[(Stage::Camps, 0.3)], now), None);
    }
    assert_eq!(t.warnings(), 1);
    now += 0.25;
    assert_eq!(
        step(&mut t, &at, &[(Stage::Camps, 0.3)], now),
        Some(Said::Slow(
            "a world step's work took 300.0 ms, over the step's 250.0 ms (camps 300.0 ms); 239 \
             more since the last warning"
                .into()
        ))
    );
    assert_eq!((t.warnings(), t.figures().overruns), (2, 241));
}

/// The figures are drawn from the last minute's steps: the last, the mean, the 99th percentile and
/// the worst step's work and how many overran, and each stage's mean and worst; since the start,
/// the steps, the overruns and the worst. A slow step leaves the recent figures a minute of steps
/// later, and stays in the worst since the start. The first step is kept apart, as its own figure.
/// Behaviour: server.operator.world-step-times-by-stage
#[test]
fn the_figures_give_the_last_minutes_last_mean_p99_and_worst_by_stage_and_the_worst_since_the_start(
) {
    let (mut t, at) = timer();
    first_step(&mut t, &at);
    for i in 0..99 {
        step(&mut t, &at, &[(Stage::Creatures, 0.010)], f64::from(i));
    }
    step(
        &mut t,
        &at,
        &[(Stage::Network, 0.280), (Stage::Creatures, 0.020)],
        99.0,
    );
    let f = t.figures().clone();
    assert_eq!(
        (f.steps, f.recent_steps, f.overruns, f.recent_overruns),
        (101, 100, 1, 1)
    );
    assert_eq!(
        (f.last_us, f.mean_us, f.p99_us, f.worst_us, f.worst_ever_us),
        (300_000, 12_900, 10_000, 300_000, 300_000)
    );
    assert_eq!(f.first_us, 2_000_000);
    let stage = |s: Stage| {
        f.stages
            .iter()
            .find(|(t, _)| *t == s)
            .map(|(_, v)| (v.mean_us, v.worst_us))
            .expect("every stage")
    };
    assert_eq!(stage(Stage::Creatures), (10_100, 20_000));
    assert_eq!(stage(Stage::Network), (2_800, 280_000));
    assert_eq!(stage(Stage::Camps), (0, 0), "the first step is apart");
    assert_eq!(stage(Stage::Saves), (0, 0));
    assert_eq!(f.stages.len(), Stage::ALL.len());
    for i in 0..RECENT_STEPS {
        #[allow(clippy::cast_precision_loss)]
        step(&mut t, &at, &[(Stage::Creatures, 0.010)], 100.0 + i as f64);
    }
    let f = t.figures();
    assert_eq!(
        (f.steps, f.recent_steps, f.overruns, f.recent_overruns),
        (341, RECENT_STEPS, 1, 0)
    );
    assert_eq!(
        (f.mean_us, f.p99_us, f.worst_us, f.worst_ever_us),
        (10_000, 10_000, 10_000, 300_000)
    );
}

/// The world keeps one step's work for each step it takes, its creatures that acted counted, and
/// charges its work to the stages it does: the network's events, the timers, the camps, the
/// creatures, the rest of the step, the fights, the saves and the rest of the pass. The transport
/// is the host's to charge, and the harness charges none.
/// Behaviour: server.operator.world-step-times-by-stage
#[test]
fn the_world_keeps_each_steps_work_by_the_stages_it_does() {
    let mut h = Harness::new();
    h.add_creatures(3);
    h.server.step_timer.set_clock(ticking(0.001));
    let mut steps = 0;
    for _ in 0..100 {
        let before = h.server.last_step;
        h.step();
        if h.server.last_step != before {
            steps += 1;
        }
    }
    let f = h.server.step_timer.figures();
    assert!(steps >= 15, "{steps} steps in five seconds");
    assert_eq!(f.steps, steps);
    assert_eq!(f.creatures, 3);
    for (stage, v) in &f.stages {
        if *stage == Stage::Transport {
            assert_eq!(v.worst_us, 0, "{}", stage.name());
        } else {
            assert!(v.mean_us > 0, "{} is charged", stage.name());
        }
    }
}

/// A world whose every step is slow warns at its first slow step, and then once a minute on the
/// world's clock: twice in a hundred seconds of steps, every one after the first counted as an
/// overrun.
/// Behaviour: server.operator.slow-world-steps-warned
#[test]
fn a_world_whose_every_step_is_slow_warns_once_a_minute() {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.step_timer.set_clock(ticking(0.3));
    for _ in 0..400 {
        h.step_by(0.25);
    }
    let f = h.server.step_timer.figures();
    assert_eq!((f.steps, f.overruns), (400, 399));
    assert_eq!(h.server.step_timer.warnings(), 2);
}

/// With no clock given, the world counts its steps and measures nothing.
/// Behaviour: server.operator.world-step-times-by-stage
#[test]
fn with_no_clock_the_world_counts_its_steps_and_measures_nothing() {
    let mut h = Harness::new();
    h.add_creatures(2);
    for _ in 0..20 {
        h.step_by(0.25);
    }
    let f = h.server.step_timer.figures();
    assert_eq!((f.steps, f.creatures), (20, 2));
    assert_eq!((f.first_us, f.worst_ever_us, f.overruns), (0, 0, 0));
    assert!(!h.server.step_timer.measures());
}
