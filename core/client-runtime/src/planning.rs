//! The travel's planning, off the frame.
//!
//! Building a region of standing places, searching it for a route and planning a journey take
//! from a few milliseconds to a few seconds, far longer than a frame. They are done here, out of
//! the frame's way: on a thread of their own where the platform has threads, and a slice of a
//! few milliseconds each frame where it has none (the web). The frame asks with a
//! [`Job`](crate::planning::Job) and hears back with an [`Answer`](crate::planning::Answer) some
//! frames later; it never waits.
//!
//! The planner reads the data files through the land source the world shares with physics, and
//! decodes a landblock's cells the first time a plan needs them, here, not in the frame.

use std::collections::VecDeque;
use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_navigation::{
    plan_journey_with, Done, HandBack, Journey, JourneyConfig, NavPath, NoPath, Obstacle,
    PortalEdge, Realm, Task, Teleport,
};
use dereth_primitives::{CellId, ObjectId, Position};
use dereth_world_data::land_source::DatLandSource;

use crate::travel::{blocks_round, middle, planner, Planner};

/// Where a route or a journey goes, as the frame knows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goal {
    /// A place.
    Place(Position),
    /// A cell: an interior cell's middle, or a land cell's middle on the ground.
    Cell(CellId),
}

/// What the frame asks of the planning.
#[derive(Debug)]
pub enum Job {
    /// Build the region round `at`, ahead of any need for it.
    Prepare(Position),
    /// A walking route from `from` as far toward `to` as one window holds: within `reach` of it
    /// when given, and with `snap`, to the nearest place to stand within that far of it first.
    Route {
        id: u64,
        from: Position,
        to: Goal,
        reach: Option<f32>,
        snap: Option<f32>,
    },
    /// A journey from `from` to `to` over walking and these portals, teleports and hand-backs:
    /// `to` taken to the nearest place to stand first, and the first walk's route planned too,
    /// ending within `reach` of `to` when the journey is one walk and `reach` is given.
    Journey {
        id: u64,
        from: Position,
        to: Goal,
        portals: Vec<PortalEdge>,
        teleports: Vec<Teleport>,
        hand_backs: Vec<HandBack>,
        reach: Option<f32>,
    },
    /// The objects in the world now (doors, other solid objects, creatures), replacing the last.
    Obstacles(Vec<Obstacle>),
    /// Take out the places just ahead of where a walk stopped.
    Block { at: Position, toward: Position },
    /// Put back every place a stopped walk took out.
    Unblock,
}

/// What the planning answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// A [`Job::Route`]'s route, the place it went to (after taking it to a place to stand), and
    /// the milliseconds it took to plan.
    Route {
        id: u64,
        to: Option<Position>,
        route: Result<(NavPath, bool), NoPath>,
        ms: f64,
    },
    /// A [`Job::Journey`]'s goal as a place to stand (`None` when there is no such place), the
    /// journey with the milliseconds it took to plan, and its first walk's route.
    Journey {
        id: u64,
        to: Option<Position>,
        journey: Option<(Journey, f64)>,
        first: Option<Result<(NavPath, bool), NoPath>>,
    },
    /// A [`Job::Unblock`]: how many places were put back.
    Unblocked(usize),
}

/// How many units of work (a place sampled, decided or settled) a slice is spent in: about a
/// millisecond's.
const SLICE: u64 = 1024;

/// The milliseconds of a frame the planning takes where it shares the frame's thread.
const FRAME_SHARE_MS: f64 = 4.0;

/// A job under way.
#[derive(Debug)]
enum Running {
    /// A region built ahead of need.
    Prepare(Box<Task>),
    /// A route.
    Route {
        id: u64,
        task: Box<Task>,
        clock: web_time::Instant,
    },
    /// A journey's goal taken to a place to stand.
    Goal { id: u64, task: Box<Task>, ask: Ask },
    /// A journey's first walk planned.
    First {
        id: u64,
        task: Box<Task>,
        to: Position,
        journey: (Journey, f64),
    },
}

/// What a journey was asked over.
#[derive(Debug)]
struct Ask {
    from: Position,
    portals: Vec<PortalEdge>,
    teleports: Vec<Teleport>,
    hand_backs: Vec<HandBack>,
    reach: Option<f32>,
}

/// The planner and its jobs, wherever they run.
struct Worker {
    land: Arc<DatLandSource>,
    nav: Planner,
    queue: VecDeque<Job>,
    /// The landblocks whose cells the next job reads, still to decode, one a slice; and the job.
    loading: VecDeque<dereth_primitives::LandblockId>,
    loaded_for: Option<Job>,
    running: Option<Running>,
    /// The objects in the world last set.
    obstacles: Vec<ObjectId>,
}

impl Worker {
    fn new(land: Arc<DatLandSource>, store: &Arc<RetailDatStore>) -> Result<Self, String> {
        let nav = planner(&land, store)?;
        Ok(Self {
            land,
            nav,
            queue: VecDeque::new(),
            loading: VecDeque::new(),
            loaded_for: None,
            running: None,
            obstacles: Vec::new(),
        })
    }

    /// Where a goal is, once its landblock's cells are decoded.
    fn place_of(&self, goal: Goal) -> Option<Position> {
        match goal {
            Goal::Place(p) => Some(p),
            Goal::Cell(c) => middle(&self.land, c),
        }
    }

    /// The landblocks whose cells `job` reads: round its two ends, as the client keeps them.
    fn blocks_of(job: &Job) -> Vec<dereth_primitives::LandblockId> {
        let cell = |g: &Goal| match g {
            Goal::Place(p) => p.cell,
            Goal::Cell(c) => *c,
        };
        match job {
            Job::Prepare(at) => blocks_round(at.cell, at.cell),
            Job::Route { from, to, .. } => blocks_round(from.cell, cell(to)),
            Job::Journey { from, to, .. } => {
                let mut v = blocks_round(from.cell, from.cell);
                v.extend(blocks_round(cell(to), cell(to)));
                v
            }
            Job::Obstacles(_) | Job::Block { .. } | Job::Unblock => Vec::new(),
        }
    }

    /// Start the next job, or do a quick one whole.
    fn start(&mut self, job: Job, out: &mut Vec<Answer>) {
        match job {
            Job::Obstacles(list) => {
                for id in std::mem::take(&mut self.obstacles) {
                    self.nav.remove_obstacle(id);
                }
                for o in list {
                    self.obstacles.push(o.id);
                    self.nav.set_obstacle(o);
                }
            }
            Job::Block { at, toward } => {
                self.nav.block_ahead(&at, &toward);
            }
            Job::Unblock => out.push(Answer::Unblocked(self.nav.unblock())),
            Job::Prepare(at) => {
                self.running = Some(Running::Prepare(Box::new(Task::place(at, 0.0))));
            }
            Job::Route {
                id,
                from,
                to,
                reach,
                snap,
            } => {
                let clock = web_time::Instant::now();
                let Some(to) = self.place_of(to) else {
                    out.push(Answer::Route {
                        id,
                        to: None,
                        route: Err(NoPath::NoStandingPlace(dereth_navigation::End::Goal)),
                        ms: 0.0,
                    });
                    return;
                };
                let task = match reach {
                    Some(r) => Task::approach(from, to, r, snap),
                    None => Task::route(from, to, snap),
                };
                self.running = Some(Running::Route {
                    id,
                    task: Box::new(task),
                    clock,
                });
            }
            Job::Journey {
                id,
                from,
                to,
                portals,
                teleports,
                hand_backs,
                reach,
            } => {
                let Some(to) = self.place_of(to) else {
                    out.push(Answer::Journey {
                        id,
                        to: None,
                        journey: None,
                        first: None,
                    });
                    return;
                };
                self.running = Some(Running::Goal {
                    id,
                    task: Box::new(Task::place(to, 12.0)),
                    ask: Ask {
                        from,
                        portals,
                        teleports,
                        hand_backs,
                        reach,
                    },
                });
            }
        }
    }

    /// The journey from `from` to `to`, and how long it took to plan.
    fn journey(&self, ask: &Ask, to: &Position) -> Option<(Journey, f64)> {
        let clock = web_time::Instant::now();
        let nav = &self.nav;
        // An interior cell of a landblock that is not loaded is taken for a dungeon, as nearly
        // every portal's far end is: loading every landblock a portal names would cost more than
        // the plan. The landblocks round the body and the goal are loaded, so a building's inside
        // there is told from a dungeon.
        let realm = |p: &Position| {
            nav.realm(p).or_else(|| {
                (p.cell.0 & 0xFFFF >= 0x0100)
                    .then(|| Realm::Dungeon(u16::try_from(p.cell.0 >> 16).unwrap_or(0)))
            })
        };
        let portals: Vec<&PortalEdge> = ask.portals.iter().collect();
        let j = plan_journey_with(
            &ask.from,
            to,
            &portals,
            &ask.teleports,
            &ask.hand_backs,
            &realm,
            &JourneyConfig::default(),
        )?;
        Some((j, clock.elapsed().as_secs_f64() * 1000.0))
    }

    /// Work on the running job for a slice; its answer, and whether it is over.
    fn slice(&mut self, units: u64, out: &mut Vec<Answer>) {
        let Some(running) = self.running.take() else {
            return;
        };
        let mut budget = units;
        self.running = match running {
            Running::Prepare(mut task) => match self.nav.advance(&mut task, &mut budget) {
                Some(_) => None,
                None => Some(Running::Prepare(task)),
            },
            Running::Route {
                id,
                mut task,
                clock,
            } => match self.nav.advance(&mut task, &mut budget) {
                Some(Done::Route(route)) => {
                    out.push(Answer::Route {
                        id,
                        to: Some(task.destination()),
                        route,
                        ms: clock.elapsed().as_secs_f64() * 1000.0,
                    });
                    None
                }
                Some(Done::Place(_)) => None,
                None => Some(Running::Route { id, task, clock }),
            },
            Running::Goal { id, mut task, ask } => match self.nav.advance(&mut task, &mut budget) {
                Some(done) => {
                    let to = match done {
                        Done::Place(Ok(p)) => p,
                        _ => task.destination(),
                    };
                    match self.journey(&ask, &to) {
                        Some(journey) => {
                            // The first walk's route, as the journey's first walk plans it.
                            let first = match journey.0.legs.first() {
                                Some(dereth_navigation::Leg::Walk { to: end }) => {
                                    let reach = ask.reach.filter(|_| journey.0.legs.len() == 1);
                                    Some(match reach {
                                        Some(r) => Task::approach(ask.from, *end, r, None),
                                        None => Task::route(ask.from, *end, None),
                                    })
                                }
                                _ => None,
                            };
                            match first {
                                Some(task) => Some(Running::First {
                                    id,
                                    task: Box::new(task),
                                    to,
                                    journey,
                                }),
                                None => {
                                    out.push(Answer::Journey {
                                        id,
                                        to: Some(to),
                                        journey: Some(journey),
                                        first: None,
                                    });
                                    None
                                }
                            }
                        }
                        None => {
                            out.push(Answer::Journey {
                                id,
                                to: Some(to),
                                journey: None,
                                first: None,
                            });
                            None
                        }
                    }
                }
                None => Some(Running::Goal { id, task, ask }),
            },
            Running::First {
                id,
                mut task,
                to,
                journey,
            } => match self.nav.advance(&mut task, &mut budget) {
                Some(done) => {
                    let first = match done {
                        Done::Route(r) => r,
                        Done::Place(_) => Err(NoPath::Unreachable),
                    };
                    out.push(Answer::Journey {
                        id,
                        to: Some(to),
                        journey: Some(journey),
                        first: Some(first),
                    });
                    None
                }
                None => Some(Running::First {
                    id,
                    task,
                    to,
                    journey,
                }),
            },
        };
    }

    /// Work through the jobs: all of them, or until `deadline` passes.
    fn work(&mut self, deadline: Option<web_time::Instant>) -> Vec<Answer> {
        let mut out = Vec::new();
        loop {
            if let Some(block) = self.loading.pop_front() {
                self.land.load_block_cells(block);
            } else if let Some(job) = self.loaded_for.take() {
                self.start(job, &mut out);
                continue;
            } else if self.running.is_none() {
                let Some(job) = self.queue.pop_front() else {
                    break;
                };
                // A job that plans first has the cells it reads decoded, a landblock a slice.
                self.loading = Self::blocks_of(&job).into();
                self.loaded_for = Some(job);
                continue;
            } else {
                self.slice(if deadline.is_some() { SLICE } else { u64::MAX }, &mut out);
            }
            if deadline.is_some_and(|d| web_time::Instant::now() >= d) {
                break;
            }
        }
        out
    }
}

/// Where the planning runs.
enum Backend {
    /// On a thread of its own: jobs go to it and answers come back over channels.
    #[cfg(not(target_arch = "wasm32"))]
    Thread {
        jobs: std::sync::mpsc::Sender<Job>,
        answers: std::sync::mpsc::Receiver<Answer>,
    },
    /// In the frame's own thread, a slice of each frame.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Frame(Box<Worker>),
}

/// The travel's planning, off the frame.
pub struct Planning {
    backend: Backend,
    /// Jobs asked for and not yet answered, and answers not yet taken: whether it has work.
    asked: usize,
    /// Where it shares the frame's thread: whether each frame takes one step of its work (a
    /// landblock's cells read, or one slice of a search) rather than its share of the frame's time.
    one_step: bool,
}

impl std::fmt::Debug for Planning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Planning")
            .field("asked", &self.asked)
            .finish_non_exhaustive()
    }
}

impl Planning {
    /// The planning over `land`, its setups and outdoor placements read from `store`: on a
    /// thread of its own where the platform has threads.
    ///
    /// # Errors
    /// The region's data does not read.
    pub fn new(land: &Arc<DatLandSource>, store: &Arc<RetailDatStore>) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::threaded(land, store)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::in_frame(land, store)
        }
    }

    /// The planning on a thread of its own.
    #[cfg(not(target_arch = "wasm32"))]
    fn threaded(land: &Arc<DatLandSource>, store: &Arc<RetailDatStore>) -> Result<Self, String> {
        let (jobs, inbox) = std::sync::mpsc::channel::<Job>();
        let (outbox, answers) = std::sync::mpsc::channel::<Answer>();
        let (ready, built) = std::sync::mpsc::channel::<Result<(), String>>();
        let (land, store) = (Arc::clone(land), Arc::clone(store));
        std::thread::Builder::new()
            .name(String::from("travel planning"))
            .spawn(move || {
                let mut worker = match Worker::new(land, &store) {
                    Ok(w) => {
                        let _ = ready.send(Ok(()));
                        w
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                // Each job as it comes; what has queued meanwhile is taken in order before the
                // next, so the objects set are the latest when a plan starts.
                while let Ok(job) = inbox.recv() {
                    worker.queue.push_back(job);
                    worker.queue.extend(inbox.try_iter());
                    for a in worker.work(None) {
                        if outbox.send(a).is_err() {
                            return;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        built
            .recv()
            .map_err(|_| String::from("the planning thread stopped"))??;
        Ok(Self {
            backend: Backend::Thread { jobs, answers },
            asked: 0,
            one_step: false,
        })
    }

    /// The planning in the frame's own thread, a slice of each frame.
    ///
    /// # Errors
    /// The region's data does not read.
    pub fn in_frame(
        land: &Arc<DatLandSource>,
        store: &Arc<RetailDatStore>,
    ) -> Result<Self, String> {
        Ok(Self {
            backend: Backend::Frame(Box::new(Worker::new(Arc::clone(land), store)?)),
            asked: 0,
            one_step: false,
        })
    }

    /// The same planning taking one step of its work each frame where it shares the frame's
    /// thread, whatever the clock says: the frames an answer takes are then counted in its work.
    #[cfg(test)]
    pub(crate) fn one_step_a_frame(mut self) -> Self {
        self.one_step = true;
        self
    }

    /// Ask for `job`; its answer comes from a later [`Self::answers`].
    pub fn ask(&mut self, job: Job) {
        let answered = matches!(job, Job::Route { .. } | Job::Journey { .. } | Job::Unblock);
        match &mut self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            Backend::Thread { jobs, .. } => {
                let _ = jobs.send(job);
            }
            Backend::Frame(w) => w.queue.push_back(job),
        }
        if answered {
            self.asked += 1;
        }
    }

    /// Whether answers are still to come.
    #[must_use]
    pub const fn waiting(&self) -> bool {
        self.asked > 0
    }

    /// Every answer still to come, waited for: for a client stepped faster than the clock,
    /// whose frames would otherwise outrun their plans.
    pub fn answers_waited(&mut self) -> Vec<Answer> {
        let mut got = Vec::new();
        while got.len() < self.asked {
            match &mut self.backend {
                #[cfg(not(target_arch = "wasm32"))]
                Backend::Thread { answers, .. } => match answers.recv() {
                    Ok(a) => got.push(a),
                    Err(_) => break,
                },
                Backend::Frame(w) => {
                    let more = w.work(None);
                    if more.is_empty() {
                        break;
                    }
                    got.extend(more);
                }
            }
        }
        self.asked = self.asked.saturating_sub(got.len());
        got
    }

    /// The answers ready this frame. In the frame's own thread, this is where the frame's slice
    /// of planning is done.
    pub fn answers(&mut self) -> Vec<Answer> {
        let got: Vec<Answer> = match &mut self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            Backend::Thread { answers, .. } => answers.try_iter().collect(),
            Backend::Frame(w) => {
                let share = if self.one_step {
                    std::time::Duration::ZERO
                } else {
                    std::time::Duration::from_secs_f64(FRAME_SHARE_MS / 1000.0)
                };
                w.work(Some(web_time::Instant::now() + share))
            }
        };
        self.asked = self.asked.saturating_sub(got.len());
        got
    }
}
