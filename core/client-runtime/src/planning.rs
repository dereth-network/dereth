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
    Done, HandBack, Journey, JourneyConfig, NavPath, NoPath, Obstacle, PortalEdge, Realm, Refusal,
    Task, Teleport,
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
        /// The portals the character is refused, each with why: with no journey, the one over
        /// them too says what stands in the way.
        refused: Vec<(PortalEdge, Refusal)>,
        teleports: Vec<Teleport>,
        hand_backs: Vec<HandBack>,
        reach: Option<f32>,
        /// The highest drop off a ledge the character lands from unhurt, metres; `0.0` for
        /// none. A journey takes a drop only where nothing else reaches the goal.
        highest_drop: f32,
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
        /// With no journey: what the portals that would lead there ask of the character, when
        /// some would ([`refused_why`]).
        why: Option<String>,
    },
    /// A [`Job::Unblock`]: how many places were put back.
    Unblocked(usize),
}

/// What a planning failure answers a journey or a route with, and the trip that waited says.
pub const PLANNING_FAILED: &str = "planning_failed";

/// A job that is answered, as much of it as answering it failed needs: its id, and where from and
/// to, for the log.
// The browser's build plans in the frame, with no thread to keep promises for.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Debug, Clone)]
struct Pending {
    id: u64,
    journey: bool,
    from: Position,
    to: Goal,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl Pending {
    /// The job's, when it is one that is answered (a journey or a route).
    fn of(job: &Job) -> Option<Self> {
        match job {
            Job::Journey { id, from, to, .. } => Some(Self {
                id: *id,
                journey: true,
                from: *from,
                to: *to,
            }),
            Job::Route { id, from, to, .. } => Some(Self {
                id: *id,
                journey: false,
                from: *from,
                to: *to,
            }),
            _ => None,
        }
    }

    /// The job's answer when planning it failed: no journey, saying so; no route.
    fn failed(&self) -> Answer {
        if self.journey {
            let to = match &self.to {
                Goal::Place(p) => *p,
                Goal::Cell(_) => self.from,
            };
            Answer::Journey {
                id: self.id,
                to: Some(to),
                journey: None,
                first: None,
                why: Some(PLANNING_FAILED.to_owned()),
            }
        } else {
            Answer::Route {
                id: self.id,
                to: None,
                route: Err(NoPath::NoCell(self.from.cell)),
                ms: 0.0,
            }
        }
    }
}

/// Runs `work` for the jobs `pending` wait on. A panic in it is caught, logged at error with its
/// message and each job's from and to, and every one of them answered as failed, so no trip waits
/// on an answer that will never come. Whether it panicked: the worker it ran is not to be trusted
/// after. Where a panic does not unwind (the browser's build aborts), there is nothing to catch.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn guarded(pending: &[Pending], work: impl FnOnce() -> Vec<Answer>) -> (Vec<Answer>, bool) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(answers) => (answers, false),
        Err(payload) => {
            let why = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| String::from("(no message)"));
            for p in pending {
                tracing::error!(
                    target: "travel",
                    "planning panicked ({why}): the {} from {:#010X} {:?} to {:?} is answered as \
                     failed",
                    if p.journey { "journey" } else { "route" },
                    p.from.cell.0,
                    p.from.frame.origin,
                    p.to
                );
            }
            if pending.is_empty() {
                tracing::error!(target: "travel", "planning panicked ({why})");
            }
            (pending.iter().map(Pending::failed).collect(), true)
        }
    }
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
    refused: Vec<(PortalEdge, Refusal)>,
    teleports: Vec<Teleport>,
    hand_backs: Vec<HandBack>,
    reach: Option<f32>,
    highest_drop: f32,
}

/// The most interior cells looked at to tell whether a cell of a landblock that is not loaded is
/// a building's inside: a building's cells reach the land within a few of them.
const BUILDING_CELLS: usize = 64;

/// The realms of interior cells of landblocks that are not loaded, read from the cell file as a
/// journey asks for them and kept: a portal far from the body may stand in a building, which a
/// walk reaches, or in a dungeon, which only a portal does.
#[derive(Default)]
struct FarRealms {
    known: std::collections::HashMap<u32, Realm>,
    buildings: std::collections::HashMap<u16, bool>,
    cells: dereth_world_data::env_cells::EnvCellLoader,
}

impl FarRealms {
    /// The realm of interior cell `cell`: the outdoors when its cells open onto the land within
    /// [`BUILDING_CELLS`] of it, in a landblock with buildings; else its landblock's dungeon.
    fn of(&mut self, store: &RetailDatStore, cell: CellId) -> Realm {
        if let Some(r) = self.known.get(&cell.0) {
            return *r;
        }
        let block = u16::try_from(cell.0 >> 16).unwrap_or(0);
        let dungeon = Realm::Dungeon(block);
        let has_buildings = *self.buildings.entry(block).or_insert_with(|| {
            crate::world_build::read_lbi(store, i32::from(block >> 8), i32::from(block & 0xFF))
                .is_some_and(|l| !l.buildings.is_empty())
        });
        if !has_buildings {
            self.known.insert(cell.0, dungeon);
            return dungeon;
        }
        let base = cell.0 & 0xFFFF_0000;
        let mut seen = std::collections::BTreeSet::from([cell.0]);
        let mut queue = VecDeque::from([cell.0]);
        let mut found = dungeon;
        'flood: while let Some(id) = queue.pop_front() {
            let Some(d) = self.cells.load_cell(store, CellId(id)) else {
                continue;
            };
            for portal in &d.cell.portals {
                if portal.other_cell_id == u32::MAX {
                    found = Realm::Outdoors;
                    break 'flood;
                }
                let next = base | (portal.other_cell_id & 0xFFFF);
                if seen.len() < BUILDING_CELLS && seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        // Every cell looked at is in the same realm, when the flood went through them all.
        if found == Realm::Outdoors || seen.len() < BUILDING_CELLS {
            for id in seen {
                self.known.insert(id, found);
            }
        } else {
            self.known.insert(cell.0, found);
        }
        found
    }
}

/// Why a journey is refused, as the travel's reasons go: `refused:` and, for each portal on the
/// way that would not take the character, what it asks (`refused: needs level 20 for Crater
/// Lair`); `None` for none.
#[must_use]
pub fn refused_why(why: &[(String, Refusal)]) -> Option<String> {
    let each: Vec<String> = why
        .iter()
        .map(|(name, r)| match r {
            Refusal::BelowLevel(min) => format!("needs level {min} for {name}"),
            Refusal::AboveLevel(max) => format!("needs level {max} or below for {name}"),
            Refusal::Requires(q) => format!("needs {q} for {name}"),
            Refusal::Retired(w) => format!("{name} refused before ({w})"),
        })
        .collect();
    (!each.is_empty()).then(|| format!("refused: {}", each.join("; ")))
}

/// The planner and its jobs, wherever they run.
struct Worker {
    land: Arc<DatLandSource>,
    store: Arc<RetailDatStore>,
    far: std::cell::RefCell<FarRealms>,
    nav: Planner,
    queue: VecDeque<Job>,
    /// The landblocks whose cells the next job reads, still to decode, one a slice; and the job.
    loading: VecDeque<dereth_primitives::LandblockId>,
    loaded_for: Option<Job>,
    running: Option<Running>,
    /// The objects in the world last set.
    obstacles: Vec<ObjectId>,
    /// The land's regions, once made; and the making of them, while it goes on. A journey waits
    /// for them.
    lands: Option<dereth_navigation::lands::LandRegions>,
    making: Option<dereth_navigation::lands::RegionBuilder>,
}

/// How many landblocks of the land's regions a slice of a frame reads.
const LAND_SLICE: u32 = 1024;

impl Worker {
    fn new(
        land: Arc<DatLandSource>,
        store: &Arc<RetailDatStore>,
        tiles: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        let nav = planner(&land, store, tiles)?;
        Ok(Self {
            land,
            store: Arc::clone(store),
            far: std::cell::RefCell::new(FarRealms::default()),
            nav,
            queue: VecDeque::new(),
            loading: VecDeque::new(),
            loaded_for: None,
            running: None,
            obstacles: Vec::new(),
            lands: None,
            making: None,
        })
    }

    /// Make the land's regions, or a slice of them when `slice`: whether they are made.
    fn make_lands(&mut self, slice: bool) -> bool {
        if self.lands.is_some() {
            return true;
        }
        let builder = self.making.get_or_insert_with(Default::default);
        let land = &self.land;
        let count = if slice { LAND_SLICE } else { u32::MAX };
        if builder.step(&|id| land.build_uncached(id), count) {
            if let Some(b) = self.making.take() {
                let clock = web_time::Instant::now();
                let lands = b.finish();
                tracing::info!(
                    target: "travel",
                    "the land's regions: {} landblocks, joined in {:.0} ms",
                    lands.blocks(),
                    clock.elapsed().as_secs_f64() * 1000.0
                );
                self.lands = Some(lands);
            }
            return true;
        }
        false
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
                refused,
                teleports,
                hand_backs,
                reach,
                highest_drop,
            } => {
                let Some(to) = self.place_of(to) else {
                    out.push(Answer::Journey {
                        id,
                        to: None,
                        journey: None,
                        first: None,
                        why: None,
                    });
                    return;
                };
                self.running = Some(Running::Goal {
                    id,
                    task: Box::new(Task::place(to, 12.0)),
                    ask: Ask {
                        from,
                        portals,
                        refused,
                        teleports,
                        hand_backs,
                        reach,
                        highest_drop,
                    },
                });
            }
        }
    }

    /// With no journey from `ask.from` to `to`: what the portals the character is refused, on
    /// the journey planned with them too, ask of it ([`refused_why`]); `None` when that journey
    /// needs none of them, or there is none even so.
    fn refusals(&self, ask: &Ask, to: &Position) -> Option<String> {
        if ask.refused.is_empty() {
            return None;
        }
        let mut all = Ask {
            from: ask.from,
            portals: ask.portals.clone(),
            refused: Vec::new(),
            teleports: ask.teleports.clone(),
            hand_backs: ask.hand_backs.clone(),
            reach: ask.reach,
            highest_drop: ask.highest_drop,
        };
        all.portals
            .extend(ask.refused.iter().map(|(e, _)| e.clone()));
        let (j, _) = self.journey(&all, to)?;
        let mut why: Vec<(String, Refusal)> = Vec::new();
        for leg in &j.legs {
            if let dereth_navigation::Leg::Portal { edge } = leg {
                if let Some((e, r)) = ask
                    .refused
                    .iter()
                    .find(|(e, _)| e.name == edge.name && e.from == edge.from)
                {
                    let named = (e.name.clone(), r.clone());
                    if !why.contains(&named) {
                        why.push(named);
                    }
                }
            }
        }
        refused_why(&why)
    }

    /// Where `p` is, for a journey: the region of the land it is in (the outdoors and the
    /// buildings on it), or its dungeon or the part of one. An interior cell of a landblock that is
    /// not loaded is read from the cell file rather than loaded, which would cost more than the
    /// plan: a building's inside (where many portals stand) is told from a dungeon (where nearly
    /// every portal leads).
    fn realm_of(&self, p: &Position) -> Option<Realm> {
        // Where the dungeon's places are built, the stretch of floor a walk joins: a part's
        // floors under holes in its upper ones are apart.
        if let Some(set) = self.nav.walk_set(p) {
            return Some(set);
        }
        let r = self.nav.realm(p).or_else(|| {
            (p.cell.0 & 0xFFFF >= 0x0100).then(|| self.far.borrow_mut().of(&self.store, p.cell))
        });
        match (r, self.lands.as_ref()) {
            (Some(Realm::Outdoors), Some(l)) => {
                Some(l.region_at(p).map_or(Realm::Outdoors, Realm::Land))
            }
            (r, _) => r,
        }
    }

    /// Why there is no journey from `ask`'s start to `to` when the two are on the land in regions
    /// no walk joins: a crater, a plateau, an island, and no portal known that leads in.
    fn walled_off(&self, ask: &Ask, to: &Position) -> Option<String> {
        match (self.realm_of(&ask.from), self.realm_of(to)) {
            (Some(Realm::Land(a)), Some(Realm::Land(b))) if a != b => Some(String::from(
                "no_way: the land there is walled off from here, and no portal known leads in",
            )),
            _ => None,
        }
    }

    /// The journey from `from` to `to`, and how long it took to plan.
    fn journey(&self, ask: &Ask, to: &Position) -> Option<(Journey, f64)> {
        let clock = web_time::Instant::now();
        let realm = |p: &Position| self.realm_of(p);
        let portals: Vec<&PortalEdge> = ask.portals.iter().collect();
        // A drop off a ledge only where nothing else reaches the goal: one way, and only where
        // the body falls unhurt.
        let drops = || {
            if ask.highest_drop <= 0.0 {
                return Vec::new();
            }
            [&ask.from, to]
                .into_iter()
                .flat_map(|p| self.nav.drops(p, ask.highest_drop))
                .collect()
        };
        let j = dereth_navigation::plan_journey_or_drop(
            &ask.from,
            to,
            &portals,
            &ask.teleports,
            &ask.hand_backs,
            drops,
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
                                        why: None,
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
                                why: self
                                    .refusals(&ask, &to)
                                    .or_else(|| self.walled_off(&ask, &to)),
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
                        why: None,
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
                // A journey plans between the land's regions: they are made first.
                if matches!(self.queue.front(), Some(Job::Journey { .. }))
                    && !self.make_lands(deadline.is_some())
                {
                    if deadline.is_some_and(|d| web_time::Instant::now() >= d) {
                        break;
                    }
                    continue;
                }
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
    /// Nothing is planned and nothing answers: for a test that answers the travel by hand. The
    /// last job asked is kept for the test to read.
    #[cfg(test)]
    Held(Option<Job>),
}

/// The travel's planning, off the frame.
pub struct Planning {
    backend: Backend,
    /// Jobs asked for and not yet answered, and answers not yet taken: whether it has work.
    asked: usize,
    /// Where it shares the frame's thread: whether each frame takes one step of its work (a
    /// landblock's cells read, or one slice of a search) rather than its share of the frame's time.
    one_step: bool,
    /// Jobs answered as failed here, the planning thread having stopped: given with the next
    /// answers.
    failed: Vec<Answer>,
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
    /// `tiles` is where the planner keeps what it builds between sessions; `None` keeps nothing.
    pub fn new(
        land: &Arc<DatLandSource>,
        store: &Arc<RetailDatStore>,
        tiles: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::threaded(land, store, tiles)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::in_frame(land, store, tiles)
        }
    }

    /// The planning on a thread of its own.
    #[cfg(not(target_arch = "wasm32"))]
    fn threaded(
        land: &Arc<DatLandSource>,
        store: &Arc<RetailDatStore>,
        tiles: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        let (jobs, inbox) = std::sync::mpsc::channel::<Job>();
        let (outbox, answers) = std::sync::mpsc::channel::<Answer>();
        let (ready, built) = std::sync::mpsc::channel::<Result<(), String>>();
        let (land, store) = (Arc::clone(land), Arc::clone(store));
        std::thread::Builder::new()
            .name(String::from("travel planning"))
            .spawn(move || {
                let mut worker = match Worker::new(Arc::clone(&land), &store, tiles.as_deref()) {
                    Ok(w) => {
                        let _ = ready.send(Ok(()));
                        w
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                // The objects last set: a worker made anew after a panic is given them again.
                let mut obstacles: Option<Vec<Obstacle>> = None;
                // Each job as it comes; what has queued meanwhile is taken in order before the
                // next, so the objects set are the latest when a plan starts.
                while let Ok(job) = inbox.recv() {
                    let batch: Vec<Job> = std::iter::once(job).chain(inbox.try_iter()).collect();
                    let pending: Vec<Pending> = batch.iter().filter_map(Pending::of).collect();
                    for j in &batch {
                        if let Job::Obstacles(o) = j {
                            obstacles = Some(o.clone());
                        }
                    }
                    worker.queue.extend(batch);
                    let (answers, panicked) = guarded(&pending, || worker.work(None));
                    if panicked {
                        // What the worker held may be half done: it is made anew.
                        match Worker::new(Arc::clone(&land), &store, tiles.as_deref()) {
                            Ok(w) => worker = w,
                            Err(e) => {
                                tracing::error!(target: "travel", "no planner after a panic: {e}");
                                for a in answers {
                                    let _ = outbox.send(a);
                                }
                                return;
                            }
                        }
                        if let Some(o) = &obstacles {
                            worker.queue.push_back(Job::Obstacles(o.clone()));
                        }
                    }
                    for a in answers {
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
            failed: Vec::new(),
        })
    }

    /// The planning in the frame's own thread, a slice of each frame.
    ///
    /// # Errors
    /// The region's data does not read.
    pub fn in_frame(
        land: &Arc<DatLandSource>,
        store: &Arc<RetailDatStore>,
        tiles: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        Ok(Self {
            backend: Backend::Frame(Box::new(Worker::new(
                Arc::clone(land),
                store,
                tiles.as_deref(),
            )?)),
            asked: 0,
            one_step: false,
            failed: Vec::new(),
        })
    }

    /// An answer for the next answers to give, as though the planning had made it: for a test
    /// that answers the travel by hand.
    #[cfg(test)]
    pub(crate) fn give(&mut self, answer: Answer) {
        self.failed.push(answer);
        self.asked += 1;
    }

    /// A planning that plans nothing: a test answers the travel by hand.
    #[cfg(test)]
    pub(crate) fn held() -> Self {
        Self {
            backend: Backend::Held(None),
            asked: 0,
            one_step: false,
            failed: Vec::new(),
        }
    }

    /// The last job a held planning was asked.
    #[cfg(test)]
    pub(crate) fn last_asked(&self) -> Option<&Job> {
        match &self.backend {
            Backend::Held(last) => last.as_ref(),
            _ => None,
        }
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
                // A planning thread that is gone answers nothing: the job is answered as failed
                // here, so no trip waits on it.
                if let Err(std::sync::mpsc::SendError(job)) = jobs.send(job) {
                    if let Some(p) = Pending::of(&job) {
                        tracing::error!(
                            target: "travel",
                            "the planning thread has stopped: job {} answered as failed",
                            p.id
                        );
                        self.failed.push(p.failed());
                    }
                }
            }
            Backend::Frame(w) => w.queue.push_back(job),
            #[cfg(test)]
            Backend::Held(last) => *last = Some(job),
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
                #[cfg(test)]
                Backend::Held(_) => break,
            }
        }
        self.asked = self.asked.saturating_sub(got.len());
        got
    }

    /// The answers ready this frame. In the frame's own thread, this is where the frame's slice
    /// of planning is done.
    pub fn answers(&mut self) -> Vec<Answer> {
        let mut got: Vec<Answer> = std::mem::take(&mut self.failed);
        got.extend(match &mut self.backend {
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
            #[cfg(test)]
            Backend::Held(_) => Vec::new(),
        });
        self.asked = self.asked.saturating_sub(got.len());
        got
    }
}

#[cfg(test)]
mod guard_tests {
    //! Behaviour: none (the planning's own keeping of its promises).
    use super::*;
    use dereth_primitives::{Frame, Quat, Vec3};

    fn at(cell: u32, x: f32) -> Position {
        Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY),
        )
    }

    #[test]
    fn a_planning_job_that_panics_is_answered_as_failed_and_the_rest_are_answered() {
        let jobs = [
            Job::Journey {
                id: 7,
                from: at(0xA9B4_0019, 84.0),
                to: Goal::Place(at(0x019E_0114, 10.0)),
                portals: Vec::new(),
                refused: Vec::new(),
                teleports: Vec::new(),
                hand_backs: Vec::new(),
                reach: None,
                highest_drop: 0.0,
            },
            Job::Route {
                id: 8,
                from: at(0x019E_0114, 10.0),
                to: Goal::Place(at(0x019E_0114, 20.0)),
                reach: None,
                snap: None,
            },
            Job::Unblock,
        ];
        let pending: Vec<Pending> = jobs.iter().filter_map(Pending::of).collect();
        assert_eq!(
            pending.len(),
            2,
            "the journey and the route are answered; unblocking is not"
        );
        // Work that panics part-way: nothing it made is kept, and each job is answered failed.
        let (answers, panicked) = guarded(&pending, || panic!("an index out of range"));
        assert!(panicked);
        match answers.as_slice() {
            [Answer::Journey {
                id: 7,
                to: Some(to),
                journey: None,
                why: Some(why),
                ..
            }, Answer::Route {
                id: 8,
                route: Err(_),
                ..
            }] => {
                assert_eq!(why, PLANNING_FAILED);
                assert_eq!(to.cell, CellId(0x019E_0114), "to where it was asked");
            }
            other => panic!("{other:?}"),
        }
        // Work that does not panic answers as it would.
        let (answers, panicked) = guarded(&pending, || vec![Answer::Unblocked(3)]);
        assert!(!panicked);
        assert_eq!(answers, [Answer::Unblocked(3)]);
    }
}

#[cfg(test)]
mod far_realm_tests {
    use super::*;

    /// Behaviour: plugins.nav.journeys-reach-a-portal-in-a-building-far-away
    /// A portal far from the body that stands in a building is walked to; the dungeon it leads to
    /// is reached only through it.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_far_cell_in_a_building_is_outdoors_and_one_in_a_dungeon_is_its_dungeon() {
        let store = dereth_dat::testing::open_store_or_fail();
        let mut far = FarRealms::default();
        // The rooms Colier Mine's and the Dry Well's portals stand in.
        assert_eq!(far.of(&store, CellId(0xAFC6_015C)), Realm::Outdoors);
        assert_eq!(far.of(&store, CellId(0x9979_0102)), Realm::Outdoors);
        // Where they lead.
        assert_eq!(far.of(&store, CellId(0x01AE_032C)), Realm::Dungeon(0x01AE));
        assert_eq!(far.of(&store, CellId(0x015A_01AE)), Realm::Dungeon(0x015A));
        // Asked again: the same, from what was kept.
        assert_eq!(far.of(&store, CellId(0xAFC6_015C)), Realm::Outdoors);
    }
}

#[cfg(test)]
mod refusal_tests {
    use super::*;
    use dereth_primitives::{Frame, Quat, Vec3};

    /// Behaviour: plugins.nav.a-refused-journey-says-why
    #[test]
    fn a_refusal_reads_as_what_the_portal_asks() {
        assert_eq!(
            refused_why(&[("Crater Lair".into(), Refusal::BelowLevel(15))]).as_deref(),
            Some("refused: needs level 15 for Crater Lair")
        );
        assert_eq!(
            refused_why(&[
                ("Low".into(), Refusal::AboveLevel(20)),
                (
                    "Vault".into(),
                    Refusal::Requires(dereth_navigation::Requirement::Quest("Key".into()))
                ),
            ])
            .as_deref(),
            Some("refused: needs level 20 or below for Low; needs quest Key for Vault")
        );
        assert_eq!(refused_why(&[]), None);
    }

    /// Behaviour: plugins.nav.a-refused-journey-says-why
    /// No journey for a level 1 character to Crater Lair, whose portal asks level 15: the answer
    /// says so.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_journey_a_portal_refuses_answers_with_what_it_asks() {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land"));
        let at = |cell: u32, x: f32, y: f32, z: f32| {
            Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
        };
        let lair = PortalEdge {
            source: dereth_navigation::EdgeSource::Hint,
            wcid: Some(3651),
            name: "Crater Lair".into(),
            from: at(0x8FD7_0015, 56.5, 115.5, 115.1),
            to: Some(at(0x018C_0191, 40.0, -20.0, 0.005)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (Some(15), None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        };
        let mut planning = Planning::in_frame(&land, &store, None).expect("a planner");
        planning.ask(Job::Journey {
            id: 1,
            from: at(0xA9B4_0019, 84.0, 7.1, 94.0),
            to: Goal::Place(at(0x018C_0191, 40.0, -20.0, 0.005)),
            portals: Vec::new(),
            refused: vec![(lair, Refusal::BelowLevel(15))],
            teleports: Vec::new(),
            hand_backs: Vec::new(),
            reach: None,
            highest_drop: 0.0,
        });
        let answers = planning.answers_waited();
        match answers.as_slice() {
            [Answer::Journey {
                journey: None, why, ..
            }] => assert_eq!(
                why.as_deref(),
                Some("refused: needs level 15 for Crater Lair")
            ),
            other => panic!("{other:?}"),
        }
    }
}

#[cfg(test)]
mod land_region_tests {
    use super::*;
    use dereth_primitives::{Frame, Quat, Vec3};

    fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
        Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
    }

    fn edge(name: &str, wcid: u32, from: Position, to: Position) -> PortalEdge {
        PortalEdge {
            source: dereth_navigation::EdgeSource::Hint,
            wcid: Some(wcid),
            name: name.into(),
            from,
            to: Some(to),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
            requires: Vec::new(),
            shape: None,
        }
    }

    /// The end of retail's ways in and out of Mt Esper's crater: the Crater Pathway from the land
    /// outside and from the crater, both into one dungeon, and its two ways up.
    fn crater_pathway() -> Vec<PortalEdge> {
        vec![
            edge(
                "Crater Pathway",
                1,
                at(0x93CB_0102, 12.0, 41.443, 101.205),
                at(0x01A7_0108, 17.571, -509.205, 0.0),
            ),
            edge(
                "Crater Pathway",
                2,
                at(0x90D0_0104, 113.0, 84.0, 277.205),
                at(0x01A7_01AA, 32.4, -8.01, 138.0),
            ),
            edge(
                "Surface",
                3,
                at(0x01A7_0103, -0.125, -522.632, 0.0),
                at(0x93CB_0105, 12.0, 24.0, 101.3),
            ),
            edge(
                "Surface",
                4,
                at(0x01A7_01A7, 17.061, -0.147, 138.0),
                at(0x90D0_0107, 95.521, 84.0, 277.3),
            ),
        ]
    }

    /// Behaviour: plugins.nav.a-journey-reaches-land-walled-off-from-here-through-the-portal-that-leads-in
    /// The land inside Mt Esper's crater is walled off from the land round it, which no walk
    /// leaves: a journey there from Holtburg goes through the Crater Pathway; with no portal known
    /// that leads in, there is none, and it says the land there is walled off.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_journey_into_mt_espers_crater_goes_through_the_crater_pathway() {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("the land"));
        let holtburg = at(0xA9B4_0019, 84.0, 7.1, 94.0);
        let crater = at(0x90D0_000C, 36.791, 89.165, 282.939);
        let ask = |portals: Vec<PortalEdge>| {
            let mut planning = Planning::in_frame(&land, &store, None).expect("a planner");
            planning.ask(Job::Journey {
                id: 1,
                from: holtburg,
                to: Goal::Place(crater),
                portals,
                refused: Vec::new(),
                teleports: Vec::new(),
                hand_backs: Vec::new(),
                reach: None,
                highest_drop: 0.0,
            });
            planning.answers_waited()
        };
        match ask(crater_pathway()).as_slice() {
            [Answer::Journey {
                journey: Some((j, _)),
                ..
            }] => {
                let used: Vec<&str> = j
                    .legs
                    .iter()
                    .filter_map(|l| match l {
                        dereth_navigation::Leg::Portal { edge } => Some(edge.name.as_str()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(used, ["Crater Pathway", "Surface"], "{j:?}");
            }
            other => panic!("{other:?}"),
        }
        match ask(Vec::new()).as_slice() {
            [Answer::Journey {
                journey: None, why, ..
            }] => {
                assert_eq!(
                why.as_deref(),
                Some("no_way: the land there is walled off from here, and no portal known leads in")
            )
            }
            other => panic!("{other:?}"),
        }
    }
}
