//! The world's step timer: how long the world worked from one step to the next, by stage, and a
//! warning when that passes the step.
//!
//! The world takes a step every [`STEP_SECONDS`](crate::spawn::STEP_SECONDS). A step's work is all
//! the world did since the step before: the network's events, the timers that came due, the step
//! itself (its camps, its creatures, the rest of it), the fights, the saves and the rest of each
//! pass, and what the host charges for its own sending and receiving. A step whose work passes the
//! step's length leaves the world behind its clock: that is an overrun.
//!
//! **The clock is the host's.** The timer reads only the clock it is given ([`StepTimer::set_clock`]):
//! the server's executable gives it the machine's monotonic clock, a test a clock of its own. With
//! none it measures nothing and every step's work reads as nothing.
//!
//! **The first step** fills every camp of the world at once, so its work is the start's, not the
//! running world's: it is told apart (its time is logged as news, and kept as its own figure), and
//! is no overrun.
//!
//! **What is kept:** the last [`RECENT_STEPS`] steps' work after the first, by stage, from which the
//! figures are drawn (the last, the mean, the 99th percentile, the worst and the overruns), and since
//! the start, the steps, the overruns and the worst step after the first. The figures are drawn as
//! each step ends, so reading them costs nothing.
//!
//! **The warning:** the first overrun is warned of at once; after that, at most one warning a minute
//! of the world's clock, counting the overruns since the last.

use std::collections::VecDeque;
use std::fmt::Write as _;

/// A step's work past this many seconds is an overrun: the step's own length.
pub const BUDGET_SECONDS: f64 = crate::spawn::STEP_SECONDS;

/// How many of the last steps the figures are drawn from: a minute's.
pub const RECENT_STEPS: usize = 240;

/// The fewest seconds of the world's clock between two warnings.
pub const WARN_EVERY_SECONDS: f64 = 60.0;

/// A part of the world's work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// What came over the network: players' messages, logins and log-offs.
    Network,
    /// What came due on the world's timers: what players and creatures began earlier.
    Timers,
    /// Camps making the creatures and items they lack.
    Camps,
    /// Every creature acting.
    Creatures,
    /// The rest of the step: people walking, pets, corpses and veins going, regeneration, talk.
    Step,
    /// Swings and creatures' attacks coming due.
    Fights,
    /// Keeping characters in the store.
    Saves,
    /// The rest of each pass: logins due, moderation, fellowships, trades, the market, drops,
    /// portal storms, the views owed, the world's patch.
    Other,
    /// The host's own sending and receiving, as the host charges it.
    Transport,
}

impl Stage {
    /// Every stage, in the order the figures give them.
    pub const ALL: [Stage; 9] = [
        Stage::Network,
        Stage::Timers,
        Stage::Camps,
        Stage::Creatures,
        Stage::Step,
        Stage::Fights,
        Stage::Saves,
        Stage::Other,
        Stage::Transport,
    ];

    /// Its name in the figures and the log.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Stage::Network => "network",
            Stage::Timers => "timers",
            Stage::Camps => "camps",
            Stage::Creatures => "creatures",
            Stage::Step => "step",
            Stage::Fights => "fights",
            Stage::Saves => "saves",
            Stage::Other => "other",
            Stage::Transport => "transport",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// How many stages there are.
const STAGES: usize = Stage::ALL.len();

/// A clock: seconds since any fixed moment, never going back.
pub type WorkClock = Box<dyn Fn() -> f64 + Send>;

/// One stage's figures over the recent steps, in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageFigures {
    pub mean_us: u64,
    pub worst_us: u64,
}

/// What the timer says of the world's steps; times in microseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Figures {
    /// The step's budget.
    pub budget_us: u64,
    /// Steps since the start, the first among them, and how many overran.
    pub steps: u64,
    pub overruns: u64,
    /// The first step's work.
    pub first_us: u64,
    /// The worst step since the start, after the first.
    pub worst_ever_us: u64,
    /// Creatures that acted at the last step.
    pub creatures: usize,
    /// The recent steps the figures below are drawn from, and how many overran.
    pub recent_steps: usize,
    pub recent_overruns: usize,
    pub last_us: u64,
    pub mean_us: u64,
    pub p99_us: u64,
    pub worst_us: u64,
    /// Each stage's, in [`Stage::ALL`]'s order.
    pub stages: Vec<(Stage, StageFigures)>,
}

/// The time a step's work took, in all and by stage, in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Work {
    total: f64,
    stages: [f64; STAGES],
}

/// What the timer has to say as a step ends, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// The first step's work: news.
    First(String),
    /// A step overran: a warning.
    Slow(String),
}

/// The world's step timer.
#[derive(Default)]
pub struct StepTimer {
    clock: Option<WorkClock>,
    /// When the stage under way began, as the clock read.
    lap_from: Option<f64>,
    /// The work since the last step ended, by stage.
    current: [f64; STAGES],
    recent: VecDeque<Work>,
    steps: u64,
    overruns: u64,
    first: f64,
    worst_ever: f64,
    creatures: usize,
    /// When the last warning was given, on the world's clock; and the overruns since.
    warned_at: Option<f64>,
    unwarned: u64,
    warnings: u64,
    figures: Figures,
}

impl std::fmt::Debug for StepTimer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepTimer")
            .field("clock", &self.clock.is_some())
            .field("steps", &self.steps)
            .field("overruns", &self.overruns)
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

/// Seconds as whole microseconds.
fn micros(seconds: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let us = (seconds.max(0.0) * 1e6).round() as u64;
    us
}

/// Seconds as milliseconds for a person, to a tenth.
fn ms(seconds: f64) -> String {
    format!("{:.1} ms", seconds * 1e3)
}

impl StepTimer {
    /// The clock the timer reads from now on.
    pub fn set_clock(&mut self, clock: WorkClock) {
        self.clock = Some(clock);
        self.lap_from = None;
    }

    /// Whether the timer has a clock to read.
    #[must_use]
    pub fn measures(&self) -> bool {
        self.clock.is_some()
    }

    fn read(&self) -> Option<f64> {
        self.clock.as_ref().map(|c| c())
    }

    /// The world's work begins (again) now: the time since the last stage is no stage's.
    pub fn begin(&mut self) {
        self.lap_from = self.read();
    }

    /// The work since the last stage ended, or since [`StepTimer::begin`], was `stage`'s.
    pub fn lap(&mut self, stage: Stage) {
        let Some(now) = self.read() else { return };
        if let Some(from) = self.lap_from {
            self.current[stage.index()] += (now - from).max(0.0);
        }
        self.lap_from = Some(now);
    }

    /// Charge `seconds` of work to `stage`, measured by the host.
    pub fn charge(&mut self, stage: Stage, seconds: f64) {
        self.current[stage.index()] += seconds.max(0.0);
    }

    /// How many creatures acted at this step.
    pub fn set_creatures(&mut self, n: usize) {
        self.creatures = n;
    }

    /// A step has ended at `world_now` on the world's clock: its work is kept and the figures drawn
    /// again. Returns what to log: the first step's time, or a warning when the step overran and
    /// one is due.
    pub fn step_ends(&mut self, world_now: f64) -> Option<Said> {
        let stages = std::mem::take(&mut self.current);
        let work = Work {
            total: stages.iter().sum(),
            stages,
        };
        self.steps += 1;
        if self.steps == 1 {
            self.first = work.total;
            self.figures = self.draw();
            return self.clock.is_some().then(|| {
                Said::First(format!(
                    "the world's first step, which fills its camps, took {}",
                    ms(work.total)
                ))
            });
        }
        self.worst_ever = self.worst_ever.max(work.total);
        if self.recent.len() == RECENT_STEPS {
            self.recent.pop_front();
        }
        self.recent.push_back(work);
        let mut warning = None;
        if work.total > BUDGET_SECONDS {
            self.overruns += 1;
            self.unwarned += 1;
            if self
                .warned_at
                .is_none_or(|t| world_now - t >= WARN_EVERY_SECONDS)
            {
                warning = Some(Said::Slow(self.warning(&work)));
                self.warned_at = Some(world_now);
                self.unwarned = 0;
                self.warnings += 1;
            }
        }
        self.figures = self.draw();
        warning
    }

    /// The warning for an overrun: the step's work, its budget, its largest stages, and the
    /// overruns since the last warning.
    fn warning(&self, work: &Work) -> String {
        let mut stages: Vec<(Stage, f64)> = Stage::ALL
            .iter()
            .map(|s| (*s, work.stages[s.index()]))
            .filter(|(_, t)| *t > 0.0)
            .collect();
        stages.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut text = format!(
            "a world step's work took {}, over the step's {}",
            ms(work.total),
            ms(BUDGET_SECONDS)
        );
        if !stages.is_empty() {
            let parts: Vec<String> = stages
                .iter()
                .take(3)
                .map(|(s, t)| format!("{} {}", s.name(), ms(*t)))
                .collect();
            let _ = write!(text, " ({})", parts.join(", "));
        }
        // This step is among those counted.
        let more = self.unwarned.saturating_sub(1);
        if more > 0 {
            let _ = write!(text, "; {more} more since the last warning");
        }
        text
    }

    /// The figures as they stand.
    fn draw(&self) -> Figures {
        let n = self.recent.len();
        let mut totals: Vec<f64> = self.recent.iter().map(|w| w.total).collect();
        totals.sort_by(f64::total_cmp);
        #[allow(clippy::cast_precision_loss)]
        let count = n.max(1) as f64;
        let mean = |f: &dyn Fn(&Work) -> f64| self.recent.iter().map(f).sum::<f64>() / count;
        let worst = |f: &dyn Fn(&Work) -> f64| self.recent.iter().map(f).fold(0.0, f64::max);
        // The 99th percentile: the least step at least 99 of every hundred are no longer than.
        let p99 = if n == 0 {
            0.0
        } else {
            totals[(n * 99).div_ceil(100) - 1]
        };
        Figures {
            budget_us: micros(BUDGET_SECONDS),
            steps: self.steps,
            overruns: self.overruns,
            first_us: micros(self.first),
            worst_ever_us: micros(self.worst_ever),
            creatures: self.creatures,
            recent_steps: n,
            recent_overruns: self
                .recent
                .iter()
                .filter(|w| w.total > BUDGET_SECONDS)
                .count(),
            last_us: micros(self.recent.back().map_or(0.0, |w| w.total)),
            mean_us: micros(mean(&|w| w.total)),
            p99_us: micros(p99),
            worst_us: micros(worst(&|w| w.total)),
            stages: Stage::ALL
                .iter()
                .map(|s| {
                    let i = s.index();
                    (
                        *s,
                        StageFigures {
                            mean_us: micros(mean(&|w| w.stages[i])),
                            worst_us: micros(worst(&|w| w.stages[i])),
                        },
                    )
                })
                .collect(),
        }
    }

    /// The figures, as drawn when the last step ended.
    #[must_use]
    pub fn figures(&self) -> &Figures {
        &self.figures
    }

    /// How many warnings have been given.
    #[must_use]
    pub fn warnings(&self) -> u64 {
        self.warnings
    }
}
